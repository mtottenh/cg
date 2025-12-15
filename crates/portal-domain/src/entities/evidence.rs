//! Evidence domain entities.
//!
//! Evidence items are attached to matches to prove results.
//! Types include demo files, screenshots, videos, external links, and server logs.

use chrono::{DateTime, Utc};
use portal_core::{EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

// =============================================================================
// EVIDENCE
// =============================================================================

/// Evidence item for a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EvidenceId,
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,

    /// Type and source
    pub evidence_type: EvidenceType,
    pub evidence_source: EvidenceSource,

    /// Metadata
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,

    /// Storage
    pub storage: EvidenceStorage,

    /// Plugin metadata
    pub plugin_metadata: serde_json::Value,

    /// Validation
    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub validation_result: Option<serde_json::Value>,

    /// Upload info
    pub uploaded_by_registration_id: Option<TournamentRegistrationId>,
    pub uploaded_by_user_id: Option<UserId>,

    /// Discovery info (for plugin-discovered)
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,

    /// Status
    pub status: EvidenceStatus,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl Evidence {
    /// Check if this evidence is accessible.
    #[must_use]
    pub fn is_accessible(&self) -> bool {
        matches!(self.status, EvidenceStatus::Active)
    }

    /// Check if this evidence is expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if this is a file-based evidence type.
    #[must_use]
    pub fn is_file(&self) -> bool {
        matches!(
            self.evidence_type,
            EvidenceType::Demo | EvidenceType::Screenshot | EvidenceType::ServerLog
        )
    }

    /// Check if this is a URL-based evidence type.
    #[must_use]
    pub fn is_url(&self) -> bool {
        matches!(
            self.evidence_type,
            EvidenceType::Video | EvidenceType::Link
        )
    }
}

/// Type of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    /// Game replay/demo file
    Demo,
    /// Screenshot image
    Screenshot,
    /// Video recording (usually external link)
    Video,
    /// External link
    Link,
    /// Game server log
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

/// Source of the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// Manually uploaded by a user
    ManualUpload,
    /// Discovered by a plugin (e.g., S3 demo scan)
    PluginDiscovery,
    /// From a game server integration
    GameServer,
    /// From an external API
    ExternalApi,
}

impl std::fmt::Display for EvidenceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManualUpload => write!(f, "manual_upload"),
            Self::PluginDiscovery => write!(f, "plugin_discovery"),
            Self::GameServer => write!(f, "game_server"),
            Self::ExternalApi => write!(f, "external_api"),
        }
    }
}

impl std::str::FromStr for EvidenceSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "manual_upload" => Ok(Self::ManualUpload),
            "plugin_discovery" => Ok(Self::PluginDiscovery),
            "game_server" => Ok(Self::GameServer),
            "external_api" => Ok(Self::ExternalApi),
            _ => Err(format!("invalid evidence source: {s}")),
        }
    }
}

/// Storage location for evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceStorage {
    /// Stored in S3
    S3 {
        bucket: String,
        key: String,
    },
    /// External URL
    Url {
        url: String,
    },
    /// Inline content (small text content)
    Inline {
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

/// Status of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    /// Active and accessible
    #[default]
    Active,
    /// Expired (past retention period)
    Expired,
    /// Deleted by user
    Deleted,
    /// Quarantined for review
    Quarantined,
}

impl std::fmt::Display for EvidenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Expired => write!(f, "expired"),
            Self::Deleted => write!(f, "deleted"),
            Self::Quarantined => write!(f, "quarantined"),
        }
    }
}

impl std::str::FromStr for EvidenceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "expired" => Ok(Self::Expired),
            "deleted" => Ok(Self::Deleted),
            "quarantined" => Ok(Self::Quarantined),
            _ => Err(format!("invalid evidence status: {s}")),
        }
    }
}

// =============================================================================
// EVIDENCE ACCESS LOG
// =============================================================================

/// Log entry for evidence access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAccessLog {
    pub id: uuid::Uuid,
    pub evidence_id: EvidenceId,
    pub accessed_by_user_id: Option<UserId>,
    pub access_type: EvidenceAccessType,
    pub ip_address: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub accessed_at: DateTime<Utc>,
}

/// Type of evidence access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAccessType {
    /// Viewed evidence metadata
    View,
    /// Downloaded the file
    Download,
    /// Shared the access link
    Share,
}

impl std::fmt::Display for EvidenceAccessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::View => write!(f, "view"),
            Self::Download => write!(f, "download"),
            Self::Share => write!(f, "share"),
        }
    }
}

impl std::str::FromStr for EvidenceAccessType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "view" => Ok(Self::View),
            "download" => Ok(Self::Download),
            "share" => Ok(Self::Share),
            _ => Err(format!("invalid evidence access type: {s}")),
        }
    }
}

// =============================================================================
// UPLOAD INFO
// =============================================================================

/// Information returned when initiating an upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceUploadInfo {
    pub evidence_id: EvidenceId,
    pub upload_url: String,
    pub upload_method: String,
    pub upload_headers: HashMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

/// Information returned when requesting evidence access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAccessUrl {
    pub url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub content_type: Option<String>,
}

// =============================================================================
// COMMANDS
// =============================================================================

/// Command to initiate an evidence upload.
#[derive(Debug, Clone)]
pub struct InitiateEvidenceUploadCommand {
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub evidence_type: EvidenceType,
    pub file_name: String,
    pub file_size_bytes: i64,
    pub mime_type: String,
    pub uploaded_by_user_id: UserId,
}

/// Command to add link evidence.
#[derive(Debug, Clone)]
pub struct AddLinkEvidenceCommand {
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub evidence_type: EvidenceType,
    pub url: String,
    pub name: String,
    pub description: Option<String>,
    pub added_by_user_id: UserId,
}

/// Command to link discovered evidence.
#[derive(Debug, Clone)]
pub struct LinkDiscoveredEvidenceCommand {
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub external_id: String,
    pub linked_by_user_id: UserId,
}

// =============================================================================
// PLUGIN EVIDENCE TYPES
// =============================================================================

/// Discovered evidence from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEvidence {
    /// External identifier for this evidence
    pub external_id: String,
    /// Type of evidence
    pub evidence_type: EvidenceType,
    /// Display name
    pub name: String,
    /// Storage location
    pub storage: EvidenceStorage,
    /// File size if known
    pub file_size_bytes: Option<i64>,
    /// Plugin-specific metadata
    pub metadata: serde_json::Value,
    /// When this was discovered
    pub discovered_at: DateTime<Utc>,
    /// Relevance score (0.0 to 1.0, higher = more likely to be the correct demo)
    pub relevance_score: f32,
}

/// Result of evidence validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceValidation {
    /// Whether the evidence validates the claimed result
    pub is_valid: bool,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Extracted result from the evidence
    pub extracted_result: Option<ExtractedResult>,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
    /// Errors (reasons for invalid)
    pub errors: Vec<String>,
}

/// Result extracted from evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedResult {
    /// Map identifier
    pub map_id: String,
    /// Score for participant 1
    pub participant1_score: i32,
    /// Score for participant 2
    pub participant2_score: i32,
    /// Duration in seconds
    pub duration_seconds: i64,
    /// Game-specific player statistics
    pub player_stats: serde_json::Value,
}

/// Metadata from a demo file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoMetadata {
    /// Map name
    pub map_name: String,
    /// Duration in seconds
    pub duration_seconds: i64,
    /// Number of players
    pub player_count: u32,
    /// Team 1 final score
    pub team1_score: i32,
    /// Team 2 final score
    pub team2_score: i32,
    /// When the demo was recorded
    pub recorded_at: DateTime<Utc>,
    /// Server name if available
    pub server_name: Option<String>,
    /// Demo file format version
    pub demo_version: String,
}

/// Context for evidence discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchEvidenceContext {
    pub tournament_id: portal_core::TournamentId,
    pub match_id: TournamentMatchId,
    pub game_id: portal_core::GameId,
    pub participants: Vec<ParticipantContext>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Participant context for evidence matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantContext {
    pub registration_id: TournamentRegistrationId,
    pub name: String,
    pub player_ids: Vec<portal_core::PlayerId>,
    pub steam_ids: Vec<String>,
}
