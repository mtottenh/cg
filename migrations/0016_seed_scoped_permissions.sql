-- Migration: Seed scoped permissions and roles
-- Description: Add permissions and roles for scoped RBAC (teams, leagues, tournaments, matches)
--
-- This migration adds:
-- 1. Updated category constraint to include 'tournament' and 'match'
-- 2. Scoped permissions (team.roster.manage, league.settings.manage, etc.)
-- 3. Scoped roles (team_captain, team_officer, league_admin, etc.)
-- 4. Role-permission mappings for scoped access control

-- ============================================================================
-- UPDATE CATEGORY CONSTRAINT
-- ============================================================================

-- Drop the old constraint and add new one with additional categories
ALTER TABLE roles DROP CONSTRAINT IF EXISTS roles_check_category;
ALTER TABLE roles ADD CONSTRAINT roles_check_category
    CHECK (category IN ('system', 'platform', 'league', 'team', 'tournament', 'match', 'custom'));

-- ============================================================================
-- SCOPED PERMISSIONS
-- ============================================================================

-- Team permissions (scoped to individual teams)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('team.roster.manage', 'Manage Team Roster', 'Invite and remove team members', 'team_scoped', FALSE),
('team.settings.manage', 'Manage Team Settings', 'Edit team name, logo, description', 'team_scoped', FALSE),
('team.roles.manage', 'Manage Team Roles', 'Promote and demote team members', 'team_scoped', FALSE),
('team.matches.play', 'Play Team Matches', 'Participate in matches as team member', 'team_scoped', FALSE),
('team.delete', 'Delete Team', 'Disband the team', 'team_scoped', TRUE),
('team.view.internal', 'View Team Internal', 'View team internal information for restricted teams', 'team_scoped', FALSE)
ON CONFLICT (name) DO NOTHING;

-- League permissions (scoped to individual leagues)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('league.settings.manage', 'Manage League Settings', 'Edit league name, description, rules', 'league_scoped', FALSE),
('league.members.manage', 'Manage League Members', 'Add, remove, and manage league membership', 'league_scoped', FALSE),
('league.tournaments.create', 'Create League Tournaments', 'Create tournaments within the league', 'league_scoped', FALSE),
('league.seasons.manage', 'Manage League Seasons', 'Create and manage league seasons', 'league_scoped', FALSE),
('league.view.internal', 'View League Internal', 'View league internal information', 'league_scoped', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Tournament permissions (scoped to individual tournaments)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('tournament.brackets.edit', 'Edit Tournament Brackets', 'Modify tournament brackets', 'tournament_scoped', FALSE),
('tournament.participants.manage', 'Manage Tournament Participants', 'Add, remove, and seed participants', 'tournament_scoped', FALSE),
('tournament.settings.manage', 'Manage Tournament Settings', 'Edit tournament settings', 'tournament_scoped', FALSE),
('tournament.results.manage', 'Manage Tournament Results', 'Report and override match results', 'tournament_scoped', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Match permissions (scoped to individual matches)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('match.admin', 'Match Admin', 'Full match control as referee', 'match_scoped', FALSE),
('match.results.report', 'Report Match Results', 'Submit match results', 'match_scoped', FALSE),
('match.participants.manage', 'Manage Match Participants', 'Substitute players during match', 'match_scoped', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Admin override permissions (global, not scoped)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.teams.manage_any', 'Manage Any Team', 'Override team-level permissions for any team', 'admin', TRUE),
('admin.leagues.manage_any', 'Manage Any League', 'Override league-level permissions for any league', 'admin', TRUE),
('admin.tournaments.manage_any', 'Manage Any Tournament', 'Override tournament-level permissions for any tournament', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;


-- ============================================================================
-- SCOPED ROLES
-- ============================================================================

-- Team roles (assigned with scope_type='team')
INSERT INTO roles (name, display_name, description, category, priority, color, is_system, is_default) VALUES
('team_captain', 'Team Captain', 'Full team administration rights including disbanding', 'team', 100, '#FFD700', TRUE, FALSE),
('team_officer', 'Team Officer', 'Can manage roster and play matches', 'team', 80, '#C0C0C0', TRUE, FALSE),
('team_player', 'Team Player', 'Active roster member who can play matches', 'team', 50, '#CD7F32', TRUE, FALSE),
('team_substitute', 'Team Substitute', 'Backup player available for matches', 'team', 40, '#8B4513', TRUE, FALSE),
('team_coach', 'Team Coach', 'Non-playing advisor with view access', 'team', 30, '#4682B4', TRUE, FALSE),
('team_manager', 'Team Manager', 'Non-playing administrator who can manage roster', 'team', 35, '#708090', TRUE, FALSE)
ON CONFLICT (name) DO NOTHING;

-- League roles (assigned with scope_type='league')
INSERT INTO roles (name, display_name, description, category, priority, color, is_system, is_default) VALUES
('league_admin', 'League Admin', 'Full league control', 'league', 100, '#8B0000', TRUE, FALSE),
('league_moderator', 'League Moderator', 'Can manage members and tournaments', 'league', 50, '#B22222', TRUE, FALSE),
('league_member', 'League Member', 'Standard league member with participation rights', 'league', 10, '#CD5C5C', TRUE, FALSE)
ON CONFLICT (name) DO NOTHING;

-- Tournament roles (assigned with scope_type='tournament')
INSERT INTO roles (name, display_name, description, category, priority, color, is_system, is_default) VALUES
('tournament_admin', 'Tournament Admin', 'Full tournament control', 'tournament', 100, '#006400', TRUE, FALSE),
('tournament_moderator', 'Tournament Moderator', 'Can manage participants and report results', 'tournament', 50, '#228B22', TRUE, FALSE)
ON CONFLICT (name) DO NOTHING;

-- Match roles (assigned with scope_type='match')
INSERT INTO roles (name, display_name, description, category, priority, color, is_system, is_default) VALUES
('match_admin', 'Match Admin', 'Full match control (referee)', 'match', 100, '#483D8B', TRUE, FALSE)
ON CONFLICT (name) DO NOTHING;


-- ============================================================================
-- ROLE-PERMISSION MAPPINGS (Scoped Roles)
-- ============================================================================

-- Team Captain: all team permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_captain' AND p.name IN (
    'team.roster.manage',
    'team.settings.manage',
    'team.roles.manage',
    'team.matches.play',
    'team.delete',
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Team Officer: roster management + play
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_officer' AND p.name IN (
    'team.roster.manage',
    'team.matches.play',
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Team Player: play matches only
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_player' AND p.name IN (
    'team.matches.play',
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Team Substitute: play matches only
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_substitute' AND p.name IN (
    'team.matches.play',
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Team Coach: view internal only (no play)
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_coach' AND p.name IN (
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Team Manager: roster management but no play
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'team_manager' AND p.name IN (
    'team.roster.manage',
    'team.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- League Admin: all league permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'league_admin' AND p.name IN (
    'league.settings.manage',
    'league.members.manage',
    'league.tournaments.create',
    'league.seasons.manage',
    'league.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- League Moderator: members and tournaments
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'league_moderator' AND p.name IN (
    'league.members.manage',
    'league.tournaments.create',
    'league.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- League Member: view only
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'league_member' AND p.name IN (
    'league.view.internal'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Tournament Admin: all tournament permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'tournament_admin' AND p.name IN (
    'tournament.brackets.edit',
    'tournament.participants.manage',
    'tournament.settings.manage',
    'tournament.results.manage'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Tournament Moderator: participants and results
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'tournament_moderator' AND p.name IN (
    'tournament.participants.manage',
    'tournament.results.manage'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Match Admin: all match permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'match_admin' AND p.name IN (
    'match.admin',
    'match.results.report',
    'match.participants.manage'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;


-- ============================================================================
-- ADMIN OVERRIDE PERMISSIONS
-- ============================================================================

-- Super Admin gets new admin override permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'super_admin' AND p.name IN (
    'admin.teams.manage_any',
    'admin.leagues.manage_any',
    'admin.tournaments.manage_any',
    -- Also all scoped permissions for global access
    'team.roster.manage', 'team.settings.manage', 'team.roles.manage',
    'team.matches.play', 'team.delete', 'team.view.internal',
    'league.settings.manage', 'league.members.manage', 'league.tournaments.create',
    'league.seasons.manage', 'league.view.internal',
    'tournament.brackets.edit', 'tournament.participants.manage',
    'tournament.settings.manage', 'tournament.results.manage',
    'match.admin', 'match.results.report', 'match.participants.manage'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Platform Admin gets admin override permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'platform_admin' AND p.name IN (
    'admin.teams.manage_any',
    'admin.leagues.manage_any',
    'admin.tournaments.manage_any'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;
