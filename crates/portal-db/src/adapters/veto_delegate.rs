//! Veto delegate repository adapter.

use crate::DbPool;
use crate::entities::VetoDelegateRow;
use async_trait::async_trait;
use portal_core::{
    DomainError, LeagueTeamSeasonId, PlayerId, TournamentId, UserId, VetoDelegateId,
};
use portal_domain::entities::veto_delegate::{DelegatedByRole, VetoDelegate};
use portal_domain::repositories::veto_delegate::{CreateVetoDelegate, VetoDelegateRepository};

// =============================================================================
// Type Conversions
// =============================================================================

impl From<VetoDelegateRow> for VetoDelegate {
    fn from(row: VetoDelegateRow) -> Self {
        Self {
            id: VetoDelegateId::from(row.id),
            team_season_id: LeagueTeamSeasonId::from(row.team_season_id),
            player_id: PlayerId::from(row.player_id),
            delegated_by_user_id: UserId::from(row.delegated_by_user_id),
            delegated_by_role: row
                .delegated_by_role
                .parse()
                .unwrap_or(DelegatedByRole::Captain),
            tournament_id: row.tournament_id.map(TournamentId::from),
            revoked_at: row.revoked_at,
            revoked_by_user_id: row.revoked_by_user_id.map(UserId::from),
            created_at: row.created_at,
        }
    }
}

// =============================================================================
// Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `VetoDelegateRepository` trait.
#[derive(Clone)]
pub struct PgVetoDelegateRepository {
    pool: DbPool,
}

impl PgVetoDelegateRepository {
    /// Create a new `PostgreSQL` veto delegate repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VetoDelegateRepository for PgVetoDelegateRepository {
    async fn create(&self, cmd: CreateVetoDelegate) -> Result<VetoDelegate, DomainError> {
        let delegate = sqlx::query_as::<_, VetoDelegateRow>(
            r"
            INSERT INTO veto_delegates (
                team_season_id, player_id, delegated_by_user_id, delegated_by_role, tournament_id
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            ",
        )
        .bind(cmd.team_season_id.as_uuid())
        .bind(cmd.player_id.as_uuid())
        .bind(cmd.delegated_by_user_id.as_uuid())
        .bind(cmd.delegated_by_role.to_string())
        .bind(cmd.tournament_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                // Unique constraint violation on active delegation
                if db_err.constraint() == Some("idx_veto_delegates_unique_active") {
                    return DomainError::Conflict(
                        "Player already has an active delegation for this team".to_string(),
                    );
                }
            }
            DomainError::Internal(e.to_string())
        })?;

        Ok(VetoDelegate::from(delegate))
    }

    async fn find_by_id(&self, id: VetoDelegateId) -> Result<Option<VetoDelegate>, DomainError> {
        let delegate =
            sqlx::query_as::<_, VetoDelegateRow>("SELECT * FROM veto_delegates WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(delegate.map(VetoDelegate::from))
    }

    async fn is_delegate(
        &self,
        team_season_id: LeagueTeamSeasonId,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<bool, DomainError> {
        // Check for:
        // 1. Global delegation (tournament_id IS NULL) for this team
        // 2. Tournament-specific delegation matching the given tournament_id
        let exists = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS(
                SELECT 1 FROM veto_delegates
                WHERE team_season_id = $1
                  AND player_id = $2
                  AND revoked_at IS NULL
                  AND (tournament_id IS NULL OR tournament_id = $3)
            )
            ",
        )
        .bind(team_season_id.as_uuid())
        .bind(player_id.as_uuid())
        .bind(tournament_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(exists)
    }

    async fn list_active(
        &self,
        team_season_id: LeagueTeamSeasonId,
    ) -> Result<Vec<VetoDelegate>, DomainError> {
        let delegates = sqlx::query_as::<_, VetoDelegateRow>(
            r"
            SELECT * FROM veto_delegates
            WHERE team_season_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            ",
        )
        .bind(team_season_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(delegates.into_iter().map(VetoDelegate::from).collect())
    }

    async fn list_by_player(&self, player_id: PlayerId) -> Result<Vec<VetoDelegate>, DomainError> {
        let delegates = sqlx::query_as::<_, VetoDelegateRow>(
            r"
            SELECT * FROM veto_delegates
            WHERE player_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            ",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(delegates.into_iter().map(VetoDelegate::from).collect())
    }

    async fn revoke(
        &self,
        id: VetoDelegateId,
        revoked_by_user_id: UserId,
    ) -> Result<VetoDelegate, DomainError> {
        let delegate = sqlx::query_as::<_, VetoDelegateRow>(
            r"
            UPDATE veto_delegates
            SET revoked_at = NOW(), revoked_by_user_id = $2
            WHERE id = $1 AND revoked_at IS NULL
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(revoked_by_user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| {
            DomainError::Internal("Delegation not found or already revoked".to_string())
        })?;

        Ok(VetoDelegate::from(delegate))
    }
}
