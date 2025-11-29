-- Migration: Add leagues management permission
-- Description: Adds admin.leagues.manage permission for league configuration

-- Add leagues management permission
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.leagues.manage', 'Manage Leagues', 'Create, update, and manage leagues', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Grant to super_admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'super_admin' AND p.name = 'admin.leagues.manage'
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Grant to platform_admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'platform_admin' AND p.name = 'admin.leagues.manage'
ON CONFLICT (role_id, permission_id) DO NOTHING;
