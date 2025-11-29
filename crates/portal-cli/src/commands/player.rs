//! Player management commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_core::PlayerId;
use portal_db::entities::{NewPlayer, UpdatePlayer};
use portal_db::repositories::{PlayerGameProfileRepository, PlayerRepository};
use portal_db::PgPool;
use uuid::Uuid;

use crate::output::{
    error, format_optional, format_timestamp, format_uuid, output_list, success, OutputFormat,
    PlayerTableRow,
};

/// Player management commands.
#[derive(Args)]
pub struct PlayerCommand {
    #[command(subcommand)]
    command: PlayerSubcommand,
}

#[derive(Subcommand)]
enum PlayerSubcommand {
    /// List players
    List {
        /// Search by display name
        #[arg(long)]
        search: Option<String>,
        /// Filter by country code
        #[arg(long)]
        country: Option<String>,
        /// Maximum results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get player details
    Get {
        /// Player ID
        id: Uuid,
    },

    /// Create a player for a user
    Create {
        /// User ID
        #[arg(long)]
        user_id: Uuid,
        /// Display name
        #[arg(long)]
        display_name: String,
        /// Country code
        #[arg(long)]
        country: Option<String>,
    },

    /// Update player profile
    Update {
        /// Player ID
        id: Uuid,
        /// Display name
        #[arg(long)]
        display_name: Option<String>,
        /// Bio
        #[arg(long)]
        bio: Option<String>,
        /// Country code
        #[arg(long)]
        country: Option<String>,
    },

    /// Show player statistics
    Stats {
        /// Player ID
        id: Uuid,
        /// Game ID
        #[arg(long)]
        game: Option<String>,
    },

    /// Reset player rating
    ResetRating {
        /// Player ID
        id: Uuid,
        /// Game ID
        game: String,
    },
}

impl PlayerCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let player_repo = PlayerRepository::new(pool.clone());
        let profile_repo = PlayerGameProfileRepository::new(pool.clone());

        match &self.command {
            PlayerSubcommand::List {
                search,
                country,
                limit,
            } => {
                list_players(
                    &player_repo,
                    search.as_deref(),
                    country.as_deref(),
                    *limit,
                    format,
                )
                .await
            }
            PlayerSubcommand::Get { id } => get_player(&player_repo, *id, format).await,
            PlayerSubcommand::Create {
                user_id,
                display_name,
                country,
            } => create_player(&player_repo, *user_id, display_name, country.as_deref()).await,
            PlayerSubcommand::Update {
                id,
                display_name,
                bio,
                country,
            } => {
                update_player(
                    &player_repo,
                    *id,
                    display_name.as_deref(),
                    bio.as_deref(),
                    country.as_deref(),
                )
                .await
            }
            PlayerSubcommand::Stats { id, game } => {
                show_stats(&profile_repo, *id, game.as_deref(), format).await
            }
            PlayerSubcommand::ResetRating { id, game } => {
                reset_rating(&profile_repo, *id, game).await
            }
        }
    }
}

async fn list_players(
    repo: &PlayerRepository,
    search: Option<&str>,
    country: Option<&str>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let players = repo
        .list(search, country, limit, 0)
        .await
        .context("Failed to fetch players")?;

    let rows: Vec<PlayerTableRow> = players
        .into_iter()
        .map(|p| PlayerTableRow {
            id: format_uuid(&p.id),
            display_name: p.display_name,
            country: format_optional(&p.country_code),
            steam_id: format_optional(&p.steam_id),
            created_at: format_timestamp(&p.created_at),
        })
        .collect();

    output_list(&rows, format)
}

async fn get_player(repo: &PlayerRepository, id: Uuid, format: OutputFormat) -> Result<()> {
    let player_id = PlayerId::from_uuid(id);
    let player = repo
        .find_by_id(player_id)
        .await
        .context("Failed to fetch player")?;

    match player {
        Some(p) => {
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "id": p.id,
                            "user_id": p.user_id,
                            "display_name": p.display_name,
                            "country_code": p.country_code,
                            "region": p.region,
                            "timezone": p.timezone,
                            "bio": p.bio,
                            "steam_id": p.steam_id,
                            "created_at": p.created_at,
                        }))?
                    );
                }
                _ => {
                    println!("Player: {}", p.display_name);
                    println!("  ID:        {}", p.id);
                    println!("  User ID:   {}", p.user_id);
                    println!("  Country:   {}", format_optional(&p.country_code));
                    println!("  Region:    {}", format_optional(&p.region));
                    println!("  Steam ID:  {}", format_optional(&p.steam_id));
                    println!("  Bio:       {}", format_optional(&p.bio));
                    println!("  Created:   {}", format_timestamp(&p.created_at));
                }
            }
            Ok(())
        }
        None => {
            error(&format!("Player not found: {id}"));
            std::process::exit(1);
        }
    }
}

async fn create_player(
    repo: &PlayerRepository,
    user_id: Uuid,
    display_name: &str,
    country: Option<&str>,
) -> Result<()> {
    let new_player = NewPlayer {
        user_id,
        display_name: display_name.to_string(),
        avatar_url: None,
        country_code: country.map(String::from),
    };

    let player = repo
        .create(new_player)
        .await
        .context("Failed to create player")?;

    success(&format!("Created player: {} ({})", display_name, player.id));
    Ok(())
}

async fn update_player(
    repo: &PlayerRepository,
    id: Uuid,
    display_name: Option<&str>,
    bio: Option<&str>,
    country: Option<&str>,
) -> Result<()> {
    let player_id = PlayerId::from_uuid(id);
    let update = UpdatePlayer {
        display_name: display_name.map(String::from),
        bio: bio.map(String::from),
        country_code: country.map(String::from),
        ..Default::default()
    };

    let player = repo
        .update(player_id, update)
        .await
        .context("Failed to update player")?;

    success(&format!("Updated player: {}", player.id));
    Ok(())
}

async fn show_stats(
    repo: &PlayerGameProfileRepository,
    id: Uuid,
    game: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let player_id = PlayerId::from_uuid(id);

    let profiles = if let Some(game_id) = game {
        // Get specific game profile
        match repo
            .find_by_player_and_game(player_id, game_id)
            .await
            .context("Failed to fetch stats")?
        {
            Some(p) => vec![p],
            None => vec![],
        }
    } else {
        // Get all profiles
        repo.list_by_player(player_id)
            .await
            .context("Failed to fetch stats")?
    };

    if profiles.is_empty() {
        println!("No game profiles found for player.");
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &profiles
                        .iter()
                        .map(|p| serde_json::json!({
                            "game_id": p.game_id,
                            "rating": p.rating,
                            "rating_deviation": p.rating_deviation,
                            "matches_played": p.matches_played,
                            "wins": p.wins,
                            "losses": p.losses,
                            "win_streak": p.win_streak,
                            "best_win_streak": p.best_win_streak,
                            "rank_tier": p.rank_tier,
                            "total_playtime_minutes": p.total_playtime_minutes
                        }))
                        .collect::<Vec<_>>()
                )?
            );
        }
        _ => {
            for p in &profiles {
                let win_rate = if p.matches_played > 0 {
                    (f64::from(p.wins) / f64::from(p.matches_played)) * 100.0
                } else {
                    0.0
                };

                println!("\n[{}]", p.game_id);
                println!("  Rating:      {} (±{})", p.rating, p.rating_deviation);
                println!("  Rank:        {}", format_optional(&p.rank_tier));
                println!("  Matches:     {}", p.matches_played);
                println!(
                    "  W/L:         {}/{} ({:.1}%)",
                    p.wins, p.losses, win_rate
                );
                println!(
                    "  Win Streak:  {} (best: {})",
                    p.win_streak, p.best_win_streak
                );
                println!("  Playtime:    {} hours", p.total_playtime_minutes / 60);
            }
        }
    }
    Ok(())
}

async fn reset_rating(repo: &PlayerGameProfileRepository, id: Uuid, game: &str) -> Result<()> {
    let player_id = PlayerId::from_uuid(id);

    repo.reset_rating(player_id, game)
        .await
        .context("Failed to reset rating")?;

    success(&format!("Reset rating for player {id} in game {game}"));
    Ok(())
}
