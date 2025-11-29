-- Migration: Add games management permission
-- Description: Adds admin.games.manage permission for game configuration

-- Add games management permission
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.games.manage', 'Manage Games', 'Configure game settings, maps, and status', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

-- Grant to super_admin (already has all permissions via wildcard insert, but be explicit)
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'super_admin' AND p.name = 'admin.games.manage'
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Grant to platform_admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r, permissions p
WHERE r.name = 'platform_admin' AND p.name = 'admin.games.manage'
ON CONFLICT (role_id, permission_id) DO NOTHING;
