//! Repository trait for veto lobby chat messages.

use async_trait::async_trait;
use portal_core::{
    DomainError, TournamentMatchId, TournamentRegistrationId, UserId, VetoLobbyMessageId,
    VetoSessionId,
};

use crate::entities::veto_lobby_message::{VetoLobbyMessage, VetoMessageType};

/// Command to create a new veto lobby message.
#[derive(Debug, Clone)]
pub struct CreateVetoLobbyMessage {
    /// The match this message belongs to.
    pub match_id: TournamentMatchId,
    /// Optional link to the veto session.
    pub veto_session_id: Option<VetoSessionId>,
    /// The user sending the message.
    pub author_user_id: UserId,
    /// The registration of the author (if a participant).
    pub author_registration_id: Option<TournamentRegistrationId>,
    /// Type of message.
    pub message_type: VetoMessageType,
    /// Message content.
    pub content: String,
    /// For team messages, which team can see this.
    pub team_registration_id: Option<TournamentRegistrationId>,
}

/// Repository trait for veto lobby message persistence.
#[async_trait]
pub trait VetoLobbyMessageRepository: Send + Sync {
    /// Create a new message.
    async fn create(&self, cmd: CreateVetoLobbyMessage) -> Result<VetoLobbyMessage, DomainError>;

    /// Find a message by ID.
    async fn find_by_id(
        &self,
        id: VetoLobbyMessageId,
    ) -> Result<Option<VetoLobbyMessage>, DomainError>;

    /// List all public messages (all, admin, system) for a match.
    ///
    /// Returns messages in chronological order (oldest first).
    async fn list_public_messages(
        &self,
        match_id: TournamentMatchId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError>;

    /// List team messages for a specific team in a match.
    ///
    /// Returns messages in chronological order (oldest first).
    async fn list_team_messages(
        &self,
        match_id: TournamentMatchId,
        team_registration_id: TournamentRegistrationId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError>;

    /// List all messages for a match (admin view).
    ///
    /// Returns messages in chronological order (oldest first).
    async fn list_all_messages(
        &self,
        match_id: TournamentMatchId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<VetoLobbyMessage>, DomainError>;

    /// Count messages in a match.
    async fn count_by_match(&self, match_id: TournamentMatchId) -> Result<i64, DomainError>;
}
