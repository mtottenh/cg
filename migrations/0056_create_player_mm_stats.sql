-- Aggregate public matchmaking stats per player per game.
-- Separate from player_game_profiles which tracks tournament/league stats.

CREATE TABLE player_mm_stats (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id           UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_id             UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,

    -- Aggregate match stats (accumulated from GC data)
    matches_played      INT NOT NULL DEFAULT 0,
    wins                INT NOT NULL DEFAULT 0,
    losses              INT NOT NULL DEFAULT 0,
    draws               INT NOT NULL DEFAULT 0,
    kills               INT NOT NULL DEFAULT 0,
    deaths              INT NOT NULL DEFAULT 0,
    assists             INT NOT NULL DEFAULT 0,
    headshots           INT NOT NULL DEFAULT 0,
    mvps                INT NOT NULL DEFAULT 0,
    entry_3k            INT NOT NULL DEFAULT 0,
    entry_4k            INT NOT NULL DEFAULT 0,
    entry_5k            INT NOT NULL DEFAULT 0,
    total_score         INT NOT NULL DEFAULT 0,
    total_duration_secs INT NOT NULL DEFAULT 0,

    -- Timestamps
    first_match_at      TIMESTAMPTZ,
    last_match_at       TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT player_mm_stats_unique UNIQUE (player_id, game_id)
);

CREATE INDEX idx_player_mm_stats_player ON player_mm_stats (player_id);
CREATE INDEX idx_player_mm_stats_game ON player_mm_stats (player_id, game_id);

CREATE TRIGGER player_mm_stats_updated_at
    BEFORE UPDATE ON player_mm_stats
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
