//! Demo catalog response DTOs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use portal_domain::entities::demo::{
    Demo, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats, ParsedDemoMetadata,
};

/// Response for a demo catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoResponse {
    /// Demo ID.
    pub id: Uuid,
    /// Game ID.
    pub game_id: Uuid,
    /// Demo file name.
    pub file_name: String,

    /// S3 bucket name.
    pub s3_bucket: String,
    /// S3 object key.
    pub s3_key: String,
    /// File size in bytes.
    pub file_size_bytes: Option<i64>,

    /// Category (uncategorized, pug, league, scrim, ignored).
    pub category: String,
    /// Whether the demo is hidden.
    pub is_hidden: bool,

    /// Associated league ID.
    pub league_id: Option<Uuid>,
    /// Associated tournament ID.
    pub tournament_id: Option<Uuid>,

    /// Parsed metadata from demo stats.
    pub metadata: Option<DemoMetadataResponse>,

    /// Processing status.
    pub status: String,
    /// When stats were fetched.
    pub stats_fetched_at: Option<DateTime<Utc>>,
    /// Stats fetch error message.
    pub stats_fetch_error: Option<String>,

    /// Who categorized this demo.
    pub categorized_by_user_id: Option<Uuid>,
    /// When it was categorized.
    pub categorized_at: Option<DateTime<Utc>>,
    /// Who hid this demo.
    pub hidden_by_user_id: Option<Uuid>,
    /// When it was hidden.
    pub hidden_at: Option<DateTime<Utc>>,
    /// Admin notes.
    pub admin_notes: Option<String>,

    /// When the demo was discovered in S3.
    pub discovered_at: DateTime<Utc>,
    /// When the record was created.
    pub created_at: DateTime<Utc>,
    /// When the record was last updated.
    pub updated_at: DateTime<Utc>,
}

impl From<Demo> for DemoResponse {
    fn from(demo: Demo) -> Self {
        Self {
            id: demo.id.as_uuid(),
            game_id: demo.game_id.as_uuid(),
            file_name: demo.file_name,
            s3_bucket: demo.s3_bucket,
            s3_key: demo.s3_key,
            file_size_bytes: demo.file_size_bytes,
            category: demo.category.to_string(),
            is_hidden: demo.is_hidden,
            league_id: demo.league_id.map(|id| id.as_uuid()),
            tournament_id: demo.tournament_id.map(|id| id.as_uuid()),
            metadata: demo.metadata.map(DemoMetadataResponse::from),
            status: demo.status.to_string(),
            stats_fetched_at: demo.stats_fetched_at,
            stats_fetch_error: demo.stats_fetch_error,
            categorized_by_user_id: demo.categorized_by_user_id.map(|id| id.as_uuid()),
            categorized_at: demo.categorized_at,
            hidden_by_user_id: demo.hidden_by_user_id.map(|id| id.as_uuid()),
            hidden_at: demo.hidden_at,
            admin_notes: demo.admin_notes,
            discovered_at: demo.discovered_at,
            created_at: demo.created_at,
            updated_at: demo.updated_at,
        }
    }
}

/// Parsed demo metadata.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoMetadataResponse {
    /// Map name (e.g., de_dust2).
    pub map_name: String,
    /// When the match took place.
    pub match_date: Option<DateTime<Utc>>,
    /// Team 1 name.
    pub team1_name: String,
    /// Team 2 name.
    pub team2_name: String,
    /// Team 1 score.
    pub team1_score: i32,
    /// Team 2 score.
    pub team2_score: i32,
    /// Total rounds played.
    pub total_rounds: i32,
    /// Duration in seconds.
    pub duration_seconds: Option<i64>,
}

impl From<ParsedDemoMetadata> for DemoMetadataResponse {
    fn from(meta: ParsedDemoMetadata) -> Self {
        Self {
            map_name: meta.map_name,
            match_date: meta.match_date,
            team1_name: meta.team1_name,
            team2_name: meta.team2_name,
            team1_score: meta.team1_score,
            team2_score: meta.team2_score,
            total_rounds: meta.total_rounds,
            duration_seconds: meta.duration_seconds,
        }
    }
}

/// Response for paginated demo list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoListResponse {
    /// List of demos.
    pub demos: Vec<DemoResponse>,
    /// Total count of matching demos.
    pub total: i64,
}

impl From<DemoListResult> for DemoListResponse {
    fn from(result: DemoListResult) -> Self {
        Self {
            demos: result.demos.into_iter().map(DemoResponse::from).collect(),
            total: result.total,
        }
    }
}

/// Response for a demo-match link.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoMatchLinkResponse {
    /// Link ID.
    pub id: Uuid,
    /// Demo ID.
    pub demo_id: Uuid,
    /// Match ID.
    pub match_id: Uuid,
    /// Game number within the series.
    pub game_number: Option<i32>,
    /// Link type (manual, auto_matched, evidence).
    pub link_type: String,
    /// Confidence score for auto-matched links.
    pub confidence_score: Option<f32>,
    /// Whether the link has been validated.
    pub validated: bool,
    /// When it was validated.
    pub validated_at: Option<DateTime<Utc>>,
    /// Validation result JSON.
    pub validation_result: Option<serde_json::Value>,
    /// Who created the link.
    pub linked_by_user_id: Option<Uuid>,
    /// When the link was created.
    pub linked_at: DateTime<Utc>,
    /// Record creation time.
    pub created_at: DateTime<Utc>,
}

impl From<DemoMatchLink> for DemoMatchLinkResponse {
    fn from(link: DemoMatchLink) -> Self {
        Self {
            id: link.id.as_uuid(),
            demo_id: link.demo_id.as_uuid(),
            match_id: link.match_id.as_uuid(),
            game_number: link.game_number,
            link_type: link.link_type.to_string(),
            confidence_score: link.confidence_score,
            validated: link.validated,
            validated_at: link.validated_at,
            validation_result: link.validation_result,
            linked_by_user_id: link.linked_by_user_id.map(|id| id.as_uuid()),
            linked_at: link.linked_at,
            created_at: link.created_at,
        }
    }
}

/// Response for a demo player entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoPlayerResponse {
    /// Entry ID.
    pub id: Uuid,
    /// Demo ID.
    pub demo_id: Uuid,
    /// Steam ID.
    pub steam_id: String,
    /// Player name in-game.
    pub player_name: String,
    /// Team name if available.
    pub team_name: Option<String>,
    /// Linked portal player ID.
    pub player_id: Option<Uuid>,
    /// Player statistics.
    pub stats: DemoPlayerStatsResponse,
    /// Record creation time.
    pub created_at: DateTime<Utc>,
}

impl From<DemoPlayer> for DemoPlayerResponse {
    fn from(player: DemoPlayer) -> Self {
        Self {
            id: player.id.as_uuid(),
            demo_id: player.demo_id.as_uuid(),
            steam_id: player.steam_id,
            player_name: player.player_name,
            team_name: player.team_name,
            player_id: player.player_id.map(|id| id.as_uuid()),
            stats: DemoPlayerStatsResponse::from(player.stats),
            created_at: player.created_at,
        }
    }
}

/// Player statistics from a demo.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoPlayerStatsResponse {
    /// Kills.
    pub kills: i32,
    /// Deaths.
    pub deaths: i32,
    /// Assists.
    pub assists: i32,
    /// Total damage.
    pub damage: i32,
    /// Average damage per round.
    pub adr: f64,
    /// Headshot kills.
    pub headshot_kills: i32,
    /// Headshot percentage.
    pub hs_percentage: f64,
    /// K/D ratio.
    pub kd_ratio: f64,
}

impl From<DemoPlayerStats> for DemoPlayerStatsResponse {
    fn from(stats: DemoPlayerStats) -> Self {
        Self {
            kills: stats.kills,
            deaths: stats.deaths,
            assists: stats.assists,
            damage: stats.damage,
            adr: stats.adr,
            headshot_kills: stats.headshot_kills,
            hs_percentage: stats.hs_percentage,
            kd_ratio: stats.kd_ratio(),
        }
    }
}

/// Response for demo status counts (admin dashboard).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoStatusCountsResponse {
    /// Count of pending demos.
    pub pending: i64,
    /// Count of processing demos.
    pub processing: i64,
    /// Count of ready demos.
    pub ready: i64,
    /// Count of failed demos.
    pub failed: i64,
    /// Count of archived demos.
    pub archived: i64,
}

/// Response for demo with players included.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoWithPlayersResponse {
    /// Demo details.
    #[serde(flatten)]
    pub demo: DemoResponse,
    /// Players in this demo.
    pub players: Vec<DemoPlayerResponse>,
}

/// Response for a list of demo IDs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoIdListResponse {
    /// List of demo IDs.
    pub demo_ids: Vec<Uuid>,
}

/// Response for a demo-match link with full demo data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoMatchLinkWithDemoResponse {
    /// The link details.
    pub link: DemoMatchLinkResponse,
    /// The demo details.
    pub demo: DemoResponse,
    /// Players in this demo (optional, depends on include_stats query).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<Vec<DemoPlayerResponse>>,
}

impl DemoMatchLinkWithDemoResponse {
    /// Create from domain types with optional players.
    #[must_use]
    pub fn from_domain(
        link: portal_domain::entities::demo::DemoMatchLink,
        demo: portal_domain::entities::demo::Demo,
        players: Vec<portal_domain::entities::demo::DemoPlayer>,
        include_players: bool,
    ) -> Self {
        Self {
            link: DemoMatchLinkResponse::from(link),
            demo: DemoResponse::from(demo),
            players: if include_players {
                Some(players.into_iter().map(DemoPlayerResponse::from).collect())
            } else {
                None
            },
        }
    }
}

/// Response for demo validation result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoValidationResultResponse {
    /// Whether the validation passed.
    pub is_valid: bool,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Score extracted from the demo [team1, team2].
    pub extracted_score: Option<[i32; 2]>,
    /// The claimed score being validated [team1, team2].
    pub claimed_score: [i32; 2],
    /// Whether the map name matches.
    pub map_match: bool,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
    /// Fatal errors.
    pub errors: Vec<String>,
}

/// Response for batch demo cataloging.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchCatalogResultResponse {
    /// Newly created demos.
    pub created: Vec<DemoResponse>,
    /// Demos that already existed.
    pub existing: Vec<DemoResponse>,
    /// Entries that failed to catalog.
    pub errors: Vec<BatchCatalogErrorResponse>,
}

/// Error entry in batch catalog result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchCatalogErrorResponse {
    /// S3 key that failed.
    pub s3_key: String,
    /// Error description.
    pub error: String,
}

impl From<portal_domain::entities::DemoValidationResult> for DemoValidationResultResponse {
    fn from(result: portal_domain::entities::DemoValidationResult) -> Self {
        Self {
            is_valid: result.is_valid,
            confidence: result.confidence,
            extracted_score: result.extracted_score.map(Into::into),
            claimed_score: [result.claimed_score.0, result.claimed_score.1],
            map_match: result.map_match,
            warnings: result.warnings,
            errors: result.errors,
        }
    }
}

/// Demo download URL response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoDownloadResponse {
    /// Demo ID.
    pub id: uuid::Uuid,
    /// Original file name.
    pub file_name: String,
    /// S3 bucket.
    pub s3_bucket: String,
    /// S3 key.
    pub s3_key: String,
    /// Download URL.
    pub download_url: String,
}
