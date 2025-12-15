//! Ban management commands.
//!
//! Provides commands for managing user bans including:
//! - Platform-wide bans
//! - Scoped bans (league, tournament, matchmaking, chat)
//! - Ban history and lifting

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use clap::{Args, Subcommand, ValueEnum};
use portal_db::entities::NewBan;
use portal_db::repositories::BanRepository;
use portal_db::PgPool;
use serde::Serialize;
use tabled::Tabled;
use uuid::Uuid;

use crate::output::{error, format_optional, format_timestamp, format_uuid, info, output_list, success, OutputFormat};

/// Ban management commands.
#[derive(Args)]
pub struct BanCommand {
    #[command(subcommand)]
    command: BanSubcommand,
}

#[derive(Subcommand)]
enum BanSubcommand {
    /// List bans with optional filters
    List {
        /// Filter by user ID
        #[arg(long)]
        user: Option<Uuid>,

        /// Filter by ban type
        #[arg(long, value_enum)]
        ban_type: Option<BanTypeArg>,

        /// Show only active bans
        #[arg(long)]
        active_only: bool,

        /// Filter by scope type
        #[arg(long)]
        scope_type: Option<String>,

        /// Filter by scope ID
        #[arg(long)]
        scope_id: Option<Uuid>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get ban details
    Get {
        /// Ban ID
        id: Uuid,
    },

    /// Create a new ban
    Create {
        /// User ID to ban
        #[arg(long)]
        user: Uuid,

        /// Ban type
        #[arg(long, value_enum)]
        ban_type: BanTypeArg,

        /// Reason for the ban
        #[arg(long)]
        reason: String,

        /// Duration (e.g., "7d", "24h", "1w", or "permanent")
        #[arg(long, default_value = "permanent")]
        duration: String,

        /// Scope type for scoped bans (league, tournament)
        #[arg(long)]
        scope_type: Option<String>,

        /// Scope ID for scoped bans
        #[arg(long)]
        scope_id: Option<Uuid>,
    },

    /// Lift an existing ban
    Lift {
        /// Ban ID to lift
        id: Uuid,

        /// Reason for lifting the ban
        #[arg(long)]
        reason: Option<String>,
    },

    /// View ban history for a user
    History {
        /// User ID
        user_id: Uuid,

        /// Include expired bans
        #[arg(long)]
        include_expired: bool,

        /// Include lifted bans
        #[arg(long)]
        include_lifted: bool,
    },

    /// Check if a user is currently banned
    Check {
        /// User ID
        user_id: Uuid,

        /// Ban type to check (defaults to platform)
        #[arg(long, value_enum)]
        ban_type: Option<BanTypeArg>,
    },
}

/// Ban type argument for CLI.
#[derive(Clone, Copy, ValueEnum)]
enum BanTypeArg {
    /// Complete platform ban
    Platform,
    /// Matchmaking ban
    Matchmaking,
    /// Chat ban
    Chat,
    /// League-specific ban
    League,
    /// Tournament-specific ban
    Tournament,
}

impl BanTypeArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Matchmaking => "matchmaking",
            Self::Chat => "chat",
            Self::League => "league",
            Self::Tournament => "tournament",
        }
    }
}

impl BanCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let repo = BanRepository::new(pool.clone());

        match &self.command {
            BanSubcommand::List {
                user,
                ban_type,
                active_only,
                scope_type,
                scope_id,
                limit,
            } => {
                list_bans(
                    &repo,
                    *user,
                    ban_type.map(BanTypeArg::as_str),
                    *active_only,
                    scope_type.as_deref(),
                    *scope_id,
                    *limit,
                    format,
                )
                .await
            }
            BanSubcommand::Get { id } => get_ban(&repo, *id, format).await,
            BanSubcommand::Create {
                user,
                ban_type,
                reason,
                duration,
                scope_type,
                scope_id,
            } => {
                create_ban(
                    &repo,
                    *user,
                    ban_type.as_str(),
                    reason,
                    duration,
                    scope_type.as_deref(),
                    *scope_id,
                )
                .await
            }
            BanSubcommand::Lift { id, reason } => lift_ban(&repo, *id, reason.as_deref()).await,
            BanSubcommand::History {
                user_id,
                include_expired,
                include_lifted,
            } => ban_history(&repo, *user_id, *include_expired, *include_lifted, format).await,
            BanSubcommand::Check { user_id, ban_type } => {
                check_ban(&repo, *user_id, ban_type.map(BanTypeArg::as_str)).await
            }
        }
    }
}

/// Table row for ban display.
#[derive(Tabled, Serialize)]
pub struct BanTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "User")]
    pub user_id: String,
    #[tabled(rename = "Type")]
    pub ban_type: String,
    #[tabled(rename = "Reason")]
    pub reason: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Expires")]
    pub expires_at: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

fn get_ban_status(ban: &portal_db::entities::BanRow) -> String {
    if ban.lifted_at.is_some() {
        "Lifted".to_string()
    } else if let Some(ends) = ban.ends_at {
        if ends < Utc::now() {
            "Expired".to_string()
        } else {
            "Active".to_string()
        }
    } else {
        "Active (Permanent)".to_string()
    }
}

fn truncate_reason(reason: &str, max_len: usize) -> String {
    if reason.len() > max_len {
        format!("{}...", &reason[..max_len - 3])
    } else {
        reason.to_string()
    }
}

async fn list_bans(
    repo: &BanRepository,
    user: Option<Uuid>,
    ban_type: Option<&str>,
    active_only: bool,
    scope_type: Option<&str>,
    scope_id: Option<Uuid>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let bans = repo
        .list(user, ban_type, active_only, scope_type, scope_id, limit, 0)
        .await
        .context("Failed to fetch bans")?;

    let rows: Vec<BanTableRow> = bans
        .into_iter()
        .map(|b| BanTableRow {
            id: format_uuid(&b.id),
            user_id: format_uuid(&b.user_id),
            ban_type: b.ban_type.clone(),
            reason: truncate_reason(&b.reason, 30),
            status: get_ban_status(&b),
            expires_at: b
                .ends_at.map_or_else(|| "Never".to_string(), |t| format_timestamp(&t)),
            created_at: format_timestamp(&b.created_at),
        })
        .collect();

    output_list(&rows, format)
}

async fn get_ban(repo: &BanRepository, id: Uuid, format: OutputFormat) -> Result<()> {
    let ban = repo
        .find_by_id(id)
        .await
        .context("Failed to fetch ban")?;

    if let Some(b) = ban {
        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": b.id,
                    "user_id": b.user_id,
                    "ban_type": b.ban_type,
                    "reason": b.reason,
                    "scope_type": b.scope_type,
                    "scope_id": b.scope_id,
                    "issued_by": b.issued_by,
                    "starts_at": b.starts_at,
                    "ends_at": b.ends_at,
                    "lifted_at": b.lifted_at,
                    "lifted_by": b.lifted_by,
                    "lift_reason": b.lift_reason,
                    "created_at": b.created_at,
                    "updated_at": b.updated_at,
                    "status": get_ban_status(&b),
                }))?
            );
        } else {
            println!("Ban Details:");
            println!("  ID:          {}", b.id);
            println!("  User:        {}", b.user_id);
            println!("  Type:        {}", b.ban_type);
            println!("  Reason:      {}", b.reason);
            println!("  Status:      {}", get_ban_status(&b));
            println!("  Scope Type:  {}", format_optional(&b.scope_type));
            println!("  Scope ID:    {}", b.scope_id.map_or_else(|| "-".to_string(), |id| id.to_string()));
            println!("  Issued By:   {}", b.issued_by.map_or_else(|| "-".to_string(), |id| id.to_string()));
            println!("  Starts:      {}", format_timestamp(&b.starts_at));
            println!(
                "  Ends:        {}",
                b.ends_at.map_or_else(|| "Never (Permanent)".to_string(), |t| format_timestamp(&t))
            );
            if let Some(lifted) = b.lifted_at {
                println!("  Lifted At:   {}", format_timestamp(&lifted));
                println!("  Lifted By:   {}", b.lifted_by.map_or_else(|| "-".to_string(), |id| id.to_string()));
                println!("  Lift Reason: {}", format_optional(&b.lift_reason));
            }
            println!("  Created:     {}", format_timestamp(&b.created_at));
        }
        Ok(())
    } else {
        error(&format!("Ban not found: {id}"));
        std::process::exit(1);
    }
}

/// Parse a human-readable duration string.
fn parse_duration(s: &str) -> Result<Option<Duration>> {
    if s == "permanent" {
        return Ok(None);
    }

    let len = s.len();
    if len < 2 {
        anyhow::bail!("Invalid duration format: {s}");
    }

    let (num, unit) = s.split_at(len - 1);
    let n: i64 = num
        .parse()
        .context("Invalid number in duration")?;

    let duration = match unit {
        "m" => Duration::minutes(n),
        "h" => Duration::hours(n),
        "d" => Duration::days(n),
        "w" => Duration::weeks(n),
        _ => anyhow::bail!("Invalid duration unit: {unit}. Use m, h, d, or w"),
    };

    Ok(Some(duration))
}

async fn create_ban(
    repo: &BanRepository,
    user_id: Uuid,
    ban_type: &str,
    reason: &str,
    duration: &str,
    scope_type: Option<&str>,
    scope_id: Option<Uuid>,
) -> Result<()> {
    // Validate scoped bans
    if matches!(ban_type, "league" | "tournament") && (scope_type.is_none() || scope_id.is_none()) {
        error(&format!(
            "{ban_type} bans require --scope-type and --scope-id"
        ));
        std::process::exit(1);
    }

    // Parse duration
    let ends_at = parse_duration(duration)?.map(|d| Utc::now() + d);

    let new_ban = NewBan {
        user_id,
        ban_type: ban_type.to_string(),
        reason: reason.to_string(),
        scope_type: scope_type.map(String::from),
        scope_id,
        issued_by: None, // CLI doesn't have user context
        starts_at: Some(Utc::now()),
        ends_at,
    };

    let ban = repo.create(new_ban).await.context("Failed to create ban")?;

    let duration_str = match ends_at {
        Some(t) => {
            let remaining = t - Utc::now();
            format_duration(remaining)
        }
        None => "permanent".to_string(),
    };

    success(&format!(
        "Created {ban_type} ban for user {user_id} ({duration_str})"
    ));
    info(&format!("Ban ID: {}", ban.id));

    Ok(())
}

fn format_duration(d: Duration) -> String {
    let days = d.num_days();
    let hours = d.num_hours() % 24;
    let minutes = d.num_minutes() % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

async fn lift_ban(repo: &BanRepository, id: Uuid, reason: Option<&str>) -> Result<()> {
    // Verify ban exists and is active
    let ban = repo
        .find_by_id(id)
        .await
        .context("Failed to fetch ban")?;

    let Some(ban) = ban else {
        error(&format!("Ban not found: {id}"));
        std::process::exit(1);
    };

    if ban.lifted_at.is_some() {
        error("Ban has already been lifted");
        std::process::exit(1);
    }

    if let Some(ends) = ban.ends_at {
        if ends < Utc::now() {
            error("Ban has already expired");
            std::process::exit(1);
        }
    }

    // Lift the ban
    repo.lift_by_id(id, None, reason)
        .await
        .context("Failed to lift ban")?;

    success(&format!("Lifted ban {} for user {}", id, ban.user_id));

    Ok(())
}

async fn ban_history(
    repo: &BanRepository,
    user_id: Uuid,
    include_expired: bool,
    include_lifted: bool,
    format: OutputFormat,
) -> Result<()> {
    // Get all bans for user
    let bans = repo
        .list(Some(user_id), None, false, None, None, 100, 0)
        .await
        .context("Failed to fetch ban history")?;

    // Filter based on options and convert to table rows
    let rows: Vec<BanTableRow> = bans
        .into_iter()
        .filter(|b| {
            let is_expired = b.ends_at.is_some_and(|t| t < Utc::now());
            let is_lifted = b.lifted_at.is_some();

            if !include_expired && is_expired && !is_lifted {
                return false;
            }
            if !include_lifted && is_lifted {
                return false;
            }
            true
        })
        .map(|b| BanTableRow {
            id: format_uuid(&b.id),
            user_id: format_uuid(&b.user_id),
            ban_type: b.ban_type.clone(),
            reason: truncate_reason(&b.reason, 30),
            status: get_ban_status(&b),
            expires_at: b
                .ends_at
                .map_or_else(|| "Never".to_string(), |t| format_timestamp(&t)),
            created_at: format_timestamp(&b.created_at),
        })
        .collect();

    if rows.is_empty() {
        info(&format!("No ban history found for user {user_id}"));
        if !include_expired || !include_lifted {
            println!("Tip: Use --include-expired and --include-lifted to see all bans");
        }
    } else {
        output_list(&rows, format)?;
    }

    Ok(())
}

async fn check_ban(repo: &BanRepository, user_id: Uuid, ban_type: Option<&str>) -> Result<()> {
    let ban_type = ban_type.unwrap_or("platform");

    let bans = repo
        .list(Some(user_id), Some(ban_type), true, None, None, 1, 0)
        .await
        .context("Failed to check ban status")?;

    if let Some(ban) = bans.first() {
        println!("{}", "User is BANNED".red().bold());
        println!();
        println!("  Type:    {}", ban.ban_type);
        println!("  Reason:  {}", ban.reason);
        println!(
            "  Expires: {}",
            ban.ends_at.map_or_else(|| "Never (Permanent)".to_string(), |t| format_timestamp(&t))
        );
        println!("  Ban ID:  {}", ban.id);
    } else {
        println!("{}", "User is NOT banned".green().bold());
        println!("  Checked ban type: {ban_type}");
    }

    Ok(())
}

use colored::Colorize;
