-- Migration: Demo player stat facts (EAV)
-- Description:
--   Queryable per-(demo, player, stat) fact rows extracted from
--   demos.stats_json at ingest. The stat space is open-ended (every weapon x
--   every modifier), so facts are EAV keyed by a catalog-defined stat_key
--   (e.g. 'headshot_kills', 'kills.weapon.mag7', 'kills.while_blind') rather
--   than wide columns. stats_json remains the immutable source of truth;
--   these rows are a derived projection (extractor_version tracks the
--   extraction logic so catalog growth is a backfill, not a schema event).
--
--   Design: docs/design-tournament-awards.md §3.2

CREATE TABLE demo_player_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    demo_id UUID NOT NULL REFERENCES demos(id) ON DELETE CASCADE,

    -- Identity: steam_id always present; player_id when resolved against
    -- the portal roster (award standings rank resolved players only).
    steam_id VARCHAR(32) NOT NULL,
    player_id UUID REFERENCES players(id) ON DELETE SET NULL,

    stat_key VARCHAR(128) NOT NULL,
    value DOUBLE PRECISION NOT NULL,

    extractor_version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT demo_player_stats_unique UNIQUE (demo_id, steam_id, stat_key)
);

-- Leaderboard access pattern: WHERE stat_key = $1 [AND demo_id IN scope]
-- GROUP BY player. The composite index serves the filter+join; the demo
-- index serves per-demo delete-and-reinsert on re-extraction.
CREATE INDEX idx_demo_player_stats_key ON demo_player_stats(stat_key, demo_id);
CREATE INDEX idx_demo_player_stats_demo ON demo_player_stats(demo_id);
CREATE INDEX idx_demo_player_stats_player
    ON demo_player_stats(player_id) WHERE player_id IS NOT NULL;

COMMENT ON TABLE demo_player_stats IS
    'EAV stat facts per (demo, player, stat_key), extracted from demos.stats_json';
