-- Migration: Add auth_provider to users
-- Description: Distinguish password ('local') accounts from Steam OpenID
-- ('steam') accounts. Steam accounts have no usable password_hash (the
-- column is already nullable) and must sign in through Steam.

ALTER TABLE users
    ADD COLUMN auth_provider VARCHAR(16) NOT NULL DEFAULT 'local';

ALTER TABLE users
    ADD CONSTRAINT users_check_auth_provider CHECK (
        auth_provider IN ('local', 'steam')
    );
