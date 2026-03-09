//! Central registry of all permission constants.
//!
//! This module provides compile-time constants for all permission names used
//! in the RBAC system. Using constants instead of string literals provides:
//! - Compile-time typo detection
//! - IDE autocomplete support
//! - Central documentation of all permissions
//! - Easy refactoring when permission names change
//!
//! # Permission Naming Convention
//!
//! Permissions follow the pattern: `{scope_type}.{resource}.{action}`
//!
//! Examples:
//! - `team.roster.manage` - Manage team roster (invite, kick)
//! - `league.settings.manage` - Manage league settings
//! - `admin.users.manage` - Platform-wide user management
//!
//! # Usage
//!
//! ```rust,ignore
//! use portal_core::permissions::team;
//!
//! // In a handler with RequireTeamPermission extractor
//! RequireTeamPermission::<{team::SETTINGS_MANAGE}>
//!
//! // In a permission check
//! permission_repo.user_has_scoped_permission(
//!     user_id,
//!     team::ROSTER_MANAGE,
//!     Some(&scope)
//! ).await?;
//! ```

/// Team-scoped permissions.
///
/// These permissions are granted to users within the context of a specific team.
/// A user might have different team roles in different teams.
pub mod team {
    /// Manage team roster - invite and remove team members.
    pub const ROSTER_MANAGE: &str = "team.roster.manage";

    /// Manage team settings - edit team name, logo, description.
    pub const SETTINGS_MANAGE: &str = "team.settings.manage";

    /// Manage team roles - promote and demote team members.
    pub const ROLES_MANAGE: &str = "team.roles.manage";

    /// Participate in matches as a team member.
    pub const MATCHES_PLAY: &str = "team.matches.play";

    /// Delete/disband the team.
    pub const DELETE: &str = "team.delete";

    /// View team internal information (for restricted teams).
    pub const VIEW_INTERNAL: &str = "team.view.internal";

    /// All team permissions for iteration.
    pub const ALL: &[&str] = &[
        ROSTER_MANAGE,
        SETTINGS_MANAGE,
        ROLES_MANAGE,
        MATCHES_PLAY,
        DELETE,
        VIEW_INTERNAL,
    ];
}

/// League-scoped permissions.
///
/// These permissions are granted to users within the context of a specific league.
pub mod league {
    /// Create a new league (platform-level permission, not scoped).
    pub const CREATE: &str = "league.create";

    /// Manage league settings - edit name, description, rules.
    pub const SETTINGS_MANAGE: &str = "league.settings.manage";

    /// Manage league members - add, remove, change membership status.
    pub const MEMBERS_MANAGE: &str = "league.members.manage";

    /// Create tournaments within the league.
    pub const TOURNAMENTS_CREATE: &str = "league.tournaments.create";

    /// Manage league seasons.
    pub const SEASONS_MANAGE: &str = "league.seasons.manage";

    /// View league internal information.
    pub const VIEW_INTERNAL: &str = "league.view.internal";

    /// All league permissions for iteration.
    pub const ALL: &[&str] = &[
        CREATE,
        SETTINGS_MANAGE,
        MEMBERS_MANAGE,
        TOURNAMENTS_CREATE,
        SEASONS_MANAGE,
        VIEW_INTERNAL,
    ];
}

/// Tournament-scoped permissions.
///
/// These permissions are granted to users within the context of a specific tournament.
pub mod tournament {
    /// Create a new tournament (may be scoped to league or platform-level).
    pub const CREATE: &str = "tournament.create";

    /// Edit tournament brackets.
    pub const BRACKETS_EDIT: &str = "tournament.brackets.edit";

    /// Manage tournament participants - add, remove, seed.
    pub const PARTICIPANTS_MANAGE: &str = "tournament.participants.manage";

    /// Manage tournament settings.
    pub const SETTINGS_MANAGE: &str = "tournament.settings.manage";

    /// Report or override match results.
    pub const RESULTS_MANAGE: &str = "tournament.results.manage";

    /// All tournament permissions for iteration.
    pub const ALL: &[&str] = &[
        CREATE,
        BRACKETS_EDIT,
        PARTICIPANTS_MANAGE,
        SETTINGS_MANAGE,
        RESULTS_MANAGE,
    ];
}

/// Match-scoped permissions.
///
/// These permissions are granted to users within the context of a specific match.
pub mod match_ {
    /// Full match admin control (referee).
    pub const ADMIN: &str = "match.admin";

    /// Report match results.
    pub const RESULTS_REPORT: &str = "match.results.report";

    /// Manage match participants (substitute players, etc.).
    pub const PARTICIPANTS_MANAGE: &str = "match.participants.manage";

    /// All match permissions for iteration.
    pub const ALL: &[&str] = &[ADMIN, RESULTS_REPORT, PARTICIPANTS_MANAGE];
}

/// API key scopes for service-to-service authentication.
///
/// These scopes are assigned to API keys (not users) and checked via
/// `AuthenticatedService::require_permission()` on internal endpoints.
pub mod service {
    /// Read active steam tracking entries.
    pub const STEAM_TRACKING_READ: &str = "steam_tracking.read";

    /// Update steam tracking poll results.
    pub const STEAM_TRACKING_WRITE: &str = "steam_tracking.write";

    /// Read pending discovered matches.
    pub const DISCOVERED_MATCHES_READ: &str = "discovered_matches.read";

    /// Submit, claim, and update discovered matches.
    pub const DISCOVERED_MATCHES_WRITE: &str = "discovered_matches.write";

    /// Catalog (batch-create) demo records.
    pub const DEMOS_CATALOG: &str = "demos.catalog";

    /// Read demo records (e.g. pending demos for processing).
    pub const DEMOS_READ: &str = "demos.read";

    /// Submit or update demo stats (parse results, mark failures).
    pub const DEMOS_STATS: &str = "demos.stats";

    /// All service scopes for iteration/validation.
    pub const ALL: &[&str] = &[
        STEAM_TRACKING_READ,
        STEAM_TRACKING_WRITE,
        DISCOVERED_MATCHES_READ,
        DISCOVERED_MATCHES_WRITE,
        DEMOS_CATALOG,
        DEMOS_READ,
        DEMOS_STATS,
    ];
}

/// Platform-wide admin permissions.
///
/// These permissions are NOT scoped - they apply globally across the platform.
/// Users with these permissions have elevated access everywhere.
pub mod admin {
    /// View all users on the platform.
    pub const USERS_VIEW: &str = "admin.users.view";

    /// Manage users - edit, disable, delete.
    pub const USERS_MANAGE: &str = "admin.users.manage";

    /// Manage bans - create, revoke platform bans.
    pub const BANS_MANAGE: &str = "admin.bans.manage";

    /// Manage any team on the platform (override team-level permissions).
    pub const TEAMS_MANAGE_ANY: &str = "admin.teams.manage_any";

    /// Manage any league on the platform.
    pub const LEAGUES_MANAGE_ANY: &str = "admin.leagues.manage_any";

    /// Manage any tournament on the platform.
    pub const TOURNAMENTS_MANAGE_ANY: &str = "admin.tournaments.manage_any";

    /// View audit logs.
    pub const AUDIT_VIEW: &str = "admin.audit.view";

    /// Manage system settings.
    pub const SYSTEM_MANAGE: &str = "admin.system.manage";

    /// All admin permissions for iteration.
    pub const ALL: &[&str] = &[
        USERS_VIEW,
        USERS_MANAGE,
        BANS_MANAGE,
        TEAMS_MANAGE_ANY,
        LEAGUES_MANAGE_ANY,
        TOURNAMENTS_MANAGE_ANY,
        AUDIT_VIEW,
        SYSTEM_MANAGE,
    ];
}

/// Get all permission constants organized by category.
///
/// Useful for seeding the database or generating documentation.
#[must_use]
pub fn all_permissions() -> Vec<(&'static str, &'static str, &'static [&'static str])> {
    vec![
        ("team", "Team Permissions", team::ALL),
        ("league", "League Permissions", league::ALL),
        ("tournament", "Tournament Permissions", tournament::ALL),
        ("match", "Match Permissions", match_::ALL),
        ("admin", "Admin Permissions", admin::ALL),
        ("service", "Service API Key Scopes", service::ALL),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_format() {
        // All permissions should follow the pattern: scope.resource.action
        for (category, _, perms) in all_permissions() {
            for perm in perms {
                let parts: Vec<&str> = perm.split('.').collect();
                assert!(
                    parts.len() >= 2,
                    "Permission '{}' should have at least 2 parts separated by dots",
                    perm
                );
                // First part should match the category (except admin and service which use different conventions)
                if category != "admin" && category != "service" {
                    assert!(
                        perm.starts_with(category),
                        "Permission '{}' should start with category '{}'",
                        perm,
                        category
                    );
                }
            }
        }
    }

    #[test]
    fn test_team_permissions() {
        assert_eq!(team::ROSTER_MANAGE, "team.roster.manage");
        assert_eq!(team::SETTINGS_MANAGE, "team.settings.manage");
        assert_eq!(team::ROLES_MANAGE, "team.roles.manage");
        assert_eq!(team::MATCHES_PLAY, "team.matches.play");
        assert_eq!(team::DELETE, "team.delete");
    }

    #[test]
    fn test_league_permissions() {
        assert_eq!(league::CREATE, "league.create");
        assert_eq!(league::SETTINGS_MANAGE, "league.settings.manage");
        assert_eq!(league::MEMBERS_MANAGE, "league.members.manage");
    }

    #[test]
    fn test_admin_permissions() {
        assert_eq!(admin::USERS_VIEW, "admin.users.view");
        assert_eq!(admin::TEAMS_MANAGE_ANY, "admin.teams.manage_any");
    }

    #[test]
    fn test_all_permissions_count() {
        let all = all_permissions();
        let total: usize = all.iter().map(|(_, _, perms)| perms.len()).sum();
        // Ensure we have a reasonable number of permissions defined
        assert!(total >= 15, "Expected at least 15 permissions, got {}", total);
    }
}
