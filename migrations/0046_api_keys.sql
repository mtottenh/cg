-- API keys for service-to-service authentication (bot ↔ Portal API).
-- Only the SHA-256 hash of the key is stored; the raw key is shown once at creation.

CREATE TABLE api_keys (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    service_name TEXT NOT NULL,
    key_hash     TEXT NOT NULL UNIQUE,
    key_prefix   TEXT NOT NULL,
    permissions  TEXT[] NOT NULL DEFAULT '{}',
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,
    expires_at   TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_by   UUID REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_key_hash ON api_keys (key_hash);
