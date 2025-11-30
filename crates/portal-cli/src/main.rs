#![allow(missing_docs)]
//! Portal CLI - Administration tool for the Gaming Portal.
//!
//! This CLI provides direct database access for administrative operations
//! including user management, RBAC, and more.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod config;
mod output;

use commands::{db, game, player, role, user};
use config::CliConfig;
use output::OutputFormat;

/// Portal CLI - Gaming Portal Administration Tool
#[derive(Parser)]
#[command(name = "portal")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Database connection URL
    #[arg(long, env = "DATABASE_URL", global = true)]
    database_url: Option<String>,

    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Output format
    #[arg(short, long, default_value = "table", global = true)]
    format: OutputFormat,

    /// Verbose output (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// User management commands
    User(user::UserCommand),

    /// Role and permission management
    Role(role::RoleCommand),

    /// Player profile management
    Player(player::PlayerCommand),

    /// Game configuration
    Game(game::GameCommand),

    /// Database utilities
    Db(db::DbCommand),
    // TODO: Add LeagueTeam command once the league team system is fully implemented
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    // Initialize tracing
    init_tracing(cli.verbose);

    // Load configuration
    let config = CliConfig::load(cli.config.as_deref(), cli.database_url.as_deref())?;

    // Connect to database
    let pool = sqlx::PgPool::connect(&config.database_url).await?;

    // Execute command
    match cli.command {
        Commands::User(cmd) => cmd.execute(&pool, cli.format).await?,
        Commands::Role(cmd) => cmd.execute(&pool, cli.format).await?,
        Commands::Player(cmd) => cmd.execute(&pool, cli.format).await?,
        Commands::Game(cmd) => cmd.execute(&pool, cli.format).await?,
        Commands::Db(cmd) => cmd.execute(&pool, cli.format).await?,
    }

    Ok(())
}

fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
}
