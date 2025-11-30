//! Database utilities commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use sqlx::PgPool;

use crate::output::{info, success, warn, OutputFormat};

/// Database utilities.
#[derive(Args)]
pub struct DbCommand {
    #[command(subcommand)]
    command: DbSubcommand,
}

#[derive(Subcommand)]
enum DbSubcommand {
    /// Run pending migrations
    Migrate,

    /// Show migration status
    Status,

    /// Show database statistics
    Stats,

    /// Seed database with test data
    Seed {
        /// Number of test users to create
        #[arg(long, default_value = "10")]
        users: i32,
    },

    /// Clear all data (DANGEROUS)
    Clear {
        /// Confirm by typing "yes-delete-everything"
        #[arg(long)]
        confirm: String,
    },
}

impl DbCommand {
    pub async fn execute(&self, pool: &PgPool, _format: OutputFormat) -> Result<()> {
        match &self.command {
            DbSubcommand::Migrate => run_migrations(pool),
            DbSubcommand::Status => show_status(pool).await,
            DbSubcommand::Stats => show_stats(pool).await,
            DbSubcommand::Seed { users } => seed_database(pool, *users).await,
            DbSubcommand::Clear { confirm } => clear_database(pool, confirm).await,
        }
    }
}

#[allow(clippy::unnecessary_wraps)] // Returns Result for consistency with other db commands
fn run_migrations(_pool: &PgPool) -> Result<()> {
    info("Running migrations...");

    // Note: In a real implementation, you would use sqlx-cli or run migrations here
    // For now, we just print instructions
    println!("\nTo run migrations, use:");
    println!("  sqlx migrate run");
    println!("\nOr with the DATABASE_URL:");
    println!("  sqlx migrate run --database-url $DATABASE_URL");

    Ok(())
}

async fn show_status(pool: &PgPool) -> Result<()> {
    info("Database Status");
    println!();

    // Check connection
    let version: (Option<String>,) = sqlx::query_as("SELECT version()")
        .fetch_one(pool)
        .await
        .context("Failed to query database")?;

    println!("PostgreSQL: {}", version.0.unwrap_or_default());

    // Check migrations table
    let has_migrations: (Option<bool>,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await
    .context("Failed to check migrations")?;

    if has_migrations.0.unwrap_or(false) {
        let migration_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
                .fetch_one(pool)
                .await
                .context("Failed to count migrations")?;

        println!("Migrations Applied: {}", migration_count.0);
    } else {
        println!("Migrations: Not initialized");
    }

    Ok(())
}

async fn show_stats(pool: &PgPool) -> Result<()> {
    info("Database Statistics");
    println!();

    // Get table counts
    let tables = [
        ("users", "Users"),
        ("players", "Players"),
        ("leagues", "Leagues"),
        ("league_seasons", "League Seasons"),
        ("league_teams", "League Teams"),
        ("league_team_seasons", "Team Seasons"),
        ("league_team_members", "Team Members"),
        ("league_team_invitations", "Team Invitations"),
        ("matches", "Matches"),
        ("tournaments", "Tournaments"),
        ("games", "Games"),
        ("bans", "Bans"),
        ("roles", "Roles"),
    ];

    for (table, display) in tables {
        let query = format!("SELECT COUNT(*) FROM {table}");
        match sqlx::query_scalar::<_, i64>(&query)
            .fetch_one(pool)
            .await
        {
            Ok(count) => println!("{display:20} {count:>10}"),
            Err(_) => println!("{:20} {:>10}", display, "(not found)"),
        }
    }

    Ok(())
}

async fn seed_database(pool: &PgPool, user_count: i32) -> Result<()> {
    warn(&format!(
        "This will create {user_count} test users with associated data"
    ));

    let confirm = dialoguer::Confirm::new()
        .with_prompt("Continue?")
        .default(false)
        .interact()
        .context("Failed to read confirmation")?;

    if !confirm {
        println!("Aborted.");
        return Ok(());
    }

    info("Seeding database...");

    // Generate password hash for all test users
    let password = "TestPassword123!";
    let salt = argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2 = argon2::Argon2::default();
    let password_hash = argon2::PasswordHasher::hash_password(&argon2, password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?
        .to_string();

    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    for i in 1..=user_count {
        let username = format!("testuser{i}");
        let email = format!("testuser{i}@example.com");
        let display_name = format!("Test User {i}");

        // Create user
        let user: Option<(uuid::Uuid,)> = sqlx::query_as(
            r"
            INSERT INTO users (username, email, password_hash, status, email_verified)
            VALUES ($1, $2, $3, 'active', TRUE)
            ON CONFLICT (email) DO NOTHING
            RETURNING id
            ",
        )
        .bind(&username)
        .bind(&email)
        .bind(&password_hash)
        .fetch_optional(&mut *tx)
        .await
        .context("Failed to create user")?;

        if let Some((user_id,)) = user {
            // Create player
            sqlx::query(
                r"
                INSERT INTO players (user_id, display_name, country_code)
                VALUES ($1, $2, 'US')
                ",
            )
            .bind(user_id)
            .bind(&display_name)
            .execute(&mut *tx)
            .await
            .context("Failed to create player")?;
        }
    }

    tx.commit().await.context("Failed to commit transaction")?;

    success(&format!("Created {user_count} test users"));
    println!("Password for all test users: {password}");

    Ok(())
}

async fn clear_database(pool: &PgPool, confirm: &str) -> Result<()> {
    if confirm != "yes-delete-everything" {
        eprintln!("ERROR: You must confirm with --confirm yes-delete-everything");
        std::process::exit(1);
    }

    warn("This will DELETE ALL DATA from the database!");

    let double_confirm = dialoguer::Confirm::new()
        .with_prompt("Are you absolutely sure? This cannot be undone!")
        .default(false)
        .interact()
        .context("Failed to read confirmation")?;

    if !double_confirm {
        println!("Aborted.");
        return Ok(());
    }

    info("Clearing database...");

    // Order matters due to foreign keys
    let tables = [
        "audit_logs",
        "saga_steps",
        "sagas",
        "substitute_assignments",
        "substitute_responses",
        "substitute_requests",
        "substitute_availability",
        "substitute_pool",
        "match_maps",
        "match_players",
        "matches",
        "lobby_chat",
        "lobby_players",
        "lobbies",
        "queue_entries",
        "queues",
        "tournament_bracket_matches",
        "tournament_brackets",
        "tournament_participants",
        "tournaments",
        "season_standings",
        "season_participants",
        "seasons",
        // League team system (new)
        "league_team_invitations",
        "league_team_members",
        "league_team_seasons",
        "league_season_participants",
        "league_teams",
        "league_seasons",
        "league_invitations",
        "league_members",
        "leagues",
        "player_game_profiles",
        "player_relationships",
        "players",
        "bans",
        "password_reset_tokens",
        "user_sessions",
        "refresh_tokens",
        "user_roles",
        "oauth_connections",
        "users",
    ];

    for table in tables {
        let query = format!("TRUNCATE TABLE {table} CASCADE");
        match sqlx::query(&query).execute(pool).await {
            Ok(_) => println!("  Cleared: {table}"),
            Err(e) => println!("  Skipped: {table} ({e})"),
        }
    }

    success("Database cleared");
    Ok(())
}
