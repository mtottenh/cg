-- Migration: Seed admin manage permissions
-- Description: Seeds the registry constants admin.users.view, admin.users.manage and
-- admin.bans.manage (portal-core/src/permissions.rs) which were defined in code but
-- never seeded. Role/ban mutations previously gated on the READ permission
-- users.view_all (held by moderators too), which allowed privilege escalation:
-- a moderator could assign themselves super_admin via POST /v1/admin/users/{id}/roles.
-- These manage permissions are granted to super_admin and platform_admin ONLY.

-- Seed the missing admin permissions (exact registry names)
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.users.view', 'View All Users (Admin)', 'View all users on the platform', 'admin', FALSE),
('admin.users.manage', 'Manage Users', 'Manage users - edit, disable, delete, roles', 'admin', TRUE),
('admin.bans.manage', 'Manage Bans', 'Manage bans - create, revoke platform bans', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Grant to super_admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'super_admin'
  AND p.name IN ('admin.users.view', 'admin.users.manage', 'admin.bans.manage')
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Grant to platform_admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'platform_admin'
  AND p.name IN ('admin.users.view', 'admin.users.manage', 'admin.bans.manage')
ON CONFLICT (role_id, permission_id) DO NOTHING;
