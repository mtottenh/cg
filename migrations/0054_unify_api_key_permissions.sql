-- Migration: Unify API key permissions with RBAC permissions table
-- Description: Replace api_keys.permissions TEXT[] with api_key_permissions join table
--              referencing the shared permissions table.

BEGIN;

-- 1. Seed the 7 service permissions into the shared permissions table
INSERT INTO permissions (name, display_name, description, category, is_dangerous)
VALUES
    ('steam_tracking.read',       'Steam Tracking Read',       'Read active steam tracking entries',                    'service', false),
    ('steam_tracking.write',      'Steam Tracking Write',      'Update steam tracking poll results',                    'service', false),
    ('discovered_matches.read',   'Discovered Matches Read',   'Read pending discovered matches',                       'service', false),
    ('discovered_matches.write',  'Discovered Matches Write',  'Submit, claim, and update discovered matches',          'service', false),
    ('demos.catalog',             'Demos Catalog',             'Catalog (batch-create) demo records',                   'service', false),
    ('demos.read',                'Demos Read',                'Read demo records (e.g. pending demos for processing)', 'service', false),
    ('demos.stats',               'Demos Stats',               'Submit or update demo stats (parse results)',           'service', false)
ON CONFLICT (name) DO NOTHING;

-- 2. Create the api_key_permissions join table
CREATE TABLE api_key_permissions (
    api_key_id    UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (api_key_id, permission_id)
);

CREATE INDEX idx_api_key_permissions_api_key ON api_key_permissions(api_key_id);

-- 3. Migrate existing data from TEXT[] -> join table
INSERT INTO api_key_permissions (api_key_id, permission_id)
SELECT ak.id, p.id
FROM api_keys ak,
     LATERAL unnest(ak.permissions) AS perm_name
JOIN permissions p ON p.name = perm_name;

-- 4. Drop the permissions column from api_keys
ALTER TABLE api_keys DROP COLUMN permissions;

COMMIT;
