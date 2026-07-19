//! User management commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_core::UserId;
use portal_db::PgPool;
use portal_db::entities::{NewBan, NewUser, UpdateUser};
use portal_db::repositories::{BanRepository, UserRepository};
use uuid::Uuid;

use crate::output::{
    OutputFormat, UserTableRow, error, format_optional, format_timestamp, format_uuid, output_list,
    success,
};

/// User management commands.
#[derive(Args)]
pub struct UserCommand {
    #[command(subcommand)]
    command: UserSubcommand,
}

#[derive(Subcommand)]
enum UserSubcommand {
    /// List users
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Search by email or username
        #[arg(long)]
        search: Option<String>,
        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get user details
    Get {
        /// User ID
        id: Uuid,
    },

    /// Create a new user
    Create {
        /// Username
        #[arg(long)]
        username: String,
        /// Email address
        #[arg(long)]
        email: String,
        /// Password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,
        /// Display name for player profile (defaults to username)
        #[arg(long)]
        display_name: Option<String>,
    },

    /// Update user
    Update {
        /// User ID
        id: Uuid,
        /// New username
        #[arg(long)]
        username: Option<String>,
        /// New email
        #[arg(long)]
        email: Option<String>,
    },

    /// Disable user account
    Disable {
        /// User ID
        id: Uuid,
        /// Reason for disabling
        #[arg(long)]
        reason: Option<String>,
    },

    /// Enable user account
    Enable {
        /// User ID
        id: Uuid,
    },

    /// Force password reset
    ResetPassword {
        /// User ID
        id: Uuid,
        /// New password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,
    },

    /// Ban user
    Ban {
        /// User ID
        id: Uuid,
        /// Ban reason
        #[arg(long)]
        reason: String,
        /// Ban duration in days (omit for permanent)
        #[arg(long)]
        duration_days: Option<i64>,
    },

    /// Unban user
    Unban {
        /// User ID
        id: Uuid,
        /// Reason for lifting ban
        #[arg(long)]
        reason: Option<String>,
    },

    /// Import legacy players as Steam-provider accounts.
    ///
    /// Reads a JSON array of `{steam_id, name, email, created_at}` (the
    /// tenmans_be export) and provisions each as a Steam sign-in account,
    /// exactly as if they had signed in through Steam. Idempotent: rows
    /// whose SteamID64 already has a player are counted as existing and
    /// left untouched. Rows without a valid SteamID64 are skipped.
    ImportSteam {
        /// Path to the JSON export file
        #[arg(long)]
        file: std::path::PathBuf,
        /// Parse and validate only; write nothing
        #[arg(long)]
        dry_run: bool,
    },
}

/// One row of the legacy player export.
#[derive(serde::Deserialize)]
struct LegacyPlayer {
    steam_id: String,
    name: String,
    email: Option<String>,
    created_at: Option<String>,
}

impl LegacyPlayer {
    /// Valid SteamID64, or `None` for internal rows (e.g. the legacy
    /// `SYSTEM` actor uses steam_id `0`).
    fn steam_id_64(&self) -> Option<i64> {
        self.steam_id.parse::<i64>().ok().filter(|id| *id > 0)
    }
}

impl UserCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let user_repo = UserRepository::new(pool.clone());
        let ban_repo = BanRepository::new(pool.clone());

        match &self.command {
            UserSubcommand::List {
                status,
                search,
                limit,
            } => {
                list_users(
                    &user_repo,
                    status.as_deref(),
                    search.as_deref(),
                    *limit,
                    format,
                )
                .await
            }

            UserSubcommand::Get { id } => get_user(&user_repo, *id, format).await,

            UserSubcommand::Create {
                username,
                email,
                password,
                display_name,
            } => {
                create_user(
                    &user_repo,
                    pool,
                    username,
                    email,
                    password.as_deref(),
                    display_name.as_deref(),
                    format,
                )
                .await
            }

            UserSubcommand::Update {
                id,
                username,
                email,
            } => {
                update_user(
                    &user_repo,
                    *id,
                    username.as_deref(),
                    email.as_deref(),
                    format,
                )
                .await
            }

            UserSubcommand::Disable { id, reason } => {
                disable_user(&user_repo, *id, reason.as_deref()).await
            }

            UserSubcommand::Enable { id } => enable_user(&user_repo, *id).await,

            UserSubcommand::ResetPassword { id, password } => {
                reset_password(&user_repo, *id, password.as_deref()).await
            }

            UserSubcommand::Ban {
                id,
                reason,
                duration_days,
            } => ban_user(&user_repo, &ban_repo, *id, reason, *duration_days).await,

            UserSubcommand::Unban { id, reason } => {
                unban_user(&user_repo, &ban_repo, *id, reason.as_deref()).await
            }

            UserSubcommand::ImportSteam { file, dry_run } => {
                import_steam_players(pool, file, *dry_run).await
            }
        }
    }
}

/// Import legacy players through the same provisioning path as Steam
/// sign-in, then backdate `created_at` and restore the real email where
/// the legacy site had one.
async fn import_steam_players(pool: &PgPool, file: &std::path::Path, dry_run: bool) -> Result<()> {
    use portal_db::{NewUserRole, PgPlayerRepository, PgUserRepository, RoleRepository};
    use portal_domain::services::UserService;
    use std::sync::Arc;

    let raw = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read {}", file.display()))?;
    let rows: Vec<LegacyPlayer> = serde_json::from_str(&raw).context("Invalid JSON export")?;

    let service = UserService::new(
        Arc::new(PgUserRepository::new(pool.clone())),
        Arc::new(PgPlayerRepository::new(pool.clone())),
    );
    let role_repo = RoleRepository::new(pool.clone());
    let default_role = role_repo
        .find_by_name("user")
        .await
        .context("Failed to look up default role")?;

    let (mut created, mut existing, mut skipped) = (0u32, 0u32, 0u32);
    for row in &rows {
        let Some(steam_id_64) = row.steam_id_64() else {
            println!("skip (no valid SteamID64): {}", row.name);
            skipped += 1;
            continue;
        };
        if dry_run {
            created += 1;
            continue;
        }

        let (user, player, was_created) = service
            .login_with_steam(steam_id_64, Some(&row.name))
            .await
            .with_context(|| format!("Failed to provision {} ({steam_id_64})", row.name))?;

        if !was_created {
            existing += 1;
            continue;
        }
        created += 1;

        if let Some(role) = &default_role {
            role_repo
                .assign_to_user(NewUserRole {
                    user_id: user.id.into(),
                    role_id: role.id,
                    scope_type: None,
                    scope_id: None,
                    granted_by: None,
                    expires_at: None,
                })
                .await
                .with_context(|| format!("Failed to grant default role to {}", row.name))?;
        }

        // Backdate to the legacy signup time so account age survives.
        if let Some(ts) = row
            .created_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        {
            let ts = ts.to_utc();
            sqlx::query("UPDATE users SET created_at = $1 WHERE id = $2")
                .bind(ts)
                .bind(uuid::Uuid::from(user.id))
                .execute(pool)
                .await?;
            sqlx::query("UPDATE players SET created_at = $1 WHERE id = $2")
                .bind(ts)
                .bind(uuid::Uuid::from(player.id))
                .execute(pool)
                .await?;
        }

        // Restore the real email over the steam_<id>@steam.invalid
        // placeholder (only the legacy email-auth account has one).
        if let Some(email) = row.email.as_deref().filter(|e| !e.is_empty()) {
            sqlx::query("UPDATE users SET email = $1 WHERE id = $2")
                .bind(email)
                .bind(uuid::Uuid::from(user.id))
                .execute(pool)
                .await
                .with_context(|| format!("Failed to set email for {}", row.name))?;
        }
    }

    if dry_run {
        success(&format!(
            "[dry-run] {} importable, {} skipped of {} rows",
            created,
            skipped,
            rows.len()
        ));
    } else {
        success(&format!(
            "Imported {created} players ({existing} already present, {skipped} skipped)"
        ));
    }
    Ok(())
}

async fn list_users(
    repo: &UserRepository,
    status: Option<&str>,
    search: Option<&str>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let users = repo
        .list(status, search, limit, 0)
        .await
        .context("Failed to fetch users")?;

    let rows: Vec<UserTableRow> = users
        .into_iter()
        .map(|u| UserTableRow {
            id: format_uuid(&u.id),
            username: u.username,
            email: u.email,
            status: u.status,
            two_factor: if u.two_factor_enabled { "Yes" } else { "No" }.to_string(),
            created_at: format_timestamp(&u.created_at),
        })
        .collect();

    output_list(&rows, format)
}

async fn get_user(repo: &UserRepository, id: Uuid, format: OutputFormat) -> Result<()> {
    let user_id = UserId::from_uuid(id);
    let user = repo
        .find_by_id(user_id)
        .await
        .context("Failed to fetch user")?;

    if let Some(u) = user {
        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": u.id,
                    "username": u.username,
                    "email": u.email,
                    "email_verified": u.email_verified,
                    "status": u.status,
                    "status_reason": u.status_reason,
                    "two_factor_enabled": u.two_factor_enabled,
                    "locale": u.locale,
                    "timezone": u.timezone,
                    "created_at": u.created_at,
                    "updated_at": u.updated_at,
                    "last_login_at": u.last_login_at,
                }))?
            );
        } else {
            println!("User Details:");
            println!("  ID:              {}", u.id);
            println!("  Username:        {}", u.username);
            println!("  Email:           {}", u.email);
            println!("  Email Verified:  {}", u.email_verified);
            println!("  Status:          {}", u.status);
            println!("  Status Reason:   {}", format_optional(&u.status_reason));
            println!("  2FA Enabled:     {}", u.two_factor_enabled);
            println!("  Locale:          {}", format_optional(&u.locale));
            println!("  Timezone:        {}", format_optional(&u.timezone));
            println!("  Created:         {}", format_timestamp(&u.created_at));
            println!("  Updated:         {}", format_timestamp(&u.updated_at));
            println!(
                "  Last Login:      {}",
                u.last_login_at
                    .map_or_else(|| "Never".to_string(), |t| format_timestamp(&t))
            );
        }
        Ok(())
    } else {
        error(&format!("User not found: {id}"));
        std::process::exit(1);
    }
}

async fn create_user(
    repo: &UserRepository,
    pool: &PgPool,
    username: &str,
    email: &str,
    password: Option<&str>,
    display_name: Option<&str>,
    _format: OutputFormat,
) -> Result<()> {
    // Prompt for password if not provided
    let password = match password {
        Some(p) => p.to_string(),
        None => dialoguer::Password::new()
            .with_prompt("Password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .interact()
            .context("Failed to read password")?,
    };

    // Hash password using argon2
    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2_hasher = argon2::Argon2::default();
    let password_hash =
        argon2::PasswordHasher::hash_password(&argon2_hasher, password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?
            .to_string();

    let new_user = NewUser {
        username: username.to_string(),
        email: email.to_string(),
        password_hash: Some(password_hash),
    };

    let user = repo
        .create(new_user)
        .await
        .context("Failed to create user")?;

    // Create player profile with the same ID as the user
    let player_repo = portal_db::repositories::PlayerRepository::new(pool.clone());
    let display_name = display_name.unwrap_or(username);
    let new_player = portal_db::entities::NewPlayer {
        user_id: user.id,
        display_name: display_name.to_string(),
        avatar_url: None,
        country_code: None,
    };

    let player = player_repo
        .create(new_player)
        .await
        .context("Failed to create player profile")?;

    success(&format!(
        "Created user: {} with player profile: {}",
        user.id, player.id
    ));
    Ok(())
}

async fn update_user(
    repo: &UserRepository,
    id: Uuid,
    _username: Option<&str>,
    email: Option<&str>,
    _format: OutputFormat,
) -> Result<()> {
    let user_id = UserId::from_uuid(id);
    let update = UpdateUser {
        email: email.map(String::from),
        ..Default::default()
    };

    let user = repo
        .update(user_id, update)
        .await
        .context("Failed to update user")?;

    success(&format!("Updated user: {}", user.id));
    Ok(())
}

async fn disable_user(repo: &UserRepository, id: Uuid, reason: Option<&str>) -> Result<()> {
    let user_id = UserId::from_uuid(id);
    let user = repo
        .disable(user_id, reason)
        .await
        .context("Failed to disable user")?;

    success(&format!("Disabled user: {}", user.id));
    Ok(())
}

async fn enable_user(repo: &UserRepository, id: Uuid) -> Result<()> {
    let user_id = UserId::from_uuid(id);
    let user = repo
        .enable(user_id)
        .await
        .context("Failed to enable user")?;

    success(&format!("Enabled user: {}", user.id));
    Ok(())
}

async fn reset_password(repo: &UserRepository, id: Uuid, password: Option<&str>) -> Result<()> {
    let password = match password {
        Some(p) => p.to_string(),
        None => dialoguer::Password::new()
            .with_prompt("New password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .interact()
            .context("Failed to read password")?,
    };

    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2_hasher = argon2::Argon2::default();
    let password_hash =
        argon2::PasswordHasher::hash_password(&argon2_hasher, password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?
            .to_string();

    let user_id = UserId::from_uuid(id);
    repo.update_password(user_id, &password_hash)
        .await
        .context("Failed to reset password")?;

    success(&format!("Reset password for user: {id}"));
    Ok(())
}

async fn ban_user(
    user_repo: &UserRepository,
    ban_repo: &BanRepository,
    id: Uuid,
    reason: &str,
    duration_days: Option<i64>,
) -> Result<()> {
    // Calculate end date
    let ends_at = duration_days.map(|d| chrono::Utc::now() + chrono::Duration::days(d));

    // Create ban record
    let new_ban = NewBan {
        user_id: id,
        ban_type: "platform".to_string(),
        reason: reason.to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None, // CLI doesn't have a user context
        starts_at: Some(chrono::Utc::now()),
        ends_at,
    };

    let ban = ban_repo
        .create(new_ban)
        .await
        .context("Failed to create ban")?;

    // Update user status
    let user_id = UserId::from_uuid(id);
    let update = UpdateUser {
        status: Some("banned".to_string()),
        status_reason: Some(reason.to_string()),
        ..Default::default()
    };

    user_repo
        .update(user_id, update)
        .await
        .context("Failed to update user status")?;

    let duration_str = match duration_days {
        Some(d) => format!("{d} days"),
        None => "permanent".to_string(),
    };

    success(&format!(
        "Banned user {id} ({duration_str}). Ban ID: {}",
        ban.id
    ));
    Ok(())
}

async fn unban_user(
    user_repo: &UserRepository,
    ban_repo: &BanRepository,
    id: Uuid,
    reason: Option<&str>,
) -> Result<()> {
    // Lift active bans
    let lifted = ban_repo
        .lift(id, None, reason)
        .await
        .context("Failed to lift bans")?;

    if lifted.is_empty() {
        error(&format!("No active bans found for user: {id}"));
        return Ok(());
    }

    // Update user status
    let user_id = UserId::from_uuid(id);
    let update = UpdateUser {
        status: Some("active".to_string()),
        status_reason: reason.map(String::from),
        ..Default::default()
    };

    user_repo
        .update(user_id, update)
        .await
        .context("Failed to update user status")?;

    success(&format!("Lifted {} ban(s) for user: {id}", lifted.len()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LegacyPlayer;

    #[test]
    fn legacy_export_parses_and_validates_steam_ids() {
        let rows: Vec<LegacyPlayer> = serde_json::from_str(
            r#"[
              {"steam_id":"0","name":"SYSTEM","email":null,"created_at":"2025-01-26T16:58:55Z"},
              {"steam_id":"76561198014255226","name":"gwoody","email":"g@example.com","created_at":"2025-02-04T21:38:15Z"},
              {"steam_id":"not-a-number","name":"broken","email":null,"created_at":null}
            ]"#,
        )
        .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].steam_id_64(), None, "SYSTEM row must be skipped");
        assert_eq!(rows[1].steam_id_64(), Some(76_561_198_014_255_226));
        assert_eq!(rows[1].email.as_deref(), Some("g@example.com"));
        assert_eq!(rows[2].steam_id_64(), None);
    }
}
