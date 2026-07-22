//! Award repository adapter (`award_templates`, `awards`, `award_results`).

use crate::DbPool;
use crate::entities::{AwardResultRow, AwardRow, AwardTemplateRow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::{AwardId, DomainError, GameId, PlayerId};
use portal_domain::entities::award::{
    Award, AwardResult, AwardScopeType, AwardStatus, AwardTemplate,
};
use portal_domain::repositories::award::{
    AwardRepository, CreateAward, CreateAwardResult, PlayerTrophy, UpdateAwardPresentation,
};
use sqlx::FromRow;
use uuid::Uuid;

/// Trophy-case row: an `award_results` row joined with its award and the
/// scope's display name.
#[derive(Debug, FromRow)]
struct TrophyRow {
    // award_results
    result_id: Uuid,
    award_id: Uuid,
    rank: i32,
    player_id: Uuid,
    value: f64,
    demos_counted: i32,
    finalized_at: DateTime<Utc>,
    // awards (aliased)
    scope_type: String,
    scope_id: Uuid,
    game_id: Uuid,
    template_id: Option<Uuid>,
    name: String,
    description: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    stat_key: String,
    aggregation: String,
    direction: String,
    min_qualifier_type: Option<String>,
    min_qualifier_value: Option<i32>,
    subject_type: String,
    status: String,
    created_by: Uuid,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    // joined scope
    scope_name: Option<String>,
}

/// Map a sqlx error, converting the scope-name unique violation into a
/// domain conflict.
fn map_award_write_error(e: sqlx::Error) -> DomainError {
    if let sqlx::Error::Database(db_err) = &e
        && db_err.constraint() == Some("awards_scope_name_unique")
    {
        return DomainError::conflict(
            "An award with this name already exists in this tournament or season",
        );
    }
    DomainError::internal(format!("Award write failed: {e}"))
}

/// Postgres implementation of [`AwardRepository`].
#[derive(Clone)]
pub struct PgAwardRepository {
    pool: DbPool,
}

impl PgAwardRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AwardRepository for PgAwardRepository {
    async fn list_templates_by_game(
        &self,
        game_id: GameId,
    ) -> Result<Vec<AwardTemplate>, DomainError> {
        let rows = sqlx::query_as::<_, AwardTemplateRow>(
            "SELECT * FROM award_templates WHERE game_id = $1 ORDER BY name",
        )
        .bind(game_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to list award templates: {e}")))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn find_template_by_key(
        &self,
        game_id: GameId,
        key: &str,
    ) -> Result<Option<AwardTemplate>, DomainError> {
        let row = sqlx::query_as::<_, AwardTemplateRow>(
            "SELECT * FROM award_templates WHERE game_id = $1 AND key = $2",
        )
        .bind(game_id.as_uuid())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find award template: {e}")))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn create(&self, award: CreateAward) -> Result<Award, DomainError> {
        let (qualifier_type, qualifier_value) = award
            .min_qualifier
            .map_or((None, None), |q| (Some(q.qualifier_type), Some(q.value)));

        let row = sqlx::query_as::<_, AwardRow>(
            r"
            INSERT INTO awards
                (scope_type, scope_id, game_id, template_id, name, description,
                 icon, color, stat_key, aggregation, direction,
                 min_qualifier_type, min_qualifier_value, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            ",
        )
        .bind(award.scope_type.as_str())
        .bind(award.scope_id)
        .bind(award.game_id.as_uuid())
        .bind(award.template_id.map(|id| id.as_uuid()))
        .bind(&award.name)
        .bind(&award.description)
        .bind(&award.icon)
        .bind(&award.color)
        .bind(&award.stat_key)
        .bind(award.aggregation.as_str())
        .bind(award.direction.as_str())
        .bind(qualifier_type.map(portal_domain::entities::MinQualifierType::as_str))
        .bind(qualifier_value)
        .bind(award.created_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(map_award_write_error)?;

        row.try_into()
    }

    async fn find_by_id(&self, id: AwardId) -> Result<Option<Award>, DomainError> {
        let row = sqlx::query_as::<_, AwardRow>("SELECT * FROM awards WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to find award: {e}")))?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_scope(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<Vec<Award>, DomainError> {
        let rows = sqlx::query_as::<_, AwardRow>(
            r"
            SELECT * FROM awards
            WHERE scope_type = $1 AND scope_id = $2
            ORDER BY created_at, name
            ",
        )
        .bind(scope_type.as_str())
        .bind(scope_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to list awards: {e}")))?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update_presentation(
        &self,
        id: AwardId,
        update: UpdateAwardPresentation,
    ) -> Result<Award, DomainError> {
        let row = sqlx::query_as::<_, AwardRow>(
            r"
            UPDATE awards
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                icon = COALESCE($4, icon),
                color = COALESCE($5, color)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.description)
        .bind(&update.icon)
        .bind(&update.color)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_award_write_error)?
        .ok_or_else(|| DomainError::LookupFailed {
            resource: "award",
            query: id.to_string(),
        })?;

        row.try_into()
    }

    async fn set_status(&self, id: AwardId, status: AwardStatus) -> Result<Award, DomainError> {
        let row = sqlx::query_as::<_, AwardRow>(
            "UPDATE awards SET status = $2 WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to set award status: {e}")))?
        .ok_or_else(|| DomainError::LookupFailed {
            resource: "award",
            query: id.to_string(),
        })?;

        row.try_into()
    }

    async fn replace_results_and_finalize(
        &self,
        award_id: AwardId,
        results: Vec<CreateAwardResult>,
    ) -> Result<Vec<AwardResult>, DomainError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            DomainError::internal(format!("Failed to begin finalize transaction: {e}"))
        })?;

        sqlx::query("DELETE FROM award_results WHERE award_id = $1")
            .bind(award_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to clear award results: {e}")))?;

        let mut written = Vec::with_capacity(results.len());
        for result in &results {
            let row = sqlx::query_as::<_, AwardResultRow>(
                r"
                INSERT INTO award_results (award_id, rank, player_id, value, demos_counted)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING *
                ",
            )
            .bind(award_id.as_uuid())
            .bind(result.rank)
            .bind(result.player_id.as_uuid())
            .bind(result.value)
            .bind(result.demos_counted)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to write award result: {e}")))?;
            written.push(AwardResult::from(row));
        }

        sqlx::query("UPDATE awards SET status = 'finalized' WHERE id = $1")
            .bind(award_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to finalize award: {e}")))?;

        tx.commit().await.map_err(|e| {
            DomainError::internal(format!("Failed to commit finalize transaction: {e}"))
        })?;

        Ok(written)
    }

    async fn list_results_by_award(
        &self,
        award_id: AwardId,
    ) -> Result<Vec<AwardResult>, DomainError> {
        let rows = sqlx::query_as::<_, AwardResultRow>(
            r"
            SELECT ar.* FROM award_results ar
            JOIN players p ON p.id = ar.player_id
            WHERE ar.award_id = $1
            ORDER BY ar.rank, p.display_name
            ",
        )
        .bind(award_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to list award results: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_trophies_by_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTrophy>, DomainError> {
        let rows = sqlx::query_as::<_, TrophyRow>(
            r"
            SELECT ar.id AS result_id,
                   ar.award_id,
                   ar.rank,
                   ar.player_id,
                   ar.value,
                   ar.demos_counted,
                   ar.finalized_at,
                   a.scope_type,
                   a.scope_id,
                   a.game_id,
                   a.template_id,
                   a.name,
                   a.description,
                   a.icon,
                   a.color,
                   a.stat_key,
                   a.aggregation,
                   a.direction,
                   a.min_qualifier_type,
                   a.min_qualifier_value,
                   a.subject_type,
                   a.status,
                   a.created_by,
                   a.created_at,
                   a.updated_at,
                   COALESCE(t.name, ls.name) AS scope_name
            FROM award_results ar
            JOIN awards a ON a.id = ar.award_id
            LEFT JOIN tournaments t
                ON a.scope_type = 'tournament' AND t.id = a.scope_id
            LEFT JOIN league_seasons ls
                ON a.scope_type = 'league_season' AND ls.id = a.scope_id
            WHERE ar.player_id = $1
              AND a.status = 'finalized'
            ORDER BY ar.finalized_at DESC, a.name
            ",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to list player trophies: {e}")))?;

        rows.into_iter().map(trophy_row_to_domain).collect()
    }
}

/// Split a joined trophy row into its domain parts.
fn trophy_row_to_domain(row: TrophyRow) -> Result<PlayerTrophy, DomainError> {
    let award: Award = AwardRow {
        id: row.award_id,
        scope_type: row.scope_type,
        scope_id: row.scope_id,
        game_id: row.game_id,
        template_id: row.template_id,
        name: row.name,
        description: row.description,
        icon: row.icon,
        color: row.color,
        stat_key: row.stat_key,
        aggregation: row.aggregation,
        direction: row.direction,
        min_qualifier_type: row.min_qualifier_type,
        min_qualifier_value: row.min_qualifier_value,
        subject_type: row.subject_type,
        status: row.status,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
    .try_into()?;

    Ok(PlayerTrophy {
        result: AwardResult::from(AwardResultRow {
            id: row.result_id,
            award_id: row.award_id,
            rank: row.rank,
            player_id: row.player_id,
            value: row.value,
            demos_counted: row.demos_counted,
            finalized_at: row.finalized_at,
        }),
        award,
        scope_name: row.scope_name,
    })
}
