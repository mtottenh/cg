-- Migration: Migrate team_members roles to RBAC scoped roles
-- Description: Creates user_roles entries from existing team_members
--
-- This migration:
-- 1. Creates scoped role assignments (user_roles) from team_members table
-- 2. Maps team_members.role to corresponding RBAC roles:
--    - captain -> team_captain
--    - officer -> team_officer
--    - player -> team_player
--    - substitute -> team_substitute
--    - coach -> team_coach
--    - manager -> team_manager
-- 3. Only migrates active members (left_at IS NULL)

-- ============================================================================
-- MIGRATE TEAM_MEMBERS TO RBAC SCOPED ROLES
-- ============================================================================

-- Insert scoped roles for all existing active team members
-- We need to join with players to get the user_id since user_roles references users

INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_at)
SELECT DISTINCT
    p.user_id,
    r.id as role_id,
    'team' as scope_type,
    tm.team_id as scope_id,
    tm.joined_at as granted_at
FROM team_members tm
JOIN players p ON p.id = tm.player_id
JOIN roles r ON r.name = CASE tm.role
    WHEN 'captain' THEN 'team_captain'
    WHEN 'officer' THEN 'team_officer'
    WHEN 'player' THEN 'team_player'
    WHEN 'substitute' THEN 'team_substitute'
    WHEN 'coach' THEN 'team_coach'
    WHEN 'manager' THEN 'team_manager'
END
WHERE tm.left_at IS NULL  -- Only active members
  AND p.user_id IS NOT NULL  -- Only players with linked users
  -- Skip if already exists (handles re-running migration)
  AND NOT EXISTS (
      SELECT 1 FROM user_roles ur
      WHERE ur.user_id = p.user_id
        AND ur.role_id = r.id
        AND ur.scope_type = 'team'
        AND ur.scope_id = tm.team_id
        AND ur.revoked_at IS NULL
  );

-- Log migration results (informational, no-op if no migrations needed)
DO $$
DECLARE
    migrated_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO migrated_count
    FROM user_roles
    WHERE scope_type = 'team'
      AND granted_at >= (NOW() - INTERVAL '1 minute');

    RAISE NOTICE 'Migrated % team member(s) to RBAC scoped roles', migrated_count;
END $$;
