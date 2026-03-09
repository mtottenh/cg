//! Shared evidence types.
//!
//! These types are shared between `portal-plugins` and `portal-domain`.
//! Plugin discovers/validates evidence; domain stores and manages it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// =============================================================================
// EVIDENCE TYPE
// =============================================================================

/// Type of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    /// Game replay/demo file.
    Demo,
    /// Screenshot image.
    Screenshot,
    /// Video recording.
    Video,
    /// External link.
    Link,
    /// Game server log.
    ServerLog,
}

impl std::fmt::Display for EvidenceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Demo => write!(f, "demo"),
            Self::Screenshot => write!(f, "screenshot"),
            Self::Video => write!(f, "video"),
            Self::Link => write!(f, "link"),
            Self::ServerLog => write!(f, "server_log"),
        }
    }
}

impl std::str::FromStr for EvidenceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "demo" => Ok(Self::Demo),
            "screenshot" => Ok(Self::Screenshot),
            "video" => Ok(Self::Video),
            "link" => Ok(Self::Link),
            "server_log" => Ok(Self::ServerLog),
            _ => Err(format!("invalid evidence type: {s}")),
        }
    }
}

// =============================================================================
// EVIDENCE STORAGE
// =============================================================================

/// Storage location for evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceStorage {
    /// Stored in S3.
    S3 {
        /// S3 bucket name.
        bucket: String,
        /// S3 object key.
        key: String,
    },
    /// External URL.
    Url {
        /// The URL.
        url: String,
    },
    /// Inline content (small text content).
    Inline {
        /// The content.
        content: String,
    },
}

impl EvidenceStorage {
    /// Get the storage type as a string.
    #[must_use]
    pub fn storage_type(&self) -> &'static str {
        match self {
            Self::S3 { .. } => "s3",
            Self::Url { .. } => "url",
            Self::Inline { .. } => "inline",
        }
    }

    /// Get the storage path (S3 key or URL).
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::S3 { key, .. } => Some(key),
            Self::Url { url } => Some(url),
            Self::Inline { .. } => None,
        }
    }

    /// Get the bucket name for S3 storage.
    #[must_use]
    pub fn bucket(&self) -> Option<&str> {
        match self {
            Self::S3 { bucket, .. } => Some(bucket),
            _ => None,
        }
    }
}

// =============================================================================
// DISCOVERED EVIDENCE
// =============================================================================

/// Evidence discovered by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEvidenceData {
    /// External identifier for this evidence.
    pub external_id: String,
    /// Type of evidence.
    pub evidence_type: EvidenceType,
    /// Display name.
    pub name: String,
    /// Storage location.
    pub storage: EvidenceStorage,
    /// File size if known.
    pub file_size_bytes: Option<i64>,
    /// Plugin-specific metadata.
    pub metadata: Value,
    /// When this was discovered.
    pub discovered_at: DateTime<Utc>,
    /// Relevance score (0.0 to 1.0, higher = more likely to be the correct demo).
    pub relevance_score: f32,
}

// =============================================================================
// EVIDENCE VALIDATION
// =============================================================================

/// Result of evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceValidationResult {
    /// Whether the evidence validates the claimed result.
    pub is_valid: bool,
    /// Confidence level (0.0 to 1.0).
    pub confidence: f32,
    /// Extracted result from the evidence.
    pub extracted_result: Option<ExtractedMatchResult>,
    /// Warnings (non-fatal issues).
    pub warnings: Vec<String>,
    /// Errors (reasons for invalid).
    pub errors: Vec<String>,
}

/// Result extracted from evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMatchResult {
    /// Map identifier.
    pub map_id: String,
    /// Score for participant 1.
    pub participant1_score: i32,
    /// Score for participant 2.
    pub participant2_score: i32,
    /// Duration in seconds.
    pub duration_seconds: i64,
    /// Game-specific player statistics.
    pub player_stats: Value,
}

/// Metadata from a demo file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoFileMetadata {
    /// Map name.
    pub map_name: String,
    /// Duration in seconds.
    pub duration_seconds: i64,
    /// Number of players.
    pub player_count: u32,
    /// Team 1 final score.
    pub team1_score: i32,
    /// Team 2 final score.
    pub team2_score: i32,
    /// When the demo was recorded.
    pub recorded_at: DateTime<Utc>,
    /// Server name if available.
    pub server_name: Option<String>,
    /// Demo file format version.
    pub demo_version: String,
}

// =============================================================================
// MATCH EVIDENCE CONTEXT
// =============================================================================

/// Context for evidence discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchEvidenceContext {
    /// Tournament ID.
    pub tournament_id: Uuid,
    /// Match ID.
    pub match_id: Uuid,
    /// Game identifier (e.g., "cs2").
    pub game_id: String,
    /// Participants in the match.
    pub participants: Vec<ParticipantEvidenceContext>,
    /// When the match was scheduled.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// When the match started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the match completed.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Context for a match participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantEvidenceContext {
    /// Registration ID.
    pub registration_id: Uuid,
    /// Display name.
    pub name: String,
    /// Player IDs (for team registration).
    pub player_ids: Vec<Uuid>,
    /// Steam IDs (for CS2, etc.).
    pub steam_ids: Vec<String>,
}

/// A claimed game result for evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMatchResult {
    /// Game number in series.
    pub game_number: i32,
    /// Map ID.
    pub map_id: Option<String>,
    /// Participant 1 score.
    pub participant1_score: i32,
    /// Participant 2 score.
    pub participant2_score: i32,
}
