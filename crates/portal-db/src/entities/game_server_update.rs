//! Game-server update database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `game_server_update_jobs` table.
#[derive(Debug, Clone, FromRow)]
pub struct GameServerUpdateJobRow {
    pub id: Uuid,
    pub server_id: Uuid,
    pub operation: String,
    pub status: String,
    pub trigger: String,
    pub requested_by: Option<Uuid>,
    pub forced: bool,
    pub phase: Option<String>,
    pub failed_phase: Option<String>,
    pub before_versions: Option<serde_json::Value>,
    pub after_versions: Option<serde_json::Value>,
    pub latest_versions: Option<serde_json::Value>,
    pub load: Option<serde_json::Value>,
    pub config_report: Option<serde_json::Value>,
    pub restarted: Option<bool>,
    pub error: Option<String>,
    pub log_tail: Option<String>,
    pub agent_synthesized: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// The update columns of a `game_servers` row.
#[derive(Debug, Clone, FromRow)]
pub struct GameServerUpdateStateRow {
    pub id: Uuid,
    pub agent_capabilities: Vec<String>,
    pub update_check: Option<serde_json::Value>,
    pub update_checked_at: Option<DateTime<Utc>>,
    pub installed_versions: Option<serde_json::Value>,
    pub demo_upload_wired: Option<bool>,
    pub overrides_applied: Option<bool>,
    pub reload_pending: bool,
    pub agent_update_job: Option<serde_json::Value>,
    pub matchzy_overrides: serde_json::Value,
    pub matchzy_overrides_pending: bool,
}
