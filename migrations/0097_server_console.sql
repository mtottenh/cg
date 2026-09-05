-- Migration: the admin server console (docs/server-console-design.md).
--
-- Two things. The agent's heartbeat (0.2.0+) carries CS2's `status` output,
-- from which the portal reads the current map and the connected players;
-- the last of each is kept on the server row so the Game Servers table and
-- the offline console can show them without a live round-trip. The raw text
-- is kept too (player addresses redacted): the parser lives in one place and
-- a fix to it applies to rows already stored.
--
-- And admin console commands get their own audit table. `server_events` is
-- MatchZy's ingest queue: its dedupe index is (reservation, event_type, map,
-- round), so a second admin command inside one reservation was silently
-- dropped by ON CONFLICT DO NOTHING. Admin actions are not match events.

ALTER TABLE game_servers
    ADD COLUMN last_map TEXT NULL,
    ADD COLUMN last_player_count INTEGER NULL,
    ADD COLUMN last_status_output TEXT NULL,
    ADD COLUMN last_status_at TIMESTAMPTZ NULL;

COMMENT ON COLUMN game_servers.last_map IS 'Engine map name from the last agent heartbeat that carried a CS2 status (agent 0.2.0+).';
COMMENT ON COLUMN game_servers.last_status_output IS 'Raw CS2 status output from that heartbeat, player addresses redacted, at most 8 KiB.';

CREATE TABLE admin_server_commands (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES game_servers(id) ON DELETE CASCADE,
    reservation_id UUID NULL REFERENCES server_reservations(id) ON DELETE SET NULL,
    admin_user_id UUID NOT NULL REFERENCES users(id),
    -- raw | map_change | action:<name>
    kind TEXT NOT NULL,
    -- What was sent, with secrets masked (sv_password ****).
    command TEXT NOT NULL,
    -- At most 4 KiB, control characters stripped.
    output TEXT NULL,
    ok BOOLEAN NOT NULL,
    force BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_admin_server_commands_server_created
    ON admin_server_commands (server_id, created_at DESC);

COMMENT ON TABLE admin_server_commands IS 'Every console command an admin sent to a game server, with who and what came back.';
