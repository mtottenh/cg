-- Individual public match results per player, materialized from
-- discovered_matches.gc_data at enrichment time.

CREATE TABLE player_match_history (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id           UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_id             UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    discovered_match_id UUID NOT NULL REFERENCES discovered_matches(id) ON DELETE CASCADE,

    -- Match metadata (from GC MatchInfo + demo)
    map                 TEXT NOT NULL DEFAULT '',
    match_time          TIMESTAMPTZ,
    team_scores         INT[] NOT NULL DEFAULT '{}',
    match_duration_secs INT NOT NULL DEFAULT 0,
    match_result        TEXT NOT NULL DEFAULT 'unknown',

    -- Per-player stats for this match (from GC PlayerStats)
    kills               INT NOT NULL DEFAULT 0,
    deaths              INT NOT NULL DEFAULT 0,
    assists             INT NOT NULL DEFAULT 0,
    score               INT NOT NULL DEFAULT 0,
    headshots           INT NOT NULL DEFAULT 0,
    mvps                INT NOT NULL DEFAULT 0,
    entry_3k            INT NOT NULL DEFAULT 0,
    entry_4k            INT NOT NULL DEFAULT 0,
    entry_5k            INT NOT NULL DEFAULT 0,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Prevent duplicate entries for same player + discovered_match
    CONSTRAINT player_match_history_unique UNIQUE (player_id, discovered_match_id)
);

CREATE INDEX idx_player_match_history_recent
    ON player_match_history (player_id, game_id, match_time DESC);

CREATE INDEX idx_player_match_history_discovered
    ON player_match_history (discovered_match_id);
