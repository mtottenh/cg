-- Migration: game-server updates through LinuxGSM
-- (docs/gameserver-lgsm-updates-design.md §7; wire contract: the
-- portal-server-agent README, "Protocol", agent 0.3.0+).
--
-- The agent's root helper (`portal-gs-update`) runs each update as a
-- transaction on the game host and reports it over the agent channel. The
-- portal owns the job record, the safety gates and the audit trail.

-- What the agent last said about the host's update machinery. All NULL /
-- empty for agents older than 0.3.0 and for hosts without the helper.
ALTER TABLE game_servers
    ADD COLUMN agent_capabilities TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN update_check JSONB NULL,
    ADD COLUMN update_checked_at TIMESTAMPTZ NULL,
    ADD COLUMN installed_versions JSONB NULL,
    ADD COLUMN demo_upload_wired BOOLEAN NULL,
    ADD COLUMN overrides_applied BOOLEAN NULL,
    ADD COLUMN reload_pending BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN agent_update_job JSONB NULL,
    ADD COLUMN matchzy_overrides JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN matchzy_overrides_pending BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN game_servers.agent_capabilities IS 'Capabilities from the last agent heartbeat (agent 0.3.0+), e.g. lgsm_update.';
COMMENT ON COLUMN game_servers.update_check IS 'The `check` object of the last update_state frame: installed, latest, update_available, load.';
COMMENT ON COLUMN game_servers.installed_versions IS 'Best-known installed versions: the last check, overlaid by the `after` of later successful jobs.';
COMMENT ON COLUMN game_servers.demo_upload_wired IS 'Whether the demo-upload block is intact in the MatchZy config file; NULL = unknown.';
COMMENT ON COLUMN game_servers.overrides_applied IS 'Whether the portal-managed MatchZy overrides are in the config file; NULL = none set or unknown.';
COMMENT ON COLUMN game_servers.reload_pending IS 'A repair changed the MatchZy config and the running server has not re-read it yet.';
COMMENT ON COLUMN game_servers.agent_update_job IS 'The operation in flight per the last heartbeat ({job_id, operation, phase, status}), including checks.';
COMMENT ON COLUMN game_servers.matchzy_overrides IS 'Portal-managed MatchZy overrides (cvar -> string|number|bool) pushed to the agent with set_matchzy_overrides.';
COMMENT ON COLUMN game_servers.matchzy_overrides_pending IS 'The stored overrides have not been accepted by the agent yet (saved while it was away); pushed on its next connect.';

CREATE TABLE game_server_update_jobs (
    -- UUID v7 when the portal created the job; the agent's own id for jobs
    -- it started (cron, SSH, auto-repair, overrides).
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES game_servers(id) ON DELETE CASCADE,
    -- `check` only ever appears for a FAILED check, which the agent reports
    -- as an update_result so the error is visible.
    operation TEXT NOT NULL
        CHECK (operation IN ('lgsm', 'game', 'mods', 'all', 'repair-config', 'check')),
    status TEXT NOT NULL
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'rolled_back', 'lost')),
    trigger TEXT NOT NULL DEFAULT 'portal'
        CHECK (trigger IN ('portal', 'overrides', 'auto_repair', 'local')),
    -- NULL for jobs the portal did not start.
    requested_by UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    forced BOOLEAN NOT NULL DEFAULT FALSE,
    phase TEXT NULL,
    failed_phase TEXT NULL,
    before_versions JSONB NULL,
    after_versions JSONB NULL,
    latest_versions JSONB NULL,
    load JSONB NULL,
    config_report JSONB NULL,
    restarted BOOLEAN NULL,
    error TEXT NULL,
    -- At most 8 KiB (the newest end is kept).
    log_tail TEXT NULL,
    -- The agent made this result up because the helper never produced one;
    -- a real result for the same job supersedes it.
    agent_synthesized BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    -- Bumped by every progress frame; the lost-job pass reads it.
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The helper holds one lock per host: at most one job in flight per server.
CREATE UNIQUE INDEX uq_game_server_update_jobs_active
    ON game_server_update_jobs (server_id)
    WHERE status IN ('queued', 'running');

CREATE INDEX idx_game_server_update_jobs_server_created
    ON game_server_update_jobs (server_id, created_at DESC);

COMMENT ON TABLE game_server_update_jobs IS 'LinuxGSM update jobs run by the agent''s helper on a game server: one row per job id, portal-started or not.';
