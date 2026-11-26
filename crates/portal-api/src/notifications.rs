//! Notification delivery: the in-app record, plus the tournament's Discord
//! channel when an organiser has connected one.
//!
//! [`Notifier::emit`] is called after whatever triggered the event has been
//! written. It never fails and never waits on Discord: the in-app rows are
//! written inline (a single insert), the Discord post runs in its own task.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use portal_db::PgNotificationRepository;
use portal_domain::services::{DiscordPost, NotificationEvent, NotificationService};
use tracing::{info, warn};

/// How long one Discord request may take.
const DISCORD_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest `retry_after` from a 429 that is worth waiting for.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(5);
/// How close to the deadline a side that has not checked in is reminded.
const REMINDER_WINDOW_MINUTES: i64 = 10;

/// The app's notification service over Postgres.
pub type AppNotificationService = NotificationService<PgNotificationRepository>;

/// Writes notifications and posts the Discord-worthy ones.
#[derive(Clone)]
pub struct Notifier {
    pub service: AppNotificationService,
    http: reqwest::Client,
    /// `PORTAL_PUBLIC_URL`, for links in Discord posts.
    public_url: Option<Arc<str>>,
    /// Replaces `https://discord.com` in webhook URLs, so tests can point
    /// delivery at a local stub. Never set in production. Shared between
    /// clones, so setting it on a test's copy of the state reaches the
    /// router's.
    webhook_base: Arc<std::sync::RwLock<Option<String>>>,
}

impl Notifier {
    /// Build from the environment. Nothing is required: without
    /// `PORTAL_PUBLIC_URL` Discord posts carry no link.
    #[must_use]
    pub fn from_env(repo: Arc<PgNotificationRepository>) -> Self {
        let non_empty = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().trim_end_matches('/').to_string())
                .filter(|v| !v.is_empty())
                .map(Arc::from)
        };
        Self {
            service: NotificationService::new(repo),
            http: reqwest::Client::builder()
                .timeout(DISCORD_TIMEOUT)
                .build()
                .unwrap_or_default(),
            public_url: non_empty("PORTAL_PUBLIC_URL"),
            webhook_base: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Send Discord posts to `base` instead of `https://discord.com`.
    /// For tests only.
    pub fn redirect_discord_for_tests(&self, base: &str) {
        if let Ok(mut guard) = self.webhook_base.write() {
            *guard = Some(base.trim_end_matches('/').to_string());
        }
    }

    /// Record the event; post it to Discord in the background when it is a
    /// Discord-worthy event that notified someone new.
    pub async fn emit(&self, event: NotificationEvent) {
        if let Some(post) = self.service.notify(event).await {
            self.spawn_discord(post);
        }
    }

    /// Remind sides that have not checked in and whose window closes soon.
    pub async fn remind_check_ins(&self) {
        let posts = self
            .service
            .remind_check_ins(
                Utc::now(),
                chrono::Duration::minutes(REMINDER_WINDOW_MINUTES),
            )
            .await;
        for post in posts {
            self.spawn_discord(post);
        }
    }

    fn spawn_discord(&self, post: DiscordPost) {
        let this = self.clone();
        tokio::spawn(async move {
            let settings = match this.service.settings(post.tournament_id).await {
                Ok(Some(s)) => s,
                Ok(None) => return,
                Err(e) => {
                    warn!(tournament_id = %post.tournament_id, error = %e,
                        "could not read notification settings");
                    return;
                }
            };
            let Some(url) = settings
                .discord_webhook_url
                .filter(|_| settings.discord_enabled)
            else {
                return;
            };
            let content = this.discord_text(&post);
            if let Err(detail) = this.post_to_discord(&url, &content).await {
                warn!(tournament_id = %post.tournament_id, %detail, "Discord notification not delivered");
            }
        });
    }

    /// One line: `**Tournament** — Title: body <link>`.
    fn discord_text(&self, post: &DiscordPost) -> String {
        let link = self
            .public_url
            .as_ref()
            .map(|base| format!(" <{base}{}>", post.link))
            .unwrap_or_default();
        format!(
            "**{}** — {}: {}{link}",
            post.tournament_name, post.title, post.body
        )
    }

    /// Post a message to a webhook, retrying once on a timeout, a 5xx, or a
    /// short 429. The error says what went wrong without the URL, which is a
    /// secret.
    ///
    /// # Errors
    ///
    /// A description of the failure (HTTP status or transport error).
    pub async fn post_to_discord(&self, url: &str, content: &str) -> Result<(), String> {
        let url = self.rewrite(url);
        let body = serde_json::json!({
            "content": content,
            "allowed_mentions": { "parse": [] },
        });
        let mut retried = false;
        loop {
            let outcome = self.http.post(&url).json(&body).send().await;
            let (retry_in, detail) = match outcome {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                Ok(resp) if resp.status().as_u16() == 429 => {
                    let wait = resp
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|v| v.get("retry_after").and_then(serde_json::Value::as_f64))
                        .map(Duration::from_secs_f64);
                    (
                        wait.filter(|w| *w <= MAX_RETRY_AFTER),
                        "Discord rate-limited the webhook (HTTP 429)".to_string(),
                    )
                }
                Ok(resp) if resp.status().is_server_error() => (
                    Some(Duration::from_millis(500)),
                    format!("Discord answered HTTP {}", resp.status().as_u16()),
                ),
                Ok(resp) => {
                    return Err(format!("Discord answered HTTP {}", resp.status().as_u16()));
                }
                Err(e) if e.is_timeout() => (
                    Some(Duration::ZERO),
                    "Discord did not answer within 5 seconds".to_string(),
                ),
                Err(e) => {
                    return Err(if e.is_connect() {
                        "could not connect to Discord".to_string()
                    } else {
                        "the request to Discord failed".to_string()
                    });
                }
            };
            match retry_in {
                Some(wait) if !retried => {
                    retried = true;
                    tokio::time::sleep(wait).await;
                }
                _ => return Err(detail),
            }
        }
    }

    fn rewrite(&self, url: &str) -> String {
        let Some(base) = self.webhook_base.read().ok().and_then(|g| g.clone()) else {
            return url.to_string();
        };
        for prefix in ["https://discord.com", "https://discordapp.com"] {
            if let Some(rest) = url.strip_prefix(prefix) {
                return format!("{base}{rest}");
            }
        }
        url.to_string()
    }

    /// Delete old notifications.
    pub async fn sweep(&self) {
        let n = self.service.sweep(Utc::now()).await;
        if n > 0 {
            info!(deleted = n, "notification retention sweep");
        }
    }
}
