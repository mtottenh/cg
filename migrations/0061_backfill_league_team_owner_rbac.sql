-- Migration: Backfill team_captain RBAC assignment for league_team owners
-- Description: Grants every active league_teams owner a team-scoped
--              team_captain role in user_roles. Enables handlers to move
--              off the ad-hoc `team.is_owner(...)` check and onto
--              `require_team_permission(..., team.settings.manage)`.
--
-- Before this migration, creating a league team inserted a row into
-- `league_team_members` (with role='captain', a free-text field) but
-- never into `user_roles`, so the RBAC permission machinery couldn't see
-- the owner as anything other than an anonymous user. That's why
-- `handlers/league_teams/team.rs` still has to query the team row and
-- compare `owner_player_id == auth.player_id` instead of just calling
-- `perm.require_team_permission(...)`.
--
-- Idempotent: `NOT EXISTS` guards against re-running on already-migrated
-- rows, same pattern as 0017.

INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_at)
SELECT DISTINCT
    p.user_id,
    r.id AS role_id,
    'team' AS scope_type,
    lt.id AS scope_id,
    lt.created_at AS granted_at
FROM league_teams lt
JOIN players p ON p.id = lt.owner_player_id
JOIN roles r ON r.name = 'team_captain'
WHERE lt.status = 'active'
  AND p.user_id IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM user_roles ur
      WHERE ur.user_id = p.user_id
        AND ur.role_id = r.id
        AND ur.scope_type = 'team'
        AND ur.scope_id = lt.id
        AND ur.revoked_at IS NULL
  );

DO $$
DECLARE
    backfilled INTEGER;
BEGIN
    SELECT COUNT(*) INTO backfilled
    FROM user_roles
    WHERE scope_type = 'team'
      AND role_id = (SELECT id FROM roles WHERE name = 'team_captain')
      AND granted_at >= (NOW() - INTERVAL '1 minute');

    RAISE NOTICE 'Backfilled % league_team owner(s) with team_captain scoped role', backfilled;
END $$;
