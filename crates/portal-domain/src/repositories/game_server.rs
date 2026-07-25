//! Game-server repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{GameId, GameServerId, ServerBookingId, TournamentId, UserId};
use portal_core::types::{AgentGamestate, GameServerStatus};
use std::net::IpAddr;

use crate::entities::{AgentCertificate, GameServer, ServerBooking};

/// Fields for registering a new game server.
#[derive(Debug, Clone)]
pub struct CreateGameServer {
    pub id: GameServerId,
    pub name: String,
    pub game_id: GameId,
    pub ip_address: IpAddr,
    pub port: u16,
    pub gotv_port: Option<u16>,
    pub region: String,
}

/// Partial update of a game server's registration fields.
///
/// `None` leaves the column unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateGameServer {
    pub name: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub port: Option<u16>,
    pub gotv_port: Option<Option<u16>>,
    pub region: Option<String>,
    pub enabled: Option<bool>,
}

/// Heartbeat-derived column updates applied atomically.
#[derive(Debug, Clone)]
pub struct RecordHeartbeat {
    pub agent_version: String,
    pub gamestate: Option<AgentGamestate>,
    pub status: GameServerStatus,
    pub at: DateTime<Utc>,
}

/// Repository for registered game servers.
#[async_trait]
pub trait GameServerRepository: Send + Sync + 'static {
    async fn create(&self, server: CreateGameServer) -> Result<GameServer, DomainError>;

    async fn find_by_id(&self, id: GameServerId) -> Result<Option<GameServer>, DomainError>;

    /// All servers, newest first (admin registry view).
    async fn list_all(&self) -> Result<Vec<GameServer>, DomainError>;

    async fn update(
        &self,
        id: GameServerId,
        update: UpdateGameServer,
    ) -> Result<GameServer, DomainError>;

    async fn delete(&self, id: GameServerId) -> Result<(), DomainError>;

    async fn set_status(
        &self,
        id: GameServerId,
        status: GameServerStatus,
    ) -> Result<(), DomainError>;

    /// Store a freshly minted (hashed) enrollment token.
    async fn set_enrollment_token(
        &self,
        id: GameServerId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    /// Resolve an enrollment token hash to its server (unexpired tokens only).
    async fn find_by_enrollment_token_hash(
        &self,
        token_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<GameServer>, DomainError>;

    /// Consume the enrollment token and record the issued cert identity.
    async fn complete_enrollment(
        &self,
        id: GameServerId,
        cert_serial: &str,
        cert_expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    /// Clear the agent cert identity (revocation).
    async fn clear_agent_cert(&self, id: GameServerId) -> Result<(), DomainError>;

    /// Apply a heartbeat's column updates.
    async fn record_heartbeat(
        &self,
        id: GameServerId,
        heartbeat: RecordHeartbeat,
    ) -> Result<(), DomainError>;

    /// Mark every non-offline server without a heartbeat since `cutoff` as
    /// offline. Returns the ids transitioned.
    async fn mark_stale_offline(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<GameServerId>, DomainError>;
}

/// Fields for recording an issued agent certificate.
#[derive(Debug, Clone)]
pub struct CreateAgentCertificate {
    pub server_id: GameServerId,
    pub serial: String,
    pub fingerprint_sha256: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
}

/// Repository for issued agent client certificates.
#[async_trait]
pub trait AgentCertRepository: Send + Sync + 'static {
    async fn create(&self, cert: CreateAgentCertificate)
    -> Result<AgentCertificate, DomainError>;

    /// Look up a certificate by its hex serial (revoked rows included —
    /// callers check [`AgentCertificate::is_valid`]).
    async fn find_by_serial(&self, serial: &str) -> Result<Option<AgentCertificate>, DomainError>;

    /// Revoke every unrevoked certificate for a server. Returns the count.
    async fn revoke_for_server(&self, server_id: GameServerId) -> Result<u64, DomainError>;
}

/// Fields for creating a booking.
#[derive(Debug, Clone)]
pub struct CreateServerBooking {
    pub id: ServerBookingId,
    pub server_id: GameServerId,
    pub tournament_id: Option<TournamentId>,
    pub reason: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: UserId,
}

/// Repository for scheduled server bookings (§6.7).
#[async_trait]
pub trait ServerBookingRepository: Send + Sync + 'static {
    async fn create(&self, booking: CreateServerBooking) -> Result<ServerBooking, DomainError>;

    async fn find_by_id(&self, id: ServerBookingId) -> Result<Option<ServerBooking>, DomainError>;

    /// Bookings for a server whose window ends after `after`, soonest first.
    async fn list_for_server(
        &self,
        server_id: GameServerId,
        after: DateTime<Utc>,
    ) -> Result<Vec<ServerBooking>, DomainError>;

    /// Bookings covering the instant `at` for a server.
    async fn find_covering(
        &self,
        server_id: GameServerId,
        at: DateTime<Utc>,
    ) -> Result<Vec<ServerBooking>, DomainError>;

    async fn delete(&self, id: ServerBookingId) -> Result<(), DomainError>;
}
