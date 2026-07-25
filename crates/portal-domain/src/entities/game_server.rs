//! Game-server domain entities.
//!
//! A [`GameServer`] is a registered CS2 host running MatchZy plus the
//! portal-server-agent. The portal never stores RCON credentials; its only
//! control channel is the agent's outbound mTLS connection, identified by
//! an issued [`AgentCertificate`]. Design: docs/matchzy-integration.md §4.

use chrono::{DateTime, Utc};
use portal_core::ids::{
    GameId, GameServerId, MatchSubstitutionId, PlayerId, ServerAgentCertId, ServerBookingId,
    ServerEventId, ServerReservationId, TournamentId, TournamentMatchId, TournamentRegistrationId,
    UserId,
};
use portal_core::types::{AgentGamestate, GameServerStatus, ReservationStatus, SubstitutionStatus};
use std::net::IpAddr;

/// A registered game server.
#[derive(Debug, Clone)]
pub struct GameServer {
    pub id: GameServerId,
    pub name: String,
    pub game_id: GameId,

    /// Address players connect to (`connect ip:port`).
    pub ip_address: IpAddr,
    pub port: u16,
    pub gotv_port: Option<u16>,
    pub region: String,

    /// Admin kill-switch; a disabled server is never allocated.
    pub enabled: bool,
    pub status: GameServerStatus,
    pub current_match_id: Option<TournamentMatchId>,

    /// Serial of the currently-valid agent client certificate.
    pub agent_cert_serial: Option<String>,
    pub agent_cert_expires_at: Option<DateTime<Utc>>,
    pub agent_version: Option<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub last_gamestate: Option<AgentGamestate>,

    /// One-time enrollment token (SHA-256 hex) and its expiry.
    pub enrollment_token_hash: Option<String>,
    pub enrollment_token_expires_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GameServer {
    /// The `ip:port` string players use to connect.
    #[must_use]
    pub fn connect_address(&self) -> String {
        format!("{}:{}", self.ip_address, self.port)
    }

    /// Whether a heartbeat has been seen within `staleness`.
    #[must_use]
    pub fn heartbeat_fresh(&self, staleness: chrono::Duration, now: DateTime<Utc>) -> bool {
        self.last_heartbeat_at
            .is_some_and(|at| now - at <= staleness)
    }

    /// Whether the server currently holds a valid (unexpired) enrollment token.
    #[must_use]
    pub fn enrollment_open(&self, now: DateTime<Utc>) -> bool {
        self.enrollment_token_hash.is_some()
            && self.enrollment_token_expires_at.is_some_and(|at| at > now)
    }
}

/// An issued agent client certificate (§4.1).
///
/// Caddy verifies chain + validity against the portal CA; the API resolves
/// the presented serial to a row here for revocation and server binding.
#[derive(Debug, Clone)]
pub struct AgentCertificate {
    pub id: ServerAgentCertId,
    pub server_id: GameServerId,
    /// Hex-encoded certificate serial number.
    pub serial: String,
    /// SHA-256 fingerprint of the DER certificate, hex-encoded.
    pub fingerprint_sha256: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub issued_at: DateTime<Utc>,
}

impl AgentCertificate {
    /// Whether this certificate authenticates an agent right now.
    #[must_use]
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.not_before <= now && now < self.not_after
    }
}

/// A scheduled event hold on a server (§6.7).
///
/// A booking scoped to a tournament reserves the server for that event's
/// matches; a `tournament_id: None` booking is a hard hold (maintenance,
/// community night) excluding the server from all allocation in the window.
#[derive(Debug, Clone)]
pub struct ServerBooking {
    pub id: ServerBookingId,
    pub server_id: GameServerId,
    pub tournament_id: Option<TournamentId>,
    pub reason: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl ServerBooking {
    /// Whether the booking window covers `at`.
    #[must_use]
    pub fn covers(&self, at: DateTime<Utc>) -> bool {
        self.starts_at <= at && at < self.ends_at
    }
}

/// A heartbeat report from a connected agent.
#[derive(Debug, Clone)]
pub struct HeartbeatUpdate {
    pub agent_version: String,
    /// Whether the agent could reach the game server's RCON.
    pub rcon_ok: bool,
    /// MatchZy gamestate from `get5_status` (None when RCON was unreachable).
    pub gamestate: Option<AgentGamestate>,
    /// The `matchid` MatchZy reports as loaded, if any.
    pub reported_matchzy_id: Option<i64>,
}

/// A match's claim on a server for one play-through (§4).
///
/// `matchzy_id` is the integer `matchid` MatchZy requires; it is also the
/// correlation key on every webhook event and the config-fetch URL.
#[derive(Debug, Clone)]
pub struct ServerReservation {
    pub id: ServerReservationId,
    /// `None` while queued (`pending`) — allocation assigns the server.
    pub server_id: Option<GameServerId>,
    pub match_id: TournamentMatchId,
    pub matchzy_id: i64,
    pub status: ReservationStatus,

    /// `sv_password` players use; shown only to participants + admins.
    pub connect_password: String,
    pub gotv_password: Option<String>,
    /// SHA-256 of the one-time config-fetch bearer token.
    pub config_token_hash: String,
    /// SHA-256 of the per-reservation event-webhook bearer token.
    pub event_token_hash: String,
    pub config_token_expires_at: DateTime<Utc>,

    /// The exact MatchZy config served (audit + rebuild-on-reassignment).
    pub match_config: Option<serde_json::Value>,

    pub config_fetched_at: Option<DateTime<Utc>>,
    pub went_live_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub retry_count: i32,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ServerReservation {
    /// Whether the config endpoint may still serve this reservation's config.
    #[must_use]
    pub fn config_fetchable(&self, now: DateTime<Utc>) -> bool {
        matches!(
            self.status,
            ReservationStatus::Configuring | ReservationStatus::Ready
        ) && self.config_token_expires_at > now
    }
}

/// A raw MatchZy webhook event, stored before processing (§6.4).
#[derive(Debug, Clone)]
pub struct ServerEvent {
    pub id: ServerEventId,
    pub reservation_id: Option<ServerReservationId>,
    pub server_id: Option<GameServerId>,
    pub event_type: String,
    pub map_number: Option<i32>,
    pub round_number: Option<i32>,
    pub payload: serde_json::Value,
    pub processed: bool,
    pub processed_at: Option<DateTime<Utc>>,
    pub processing_error: Option<String>,
    pub received_at: DateTime<Utc>,
}

/// A mid-series substitution request (§6.8).
///
/// A substitution creates new effective-roster state from
/// `from_game_number` onward; history is never rewritten (game 1's demo
/// still validates against the original five).
#[derive(Debug, Clone)]
pub struct MatchSubstitution {
    pub id: MatchSubstitutionId,
    pub match_id: TournamentMatchId,
    /// The side being substituted.
    pub registration_id: TournamentRegistrationId,
    pub reservation_id: Option<ServerReservationId>,
    pub player_out_id: PlayerId,
    /// `None` = play short-handed.
    pub player_in_id: Option<PlayerId>,
    pub from_game_number: i32,
    pub status: SubstitutionStatus,
    pub requested_by: UserId,
    pub approved_by: Option<UserId>,
    pub failure_reason: Option<String>,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
