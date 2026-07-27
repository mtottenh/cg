//! Game-server repository adapters.

use crate::DbPool;
use crate::entities::{
    GameServerRow, MatchSubstitutionRow, ServerAgentCertRow, ServerBookingRow, ServerEventRow,
    ServerReservationRow,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    GameId, GameServerId, MatchSubstitutionId, PlayerId, ServerAgentCertId, ServerBookingId,
    ServerEventId, ServerReservationId, TournamentId, TournamentMatchId, TournamentRegistrationId,
    UserId,
};
use portal_core::types::{GameServerStatus, ReservationStatus, SubstitutionStatus};
use portal_domain::entities::{
    AgentCertificate, GameServer, MatchSubstitution, ServerBooking, ServerEvent, ServerReservation,
};
use portal_domain::repositories::{
    AgentCertRepository, CreateAgentCertificate, CreateGameServer, CreateMatchSubstitution,
    CreateServerBooking, CreateServerEvent, CreateServerReservation, GameServerRepository,
    MatchSubstitutionRepository, RecordHeartbeat, ServerBookingRepository, ServerEventRepository,
    ServerReservationRepository, UpdateGameServer,
};

/// Explicit column list: `ip_address` must go through `host()` to come back
/// as text, so `SELECT *` is not usable on this table.
const SERVER_COLS: &str = "id, name, game_id, host(ip_address) AS ip_address, port, gotv_port, \
     region, enabled, allow_pugs, status, current_match_id, agent_cert_serial, \
     agent_cert_expires_at, agent_version, last_heartbeat_at, last_gamestate, \
     enrollment_token_hash, enrollment_token_expires_at, created_at, updated_at";

// =============================================================================
// Type Conversions
// =============================================================================

impl From<GameServerRow> for GameServer {
    fn from(row: GameServerRow) -> Self {
        Self {
            id: GameServerId::from(row.id),
            name: row.name,
            game_id: GameId::from(row.game_id),
            // host(ip_address) always yields a parseable address; a failure
            // here means DB corruption, surfaced as the unspecified address
            // rather than a panic in a From impl.
            ip_address: row
                .ip_address
                .parse()
                .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
            port: u16::try_from(row.port).unwrap_or_default(),
            gotv_port: row.gotv_port.and_then(|p| u16::try_from(p).ok()),
            region: row.region,
            enabled: row.enabled,
            allow_pugs: row.allow_pugs,
            status: row.status.parse().unwrap_or_default(),
            current_match_id: row.current_match_id.map(TournamentMatchId::from),
            agent_cert_serial: row.agent_cert_serial,
            agent_cert_expires_at: row.agent_cert_expires_at,
            agent_version: row.agent_version,
            last_heartbeat_at: row.last_heartbeat_at,
            last_gamestate: row
                .last_gamestate
                .as_deref()
                .map(portal_core::types::AgentGamestate::parse_lenient),
            enrollment_token_hash: row.enrollment_token_hash,
            enrollment_token_expires_at: row.enrollment_token_expires_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<ServerAgentCertRow> for AgentCertificate {
    fn from(row: ServerAgentCertRow) -> Self {
        Self {
            id: ServerAgentCertId::from(row.id),
            server_id: GameServerId::from(row.server_id),
            serial: row.serial,
            fingerprint_sha256: row.fingerprint_sha256,
            not_before: row.not_before,
            not_after: row.not_after,
            revoked_at: row.revoked_at,
            issued_at: row.issued_at,
        }
    }
}

impl From<ServerBookingRow> for ServerBooking {
    fn from(row: ServerBookingRow) -> Self {
        Self {
            id: ServerBookingId::from(row.id),
            server_id: GameServerId::from(row.server_id),
            tournament_id: row.tournament_id.map(TournamentId::from),
            reason: row.reason,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            created_by: UserId::from(row.created_by),
            created_at: row.created_at,
        }
    }
}

fn internal(e: sqlx::Error) -> DomainError {
    DomainError::Internal(e.to_string())
}

// =============================================================================
// Game Server Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `GameServerRepository` trait.
#[derive(Clone)]
pub struct PgGameServerRepository {
    pool: DbPool,
}

impl PgGameServerRepository {
    /// Create a new PostgreSQL game server repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GameServerRepository for PgGameServerRepository {
    async fn create(&self, server: CreateGameServer) -> Result<GameServer, DomainError> {
        let sql = format!(
            "INSERT INTO game_servers (id, name, game_id, ip_address, port, gotv_port, region) \
             VALUES ($1, $2, $3, $4::inet, $5, $6, $7) \
             RETURNING {SERVER_COLS}"
        );
        let row = sqlx::query_as::<_, GameServerRow>(&sql)
            .bind(server.id.as_uuid())
            .bind(&server.name)
            .bind(server.game_id.as_uuid())
            .bind(server.ip_address.to_string())
            .bind(i32::from(server.port))
            .bind(server.gotv_port.map(i32::from))
            .bind(&server.region)
            .fetch_one(&self.pool)
            .await
            .map_err(internal)?;
        Ok(GameServer::from(row))
    }

    async fn find_by_id(&self, id: GameServerId) -> Result<Option<GameServer>, DomainError> {
        let sql = format!("SELECT {SERVER_COLS} FROM game_servers WHERE id = $1");
        let row = sqlx::query_as::<_, GameServerRow>(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        Ok(row.map(GameServer::from))
    }

    async fn list_all(&self) -> Result<Vec<GameServer>, DomainError> {
        let sql = format!("SELECT {SERVER_COLS} FROM game_servers ORDER BY created_at DESC");
        let rows = sqlx::query_as::<_, GameServerRow>(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(internal)?;
        Ok(rows.into_iter().map(GameServer::from).collect())
    }

    async fn update(
        &self,
        id: GameServerId,
        update: UpdateGameServer,
    ) -> Result<GameServer, DomainError> {
        // gotv_port distinguishes "unchanged" (outer None) from "set NULL"
        // (Some(None)), which COALESCE cannot express — hence the CASE.
        let sql = format!(
            "UPDATE game_servers SET \
                name = COALESCE($2, name), \
                ip_address = COALESCE($3::inet, ip_address), \
                port = COALESCE($4, port), \
                gotv_port = CASE WHEN $5 THEN $6 ELSE gotv_port END, \
                region = COALESCE($7, region), \
                enabled = COALESCE($8, enabled), \
                allow_pugs = COALESCE($9, allow_pugs), \
                updated_at = NOW() \
             WHERE id = $1 \
             RETURNING {SERVER_COLS}"
        );
        let row = sqlx::query_as::<_, GameServerRow>(&sql)
            .bind(id.as_uuid())
            .bind(update.name)
            .bind(update.ip_address.map(|ip| ip.to_string()))
            .bind(update.port.map(i32::from))
            .bind(update.gotv_port.is_some())
            .bind(update.gotv_port.flatten().map(i32::from))
            .bind(update.region)
            .bind(update.enabled)
            .bind(update.allow_pugs)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        row.map(GameServer::from)
            .ok_or(DomainError::GameServerNotFound(id))
    }

    async fn delete(&self, id: GameServerId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM game_servers WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                if e.to_string().contains("foreign key") {
                    DomainError::Conflict(
                        "server has reservation history; disable it instead of deleting".into(),
                    )
                } else {
                    internal(e)
                }
            })?;
        if result.rows_affected() == 0 {
            return Err(DomainError::GameServerNotFound(id));
        }
        Ok(())
    }

    async fn set_status(
        &self,
        id: GameServerId,
        status: GameServerStatus,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE game_servers SET status = $2, updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status.to_string())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(())
    }

    async fn set_enrollment_token(
        &self,
        id: GameServerId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET enrollment_token_hash = $2, \
             enrollment_token_expires_at = $3, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn find_by_enrollment_token_hash(
        &self,
        token_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<GameServer>, DomainError> {
        let sql = format!(
            "SELECT {SERVER_COLS} FROM game_servers \
             WHERE enrollment_token_hash = $1 AND enrollment_token_expires_at > $2"
        );
        let row = sqlx::query_as::<_, GameServerRow>(&sql)
            .bind(token_hash)
            .bind(now)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        Ok(row.map(GameServer::from))
    }

    async fn complete_enrollment(
        &self,
        id: GameServerId,
        cert_serial: &str,
        cert_expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET \
                agent_cert_serial = $2, agent_cert_expires_at = $3, \
                enrollment_token_hash = NULL, enrollment_token_expires_at = NULL, \
                updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(cert_serial)
        .bind(cert_expires_at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn clear_agent_cert(&self, id: GameServerId) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET agent_cert_serial = NULL, agent_cert_expires_at = NULL, \
             status = 'offline', updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn set_demo_token(&self, id: GameServerId, token_hash: &str) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET demo_token_hash = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(token_hash)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn find_by_demo_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<GameServer>, DomainError> {
        let sql = format!("SELECT {SERVER_COLS} FROM game_servers WHERE demo_token_hash = $1");
        let row = sqlx::query_as::<_, GameServerRow>(&sql)
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(internal)?;
        Ok(row.map(GameServer::from))
    }

    async fn record_heartbeat(
        &self,
        id: GameServerId,
        heartbeat: RecordHeartbeat,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE game_servers SET \
                agent_version = $2, last_heartbeat_at = $3, last_gamestate = $4, \
                status = $5, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(&heartbeat.agent_version)
        .bind(heartbeat.at)
        .bind(heartbeat.gamestate.map(|g| g.to_string()))
        .bind(heartbeat.status.to_string())
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn mark_stale_offline(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<GameServerId>, DomainError> {
        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "UPDATE game_servers SET status = 'offline', updated_at = NOW() \
             WHERE status <> 'offline' \
               AND (last_heartbeat_at IS NULL OR last_heartbeat_at < $1) \
             RETURNING id",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows
            .into_iter()
            .map(|(id,)| GameServerId::from(id))
            .collect())
    }
}

// =============================================================================
// Agent Certificate Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `AgentCertRepository` trait.
#[derive(Clone)]
pub struct PgAgentCertRepository {
    pool: DbPool,
}

impl PgAgentCertRepository {
    /// Create a new PostgreSQL agent certificate repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentCertRepository for PgAgentCertRepository {
    async fn create(&self, cert: CreateAgentCertificate) -> Result<AgentCertificate, DomainError> {
        let row = sqlx::query_as::<_, ServerAgentCertRow>(
            "INSERT INTO server_agent_certs \
                (id, server_id, serial, fingerprint_sha256, not_before, not_after) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING *",
        )
        .bind(ServerAgentCertId::new().as_uuid())
        .bind(cert.server_id.as_uuid())
        .bind(&cert.serial)
        .bind(&cert.fingerprint_sha256)
        .bind(cert.not_before)
        .bind(cert.not_after)
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        Ok(AgentCertificate::from(row))
    }

    async fn find_by_serial(&self, serial: &str) -> Result<Option<AgentCertificate>, DomainError> {
        let row = sqlx::query_as::<_, ServerAgentCertRow>(
            "SELECT * FROM server_agent_certs WHERE serial = $1",
        )
        .bind(serial)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(AgentCertificate::from))
    }

    async fn revoke_for_server(&self, server_id: GameServerId) -> Result<u64, DomainError> {
        let result = sqlx::query(
            "UPDATE server_agent_certs SET revoked_at = NOW() \
             WHERE server_id = $1 AND revoked_at IS NULL",
        )
        .bind(server_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(result.rows_affected())
    }
}

// =============================================================================
// Server Booking Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `ServerBookingRepository` trait.
#[derive(Clone)]
pub struct PgServerBookingRepository {
    pool: DbPool,
}

impl PgServerBookingRepository {
    /// Create a new PostgreSQL server booking repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServerBookingRepository for PgServerBookingRepository {
    async fn create(&self, booking: CreateServerBooking) -> Result<ServerBooking, DomainError> {
        let row = sqlx::query_as::<_, ServerBookingRow>(
            "INSERT INTO server_bookings \
                (id, server_id, tournament_id, reason, starts_at, ends_at, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING *",
        )
        .bind(booking.id.as_uuid())
        .bind(booking.server_id.as_uuid())
        .bind(booking.tournament_id.map(|t| t.as_uuid()))
        .bind(&booking.reason)
        .bind(booking.starts_at)
        .bind(booking.ends_at)
        .bind(booking.created_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        Ok(ServerBooking::from(row))
    }

    async fn find_by_id(&self, id: ServerBookingId) -> Result<Option<ServerBooking>, DomainError> {
        let row =
            sqlx::query_as::<_, ServerBookingRow>("SELECT * FROM server_bookings WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(internal)?;
        Ok(row.map(ServerBooking::from))
    }

    async fn list_for_server(
        &self,
        server_id: GameServerId,
        after: DateTime<Utc>,
    ) -> Result<Vec<ServerBooking>, DomainError> {
        let rows = sqlx::query_as::<_, ServerBookingRow>(
            "SELECT * FROM server_bookings \
             WHERE server_id = $1 AND ends_at > $2 \
             ORDER BY starts_at ASC",
        )
        .bind(server_id.as_uuid())
        .bind(after)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerBooking::from).collect())
    }

    async fn find_covering(
        &self,
        server_id: GameServerId,
        at: DateTime<Utc>,
    ) -> Result<Vec<ServerBooking>, DomainError> {
        let rows = sqlx::query_as::<_, ServerBookingRow>(
            "SELECT * FROM server_bookings \
             WHERE server_id = $1 AND starts_at <= $2 AND ends_at > $2",
        )
        .bind(server_id.as_uuid())
        .bind(at)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerBooking::from).collect())
    }

    async fn delete(&self, id: ServerBookingId) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM server_bookings WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::ServerBookingNotFound(id));
        }
        Ok(())
    }
}

// =============================================================================
// Server Reservation Repository Adapter
// =============================================================================

impl From<ServerReservationRow> for ServerReservation {
    fn from(row: ServerReservationRow) -> Self {
        Self {
            id: ServerReservationId::from(row.id),
            server_id: row.server_id.map(GameServerId::from),
            match_id: TournamentMatchId::from(row.match_id),
            matchzy_id: row.matchzy_id,
            status: row.status.parse().unwrap_or_default(),
            kind: row.reservation_kind.parse().unwrap_or_default(),
            connect_password: row.connect_password,
            gotv_password: row.gotv_password,
            config_token_hash: row.config_token_hash,
            event_token_hash: row.event_token_hash,
            config_token_expires_at: row.config_token_expires_at,
            match_config: row.match_config,
            config_fetched_at: row.config_fetched_at,
            config_fetch_count: row.config_fetch_count,
            went_live_at: row.went_live_at,
            completed_at: row.completed_at,
            failure_reason: row.failure_reason,
            retry_count: row.retry_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<ServerEventRow> for ServerEvent {
    fn from(row: ServerEventRow) -> Self {
        Self {
            id: ServerEventId::from(row.id),
            reservation_id: row.reservation_id.map(ServerReservationId::from),
            server_id: row.server_id.map(GameServerId::from),
            event_type: row.event_type,
            map_number: row.map_number,
            round_number: row.round_number,
            payload: row.payload,
            processed: row.processed,
            processed_at: row.processed_at,
            processing_error: row.processing_error,
            received_at: row.received_at,
        }
    }
}

/// PostgreSQL implementation of the domain `ServerReservationRepository` trait.
#[derive(Clone)]
pub struct PgServerReservationRepository {
    pool: DbPool,
}

impl PgServerReservationRepository {
    /// Create a new PostgreSQL server reservation repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServerReservationRepository for PgServerReservationRepository {
    async fn create_pending(
        &self,
        create: CreateServerReservation,
    ) -> Result<ServerReservation, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "INSERT INTO server_reservations \
                (id, match_id, reservation_kind, connect_password, gotv_password, \
                 config_token_hash, event_token_hash, config_token_expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             RETURNING *",
        )
        .bind(create.id.as_uuid())
        .bind(create.match_id.as_uuid())
        .bind(create.kind.to_string())
        .bind(&create.connect_password)
        .bind(&create.gotv_password)
        .bind(&create.config_token_hash)
        .bind(&create.event_token_hash)
        .bind(create.config_token_expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("uq_server_reservations_live_match") {
                DomainError::Conflict("this match already has a live server reservation".into())
            } else {
                internal(e)
            }
        })?;
        Ok(ServerReservation::from(row))
    }

    async fn allocate(
        &self,
        id: ServerReservationId,
        game_id: GameId,
        tournament_id: TournamentId,
        region: Option<&str>,
        heartbeat_cutoff: DateTime<Utc>,
        now: DateTime<Utc>,
        scheduled_at: Option<DateTime<Utc>>,
        for_pug: bool,
    ) -> Result<Option<(ServerReservation, GameServer)>, DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;

        // §6.7 allocation predicate. FOR UPDATE SKIP LOCKED settles the race
        // between two matches finishing veto simultaneously at the database.
        let pick_sql = format!(
            "SELECT {SERVER_COLS} FROM game_servers gs \
             WHERE gs.enabled AND gs.status = 'available' AND gs.game_id = $1 \
               AND ($2::text IS NULL OR gs.region = $2) \
               AND gs.last_heartbeat_at > $3 \
               AND gs.last_gamestate = 'none' \
               AND (NOT $7 OR gs.allow_pugs) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM server_bookings b \
                   WHERE b.server_id = gs.id \
                     AND ((b.starts_at <= $4 AND b.ends_at > $4) \
                          OR ($6::timestamptz IS NOT NULL \
                              AND b.starts_at <= $6 AND b.ends_at > $6)) \
                     AND (b.tournament_id IS NULL OR b.tournament_id <> $5) \
               ) \
             ORDER BY EXISTS ( \
                   SELECT 1 FROM server_bookings b2 \
                   WHERE b2.server_id = gs.id \
                     AND b2.starts_at <= $4 AND b2.ends_at > $4 \
                     AND b2.tournament_id = $5 \
               ) DESC, gs.last_heartbeat_at DESC \
             LIMIT 1 \
             FOR UPDATE OF gs SKIP LOCKED"
        );
        let server_row = sqlx::query_as::<_, GameServerRow>(&pick_sql)
            .bind(game_id.as_uuid())
            .bind(region)
            .bind(heartbeat_cutoff)
            .bind(now)
            .bind(tournament_id.as_uuid())
            .bind(scheduled_at)
            .bind(for_pug)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal)?;

        let Some(server_row) = server_row else {
            tx.rollback().await.map_err(internal)?;
            return Ok(None);
        };
        let server_id = server_row.id;

        let reservation_row = sqlx::query_as::<_, ServerReservationRow>(
            "UPDATE server_reservations SET server_id = $2, updated_at = NOW() \
             WHERE id = $1 AND status = 'pending' \
             RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(server_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;

        sqlx::query(
            "UPDATE game_servers SET status = 'reserved', current_match_id = $2, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(server_id)
        .bind(reservation_row.match_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;

        tx.commit().await.map_err(internal)?;

        let reservation = ServerReservation::from(reservation_row);
        let mut server = GameServer::from(server_row);
        server.status = GameServerStatus::Reserved;
        server.current_match_id = Some(reservation.match_id);
        Ok(Some((reservation, server)))
    }

    async fn release(
        &self,
        id: ServerReservationId,
        final_status: ReservationStatus,
        reason: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut tx = self.pool.begin().await.map_err(internal)?;
        let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
            "UPDATE server_reservations SET status = $2, \
                failure_reason = COALESCE($3, failure_reason), \
                completed_at = CASE WHEN $2 = 'completed' THEN NOW() ELSE completed_at END, \
                updated_at = NOW() \
             WHERE id = $1 RETURNING server_id",
        )
        .bind(id.as_uuid())
        .bind(final_status.to_string())
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?;

        if let Some((Some(server_id),)) = row {
            sqlx::query(
                "UPDATE game_servers SET status = 'available', current_match_id = NULL, \
                 updated_at = NOW() \
                 WHERE id = $1 AND status IN ('reserved', 'configuring', 'in_match', 'busy_external')",
            )
            .bind(server_id)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
        }
        tx.commit().await.map_err(internal)?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: ServerReservationId,
    ) -> Result<Option<ServerReservation>, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerReservation::from))
    }

    async fn find_by_matchzy_id(
        &self,
        matchzy_id: i64,
    ) -> Result<Option<ServerReservation>, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE matchzy_id = $1",
        )
        .bind(matchzy_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerReservation::from))
    }

    async fn find_live_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ServerReservation>, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE match_id = $1 \
             AND status IN ('pending','configuring','ready','live')",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerReservation::from))
    }

    async fn find_latest_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ServerReservation>, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE match_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerReservation::from))
    }

    async fn find_live_by_server(
        &self,
        server_id: GameServerId,
    ) -> Result<Option<ServerReservation>, DomainError> {
        let row = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE server_id = $1 \
             AND status IN ('pending','configuring','ready','live')",
        )
        .bind(server_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerReservation::from))
    }

    async fn count_pending_before(&self, before: DateTime<Utc>) -> Result<i64, DomainError> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM server_reservations WHERE status = 'pending' \
             AND created_at < $1",
        )
        .bind(before)
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        Ok(count)
    }

    async fn has_live_for_server(&self, server_id: GameServerId) -> Result<bool, DomainError> {
        let (exists,): (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM server_reservations WHERE server_id = $1 \
             AND status IN ('pending','configuring','ready','live'))",
        )
        .bind(server_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        Ok(exists)
    }

    async fn set_status(
        &self,
        id: ServerReservationId,
        status: ReservationStatus,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE server_reservations SET status = $2, updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status.to_string())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(())
    }

    async fn mark_failed(&self, id: ServerReservationId, reason: &str) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET status = 'failed', failure_reason = $2, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn set_config_token(
        &self,
        id: ServerReservationId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET config_token_hash = $2, \
             config_token_expires_at = $3, config_fetch_count = 0, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn store_config(
        &self,
        id: ServerReservationId,
        config: &serde_json::Value,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET match_config = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(config)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn mark_config_fetched(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET config_fetched_at = $2, \
             config_fetch_count = config_fetch_count + 1, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn mark_live(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET status = 'live', went_live_at = $2, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn mark_completed(
        &self,
        id: ServerReservationId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_reservations SET status = 'completed', completed_at = $2, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn increment_retry(&self, id: ServerReservationId) -> Result<i32, DomainError> {
        let (count,): (i32,) = sqlx::query_as(
            "UPDATE server_reservations SET retry_count = retry_count + 1, \
             updated_at = NOW() WHERE id = $1 RETURNING retry_count",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(internal)?;
        Ok(count)
    }

    async fn touch(&self, id: ServerReservationId) -> Result<(), DomainError> {
        sqlx::query("UPDATE server_reservations SET updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(())
    }

    async fn list_pending(&self, limit: i64) -> Result<Vec<ServerReservation>, DomainError> {
        let rows = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE status = 'pending' \
             ORDER BY CASE reservation_kind WHEN 'match' THEN 0 ELSE 1 END, \
                      created_at ASC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerReservation::from).collect())
    }

    async fn list_stuck_configuring(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ServerReservation>, DomainError> {
        let rows = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE status = 'configuring' \
             AND updated_at < $1",
        )
        .bind(before)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerReservation::from).collect())
    }

    async fn list_active_stale(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ServerReservation>, DomainError> {
        let rows = sqlx::query_as::<_, ServerReservationRow>(
            "SELECT * FROM server_reservations WHERE status IN ('ready','live') \
             AND updated_at < $1",
        )
        .bind(before)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerReservation::from).collect())
    }
}

// =============================================================================
// Server Event Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `ServerEventRepository` trait.
#[derive(Clone)]
pub struct PgServerEventRepository {
    pool: DbPool,
}

impl PgServerEventRepository {
    /// Create a new PostgreSQL server event repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ServerEventRepository for PgServerEventRepository {
    async fn insert(&self, event: CreateServerEvent) -> Result<Option<ServerEvent>, DomainError> {
        // ON CONFLICT DO NOTHING against the dedupe index makes MatchZy
        // replays no-ops without surfacing an error.
        let row = sqlx::query_as::<_, ServerEventRow>(
            "INSERT INTO server_events \
                (id, reservation_id, server_id, event_type, map_number, round_number, payload) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT DO NOTHING \
             RETURNING *",
        )
        .bind(ServerEventId::new().as_uuid())
        .bind(event.reservation_id.map(|r| r.as_uuid()))
        .bind(event.server_id.as_uuid())
        .bind(&event.event_type)
        .bind(event.map_number)
        .bind(event.round_number)
        .bind(&event.payload)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerEvent::from))
    }

    async fn mark_processed(
        &self,
        id: ServerEventId,
        error: Option<&str>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE server_events SET processed = TRUE, processed_at = NOW(), \
             processing_error = $2 WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn latest_round_end(
        &self,
        reservation_id: ServerReservationId,
    ) -> Result<Option<ServerEvent>, DomainError> {
        let row = sqlx::query_as::<_, ServerEventRow>(
            "SELECT * FROM server_events WHERE reservation_id = $1 \
             AND event_type = 'round_end' ORDER BY received_at DESC LIMIT 1",
        )
        .bind(reservation_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerEvent::from))
    }

    async fn record_processing_error(
        &self,
        id: ServerEventId,
        error: &str,
    ) -> Result<(), DomainError> {
        sqlx::query("UPDATE server_events SET processing_error = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(error)
            .execute(&self.pool)
            .await
            .map_err(internal)?;
        Ok(())
    }

    async fn list_unprocessed(&self, limit: i64) -> Result<Vec<ServerEvent>, DomainError> {
        let rows = sqlx::query_as::<_, ServerEventRow>(
            "SELECT * FROM server_events WHERE processed = FALSE \
             AND event_type <> 'backup_uploaded' \
             AND received_at < NOW() - INTERVAL '1 minute' \
             ORDER BY received_at ASC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(ServerEvent::from).collect())
    }

    async fn find_backup_key(
        &self,
        reservation_id: ServerReservationId,
        filename: &str,
    ) -> Result<Option<String>, DomainError> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT payload FROM server_events WHERE reservation_id = $1 \
             AND event_type = 'backup_uploaded' AND payload->>'filename' = $2 \
             ORDER BY received_at DESC LIMIT 1",
        )
        .bind(reservation_id.as_uuid())
        .bind(filename)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.and_then(|(payload,)| {
            payload
                .get("storage_key")
                .and_then(|k| k.as_str())
                .map(str::to_string)
        }))
    }

    async fn latest_backup(
        &self,
        reservation_id: ServerReservationId,
        before_round: Option<i32>,
    ) -> Result<Option<ServerEvent>, DomainError> {
        let row = sqlx::query_as::<_, ServerEventRow>(
            "SELECT * FROM server_events WHERE reservation_id = $1 \
             AND event_type = 'backup_uploaded' \
             AND ($2::int IS NULL OR round_number <= $2) \
             ORDER BY map_number DESC NULLS LAST, round_number DESC NULLS LAST, \
                      received_at DESC \
             LIMIT 1",
        )
        .bind(reservation_id.as_uuid())
        .bind(before_round)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(ServerEvent::from))
    }
}

// =============================================================================
// Match Substitution Repository Adapter
// =============================================================================

impl From<MatchSubstitutionRow> for MatchSubstitution {
    fn from(row: MatchSubstitutionRow) -> Self {
        Self {
            id: MatchSubstitutionId::from(row.id),
            match_id: TournamentMatchId::from(row.match_id),
            registration_id: TournamentRegistrationId::from(row.registration_id),
            reservation_id: row.reservation_id.map(ServerReservationId::from),
            player_out_id: PlayerId::from(row.player_out_id),
            player_in_id: row.player_in_id.map(PlayerId::from),
            from_game_number: row.from_game_number,
            status: row.status.parse().unwrap_or_default(),
            requested_by: UserId::from(row.requested_by),
            approved_by: row.approved_by.map(UserId::from),
            failure_reason: row.failure_reason,
            applied_at: row.applied_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// PostgreSQL implementation of the domain `MatchSubstitutionRepository` trait.
#[derive(Clone)]
pub struct PgMatchSubstitutionRepository {
    pool: DbPool,
}

impl PgMatchSubstitutionRepository {
    /// Create a new PostgreSQL match substitution repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MatchSubstitutionRepository for PgMatchSubstitutionRepository {
    async fn create(&self, sub: CreateMatchSubstitution) -> Result<MatchSubstitution, DomainError> {
        let row = sqlx::query_as::<_, MatchSubstitutionRow>(
            "INSERT INTO match_substitutions \
                (id, match_id, registration_id, reservation_id, player_out_id, player_in_id, \
                 from_game_number, status, requested_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             RETURNING *",
        )
        .bind(sub.id.as_uuid())
        .bind(sub.match_id.as_uuid())
        .bind(sub.registration_id.as_uuid())
        .bind(sub.reservation_id.map(|r| r.as_uuid()))
        .bind(sub.player_out_id.as_uuid())
        .bind(sub.player_in_id.map(|p| p.as_uuid()))
        .bind(sub.from_game_number)
        .bind(sub.status.to_string())
        .bind(sub.requested_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("uq_match_substitutions_live") {
                DomainError::Conflict("a substitution for this player is already in flight".into())
            } else {
                internal(e)
            }
        })?;
        Ok(MatchSubstitution::from(row))
    }

    async fn find_by_id(
        &self,
        id: MatchSubstitutionId,
    ) -> Result<Option<MatchSubstitution>, DomainError> {
        let row = sqlx::query_as::<_, MatchSubstitutionRow>(
            "SELECT * FROM match_substitutions WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(internal)?;
        Ok(row.map(MatchSubstitution::from))
    }

    async fn list_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchSubstitution>, DomainError> {
        let rows = sqlx::query_as::<_, MatchSubstitutionRow>(
            "SELECT * FROM match_substitutions WHERE match_id = $1 ORDER BY created_at ASC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(MatchSubstitution::from).collect())
    }

    async fn list_applied_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchSubstitution>, DomainError> {
        let rows = sqlx::query_as::<_, MatchSubstitutionRow>(
            "SELECT * FROM match_substitutions WHERE match_id = $1 AND status = 'applied' \
             ORDER BY created_at ASC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(MatchSubstitution::from).collect())
    }

    async fn list_applying(&self, limit: i64) -> Result<Vec<MatchSubstitution>, DomainError> {
        let rows = sqlx::query_as::<_, MatchSubstitutionRow>(
            "SELECT * FROM match_substitutions WHERE status = 'applying' \
             ORDER BY created_at ASC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(MatchSubstitution::from).collect())
    }

    async fn list_applying_by_reservation(
        &self,
        reservation_id: ServerReservationId,
    ) -> Result<Vec<MatchSubstitution>, DomainError> {
        let rows = sqlx::query_as::<_, MatchSubstitutionRow>(
            "SELECT * FROM match_substitutions WHERE reservation_id = $1 \
             AND status = 'applying' ORDER BY created_at ASC",
        )
        .bind(reservation_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(internal)?;
        Ok(rows.into_iter().map(MatchSubstitution::from).collect())
    }

    async fn set_status(
        &self,
        id: MatchSubstitutionId,
        status: SubstitutionStatus,
        failure_reason: Option<&str>,
        approved_by: Option<UserId>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE match_substitutions SET status = $2, \
                failure_reason = COALESCE($3, failure_reason), \
                approved_by = COALESCE($4, approved_by), \
                updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(failure_reason)
        .bind(approved_by.map(|u| u.as_uuid()))
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn mark_applied(
        &self,
        id: MatchSubstitutionId,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE match_substitutions SET status = 'applied', applied_at = $2, \
             updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(internal)?;
        Ok(())
    }
}
