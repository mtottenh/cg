//! PostgreSQL implementations of VetoSessionRepository and VetoActionRepository.

use crate::DbPool;
use crate::entities::{NewVetoAction, NewVetoSession, VetoActionRow, VetoSessionRow};
use async_trait::async_trait;
use portal_core::{
    DomainError, TournamentMatchId, TournamentRegistrationId, UserId, VetoActionId, VetoSessionId,
};
use portal_domain::entities::veto::{VetoAction, VetoActionType, VetoSession, VetoStatus};
use portal_domain::repositories::tournament::{
    CreateVetoAction, CreateVetoSession, UpdateVetoSession as DomainUpdateVetoSession,
    VetoActionRepository, VetoSessionRepository,
};

// =============================================================================
// VETO SESSION REPOSITORY
// =============================================================================

/// PostgreSQL implementation of VetoSessionRepository.
#[derive(Debug, Clone)]
pub struct PgVetoSessionRepository {
    pool: DbPool,
}

impl PgVetoSessionRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VetoSessionRepository for PgVetoSessionRepository {
    async fn find_by_id(&self, id: VetoSessionId) -> Result<Option<VetoSession>, DomainError> {
        let row = sqlx::query_as::<_, VetoSessionRow>(r"SELECT * FROM veto_sessions WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to find veto session: {e}")))?;

        row.map(session_row_to_domain).transpose()
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<VetoSession>, DomainError> {
        let row =
            sqlx::query_as::<_, VetoSessionRow>(r"SELECT * FROM veto_sessions WHERE match_id = $1")
                .bind(match_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("Failed to find veto session by match: {e}"))
                })?;

        row.map(session_row_to_domain).transpose()
    }

    async fn create(&self, session: CreateVetoSession) -> Result<VetoSession, DomainError> {
        let new_session = NewVetoSession {
            match_id: session.match_id.as_uuid(),
            veto_format_id: session.veto_format_id,
            map_pool: session.map_pool.clone(),
            remaining_maps: session.map_pool,
            timeout_seconds: session.timeout_seconds as i32,
            side_selection_mode: session.side_selection_mode.to_string(),
        };

        let row = sqlx::query_as::<_, VetoSessionRow>(
            r"
            INSERT INTO veto_sessions (
                match_id, veto_format_id, map_pool, remaining_maps, timeout_seconds, side_selection_mode
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(new_session.match_id)
        .bind(&new_session.veto_format_id)
        .bind(&new_session.map_pool)
        .bind(&new_session.remaining_maps)
        .bind(new_session.timeout_seconds)
        .bind(&new_session.side_selection_mode)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create veto session: {e}")))?;

        session_row_to_domain(row)
    }

    async fn update(
        &self,
        id: VetoSessionId,
        update: DomainUpdateVetoSession,
    ) -> Result<VetoSession, DomainError> {
        // Build dynamic update query
        let mut set_clauses = vec!["updated_at = NOW()".to_string()];
        let mut param_index = 2; // $1 is the id

        if update.first_action_registration_id.is_some() {
            set_clauses.push(format!("first_action_registration_id = ${param_index}"));
            param_index += 1;
        }
        if update.coin_flip_winner_registration_id.is_some() {
            set_clauses.push(format!("coin_flip_winner_registration_id = ${param_index}"));
            param_index += 1;
        }
        if update.current_action_number.is_some() {
            set_clauses.push(format!("current_action_number = ${param_index}"));
            param_index += 1;
        }
        if update.current_team_turn.is_some() {
            set_clauses.push(format!("current_team_turn = ${param_index}"));
            param_index += 1;
        }
        if update.remaining_maps.is_some() {
            set_clauses.push(format!("remaining_maps = ${param_index}"));
            param_index += 1;
        }
        if update.selected_maps.is_some() {
            set_clauses.push(format!("selected_maps = ${param_index}"));
            param_index += 1;
        }
        if update.status.is_some() {
            set_clauses.push(format!("status = ${param_index}"));
            param_index += 1;
        }
        if update.action_deadline.is_some() {
            set_clauses.push(format!("action_deadline = ${param_index}"));
            param_index += 1;
        }
        if update.started_at.is_some() {
            set_clauses.push(format!("started_at = ${param_index}"));
            param_index += 1;
        }
        if update.completed_at.is_some() {
            set_clauses.push(format!("completed_at = ${param_index}"));
        }

        let query = format!(
            "UPDATE veto_sessions SET {} WHERE id = $1 RETURNING *",
            set_clauses.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, VetoSessionRow>(&query).bind(id.as_uuid());

        if let Some(reg_id) = update.first_action_registration_id {
            query_builder = query_builder.bind(reg_id.as_uuid());
        }
        if let Some(reg_id) = update.coin_flip_winner_registration_id {
            query_builder = query_builder.bind(reg_id.as_uuid());
        }
        if let Some(action_num) = update.current_action_number {
            query_builder = query_builder.bind(action_num as i32);
        }
        if let Some(team_turn) = update.current_team_turn {
            query_builder = query_builder.bind(team_turn.map(|t| t.as_uuid()));
        }
        if let Some(ref maps) = update.remaining_maps {
            query_builder = query_builder.bind(maps);
        }
        if let Some(ref maps) = update.selected_maps {
            query_builder = query_builder.bind(maps);
        }
        if let Some(ref status) = update.status {
            query_builder = query_builder.bind(status.to_string());
        }
        if let Some(deadline) = update.action_deadline {
            query_builder = query_builder.bind(deadline);
        }
        if let Some(started) = update.started_at {
            query_builder = query_builder.bind(started);
        }
        if let Some(completed) = update.completed_at {
            query_builder = query_builder.bind(completed);
        }

        let row = query_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to update veto session: {e}")))?;

        session_row_to_domain(row)
    }

    async fn update_status(
        &self,
        id: VetoSessionId,
        status: VetoStatus,
    ) -> Result<VetoSession, DomainError> {
        let row = sqlx::query_as::<_, VetoSessionRow>(
            r"
            UPDATE veto_sessions
            SET status = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update veto session status: {e}")))?;

        session_row_to_domain(row)
    }

    async fn find_timed_out(&self) -> Result<Vec<VetoSession>, DomainError> {
        let rows = sqlx::query_as::<_, VetoSessionRow>(
            r"
            SELECT * FROM veto_sessions
            WHERE status = 'in_progress'
              AND action_deadline IS NOT NULL
              AND action_deadline < NOW()
            ORDER BY action_deadline ASC
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find timed out sessions: {e}")))?;

        rows.into_iter().map(session_row_to_domain).collect()
    }

    async fn delete(&self, id: VetoSessionId) -> Result<(), DomainError> {
        sqlx::query(r"DELETE FROM veto_sessions WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to delete veto session: {e}")))?;

        Ok(())
    }
}

// =============================================================================
// VETO ACTION REPOSITORY
// =============================================================================

/// PostgreSQL implementation of VetoActionRepository.
#[derive(Debug, Clone)]
pub struct PgVetoActionRepository {
    pool: DbPool,
}

impl PgVetoActionRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VetoActionRepository for PgVetoActionRepository {
    async fn find_by_id(&self, id: VetoActionId) -> Result<Option<VetoAction>, DomainError> {
        let row = sqlx::query_as::<_, VetoActionRow>(r"SELECT * FROM veto_actions WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to find veto action: {e}")))?;

        row.map(action_row_to_domain).transpose()
    }

    async fn find_by_session_and_number(
        &self,
        session_id: VetoSessionId,
        action_number: u32,
    ) -> Result<Option<VetoAction>, DomainError> {
        let row = sqlx::query_as::<_, VetoActionRow>(
            r"
            SELECT * FROM veto_actions
            WHERE session_id = $1 AND action_number = $2
            ",
        )
        .bind(session_id.as_uuid())
        .bind(action_number as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to find veto action: {e}")))?;

        row.map(action_row_to_domain).transpose()
    }

    async fn list_by_session(
        &self,
        session_id: VetoSessionId,
    ) -> Result<Vec<VetoAction>, DomainError> {
        let rows = sqlx::query_as::<_, VetoActionRow>(
            r"
            SELECT * FROM veto_actions
            WHERE session_id = $1
            ORDER BY action_number ASC
            ",
        )
        .bind(session_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to list veto actions: {e}")))?;

        rows.into_iter().map(action_row_to_domain).collect()
    }

    async fn create(&self, action: CreateVetoAction) -> Result<VetoAction, DomainError> {
        let new_action = NewVetoAction {
            session_id: action.session_id.as_uuid(),
            action_number: action.action_number as i32,
            action_type: action.action_type.to_string(),
            map_id: action.map_id,
            performed_by_registration_id: action
                .performed_by_registration_id
                .map(|id| id.as_uuid()),
            performed_by_user_id: action.performed_by_user_id.map(|id| id.as_uuid()),
            was_auto_action: action.was_auto_action,
            auto_action_reason: action.auto_action_reason,
        };

        // Idempotent on `veto_actions_unique (session_id, action_number)`:
        // if the action row already exists the session cursor never advanced
        // (the process died between the two writes), so return the committed
        // row and let the caller finish advancing the session instead of
        // failing with a unique violation and wedging the session forever.
        let existing = sqlx::query_as::<_, VetoActionRow>(
            r"
            INSERT INTO veto_actions (
                session_id, action_number, action_type, map_id,
                performed_by_registration_id, performed_by_user_id,
                was_auto_action, auto_action_reason
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT ON CONSTRAINT veto_actions_unique DO NOTHING
            RETURNING *
            ",
        )
        .bind(new_action.session_id)
        .bind(new_action.action_number)
        .bind(&new_action.action_type)
        .bind(&new_action.map_id)
        .bind(new_action.performed_by_registration_id)
        .bind(new_action.performed_by_user_id)
        .bind(new_action.was_auto_action)
        .bind(&new_action.auto_action_reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to create veto action: {e}")))?;

        let row = match existing {
            Some(row) => row,
            None => sqlx::query_as::<_, VetoActionRow>(
                "SELECT * FROM veto_actions WHERE session_id = $1 AND action_number = $2",
            )
            .bind(new_action.session_id)
            .bind(new_action.action_number)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                DomainError::Internal(format!("Failed to load existing veto action: {e}"))
            })?,
        };

        action_row_to_domain(row)
    }

    async fn update_side_selection(
        &self,
        id: VetoActionId,
        side: String,
        selected_by: TournamentRegistrationId,
    ) -> Result<VetoAction, DomainError> {
        let row = sqlx::query_as::<_, VetoActionRow>(
            r"
            UPDATE veto_actions
            SET side_selection = $2,
                side_selected_by_registration_id = $3,
                side_selected_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&side)
        .bind(selected_by.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to update side selection: {e}")))?;

        action_row_to_domain(row)
    }
}

// =============================================================================
// CONVERSION FUNCTIONS
// =============================================================================

fn session_row_to_domain(row: VetoSessionRow) -> Result<VetoSession, DomainError> {
    use portal_domain::entities::veto::SideSelectionMode;

    let status: VetoStatus = row
        .status
        .parse()
        .map_err(|e: String| DomainError::Internal(format!("Invalid veto status: {e}")))?;

    let side_selection_mode: SideSelectionMode = row
        .side_selection_mode
        .parse()
        .unwrap_or(SideSelectionMode::Knife);

    Ok(VetoSession {
        id: VetoSessionId::from_uuid(row.id),
        match_id: TournamentMatchId::from_uuid(row.match_id),
        veto_format_id: row.veto_format_id,
        map_pool: row.map_pool,
        coin_flip_winner_registration_id: row
            .coin_flip_winner_registration_id
            .map(TournamentRegistrationId::from_uuid),
        first_action_registration_id: row
            .first_action_registration_id
            .map(TournamentRegistrationId::from_uuid),
        current_action_number: row.current_action_number as u32,
        current_team_turn: row
            .current_team_turn
            .map(TournamentRegistrationId::from_uuid),
        remaining_maps: row.remaining_maps,
        selected_maps: row.selected_maps,
        status,
        action_deadline: row.action_deadline,
        timeout_seconds: row.timeout_seconds as u32,
        side_selection_mode,
        started_at: row.started_at,
        completed_at: row.completed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn action_row_to_domain(row: VetoActionRow) -> Result<VetoAction, DomainError> {
    let action_type: VetoActionType = row
        .action_type
        .parse()
        .map_err(|e: String| DomainError::Internal(format!("Invalid veto action type: {e}")))?;

    Ok(VetoAction {
        id: VetoActionId::from_uuid(row.id),
        session_id: VetoSessionId::from_uuid(row.session_id),
        action_number: row.action_number as u32,
        action_type,
        map_id: row.map_id,
        performed_by_registration_id: row
            .performed_by_registration_id
            .map(TournamentRegistrationId::from_uuid),
        performed_by_user_id: row.performed_by_user_id.map(UserId::from_uuid),
        side_selection: row.side_selection,
        side_selected_by_registration_id: row
            .side_selected_by_registration_id
            .map(TournamentRegistrationId::from_uuid),
        side_selected_at: row.side_selected_at,
        was_auto_action: row.was_auto_action,
        auto_action_reason: row.auto_action_reason,
        performed_at: row.performed_at,
    })
}
