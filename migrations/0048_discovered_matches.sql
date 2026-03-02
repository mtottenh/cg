-- Discovered matches from Steam share code polling.
-- These represent matches found but not yet enriched with full GC data.

CREATE TYPE discovered_match_status AS ENUM (
    'pending', 'enriching', 'enriched', 'failed', 'expired'
);

CREATE TABLE discovered_matches (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tracking_id     UUID NOT NULL REFERENCES steam_tracking(id) ON DELETE CASCADE,
    game_id         UUID NOT NULL REFERENCES games(id),
    share_code      TEXT NOT NULL UNIQUE,
    match_id        BIGINT NOT NULL,
    outcome_id      BIGINT NOT NULL,
    token           INT NOT NULL,
    status          discovered_match_status NOT NULL DEFAULT 'pending',
    gc_data         JSONB,
    demo_url        TEXT,
    demo_id         UUID REFERENCES demos(id),
    error           TEXT,
    retry_count     INT NOT NULL DEFAULT 0,
    max_retries     INT NOT NULL DEFAULT 3,
    discovered_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    enriched_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_discovered_matches_status ON discovered_matches (status);
CREATE INDEX idx_discovered_matches_pending
    ON discovered_matches (status, created_at) WHERE status IN ('pending', 'failed');
CREATE INDEX idx_discovered_matches_tracking ON discovered_matches (tracking_id);
