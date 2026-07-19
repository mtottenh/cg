//! PostgreSQL implementation of ResultClaimRepository.

use crate::DbPool;
use crate::entities::{NewResultClaim, ResultClaimRow};
use async_trait::async_trait;
use portal_core::{
    DemoMatchLinkId, DomainError, EvidenceId, ResultClaimId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use portal_domain::entities::result_claim::{ClaimStatus, GameResult, ResultClaim};
use portal_domain::repositories::tournament::{
    CreateResultClaim, ResultClaimRepository, UpdateResultClaim as DomainUpdateResultClaim,
};

// =============================================================================
// RESULT CLAIM REPOSITORY
// =============================================================================

/// PostgreSQL implementation of ResultClaimRepository.
#[derive(Debug, Clone)]
pub struct PgResultClaimRepository {
    pool: DbPool,
}

impl PgResultClaimRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResultClaimRepository for PgResultClaimRepository {
    async fn find_by_id(&self, id: ResultClaimId) -> Result<Option<ResultClaim>, DomainError> {
        let row = sqlx::query_as::<_, ResultClaimRow>(r"SELECT * FROM result_claims WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to find result claim: {e}")))?;

        row.map(row_to_domain).transpose()
    }

    async fn find_pending_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultClaim>, DomainError> {
        let row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            SELECT * FROM result_claims
            WHERE match_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find pending result claim: {e}")))?;

        row.map(row_to_domain).transpose()
    }

    async fn list_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ResultClaim>, DomainError> {
        let rows = sqlx::query_as::<_, ResultClaimRow>(
            r"
            SELECT * FROM result_claims
            WHERE match_id = $1
            ORDER BY created_at DESC
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to list result claims: {e}")))?;

        rows.into_iter().map(row_to_domain).collect()
    }

    async fn create(&self, claim: CreateResultClaim) -> Result<ResultClaim, DomainError> {
        let game_results_json = serde_json::to_value(&claim.game_results)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize game results: {e}")))?;

        let evidence_uuids: Vec<_> = claim
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();
        let demo_link_uuids: Vec<_> = claim
            .demo_link_ids
            .iter()
            .map(portal_core::DemoMatchLinkId::as_uuid)
            .collect();

        let new_claim = NewResultClaim {
            match_id: claim.match_id.as_uuid(),
            submitted_by_registration_id: claim.submitted_by_registration_id.as_uuid(),
            submitted_by_user_id: claim.submitted_by_user_id.as_uuid(),
            claimed_winner_registration_id: claim.claimed_winner_registration_id.as_uuid(),
            claimed_participant1_score: claim.participant1_score,
            claimed_participant2_score: claim.participant2_score,
            game_results: game_results_json,
            auto_confirm_at: claim.auto_confirm_at,
            evidence_ids: evidence_uuids,
            demo_link_ids: demo_link_uuids,
            submitter_notes: claim.notes,
        };

        let row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            INSERT INTO result_claims (
                match_id, submitted_by_registration_id, submitted_by_user_id,
                claimed_winner_registration_id, claimed_participant1_score,
                claimed_participant2_score, game_results, auto_confirm_at,
                evidence_ids, demo_link_ids, submitter_notes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(new_claim.match_id)
        .bind(new_claim.submitted_by_registration_id)
        .bind(new_claim.submitted_by_user_id)
        .bind(new_claim.claimed_winner_registration_id)
        .bind(new_claim.claimed_participant1_score)
        .bind(new_claim.claimed_participant2_score)
        .bind(&new_claim.game_results)
        .bind(new_claim.auto_confirm_at)
        .bind(&new_claim.evidence_ids)
        .bind(&new_claim.demo_link_ids)
        .bind(&new_claim.submitter_notes)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create result claim: {e}")))?;

        row_to_domain(row)
    }

    async fn update(
        &self,
        id: ResultClaimId,
        update: DomainUpdateResultClaim,
    ) -> Result<ResultClaim, DomainError> {
        // Build dynamic update query
        let mut set_clauses = vec!["updated_at = NOW()".to_string()];
        let mut param_index = 2; // $1 is the id

        if update.status.is_some() {
            set_clauses.push(format!("status = ${param_index}"));
            param_index += 1;
        }
        if update.confirmed_at.is_some() {
            set_clauses.push(format!("confirmed_at = ${param_index}"));
            param_index += 1;
        }
        if update.confirmed_by_registration_id.is_some() {
            set_clauses.push(format!("confirmed_by_registration_id = ${param_index}"));
            param_index += 1;
        }
        if update.confirmed_by_user_id.is_some() {
            set_clauses.push(format!("confirmed_by_user_id = ${param_index}"));
            param_index += 1;
        }
        if update.was_auto_confirmed.is_some() {
            set_clauses.push(format!("was_auto_confirmed = ${param_index}"));
        }

        let query = format!(
            "UPDATE result_claims SET {} WHERE id = $1 RETURNING *",
            set_clauses.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, ResultClaimRow>(&query).bind(id.as_uuid());

        if let Some(ref status) = update.status {
            query_builder = query_builder.bind(status.to_string());
        }
        if let Some(confirmed_at) = update.confirmed_at {
            query_builder = query_builder.bind(confirmed_at);
        }
        if let Some(reg_id) = update.confirmed_by_registration_id {
            query_builder = query_builder.bind(reg_id.as_uuid());
        }
        if let Some(user_id) = update.confirmed_by_user_id {
            query_builder = query_builder.bind(user_id.as_uuid());
        }
        if let Some(was_auto) = update.was_auto_confirmed {
            query_builder = query_builder.bind(was_auto);
        }

        let row = query_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to update result claim: {e}")))?;

        row_to_domain(row)
    }

    async fn update_status(
        &self,
        id: ResultClaimId,
        status: ClaimStatus,
    ) -> Result<ResultClaim, DomainError> {
        let row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            UPDATE result_claims
            SET status = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update result claim status: {e}")))?;

        row_to_domain(row)
    }

    async fn confirm(
        &self,
        id: ResultClaimId,
        confirmed_by_registration_id: TournamentRegistrationId,
        confirmed_by_user_id: UserId,
        was_auto: bool,
    ) -> Result<ResultClaim, DomainError> {
        let row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            UPDATE result_claims
            SET status = 'confirmed',
                confirmed_at = NOW(),
                confirmed_by_registration_id = $2,
                confirmed_by_user_id = $3,
                was_auto_confirmed = $4,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(confirmed_by_registration_id.as_uuid())
        .bind(confirmed_by_user_id.as_uuid())
        .bind(was_auto)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to confirm result claim: {e}")))?;

        row_to_domain(row)
    }

    async fn confirm_and_apply_to_match(
        &self,
        id: ResultClaimId,
        confirmed_by_registration_id: TournamentRegistrationId,
        confirmed_by_user_id: UserId,
        was_auto: bool,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        loser_registration_id: TournamentRegistrationId,
        participant1_score: i32,
        participant2_score: i32,
    ) -> Result<ResultClaim, DomainError> {
        // Both the claim-side Confirm and the match-side submit_result
        // commit in one tx or neither does. The old split
        // `confirm(...) + submit_result(...) + complete(...)` chain
        // could leave a Confirmed claim pointing at an incomplete
        // match — a dangling FK target the bracket-progression saga
        // would then trip over. Note: `submit_result` also sets
        // `status = 'completed'` on the match, so a separate
        // `complete(...)` write isn't needed.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        let claim_row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            UPDATE result_claims
            SET status = 'confirmed',
                confirmed_at = NOW(),
                confirmed_by_registration_id = $2,
                confirmed_by_user_id = $3,
                was_auto_confirmed = $4,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(confirmed_by_registration_id.as_uuid())
        .bind(confirmed_by_user_id.as_uuid())
        .bind(was_auto)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to confirm result claim: {e}")))?;

        sqlx::query(
            r"
            UPDATE tournament_matches SET
                participant1_score = $2,
                participant2_score = $3,
                winner_registration_id = $4,
                loser_registration_id = $5,
                completed_at = NOW(),
                status = 'completed',
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(match_id.as_uuid())
        .bind(participant1_score)
        .bind(participant2_score)
        .bind(winner_registration_id.as_uuid())
        .bind(loser_registration_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to apply match result: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        row_to_domain(claim_row)
    }

    async fn create_and_supersede_pending(
        &self,
        claim: CreateResultClaim,
    ) -> Result<ResultClaim, DomainError> {
        // One tx: supersede any pre-existing pending claim for the
        // match, then insert the new one. The split version left the
        // match in a claim-less state if the create failed after the
        // supersede succeeded. See audit I5.
        let game_results_json = serde_json::to_value(&claim.game_results)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize game results: {e}")))?;

        let evidence_uuids: Vec<_> = claim
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();
        let demo_link_uuids: Vec<_> = claim
            .demo_link_ids
            .iter()
            .map(portal_core::DemoMatchLinkId::as_uuid)
            .collect();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to begin transaction: {e}")))?;

        // Supersede any existing pending claim for this match. No row
        // is fine — first submission has nothing to supersede.
        sqlx::query(
            r"
            UPDATE result_claims
            SET status = 'superseded', updated_at = NOW()
            WHERE match_id = $1 AND status = 'pending'
            ",
        )
        .bind(claim.match_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to supersede pending claim: {e}")))?;

        let row = sqlx::query_as::<_, ResultClaimRow>(
            r"
            INSERT INTO result_claims (
                match_id, submitted_by_registration_id, submitted_by_user_id,
                claimed_winner_registration_id, claimed_participant1_score,
                claimed_participant2_score, game_results, auto_confirm_at,
                evidence_ids, demo_link_ids, submitter_notes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(claim.match_id.as_uuid())
        .bind(claim.submitted_by_registration_id.as_uuid())
        .bind(claim.submitted_by_user_id.as_uuid())
        .bind(claim.claimed_winner_registration_id.as_uuid())
        .bind(claim.participant1_score)
        .bind(claim.participant2_score)
        .bind(&game_results_json)
        .bind(claim.auto_confirm_at)
        .bind(&evidence_uuids)
        .bind(&demo_link_uuids)
        .bind(&claim.notes)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create result claim: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to commit: {e}")))?;

        row_to_domain(row)
    }

    async fn supersede_pending_claims(
        &self,
        match_id: TournamentMatchId,
        except_claim_id: ResultClaimId,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE result_claims
            SET status = 'superseded', updated_at = NOW()
            WHERE match_id = $1
              AND id != $2
              AND status = 'pending'
            ",
        )
        .bind(match_id.as_uuid())
        .bind(except_claim_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to supersede pending claims: {e}")))?;

        Ok(())
    }

    async fn find_ready_for_auto_confirm(&self) -> Result<Vec<ResultClaim>, DomainError> {
        let rows = sqlx::query_as::<_, ResultClaimRow>(
            r"
            SELECT * FROM result_claims
            WHERE status = 'pending'
              AND auto_confirm_at IS NOT NULL
              AND auto_confirm_at <= NOW()
            ORDER BY auto_confirm_at ASC
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            DomainError::Internal(format!("Failed to find claims ready for auto-confirm: {e}"))
        })?;

        rows.into_iter().map(row_to_domain).collect()
    }
}

// =============================================================================
// CONVERSION FUNCTIONS
// =============================================================================

fn row_to_domain(row: ResultClaimRow) -> Result<ResultClaim, DomainError> {
    let status: ClaimStatus = row
        .status
        .parse()
        .map_err(|e: String| DomainError::Internal(format!("Invalid claim status: {e}")))?;

    let game_results: Vec<GameResult> = serde_json::from_value(row.game_results)
        .map_err(|e| DomainError::Internal(format!("Failed to deserialize game results: {e}")))?;

    Ok(ResultClaim {
        id: ResultClaimId::from_uuid(row.id),
        match_id: TournamentMatchId::from_uuid(row.match_id),
        submitted_by_registration_id: TournamentRegistrationId::from_uuid(
            row.submitted_by_registration_id,
        ),
        submitted_by_user_id: UserId::from_uuid(row.submitted_by_user_id),
        claimed_winner_registration_id: TournamentRegistrationId::from_uuid(
            row.claimed_winner_registration_id,
        ),
        claimed_participant1_score: row.claimed_participant1_score,
        claimed_participant2_score: row.claimed_participant2_score,
        game_results,
        status,
        confirmed_at: row.confirmed_at,
        confirmed_by_registration_id: row
            .confirmed_by_registration_id
            .map(TournamentRegistrationId::from_uuid),
        confirmed_by_user_id: row.confirmed_by_user_id.map(UserId::from_uuid),
        auto_confirm_at: row.auto_confirm_at,
        was_auto_confirmed: row.was_auto_confirmed,
        evidence_ids: row
            .evidence_ids
            .into_iter()
            .map(EvidenceId::from_uuid)
            .collect(),
        demo_link_ids: row
            .demo_link_ids
            .into_iter()
            .map(DemoMatchLinkId::from_uuid)
            .collect(),
        submitter_notes: row.submitter_notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
