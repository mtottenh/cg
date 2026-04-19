//! Veto lobby chat service.
//!
//! Handles chat message creation and retrieval for veto lobby WebSocket sessions.

use std::sync::Arc;

use portal_core::{DomainError, TournamentMatchId, TournamentRegistrationId, UserId, VetoSessionId};

use crate::entities::veto_lobby_message::{VetoLobbyMessage, VetoMessageType};
use crate::repositories::veto_lobby_message::{CreateVetoLobbyMessage, VetoLobbyMessageRepository};
use crate::repositories::tournament::{TournamentMatchRepository, TournamentRegistrationRepository};

/// Configuration for the veto lobby chat service.
#[derive(Debug, Clone)]
pub struct VetoLobbyChatConfig {
    /// Maximum message length in characters.
    pub max_message_length: usize,
    /// Maximum messages to load for history.
    pub max_history_messages: i64,
}

impl Default for VetoLobbyChatConfig {
    fn default() -> Self {
        Self {
            max_message_length: 500,
            max_history_messages: 100,
        }
    }
}

/// Service for veto lobby chat operations.
pub struct VetoLobbyChatService<VLMR, TMR, TRR>
where
    VLMR: VetoLobbyMessageRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    message_repo: Arc<VLMR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    config: VetoLobbyChatConfig,
}

impl<VLMR, TMR, TRR> VetoLobbyChatService<VLMR, TMR, TRR>
where
    VLMR: VetoLobbyMessageRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new veto lobby chat service.
    pub fn new(
        message_repo: Arc<VLMR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
    ) -> Self {
        Self {
            message_repo,
            match_repo,
            registration_repo,
            config: VetoLobbyChatConfig::default(),
        }
    }

    /// Create a new service with custom configuration.
    pub fn with_config(
        message_repo: Arc<VLMR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
        config: VetoLobbyChatConfig,
    ) -> Self {
        Self {
            message_repo,
            match_repo,
            registration_repo,
            config,
        }
    }

    /// Send a team chat message (private to one team).
    pub async fn send_team_message(
        &self,
        match_id: TournamentMatchId,
        user_id: UserId,
        registration_id: TournamentRegistrationId,
        veto_session_id: Option<VetoSessionId>,
        content: String,
    ) -> Result<VetoLobbyMessage, DomainError> {
        self.validate_message(&content)?;
        self.validate_match_exists(match_id).await?;

        self.message_repo
            .create(CreateVetoLobbyMessage {
                match_id,
                veto_session_id,
                author_user_id: user_id,
                author_registration_id: Some(registration_id),
                message_type: VetoMessageType::Team,
                content,
                team_registration_id: Some(registration_id),
            })
            .await
    }

    /// Send an all-chat message (visible to everyone).
    pub async fn send_all_message(
        &self,
        match_id: TournamentMatchId,
        user_id: UserId,
        registration_id: Option<TournamentRegistrationId>,
        veto_session_id: Option<VetoSessionId>,
        content: String,
    ) -> Result<VetoLobbyMessage, DomainError> {
        self.validate_message(&content)?;
        self.validate_match_exists(match_id).await?;

        self.message_repo
            .create(CreateVetoLobbyMessage {
                match_id,
                veto_session_id,
                author_user_id: user_id,
                author_registration_id: registration_id,
                message_type: VetoMessageType::All,
                content,
                team_registration_id: None,
            })
            .await
    }

    /// Send an admin message (from staff, visible to all).
    pub async fn send_admin_message(
        &self,
        match_id: TournamentMatchId,
        user_id: UserId,
        veto_session_id: Option<VetoSessionId>,
        content: String,
    ) -> Result<VetoLobbyMessage, DomainError> {
        self.validate_message(&content)?;
        self.validate_match_exists(match_id).await?;

        self.message_repo
            .create(CreateVetoLobbyMessage {
                match_id,
                veto_session_id,
                author_user_id: user_id,
                author_registration_id: None,
                message_type: VetoMessageType::Admin,
                content,
                team_registration_id: None,
            })
            .await
    }

    /// Send a system message (automated, visible to all).
    ///
    /// Uses a nil UUID as the author since system messages have no human author.
    pub async fn send_system_message(
        &self,
        match_id: TournamentMatchId,
        veto_session_id: Option<VetoSessionId>,
        content: String,
    ) -> Result<VetoLobbyMessage, DomainError> {
        // System messages bypass length validation since they're automated
        self.validate_match_exists(match_id).await?;

        // Use nil UUID for system messages
        let system_user_id = UserId::from(uuid::Uuid::nil());

        self.message_repo
            .create(CreateVetoLobbyMessage {
                match_id,
                veto_session_id,
                author_user_id: system_user_id,
                author_registration_id: None,
                message_type: VetoMessageType::System,
                content,
                team_registration_id: None,
            })
            .await
    }

    /// Get chat history for a participant (includes their team messages + public).
    ///
    /// Messages are returned in chronological order (oldest first).
    pub async fn get_participant_history(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        let limit = self.config.max_history_messages;

        // Get public messages
        let public = self.message_repo.list_public_messages(match_id, limit, 0).await?;

        // Get team messages for this participant
        let team = self.message_repo.list_team_messages(match_id, registration_id, limit, 0).await?;

        // Merge and sort by timestamp
        let mut all: Vec<_> = public.into_iter().chain(team).collect();
        all.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        // Truncate to limit
        all.truncate(limit as usize);

        Ok(all)
    }

    /// Get chat history for a spectator (public messages only).
    ///
    /// Messages are returned in chronological order (oldest first).
    pub async fn get_spectator_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        self.message_repo
            .list_public_messages(match_id, self.config.max_history_messages, 0)
            .await
    }

    /// Get all chat history (admin view, includes all team messages).
    ///
    /// Messages are returned in chronological order (oldest first).
    pub async fn get_admin_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError> {
        self.message_repo
            .list_all_messages(match_id, self.config.max_history_messages, 0)
            .await
    }

    /// Validate message content.
    fn validate_message(&self, content: &str) -> Result<(), DomainError> {
        let trimmed = content.trim();

        if trimmed.is_empty() {
            return Err(DomainError::InvalidState("Message cannot be empty".into()));
        }

        if trimmed.len() > self.config.max_message_length {
            return Err(DomainError::InvalidState(format!(
                "Message exceeds maximum length of {} characters",
                self.config.max_message_length
            )));
        }

        Ok(())
    }

    /// Validate that the match exists.
    async fn validate_match_exists(&self, match_id: TournamentMatchId) -> Result<(), DomainError> {
        let match_exists = self.match_repo.find_by_id(match_id).await?;
        if match_exists.is_none() {
            return Err(DomainError::TournamentMatchNotFound(match_id));
        }
        Ok(())
    }
}

impl<VLMR, TMR, TRR> Clone for VetoLobbyChatService<VLMR, TMR, TRR>
where
    VLMR: VetoLobbyMessageRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    fn clone(&self) -> Self {
        Self {
            message_repo: Arc::clone(&self.message_repo),
            match_repo: Arc::clone(&self.match_repo),
            registration_repo: Arc::clone(&self.registration_repo),
            config: self.config.clone(),
        }
    }
}
