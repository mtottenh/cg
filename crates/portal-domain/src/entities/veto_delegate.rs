//! Veto delegate domain entities.
//!
//! Handles delegation of veto (pick/ban) authority from team captains/owners
//! to other team members.

use chrono::{DateTime, Utc};
use portal_core::{
    LeagueTeamSeasonId, PlayerId, TournamentId, UserId, VetoDelegateId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// VETO DELEGATE
// =============================================================================

/// Delegation of veto authority to a team member.
///
/// When a captain, team owner, or tournament admin delegates veto authority,
/// that player can perform picks/bans on behalf of the team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VetoDelegate {
    pub id: VetoDelegateId,
    /// The team-season this delegation applies to.
    pub team_season_id: LeagueTeamSeasonId,
    /// The player being delegated authority.
    pub player_id: PlayerId,
    /// User who created this delegation.
    pub delegated_by_user_id: UserId,
    /// Role that authorized the delegation.
    pub delegated_by_role: DelegatedByRole,
    /// Optional scope to specific tournament (None = all tournaments).
    pub tournament_id: Option<TournamentId>,
    /// When the delegation was revoked (None = still active).
    pub revoked_at: Option<DateTime<Utc>>,
    /// Who revoked the delegation.
    pub revoked_by_user_id: Option<UserId>,
    /// When the delegation was created.
    pub created_at: DateTime<Utc>,
}

impl VetoDelegate {
    /// Check if this delegation is currently active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Check if this delegation applies to a specific tournament.
    #[must_use]
    pub fn applies_to_tournament(&self, tournament_id: TournamentId) -> bool {
        match self.tournament_id {
            None => true, // Applies to all tournaments
            Some(scoped_id) => scoped_id == tournament_id,
        }
    }
}

// =============================================================================
// DELEGATED BY ROLE
// =============================================================================

/// The role that authorized the delegation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedByRole {
    /// Team captain delegated authority.
    Captain,
    /// Team owner delegated authority.
    Owner,
    /// Tournament admin delegated authority.
    TournamentAdmin,
}

impl DelegatedByRole {
    /// Check if this role can revoke delegations made by another role.
    ///
    /// Rules:
    /// - Tournament admins can revoke any delegation
    /// - Owners can revoke captain and owner delegations
    /// - Captains can only revoke captain delegations
    #[must_use]
    pub const fn can_revoke(&self, other: DelegatedByRole) -> bool {
        match self {
            Self::TournamentAdmin => true,
            Self::Owner => matches!(other, Self::Captain | Self::Owner),
            Self::Captain => matches!(other, Self::Captain),
        }
    }
}

impl std::fmt::Display for DelegatedByRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Captain => write!(f, "captain"),
            Self::Owner => write!(f, "owner"),
            Self::TournamentAdmin => write!(f, "tournament_admin"),
        }
    }
}

impl std::str::FromStr for DelegatedByRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "captain" => Ok(Self::Captain),
            "owner" => Ok(Self::Owner),
            "tournament_admin" => Ok(Self::TournamentAdmin),
            _ => Err(format!("invalid delegated by role: {s}")),
        }
    }
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// Command to create a veto delegation.
#[derive(Debug, Clone)]
pub struct CreateVetoDelegateCommand {
    /// The team-season to delegate for.
    pub team_season_id: LeagueTeamSeasonId,
    /// The player to delegate to.
    pub delegate_player_id: PlayerId,
    /// User creating the delegation.
    pub delegating_user_id: UserId,
    /// Player ID of the user creating the delegation.
    pub delegating_player_id: PlayerId,
    /// Optional scope to specific tournament.
    pub tournament_id: Option<TournamentId>,
}

/// Command to revoke a veto delegation.
#[derive(Debug, Clone)]
pub struct RevokeVetoDelegateCommand {
    /// The delegation to revoke.
    pub delegate_id: VetoDelegateId,
    /// User revoking the delegation.
    pub revoking_user_id: UserId,
    /// Player ID of the user revoking.
    pub revoking_player_id: PlayerId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegated_by_role_display() {
        assert_eq!(DelegatedByRole::Captain.to_string(), "captain");
        assert_eq!(DelegatedByRole::Owner.to_string(), "owner");
        assert_eq!(DelegatedByRole::TournamentAdmin.to_string(), "tournament_admin");
    }

    #[test]
    fn test_delegated_by_role_from_str() {
        assert_eq!("captain".parse::<DelegatedByRole>().unwrap(), DelegatedByRole::Captain);
        assert_eq!("owner".parse::<DelegatedByRole>().unwrap(), DelegatedByRole::Owner);
        assert_eq!("tournament_admin".parse::<DelegatedByRole>().unwrap(), DelegatedByRole::TournamentAdmin);
        assert!("invalid".parse::<DelegatedByRole>().is_err());
    }

    #[test]
    fn test_can_revoke() {
        // Admin can revoke anything
        assert!(DelegatedByRole::TournamentAdmin.can_revoke(DelegatedByRole::Captain));
        assert!(DelegatedByRole::TournamentAdmin.can_revoke(DelegatedByRole::Owner));
        assert!(DelegatedByRole::TournamentAdmin.can_revoke(DelegatedByRole::TournamentAdmin));

        // Owner can revoke captain and owner
        assert!(DelegatedByRole::Owner.can_revoke(DelegatedByRole::Captain));
        assert!(DelegatedByRole::Owner.can_revoke(DelegatedByRole::Owner));
        assert!(!DelegatedByRole::Owner.can_revoke(DelegatedByRole::TournamentAdmin));

        // Captain can only revoke captain
        assert!(DelegatedByRole::Captain.can_revoke(DelegatedByRole::Captain));
        assert!(!DelegatedByRole::Captain.can_revoke(DelegatedByRole::Owner));
        assert!(!DelegatedByRole::Captain.can_revoke(DelegatedByRole::TournamentAdmin));
    }

    #[test]
    fn test_applies_to_tournament() {
        let tournament_id = TournamentId::new();
        let other_tournament_id = TournamentId::new();

        // Unscoped delegation applies to all
        let unscoped = VetoDelegate {
            id: VetoDelegateId::new(),
            team_season_id: LeagueTeamSeasonId::new(),
            player_id: PlayerId::new(),
            delegated_by_user_id: UserId::new(),
            delegated_by_role: DelegatedByRole::Captain,
            tournament_id: None,
            revoked_at: None,
            revoked_by_user_id: None,
            created_at: Utc::now(),
        };
        assert!(unscoped.applies_to_tournament(tournament_id));
        assert!(unscoped.applies_to_tournament(other_tournament_id));

        // Scoped delegation only applies to specific tournament
        let scoped = VetoDelegate {
            tournament_id: Some(tournament_id),
            ..unscoped
        };
        assert!(scoped.applies_to_tournament(tournament_id));
        assert!(!scoped.applies_to_tournament(other_tournament_id));
    }
}
