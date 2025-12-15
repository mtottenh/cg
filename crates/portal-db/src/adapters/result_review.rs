//! Result review repository adapters.

use crate::entities::ResultReviewRow;
use crate::DbPool;
use async_trait::async_trait;
use portal_core::{
    DemoMatchLinkId, DomainError, ResultClaimId, ResultReviewId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use portal_domain::entities::demo_validation::{DemoValidationResult, UnrecognizedPlayer};
use portal_domain::entities::result_review::{ResultReview, ResultReviewStatus};
use portal_domain::repositories::result_review::ResultReviewRepository;
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<ResultReviewRow> for ResultReview {
    fn from(row: ResultReviewRow) -> Self {
        let validation_result: Option<DemoValidationResult> = row
            .validation_result
            .and_then(|json| serde_json::from_value(json.0).ok());

        let unrecognized_players: Vec<UnrecognizedPlayer> = row
            .unrecognized_players
            .0
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        Self {
            id: ResultReviewId::from(row.id),
            result_claim_id: ResultClaimId::from(row.result_claim_id),
            match_id: TournamentMatchId::from(row.match_id),
            roster_mismatch: row.roster_mismatch,
            score_mismatch: row.score_mismatch,
            winner_mismatch: row.winner_mismatch,
            demo_link_id: row.demo_link_id.map(DemoMatchLinkId::from),
            validation_result,
            unrecognized_players,
            status: row.status.parse().unwrap_or_default(),
            captain1_registration_id: TournamentRegistrationId::from(row.captain1_registration_id),
            captain1_acknowledged: row.captain1_acknowledged,
            captain1_acknowledged_at: row.captain1_acknowledged_at,
            captain1_acknowledged_by_user_id: row
                .captain1_acknowledged_by_user_id
                .map(UserId::from),
            captain2_registration_id: TournamentRegistrationId::from(row.captain2_registration_id),
            captain2_acknowledged: row.captain2_acknowledged,
            captain2_acknowledged_at: row.captain2_acknowledged_at,
            captain2_acknowledged_by_user_id: row
                .captain2_acknowledged_by_user_id
                .map(UserId::from),
            reviewed_by_user_id: row.reviewed_by_user_id.map(UserId::from),
            reviewed_at: row.reviewed_at,
            admin_notes: row.admin_notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Result Review Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `ResultReviewRepository` trait.
#[derive(Clone)]
pub struct PgResultReviewRepository {
    pool: DbPool,
}

impl PgResultReviewRepository {
    /// Create a new `PostgreSQL` result review repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResultReviewRepository for PgResultReviewRepository {
    async fn insert(&self, review: &ResultReview) -> Result<(), DomainError> {
        let validation_result_json = review
            .validation_result
            .as_ref()
            .and_then(|v| serde_json::to_value(v).ok());

        let unrecognized_players_json: Vec<serde_json::Value> = review
            .unrecognized_players
            .iter()
            .filter_map(|p| serde_json::to_value(p).ok())
            .collect();

        sqlx::query(
            r"
            INSERT INTO result_reviews (
                id, result_claim_id, match_id, roster_mismatch, score_mismatch, winner_mismatch,
                demo_link_id, validation_result, unrecognized_players, status,
                captain1_registration_id, captain1_acknowledged, captain1_acknowledged_at,
                captain1_acknowledged_by_user_id, captain2_registration_id, captain2_acknowledged,
                captain2_acknowledged_at, captain2_acknowledged_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::result_review_status, $11, $12, $13, $14, $15, $16, $17, $18)
            ",
        )
        .bind(review.id.as_uuid())
        .bind(review.result_claim_id.as_uuid())
        .bind(review.match_id.as_uuid())
        .bind(review.roster_mismatch)
        .bind(review.score_mismatch)
        .bind(review.winner_mismatch)
        .bind(review.demo_link_id.map(|id| id.as_uuid()))
        .bind(validation_result_json)
        .bind(serde_json::to_value(&unrecognized_players_json).unwrap_or_default())
        .bind(review.status.as_str())
        .bind(review.captain1_registration_id.as_uuid())
        .bind(review.captain1_acknowledged)
        .bind(review.captain1_acknowledged_at)
        .bind(review.captain1_acknowledged_by_user_id.map(|id| id.as_uuid()))
        .bind(review.captain2_registration_id.as_uuid())
        .bind(review.captain2_acknowledged)
        .bind(review.captain2_acknowledged_at)
        .bind(review.captain2_acknowledged_by_user_id.map(|id| id.as_uuid()))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: ResultReviewId) -> Result<Option<ResultReview>, DomainError> {
        let review =
            sqlx::query_as::<_, ResultReviewRow>("SELECT * FROM result_reviews WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(review.map(ResultReview::from))
    }

    async fn find_by_claim_id(
        &self,
        claim_id: ResultClaimId,
    ) -> Result<Option<ResultReview>, DomainError> {
        let review = sqlx::query_as::<_, ResultReviewRow>(
            "SELECT * FROM result_reviews WHERE result_claim_id = $1",
        )
        .bind(claim_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(review.map(ResultReview::from))
    }

    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        let review = sqlx::query_as::<_, ResultReviewRow>(
            "SELECT * FROM result_reviews WHERE match_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(review.map(ResultReview::from))
    }

    async fn find_pending_admin_reviews(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResultReview>, DomainError> {
        let reviews = sqlx::query_as::<_, ResultReviewRow>(
            r"
            SELECT * FROM result_reviews
            WHERE status = 'pending_admin_review'
            ORDER BY created_at ASC
            LIMIT $1 OFFSET $2
            ",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(reviews.into_iter().map(ResultReview::from).collect())
    }

    async fn count_pending_admin_reviews(&self) -> Result<i64, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM result_reviews WHERE status = 'pending_admin_review'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }

    async fn update_captain_acknowledgment(
        &self,
        id: ResultReviewId,
        captain_side: i32,
        acknowledged_by_user_id: UserId,
    ) -> Result<(), DomainError> {
        let query = match captain_side {
            1 => {
                r"
                UPDATE result_reviews SET
                    captain1_acknowledged = true,
                    captain1_acknowledged_at = NOW(),
                    captain1_acknowledged_by_user_id = $2,
                    updated_at = NOW()
                WHERE id = $1
                "
            }
            2 => {
                r"
                UPDATE result_reviews SET
                    captain2_acknowledged = true,
                    captain2_acknowledged_at = NOW(),
                    captain2_acknowledged_by_user_id = $2,
                    updated_at = NOW()
                WHERE id = $1
                "
            }
            _ => {
                return Err(DomainError::Internal(format!(
                    "Invalid captain side: {captain_side}"
                )));
            }
        };

        sqlx::query(query)
            .bind(id.as_uuid())
            .bind(acknowledged_by_user_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn update_status(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE result_reviews SET
                status = $2::result_review_status,
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn resolve(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
        reviewed_by_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE result_reviews SET
                status = $2::result_review_status,
                reviewed_by_user_id = $3,
                reviewed_at = NOW(),
                admin_notes = $4,
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .bind(reviewed_by_user_id.as_uuid())
        .bind(admin_notes)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}
