-- Player notifications (in-app) and per-tournament Discord delivery.
--
-- A notification belongs to one user. `dedupe_key` makes an event notify a
-- user at most once however many times its trigger fires (the lifecycle pass
-- re-checks the same state every tick).

CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(48) NOT NULL,
    title VARCHAR(160) NOT NULL,
    body TEXT NOT NULL,
    link VARCHAR(512),
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,
    match_id UUID REFERENCES tournament_matches(id) ON DELETE CASCADE,
    dedupe_key VARCHAR(160),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX uq_notifications_user_dedupe
    ON notifications (user_id, dedupe_key)
    WHERE dedupe_key IS NOT NULL;

CREATE INDEX idx_notifications_user_unread
    ON notifications (user_id, read_at NULLS FIRST, created_at DESC);

-- Admin-only delivery settings. Kept out of `tournaments.settings`, which is
-- returned to anyone who can see the tournament: a webhook URL is a secret.
CREATE TABLE tournament_notification_settings (
    tournament_id UUID PRIMARY KEY REFERENCES tournaments(id) ON DELETE CASCADE,
    discord_webhook_url TEXT,
    discord_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL
);
