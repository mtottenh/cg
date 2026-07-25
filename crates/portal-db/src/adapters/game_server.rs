//! Game-server repository adapters.

use crate::DbPool;
use crate::entities::{GameServerRow, ServerAgentCertRow, ServerBookingRow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    GameId, GameServerId, ServerAgentCertId, ServerBookingId, TournamentId, TournamentMatchId,
    UserId,
};
use portal_core::types::GameServerStatus;
use portal_domain::entities::{AgentCertificate, GameServer, ServerBooking};
use portal_domain::repositories::{
    AgentCertRepository, CreateAgentCertificate, CreateGameServer, CreateServerBooking,
    GameServerRepository, RecordHeartbeat, ServerBookingRepository, UpdateGameServer,
};

/// Explicit column list: `ip_address` must go through `host()` to come back
/// as text, so `SELECT *` is not usable on this table.
const SERVER_COLS: &str = "id, name, game_id, host(ip_address) AS ip_address, port, gotv_port, \
     region, enabled, status, current_match_id, agent_cert_serial, agent_cert_expires_at, \
     agent_version, last_heartbeat_at, last_gamestate, enrollment_token_hash, \
     enrollment_token_expires_at, created_at, updated_at";

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
        Ok(rows.into_iter().map(|(id,)| GameServerId::from(id)).collect())
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
    async fn create(
        &self,
        cert: CreateAgentCertificate,
    ) -> Result<AgentCertificate, DomainError> {
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
        let row = sqlx::query_as::<_, ServerBookingRow>(
            "SELECT * FROM server_bookings WHERE id = $1",
        )
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
