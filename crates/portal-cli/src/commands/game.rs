//! Game configuration commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_db::entities::{NewGame, UpdateGame};
use portal_db::repositories::GameRepository;
use portal_db::PgPool;

use crate::output::{error, output_list, success, OutputFormat, GameTableRow};

/// Game management commands.
#[derive(Args)]
pub struct GameCommand {
    #[command(subcommand)]
    command: GameSubcommand,
}

#[derive(Subcommand)]
enum GameSubcommand {
    /// List all games
    List,

    /// Get game details
    Get {
        /// Game ID
        id: String,
    },

    /// Create a new game
    Create {
        /// Game ID (slug, e.g., "cs2")
        #[arg(long)]
        id: String,
        /// Display name
        #[arg(long)]
        name: String,
        /// Team size
        #[arg(long, default_value = "5")]
        team_size: i32,
    },

    /// Update game
    Update {
        /// Game ID
        id: String,
        /// Display name
        #[arg(long)]
        name: Option<String>,
        /// Status
        #[arg(long)]
        status: Option<String>,
    },

    /// Enable game
    Enable {
        /// Game ID
        id: String,
    },

    /// Disable game
    Disable {
        /// Game ID
        id: String,
    },
}

impl GameCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let repo = GameRepository::new(pool.clone());

        match &self.command {
            GameSubcommand::List => list_games(&repo, format).await,
            GameSubcommand::Get { id } => get_game(&repo, id, format).await,
            GameSubcommand::Create { id, name, team_size } => {
                create_game(&repo, id, name, *team_size).await
            }
            GameSubcommand::Update { id, name, status } => {
                update_game(&repo, id, name.as_deref(), status.as_deref()).await
            }
            GameSubcommand::Enable { id } => enable_game(&repo, id).await,
            GameSubcommand::Disable { id } => disable_game(&repo, id).await,
        }
    }
}

async fn list_games(repo: &GameRepository, format: OutputFormat) -> Result<()> {
    let games = repo.list().await.context("Failed to fetch games")?;

    let rows: Vec<GameTableRow> = games
        .into_iter()
        .map(|g| GameTableRow {
            id: g.slug,
            display_name: g.display_name,
            team_size: g.team_size_default,
            status: g.status,
        })
        .collect();

    output_list(&rows, format)
}

async fn get_game(repo: &GameRepository, slug: &str, format: OutputFormat) -> Result<()> {
    let game = repo.find_by_slug(slug).await.context("Failed to fetch game")?;

    if let Some(g) = game {
        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": g.slug,
                    "display_name": g.display_name,
                    "short_name": g.short_name,
                    "description": g.description,
                    "team_size_min": g.team_size_min,
                    "team_size_max": g.team_size_max,
                    "team_size_default": g.team_size_default,
                    "status": g.status,
                    "is_featured": g.is_featured,
                    "sort_order": g.sort_order,
                }))?
            );
        } else {
            println!("Game: {}", g.display_name);
            println!("  Slug:       {}", g.slug);
            println!(
                "  Team Size:  {} (min: {}, max: {})",
                g.team_size_default, g.team_size_min, g.team_size_max
            );
            println!("  Status:     {}", g.status);
            println!("  Featured:   {}", g.is_featured);
        }
        Ok(())
    } else {
        error(&format!("Game not found: {slug}"));
        std::process::exit(1);
    }
}

async fn create_game(repo: &GameRepository, slug: &str, name: &str, team_size: i32) -> Result<()> {
    let new_game = NewGame {
        slug: slug.to_string(),
        display_name: name.to_string(),
        short_name: None,
        description: None,
        plugin_id: slug.to_string(), // Default to same as game slug
        plugin_version: "1.0.0".to_string(),
        team_size_min: team_size,
        team_size_max: team_size,
        team_size_default: team_size,
    };

    repo.create(new_game)
        .await
        .context("Failed to create game")?;

    success(&format!("Created game: {name} ({slug})"));
    Ok(())
}

async fn update_game(
    repo: &GameRepository,
    id: &str,
    name: Option<&str>,
    status: Option<&str>,
) -> Result<()> {
    let update = UpdateGame {
        display_name: name.map(String::from),
        status: status.map(String::from),
        ..Default::default()
    };

    repo.update(id, update)
        .await
        .context("Failed to update game")?;

    success(&format!("Updated game: {id}"));
    Ok(())
}

async fn enable_game(repo: &GameRepository, id: &str) -> Result<()> {
    repo.enable(id).await.context("Failed to enable game")?;

    success(&format!("Enabled game: {id}"));
    Ok(())
}

async fn disable_game(repo: &GameRepository, id: &str) -> Result<()> {
    repo.disable(id).await.context("Failed to disable game")?;

    success(&format!("Disabled game: {id}"));
    Ok(())
}
