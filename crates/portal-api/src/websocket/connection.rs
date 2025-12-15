//! WebSocket connection types.

use chrono::{DateTime, Utc};
use portal_core::{PlayerId, TournamentRegistrationId, UserId};

/// A WebSocket connection to a veto lobby.
#[derive(Debug, Clone)]
pub struct VetoConnection {
    /// User ID of the connected user.
    pub user_id: UserId,
    /// Player ID of the connected user.
    pub player_id: PlayerId,
    /// Username for display.
    pub username: String,
    /// Role of this connection.
    pub role: ConnectionRole,
    /// Tournament registration ID if participant.
    pub registration_id: Option<TournamentRegistrationId>,
    /// Team name if participant.
    pub team_name: Option<String>,
    /// When the connection was established.
    pub connected_at: DateTime<Utc>,
}

impl VetoConnection {
    /// Create a new participant connection.
    pub fn participant(
        user_id: UserId,
        player_id: PlayerId,
        username: String,
        registration_id: TournamentRegistrationId,
        team_name: String,
    ) -> Self {
        Self {
            user_id,
            player_id,
            username,
            role: ConnectionRole::Participant,
            registration_id: Some(registration_id),
            team_name: Some(team_name),
            connected_at: Utc::now(),
        }
    }

    /// Create a new spectator connection.
    pub fn spectator(user_id: UserId, player_id: PlayerId, username: String) -> Self {
        Self {
            user_id,
            player_id,
            username,
            role: ConnectionRole::Spectator,
            registration_id: None,
            team_name: None,
            connected_at: Utc::now(),
        }
    }

    /// Create a new admin connection.
    pub fn admin(user_id: UserId, player_id: PlayerId, username: String) -> Self {
        Self {
            user_id,
            player_id,
            username,
            role: ConnectionRole::Admin,
            registration_id: None,
            team_name: None,
            connected_at: Utc::now(),
        }
    }

    /// Check if this is a spectator connection.
    #[must_use]
    pub const fn is_spectator(&self) -> bool {
        matches!(self.role, ConnectionRole::Spectator)
    }

    /// Check if this is a participant connection.
    #[must_use]
    pub const fn is_participant(&self) -> bool {
        matches!(self.role, ConnectionRole::Participant)
    }

    /// Check if this is an admin connection.
    #[must_use]
    pub const fn is_admin(&self) -> bool {
        matches!(self.role, ConnectionRole::Admin)
    }

    /// Check if this connection can send team chat.
    #[must_use]
    pub const fn can_send_team_chat(&self) -> bool {
        matches!(self.role, ConnectionRole::Participant)
    }

    /// Check if this connection can send all chat.
    #[must_use]
    pub const fn can_send_all_chat(&self) -> bool {
        // Everyone can send all chat
        true
    }

    /// Check if this connection can perform veto actions.
    #[must_use]
    pub const fn can_perform_veto_action(&self) -> bool {
        matches!(self.role, ConnectionRole::Participant)
    }
}

/// Role of a WebSocket connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionRole {
    /// Match participant (can chat, perform veto actions).
    Participant,
    /// Spectator (read-only access to public events).
    Spectator,
    /// Admin (full access).
    Admin,
}

impl ConnectionRole {
    /// Get the string representation for serialization.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Participant => "participant",
            Self::Spectator => "spectator",
            Self::Admin => "admin",
        }
    }
}

impl std::fmt::Display for ConnectionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
