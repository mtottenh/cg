//! Game-server database entities.
//!
//! `ip_address` is selected as text (`host(ip_address)`) so no INET-mapping
//! crate is needed; inserts cast the bound string with `::inet`.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `game_servers` table.
#[derive(Debug, Clone, FromRow)]
pub struct GameServerRow {
    pub id: Uuid,
    pub name: String,
    pub game_id: Uuid,
    /// `host(ip_address)` — always the bare address, no prefix length.
    pub ip_address: String,
    pub port: i32,
    pub gotv_port: Option<i32>,
    pub region: String,
    pub enabled: bool,
    pub allow_pugs: bool,
    pub status: String,
    pub current_match_id: Option<Uuid>,
    pub agent_cert_serial: Option<String>,
    pub agent_cert_expires_at: Option<DateTime<Utc>>,
    pub agent_version: Option<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub last_gamestate: Option<String>,
    pub enrollment_token_hash: Option<String>,
    pub enrollment_token_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Database row for the `server_agent_certs` table.
#[derive(Debug, Clone, FromRow)]
pub struct ServerAgentCertRow {
    pub id: Uuid,
    pub server_id: Uuid,
    pub serial: String,
    pub fingerprint_sha256: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub issued_at: DateTime<Utc>,
}

/// Database row for the `server_bookings` table.
#[derive(Debug, Clone, FromRow)]
pub struct ServerBookingRow {
    pub id: Uuid,
    pub server_id: Uuid,
    pub tournament_id: Option<Uuid>,
    pub reason: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Database row for the `server_reservations` table.
#[derive(Debug, Clone, FromRow)]
pub struct ServerReservationRow {
    pub id: Uuid,
    pub server_id: Option<Uuid>,
    pub match_id: Uuid,
    pub matchzy_id: i64,
    pub status: String,
    pub reservation_kind: String,
    pub connect_password: String,
    pub gotv_password: Option<String>,
    pub config_token_hash: String,
    pub event_token_hash: String,
    pub config_token_expires_at: DateTime<Utc>,
    pub match_config: Option<serde_json::Value>,
    pub config_fetched_at: Option<DateTime<Utc>>,
    pub config_fetch_count: i32,
    pub went_live_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Database row for the `server_events` table.
#[derive(Debug, Clone, FromRow)]
pub struct ServerEventRow {
    pub id: Uuid,
    pub reservation_id: Option<Uuid>,
    pub server_id: Option<Uuid>,
    pub event_type: String,
    pub map_number: Option<i32>,
    pub round_number: Option<i32>,
    pub payload: serde_json::Value,
    pub processed: bool,
    pub processed_at: Option<DateTime<Utc>>,
    pub processing_error: Option<String>,
    pub received_at: DateTime<Utc>,
}

/// Database row for the `match_substitutions` table.
#[derive(Debug, Clone, FromRow)]
pub struct MatchSubstitutionRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub registration_id: Uuid,
    pub reservation_id: Option<Uuid>,
    pub player_out_id: Uuid,
    pub player_in_id: Option<Uuid>,
    pub from_game_number: i32,
    pub status: String,
    pub requested_by: Uuid,
    pub approved_by: Option<Uuid>,
    pub failure_reason: Option<String>,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
