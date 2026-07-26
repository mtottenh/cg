//! Game-server repository traits.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    GameId, GameServerId, MatchSubstitutionId, PlayerId, ServerBookingId, ServerEventId,
    ServerReservationId, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId,
};
use portal_core::types::{AgentGamestate, GameServerStatus, ReservationStatus, SubstitutionStatus};
use std::net::IpAddr;

use crate::entities::{
    AgentCertificate, GameServer, MatchSubstitution, ServerBooking, ServerEvent, ServerReservation,
};

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

    /// Store the server-scoped demo-upload token hash (minted at
    /// enrollment; lives in the server's permanent MatchZy config, §6.3).
    async fn set_demo_token(&self, id: GameServerId, token_hash: &str) -> Result<(), DomainError>;

    /// Resolve a demo-upload token hash to its server.
    async fn find_by_demo_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<GameServer>, DomainError>;

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
    async fn create(&self, cert: CreateAgentCertificate) -> Result<AgentCertificate, DomainError>;

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

/// Fields for creating a reservation (tokens pre-hashed by the service).
#[derive(Debug, Clone)]
pub struct CreateServerReservation {
    pub id: ServerReservationId,
    pub match_id: TournamentMatchId,
    pub connect_password: String,
    pub gotv_password: Option<String>,
    pub config_token_hash: String,
    pub event_token_hash: String,
    pub config_token_expires_at: DateTime<Utc>,
}

/// Repository for match server reservations.
#[async_trait]
pub trait ServerReservationRepository: Send + Sync + 'static {
    /// Create a queued (`pending`, serverless) reservation. Fails on the
    /// one-live-reservation-per-match unique index if one already exists.
    async fn create_pending(
        &self,
        create: CreateServerReservation,
    ) -> Result<ServerReservation, DomainError>;

    /// Atomically pick an eligible server (§6.7 predicate, `FOR UPDATE
    /// SKIP LOCKED`), assign it to the pending reservation, and mark the
    /// server `reserved` with `current_match_id` set — one transaction.
    ///
    /// Eligibility: enabled, `available`, matching game, fresh heartbeat,
    /// idle gamestate, and no booking covering `now` for a different
    /// tournament. Servers with a booking for `tournament_id` sort first.
    /// Returns `None` when no server qualifies (reservation untouched).
    async fn allocate(
        &self,
        id: ServerReservationId,
        game_id: GameId,
        tournament_id: TournamentId,
        region: Option<&str>,
        heartbeat_cutoff: DateTime<Utc>,
        now: DateTime<Utc>,
        scheduled_at: Option<DateTime<Utc>>,
    ) -> Result<Option<(ServerReservation, GameServer)>, DomainError>;

    /// Terminalize a reservation and free its server in one transaction:
    /// the reservation gets `final_status` (+ optional reason), and the
    /// server — if this reservation holds it — returns to `available`
    /// with `current_match_id` cleared.
    async fn release(
        &self,
        id: ServerReservationId,
        final_status: ReservationStatus,
        reason: Option<&str>,
    ) -> Result<(), DomainError>;

    async fn find_by_id(
        &self,
        id: ServerReservationId,
    ) -> Result<Option<ServerReservation>, DomainError>;

    async fn find_by_matchzy_id(
        &self,
        matchzy_id: i64,
    ) -> Result<Option<ServerReservation>, DomainError>;

    /// The live (pending/configuring/ready/live) reservation for a match.
    async fn find_live_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ServerReservation>, DomainError>;

    /// Most recent reservation for a match regardless of status.
    async fn find_latest_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ServerReservation>, DomainError>;

    /// Whether the server currently holds a live reservation.
    async fn has_live_for_server(&self, server_id: GameServerId) -> Result<bool, DomainError>;

    /// The live reservation currently holding this server, if any.
    async fn find_live_by_server(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<ServerReservation>, DomainError>;

    /// `applying` substitution retry sweep support: not here — see
    /// [`MatchSubstitutionRepository::list_applying`].
    ///
    /// Count of queued reservations created before `before` (queue position).
    async fn count_pending_before(&self, before: DateTime<Utc>) -> Result<i64, DomainError>;

    async fn set_status(
        &self,
        id: ServerReservationId,
        status: ReservationStatus,
    ) -> Result<(), DomainError>;

    /// Terminal failure with a human-readable reason.
    async fn mark_failed(&self, id: ServerReservationId, reason: &str) -> Result<(), DomainError>;

    /// Replace the config-fetch token (a fresh one is minted per load
    /// attempt, §6.6).
    async fn set_config_token(
        &self,
        id: ServerReservationId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    /// Store the generated MatchZy config JSON.
    async fn store_config(
        &self,
        id: ServerReservationId,
        config: &serde_json::Value,
    ) -> Result<(), DomainError>;

    async fn mark_config_fetched(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    async fn mark_live(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    async fn mark_completed(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError>;

    /// Bump the retry counter, returning the new value.
    async fn increment_retry(&self, id: ServerReservationId) -> Result<i32, DomainError>;

    /// Touch `updated_at` (event-activity marker for staleness detection).
    async fn touch(&self, id: ServerReservationId) -> Result<(), DomainError>;

    /// Reservations still waiting for a server.
    async fn list_pending(&self, limit: i64) -> Result<Vec<ServerReservation>, DomainError>;

    /// `configuring` reservations not updated since `before` (load retry).
    async fn list_stuck_configuring(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ServerReservation>, DomainError>;

    /// `ready`/`live` reservations with no activity since `before`
    /// (missed-webhook reconciliation, §6.6).
    async fn list_active_stale(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ServerReservation>, DomainError>;
}

/// Fields for inserting a raw webhook event.
#[derive(Debug, Clone)]
pub struct CreateServerEvent {
    pub reservation_id: Option<ServerReservationId>,
    pub server_id: GameServerId,
    pub event_type: String,
    pub map_number: Option<i32>,
    pub round_number: Option<i32>,
    pub payload: serde_json::Value,
}

/// Repository for raw MatchZy webhook events.
#[async_trait]
pub trait ServerEventRepository: Send + Sync + 'static {
    /// Insert an event. Returns `None` when the dedupe index rejects it
    /// (same reservation + type + map + round already stored) — MatchZy
    /// replays become no-ops (§6.4).
    async fn insert(&self, event: CreateServerEvent) -> Result<Option<ServerEvent>, DomainError>;

    async fn mark_processed(
        &self,
        id: ServerEventId,
        error: Option<&str>,
    ) -> Result<(), DomainError>;

    /// Latest round_end payload for a reservation (live-score snapshot).
    async fn latest_round_end(
        &self,
        reservation_id: ServerReservationId,
    ) -> Result<Option<ServerEvent>, DomainError>;

    /// Record a processing error WITHOUT marking processed (retried by
    /// the lifecycle sweep, §6.4).
    async fn record_processing_error(
        &self,
        id: ServerEventId,
        error: &str,
    ) -> Result<(), DomainError>;

    /// Oldest unprocessed events (excluding `backup_uploaded`, which is
    /// storage-indexing, not pipeline work).
    async fn list_unprocessed(&self, limit: i64) -> Result<Vec<ServerEvent>, DomainError>;

    /// Storage key of an uploaded round backup, by filename.
    async fn find_backup_key(
        &self,
        reservation_id: ServerReservationId,
        filename: &str,
    ) -> Result<Option<String>, DomainError>;

    /// The most recent `backup_uploaded` event for a reservation, optionally
    /// bounded to rounds `<= before_round` (restore target selection).
    async fn latest_backup(
        &self,
        reservation_id: ServerReservationId,
        before_round: Option<i32>,
    ) -> Result<Option<ServerEvent>, DomainError>;
}

/// Fields for creating a substitution request.
#[derive(Debug, Clone)]
pub struct CreateMatchSubstitution {
    pub id: MatchSubstitutionId,
    pub match_id: TournamentMatchId,
    pub registration_id: TournamentRegistrationId,
    pub reservation_id: Option<ServerReservationId>,
    pub player_out_id: PlayerId,
    pub player_in_id: Option<PlayerId>,
    pub from_game_number: i32,
    pub status: SubstitutionStatus,
    pub requested_by: UserId,
}

/// Repository for mid-series substitutions (§6.8).
#[async_trait]
pub trait MatchSubstitutionRepository: Send + Sync + 'static {
    /// Create a request. The partial unique index rejects a second live
    /// request for the same outgoing player.
    async fn create(&self, sub: CreateMatchSubstitution) -> Result<MatchSubstitution, DomainError>;

    async fn find_by_id(
        &self,
        id: MatchSubstitutionId,
    ) -> Result<Option<MatchSubstitution>, DomainError>;

    async fn list_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchSubstitution>, DomainError>;

    /// Applied substitutions for a match (effective-roster computation).
    async fn list_applied_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchSubstitution>, DomainError>;

    /// All `applying` rows (lifecycle retry sweep).
    async fn list_applying(&self, limit: i64) -> Result<Vec<MatchSubstitution>, DomainError>;

    /// `applying` rows for a reservation (halftime retry, §6.8).
    async fn list_applying_by_reservation(
        &self,
        reservation_id: ServerReservationId,
    ) -> Result<Vec<MatchSubstitution>, DomainError>;

    async fn set_status(
        &self,
        id: MatchSubstitutionId,
        status: SubstitutionStatus,
        failure_reason: Option<&str>,
        approved_by: Option<UserId>,
    ) -> Result<(), DomainError>;

    async fn mark_applied(
        &self,
        id: MatchSubstitutionId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError>;
}
