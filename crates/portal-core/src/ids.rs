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
    pub type_name: &'static str,
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

// Team IDs
define_id!(
    /// Unique identifier for a team.
    TeamId
);

define_id!(
    /// Unique identifier for a team invitation.
    TeamInvitationId
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
    /// Unique identifier for a team within a league season.
    LeagueTeamId
);

define_id!(
    /// Unique identifier for a league team member.
    LeagueTeamMemberId
);

define_id!(
    /// Unique identifier for a league team invitation.
    LeagueTeamInvitationId
);

// Legacy: Keep for backwards compatibility during migration
define_id!(
    /// Unique identifier for a season within a league.
    #[deprecated(note = "Use LeagueSeasonId instead")]
    SeasonId
);

define_id!(
    /// Unique identifier for a tournament bracket.
    BracketId
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
        let id = TeamId::new();
        let id_str = id.to_string();
        let parsed: TeamId = id_str.parse().unwrap();
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
