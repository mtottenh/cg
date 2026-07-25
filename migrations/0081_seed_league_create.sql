-- Migration: Seed league.create
-- Description:
--   `league.create` has sat in `portal_core::permissions::league` since scoped
--   RBAC was introduced (0016), and — unlike its sibling `tournament.create`,
--   which 0030 seeds — was never inserted into the `permissions` table. So no
--   role has ever held it.
--
--   Nothing gates on it *yet*, which is the only reason this has not already
--   surfaced as a mystery 403: the first handler to write
--   `require_permission(&auth, permissions::league::CREATE)` would have been
--   refused for every caller including super_admin, with no error anywhere
--   pointing at the missing row. That is exactly the shape of the
--   `admin.system.manage` defect fixed in 0080 — the constant looks live at
--   the call site, and the seed is the invisible half.
--
--   Found by extending `test_every_declared_permission_is_seeded_and_granted`
--   from `admin::ALL` to the scoped registries as well while adding the P-72
--   admin score override (which gates on `tournament.results.manage`). The
--   test previously covered only one of five registries.
--
--   Granted to super_admin and platform_admin, matching how the other
--   platform-level creation rights are held. It is not marked dangerous:
--   creating a league is additive and reversible, in line with
--   `tournament.create`.

INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('league.create', 'Create Leagues', 'Create new leagues', 'league', FALSE)
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name IN ('super_admin', 'platform_admin')
  AND p.name = 'league.create'
ON CONFLICT DO NOTHING;
