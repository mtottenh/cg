//! Scanner commands for automated demo ingestion.
//!
//! Scans S3 buckets for new demo files, catalogs them via the Portal API,
//! and fetches stats from an external stats service.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

/// Scanner commands for S3 demo ingestion.
#[derive(Args)]
pub struct ScanCommand {
    #[command(subcommand)]
    command: ScanSubcommand,
}

#[derive(Subcommand)]
enum ScanSubcommand {
    /// Scan S3 bucket for new demos, catalog them, fetch stats
    Run {
        /// S3 bucket to scan
        #[arg(long, env = "SCANNER_S3_BUCKET")]
        bucket: String,
        /// S3 key prefix to scan within
        #[arg(long, env = "SCANNER_S3_PREFIX", default_value = "")]
        prefix: String,
        /// Game ID for cataloged demos
        #[arg(long, env = "SCANNER_GAME_ID")]
        game_id: String,
        /// Portal API base URL
        #[arg(long, env = "PORTAL_API_URL", default_value = "http://localhost:3000")]
        api_url: String,
        /// Portal API auth token
        #[arg(long, env = "PORTAL_API_TOKEN")]
        api_token: String,
        /// External stats service URL
        #[arg(long, env = "CS2_DEMO_SERVICE_URL", default_value = "https://demos.cs210mans.uk")]
        stats_url: String,
        /// File extension to match
        #[arg(long, default_value = ".dem")]
        extension: String,
        /// Number of demos per batch catalog request
        #[arg(long, default_value = "100")]
        batch_size: usize,
        /// Print what would be done without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Process stats for pending demos
    ProcessStats {
        /// Portal API base URL
        #[arg(long, env = "PORTAL_API_URL", default_value = "http://localhost:3000")]
        api_url: String,
        /// Portal API auth token
        #[arg(long, env = "PORTAL_API_TOKEN")]
        api_token: String,
        /// External stats service URL
        #[arg(long, env = "CS2_DEMO_SERVICE_URL")]
        stats_url: String,
        /// Maximum number of pending demos to process
        #[arg(long, default_value = "50")]
        limit: i64,
    },
}

impl ScanCommand {
    pub async fn execute(&self) -> Result<()> {
        match &self.command {
            ScanSubcommand::Run {
                bucket,
                prefix,
                game_id,
                api_url,
                api_token,
                stats_url,
                extension,
                batch_size,
                dry_run,
            } => {
                run_scan(
                    bucket, prefix, game_id, api_url, api_token, stats_url, extension,
                    *batch_size, *dry_run,
                )
                .await
            }
            ScanSubcommand::ProcessStats {
                api_url,
                api_token,
                stats_url,
                limit,
            } => process_stats(api_url, api_token, stats_url, *limit).await,
        }
    }
}

// =============================================================================
// API CLIENT
// =============================================================================

struct PortalApiClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

/// Batch catalog API response.
type BatchCatalogResult = Result<BatchCatalogResponse>;

impl PortalApiClient {
    fn new(base_url: &str, token: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    async fn batch_catalog(
        &self,
        game_id: &str,
        demos: &[BatchEntry],
    ) -> BatchCatalogResult {
        let body = serde_json::json!({
            "game_id": game_id,
            "demos": demos,
        });

        let url = format!("{}/v1/admin/demos/batch", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to call batch catalog")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Batch catalog failed ({status}): {text}");
        }

        let wrapper: serde_json::Value = resp.json().await.context("Failed to parse response")?;
        let data = wrapper
            .get("data")
            .ok_or_else(|| anyhow::anyhow!("Missing data field"))?;

        serde_json::from_value(data.clone()).context("Failed to parse batch result")
    }

    async fn submit_stats(&self, demo_id: &str, stats: &serde_json::Value) -> Result<()> {
        let url = format!("{}/v1/admin/demos/{demo_id}/stats", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(stats)
            .send()
            .await
            .context("Failed to submit stats")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Submit stats failed ({status}): {text}");
        }

        Ok(())
    }

    async fn mark_failed(&self, demo_id: &str, error: &str) -> Result<()> {
        let url = format!("{}/v1/admin/demos/{demo_id}/stats-failed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "error": error }))
            .send()
            .await
            .context("Failed to mark demo as failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Mark failed request failed ({status}): {text}");
        }

        Ok(())
    }

    async fn get_pending(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let url = format!("{}/v1/admin/demos/pending?limit={limit}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("Failed to fetch pending demos")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Get pending failed ({status}): {text}");
        }

        let wrapper: serde_json::Value = resp.json().await.context("Failed to parse response")?;
        let data = wrapper
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(data)
    }
}

// =============================================================================
// DATA TYPES
// =============================================================================

#[derive(serde::Serialize)]
struct BatchEntry {
    file_name: String,
    s3_bucket: String,
    s3_key: String,
    file_size_bytes: Option<i64>,
}

#[derive(serde::Deserialize, Debug)]
struct BatchCatalogResponse {
    created: Vec<DemoCatalogEntry>,
    existing: Vec<DemoCatalogEntry>,
    errors: Vec<BatchError>,
}

#[derive(serde::Deserialize, Debug)]
struct DemoCatalogEntry {
    id: String,
    file_name: String,
}

#[derive(serde::Deserialize, Debug)]
struct BatchError {
    s3_key: String,
    error: String,
}

// =============================================================================
// SCANNER FLOW
// =============================================================================

#[allow(clippy::too_many_arguments)]
async fn run_scan(
    bucket: &str,
    prefix: &str,
    game_id: &str,
    api_url: &str,
    api_token: &str,
    stats_url: &str,
    extension: &str,
    batch_size: usize,
    dry_run: bool,
) -> Result<()> {
    println!("Scanning s3://{bucket}/{prefix}");
    println!("  Game ID:    {game_id}");
    println!("  Extension:  {extension}");
    println!("  Batch size: {batch_size}");
    if dry_run {
        println!("  DRY RUN MODE");
    }

    // Initialize S3 client
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3_client = aws_sdk_s3::Client::new(&config);

    // List all objects
    let mut all_keys: Vec<(String, i64)> = Vec::new();
    let mut continuation_token: Option<String> = None;

    loop {
        let mut req = s3_client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix);

        if let Some(token) = &continuation_token {
            req = req.continuation_token(token);
        }

        let output = req.send().await.context("Failed to list S3 objects")?;

        for obj in output.contents() {
            if let Some(key) = obj.key() {
                if key.ends_with(extension) {
                    let size = obj.size().unwrap_or(0);
                    all_keys.push((key.to_string(), size));
                }
            }
        }

        if output.is_truncated() == Some(true) {
            continuation_token = output.next_continuation_token().map(String::from);
        } else {
            break;
        }
    }

    println!("Found {} matching files", all_keys.len());

    if dry_run {
        for (key, size) in &all_keys {
            println!("  {key} ({size} bytes)");
        }
        return Ok(());
    }

    let api = PortalApiClient::new(api_url, api_token);
    let stats_base = stats_url.trim_end_matches('/');

    let mut total_created = 0usize;
    let mut total_existing = 0usize;
    let mut total_errors = 0usize;
    let mut stats_ok = 0usize;
    let mut stats_failed = 0usize;

    // Process in batches
    for chunk in all_keys.chunks(batch_size) {
        let entries: Vec<BatchEntry> = chunk
            .iter()
            .map(|(key, size)| {
                let file_name = key
                    .rsplit('/')
                    .next()
                    .unwrap_or(key)
                    .to_string();
                BatchEntry {
                    file_name,
                    s3_bucket: bucket.to_string(),
                    s3_key: key.clone(),
                    file_size_bytes: Some(*size),
                }
            })
            .collect();

        let result = api.batch_catalog(game_id, &entries).await?;

        total_created += result.created.len();
        total_existing += result.existing.len();
        total_errors += result.errors.len();

        for err in &result.errors {
            eprintln!("  ERROR cataloging {}: {}", err.s3_key, err.error);
        }

        // For newly created demos, fetch and submit stats
        for demo in &result.created {
            let demo_name = &demo.file_name;
            let stats_file = format!("{demo_name}.stats.json");
            let stats_fetch_url = format!("{stats_base}/stats/{stats_file}");

            match fetch_and_submit_stats(&api, &demo.id, &stats_fetch_url).await {
                Ok(()) => {
                    stats_ok += 1;
                }
                Err(e) => {
                    let error_msg = format!("{e:#}");
                    eprintln!("  WARN stats for {demo_name}: {error_msg}");
                    if let Err(mark_err) = api.mark_failed(&demo.id, &error_msg).await {
                        eprintln!("  ERROR marking failed: {mark_err}");
                    }
                    stats_failed += 1;
                }
            }
        }
    }

    println!();
    println!("Summary:");
    println!("  Scanned:      {}", all_keys.len());
    println!("  New:          {total_created}");
    println!("  Existing:     {total_existing}");
    println!("  Catalog errs: {total_errors}");
    println!("  Stats OK:     {stats_ok}");
    println!("  Stats failed: {stats_failed}");

    Ok(())
}

async fn process_stats(
    api_url: &str,
    api_token: &str,
    stats_url: &str,
    limit: i64,
) -> Result<()> {
    let api = PortalApiClient::new(api_url, api_token);
    let stats_base = stats_url.trim_end_matches('/');

    let pending = api.get_pending(limit).await?;
    println!("Found {} pending demos", pending.len());

    let mut stats_ok = 0usize;
    let mut stats_failed = 0usize;

    for demo in &pending {
        let demo_id = demo["id"].as_str().unwrap_or_default();
        let file_name = demo["file_name"].as_str().unwrap_or_default();
        let stats_file = format!("{file_name}.stats.json");
        let stats_fetch_url = format!("{stats_base}/stats/{stats_file}");

        match fetch_and_submit_stats(&api, demo_id, &stats_fetch_url).await {
            Ok(()) => {
                println!("  OK: {file_name}");
                stats_ok += 1;
            }
            Err(e) => {
                let error_msg = format!("{e:#}");
                eprintln!("  FAIL: {file_name} - {error_msg}");
                if let Err(mark_err) = api.mark_failed(demo_id, &error_msg).await {
                    eprintln!("  ERROR marking failed: {mark_err}");
                }
                stats_failed += 1;
            }
        }
    }

    println!();
    println!("Summary: {stats_ok} ok, {stats_failed} failed");
    Ok(())
}

/// Fetch stats from the external service and convert to the API format, then submit.
async fn fetch_and_submit_stats(
    api: &PortalApiClient,
    demo_id: &str,
    stats_url: &str,
) -> Result<()> {
    // Fetch raw stats JSON from external service
    let resp = api
        .client
        .get(stats_url)
        .send()
        .await
        .context("Failed to fetch stats")?;

    if !resp.status().is_success() {
        let status = resp.status();
        anyhow::bail!("Stats service returned {status}");
    }

    let raw_stats: serde_json::Value = resp.json().await.context("Failed to parse stats JSON")?;

    // Convert Cs2DemoStats format to generic SubmitDemoStatsRequest
    let stats_body = convert_cs2_stats_to_request(&raw_stats);

    api.submit_stats(demo_id, &stats_body).await
}

/// Convert CS2 demo stats JSON into the generic SubmitDemoStatsRequest format.
fn convert_cs2_stats_to_request(stats: &serde_json::Value) -> serde_json::Value {
    // Extract team names
    let teams = stats.get("teams").and_then(|t| t.as_array());
    let (team1_name, team2_name, team1_score, team2_score) = match teams {
        Some(t) if t.len() >= 2 => (
            t[0]["team_name"].as_str().unwrap_or("Team 1"),
            t[1]["team_name"].as_str().unwrap_or("Team 2"),
            t[0]["score"].as_i64().unwrap_or(0),
            t[1]["score"].as_i64().unwrap_or(0),
        ),
        _ => ("Team 1", "Team 2", 0, 0),
    };

    // Build players array
    let mut players = Vec::new();
    if let Some(teams_arr) = teams {
        for team in teams_arr {
            let team_name = team["team_name"].as_str().unwrap_or_default();
            if let Some(team_players) = team.get("players").and_then(|p| p.as_array()) {
                for player in team_players {
                    // Extract steam_id (may be numeric or string)
                    let steam_id_str = player
                        .get("player_id")
                        .or_else(|| player.get("steam_id"))
                        .map(|v| match v {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();

                    let player_name = player["player_name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();

                    let player_stats = serde_json::json!({
                        "kills": player.get("kills").and_then(serde_json::Value::as_i64).unwrap_or(0),
                        "deaths": player.get("deaths").and_then(serde_json::Value::as_i64).unwrap_or(0),
                        "assists": player.get("assists").and_then(serde_json::Value::as_i64).unwrap_or(0),
                        "damage": player.get("damage").and_then(serde_json::Value::as_i64).unwrap_or(0),
                        "adr": player.get("adr").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                        "headshot_kills": player.get("headshot_kills").and_then(serde_json::Value::as_i64).unwrap_or(0),
                        "hs_percentage": player.get("hs_percentage").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                    });

                    players.push(serde_json::json!({
                        "steam_id": steam_id_str,
                        "player_name": player_name,
                        "team_name": team_name,
                        "stats": player_stats,
                    }));
                }
            }
        }
    }

    // Calculate total rounds
    let total_rounds = team1_score + team2_score;

    serde_json::json!({
        "map_name": stats.get("map").and_then(serde_json::Value::as_str),
        "match_date": stats.get("match_date").and_then(serde_json::Value::as_str),
        "duration_seconds": stats.get("duration_seconds").and_then(serde_json::Value::as_i64),
        "team1_name": team1_name,
        "team2_name": team2_name,
        "team1_score": team1_score,
        "team2_score": team2_score,
        "total_rounds": total_rounds,
        "raw_stats": stats,
        "players": players,
    })
}
