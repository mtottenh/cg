-- Migration: System settings key-value store
-- Description:
--   Runtime-togglable platform settings, JSONB-valued so each key can carry
--   whatever shape it needs. First consumer: demo_auto_link_enabled — the
--   admin kill-switch for the demo→match auto-link pass at stats ingestion.
--   When disabled, evidence uploads still link demos to their match directly
--   (that path never depended on auto-linking).

CREATE TABLE system_settings (
    key VARCHAR(128) PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO system_settings (key, value)
VALUES ('demo_auto_link_enabled', 'true'::jsonb)
ON CONFLICT (key) DO NOTHING;
