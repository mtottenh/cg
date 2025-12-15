//! Demo catalog commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_core::DemoCategory;
use portal_db::PgPool;
use serde::Serialize;
use tabled::Tabled;

use crate::output::{error, format_optional, format_timestamp, format_uuid, info, output_list, success, OutputFormat};

/// Demo management commands.
#[derive(Args)]
pub struct DemoCommand {
    #[command(subcommand)]
    command: DemoSubcommand,
}

#[derive(Subcommand)]
enum DemoSubcommand {
    /// List demos with optional filtering
    List {
        /// Filter by game ID
        #[arg(long)]
        game: Option<String>,
        /// Filter by category (uncategorized, pug, league, scrim, ignored)
        #[arg(long)]
        category: Option<String>,
        /// Filter by status (pending, processing, ready, failed, archived)
        #[arg(long)]
        status: Option<String>,
        /// Filter by map name
        #[arg(long)]
        map: Option<String>,
        /// Filter by Steam ID
        #[arg(long)]
        steam_id: Option<String>,
        /// Include hidden demos
        #[arg(long)]
        include_hidden: bool,
        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get demo details
    Get {
        /// Demo ID
        id: String,
    },

    /// Get demo players
    Players {
        /// Demo ID
        id: String,
    },

    /// Show demo status counts
    Stats,

    /// List pending demos for processing
    Pending {
        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Categorize a demo
    Categorize {
        /// Demo ID
        id: String,
        /// Category (uncategorized, pug, league, scrim, ignored)
        #[arg(long)]
        category: String,
    },

    /// Hide a demo
    Hide {
        /// Demo ID
        id: String,
    },

    /// Unhide a demo
    Unhide {
        /// Demo ID
        id: String,
    },
}

impl DemoCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            DemoSubcommand::List {
                game,
                category,
                status,
                map,
                steam_id,
                include_hidden,
                limit,
            } => {
                list_demos(
                    pool,
                    game.as_deref(),
                    category.as_deref(),
                    status.as_deref(),
                    map.as_deref(),
                    steam_id.as_deref(),
                    *include_hidden,
                    *limit,
                    format,
                )
                .await
            }
            DemoSubcommand::Get { id } => get_demo(pool, id, format).await,
            DemoSubcommand::Players { id } => get_demo_players(pool, id, format).await,
            DemoSubcommand::Stats => show_stats(pool, format).await,
            DemoSubcommand::Pending { limit } => list_pending(pool, *limit, format).await,
            DemoSubcommand::Categorize { id, category } => {
                categorize_demo(pool, id, category).await
            }
            DemoSubcommand::Hide { id } => hide_demo(pool, id).await,
            DemoSubcommand::Unhide { id } => unhide_demo(pool, id).await,
        }
    }
}

/// Table row for demo display.
#[derive(Tabled, Serialize)]
pub struct DemoTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "File")]
    pub file_name: String,
    #[tabled(rename = "Map")]
    pub map: String,
    #[tabled(rename = "Category")]
    pub category: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Hidden")]
    pub hidden: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

/// Table row for demo player display.
#[derive(Tabled, Serialize)]
pub struct DemoPlayerTableRow {
    #[tabled(rename = "Steam ID")]
    pub steam_id: String,
    #[tabled(rename = "Name")]
    pub player_name: String,
    #[tabled(rename = "Team")]
    pub team: String,
    #[tabled(rename = "K")]
    pub kills: i32,
    #[tabled(rename = "D")]
    pub deaths: i32,
    #[tabled(rename = "A")]
    pub assists: i32,
    #[tabled(rename = "ADR")]
    pub adr: String,
    #[tabled(rename = "HS%")]
    pub hs_pct: String,
}

/// Table row for demo stats display.
#[derive(Tabled, Serialize)]
pub struct DemoStatsRow {
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Count")]
    pub count: i64,
}

#[allow(clippy::too_many_arguments)]
async fn list_demos(
    pool: &PgPool,
    game: Option<&str>,
    category: Option<&str>,
    status: Option<&str>,
    map: Option<&str>,
    steam_id: Option<&str>,
    include_hidden: bool,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    // Build and execute query
    let mut query = String::from(
        r#"
        SELECT d.id, d.file_name, d.category, d.status, d.is_hidden, d.created_at,
               d.map_name
        FROM demos d
        WHERE 1=1
        "#,
    );

    if game.is_some() {
        query.push_str(" AND d.game_id = $1::uuid");
    }
    if let Some(cat) = category {
        query.push_str(&format!(" AND d.category = '{cat}'"));
    }
    if let Some(s) = status {
        query.push_str(&format!(" AND d.status = '{s}'"));
    }
    if let Some(m) = map {
        query.push_str(&format!(" AND d.map_name ILIKE '%{m}%'"));
    }
    if let Some(sid) = steam_id {
        query.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM demo_players dp WHERE dp.demo_id = d.id AND dp.steam_id = '{sid}')"
        ));
    }
    if !include_hidden {
        query.push_str(" AND d.is_hidden = false");
    }
    query.push_str(&format!(" ORDER BY d.created_at DESC LIMIT {limit}"));

    let rows: Vec<(uuid::Uuid, String, String, String, bool, chrono::DateTime<chrono::Utc>, Option<String>)> = if let Some(g) = game {
        let game_uuid: uuid::Uuid = g.parse().context("Invalid game ID")?;
        sqlx::query_as(&query)
            .bind(game_uuid)
            .fetch_all(pool)
            .await
            .context("Failed to fetch demos")?
    } else {
        sqlx::query_as(&query)
            .fetch_all(pool)
            .await
            .context("Failed to fetch demos")?
    };

    let table_rows: Vec<DemoTableRow> = rows
        .into_iter()
        .map(|(id, file_name, cat, stat, hidden, created, map_name)| DemoTableRow {
            id: format_uuid(&id),
            file_name: if file_name.len() > 40 {
                format!("{}...", &file_name[..37])
            } else {
                file_name
            },
            map: map_name.unwrap_or_else(|| "-".to_string()),
            category: cat,
            status: stat,
            hidden: if hidden { "Yes" } else { "No" }.to_string(),
            created_at: format_timestamp(&created),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn get_demo(pool: &PgPool, id: &str, format: OutputFormat) -> Result<()> {
    let demo_id: uuid::Uuid = id.parse().context("Invalid demo ID")?;

    // Query basic info (max 16 columns for sqlx tuples)
    let basic: Option<(
        uuid::Uuid,
        String,
        String,
        String,
        Option<i64>,
        String,
        String,
        bool,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"
        SELECT id, file_name, s3_bucket, s3_key, file_size_bytes,
               category, status, is_hidden, created_at
        FROM demos
        WHERE id = $1
        "#,
    )
    .bind(demo_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch demo")?;

    // Query match info separately
    let match_info: Option<(Option<String>, Option<String>, Option<String>, Option<i32>, Option<i32>)> =
        sqlx::query_as(
            r#"
            SELECT map_name, team1_name, team2_name, team1_score, team2_score
            FROM demos
            WHERE id = $1
            "#,
        )
        .bind(demo_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch demo match info")?;

    if let Some(d) = basic {
        let mi = match_info.unwrap_or((None, None, None, None, None));

        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": d.0,
                    "file_name": d.1,
                    "s3_bucket": d.2,
                    "s3_key": d.3,
                    "file_size_bytes": d.4,
                    "category": d.5,
                    "status": d.6,
                    "is_hidden": d.7,
                    "created_at": d.8,
                    "map_name": mi.0,
                    "team1_name": mi.1,
                    "team2_name": mi.2,
                    "team1_score": mi.3,
                    "team2_score": mi.4,
                }))?
            );
        } else {
            println!("Demo: {}", d.0);
            println!("  File:       {}", d.1);
            println!("  S3:         s3://{}/{}", d.2, d.3);
            println!("  Size:       {} bytes", format_optional(&d.4));
            println!("  Category:   {}", d.5);
            println!("  Status:     {}", d.6);
            println!("  Hidden:     {}", if d.7 { "Yes" } else { "No" });
            if let Some(map) = &mi.0 {
                println!("  Map:        {}", map);
            }
            if let (Some(t1), Some(t2)) = (&mi.1, &mi.2) {
                println!(
                    "  Match:      {} vs {} ({}-{})",
                    t1,
                    t2,
                    mi.3.unwrap_or(0),
                    mi.4.unwrap_or(0)
                );
            }
            println!("  Created:    {}", format_timestamp(&d.8));
        }
        Ok(())
    } else {
        error(&format!("Demo not found: {id}"));
        std::process::exit(1);
    }
}

async fn get_demo_players(pool: &PgPool, id: &str, format: OutputFormat) -> Result<()> {
    let demo_id: uuid::Uuid = id.parse().context("Invalid demo ID")?;

    let rows: Vec<(String, String, Option<String>, i32, i32, i32, f64, f64)> = sqlx::query_as(
        r#"
        SELECT steam_id, player_name, team_name, kills, deaths, assists, adr, hs_percentage
        FROM demo_players
        WHERE demo_id = $1
        ORDER BY kills DESC
        "#,
    )
    .bind(demo_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch demo players")?;

    if rows.is_empty() {
        info("No players found for this demo.");
        return Ok(());
    }

    let table_rows: Vec<DemoPlayerTableRow> = rows
        .into_iter()
        .map(|(steam_id, name, team, k, d, a, adr, hs)| DemoPlayerTableRow {
            steam_id,
            player_name: name,
            team: team.unwrap_or_else(|| "-".to_string()),
            kills: k,
            deaths: d,
            assists: a,
            adr: format!("{:.1}", adr),
            hs_pct: format!("{:.1}%", hs),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn show_stats(pool: &PgPool, format: OutputFormat) -> Result<()> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT status, COUNT(*) as count
        FROM demos
        GROUP BY status
        ORDER BY
            CASE status
                WHEN 'pending' THEN 1
                WHEN 'processing' THEN 2
                WHEN 'ready' THEN 3
                WHEN 'failed' THEN 4
                WHEN 'archived' THEN 5
                ELSE 6
            END
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch demo stats")?;

    let table_rows: Vec<DemoStatsRow> = rows
        .into_iter()
        .map(|(status, count)| DemoStatsRow { status, count })
        .collect();

    output_list(&table_rows, format)
}

async fn list_pending(pool: &PgPool, limit: i64, format: OutputFormat) -> Result<()> {
    let rows: Vec<(uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT id, file_name, s3_key, created_at
        FROM demos
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending demos")?;

    let table_rows: Vec<DemoTableRow> = rows
        .into_iter()
        .map(|(id, file_name, _s3_key, created)| DemoTableRow {
            id: format_uuid(&id),
            file_name: if file_name.len() > 40 {
                format!("{}...", &file_name[..37])
            } else {
                file_name
            },
            map: "-".to_string(),
            category: "uncategorized".to_string(),
            status: "pending".to_string(),
            hidden: "No".to_string(),
            created_at: format_timestamp(&created),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn categorize_demo(pool: &PgPool, id: &str, category: &str) -> Result<()> {
    let demo_id: uuid::Uuid = id.parse().context("Invalid demo ID")?;

    // Validate category
    let _cat: DemoCategory = category
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid category. Use: uncategorized, pug, league, scrim, ignored"))?;

    sqlx::query(
        r#"
        UPDATE demos
        SET category = $1, categorized_at = NOW(), updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(category)
    .bind(demo_id)
    .execute(pool)
    .await
    .context("Failed to categorize demo")?;

    success(&format!("Categorized demo {id} as {category}"));
    Ok(())
}

async fn hide_demo(pool: &PgPool, id: &str) -> Result<()> {
    let demo_id: uuid::Uuid = id.parse().context("Invalid demo ID")?;

    sqlx::query(
        r#"
        UPDATE demos
        SET is_hidden = true, hidden_at = NOW(), updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(demo_id)
    .execute(pool)
    .await
    .context("Failed to hide demo")?;

    success(&format!("Hidden demo {id}"));
    Ok(())
}

async fn unhide_demo(pool: &PgPool, id: &str) -> Result<()> {
    let demo_id: uuid::Uuid = id.parse().context("Invalid demo ID")?;

    sqlx::query(
        r#"
        UPDATE demos
        SET is_hidden = false, hidden_at = NULL, hidden_by_user_id = NULL, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(demo_id)
    .execute(pool)
    .await
    .context("Failed to unhide demo")?;

    success(&format!("Unhidden demo {id}"));
    Ok(())
}
