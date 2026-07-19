//! Veto lobby message repository adapter.

use crate::DbPool;
use crate::entities::VetoLobbyMessageRow;
use async_trait::async_trait;
use portal_core::{
    DomainError, TournamentMatchId, TournamentRegistrationId, UserId, VetoLobbyMessageId,
    VetoSessionId,
};
use portal_domain::entities::veto_lobby_message::{VetoLobbyMessage, VetoMessageType};
use portal_domain::repositories::veto_lobby_message::{
    CreateVetoLobbyMessage, VetoLobbyMessageRepository,
};
use sqlx::Row;

// =============================================================================
// Type Conversions
// =============================================================================

impl From<VetoLobbyMessageRow> for VetoLobbyMessage {
    fn from(row: VetoLobbyMessageRow) -> Self {
        Self {
            id: VetoLobbyMessageId::from(row.id),
            match_id: TournamentMatchId::from(row.match_id),
            veto_session_id: row.veto_session_id.map(VetoSessionId::from),
            author_user_id: UserId::from(row.author_user_id),
            author_registration_id: row
                .author_registration_id
                .map(TournamentRegistrationId::from),
            message_type: row.message_type.parse().unwrap_or(VetoMessageType::System),
            content: row.content,
            team_registration_id: row.team_registration_id.map(TournamentRegistrationId::from),
            created_at: row.created_at,
        }
    }
}

// =============================================================================
// Repository Adapter
// =============================================================================

/// `PostgreSQL` implementation of the domain `VetoLobbyMessageRepository` trait.
#[derive(Clone)]
pub struct PgVetoLobbyMessageRepository {
    pool: DbPool,
}

impl PgVetoLobbyMessageRepository {
    /// Create a new `PostgreSQL` veto lobby message repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VetoLobbyMessageRepository for PgVetoLobbyMessageRepository {
    async fn create(&self, cmd: CreateVetoLobbyMessage) -> Result<VetoLobbyMessage, DomainError> {
        let message = sqlx::query_as::<_, VetoLobbyMessageRow>(
            r"
            INSERT INTO veto_lobby_messages (
                match_id, veto_session_id, author_user_id, author_registration_id,
                message_type, content, team_registration_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(cmd.match_id.as_uuid())
        .bind(cmd.veto_session_id.map(|id| id.as_uuid()))
        .bind(cmd.author_user_id.as_uuid())
        .bind(cmd.author_registration_id.map(|id| id.as_uuid()))
        .bind(cmd.message_type.to_string())
        .bind(&cmd.content)
        .bind(cmd.team_registration_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(VetoLobbyMessage::from(message))
    }

    async fn find_by_id(
        &self,
        id: VetoLobbyMessageId,
    ) -> Result<Option<VetoLobbyMessage>, DomainError> {
        let message = sqlx::query_as::<_, VetoLobbyMessageRow>(
            "SELECT * FROM veto_lobby_messages WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(message.map(VetoLobbyMessage::from))
    }

    async fn list_public_messages(
        &self,
        match_id: TournamentMatchId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        let messages = sqlx::query_as::<_, VetoLobbyMessageRow>(
            r"
            SELECT * FROM veto_lobby_messages
            WHERE match_id = $1 AND message_type IN ('all', 'admin', 'system')
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(match_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(messages.into_iter().map(VetoLobbyMessage::from).collect())
    }

    async fn list_team_messages(
        &self,
        match_id: TournamentMatchId,
        team_registration_id: TournamentRegistrationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        let messages = sqlx::query_as::<_, VetoLobbyMessageRow>(
            r"
            SELECT * FROM veto_lobby_messages
            WHERE match_id = $1 AND message_type = 'team' AND team_registration_id = $2
            ORDER BY created_at ASC
            LIMIT $3 OFFSET $4
            ",
        )
        .bind(match_id.as_uuid())
        .bind(team_registration_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(messages.into_iter().map(VetoLobbyMessage::from).collect())
    }

    async fn list_all_messages(
        &self,
        match_id: TournamentMatchId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        let messages = sqlx::query_as::<_, VetoLobbyMessageRow>(
            r"
            SELECT * FROM veto_lobby_messages
            WHERE match_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            ",
        )
        .bind(match_id.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(messages.into_iter().map(VetoLobbyMessage::from).collect())
    }

    async fn count_by_match(&self, match_id: TournamentMatchId) -> Result<i64, DomainError> {
        let row =
            sqlx::query("SELECT COUNT(*) as count FROM veto_lobby_messages WHERE match_id = $1")
                .bind(match_id.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.get("count"))
    }
}
