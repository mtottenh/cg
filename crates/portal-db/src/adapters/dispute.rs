//! Dispute repository adapters.

use crate::DbPool;
use crate::entities::{DisputeMessageRow, DisputeRow};
use async_trait::async_trait;
use portal_core::{
    DisputeId, DisputeMessageId, DomainError, EvidenceId, ResultClaimId, TournamentId,
    TournamentMatchId, TournamentRegistrationId, UserId,
};
use portal_domain::entities::dispute::{
    AuthorType, Dispute, DisputeMessage, DisputePriority, DisputeReason, DisputeResolution,
    DisputeStatus, ResolutionType,
};
use portal_domain::repositories::dispute::{
    CreateDispute, CreateDisputeMessage, DisputeMessageRepository, DisputeRepository, UpdateDispute,
};
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<DisputeRow> for Dispute {
    fn from(row: DisputeRow) -> Self {
        let resolution = if let Some(resolution_type) = row.resolution_type {
            Some(DisputeResolution {
                resolution_type: resolution_type.parse().unwrap_or(ResolutionType::Upheld),
                notes: row.resolution_notes.unwrap_or_default(),
                new_winner_registration_id: row
                    .new_winner_registration_id
                    .map(TournamentRegistrationId::from),
                new_participant1_score: row.new_participant1_score,
                new_participant2_score: row.new_participant2_score,
            })
        } else {
            None
        };

        Self {
            id: DisputeId::from(row.id),
            match_id: TournamentMatchId::from(row.match_id),
            result_claim_id: row.result_claim_id.map(ResultClaimId::from),
            disputed_by_registration_id: TournamentRegistrationId::from(
                row.disputed_by_registration_id,
            ),
            disputed_by_user_id: UserId::from(row.disputed_by_user_id),
            reason: row.reason.parse().unwrap_or(DisputeReason::Other),
            description: row.description,
            evidence_ids: row.evidence_ids.into_iter().map(EvidenceId::from).collect(),
            original_winner_registration_id: row
                .original_winner_registration_id
                .map(TournamentRegistrationId::from),
            original_participant1_score: row.original_participant1_score,
            original_participant2_score: row.original_participant2_score,
            status: row.status.parse().unwrap_or_default(),
            priority: row.priority.parse().unwrap_or_default(),
            resolved_at: row.resolved_at,
            resolved_by_user_id: row.resolved_by_user_id.map(UserId::from),
            resolution,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<DisputeMessageRow> for DisputeMessage {
    fn from(row: DisputeMessageRow) -> Self {
        Self {
            id: DisputeMessageId::from(row.id),
            dispute_id: DisputeId::from(row.dispute_id),
            author_user_id: UserId::from(row.author_user_id),
            author_type: row.author_type.parse().unwrap_or(AuthorType::System),
            message: row.message,
            evidence_ids: row.evidence_ids.into_iter().map(EvidenceId::from).collect(),
            is_internal: row.is_internal,
            created_at: row.created_at,
        }
    }
}

// =============================================================================
// Dispute Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `DisputeRepository` trait.
#[derive(Clone)]
pub struct PgDisputeRepository {
    pool: DbPool,
}

impl PgDisputeRepository {
    /// Create a new `PostgreSQL` dispute repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DisputeRepository for PgDisputeRepository {
    async fn create(&self, data: CreateDispute) -> Result<Dispute, DomainError> {
        let evidence_ids: Vec<uuid::Uuid> = data
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        let dispute = sqlx::query_as::<_, DisputeRow>(
            r"
            INSERT INTO disputes (
                match_id, result_claim_id, disputed_by_registration_id, disputed_by_user_id,
                reason, description, evidence_ids, original_winner_registration_id,
                original_participant1_score, original_participant2_score, priority
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(data.match_id.as_uuid())
        .bind(data.result_claim_id.map(|id| id.as_uuid()))
        .bind(data.disputed_by_registration_id.as_uuid())
        .bind(data.disputed_by_user_id.as_uuid())
        .bind(data.reason.to_string())
        .bind(&data.description)
        .bind(&evidence_ids)
        .bind(data.original_winner_registration_id.map(|id| id.as_uuid()))
        .bind(data.original_participant1_score)
        .bind(data.original_participant2_score)
        .bind(data.priority.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Dispute::from(dispute))
    }

    async fn find_by_id(&self, id: DisputeId) -> Result<Option<Dispute>, DomainError> {
        let dispute = sqlx::query_as::<_, DisputeRow>("SELECT * FROM disputes WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(dispute.map(Dispute::from))
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<Dispute>, DomainError> {
        let disputes = sqlx::query_as::<_, DisputeRow>(
            "SELECT * FROM disputes WHERE match_id = $1 ORDER BY created_at DESC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(disputes.into_iter().map(Dispute::from).collect())
    }

    async fn find_pending_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<Dispute>, DomainError> {
        let dispute = sqlx::query_as::<_, DisputeRow>(
            r"
            SELECT * FROM disputes
            WHERE match_id = $1 AND status IN ('pending', 'under_review')
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(dispute.map(Dispute::from))
    }

    async fn find_pending(
        &self,
        tournament_id: Option<TournamentId>,
        priority: Option<DisputePriority>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Dispute>, i64), DomainError> {
        // Build query dynamically based on filters
        let base_query = r"
            SELECT d.* FROM disputes d
            JOIN tournament_matches m ON d.match_id = m.id
            WHERE d.status IN ('pending', 'under_review')
        ";

        let mut conditions = Vec::new();
        let mut param_count = 0;

        if tournament_id.is_some() {
            param_count += 1;
            conditions.push(format!("m.tournament_id = ${param_count}"));
        }

        if priority.is_some() {
            param_count += 1;
            conditions.push(format!("d.priority = ${param_count}"));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" AND {}", conditions.join(" AND "))
        };

        let count_query = format!(
            "SELECT COUNT(*) as count FROM disputes d JOIN tournament_matches m ON d.match_id = m.id WHERE d.status IN ('pending', 'under_review'){where_clause}"
        );

        let items_query = format!(
            "{base_query}{where_clause} ORDER BY d.priority DESC, d.created_at ASC LIMIT ${} OFFSET ${}",
            param_count + 1,
            param_count + 2
        );

        // Build and execute count query
        let mut count_builder = sqlx::query(&count_query);
        let mut items_builder = sqlx::query_as::<_, DisputeRow>(&items_query);

        if let Some(tid) = &tournament_id {
            count_builder = count_builder.bind(tid.as_uuid());
            items_builder = items_builder.bind(tid.as_uuid());
        }

        if let Some(p) = &priority {
            count_builder = count_builder.bind(p.to_string());
            items_builder = items_builder.bind(p.to_string());
        }

        items_builder = items_builder.bind(limit).bind(offset);

        let count_row = count_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let total: i64 = count_row.get("count");

        let disputes = items_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((disputes.into_iter().map(Dispute::from).collect(), total))
    }

    async fn list_filtered(
        &self,
        status: Option<DisputeStatus>,
        tournament_id: Option<TournamentId>,
        match_id: Option<TournamentMatchId>,
        priority: Option<DisputePriority>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Dispute>, i64), DomainError> {
        // Dynamic filters; unlike find_pending, no status is hard-coded —
        // `status: None` really does mean every status.
        let mut conditions = Vec::new();
        let mut param_count = 0;

        if status.is_some() {
            param_count += 1;
            conditions.push(format!("d.status = ${param_count}"));
        }
        if tournament_id.is_some() {
            param_count += 1;
            conditions.push(format!("m.tournament_id = ${param_count}"));
        }
        if match_id.is_some() {
            param_count += 1;
            conditions.push(format!("d.match_id = ${param_count}"));
        }
        if priority.is_some() {
            param_count += 1;
            conditions.push(format!("d.priority = ${param_count}"));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!(
            "SELECT COUNT(*) as count FROM disputes d \
             JOIN tournament_matches m ON d.match_id = m.id{where_clause}"
        );
        let items_query = format!(
            "SELECT d.* FROM disputes d \
             JOIN tournament_matches m ON d.match_id = m.id{where_clause} \
             ORDER BY d.priority DESC, d.created_at DESC LIMIT ${} OFFSET ${}",
            param_count + 1,
            param_count + 2
        );

        let mut count_builder = sqlx::query(&count_query);
        let mut items_builder = sqlx::query_as::<_, DisputeRow>(&items_query);

        if let Some(s) = &status {
            count_builder = count_builder.bind(s.to_string());
            items_builder = items_builder.bind(s.to_string());
        }
        if let Some(tid) = &tournament_id {
            count_builder = count_builder.bind(tid.as_uuid());
            items_builder = items_builder.bind(tid.as_uuid());
        }
        if let Some(mid) = &match_id {
            count_builder = count_builder.bind(mid.as_uuid());
            items_builder = items_builder.bind(mid.as_uuid());
        }
        if let Some(p) = &priority {
            count_builder = count_builder.bind(p.to_string());
            items_builder = items_builder.bind(p.to_string());
        }
        items_builder = items_builder.bind(limit).bind(offset);

        let count_row = count_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let total: i64 = count_row.get("count");

        let disputes = items_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((disputes.into_iter().map(Dispute::from).collect(), total))
    }

    async fn update(&self, id: DisputeId, data: UpdateDispute) -> Result<Dispute, DomainError> {
        // Build dynamic update query
        let mut updates = vec!["updated_at = NOW()".to_string()];
        let mut param_count = 1;

        if data.status.is_some() {
            param_count += 1;
            updates.push(format!("status = ${param_count}"));
        }

        if data.priority.is_some() {
            param_count += 1;
            updates.push(format!("priority = ${param_count}"));
        }

        if data.resolved_by_user_id.is_some() {
            param_count += 1;
            updates.push(format!("resolved_by_user_id = ${param_count}"));
        }

        if data.resolution.is_some() {
            param_count += 1;
            updates.push(format!("resolution_type = ${param_count}"));
            param_count += 1;
            updates.push(format!("resolution_notes = ${param_count}"));
            param_count += 1;
            updates.push(format!("new_winner_registration_id = ${param_count}"));
            param_count += 1;
            updates.push(format!("new_participant1_score = ${param_count}"));
            param_count += 1;
            updates.push(format!("new_participant2_score = ${param_count}"));
            updates.push("resolved_at = NOW()".to_string());
        }

        let query = format!(
            "UPDATE disputes SET {} WHERE id = $1 RETURNING *",
            updates.join(", ")
        );

        let mut builder = sqlx::query_as::<_, DisputeRow>(&query).bind(id.as_uuid());

        if let Some(status) = &data.status {
            builder = builder.bind(status.to_string());
        }

        if let Some(priority) = &data.priority {
            builder = builder.bind(priority.to_string());
        }

        if let Some(user_id) = &data.resolved_by_user_id {
            builder = builder.bind(user_id.as_uuid());
        }

        if let Some(resolution) = &data.resolution {
            builder = builder.bind(resolution.resolution_type.to_string());
            builder = builder.bind(&resolution.notes);
            builder = builder.bind(resolution.new_winner_registration_id.map(|id| id.as_uuid()));
            builder = builder.bind(resolution.new_participant1_score);
            builder = builder.bind(resolution.new_participant2_score);
        }

        let dispute = builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Dispute::from(dispute))
    }

    async fn exists_pending_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<bool, DomainError> {
        let row = sqlx::query(
            "SELECT 1 FROM disputes WHERE match_id = $1 AND status IN ('pending', 'under_review')",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn resolve(
        &self,
        id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
    ) -> Result<Dispute, DomainError> {
        let dispute = sqlx::query_as::<_, DisputeRow>(
            r"
            UPDATE disputes SET
                status = 'resolved',
                resolved_at = NOW(),
                resolved_by_user_id = $2,
                resolution_type = $3,
                resolution_notes = $4,
                new_winner_registration_id = $5,
                new_participant1_score = $6,
                new_participant2_score = $7,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(resolved_by.as_uuid())
        .bind(resolution.resolution_type.to_string())
        .bind(&resolution.notes)
        .bind(resolution.new_winner_registration_id.map(|id| id.as_uuid()))
        .bind(resolution.new_participant1_score)
        .bind(resolution.new_participant2_score)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::DisputeNotFound(id))?;

        Ok(Dispute::from(dispute))
    }

    async fn cancel(&self, id: DisputeId) -> Result<Dispute, DomainError> {
        let dispute = sqlx::query_as::<_, DisputeRow>(
            r"
            UPDATE disputes SET
                status = 'cancelled',
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::DisputeNotFound(id))?;

        Ok(Dispute::from(dispute))
    }

    async fn resolve_with_status_change(
        &self,
        dispute_id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
        match_id: TournamentMatchId,
        new_match_status: portal_core::types::TournamentMatchStatus,
        resolution_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError> {
        // Atomic counterpart of the `dispute_repo.resolve + match_repo.
        // update_status + message_repo.create` chain used by
        // uphold / rematch / double_dq. The three writes run in one tx
        // so an admin action never lands half-applied — previously the
        // dispute could be marked Resolved but the match left in
        // `Disputed`, which blocks every subsequent operation on it.
        // See audit I5.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let dispute_row = sqlx::query_as::<_, DisputeRow>(
            r"
            UPDATE disputes SET
                status = 'resolved',
                resolved_at = NOW(),
                resolved_by_user_id = $2,
                resolution_type = $3,
                resolution_notes = $4,
                new_winner_registration_id = $5,
                new_participant1_score = $6,
                new_participant2_score = $7,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(dispute_id.as_uuid())
        .bind(resolved_by.as_uuid())
        .bind(resolution.resolution_type.to_string())
        .bind(&resolution.notes)
        .bind(resolution.new_winner_registration_id.map(|id| id.as_uuid()))
        .bind(resolution.new_participant1_score)
        .bind(resolution.new_participant2_score)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::DisputeNotFound(dispute_id))?;

        sqlx::query(
            r"
            UPDATE tournament_matches SET
                status = $2,
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(match_id.as_uuid())
        .bind(new_match_status.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let msg_evidence_ids: Vec<uuid::Uuid> = resolution_message
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        sqlx::query(
            r"
            INSERT INTO dispute_messages (
                dispute_id, author_user_id, author_type, message, evidence_ids, is_internal
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(dispute_id.as_uuid())
        .bind(resolution_message.author_user_id.as_uuid())
        .bind(resolution_message.author_type.to_string())
        .bind(&resolution_message.message)
        .bind(&msg_evidence_ids)
        .bind(resolution_message.is_internal)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Dispute::from(dispute_row))
    }

    async fn cancel_with_match_restore(
        &self,
        dispute_id: DisputeId,
        match_id: TournamentMatchId,
        cancellation_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError> {
        // Atomic counterpart of cancel + update_status(Completed) +
        // message_repo.create. Same rationale as
        // `resolve_with_status_change`; the difference is that this
        // one flips the dispute to Cancelled (not Resolved) and
        // always restores the match to Completed.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let dispute_row = sqlx::query_as::<_, DisputeRow>(
            r"
            UPDATE disputes SET
                status = 'cancelled',
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(dispute_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::DisputeNotFound(dispute_id))?;

        sqlx::query(
            r"
            UPDATE tournament_matches SET
                status = 'completed',
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(match_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let msg_evidence_ids: Vec<uuid::Uuid> = cancellation_message
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        sqlx::query(
            r"
            INSERT INTO dispute_messages (
                dispute_id, author_user_id, author_type, message, evidence_ids, is_internal
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(dispute_id.as_uuid())
        .bind(cancellation_message.author_user_id.as_uuid())
        .bind(cancellation_message.author_type.to_string())
        .bind(&cancellation_message.message)
        .bind(&msg_evidence_ids)
        .bind(cancellation_message.is_internal)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Dispute::from(dispute_row))
    }

    async fn raise_atomic(
        &self,
        create: CreateDispute,
        initial_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError> {
        // Insert dispute + flip match status + append message, all in
        // one tx. The `match_id` on the incoming message is a
        // placeholder because the caller doesn't yet know the dispute
        // id — we overwrite it after the INSERT below. See audit I5.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let create_evidence_ids: Vec<uuid::Uuid> = create
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        let dispute_row = sqlx::query_as::<_, DisputeRow>(
            r"
            INSERT INTO disputes (
                match_id, result_claim_id, disputed_by_registration_id, disputed_by_user_id,
                reason, description, evidence_ids, original_winner_registration_id,
                original_participant1_score, original_participant2_score, priority
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(create.match_id.as_uuid())
        .bind(create.result_claim_id.map(|id| id.as_uuid()))
        .bind(create.disputed_by_registration_id.as_uuid())
        .bind(create.disputed_by_user_id.as_uuid())
        .bind(create.reason.to_string())
        .bind(&create.description)
        .bind(&create_evidence_ids)
        .bind(
            create
                .original_winner_registration_id
                .map(|id| id.as_uuid()),
        )
        .bind(create.original_participant1_score)
        .bind(create.original_participant2_score)
        .bind(create.priority.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let dispute = Dispute::from(dispute_row);

        sqlx::query(
            r"
            UPDATE tournament_matches SET
                status = 'disputed',
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(create.match_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let msg_evidence_ids: Vec<uuid::Uuid> = initial_message
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        sqlx::query(
            r"
            INSERT INTO dispute_messages (
                dispute_id, author_user_id, author_type, message, evidence_ids, is_internal
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(dispute.id.as_uuid())
        .bind(initial_message.author_user_id.as_uuid())
        .bind(initial_message.author_type.to_string())
        .bind(&initial_message.message)
        .bind(&msg_evidence_ids)
        .bind(initial_message.is_internal)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(dispute)
    }

    async fn resolve_with_overturn(
        &self,
        dispute_id: DisputeId,
        resolved_by: UserId,
        resolution: DisputeResolution,
        match_id: TournamentMatchId,
        new_winner_registration_id: TournamentRegistrationId,
        new_loser_registration_id: TournamentRegistrationId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        resolution_message: CreateDisputeMessage,
    ) -> Result<Dispute, DomainError> {
        // Resolve dispute + overwrite match result + append message,
        // all atomically. Previously the service did these as four
        // sequential calls; a failure between them left admins staring
        // at a Resolved dispute whose match still showed the disputed
        // result, and the bracket progression would advance the wrong
        // winner. See audit I5.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let dispute_row = sqlx::query_as::<_, DisputeRow>(
            r"
            UPDATE disputes SET
                status = 'resolved',
                resolved_at = NOW(),
                resolved_by_user_id = $2,
                resolution_type = $3,
                resolution_notes = $4,
                new_winner_registration_id = $5,
                new_participant1_score = $6,
                new_participant2_score = $7,
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(dispute_id.as_uuid())
        .bind(resolved_by.as_uuid())
        .bind(resolution.resolution_type.to_string())
        .bind(&resolution.notes)
        .bind(resolution.new_winner_registration_id.map(|id| id.as_uuid()))
        .bind(resolution.new_participant1_score)
        .bind(resolution.new_participant2_score)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::DisputeNotFound(dispute_id))?;

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
        .bind(new_participant1_score)
        .bind(new_participant2_score)
        .bind(new_winner_registration_id.as_uuid())
        .bind(new_loser_registration_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let msg_evidence_ids: Vec<uuid::Uuid> = resolution_message
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        sqlx::query(
            r"
            INSERT INTO dispute_messages (
                dispute_id, author_user_id, author_type, message, evidence_ids, is_internal
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(dispute_id.as_uuid())
        .bind(resolution_message.author_user_id.as_uuid())
        .bind(resolution_message.author_type.to_string())
        .bind(&resolution_message.message)
        .bind(&msg_evidence_ids)
        .bind(resolution_message.is_internal)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(Dispute::from(dispute_row))
    }
}

// =============================================================================
// Dispute Message Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `DisputeMessageRepository` trait.
#[derive(Clone)]
pub struct PgDisputeMessageRepository {
    pool: DbPool,
}

impl PgDisputeMessageRepository {
    /// Create a new `PostgreSQL` dispute message repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DisputeMessageRepository for PgDisputeMessageRepository {
    async fn create(&self, data: CreateDisputeMessage) -> Result<DisputeMessage, DomainError> {
        let evidence_ids: Vec<uuid::Uuid> = data
            .evidence_ids
            .iter()
            .map(portal_core::EvidenceId::as_uuid)
            .collect();

        let message = sqlx::query_as::<_, DisputeMessageRow>(
            r"
            INSERT INTO dispute_messages (
                dispute_id, author_user_id, author_type, message, evidence_ids, is_internal
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(data.dispute_id.as_uuid())
        .bind(data.author_user_id.as_uuid())
        .bind(data.author_type.to_string())
        .bind(&data.message)
        .bind(&evidence_ids)
        .bind(data.is_internal)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(DisputeMessage::from(message))
    }

    async fn find_by_id(
        &self,
        id: DisputeMessageId,
    ) -> Result<Option<DisputeMessage>, DomainError> {
        let message =
            sqlx::query_as::<_, DisputeMessageRow>("SELECT * FROM dispute_messages WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(message.map(DisputeMessage::from))
    }

    async fn find_by_dispute(
        &self,
        dispute_id: DisputeId,
        include_internal: bool,
    ) -> Result<Vec<DisputeMessage>, DomainError> {
        let query = if include_internal {
            "SELECT * FROM dispute_messages WHERE dispute_id = $1 ORDER BY created_at ASC"
        } else {
            "SELECT * FROM dispute_messages WHERE dispute_id = $1 AND is_internal = false ORDER BY created_at ASC"
        };

        let messages = sqlx::query_as::<_, DisputeMessageRow>(query)
            .bind(dispute_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(messages.into_iter().map(DisputeMessage::from).collect())
    }

    async fn count_by_dispute(&self, dispute_id: DisputeId) -> Result<i64, DomainError> {
        let row =
            sqlx::query("SELECT COUNT(*) as count FROM dispute_messages WHERE dispute_id = $1")
                .bind(dispute_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }
}
