//! Bootstrap commands for initial system setup.
//!
//! These commands are used for first-time setup of the portal,
//! including creating the initial admin user.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_db::PgPool;
use portal_db::repositories::RoleRepository;

use crate::output::{OutputFormat, error, info, success, warn};

/// Bootstrap commands for initial setup.
#[derive(Args)]
pub struct BootstrapCommand {
    #[command(subcommand)]
    command: BootstrapSubcommand,
}

#[derive(Subcommand)]
enum BootstrapSubcommand {
    /// Create initial super_admin user
    Admin {
        /// Admin username
        #[arg(long)]
        username: String,

        /// Admin email address
        #[arg(long)]
        email: String,

        /// Admin password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,

        /// Display name for player profile (defaults to username)
        #[arg(long)]
        display_name: Option<String>,

        /// Skip check for existing admin users
        #[arg(long)]
        force: bool,
    },
}

impl BootstrapCommand {
    pub async fn execute(&self, pool: &PgPool, _format: OutputFormat) -> Result<()> {
        match &self.command {
            BootstrapSubcommand::Admin {
                username,
                email,
                password,
                display_name,
                force,
            } => {
                bootstrap_admin(
                    pool,
                    username,
                    email,
                    password.as_deref(),
                    display_name.as_deref(),
                    *force,
                )
                .await
            }
        }
    }
}

async fn bootstrap_admin(
    pool: &PgPool,
    username: &str,
    email: &str,
    password: Option<&str>,
    display_name: Option<&str>,
    force: bool,
) -> Result<()> {
    let role_repo = RoleRepository::new(pool.clone());

    // Check if any admin already exists
    if !force {
        let existing_admins = role_repo
            .get_users_with_role("super_admin")
            .await
            .context("Failed to check for existing admins")?;

        if !existing_admins.is_empty() {
            error("A super_admin user already exists.");
            info(
                "Use --force to create another admin, or use 'role assign' to grant admin to an existing user.",
            );
            std::process::exit(1);
        }
    }

    // Prompt for password if not provided
    let password = match password {
        Some(p) => p.to_string(),
        None => dialoguer::Password::new()
            .with_prompt("Admin password")
            .with_confirmation("Confirm password", "Passwords do not match")
            .interact()
            .context("Failed to read password")?,
    };

    // Validate password strength
    if password.len() < 8 {
        error("Password must be at least 8 characters long");
        std::process::exit(1);
    }

    // Hash password using argon2
    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2_hasher = argon2::Argon2::default();
    let password_hash =
        argon2::PasswordHasher::hash_password(&argon2_hasher, password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?
            .to_string();

    // Start transaction
    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    // Create user
    info("Creating admin user...");
    let user = sqlx::query_as::<_, portal_db::entities::UserRow>(
        r"
        INSERT INTO users (username, email, password_hash, status, email_verified)
        VALUES ($1, $2, $3, 'active', TRUE)
        RETURNING *
        ",
    )
    .bind(username)
    .bind(email)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to create user")?;

    // Create player profile
    info("Creating player profile...");
    let display_name = display_name.unwrap_or(username);
    let player = sqlx::query_as::<_, portal_db::entities::PlayerRow>(
        r"
        INSERT INTO players (user_id, display_name, avatar_url, country_code)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        ",
    )
    .bind(user.id)
    .bind(display_name)
    .bind(None::<String>)
    .bind(None::<String>)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to create player profile")?;

    // Get super_admin role
    let role = sqlx::query_as::<_, portal_db::entities::RoleRow>(
        "SELECT * FROM roles WHERE name = 'super_admin'",
    )
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to fetch super_admin role")?;

    let Some(role) = role else {
        error("super_admin role not found. Please run database migrations first.");
        std::process::exit(1);
    };

    // Assign super_admin role
    info("Assigning super_admin role...");
    sqlx::query(
        r"
        INSERT INTO user_roles (user_id, role_id, granted_by)
        VALUES ($1, $2, NULL)
        ",
    )
    .bind(user.id)
    .bind(role.id)
    .execute(&mut *tx)
    .await
    .context("Failed to assign super_admin role")?;

    // Commit transaction
    tx.commit().await.context("Failed to commit transaction")?;

    println!();
    success("Admin user created successfully!");
    println!();
    println!("  User ID:    {}", user.id);
    println!("  Player ID:  {}", player.id);
    println!("  Username:   {username}");
    println!("  Email:      {email}");
    println!("  Role:       super_admin");
    println!();
    warn("Please store the password securely. It cannot be recovered.");

    Ok(())
}
