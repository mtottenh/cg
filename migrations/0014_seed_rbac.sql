-- Migration: Seed RBAC data
-- Description: Initial roles and permissions for the gaming portal

-- ============================================================================
-- ROLES
-- ============================================================================

-- System Roles (highest priority, cannot be deleted)
INSERT INTO roles (name, display_name, description, category, priority, color, is_system, is_default) VALUES
-- Super Admin - Full system access
('super_admin', 'Super Admin', 'Full system access with all permissions', 'system', 1000, '#FF0000', TRUE, FALSE),
-- Platform Admin - Manages platform operations
('platform_admin', 'Platform Admin', 'Platform administrator with broad management capabilities', 'platform', 900, '#FF6B00', TRUE, FALSE),
-- Moderator - Content and user moderation
('moderator', 'Moderator', 'Content and user moderation capabilities', 'platform', 500, '#9B59B6', TRUE, FALSE),
-- User - Default authenticated user role
('user', 'User', 'Standard authenticated user with basic permissions', 'platform', 100, '#3498DB', TRUE, TRUE)
ON CONFLICT (name) DO NOTHING;


-- ============================================================================
-- PERMISSIONS
-- ============================================================================

-- User Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('users.view', 'View Users', 'View user profiles and public information', 'users', FALSE),
('users.view_all', 'View All Users', 'View all users including private information', 'users', FALSE),
('users.edit_self', 'Edit Own Profile', 'Edit own user profile', 'users', FALSE),
('users.edit_any', 'Edit Any User', 'Edit any user profile', 'users', TRUE),
('users.ban', 'Ban Users', 'Ban or suspend user accounts', 'users', TRUE),
('users.delete', 'Delete Users', 'Permanently delete user accounts', 'users', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Player Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('players.view', 'View Players', 'View player profiles and statistics', 'players', FALSE),
('players.view_all', 'View All Players', 'View all player data including private stats', 'players', FALSE),
('players.edit_self', 'Edit Own Player Profile', 'Edit own player profile and settings', 'players', FALSE),
('players.edit_any', 'Edit Any Player', 'Edit any player profile', 'players', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Team Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('teams.view', 'View Teams', 'View team information and rosters', 'teams', FALSE),
('teams.create', 'Create Teams', 'Create new teams', 'teams', FALSE),
('teams.edit_own', 'Edit Own Team', 'Edit teams where user is captain', 'teams', FALSE),
('teams.edit_any', 'Edit Any Team', 'Edit any team regardless of membership', 'teams', TRUE),
('teams.delete_own', 'Delete Own Team', 'Disband teams where user is founder', 'teams', FALSE),
('teams.delete_any', 'Delete Any Team', 'Disband any team', 'teams', TRUE),
('teams.invite', 'Invite to Team', 'Invite players to teams user manages', 'teams', FALSE),
('teams.kick', 'Kick from Team', 'Remove members from teams user manages', 'teams', FALSE),
('teams.manage_roles', 'Manage Team Roles', 'Change member roles within teams', 'teams', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Match Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('matches.view', 'View Matches', 'View match information and results', 'matches', FALSE),
('matches.create', 'Create Matches', 'Create new matches', 'matches', FALSE),
('matches.edit', 'Edit Matches', 'Edit match details and settings', 'matches', FALSE),
('matches.delete', 'Delete Matches', 'Delete or cancel matches', 'matches', TRUE),
('matches.report', 'Report Match Results', 'Submit and report match results', 'matches', FALSE),
('matches.verify', 'Verify Match Results', 'Verify and approve match results', 'matches', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Tournament Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('tournaments.view', 'View Tournaments', 'View tournament information', 'tournaments', FALSE),
('tournaments.create', 'Create Tournaments', 'Create new tournaments', 'tournaments', FALSE),
('tournaments.edit_own', 'Edit Own Tournaments', 'Edit tournaments user created', 'tournaments', FALSE),
('tournaments.edit_any', 'Edit Any Tournament', 'Edit any tournament', 'tournaments', TRUE),
('tournaments.delete', 'Delete Tournaments', 'Delete or cancel tournaments', 'tournaments', TRUE),
('tournaments.manage_participants', 'Manage Tournament Participants', 'Add/remove tournament participants', 'tournaments', FALSE)
ON CONFLICT (name) DO NOTHING;

-- League Management
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('leagues.view', 'View Leagues', 'View league information', 'leagues', FALSE),
('leagues.create', 'Create Leagues', 'Create new leagues', 'leagues', FALSE),
('leagues.edit', 'Edit Leagues', 'Edit league settings and configuration', 'leagues', TRUE),
('leagues.delete', 'Delete Leagues', 'Delete or close leagues', 'leagues', TRUE),
('leagues.manage_members', 'Manage League Members', 'Add/remove league members', 'leagues', FALSE)
ON CONFLICT (name) DO NOTHING;

-- Administrative
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.view_logs', 'View Audit Logs', 'View system audit logs', 'admin', FALSE),
('admin.manage_roles', 'Manage Roles', 'Create, edit, and delete roles', 'admin', TRUE),
('admin.manage_permissions', 'Manage Permissions', 'Assign and revoke permissions', 'admin', TRUE),
('admin.system_settings', 'System Settings', 'Modify system-wide settings', 'admin', TRUE),
('admin.impersonate', 'Impersonate Users', 'Impersonate other users for debugging', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Moderation
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('mod.view_reports', 'View Reports', 'View user and content reports', 'moderation', FALSE),
('mod.resolve_reports', 'Resolve Reports', 'Resolve and act on reports', 'moderation', FALSE),
('mod.warn_users', 'Warn Users', 'Issue warnings to users', 'moderation', FALSE),
('mod.mute_users', 'Mute Users', 'Temporarily mute users in chat', 'moderation', FALSE)
ON CONFLICT (name) DO NOTHING;


-- ============================================================================
-- ROLE-PERMISSION MAPPINGS
-- ============================================================================

-- Super Admin gets ALL permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'super_admin'
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Platform Admin permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'platform_admin'
  AND p.name IN (
    -- User management (except delete)
    'users.view', 'users.view_all', 'users.edit_self', 'users.edit_any', 'users.ban',
    -- Player management
    'players.view', 'players.view_all', 'players.edit_self', 'players.edit_any',
    -- Team management
    'teams.view', 'teams.create', 'teams.edit_own', 'teams.edit_any',
    'teams.delete_own', 'teams.delete_any', 'teams.invite', 'teams.kick', 'teams.manage_roles',
    -- Match management
    'matches.view', 'matches.create', 'matches.edit', 'matches.delete',
    'matches.report', 'matches.verify',
    -- Tournament management
    'tournaments.view', 'tournaments.create', 'tournaments.edit_own',
    'tournaments.edit_any', 'tournaments.delete', 'tournaments.manage_participants',
    -- League management
    'leagues.view', 'leagues.create', 'leagues.edit', 'leagues.delete', 'leagues.manage_members',
    -- Admin (except dangerous)
    'admin.view_logs', 'admin.manage_roles',
    -- All moderation
    'mod.view_reports', 'mod.resolve_reports', 'mod.warn_users', 'mod.mute_users'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Moderator permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'moderator'
  AND p.name IN (
    -- View permissions
    'users.view', 'users.view_all', 'players.view', 'players.view_all',
    'teams.view', 'matches.view', 'tournaments.view', 'leagues.view',
    -- Own profile management
    'users.edit_self', 'players.edit_self',
    -- Team operations
    'teams.create', 'teams.edit_own', 'teams.delete_own',
    'teams.invite', 'teams.kick', 'teams.manage_roles',
    -- Match reporting
    'matches.report', 'matches.verify',
    -- Moderation
    'mod.view_reports', 'mod.resolve_reports', 'mod.warn_users', 'mod.mute_users',
    -- User discipline
    'users.ban'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- User (default) permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'user'
  AND p.name IN (
    -- Basic view permissions
    'users.view', 'players.view', 'teams.view', 'matches.view',
    'tournaments.view', 'leagues.view',
    -- Self-management
    'users.edit_self', 'players.edit_self',
    -- Team operations (creating and managing own teams)
    'teams.create', 'teams.edit_own', 'teams.delete_own',
    'teams.invite', 'teams.kick', 'teams.manage_roles',
    -- Match participation
    'matches.report'
  )
ON CONFLICT (role_id, permission_id) DO NOTHING;
