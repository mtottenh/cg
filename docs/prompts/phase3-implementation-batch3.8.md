# Phase 3.8 Implementation - CS2 Demo Evidence Integration

## Context

You are implementing **Sub-Phase 3.8** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This sub-phase was previously deferred but can now be completed using an existing external demo system.

**Prerequisites**: Phase 3 Batches 1-4 are complete. The evidence system (3.7) is fully functional.

**External Demo System**:
- Demos are uploaded to S3 and served via: `https://demos.cs210mans.uk`
- Pre-parsed stats JSON available at: `https://demos.cs210mans.uk/stats/{demo_name}.stats.json`
- No local demo parsing required - stats are pre-computed externally

**Reference Files**:
- `crates/portal-domain/src/services/tournament/evidence.rs` - Existing evidence service
- `crates/portal-plugins/src/traits.rs` - Plugin traits
- `crates/portal-plugins/src/games/cs2/mod.rs` - CS2 plugin
- `docs/phase3/05-evidence-system.md` - Evidence system design

---

## Your Task

Implement CS2 demo discovery and validation using the external demo stats API.

### Goals

1. **Demo Discovery**: Find demos that match a tournament match based on timing, players, and map
2. **Stats Fetching**: Retrieve pre-parsed stats from the external API
3. **Result Validation**: Compare demo stats against claimed match results
4. **Evidence Linking**: Allow players/admins to link discovered demos to matches

---

## Implementation

### 1. Demo Stats Types

Create `crates/portal-plugins/src/games/cs2/demo_stats.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pre-parsed demo stats from the external demo service.
/// Fetched from: https://demos.cs210mans.uk/stats/{demo_name}.stats.json
///
/// Example: https://demos.cs210mans.uk/stats/2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem.stats.json
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cs2DemoStats {
    /// Schema version for forward compatibility
    pub schema_version: i32,

    /// Map name (e.g., "de_inferno")
    pub map: String,

    /// Match date as ISO 8601 string
    pub match_date: String,

    /// Demo file name
    pub demo_file: String,

    /// Unique match identifier
    pub match_id: String,

    /// Teams keyed by team name (e.g., "team_Maxymimi" -> TeamInfo)
    pub teams: HashMap<String, TeamInfo>,

    /// Final scores keyed by team name (e.g., "team_Maxymimi" -> 13)
    pub final_score: HashMap<String, i32>,

    /// Aggregated player stats keyed by Steam ID
    pub player_summaries: HashMap<String, PlayerSummary>,

    /// Round-by-round data
    pub rounds: Vec<RoundData>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamInfo {
    /// Team ID (2 for T, 3 for CT typically)
    pub team_id: i32,

    /// Team name
    pub team_name: String,

    /// Side: "T" or "CT"
    pub team_side: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundData {
    /// Round number (1-indexed)
    pub round_number: i32,

    /// Winning team name
    pub winner_team: String,

    /// Winning side ("T" or "CT")
    pub winner_side: String,

    /// Score after this round, keyed by team name
    pub round_score: HashMap<String, i32>,

    /// Player states at round start, keyed by Steam ID
    pub player_states: HashMap<String, PlayerState>,

    /// Events during the round
    pub events: Vec<RoundEvent>,

    /// Player stats for this round, keyed by Steam ID
    pub player_stats: HashMap<String, RoundPlayerStats>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerState {
    /// Steam ID (64-bit as number)
    pub player_id: u64,

    /// Player name during match
    pub player_name: String,

    /// Team affiliation
    pub team: TeamInfo,

    /// Starting money for the round
    pub starting_money: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundPlayerStats {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub damage: i32,
}

/// Aggregated player stats for the entire match.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerSummary {
    pub player_id: u64,
    pub player_name: String,
    pub team: TeamInfo,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub headshot_kills: i32,
    pub flash_assists: i32,
    pub damage_dealt: i32,
    pub utility_damage: i32,
    pub adr: f64,
    pub hs_percentage: f64,
    pub wallbangs: i32,
    pub smoke_kills: i32,
    pub blind_kills: i32,
    pub blinded_kills: i32,
    pub flash_duration: f64,
    pub enemies_flashed: i32,
    pub bomb_plants: i32,
    pub bomb_defuses: i32,
    /// Outgoing interactions keyed by target Steam ID
    pub outgoing_interactions: HashMap<String, PlayerInteraction>,
    /// Incoming interactions keyed by source Steam ID
    pub incoming_interactions: HashMap<String, PlayerInteraction>,
    /// Kills per weapon, keyed by weapon ID (as string)
    pub weapon_kills: HashMap<String, i32>,
}

/// Interaction between players (kills/assists).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerInteraction {
    #[serde(default)]
    pub killed: Option<i32>,
    #[serde(default)]
    pub assisted: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundEvent {
    pub event_type: String,
    pub event_time: f64,
    #[serde(default)]
    pub source_player_id: Option<u64>,
    #[serde(default)]
    pub target_player_id: Option<u64>,
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default)]
    pub weapon_type: Option<i32>,
    #[serde(default)]
    pub attributes: Option<serde_json::Value>,
}


impl Cs2DemoStats {
    /// Parse match_date to DateTime.
    pub fn match_datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.match_date)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Check if this demo likely matches a tournament match by timeframe.
    pub fn matches_timeframe(&self, match_start: DateTime<Utc>, tolerance_minutes: i64) -> bool {
        self.match_datetime()
            .map(|dt| (dt - match_start).num_minutes().abs() <= tolerance_minutes)
            .unwrap_or(false)
    }

    /// Get team names.
    pub fn team_names(&self) -> Vec<String> {
        self.teams.keys().cloned().collect()
    }

    /// Get score for a team by name.
    pub fn score_for_team(&self, team_name: &str) -> Option<i32> {
        self.final_score.get(team_name).copied()
    }

    /// Get all Steam IDs that participated in the match.
    pub fn all_steam_ids(&self) -> Vec<String> {
        self.player_summaries.keys().cloned().collect()
    }

    /// Get Steam IDs for a specific team.
    pub fn steam_ids_for_team(&self, team_name: &str) -> Vec<String> {
        self.player_summaries
            .iter()
            .filter(|(_, ps)| ps.team.team_name == team_name)
            .map(|(steam_id, _)| steam_id.clone())
            .collect()
    }

    /// Check if specific Steam IDs participated in the match.
    pub fn has_players(&self, steam_ids: &[String]) -> bool {
        steam_ids.iter().all(|id| self.player_summaries.contains_key(id))
    }

    /// Get a player summary by Steam ID.
    pub fn get_player(&self, steam_id: &str) -> Option<&PlayerSummary> {
        self.player_summaries.get(steam_id)
    }

    /// Get all player summaries as a vector.
    pub fn all_player_summaries(&self) -> Vec<&PlayerSummary> {
        self.player_summaries.values().collect()
    }

    /// Determine the winning team name.
    pub fn winner_team_name(&self) -> Option<String> {
        self.final_score
            .iter()
            .max_by_key(|(_, score)| *score)
            .map(|(name, _)| name.clone())
    }

    /// Get total rounds played.
    pub fn total_rounds(&self) -> i32 {
        self.rounds.len() as i32
    }
}
```

### 2. Demo Stats Client

Create `crates/portal-plugins/src/games/cs2/demo_client.rs`:

```rust
use super::demo_stats::Cs2DemoStats;
use crate::error::PluginError;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, instrument, warn};

const DEMO_BASE_URL: &str = "https://demos.cs210mans.uk";
const STATS_PATH: &str = "/stats";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Client for fetching demo stats from the external demo service.
#[derive(Debug, Clone)]
pub struct Cs2DemoClient {
    client: Client,
    base_url: String,
}

impl Default for Cs2DemoClient {
    fn default() -> Self {
        Self::new(DEMO_BASE_URL.to_string())
    }
}

impl Cs2DemoClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Fetch demo stats by demo name.
    ///
    /// # Arguments
    /// * `demo_name` - The demo file name (e.g., "match_12345.dem")
    #[instrument(skip(self))]
    pub async fn get_demo_stats(&self, demo_name: &str) -> Result<Cs2DemoStats, PluginError> {
        // Remove .dem extension if present for stats lookup
        let stats_name = demo_name.trim_end_matches(".dem");
        let url = format!("{}{}/{}.stats.json", self.base_url, STATS_PATH, stats_name);

        debug!(url = %url, "Fetching demo stats");

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ExternalService(format!("Failed to fetch demo stats: {}", e)))?;

        if !response.status().is_success() {
            return Err(PluginError::NotFound(format!(
                "Demo stats not found: {} (status: {})",
                demo_name,
                response.status()
            )));
        }

        let stats: Cs2DemoStats = response
            .json()
            .await
            .map_err(|e| PluginError::ParseError(format!("Failed to parse demo stats: {}", e)))?;

        Ok(stats)
    }

    /// Get the download URL for a demo.
    pub fn get_demo_url(&self, demo_name: &str) -> String {
        format!("{}/{}", self.base_url, demo_name)
    }

    /// Get the stats URL for a demo.
    pub fn get_stats_url(&self, demo_name: &str) -> String {
        let stats_name = demo_name.trim_end_matches(".dem");
        format!("{}{}/{}.stats.json", self.base_url, STATS_PATH, stats_name)
    }

    /// List available demos (if the service supports directory listing).
    /// Returns demo names that can be used with get_demo_stats.
    #[instrument(skip(self))]
    pub async fn list_demos(&self, prefix: Option<&str>) -> Result<Vec<String>, PluginError> {
        // This would need to be implemented based on how your S3/demo service
        // exposes directory listing. For now, return an error indicating
        // that discovery should use known demo names.
        Err(PluginError::NotSupported(
            "Demo listing not implemented - use known demo names".to_string()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires external service
    async fn test_fetch_demo_stats() {
        let client = Cs2DemoClient::default();
        // Replace with an actual demo name for integration testing
        let result = client.get_demo_stats("test_demo").await;
        println!("Result: {:?}", result);
    }
}
```

### 3. Evidence Validation

Create `crates/portal-plugins/src/games/cs2/evidence_validator.rs`:

```rust
use super::demo_stats::Cs2DemoStats;
use crate::types::{EvidenceValidation, ExtractedResult, GameResult};
use tracing::debug;

/// Validates demo evidence against claimed match results.
pub struct Cs2EvidenceValidator;

impl Cs2EvidenceValidator {
    /// Validate demo stats against a claimed game result.
    ///
    /// # Arguments
    /// * `stats` - Parsed demo stats from the external service
    /// * `claimed_result` - The result being claimed by the player
    /// * `participant1_steam_ids` - Steam IDs for participant 1 (tournament registration)
    /// * `participant2_steam_ids` - Steam IDs for participant 2 (tournament registration)
    pub fn validate(
        stats: &Cs2DemoStats,
        claimed_result: &GameResult,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> EvidenceValidation {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut confidence = 1.0f32;

        // 1. Verify map matches
        if !Self::maps_match(&stats.map, &claimed_result.map_id) {
            errors.push(format!(
                "Map mismatch: demo has '{}', claimed '{}'",
                stats.map, claimed_result.map_id
            ));
            confidence *= 0.0; // Fatal mismatch
        }

        // 2. Verify players participated
        let (p1_present, p2_present) = Self::verify_players(
            stats,
            participant1_steam_ids,
            participant2_steam_ids,
        );

        if !p1_present {
            warnings.push("Not all Participant 1 players found in demo".to_string());
            confidence *= 0.7;
        }
        if !p2_present {
            warnings.push("Not all Participant 2 players found in demo".to_string());
            confidence *= 0.7;
        }

        // 3. Determine which demo team corresponds to which participant
        let team_mapping = Self::determine_team_mapping(stats, participant1_steam_ids, participant2_steam_ids);

        match team_mapping {
            Some((p1_team_name, p2_team_name, mapping_confidence)) => {
                confidence *= mapping_confidence;

                // 4. Extract and compare scores
                let demo_p1_score = stats.score_for_team(&p1_team_name).unwrap_or(0);
                let demo_p2_score = stats.score_for_team(&p2_team_name).unwrap_or(0);

                let scores_match = demo_p1_score == claimed_result.participant1_score
                    && demo_p2_score == claimed_result.participant2_score;

                if !scores_match {
                    errors.push(format!(
                        "Score mismatch: demo shows {}-{}, claimed {}-{}",
                        demo_p1_score, demo_p2_score,
                        claimed_result.participant1_score, claimed_result.participant2_score
                    ));
                    confidence *= 0.0; // Fatal mismatch
                }

                // 5. Verify winner
                let demo_winner_is_p1 = demo_p1_score > demo_p2_score;
                let claimed_winner_is_p1 = claimed_result.participant1_score > claimed_result.participant2_score;

                if demo_winner_is_p1 != claimed_winner_is_p1 {
                    errors.push("Winner mismatch between demo and claimed result".to_string());
                    confidence *= 0.0;
                }

                // Build extracted result
                let extracted_result = ExtractedResult {
                    map_id: stats.map.clone(),
                    participant1_score: demo_p1_score,
                    participant2_score: demo_p2_score,
                    duration_seconds: 0, // Not available in stats format
                    player_stats: serde_json::to_value(&stats.player_summaries).unwrap_or_default(),
                };

                EvidenceValidation {
                    is_valid: errors.is_empty() && confidence > 0.5,
                    confidence,
                    extracted_result: Some(extracted_result),
                    warnings,
                    errors,
                }
            }
            None => {
                errors.push("Could not determine team mapping from Steam IDs".to_string());
                EvidenceValidation {
                    is_valid: false,
                    confidence: 0.0,
                    extracted_result: None,
                    warnings,
                    errors,
                }
            }
        }
    }

    /// Check if map names match (handles different naming conventions).
    fn maps_match(demo_map: &str, claimed_map: &str) -> bool {
        let normalize = |s: &str| s.to_lowercase().replace("de_", "").replace("_", "");
        normalize(demo_map) == normalize(claimed_map)
    }

    /// Verify that expected players are in the demo.
    fn verify_players(
        stats: &Cs2DemoStats,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> (bool, bool) {
        let demo_steam_ids = stats.all_steam_ids();

        let p1_present = participant1_steam_ids.iter().all(|id| demo_steam_ids.contains(id));
        let p2_present = participant2_steam_ids.iter().all(|id| demo_steam_ids.contains(id));

        (p1_present, p2_present)
    }

    /// Determine which demo team corresponds to which participant.
    /// Returns (participant1_team_name, participant2_team_name, confidence) or None if undetermined.
    fn determine_team_mapping(
        stats: &Cs2DemoStats,
        participant1_steam_ids: &[String],
        participant2_steam_ids: &[String],
    ) -> Option<(String, String, f32)> {
        let team_names: Vec<String> = stats.team_names();
        if team_names.len() != 2 {
            return None;
        }

        let team_a = &team_names[0];
        let team_b = &team_names[1];

        let team_a_ids = stats.steam_ids_for_team(team_a);
        let team_b_ids = stats.steam_ids_for_team(team_b);

        // Count how many participant1 players are in each demo team
        let p1_in_team_a = participant1_steam_ids.iter()
            .filter(|id| team_a_ids.contains(id))
            .count();
        let p1_in_team_b = participant1_steam_ids.iter()
            .filter(|id| team_b_ids.contains(id))
            .count();

        let total_p1 = participant1_steam_ids.len().max(1);

        if p1_in_team_a > p1_in_team_b {
            // Participant 1 is team A
            let confidence = p1_in_team_a as f32 / total_p1 as f32;
            Some((team_a.clone(), team_b.clone(), confidence))
        } else if p1_in_team_b > p1_in_team_a {
            // Participant 1 is team B
            let confidence = p1_in_team_b as f32 / total_p1 as f32;
            Some((team_b.clone(), team_a.clone(), confidence))
        } else {
            // Equal or zero - can't determine
            None
        }
    }
}
```

### 4. Extend CS2 Plugin

Update `crates/portal-plugins/src/games/cs2/mod.rs`:

```rust
mod demo_client;
mod demo_stats;
mod evidence_validator;

pub use demo_client::Cs2DemoClient;
pub use demo_stats::{Cs2DemoStats, Cs2PlayerStats, Cs2RoundStats};
pub use evidence_validator::Cs2EvidenceValidator;

use crate::error::PluginError;
use crate::traits::EvidencePlugin;
use crate::types::{
    DiscoveredEvidence, EvidenceStorage, EvidenceType, EvidenceValidation,
    GameResult, MatchContext,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

// ... existing CS2 plugin code ...

/// CS2 plugin with evidence support.
pub struct Cs2PluginWithEvidence {
    inner: Cs2Plugin,
    demo_client: Arc<Cs2DemoClient>,
}

impl Cs2PluginWithEvidence {
    pub fn new() -> Self {
        Self {
            inner: Cs2Plugin::new(),
            demo_client: Arc::new(Cs2DemoClient::default()),
        }
    }

    pub fn with_demo_url(mut self, base_url: String) -> Self {
        self.demo_client = Arc::new(Cs2DemoClient::new(base_url));
        self
    }
}

#[async_trait]
impl EvidencePlugin for Cs2PluginWithEvidence {
    async fn discover_evidence(
        &self,
        match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError> {
        // For automatic discovery, we'd need the demo service to support listing.
        // For now, return empty - users will manually provide demo names.
        //
        // In the future, if the demo service exposes an API to search by:
        // - Time range
        // - Steam IDs
        // - Server name
        // This could automatically find matching demos.
        Ok(Vec::new())
    }

    async fn validate_evidence(
        &self,
        demo_name: &str,
        claimed_result: &GameResult,
        team1_steam_ids: &[String],
        team2_steam_ids: &[String],
    ) -> Result<EvidenceValidation, PluginError> {
        // Fetch stats from external service
        let stats = self.demo_client.get_demo_stats(demo_name).await?;

        // Validate against claimed result
        let validation = Cs2EvidenceValidator::validate(
            &stats,
            claimed_result,
            team1_steam_ids,
            team2_steam_ids,
        );

        Ok(validation)
    }

    async fn get_demo_metadata(
        &self,
        demo_name: &str,
    ) -> Result<Cs2DemoStats, PluginError> {
        self.demo_client.get_demo_stats(demo_name).await
    }

    fn get_demo_url(&self, demo_name: &str) -> String {
        self.demo_client.get_demo_url(demo_name)
    }

    fn get_stats_url(&self, demo_name: &str) -> String {
        self.demo_client.get_stats_url(demo_name)
    }
}
```

### 5. Update Plugin Types

Add to `crates/portal-plugins/src/types.rs`:

```rust
/// Result of evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceValidation {
    /// Whether the evidence is valid.
    pub is_valid: bool,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,

    /// Extracted result from the evidence.
    pub extracted_result: Option<ExtractedResult>,

    /// Non-fatal issues found.
    pub warnings: Vec<String>,

    /// Fatal issues that invalidate the evidence.
    pub errors: Vec<String>,
}

/// Result extracted from evidence (e.g., demo file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedResult {
    /// Map played.
    pub map_id: String,

    /// Participant 1 score.
    pub participant1_score: i32,

    /// Participant 2 score.
    pub participant2_score: i32,

    /// Match duration in seconds.
    pub duration_seconds: i64,

    /// Player stats as JSON.
    pub player_stats: serde_json::Value,
}

/// Context for evidence discovery.
#[derive(Debug, Clone)]
pub struct MatchContext {
    pub tournament_id: String,
    pub match_id: String,
    pub game_id: String,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub participant1_steam_ids: Vec<String>,
    pub participant2_steam_ids: Vec<String>,
    pub map_pool: Vec<String>,
}

/// Evidence discovered by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEvidence {
    /// External identifier (e.g., demo file name).
    pub external_id: String,

    /// Type of evidence.
    pub evidence_type: EvidenceType,

    /// Display name.
    pub name: String,

    /// Where the evidence is stored.
    pub storage_url: String,

    /// File size if known.
    pub file_size_bytes: Option<i64>,

    /// Plugin-specific metadata.
    pub metadata: serde_json::Value,

    /// When the evidence was discovered.
    pub discovered_at: chrono::DateTime<chrono::Utc>,

    /// How likely this evidence matches the match (0.0 - 1.0).
    pub relevance_score: f32,
}
```

### 6. API Handlers

Add to `crates/portal-api/src/handlers/evidence.rs`:

```rust
/// Validate a demo against a match result.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/evidence/validate-demo",
    request_body = ValidateDemoRequest,
    responses(
        (status = 200, description = "Validation result", body = DataResponse<DemoValidationResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn validate_demo(
    State(state): State<AppState>,
    Path((tournament_id, match_id)): Path<(TournamentId, TournamentMatchId)>,
    AuthenticatedUser(user): AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<ValidateDemoRequest>,
) -> ApiResult<Json<DataResponse<DemoValidationResponse>>> {
    // Get match details to find participant Steam IDs
    let match_ = state
        .match_repo
        .find_by_id(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Match not found"))?;

    // Get participant Steam IDs from registrations
    let (team1_steam_ids, team2_steam_ids) = get_participant_steam_ids(
        &state,
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ).await?;

    // Build claimed result from request or match data
    let claimed_result = GameResult {
        game_number: req.game_number.unwrap_or(1),
        map_id: req.map_id.clone(),
        participant1_score: req.participant1_score,
        participant2_score: req.participant2_score,
        winner_registration_id: if req.participant1_score > req.participant2_score {
            match_.participant1_registration_id
        } else {
            match_.participant2_registration_id
        },
    };

    // Validate using CS2 plugin
    let cs2_plugin = Cs2PluginWithEvidence::new();
    let validation = cs2_plugin
        .validate_evidence(
            &req.demo_name,
            &claimed_result,
            &team1_steam_ids,
            &team2_steam_ids,
        )
        .await
        .map_err(|e| ApiError::bad_request(format!("Validation failed: {}", e)))?;

    Ok(Json(DataResponse::new(DemoValidationResponse {
        is_valid: validation.is_valid,
        confidence: validation.confidence,
        extracted_result: validation.extracted_result,
        warnings: validation.warnings,
        errors: validation.errors,
        demo_url: cs2_plugin.get_demo_url(&req.demo_name),
        stats_url: cs2_plugin.get_stats_url(&req.demo_name),
    })))
}

/// Get demo stats without validation.
#[utoipa::path(
    get,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/evidence/demo-stats/{demo_name}",
    responses(
        (status = 200, description = "Demo stats", body = DataResponse<DemoStatsResponse>),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn get_demo_stats(
    State(state): State<AppState>,
    Path((tournament_id, match_id, demo_name)): Path<(TournamentId, TournamentMatchId, String)>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> ApiResult<Json<DataResponse<DemoStatsResponse>>> {
    let cs2_plugin = Cs2PluginWithEvidence::new();

    let stats = cs2_plugin
        .get_demo_metadata(&demo_name)
        .await
        .map_err(|e| match e {
            PluginError::NotFound(_) => ApiError::not_found(format!("Demo not found: {}", demo_name)),
            _ => ApiError::internal(format!("Failed to fetch demo stats: {}", e)),
        })?;

    Ok(Json(DataResponse::new(DemoStatsResponse {
        demo_name: stats.demo_name,
        map_name: stats.map_name,
        duration_seconds: stats.duration_seconds,
        recorded_at: stats.recorded_at,
        team1_score: stats.team1_score,
        team2_score: stats.team2_score,
        team1_name: stats.team1_name,
        team2_name: stats.team2_name,
        players: stats.players.into_iter().map(Into::into).collect(),
        demo_url: cs2_plugin.get_demo_url(&demo_name),
        stats_url: cs2_plugin.get_stats_url(&demo_name),
    })))
}

/// Link a demo to a match as evidence.
#[utoipa::path(
    post,
    path = "/v1/tournaments/{tournament_id}/matches/{match_id}/evidence/link-demo",
    request_body = LinkDemoRequest,
    responses(
        (status = 201, description = "Demo linked", body = DataResponse<EvidenceResponse>),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 404, description = "Demo not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "evidence"
)]
pub async fn link_demo(
    State(state): State<AppState>,
    Path((tournament_id, match_id)): Path<(TournamentId, TournamentMatchId)>,
    AuthenticatedUser(user): AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<LinkDemoRequest>,
) -> ApiResult<Json<DataResponse<EvidenceResponse>>> {
    let cs2_plugin = Cs2PluginWithEvidence::new();

    // Verify demo exists by fetching stats
    let stats = cs2_plugin
        .get_demo_metadata(&req.demo_name)
        .await
        .map_err(|e| ApiError::not_found(format!("Demo not found: {}", req.demo_name)))?;

    // Create evidence record
    let evidence = state
        .evidence_service
        .add_link(
            match_id,
            req.game_number,
            EvidenceType::Demo,
            cs2_plugin.get_demo_url(&req.demo_name),
            format!("CS2 Demo: {}", stats.map_name),
            req.description,
            user.user_id,
        )
        .await?;

    // Store demo metadata as plugin_metadata
    state
        .evidence_repo
        .update_plugin_metadata(evidence.id, serde_json::to_value(&stats)?)
        .await?;

    Ok(Json(DataResponse::new(evidence.into())))
}
```

### 7. Request/Response DTOs

Add to `crates/portal-api/src/dto/requests/evidence.rs`:

```rust
/// Request to validate a demo against a match result.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ValidateDemoRequest {
    /// Demo file name (e.g., "match_12345.dem")
    #[validate(length(min = 1, max = 256))]
    pub demo_name: String,

    /// Map ID to validate against.
    #[validate(length(min = 1, max = 64))]
    pub map_id: String,

    /// Claimed score for participant 1.
    #[validate(range(min = 0, max = 100))]
    pub participant1_score: i32,

    /// Claimed score for participant 2.
    #[validate(range(min = 0, max = 100))]
    pub participant2_score: i32,

    /// Game number (for series matches).
    pub game_number: Option<i32>,
}

/// Request to link a demo to a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LinkDemoRequest {
    /// Demo file name.
    #[validate(length(min = 1, max = 256))]
    pub demo_name: String,

    /// Game number this demo is for (for series).
    pub game_number: Option<i32>,

    /// Optional description.
    #[validate(length(max = 500))]
    pub description: Option<String>,
}
```

Add to `crates/portal-api/src/dto/responses/evidence.rs`:

```rust
/// Demo validation response.
#[derive(Debug, Serialize, ToSchema)]
pub struct DemoValidationResponse {
    /// Whether the demo validates the claimed result.
    pub is_valid: bool,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,

    /// Result extracted from the demo.
    pub extracted_result: Option<ExtractedResultResponse>,

    /// Non-fatal warnings.
    pub warnings: Vec<String>,

    /// Fatal errors.
    pub errors: Vec<String>,

    /// URL to download the demo.
    pub demo_url: String,

    /// URL to view stats JSON.
    pub stats_url: String,
}

/// Demo stats response.
#[derive(Debug, Serialize, ToSchema)]
pub struct DemoStatsResponse {
    pub demo_name: String,
    pub map_name: String,
    pub duration_seconds: i64,
    pub recorded_at: DateTime<Utc>,
    pub team1_score: i32,
    pub team2_score: i32,
    pub team1_name: Option<String>,
    pub team2_name: Option<String>,
    pub players: Vec<PlayerStatsResponse>,
    pub demo_url: String,
    pub stats_url: String,
}

/// Player stats from demo.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlayerStatsResponse {
    pub steam_id: String,
    pub name: String,
    pub team: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub headshots: i32,
    pub damage: i32,
    pub adr: f32,
    pub rating: Option<f32>,
}

/// Extracted result from evidence.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractedResultResponse {
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub duration_seconds: i64,
}
```

### 8. Routes

Add to evidence routes:

```rust
// In routes/matches.rs or routes/evidence.rs
.route(
    "/v1/tournaments/:tournament_id/matches/:match_id/evidence/validate-demo",
    post(handlers::evidence::validate_demo),
)
.route(
    "/v1/tournaments/:tournament_id/matches/:match_id/evidence/demo-stats/:demo_name",
    get(handlers::evidence::get_demo_stats),
)
.route(
    "/v1/tournaments/:tournament_id/matches/:match_id/evidence/link-demo",
    post(handlers::evidence::link_demo),
)
```

### 9. OpenAPI Registration

Add to `crates/portal-api/src/openapi.rs`:

```rust
// In paths
handlers::evidence::validate_demo,
handlers::evidence::get_demo_stats,
handlers::evidence::link_demo,

// In schemas
ValidateDemoRequest,
LinkDemoRequest,
DemoValidationResponse,
DemoStatsResponse,
PlayerStatsResponse,
ExtractedResultResponse,
```

---

## Tests

Create `crates/portal-api/tests/demo_evidence_test.rs`:

```rust
//! CS2 Demo Evidence integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_demo_evidence_endpoints_exist() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";

    // Validate demo endpoint
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/validate-demo",
                tournament_id, match_id
            ),
            &json!({
                "demo_name": "test_demo.dem",
                "map_id": "de_dust2",
                "participant1_score": 16,
                "participant2_score": 10
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/validate-demo should exist"
    );

    // Get demo stats endpoint
    let response = app
        .get_auth(&format!(
            "/v1/tournaments/{}/matches/{}/evidence/demo-stats/test_demo",
            tournament_id, match_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/demo-stats/:name should exist"
    );

    // Link demo endpoint
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/link-demo",
                tournament_id, match_id
            ),
            &json!({
                "demo_name": "test_demo.dem"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/link-demo should exist"
    );
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_validate_demo_invalid_demo_name() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";

    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/validate-demo",
                tournament_id, match_id
            ),
            &json!({
                "demo_name": "",
                "map_id": "de_dust2",
                "participant1_score": 16,
                "participant2_score": 10
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_validate_demo_invalid_scores() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";

    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/validate-demo",
                tournament_id, match_id
            ),
            &json!({
                "demo_name": "test_demo.dem",
                "map_id": "de_dust2",
                "participant1_score": -1,
                "participant2_score": 10
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_link_demo_missing_name() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";

    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/link-demo",
                tournament_id, match_id
            ),
            &json!({}),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_demo_endpoints_require_auth() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";

    let response = app
        .post_json_no_auth(
            &format!(
                "/v1/tournaments/{}/matches/{}/evidence/validate-demo",
                tournament_id, match_id
            ),
            &json!({
                "demo_name": "test_demo.dem",
                "map_id": "de_dust2",
                "participant1_score": 16,
                "participant2_score": 10
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}
```

Create `crates/portal-plugins/tests/cs2_demo_test.rs`:

```rust
//! CS2 Demo Client and Validator unit tests.

use portal_plugins::games::cs2::{
    Cs2DemoStats, Cs2EvidenceValidator, TeamInfo, RoundData, PlayerState,
    RoundPlayerStats, PlayerSummary, PlayerInteraction,
};
use portal_plugins::types::GameResult;
use std::collections::HashMap;

fn create_test_stats() -> Cs2DemoStats {
    let team_alpha = TeamInfo {
        team_id: 2,
        team_name: "team_Alpha".to_string(),
        team_side: "T".to_string(),
    };
    let team_beta = TeamInfo {
        team_id: 3,
        team_name: "team_Beta".to_string(),
        team_side: "CT".to_string(),
    };

    let mut teams = HashMap::new();
    teams.insert("team_Alpha".to_string(), team_alpha.clone());
    teams.insert("team_Beta".to_string(), team_beta.clone());

    let mut final_score = HashMap::new();
    final_score.insert("team_Alpha".to_string(), 16);
    final_score.insert("team_Beta".to_string(), 10);

    // Player summaries - aggregated stats
    let mut player_summaries = HashMap::new();
    player_summaries.insert("76561198000000001".to_string(), PlayerSummary {
        player_id: 76561198000000001,
        player_name: "Player1".to_string(),
        team: team_alpha.clone(),
        kills: 20,
        deaths: 12,
        assists: 5,
        headshot_kills: 8,
        flash_assists: 2,
        damage_dealt: 2400,
        utility_damage: 100,
        adr: 92.3,
        hs_percentage: 40.0,
        wallbangs: 1,
        smoke_kills: 0,
        blind_kills: 0,
        blinded_kills: 0,
        flash_duration: 15.5,
        enemies_flashed: 8,
        bomb_plants: 3,
        bomb_defuses: 0,
        outgoing_interactions: HashMap::new(),
        incoming_interactions: HashMap::new(),
        weapon_kills: HashMap::new(),
    });
    player_summaries.insert("76561198000000002".to_string(), PlayerSummary {
        player_id: 76561198000000002,
        player_name: "Player2".to_string(),
        team: team_beta.clone(),
        kills: 15,
        deaths: 18,
        assists: 3,
        headshot_kills: 5,
        flash_assists: 1,
        damage_dealt: 1800,
        utility_damage: 50,
        adr: 69.2,
        hs_percentage: 33.3,
        wallbangs: 0,
        smoke_kills: 0,
        blind_kills: 0,
        blinded_kills: 0,
        flash_duration: 10.2,
        enemies_flashed: 5,
        bomb_plants: 0,
        bomb_defuses: 2,
        outgoing_interactions: HashMap::new(),
        incoming_interactions: HashMap::new(),
        weapon_kills: HashMap::new(),
    });

    // Round data (simplified for tests)
    let mut player_states = HashMap::new();
    player_states.insert("76561198000000001".to_string(), PlayerState {
        player_id: 76561198000000001,
        player_name: "Player1".to_string(),
        team: team_alpha.clone(),
        starting_money: 800,
    });
    player_states.insert("76561198000000002".to_string(), PlayerState {
        player_id: 76561198000000002,
        player_name: "Player2".to_string(),
        team: team_beta.clone(),
        starting_money: 800,
    });

    let mut player_stats = HashMap::new();
    player_stats.insert("76561198000000001".to_string(), RoundPlayerStats {
        kills: 2,
        deaths: 1,
        assists: 0,
        damage: 200,
    });
    player_stats.insert("76561198000000002".to_string(), RoundPlayerStats {
        kills: 1,
        deaths: 2,
        assists: 0,
        damage: 150,
    });

    let mut round_score = HashMap::new();
    round_score.insert("team_Alpha".to_string(), 1);
    round_score.insert("team_Beta".to_string(), 0);

    Cs2DemoStats {
        schema_version: 3,
        map: "de_dust2".to_string(),
        match_date: "2024-09-14T20:17:30Z".to_string(),
        demo_file: "test_match.dem".to_string(),
        match_id: "test-match-123".to_string(),
        teams,
        final_score,
        player_summaries,
        rounds: vec![RoundData {
            round_number: 1,
            winner_team: "team_Alpha".to_string(),
            winner_side: "T".to_string(),
            round_score,
            player_states,
            events: vec![],
            player_stats,
        }],
    }
}

#[test]
fn test_validate_matching_result() {
    let stats = create_test_stats();
    let claimed = GameResult {
        game_number: 1,
        map_id: "de_dust2".to_string(),
        participant1_score: 16,
        participant2_score: 10,
        winner_registration_id: "team1".to_string(),
    };

    let p1_ids = vec!["76561198000000001".to_string()];
    let p2_ids = vec!["76561198000000002".to_string()];

    let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

    assert!(result.is_valid);
    assert!(result.confidence > 0.8);
    assert!(result.errors.is_empty());
}

#[test]
fn test_validate_score_mismatch() {
    let stats = create_test_stats();
    let claimed = GameResult {
        game_number: 1,
        map_id: "de_dust2".to_string(),
        participant1_score: 16,
        participant2_score: 14, // Wrong score
        winner_registration_id: "team1".to_string(),
    };

    let p1_ids = vec!["76561198000000001".to_string()];
    let p2_ids = vec!["76561198000000002".to_string()];

    let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

    assert!(!result.is_valid);
    assert!(!result.errors.is_empty());
    assert!(result.errors.iter().any(|e| e.contains("Score mismatch")));
}

#[test]
fn test_validate_map_mismatch() {
    let stats = create_test_stats();
    let claimed = GameResult {
        game_number: 1,
        map_id: "de_mirage".to_string(), // Wrong map
        participant1_score: 16,
        participant2_score: 10,
        winner_registration_id: "team1".to_string(),
    };

    let p1_ids = vec!["76561198000000001".to_string()];
    let p2_ids = vec!["76561198000000002".to_string()];

    let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

    assert!(!result.is_valid);
    assert!(result.errors.iter().any(|e| e.contains("Map mismatch")));
}

#[test]
fn test_validate_missing_players_warning() {
    let stats = create_test_stats();
    let claimed = GameResult {
        game_number: 1,
        map_id: "de_dust2".to_string(),
        participant1_score: 16,
        participant2_score: 10,
        winner_registration_id: "team1".to_string(),
    };

    // Include a player not in the demo
    let p1_ids = vec![
        "76561198000000001".to_string(),
        "76561198000000099".to_string(), // Not in demo
    ];
    let p2_ids = vec!["76561198000000002".to_string()];

    let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

    // Should still be valid but with reduced confidence
    assert!(result.is_valid);
    assert!(result.confidence < 1.0);
    assert!(!result.warnings.is_empty());
}

#[test]
fn test_map_name_normalization() {
    let stats = create_test_stats();
    let claimed = GameResult {
        game_number: 1,
        map_id: "dust2".to_string(), // Without de_ prefix
        participant1_score: 16,
        participant2_score: 10,
        winner_registration_id: "team1".to_string(),
    };

    let p1_ids = vec!["76561198000000001".to_string()];
    let p2_ids = vec!["76561198000000002".to_string()];

    let result = Cs2EvidenceValidator::validate(&stats, &claimed, &p1_ids, &p2_ids);

    // Should match despite different naming
    assert!(result.is_valid);
}

#[test]
fn test_player_summaries() {
    let stats = create_test_stats();

    assert_eq!(stats.player_summaries.len(), 2);

    let player1 = stats.get_player("76561198000000001").unwrap();
    assert_eq!(player1.kills, 20);
    assert_eq!(player1.deaths, 12);
    assert_eq!(player1.team.team_name, "team_Alpha");
    assert!(player1.adr > 90.0);
}

#[test]
fn test_team_steam_ids() {
    let stats = create_test_stats();

    let alpha_ids = stats.steam_ids_for_team("team_Alpha");
    let beta_ids = stats.steam_ids_for_team("team_Beta");

    assert!(alpha_ids.contains(&"76561198000000001".to_string()));
    assert!(beta_ids.contains(&"76561198000000002".to_string()));
}

#[test]
fn test_winner_team_name() {
    let stats = create_test_stats();
    let winner = stats.winner_team_name();

    assert_eq!(winner, Some("team_Alpha".to_string()));
}
```

---

## Acceptance Criteria

- [ ] Demo stats can be fetched from external service
- [ ] Demo validation compares scores correctly
- [ ] Map name normalization handles de_ prefix
- [ ] Player matching uses Steam IDs
- [ ] Team mapping determined automatically
- [ ] Demos can be linked as evidence
- [ ] Validation returns detailed errors/warnings
- [ ] All tests pass
- [ ] OpenAPI docs updated

---

## Configuration

Add to environment variables:

```env
# CS2 Demo Service
CS2_DEMO_BASE_URL=https://demos.cs210mans.uk
```

The default URL is hardcoded but can be overridden for testing.

---

## JSON Schema

A complete JSON Schema document for schema version 3 is available at:
`docs/cs2-demo-stats-schema-v3.json`

This schema can be used for validation and code generation.

## Expected Stats JSON Format

Based on the actual demo service at `https://demos.cs210mans.uk/stats/`:

```json
{
  "schema_version": 3,
  "map": "de_inferno",
  "match_date": "2024-09-14T20:17:30Z",
  "demo_file": "2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem",
  "match_id": "unique-match-id",
  "teams": {
    "team_Maxymimi": {
      "team_id": 2,
      "team_name": "team_Maxymimi",
      "team_side": "T"
    },
    "team_Zan": {
      "team_id": 3,
      "team_name": "team_Zan",
      "team_side": "CT"
    }
  },
  "final_score": {
    "team_Maxymimi": 13,
    "team_Zan": 10
  },
  "player_summaries": {
    "76561197962015608": {
      "player_id": 76561197962015608,
      "player_name": "dewsy",
      "team": {
        "team_id": 2,
        "team_name": "team_Maxymimi",
        "team_side": "T"
      },
      "kills": 18,
      "deaths": 14,
      "assists": 3,
      "headshot_kills": 5,
      "flash_assists": 0,
      "damage_dealt": 1793,
      "utility_damage": 0,
      "adr": 77.96,
      "hs_percentage": 27.78,
      "wallbangs": 1,
      "smoke_kills": 0,
      "blind_kills": 0,
      "blinded_kills": 1,
      "flash_duration": 25.90,
      "enemies_flashed": 11,
      "bomb_plants": 2,
      "bomb_defuses": 1,
      "outgoing_interactions": {
        "76561197985524918": {"killed": 3},
        "76561198019332496": {"killed": 6}
      },
      "incoming_interactions": {
        "76561197969684583": {"killed": 3, "assisted": 2}
      },
      "weapon_kills": {
        "303": 4,
        "304": 6,
        "307": 1
      }
    }
  },
  "rounds": [
    {
      "round_number": 1,
      "winner_team": "team_Maxymimi",
      "winner_side": "T",
      "round_score": {
        "team_Maxymimi": 1,
        "team_Zan": 0
      },
      "player_states": {
        "76561197962015608": {
          "player_id": 76561197962015608,
          "player_name": "dewsy",
          "team": {
            "team_id": 2,
            "team_name": "team_Maxymimi",
            "team_side": "T"
          },
          "starting_money": 800
        }
      },
      "events": [
        {
          "event_type": "damage",
          "event_time": 0.475,
          "source_player_id": 76561197969684583,
          "target_player_id": 76561197962015608,
          "weapon": "HE Grenade",
          "weapon_type": 506,
          "attributes": {
            "damage": 2,
            "armor_damage": 0,
            "health_left": 98,
            "hitgroup": 0
          }
        },
        {
          "event_type": "kill",
          "event_time": 45.123,
          "source_player_id": 76561197962015608,
          "target_player_id": 76561198000000002,
          "weapon": "AK-47",
          "weapon_type": 303,
          "attributes": {
            "headshot": true,
            "penetrated": false
          }
        }
      ],
      "player_stats": {
        "76561197962015608": {
          "kills": 2,
          "deaths": 0,
          "assists": 1,
          "damage": 180
        }
      }
    }
  ]
}
```

**Key Format Details:**
- `schema_version`: Integer indicating the schema version (currently 3)
- `teams` and `final_score` are keyed by team name strings, not numeric indices
- `player_summaries`: **Pre-aggregated stats** for each player (no manual aggregation needed)
- `rounds[].player_stats`: Per-round stats for granular analysis
- Players are identified by Steam ID (as string keys)
- `match_date` is ISO 8601 format
- Event types: `damage`, `kill`, `assist`, `flash`, `bomb_plant`, `bomb_defuse`, `round_end`

---

## Output

After completing this sub-phase:

1. Run tests: `cargo test -p portal-plugins -- cs2_demo`
2. Run API tests: `cargo test -p portal-api --test demo_evidence_test --features test-utils`
3. Verify OpenAPI at `/swagger-ui`
4. Update batch 3 documentation to mark 3.8 as complete
