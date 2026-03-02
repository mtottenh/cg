-- Steam match tracking per player.
-- Players opt in with their game authentication code so the poller bot can
-- discover new share codes on their behalf.

CREATE TABLE steam_tracking (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_id         UUID NOT NULL REFERENCES games(id),
    steam_id_64     BIGINT NOT NULL,
    game_auth_code  TEXT NOT NULL,
    last_known_code TEXT,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    poll_errors     INT NOT NULL DEFAULT 0,
    last_poll_at    TIMESTAMPTZ,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_steam_tracking_player_game UNIQUE (player_id, game_id),
    CONSTRAINT uq_steam_tracking_steam_id_game UNIQUE (steam_id_64, game_id)
);

CREATE INDEX idx_steam_tracking_active ON steam_tracking (is_active, game_id) WHERE is_active;
