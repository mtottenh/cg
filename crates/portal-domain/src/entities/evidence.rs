//! Evidence domain entities.
//!
//! Evidence items are attached to matches to prove results.
//! Types include demo files, screenshots, videos, external links, and server logs.

use chrono::{DateTime, Utc};
use portal_core::{EvidenceId, TournamentMatchId, TournamentRegistrationId, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

// Re-export shared types from portal-core
pub use portal_core::types::evidence::{
    DemoFileMetadata as DemoMetadata, DiscoveredEvidenceData as DiscoveredEvidence,
    EvidenceStorage, EvidenceType, EvidenceValidationResult as EvidenceValidation,
    ExtractedMatchResult as ExtractedResult, MatchEvidenceContext,
    ParticipantEvidenceContext as ParticipantContext,
};

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
        matches!(self.evidence_type, EvidenceType::Video | EvidenceType::Link)
    }
}

// EvidenceType is re-exported from portal-core above.

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

// EvidenceStorage is re-exported from portal-core above.

/// Status of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    /// Upload initiated, file not yet confirmed
    Pending,
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
            Self::Pending => write!(f, "pending"),
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
            "pending" => Ok(Self::Pending),
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

// DiscoveredEvidence, EvidenceValidation, ExtractedResult, DemoMetadata,
// MatchEvidenceContext, and ParticipantContext are all re-exported from portal-core above.
