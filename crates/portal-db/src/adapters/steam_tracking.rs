//! Steam tracking repository adapter.

use crate::DbPool;
use crate::entities::SteamTrackingRow;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, SteamTrackingId};
use portal_domain::entities::steam_tracking::{SteamTracking, UpdatePollResultCommand};
use portal_domain::repositories::steam_tracking::{CreateSteamTracking, SteamTrackingRepository};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<SteamTrackingRow> for SteamTracking {
    fn from(row: SteamTrackingRow) -> Self {
        Self {
            id: SteamTrackingId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            steam_id_64: row.steam_id_64,
            game_auth_code: row.game_auth_code,
            last_known_code: row.last_known_code,
            is_active: row.is_active,
            poll_errors: row.poll_errors,
            last_poll_at: row.last_poll_at,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Steam Tracking Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `SteamTrackingRepository` trait.
#[derive(Clone)]
pub struct PgSteamTrackingRepository {
    pool: DbPool,
}

impl PgSteamTrackingRepository {
    /// Create a new PostgreSQL steam tracking repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SteamTrackingRepository for PgSteamTrackingRepository {
    async fn find_by_id(&self, id: SteamTrackingId) -> Result<Option<SteamTracking>, DomainError> {
        let row =
            sqlx::query_as::<_, SteamTrackingRow>("SELECT * FROM steam_tracking WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(SteamTracking::from))
    }

    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<SteamTracking>, DomainError> {
        let row = sqlx::query_as::<_, SteamTrackingRow>(
            "SELECT * FROM steam_tracking WHERE player_id = $1 AND game_id = $2",
        )
        .bind(player_id.as_uuid())
        .bind(game_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(SteamTracking::from))
    }

    async fn create(&self, cmd: CreateSteamTracking) -> Result<SteamTracking, DomainError> {
        let row = sqlx::query_as::<_, SteamTrackingRow>(
            r"
            INSERT INTO steam_tracking (player_id, game_id, steam_id_64, game_auth_code, last_known_code)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
        )
        .bind(cmd.player_id.as_uuid())
        .bind(cmd.game_id.as_uuid())
        .bind(cmd.steam_id_64)
        .bind(&cmd.game_auth_code)
        .bind(&cmd.initial_share_code)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("uq_steam_tracking_player_game") {
                DomainError::Conflict("Player already has tracking for this game".into())
            } else if e.to_string().contains("uq_steam_tracking_steam_id_game") {
                DomainError::Conflict("This Steam ID is already being tracked for this game".into())
            } else {
                DomainError::Internal(e.to_string())
            }
        })?;

        Ok(SteamTracking::from(row))
    }

    async fn update_auth_code(
        &self,
        id: SteamTrackingId,
        auth_code: &str,
    ) -> Result<SteamTracking, DomainError> {
        let row = sqlx::query_as::<_, SteamTrackingRow>(
            r"
            UPDATE steam_tracking
            SET game_auth_code = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(auth_code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        Ok(SteamTracking::from(row))
    }

    async fn deactivate(&self, id: SteamTrackingId) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE steam_tracking SET is_active = FALSE, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: SteamTrackingId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM steam_tracking WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn find_active_by_game(
        &self,
        game_id: GameId,
    ) -> Result<Vec<SteamTracking>, DomainError> {
        let rows = sqlx::query_as::<_, SteamTrackingRow>(
            r"
            SELECT * FROM steam_tracking
            WHERE is_active = TRUE AND game_id = $1
            ORDER BY last_poll_at ASC NULLS FIRST
            ",
        )
        .bind(game_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(SteamTracking::from).collect())
    }

    async fn update_poll_result(
        &self,
        id: SteamTrackingId,
        cmd: UpdatePollResultCommand,
    ) -> Result<SteamTracking, DomainError> {
        let row = if let Some(error) = &cmd.error {
            // Poll failed: increment errors, record message
            sqlx::query_as::<_, SteamTrackingRow>(
                r"
                UPDATE steam_tracking
                SET poll_errors = poll_errors + 1,
                    last_error = $2,
                    last_poll_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                RETURNING *
                ",
            )
            .bind(id.as_uuid())
            .bind(error)
            .fetch_optional(&self.pool)
            .await
        } else {
            // Poll succeeded: reset errors, update cursor
            sqlx::query_as::<_, SteamTrackingRow>(
                r"
                UPDATE steam_tracking
                SET last_known_code = COALESCE($2, last_known_code),
                    poll_errors = 0,
                    last_error = NULL,
                    last_poll_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                RETURNING *
                ",
            )
            .bind(id.as_uuid())
            .bind(&cmd.last_known_code)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        Ok(SteamTracking::from(row))
    }
}
