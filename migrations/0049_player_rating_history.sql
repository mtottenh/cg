-- Player rating history for tracking in-game rating changes over time.
-- Populated by external services (e.g. steam_bot) that periodically
-- submit observed player ratings from matchmaking demos.

CREATE TABLE player_rating_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id),
    game_id UUID NOT NULL REFERENCES games(id),
    rating INT NOT NULL,
    source VARCHAR(64) NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_player_rating_history_lookup
    ON player_rating_history (player_id, game_id, recorded_at DESC);
