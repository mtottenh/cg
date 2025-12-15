//! Evidence response DTOs.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

use portal_domain::entities::evidence::{
    DiscoveredEvidence, Evidence, EvidenceAccessUrl, EvidenceUploadInfo, EvidenceValidation,
};

/// Response for evidence details.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EvidenceResponse {
    pub id: Uuid,
    pub match_id: Uuid,
    pub game_number: Option<i32>,
    pub evidence_type: String,
    pub evidence_source: String,
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub storage_type: String,
    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub uploaded_by_registration_id: Option<Uuid>,
    pub uploaded_by_user_id: Option<Uuid>,
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<Evidence> for EvidenceResponse {
    fn from(e: Evidence) -> Self {
        Self {
            id: e.id.as_uuid(),
            match_id: e.match_id.as_uuid(),
            game_number: e.game_number,
            evidence_type: e.evidence_type.to_string(),
            evidence_source: e.evidence_source.to_string(),
            name: e.name,
            description: e.description,
            file_size_bytes: e.file_size_bytes,
            mime_type: e.mime_type,
            storage_type: e.storage.storage_type().to_string(),
            validated: e.validated,
            validated_at: e.validated_at,
            uploaded_by_registration_id: e.uploaded_by_registration_id.map(|id| id.as_uuid()),
            uploaded_by_user_id: e.uploaded_by_user_id.map(|id| id.as_uuid()),
            discovered_by_plugin: e.discovered_by_plugin,
            discovered_at: e.discovered_at,
            status: e.status.to_string(),
            created_at: e.created_at,
            expires_at: e.expires_at,
        }
    }
}

/// Response for upload initiation.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadInfoResponse {
    /// ID of the created evidence record
    pub evidence_id: Uuid,
    /// Presigned URL for uploading
    pub upload_url: String,
    /// HTTP method to use (PUT)
    pub upload_method: String,
    /// Headers to include in the upload request
    pub upload_headers: HashMap<String, String>,
    /// When the upload URL expires
    pub expires_at: DateTime<Utc>,
}

impl From<EvidenceUploadInfo> for UploadInfoResponse {
    fn from(info: EvidenceUploadInfo) -> Self {
        Self {
            evidence_id: info.evidence_id.as_uuid(),
            upload_url: info.upload_url,
            upload_method: info.upload_method,
            upload_headers: info.upload_headers,
            expires_at: info.expires_at,
        }
    }
}

/// Response for evidence access URL.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AccessUrlResponse {
    /// Presigned URL for accessing the evidence
    pub url: String,
    /// When the URL expires (if applicable)
    pub expires_at: Option<DateTime<Utc>>,
    /// Content type of the file
    pub content_type: Option<String>,
}

impl From<EvidenceAccessUrl> for AccessUrlResponse {
    fn from(access: EvidenceAccessUrl) -> Self {
        Self {
            url: access.url,
            expires_at: access.expires_at,
            content_type: access.content_type,
        }
    }
}

/// Response for discovered evidence.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiscoveredEvidenceResponse {
    /// External identifier
    pub external_id: String,
    /// Type of evidence
    pub evidence_type: String,
    /// Display name
    pub name: String,
    /// File size if known
    pub file_size_bytes: Option<i64>,
    /// When discovered
    pub discovered_at: DateTime<Utc>,
    /// Relevance score (0.0 to 1.0)
    pub relevance_score: f32,
    /// Plugin-specific metadata
    pub metadata: serde_json::Value,
}

impl From<DiscoveredEvidence> for DiscoveredEvidenceResponse {
    fn from(d: DiscoveredEvidence) -> Self {
        Self {
            external_id: d.external_id,
            evidence_type: d.evidence_type.to_string(),
            name: d.name,
            file_size_bytes: d.file_size_bytes,
            discovered_at: d.discovered_at,
            relevance_score: d.relevance_score,
            metadata: d.metadata,
        }
    }
}

/// Response for evidence validation.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidationResultResponse {
    /// Whether the evidence validates the claimed result
    pub is_valid: bool,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Extracted result from evidence
    pub extracted_result: Option<ExtractedResultResponse>,
    /// Warning messages
    pub warnings: Vec<String>,
    /// Error messages
    pub errors: Vec<String>,
}

impl From<EvidenceValidation> for ValidationResultResponse {
    fn from(v: EvidenceValidation) -> Self {
        Self {
            is_valid: v.is_valid,
            confidence: v.confidence,
            extracted_result: v.extracted_result.map(|r| ExtractedResultResponse {
                map_id: r.map_id,
                participant1_score: r.participant1_score,
                participant2_score: r.participant2_score,
                duration_seconds: r.duration_seconds,
            }),
            warnings: v.warnings,
            errors: v.errors,
        }
    }
}

/// Extracted result from evidence.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExtractedResultResponse {
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub duration_seconds: i64,
}

/// Summary response for evidence list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EvidenceSummaryResponse {
    pub id: Uuid,
    pub evidence_type: String,
    pub name: String,
    pub status: String,
    pub validated: bool,
    pub created_at: DateTime<Utc>,
}

impl From<Evidence> for EvidenceSummaryResponse {
    fn from(e: Evidence) -> Self {
        Self {
            id: e.id.as_uuid(),
            evidence_type: e.evidence_type.to_string(),
            name: e.name,
            status: e.status.to_string(),
            validated: e.validated,
            created_at: e.created_at,
        }
    }
}

// =============================================================================
// CS2 DEMO VALIDATION RESPONSES
// =============================================================================

/// Demo validation response.
#[derive(Debug, Clone, Serialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DemoStatsResponse {
    /// Demo file name.
    pub demo_name: String,

    /// Map name (e.g., "de_dust2").
    pub map_name: String,

    /// Match date as ISO 8601 string.
    pub match_date: String,

    /// Unique match identifier from the demo.
    pub match_id: String,

    /// Team 1 final score.
    pub team1_score: i32,

    /// Team 2 final score.
    pub team2_score: i32,

    /// Team 1 name from demo.
    pub team1_name: String,

    /// Team 2 name from demo.
    pub team2_name: String,

    /// Total rounds played.
    pub total_rounds: i32,

    /// Player statistics.
    pub players: Vec<DemoPlayerStatsResponse>,

    /// URL to download the demo.
    pub demo_url: String,

    /// URL to view stats JSON.
    pub stats_url: String,
}

/// Player stats from demo.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DemoPlayerStatsResponse {
    /// Steam ID (64-bit as string).
    pub steam_id: String,

    /// Player name during match.
    pub name: String,

    /// Team name.
    pub team: String,

    /// Kills.
    pub kills: i32,

    /// Deaths.
    pub deaths: i32,

    /// Assists.
    pub assists: i32,

    /// Damage dealt.
    pub damage: i32,

    /// Average damage per round.
    pub adr: f64,
}
