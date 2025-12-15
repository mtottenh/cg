//! Demo catalog request DTOs.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

/// Query parameters for listing demos.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct ListDemosQuery {
    /// Filter by game ID.
    pub game_id: Option<Uuid>,
    /// Filter by category (uncategorized, pug, league, scrim, ignored).
    pub category: Option<String>,
    /// Filter by processing status (pending, processing, ready, failed, archived).
    pub status: Option<String>,
    /// Filter by league ID.
    pub league_id: Option<Uuid>,
    /// Filter by tournament ID.
    pub tournament_id: Option<Uuid>,
    /// Filter by map name (partial match).
    pub map_name: Option<String>,
    /// Filter by team name (partial match).
    pub team_name: Option<String>,
    /// Filter by player Steam ID.
    pub steam_id: Option<String>,
    /// Filter by match date from (ISO 8601).
    pub match_date_from: Option<String>,
    /// Filter by match date to (ISO 8601).
    pub match_date_to: Option<String>,
    /// Include hidden demos (admin only).
    #[serde(default)]
    pub include_hidden: bool,
    /// Maximum number of results.
    pub limit: Option<i64>,
    /// Offset for pagination.
    pub offset: Option<i64>,
}

/// Request to catalog a new demo from S3.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CatalogDemoRequest {
    /// Game ID for this demo.
    pub game_id: Uuid,
    /// Demo file name.
    #[validate(length(min = 1, max = 512))]
    pub file_name: String,
    /// S3 bucket name.
    #[validate(length(min = 1, max = 128))]
    pub s3_bucket: String,
    /// S3 object key.
    #[validate(length(min = 1, max = 512))]
    pub s3_key: String,
    /// File size in bytes (optional).
    pub file_size_bytes: Option<i64>,
}

/// Request to categorize a demo.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CategorizeDemoRequest {
    /// Demo category (uncategorized, pug, league, scrim, ignored).
    #[validate(length(min = 1, max = 32))]
    pub category: String,
}

/// Request to hide or unhide a demo.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct SetDemoVisibilityRequest {
    /// Whether the demo should be hidden.
    pub is_hidden: bool,
}

/// Request to associate a demo with a league/tournament.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct AssociateDemoRequest {
    /// League ID to associate with (optional).
    pub league_id: Option<Uuid>,
    /// Tournament ID to associate with (optional).
    pub tournament_id: Option<Uuid>,
}

/// Request to set admin notes on a demo.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct SetDemoNotesRequest {
    /// Admin notes (optional, null to clear).
    #[validate(length(max = 2000))]
    pub notes: Option<String>,
}

/// Request to link a demo to a match.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct LinkDemoToMatchRequest {
    /// Tournament match ID.
    pub match_id: Uuid,
    /// Game number within the series (optional).
    pub game_number: Option<i32>,
    /// Link type (manual, auto_matched, evidence).
    #[validate(length(min = 1, max = 32))]
    pub link_type: Option<String>,
}

/// Request to unlink a demo from a match.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UnlinkDemoFromMatchRequest {
    /// Tournament match ID.
    pub match_id: Uuid,
}

/// Query parameters for pending demos processing.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct PendingDemosQuery {
    /// Maximum number of demos to return.
    pub limit: Option<i64>,
}

/// Query parameters for getting demos linked to a match.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct GetDemosForMatchQuery {
    /// Include player stats in the response.
    #[serde(default)]
    pub include_stats: bool,
    /// Filter by game number within the match series.
    pub game_number: Option<i32>,
}
