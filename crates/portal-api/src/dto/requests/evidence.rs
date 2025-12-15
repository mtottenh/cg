//! Evidence request DTOs.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

/// Request to initiate a file upload.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct InitiateUploadRequest {
    /// Type of evidence (demo, screenshot, server_log)
    pub evidence_type: String,
    /// Original file name
    pub file_name: String,
    /// File size in bytes
    pub file_size_bytes: i64,
    /// MIME type of the file
    pub mime_type: String,
    /// Optional game number within the match
    pub game_number: Option<i32>,
    /// Optional description
    pub description: Option<String>,
}

/// Request to add a link as evidence.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct AddLinkEvidenceRequest {
    /// Type of evidence (video, link)
    pub evidence_type: String,
    /// URL of the evidence
    pub url: String,
    /// Display name for the evidence
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional game number within the match
    pub game_number: Option<i32>,
}

/// Request to link discovered evidence to a match.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct LinkDiscoveredEvidenceRequest {
    /// External ID of the discovered evidence
    pub external_id: String,
    /// Optional game number within the match
    pub game_number: Option<i32>,
}

/// Query parameters for listing evidence.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct ListEvidenceQuery {
    /// Filter by evidence type
    pub evidence_type: Option<String>,
    /// Filter by game number
    pub game_number: Option<i32>,
    /// Filter by status (active, expired, deleted)
    pub status: Option<String>,
    /// Include plugin-discovered evidence
    #[serde(default)]
    pub include_discovered: bool,
}

/// Query parameters for evidence discovery.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct DiscoverEvidenceQuery {
    /// Minimum relevance score (0.0 to 1.0)
    pub min_relevance: Option<f32>,
    /// Maximum number of results
    pub limit: Option<i32>,
}

/// Request to validate evidence against claimed result.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ValidateEvidenceRequest {
    /// Evidence IDs to validate
    pub evidence_ids: Vec<Uuid>,
    /// Expected participant 1 score
    pub expected_participant1_score: Option<i32>,
    /// Expected participant 2 score
    pub expected_participant2_score: Option<i32>,
}

/// Request to validate a CS2 demo against a claimed match result.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct ValidateDemoRequest {
    /// Demo file name (e.g., "match_12345.dem" or "2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem")
    #[validate(length(min = 1, max = 256))]
    pub demo_name: String,

    /// Map ID to validate against (e.g., "de_dust2"). Optional - skips map validation if not provided.
    #[validate(length(max = 64))]
    pub map_id: Option<String>,

    /// Claimed score for participant 1.
    #[validate(range(min = 0, max = 100))]
    pub participant1_score: i32,

    /// Claimed score for participant 2.
    #[validate(range(min = 0, max = 100))]
    pub participant2_score: i32,

    /// Game number (for series matches). Defaults to 1.
    pub game_number: Option<i32>,
}

/// Request to link a demo to a match as evidence.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct LinkDemoRequest {
    /// Demo file name.
    #[validate(length(min = 1, max = 256))]
    pub demo_name: String,

    /// Game number this demo is for (for series). Defaults to 1.
    pub game_number: Option<i32>,

    /// Optional description.
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

/// Request to get demo stats.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct GetDemoStatsQuery {
    /// Participant 1 Steam IDs for team mapping (comma-separated).
    pub participant1_steam_ids: Option<String>,

    /// Participant 2 Steam IDs for team mapping (comma-separated).
    pub participant2_steam_ids: Option<String>,
}
