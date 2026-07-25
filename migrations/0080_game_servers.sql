-- Migration: Game server integration (MatchZy)
-- Design: docs/matchzy-integration.md §4 (data model), §4.1 (cert registry),
--         §6.7 (busyness/bookings), §6.8 (mid-series substitutions)
--
-- Registered CS2 servers run MatchZy + the portal-server-agent. The portal
-- reserves a server per match after veto completes, serves it a MatchZy match
-- config, and ingests its webhooks. Control channel is the agent's outbound
-- mTLS WSS connection; the portal stores NO RCON credentials.
--
-- Phase 1 uses game_servers + server_agent_certs (+ bookings for admin holds);
-- reservations/events/substitutions land here too so the schema matches the
-- design doc and later phases are code-only.

CREATE TABLE game_servers (
    id UUID PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    game_id UUID NOT NULL REFERENCES games(id),

    -- Connection info shown to players / used in connect strings
    ip_address INET NOT NULL,
    port INTEGER NOT NULL,
    gotv_port INTEGER,
    region VARCHAR(32) NOT NULL,

    -- Ops
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    status VARCHAR(20) NOT NULL DEFAULT 'offline',
    current_match_id UUID REFERENCES tournament_matches(id),

    -- Agent identity & health (heartbeats over the mTLS channel)
    agent_cert_serial VARCHAR(64),
    agent_cert_expires_at TIMESTAMPTZ,
    agent_version VARCHAR(32),
    last_heartbeat_at TIMESTAMPTZ,
    last_gamestate VARCHAR(32),

    -- Enrollment (one-time token, hashed like api_keys.key_hash)
    enrollment_token_hash VARCHAR(64),
    enrollment_token_expires_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT game_servers_check_port CHECK (port > 0 AND port < 65536),
    CONSTRAINT game_servers_check_gotv_port CHECK (
        gotv_port IS NULL OR (gotv_port > 0 AND gotv_port < 65536)
    ),
    -- 'busy_external': heartbeat shows a match the portal didn't set up (§6.7)
    CONSTRAINT game_servers_check_status CHECK (status IN (
        'offline', 'available', 'reserved', 'configuring', 'in_match',
        'busy_external', 'error'
    ))
);

CREATE INDEX idx_game_servers_available ON game_servers(game_id, region, status)
    WHERE status = 'available' AND enabled;
CREATE INDEX idx_game_servers_match ON game_servers(current_match_id)
    WHERE current_match_id IS NOT NULL;

-- Issued agent client certificates. Caddy verifies chain + validity against
-- the portal CA; the API resolves the presented serial here for revocation
-- and server binding (§5.4). Multiple rows per server across renewals.
CREATE TABLE server_agent_certs (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES game_servers(id) ON DELETE CASCADE,
    serial VARCHAR(64) NOT NULL UNIQUE,
    fingerprint_sha256 VARCHAR(64) NOT NULL,
    not_before TIMESTAMPTZ NOT NULL,
    not_after TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_agent_certs_server ON server_agent_certs(server_id)
    WHERE revoked_at IS NULL;

-- Scheduled event holds: block a server (window) for a tournament/event so
-- auto-allocation for other tournaments — or any future pug/scrim consumer —
-- cannot take it (§6.7). A NULL tournament_id is a hard hold (maintenance).
-- Bookings never touch game_servers.status; they filter allocation.
CREATE TABLE server_bookings (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES game_servers(id) ON DELETE CASCADE,
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,
    reason VARCHAR(255),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT server_bookings_check_window CHECK (ends_at > starts_at)
);

CREATE INDEX idx_server_bookings_window ON server_bookings(server_id, starts_at, ends_at);

-- One reservation per match play-through. matchzy_id is the integer matchid
-- MatchZy requires; it is also the correlation key on every webhook event.
CREATE TABLE server_reservations (
    id UUID PRIMARY KEY,
    server_id UUID NOT NULL REFERENCES game_servers(id),
    match_id UUID NOT NULL REFERENCES tournament_matches(id),
    matchzy_id BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,

    status VARCHAR(20) NOT NULL DEFAULT 'pending',

    -- Secrets, generated per reservation. Tokens stored as SHA-256 hex.
    connect_password VARCHAR(64) NOT NULL,
    gotv_password VARCHAR(64),
    config_token_hash VARCHAR(64) NOT NULL,
    event_token_hash VARCHAR(64) NOT NULL,
    config_token_expires_at TIMESTAMPTZ NOT NULL,

    -- The exact MatchZy config served (audit + rebuild-on-reassignment)
    match_config JSONB,

    config_fetched_at TIMESTAMPTZ,
    went_live_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    failure_reason TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT server_reservations_check_status CHECK (status IN (
        'pending', 'configuring', 'ready', 'live',
        'completed', 'failed', 'cancelled'
    ))
);

-- §6.7 invariant: server_reservations is the single arbiter of busyness.
-- Double-booking is unrepresentable, not merely unlikely.
CREATE UNIQUE INDEX uq_server_reservations_live_match ON server_reservations(match_id)
    WHERE status IN ('pending', 'configuring', 'ready', 'live');
CREATE UNIQUE INDEX uq_server_reservations_live_server ON server_reservations(server_id)
    WHERE status IN ('pending', 'configuring', 'ready', 'live');
CREATE INDEX idx_server_reservations_match ON server_reservations(match_id);

-- Raw MatchZy webhook events. Insert-then-process; the dedupe index makes
-- MatchZy's no-retry/no-dedup delivery idempotent on our side (§6.4).
CREATE TABLE server_events (
    id UUID PRIMARY KEY,
    reservation_id UUID REFERENCES server_reservations(id) ON DELETE SET NULL,
    server_id UUID REFERENCES game_servers(id) ON DELETE SET NULL,
    -- MatchZy owns this vocabulary (series_start, going_live, round_end,
    -- map_result, series_end, ...) — deliberately no CHECK enum.
    event_type VARCHAR(64) NOT NULL,
    map_number INTEGER,
    round_number INTEGER,
    payload JSONB NOT NULL,
    processed BOOLEAN NOT NULL DEFAULT FALSE,
    processed_at TIMESTAMPTZ,
    processing_error TEXT,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_events_unprocessed ON server_events(received_at)
    WHERE processed = FALSE;
CREATE UNIQUE INDEX uq_server_events_dedupe ON server_events(
    reservation_id, event_type,
    COALESCE(map_number, -1), COALESCE(round_number, -1)
);

-- Mid-series substitutions (§6.8). A substitution creates new effective-lineup
-- state from from_game_number onward; history is never rewritten.
CREATE TABLE match_substitutions (
    id UUID PRIMARY KEY,
    match_id UUID NOT NULL REFERENCES tournament_matches(id),
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    reservation_id UUID REFERENCES server_reservations(id) ON DELETE SET NULL,
    player_out_id UUID NOT NULL REFERENCES players(id),
    -- NULL = play short-handed
    player_in_id UUID REFERENCES players(id),
    from_game_number INTEGER NOT NULL,

    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    requested_by UUID NOT NULL REFERENCES users(id),
    approved_by UUID REFERENCES users(id),
    failure_reason TEXT,
    applied_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT match_substitutions_check_status CHECK (status IN (
        'pending', 'awaiting_approval', 'applying', 'applied',
        'failed', 'rejected', 'cancelled'
    )),
    CONSTRAINT match_substitutions_check_players CHECK (
        player_in_id IS NULL OR player_in_id <> player_out_id
    )
);

CREATE INDEX idx_match_substitutions_match ON match_substitutions(match_id);
CREATE UNIQUE INDEX uq_match_substitutions_live ON match_substitutions(match_id, player_out_id)
    WHERE status IN ('pending', 'awaiting_approval', 'applying');

-- Server-sourced result claims (§6.5): a series_end webhook creates a claim
-- with source='server' and no submitting registration. Existing participant
-- claims are backfilled by the DEFAULT.
ALTER TABLE result_claims
    ADD COLUMN source VARCHAR(16) NOT NULL DEFAULT 'participant';
ALTER TABLE result_claims
    ADD CONSTRAINT result_claims_check_source CHECK (
        source IN ('participant', 'server', 'admin')
    );
ALTER TABLE result_claims
    ALTER COLUMN submitted_by_registration_id DROP NOT NULL;
ALTER TABLE result_claims
    ALTER COLUMN submitted_by_user_id DROP NOT NULL;
-- Participant claims must still carry a submitter; server claims must not.
ALTER TABLE result_claims
    ADD CONSTRAINT result_claims_check_submitter CHECK (
        (source = 'participant' AND submitted_by_registration_id IS NOT NULL
                                AND submitted_by_user_id IS NOT NULL)
        OR source <> 'participant'
    );

-- Permission: game-server registry administration (0062/0065 model —
-- super_admin + platform_admin only).
INSERT INTO permissions (name, display_name, description, category, is_dangerous) VALUES
('admin.servers.manage', 'Manage Game Servers',
 'Register, edit, enroll, revoke, and remove game servers; manage bookings', 'admin', TRUE)
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name IN ('super_admin', 'platform_admin')
  AND p.name = 'admin.servers.manage'
ON CONFLICT DO NOTHING;
