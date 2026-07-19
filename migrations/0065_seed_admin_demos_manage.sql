-- Migration: Seed admin.demos.manage
-- Description:
--   Demo-catalog administration (catalog, categorize, hide, link/unlink to
--   matches, delete) previously gated on is_admin (= the READ permission
--   users.view_all, held by moderators). Now gated on a real manage
--   permission, granted to super_admin and platform_admin only — matching
--   the admin.users.manage / admin.bans.manage model from 0062.

INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.demos.manage', 'Manage Demos',
 'Catalog, categorize, hide, link/unlink, and delete demos', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name IN ('super_admin', 'platform_admin')
  AND p.name = 'admin.demos.manage'
ON CONFLICT DO NOTHING;
