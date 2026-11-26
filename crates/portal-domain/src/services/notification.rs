//! Turns match events into notifications for the people they concern.
//!
//! [`NotificationService::notify`] never fails: a notification that cannot be
//! written is logged and dropped, so whatever triggered it (a proposal, a
//! check-in window, a server coming up) is never held up or undone by it.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    DisputeId, NotificationId, ResultClaimId, ScheduleProposalId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, UserId, VetoSessionId,
};
use tracing::warn;

use crate::entities::notification::{
    Audience, MatchNotificationContext, NewNotification, Notification, NotificationKind,
    TournamentNotificationSettings,
};
use crate::repositories::NotificationRepository;

/// Something that happened to a match that people should hear about.
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    /// A side proposed (or counter-proposed) times; the other side answers.
    ProposalReceived {
        match_id: TournamentMatchId,
        proposal_id: ScheduleProposalId,
        proposed_by: TournamentRegistrationId,
        counter: bool,
    },
    /// The teams agreed a time.
    ProposalAccepted {
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
    },
    /// The other side turned the proposal down.
    ProposalRejected {
        match_id: TournamentMatchId,
        proposal_id: ScheduleProposalId,
        proposed_by: TournamentRegistrationId,
    },
    /// Nobody answered the proposal in time.
    ProposalExpired {
        match_id: TournamentMatchId,
        proposal_id: ScheduleProposalId,
        proposed_by: TournamentRegistrationId,
    },
    /// An organiser set or changed the match time.
    MatchTimeSet {
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
    },
    /// The check-in window is open (or its deadline moved).
    CheckinOpen {
        match_id: TournamentMatchId,
        deadline: DateTime<Utc>,
    },
    /// The window closes soon and this side has not checked in.
    CheckinReminder {
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        deadline: DateTime<Utc>,
    },
    /// The map veto has started.
    VetoStarted {
        match_id: TournamentMatchId,
        session_id: VetoSessionId,
    },
    /// The game server has the match loaded and players can connect.
    ServerReady {
        match_id: TournamentMatchId,
        reservation_id: uuid::Uuid,
    },
    /// No server was free; the match waits for one.
    ServerQueued {
        match_id: TournamentMatchId,
        reservation_id: uuid::Uuid,
    },
    /// A waiting match was given a server.
    ServerAssignedFromQueue {
        match_id: TournamentMatchId,
        reservation_id: uuid::Uuid,
    },
    /// A result was reported and waits for confirmation: by the other side
    /// of `reported_by`, or by both sides when the game server reported it.
    ResultToConfirm {
        match_id: TournamentMatchId,
        claim_id: ResultClaimId,
        reported_by: Option<TournamentRegistrationId>,
    },
    /// The match is decided (`completed` or `forfeit`). Other statuses are
    /// ignored, so callers can emit this after any settlement.
    ResultFinal { match_id: TournamentMatchId },
    /// An organiser ruled on a dispute.
    DisputeRuled {
        match_id: TournamentMatchId,
        dispute_id: DisputeId,
    },
    /// An organiser reset the match to be played again.
    MatchReset { match_id: TournamentMatchId },
}

impl NotificationEvent {
    /// The match the event is about.
    #[must_use]
    pub const fn match_id(&self) -> TournamentMatchId {
        match self {
            Self::ProposalReceived { match_id, .. }
            | Self::ProposalAccepted { match_id, .. }
            | Self::ProposalRejected { match_id, .. }
            | Self::ProposalExpired { match_id, .. }
            | Self::MatchTimeSet { match_id, .. }
            | Self::CheckinOpen { match_id, .. }
            | Self::CheckinReminder { match_id, .. }
            | Self::VetoStarted { match_id, .. }
            | Self::ServerReady { match_id, .. }
            | Self::ServerQueued { match_id, .. }
            | Self::ServerAssignedFromQueue { match_id, .. }
            | Self::ResultToConfirm { match_id, .. }
            | Self::ResultFinal { match_id }
            | Self::DisputeRuled { match_id, .. }
            | Self::MatchReset { match_id } => *match_id,
        }
    }
}

/// What to post to the tournament's Discord channel, if it has one.
#[derive(Debug, Clone)]
pub struct DiscordPost {
    pub tournament_id: TournamentId,
    pub tournament_name: String,
    pub title: String,
    pub body: String,
    /// SPA path; the API prefixes its public URL.
    pub link: String,
}

/// Everything a notification says, before recipients are resolved.
struct Draft {
    kind: NotificationKind,
    title: String,
    body: String,
    dedupe_key: String,
    recipients: Vec<TournamentRegistrationId>,
    audience: Audience,
}

/// A page of a user's notifications.
#[derive(Debug, Clone)]
pub struct NotificationPage {
    pub items: Vec<Notification>,
    pub unread_count: i64,
    pub has_more: bool,
}

/// Read notifications are kept this long.
const KEEP_READ_DAYS: i64 = 30;
/// Unread ones this long.
const KEEP_ANY_DAYS: i64 = 90;

/// Writes and reads notifications.
pub struct NotificationService<R: NotificationRepository> {
    repo: Arc<R>,
}

impl<R: NotificationRepository> Clone for NotificationService<R> {
    fn clone(&self) -> Self {
        Self {
            repo: Arc::clone(&self.repo),
        }
    }
}

/// "Sat 28 Nov 21:30 UTC".
fn when(t: DateTime<Utc>) -> String {
    t.format("%a %-d %b %H:%M UTC").to_string()
}

/// "21:30 UTC".
fn clock(t: DateTime<Utc>) -> String {
    t.format("%H:%M UTC").to_string()
}

impl<R: NotificationRepository> NotificationService<R> {
    /// Create the service.
    #[must_use]
    pub const fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// The underlying repository.
    #[must_use]
    pub fn repo(&self) -> &Arc<R> {
        &self.repo
    }

    /// Record the event for everyone it concerns.
    ///
    /// Returns what to post to Discord when the event is one that goes there
    /// and at least one person was newly notified (a repeat of an event that
    /// was already announced posts nothing). Never fails: errors are logged.
    pub async fn notify(&self, event: NotificationEvent) -> Option<DiscordPost> {
        match self.try_notify(&event).await {
            Ok(post) => post,
            Err(e) => {
                warn!(match_id = %event.match_id(), error = %e, "notification not recorded");
                None
            }
        }
    }

    async fn try_notify(
        &self,
        event: &NotificationEvent,
    ) -> Result<Option<DiscordPost>, DomainError> {
        let Some(ctx) = self.repo.match_context(event.match_id()).await? else {
            return Ok(None);
        };
        let Some(draft) = draft(event, &ctx) else {
            return Ok(None);
        };
        let users = self
            .repo
            .recipients(&draft.recipients, draft.audience)
            .await?;
        if users.is_empty() {
            return Ok(None);
        }
        let new = NewNotification {
            kind: draft.kind,
            title: draft.title.clone(),
            body: draft.body.clone(),
            link: Some(ctx.link()),
            tournament_id: Some(ctx.tournament_id),
            match_id: Some(ctx.match_id),
            dedupe_key: Some(draft.dedupe_key),
        };
        let written = self.repo.insert_for_users(&users, &new).await?;
        if written.is_empty() || !draft.kind.goes_to_discord() {
            return Ok(None);
        }
        Ok(Some(DiscordPost {
            tournament_id: ctx.tournament_id,
            tournament_name: ctx.tournament_name.clone(),
            title: draft.title,
            body: draft.body,
            link: ctx.link(),
        }))
    }

    /// Remind every side whose check-in window closes within `within` and
    /// that has not checked in. Each side is reminded once per match.
    pub async fn remind_check_ins(&self, now: DateTime<Utc>, within: Duration) -> Vec<DiscordPost> {
        let candidates = match self
            .repo
            .check_in_reminder_candidates(now, now + within)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "could not list check-in reminders");
                return Vec::new();
            }
        };
        let mut posts = Vec::new();
        for c in candidates {
            if let Some(post) = self
                .notify(NotificationEvent::CheckinReminder {
                    match_id: c.match_id,
                    registration_id: c.registration_id,
                    deadline: c.deadline,
                })
                .await
            {
                posts.push(post);
            }
        }
        posts
    }

    /// A page of the user's notifications, newest first.
    pub async fn list(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        before: Option<NotificationId>,
    ) -> Result<NotificationPage, DomainError> {
        let limit = limit.clamp(1, 100);
        let mut items = self
            .repo
            .list_for_user(user_id, unread_only, limit + 1, before)
            .await?;
        let has_more = items.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let (unread_count, _) = self.repo.unread_count(user_id).await?;
        Ok(NotificationPage {
            items,
            unread_count,
            has_more,
        })
    }

    /// Unread count and the newest unread id.
    pub async fn unread_count(
        &self,
        user_id: UserId,
    ) -> Result<(i64, Option<NotificationId>), DomainError> {
        self.repo.unread_count(user_id).await
    }

    /// Mark some (or all) of the user's notifications read.
    pub async fn mark_read(
        &self,
        user_id: UserId,
        ids: Option<&[NotificationId]>,
    ) -> Result<u64, DomainError> {
        self.repo.mark_read(user_id, ids, Utc::now()).await
    }

    /// Delete old notifications. Never fails: errors are logged.
    pub async fn sweep(&self, now: DateTime<Utc>) -> u64 {
        match self
            .repo
            .delete_expired(
                now - Duration::days(KEEP_READ_DAYS),
                now - Duration::days(KEEP_ANY_DAYS),
            )
            .await
        {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "notification retention sweep failed");
                0
            }
        }
    }

    /// A tournament's delivery settings.
    pub async fn settings(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Option<TournamentNotificationSettings>, DomainError> {
        self.repo.get_settings(tournament_id).await
    }

    /// Save a tournament's delivery settings. The URL must be a Discord
    /// webhook.
    pub async fn save_settings(
        &self,
        tournament_id: TournamentId,
        discord_enabled: bool,
        webhook: Option<Option<String>>,
        updated_by: UserId,
    ) -> Result<TournamentNotificationSettings, DomainError> {
        let webhook = match webhook {
            Some(Some(url)) if url.trim().is_empty() => Some(None),
            Some(Some(url)) => {
                validate_webhook_url(url.trim())?;
                Some(Some(url.trim().to_string()))
            }
            other => other,
        };
        let current = self.repo.get_settings(tournament_id).await?;
        let will_have_url = match &webhook {
            Some(url) => url.is_some(),
            None => current
                .as_ref()
                .is_some_and(|s| s.discord_webhook_url.is_some()),
        };
        if discord_enabled && !will_have_url {
            return Err(DomainError::InvalidState(
                "Add a Discord webhook URL before turning Discord on".to_string(),
            ));
        }
        self.repo
            .upsert_settings(tournament_id, discord_enabled, webhook, updated_by)
            .await
    }
}

/// Accept only Discord webhook URLs.
///
/// # Errors
///
/// `Validation` when the URL is not a Discord webhook.
pub fn validate_webhook_url(url: &str) -> Result<(), DomainError> {
    let ok = url.len() <= 512
        && !url.chars().any(char::is_whitespace)
        && [
            "https://discord.com/api/webhooks/",
            "https://discordapp.com/api/webhooks/",
        ]
        .iter()
        .any(|p| url.starts_with(p) && url.len() > p.len());
    if ok {
        Ok(())
    } else {
        Err(DomainError::InvalidState(
            "The webhook URL must be a Discord webhook (https://discord.com/api/webhooks/…)"
                .to_string(),
        ))
    }
}

/// The text and audience of an event, or `None` when it concerns nobody.
#[allow(clippy::too_many_lines)]
fn draft(event: &NotificationEvent, ctx: &MatchNotificationContext) -> Option<Draft> {
    let fixture = ctx.fixture();
    let round = ctx.round;
    let both = ctx.participants();
    let d = match event {
        NotificationEvent::ProposalReceived {
            proposal_id,
            proposed_by,
            counter,
            ..
        } => {
            let to = ctx.opponent_of(*proposed_by)?;
            let who = ctx.name_of(*proposed_by);
            Draft {
                kind: NotificationKind::ProposalReceived,
                title: if *counter {
                    "New times suggested".to_string()
                } else {
                    "New match time proposed".to_string()
                },
                body: format!(
                    "{who} proposed times for {fixture} (round {round}). Accept one, or suggest others."
                ),
                dedupe_key: format!("proposal:{proposal_id}"),
                recipients: vec![to],
                audience: Audience::Schedulers,
            }
        }
        NotificationEvent::ProposalAccepted { scheduled_at, .. } => Draft {
            kind: NotificationKind::ProposalAccepted,
            title: "Match time agreed".to_string(),
            body: format!(
                "{fixture} (round {round}) is set for {}. Check-in opens 15 minutes before.",
                when(*scheduled_at)
            ),
            dedupe_key: format!("scheduled:{}:{}", ctx.match_id, scheduled_at.timestamp()),
            recipients: both,
            audience: Audience::Roster,
        },
        NotificationEvent::ProposalRejected {
            proposal_id,
            proposed_by,
            ..
        } => Draft {
            kind: NotificationKind::ProposalRejected,
            title: "Proposed times turned down".to_string(),
            body: format!(
                "Your opponent turned down the times proposed for {fixture} (round {round}). Propose other times."
            ),
            dedupe_key: format!("proposal_rejected:{proposal_id}"),
            recipients: vec![*proposed_by],
            audience: Audience::Schedulers,
        },
        NotificationEvent::ProposalExpired {
            proposal_id,
            proposed_by,
            ..
        } => Draft {
            kind: NotificationKind::ProposalExpired,
            title: "Proposal expired".to_string(),
            body: format!(
                "Nobody answered the times proposed for {fixture} (round {round}). Propose again, or ask an organiser."
            ),
            dedupe_key: format!("proposal_expired:{proposal_id}"),
            recipients: vec![*proposed_by],
            audience: Audience::Schedulers,
        },
        NotificationEvent::MatchTimeSet { scheduled_at, .. } => Draft {
            kind: NotificationKind::MatchTimeSet,
            title: "Match time set".to_string(),
            body: format!(
                "An organiser set {fixture} (round {round}) for {}. Check-in opens 15 minutes before.",
                when(*scheduled_at)
            ),
            dedupe_key: format!("scheduled:{}:{}", ctx.match_id, scheduled_at.timestamp()),
            recipients: both,
            audience: Audience::Roster,
        },
        NotificationEvent::CheckinOpen { deadline, .. } => Draft {
            kind: NotificationKind::CheckinOpen,
            title: "Check-in is open".to_string(),
            body: format!(
                "Check in for {fixture} (round {round}) by {}. A side that doesn't is forfeited.",
                clock(*deadline)
            ),
            dedupe_key: format!("checkin_open:{}:{}", ctx.match_id, deadline.timestamp()),
            recipients: both,
            audience: Audience::Schedulers,
        },
        NotificationEvent::CheckinReminder {
            registration_id,
            deadline,
            ..
        } => Draft {
            kind: NotificationKind::CheckinReminder,
            title: "Check in now".to_string(),
            body: format!(
                "{} has not checked in for {fixture} (round {round}). Check-in closes at {}.",
                ctx.name_of(*registration_id),
                clock(*deadline)
            ),
            dedupe_key: format!("checkin_reminder:{}:{registration_id}", ctx.match_id),
            recipients: vec![*registration_id],
            audience: Audience::Schedulers,
        },
        NotificationEvent::VetoStarted { session_id, .. } => Draft {
            kind: NotificationKind::VetoStarted,
            title: "Map veto started".to_string(),
            body: format!("The map veto for {fixture} (round {round}) has started."),
            dedupe_key: format!("veto:{session_id}"),
            recipients: both,
            audience: Audience::Schedulers,
        },
        NotificationEvent::ServerReady { reservation_id, .. } => Draft {
            kind: NotificationKind::ServerReady,
            title: "Server ready".to_string(),
            body: format!(
                "The game server for {fixture} (round {round}) is ready. Connect from the match page."
            ),
            dedupe_key: format!("server_ready:{reservation_id}"),
            recipients: both,
            audience: Audience::Roster,
        },
        NotificationEvent::ServerQueued { reservation_id, .. } => Draft {
            kind: NotificationKind::ServerQueued,
            title: "Waiting for a game server".to_string(),
            body: format!(
                "Every game server is busy. {fixture} (round {round}) gets the next free one."
            ),
            dedupe_key: format!("server_queued:{reservation_id}"),
            recipients: both,
            audience: Audience::Schedulers,
        },
        NotificationEvent::ServerAssignedFromQueue { reservation_id, .. } => Draft {
            kind: NotificationKind::ServerAssignedFromQueue,
            title: "Game server free".to_string(),
            body: format!(
                "A game server is now loading {fixture} (round {round}). Connect details appear on the match page shortly."
            ),
            dedupe_key: format!("server_assigned:{reservation_id}"),
            recipients: both,
            audience: Audience::Roster,
        },
        NotificationEvent::ResultToConfirm {
            claim_id,
            reported_by,
            ..
        } => {
            let (recipients, body) = match reported_by {
                Some(reg) => (
                    vec![ctx.opponent_of(*reg)?],
                    format!(
                        "{} reported a result for {fixture} (round {round}). Confirm it, or dispute it.",
                        ctx.name_of(*reg)
                    ),
                ),
                None => (
                    both,
                    format!(
                        "The game server reported the result of {fixture} (round {round}). It becomes final unless disputed."
                    ),
                ),
            };
            Draft {
                kind: NotificationKind::ResultToConfirm,
                title: "Result to confirm".to_string(),
                body,
                dedupe_key: format!("claim:{claim_id}"),
                recipients,
                audience: Audience::Schedulers,
            }
        }
        NotificationEvent::ResultFinal { .. } => {
            let status = ctx.status.as_str();
            if status != "completed" && status != "forfeit" {
                return None;
            }
            let body = match (
                ctx.winner_registration_id,
                ctx.participant1_score,
                ctx.participant2_score,
            ) {
                (Some(w), Some(a), Some(b)) => format!(
                    "{fixture} (round {round}) is final: {} won {}-{}{}.",
                    ctx.name_of(w),
                    a.max(b),
                    a.min(b),
                    if status == "forfeit" {
                        " by forfeit"
                    } else {
                        ""
                    }
                ),
                _ => format!("{fixture} (round {round}) is over."),
            };
            Draft {
                kind: NotificationKind::ResultFinal,
                title: "Match result final".to_string(),
                body,
                dedupe_key: format!("final:{}:{status}", ctx.match_id),
                recipients: both,
                audience: Audience::Roster,
            }
        }
        NotificationEvent::DisputeRuled { dispute_id, .. } => Draft {
            kind: NotificationKind::DisputeRuled,
            title: "Dispute ruled".to_string(),
            body: format!(
                "An organiser ruled on the dispute for {fixture} (round {round}). See the match page."
            ),
            dedupe_key: format!("dispute:{dispute_id}"),
            recipients: both,
            audience: Audience::Roster,
        },
        NotificationEvent::MatchReset { .. } => Draft {
            kind: NotificationKind::MatchReset,
            title: "Match reset".to_string(),
            body: format!(
                "An organiser reset {fixture} (round {round}). It will be scheduled and played again."
            ),
            dedupe_key: format!("reset:{}:{}", ctx.match_id, Utc::now().timestamp()),
            recipients: both,
            audience: Audience::Roster,
        },
    };
    if d.recipients.is_empty() {
        return None;
    }
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::validate_webhook_url;

    #[test]
    fn webhook_must_be_discord() {
        assert!(validate_webhook_url("https://discord.com/api/webhooks/1/abc").is_ok());
        assert!(validate_webhook_url("https://discordapp.com/api/webhooks/1/abc").is_ok());
        assert!(validate_webhook_url("https://discord.com/api/webhooks/").is_err());
        assert!(validate_webhook_url("http://discord.com/api/webhooks/1/abc").is_err());
        assert!(validate_webhook_url("https://evil.example/api/webhooks/1").is_err());
        assert!(validate_webhook_url("https://discord.com.evil.example/api/webhooks/1").is_err());
    }
}
