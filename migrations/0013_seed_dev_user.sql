-- Migration: Seed dev user for local development
-- Description: Creates a well-known dev user and player consumed by the
-- dev-token auth path that is compiled in only under the `test-utils` cargo
-- feature. Production binaries do not contain the dev-token branch, so this
-- user has no special meaning there.
-- Note: This user has a fixed UUID that matches the auth extractor constants

-- Create dev user (only if not exists)
INSERT INTO users (id, username, email, password_hash, status, email_verified)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'devuser',
    'dev@example.com',
    '$argon2id$v=19$m=19456,t=2,p=1$placeholder', -- Not a real password hash
    'active',
    TRUE
)
ON CONFLICT (id) DO NOTHING;

-- Create dev player
INSERT INTO players (id, user_id, display_name, country_code)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'DevPlayer',
    'US'
)
ON CONFLICT (id) DO NOTHING;
