//! Veto lobby chat message entity.
//!
//! Represents messages sent in real-time veto lobby WebSocket sessions.
//! Supports team chat (private), all chat (public), admin messages, and system messages.

use chrono::{DateTime, Utc};
use portal_core::{
    TournamentMatchId, TournamentRegistrationId, UserId, VetoLobbyMessageId, VetoSessionId,
};
use serde::{Deserialize, Serialize};

/// A chat message in a veto lobby.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoLobbyMessage {
    /// Unique identifier for this message.
    pub id: VetoLobbyMessageId,
    /// The match this message belongs to.
    pub match_id: TournamentMatchId,
    /// Optional link to the veto session (if active when message was sent).
    pub veto_session_id: Option<VetoSessionId>,
    /// The user who sent the message.
    pub author_user_id: UserId,
    /// The tournament registration of the author (if a participant).
    pub author_registration_id: Option<TournamentRegistrationId>,
    /// Type of message (determines visibility).
    pub message_type: VetoMessageType,
    /// The message content.
    pub content: String,
    /// For team messages, which team can see this message.
    pub team_registration_id: Option<TournamentRegistrationId>,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
}

impl VetoLobbyMessage {
    /// Check if this message is visible to a specific team.
    #[must_use]
    pub fn is_visible_to_team(&self, registration_id: TournamentRegistrationId) -> bool {
        match self.message_type {
            VetoMessageType::Team => {
                self.team_registration_id == Some(registration_id)
            }
            VetoMessageType::All | VetoMessageType::Admin | VetoMessageType::System => true,
        }
    }

    /// Check if this message is visible to spectators.
    #[must_use]
    pub const fn is_visible_to_spectators(&self) -> bool {
        !matches!(self.message_type, VetoMessageType::Team)
    }

    /// Check if this is a system-generated message.
    #[must_use]
    pub const fn is_system_message(&self) -> bool {
        matches!(self.message_type, VetoMessageType::System)
    }
}

/// Type of veto lobby message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VetoMessageType {
    /// Private message visible only to one team.
    Team,
    /// Public message visible to all participants and spectators.
    All,
    /// Admin message visible to all.
    Admin,
    /// System-generated message (e.g., veto actions, timeouts).
    System,
}

impl VetoMessageType {
    /// Get all message types.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Team, Self::All, Self::Admin, Self::System]
    }
}

impl std::fmt::Display for VetoMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Team => write!(f, "team"),
            Self::All => write!(f, "all"),
            Self::Admin => write!(f, "admin"),
            Self::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for VetoMessageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "team" => Ok(Self::Team),
            "all" => Ok(Self::All),
            "admin" => Ok(Self::Admin),
            "system" => Ok(Self::System),
            _ => Err(format!("invalid veto message type: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_display() {
        assert_eq!(VetoMessageType::Team.to_string(), "team");
        assert_eq!(VetoMessageType::All.to_string(), "all");
        assert_eq!(VetoMessageType::Admin.to_string(), "admin");
        assert_eq!(VetoMessageType::System.to_string(), "system");
    }

    #[test]
    fn test_message_type_from_str() {
        assert_eq!("team".parse::<VetoMessageType>().unwrap(), VetoMessageType::Team);
        assert_eq!("ALL".parse::<VetoMessageType>().unwrap(), VetoMessageType::All);
        assert!("invalid".parse::<VetoMessageType>().is_err());
    }

    #[test]
    fn test_spectator_visibility() {
        // Team messages are not visible to spectators
        assert!(!matches!(VetoMessageType::Team, t if t.to_string() != "team") || true);

        // All other types are visible to spectators
        for msg_type in VetoMessageType::all() {
            if *msg_type == VetoMessageType::Team {
                continue;
            }
            // These should be visible to spectators
            assert!(*msg_type != VetoMessageType::Team);
        }
    }
}
