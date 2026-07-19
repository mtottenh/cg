//! Permission scope types for RBAC with scoped permissions.
//!
//! This module provides types for scoping permissions to specific entities:
//! - Teams (team captain, officer, player roles)
//! - Leagues (league admin, moderator roles)
//! - Tournaments (tournament admin roles)
//! - Matches (match admin/referee roles)

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::{LeagueId, LeagueTeamId, MatchId, TournamentId};

/// Types of scopes for permission checking.
///
/// Scoped permissions allow users to have different roles/permissions
/// in different contexts. For example, a user might be a captain of one team
/// but just a player on another team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeType {
    /// Team-scoped permissions (captain, officer, player, etc.)
    Team,
    /// League-scoped permissions (admin, moderator, member)
    League,
    /// Tournament-scoped permissions (admin, moderator)
    Tournament,
    /// Match-scoped permissions (referee, admin)
    Match,
}

impl ScopeType {
    /// Get the string representation of the scope type.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::League => "league",
            Self::Tournament => "tournament",
            Self::Match => "match",
        }
    }

    /// Get all available scope types.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Team, Self::League, Self::Tournament, Self::Match]
    }
}

impl fmt::Display for ScopeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error when parsing an invalid scope type string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseScopeTypeError {
    /// The input string that failed to parse.
    pub input: String,
}

impl fmt::Display for ParseScopeTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid scope type '{}': expected one of team, league, tournament, match",
            self.input
        )
    }
}

impl std::error::Error for ParseScopeTypeError {}

impl FromStr for ScopeType {
    type Err = ParseScopeTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "team" => Ok(Self::Team),
            "league" => Ok(Self::League),
            "tournament" => Ok(Self::Tournament),
            "match" => Ok(Self::Match),
            _ => Err(ParseScopeTypeError {
                input: s.to_string(),
            }),
        }
    }
}

/// A permission scope combining the type and entity ID.
///
/// Used for checking if a user has a specific permission within
/// a particular context (e.g., "team.settings.manage" for team X).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PermissionScope {
    /// The type of scope (team, league, tournament, match).
    pub scope_type: ScopeType,
    /// The ID of the entity within that scope.
    pub scope_id: Uuid,
}

impl PermissionScope {
    /// Create a new permission scope.
    #[must_use]
    pub const fn new(scope_type: ScopeType, scope_id: Uuid) -> Self {
        Self {
            scope_type,
            scope_id,
        }
    }

    /// Create a team-scoped permission.
    #[must_use]
    pub fn team(id: impl Into<Uuid>) -> Self {
        Self {
            scope_type: ScopeType::Team,
            scope_id: id.into(),
        }
    }

    /// Create a team-scoped permission from a `LeagueTeamId`.
    #[must_use]
    pub const fn from_league_team_id(id: LeagueTeamId) -> Self {
        Self {
            scope_type: ScopeType::Team,
            scope_id: id.as_uuid(),
        }
    }

    /// Create a league-scoped permission.
    #[must_use]
    pub fn league(id: impl Into<Uuid>) -> Self {
        Self {
            scope_type: ScopeType::League,
            scope_id: id.into(),
        }
    }

    /// Create a league-scoped permission from a `LeagueId`.
    #[must_use]
    pub const fn from_league_id(id: LeagueId) -> Self {
        Self {
            scope_type: ScopeType::League,
            scope_id: id.as_uuid(),
        }
    }

    /// Create a tournament-scoped permission.
    #[must_use]
    pub fn tournament(id: impl Into<Uuid>) -> Self {
        Self {
            scope_type: ScopeType::Tournament,
            scope_id: id.into(),
        }
    }

    /// Create a tournament-scoped permission from a `TournamentId`.
    #[must_use]
    pub const fn from_tournament_id(id: TournamentId) -> Self {
        Self {
            scope_type: ScopeType::Tournament,
            scope_id: id.as_uuid(),
        }
    }

    /// Create a match-scoped permission.
    ///
    /// Named `match_` to avoid conflict with the `match` keyword.
    #[must_use]
    pub fn match_(id: impl Into<Uuid>) -> Self {
        Self {
            scope_type: ScopeType::Match,
            scope_id: id.into(),
        }
    }

    /// Create a match-scoped permission from a `MatchId`.
    #[must_use]
    pub const fn from_match_id(id: MatchId) -> Self {
        Self {
            scope_type: ScopeType::Match,
            scope_id: id.as_uuid(),
        }
    }
}

impl fmt::Display for PermissionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.scope_type, self.scope_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_type_as_str() {
        assert_eq!(ScopeType::Team.as_str(), "team");
        assert_eq!(ScopeType::League.as_str(), "league");
        assert_eq!(ScopeType::Tournament.as_str(), "tournament");
        assert_eq!(ScopeType::Match.as_str(), "match");
    }

    #[test]
    fn test_scope_type_parse() {
        assert_eq!("team".parse::<ScopeType>().unwrap(), ScopeType::Team);
        assert_eq!("TEAM".parse::<ScopeType>().unwrap(), ScopeType::Team);
        assert_eq!("league".parse::<ScopeType>().unwrap(), ScopeType::League);
        assert_eq!(
            "tournament".parse::<ScopeType>().unwrap(),
            ScopeType::Tournament
        );
        assert_eq!("match".parse::<ScopeType>().unwrap(), ScopeType::Match);
        assert!("invalid".parse::<ScopeType>().is_err());
    }

    #[test]
    fn test_scope_type_display() {
        assert_eq!(format!("{}", ScopeType::Team), "team");
        assert_eq!(format!("{}", ScopeType::League), "league");
    }

    #[test]
    fn test_scope_type_serialization() {
        let scope = ScopeType::Tournament;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"tournament\"");

        let deserialized: ScopeType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, scope);
    }

    #[test]
    fn test_permission_scope_constructors() {
        let uuid = Uuid::now_v7();

        let team_scope = PermissionScope::team(uuid);
        assert_eq!(team_scope.scope_type, ScopeType::Team);
        assert_eq!(team_scope.scope_id, uuid);

        let league_scope = PermissionScope::league(uuid);
        assert_eq!(league_scope.scope_type, ScopeType::League);

        let tournament_scope = PermissionScope::tournament(uuid);
        assert_eq!(tournament_scope.scope_type, ScopeType::Tournament);

        let match_scope = PermissionScope::match_(uuid);
        assert_eq!(match_scope.scope_type, ScopeType::Match);
    }

    #[test]
    fn test_permission_scope_from_typed_ids() {
        let team_id = LeagueTeamId::new();
        let scope = PermissionScope::from_league_team_id(team_id);
        assert_eq!(scope.scope_type, ScopeType::Team);
        assert_eq!(scope.scope_id, team_id.as_uuid());
    }

    #[test]
    fn test_permission_scope_display() {
        let uuid = Uuid::nil();
        let scope = PermissionScope::team(uuid);
        assert_eq!(
            format!("{}", scope),
            "team:00000000-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn test_scope_type_all() {
        let all = ScopeType::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&ScopeType::Team));
        assert!(all.contains(&ScopeType::League));
        assert!(all.contains(&ScopeType::Tournament));
        assert!(all.contains(&ScopeType::Match));
    }
}
