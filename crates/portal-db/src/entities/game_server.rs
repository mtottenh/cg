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
