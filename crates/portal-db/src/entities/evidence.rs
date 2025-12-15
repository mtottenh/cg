//! Evidence database entities.
//!
//! These entities map to the evidence-related tables:
//! `match_evidence`, `evidence_access_log`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// =============================================================================
// EVIDENCE
// =============================================================================

/// Database row for the `match_evidence` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EvidenceRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub game_number: Option<i32>,

    // Type and source
    pub evidence_type: String,
    pub evidence_source: String,

    // Metadata
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,

    // Storage location
    pub storage_type: String,
    pub storage_path: Option<String>,
    pub storage_bucket: Option<String>,

    // Plugin metadata
    pub plugin_metadata: serde_json::Value,

    // Validation
    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub validation_result: Option<serde_json::Value>,

    // Upload info
    pub uploaded_by_registration_id: Option<Uuid>,
    pub uploaded_by_user_id: Option<Uuid>,

    // Discovery info
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,

    // Status
    pub status: String,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Data for inserting new evidence.
#[derive(Debug, Clone)]
pub struct NewEvidence {
    pub match_id: Uuid,
    pub game_number: Option<i32>,
    pub evidence_type: String,
    pub evidence_source: String,
    pub name: String,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub storage_type: String,
    pub storage_path: Option<String>,
    pub storage_bucket: Option<String>,
    pub plugin_metadata: serde_json::Value,
    pub uploaded_by_registration_id: Option<Uuid>,
    pub uploaded_by_user_id: Option<Uuid>,
    pub discovered_by_plugin: Option<String>,
    pub discovered_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Data for updating evidence.
#[derive(Debug, Clone, Default)]
pub struct UpdateEvidence {
    pub name: Option<String>,
    pub description: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub mime_type: Option<String>,
    pub storage_type: Option<String>,
    pub storage_path: Option<String>,
    pub storage_bucket: Option<String>,
    pub plugin_metadata: Option<serde_json::Value>,
    pub status: Option<String>,
}

// =============================================================================
// EVIDENCE ACCESS LOG
// =============================================================================

/// Database row for the `evidence_access_log` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EvidenceAccessLogRow {
    pub id: Uuid,
    pub evidence_id: Uuid,
    pub accessed_by_user_id: Option<Uuid>,
    pub access_type: String,
    pub ip_address: Option<String>, // Stored as text, parsed to IpAddr in domain
    pub user_agent: Option<String>,
    pub accessed_at: DateTime<Utc>,
}

/// Data for inserting a new access log entry.
#[derive(Debug, Clone)]
pub struct NewEvidenceAccessLog {
    pub evidence_id: Uuid,
    pub accessed_by_user_id: Option<Uuid>,
    pub access_type: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}
