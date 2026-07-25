-- Migration: Seed admin.system.manage
-- Description:
--   `admin.system.manage` has existed in the permission registry
--   (portal-core/src/permissions.rs) since the admin permissions were
--   introduced, and is the gate on `POST /v1/players/{id}/games/{game}/rating`
--   (handlers/player_game_profiles.rs submit_player_rating) — the only path
--   by which an operator can correct a wrong scraped Premier rating.
--
--   It was never inserted into the `permissions` table, so no role held it and
--   the endpoint was 403 for EVERY real caller, super_admin included. The
--   integration tests did not catch it because they call the endpoint as the
--   well-known dev account, which `PermissionChecker` bypasses outright in
--   `test-utils` builds; and no UI ever called it (P-68), so no human hit the
--   403 either. A gate nobody can pass is indistinguishable from a broken
--   endpoint until someone tries to use it.
--
--   Granted to super_admin and platform_admin only, matching the
--   admin.users.manage / admin.bans.manage model from 0062 and
--   admin.demos.manage from 0065. Rating drives seeding and league entry
--   gates (min_rating_per_player), so this is a dangerous permission.

INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.system.manage', 'Manage System Settings',
 'Manage platform system settings and override pipeline-derived player data such as ratings',
 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name IN ('super_admin', 'platform_admin')
  AND p.name = 'admin.system.manage'
ON CONFLICT DO NOTHING;
