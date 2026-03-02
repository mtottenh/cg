//! Strongly-typed ID newtypes.
//!
//! All entity IDs are wrapped in newtype structs to provide type safety
//! and prevent accidentally mixing up IDs of different entity types.
//!
//! ## UUID-based IDs
//!
//! Most entity IDs wrap a UUID internally:
//! - Implements `Display` for string conversion
//! - Implements `FromStr` for parsing
//! - Implements serde traits for serialization
//! - Provides `new()` for generating new IDs and `from_uuid()` for wrapping existing ones
//!
//! ## String-based Slugs
//!
//! Some entities also have human-readable slugs (like `GameSlug` for "cs2", "aoe4"):
//! - Wraps a String internally
//! - Used for URL-friendly identifiers
//! - Distinct from UUIDs to prevent mixing identifier types

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Error when parsing an invalid ID string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError {
    /// The name of the ID type that failed to parse.
    pub type_name: &'static str,
    /// The input string that failed to parse.
    pub input: String,
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {} ID: {}", self.type_name, self.input)
    }
}

impl std::error::Error for ParseIdError {}

/// Macro to generate a strongly-typed ID newtype.
macro_rules! define_id {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generate a new random ID using UUID v7 (time-ordered).
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wrap an existing UUID.
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Get the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }

            /// Convert to a hyphenated string representation.
            #[must_use]
            pub fn to_string(&self) -> String {
                self.0.to_string()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s)
                    .map(Self)
                    .map_err(|_| ParseIdError {
                        type_name: stringify!($name),
                        input: s.to_string(),
                    })
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Uuid {
                id.0
            }
        }

        impl AsRef<Uuid> for $name {
            fn as_ref(&self) -> &Uuid {
                &self.0
            }
        }
    };
}

/// Macro to generate a strongly-typed string-based slug newtype.
macro_rules! define_slug {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create a new slug from a string.
            #[must_use]
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Get the inner string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume and return the inner String.
            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<$name> for String {
            fn from(slug: $name) -> String {
                slug.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
    };
}

// =============================================================================
// UUID-based IDs
// =============================================================================

// Core identity IDs
define_id!(
    /// Unique identifier for a user account.
    ///
    /// Users are the authentication entity - they log in and own player profiles.
    UserId
);

define_id!(
    /// Unique identifier for a player profile.
    ///
    /// Players are the gaming entity - they join teams, play matches, have ratings.
    /// A user may have one player profile.
    PlayerId
);

// Game IDs
define_id!(
    /// Unique identifier for a game (e.g., CS2, AoE4).
    GameId
);

define_id!(
    /// Unique identifier for a player's game-specific profile.
    PlayerGameProfileId
);

// Match IDs
define_id!(
    /// Unique identifier for a match.
    MatchId
);

define_id!(
    /// Unique identifier for a matchmaking queue.
    QueueId
);

define_id!(
    /// Unique identifier for a queue entry.
    QueueEntryId
);

// Lobby IDs
define_id!(
    /// Unique identifier for a lobby.
    LobbyId
);

// Tournament and League IDs
define_id!(
    /// Unique identifier for a tournament.
    TournamentId
);

define_id!(
    /// Unique identifier for a stage within a tournament.
    TournamentStageId
);

define_id!(
    /// Unique identifier for a bracket within a tournament stage.
    TournamentBracketId
);

define_id!(
    /// Unique identifier for a tournament registration (participant entry).
    TournamentRegistrationId
);

define_id!(
    /// Unique identifier for a match within a tournament bracket.
    TournamentMatchId
);

define_id!(
    /// Unique identifier for an individual game within a tournament match (Bo3, Bo5 series).
    TournamentMatchGameId
);

define_id!(
    /// Unique identifier for a tournament map pool configuration.
    TournamentMapPoolId
);

define_id!(
    /// Unique identifier for a league.
    LeagueId
);

define_id!(
    /// Unique identifier for a league member.
    LeagueMemberId
);

define_id!(
    /// Unique identifier for a league invitation.
    LeagueInvitationId
);

define_id!(
    /// Unique identifier for a season within a league.
    LeagueSeasonId
);

define_id!(
    /// Unique identifier for a team within a league (persistent identity).
    LeagueTeamId
);

define_id!(
    /// Unique identifier for a team's participation in a season.
    LeagueTeamSeasonId
);

define_id!(
    /// Unique identifier for a league team member.
    LeagueTeamMemberId
);

define_id!(
    /// Unique identifier for a league team invitation.
    LeagueTeamInvitationId
);

define_id!(
    /// Unique identifier for a tournament bracket.
    BracketId
);

define_id!(
    /// Unique identifier for a match status log entry.
    ///
    /// Used to track match state machine transitions for audit purposes.
    MatchStatusLogId
);

define_id!(
    /// Unique identifier for a schedule proposal.
    ///
    /// Used in the match scheduling negotiation system.
    ScheduleProposalId
);

define_id!(
    /// Unique identifier for an availability window.
    ///
    /// Represents a recurring time slot when a player or team is available.
    AvailabilityWindowId
);

define_id!(
    /// Unique identifier for an availability exception.
    ///
    /// Represents a specific date override for availability (blocked or custom hours).
    AvailabilityExceptionId
);

define_id!(
    /// Unique identifier for a suggested time slot.
    ///
    /// Represents an auto-generated or manual time suggestion for match scheduling.
    SuggestedTimeId
);

define_id!(
    /// Unique identifier for a veto session.
    ///
    /// Represents a map pick/ban session for a tournament match.
    VetoSessionId
);

define_id!(
    /// Unique identifier for a veto action.
    ///
    /// Represents a single ban, pick, or decider action in a veto session.
    VetoActionId
);

define_id!(
    /// Unique identifier for a result claim.
    ///
    /// Represents a submitted match result awaiting confirmation.
    ResultClaimId
);

define_id!(
    /// Unique identifier for match evidence.
    ///
    /// Represents demo files, screenshots, or other evidence linked to matches.
    EvidenceId
);

// Infrastructure IDs
define_id!(
    /// Unique identifier for a game server.
    GameServerId
);

define_id!(
    /// Unique identifier for a saga execution.
    SagaId
);

define_id!(
    /// Unique identifier for a progression log entry.
    ProgressionLogId
);

define_id!(
    /// Unique identifier for a ban record.
    BanId
);

define_id!(
    /// Unique identifier for a forfeit record.
    ///
    /// Tracks forfeits due to no-show, withdrawal, disqualification, or technical default.
    ForfeitRecordId
);

define_id!(
    /// Unique identifier for a dispute.
    ///
    /// Used for match result disputes that require admin resolution.
    DisputeId
);

define_id!(
    /// Unique identifier for a dispute message.
    ///
    /// Messages in a dispute thread for communication between participants and admins.
    DisputeMessageId
);

// Demo Catalog IDs
define_id!(
    /// Unique identifier for a demo file in the catalog.
    ///
    /// Demos exist independently of matches and can be browsed, categorized, and linked.
    DemoId
);

define_id!(
    /// Unique identifier for a demo-match link.
    ///
    /// Links a demo to a tournament match (many-to-many relationship).
    DemoMatchLinkId
);

define_id!(
    /// Unique identifier for a player's appearance in a demo.
    ///
    /// Tracks player stats extracted from demo files.
    DemoPlayerId
);

define_id!(
    /// Unique identifier for a result review.
    ///
    /// Used for tracking validation issues requiring human review before match completion.
    ResultReviewId
);

define_id!(
    /// Unique identifier for a veto lobby chat message.
    ///
    /// Messages sent in the real-time veto lobby (team chat, all chat, system messages).
    VetoLobbyMessageId
);

define_id!(
    /// Unique identifier for a veto delegate.
    ///
    /// Represents a delegation of veto (pick/ban) authority from a captain/owner to a team member.
    VetoDelegateId
);

// Bot / Service IDs
define_id!(
    /// Unique identifier for an API key used by bots and services.
    ApiKeyId
);

define_id!(
    /// Unique identifier for a steam tracking entry.
    SteamTrackingId
);

define_id!(
    /// Unique identifier for a discovered match from Steam.
    DiscoveredMatchId
);

define_id!(
    /// Unique identifier for a player rating history entry.
    PlayerRatingHistoryId
);

// =============================================================================
// String-based Slugs
// =============================================================================

define_slug!(
    /// Human-readable game identifier (e.g., "cs2", "aoe4").
    ///
    /// Used in URLs and API calls for readability. The actual game entity
    /// is identified by `GameId` (UUID).
    GameSlug
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_parsing() {
        let id = LeagueTeamId::new();
        let id_str = id.to_string();
        let parsed: LeagueTeamId = id_str.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_id_parse_error() {
        let result: Result<PlayerId, _> = "not-a-uuid".parse();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.type_name, "PlayerId");
        assert_eq!(err.input, "not-a-uuid");
    }

    #[test]
    fn test_id_serialization() {
        let id = MatchId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: MatchId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_uuid_conversion() {
        let uuid = Uuid::now_v7();
        let id = GameId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);

        let back: Uuid = id.into();
        assert_eq!(back, uuid);
    }
}
