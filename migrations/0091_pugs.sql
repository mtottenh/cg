-- Migration: Pick-Up Games (PUGs)
-- Design: ~/.claude/plans/pug-design.md (2026-07-26)
--
-- A PUG is a one-off match outside any league/tournament: a creator gathers
-- players via an ephemeral join code, teams self-assign, maps are chosen by
-- standard veto or "the wheel" (weighted random pick), and the match runs on
-- the existing game-server pipeline.
--
-- Model: the `pugs` table owns the social/gathering phase. At lock-in the PUG
-- materializes as a hidden single-match container tournament (kind='pug') with
-- two ad-hoc teams, so veto sessions, server reservations, MatchZy configs,
-- events, results and demos all run unmodified. PUG stats stay separate:
-- the profile stats updater skips kind='pug' and demos are categorized 'pug'.

-- =============================================================================
-- 1. AD-HOC TEAMS (finishing what 0030 anticipated on registrations.adhoc_team_id)
-- =============================================================================

CREATE TABLE tournament_adhoc_teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    name VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_adhoc_teams_tournament ON tournament_adhoc_teams(tournament_id);

CREATE TABLE tournament_adhoc_team_members (
    adhoc_team_id UUID NOT NULL REFERENCES tournament_adhoc_teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    is_captain BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (adhoc_team_id, player_id)
);

CREATE INDEX idx_adhoc_team_members_player ON tournament_adhoc_team_members(player_id);

ALTER TABLE tournament_registrations
    ADD CONSTRAINT tournament_registrations_fk_adhoc_team
    FOREIGN KEY (adhoc_team_id) REFERENCES tournament_adhoc_teams(id) ON DELETE CASCADE;

COMMENT ON TABLE tournament_adhoc_teams IS 'Ephemeral rosters for adhoc-participant tournaments (PUG containers)';

-- =============================================================================
-- 2. TOURNAMENT KIND (hide PUG containers from public listings)
-- =============================================================================

ALTER TABLE tournaments
    ADD COLUMN kind VARCHAR(16) NOT NULL DEFAULT 'standard';
ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_check_kind CHECK (kind IN ('standard', 'pug'));

CREATE INDEX idx_tournaments_kind ON tournaments(kind) WHERE kind <> 'standard';

COMMENT ON COLUMN tournaments.kind IS 'standard = real tournament; pug = hidden single-match PUG container';

-- =============================================================================
-- 3. THE PUG AGGREGATE
-- =============================================================================

CREATE TABLE pugs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id),
    created_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Ephemeral invite code (rotatable; dead once the pug leaves 'gathering')
    join_code VARCHAR(16) NOT NULL,

    status VARCHAR(16) NOT NULL DEFAULT 'gathering',

    -- Settings chosen at creation
    match_format VARCHAR(4) NOT NULL,
    map_selection_mode VARCHAR(8) NOT NULL,
    side_selection_mode VARCHAR(32) NOT NULL DEFAULT 'knife',
    team_size INTEGER NOT NULL,
    region VARCHAR(32),
    map_pool TEXT[],                 -- veto mode: custom pool (NULL = game default)
    listed BOOLEAN NOT NULL DEFAULT FALSE,  -- opt-in: show in the public open-PUGs browser

    -- Set at materialization (lock)
    tournament_id UUID REFERENCES tournaments(id) ON DELETE SET NULL,
    match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,

    -- Denormalized at series end (results feed never joins four tables)
    winner_team SMALLINT,
    team1_score INTEGER,
    team2_score INTEGER,
    completed_at TIMESTAMPTZ,

    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pugs_check_status CHECK (status IN (
        'gathering', 'map_selection', 'awaiting_server', 'live',
        'completed', 'cancelled', 'expired'
    )),
    CONSTRAINT pugs_check_format CHECK (match_format IN ('bo1', 'bo3', 'bo5')),
    CONSTRAINT pugs_check_map_mode CHECK (map_selection_mode IN ('veto', 'wheel')),
    CONSTRAINT pugs_check_team_size CHECK (team_size >= 1 AND team_size <= 16),
    CONSTRAINT pugs_check_winner_team CHECK (winner_team IS NULL OR winner_team IN (1, 2))
);

CREATE UNIQUE INDEX uq_pugs_join_code ON pugs(join_code);
CREATE INDEX idx_pugs_creator ON pugs(created_by_user_id, status);
CREATE INDEX idx_pugs_match ON pugs(match_id) WHERE match_id IS NOT NULL;
-- Sweeper scans for stale gathering lobbies
CREATE INDEX idx_pugs_gathering_expiry ON pugs(expires_at) WHERE status = 'gathering';
-- Public open-PUGs browser
CREATE INDEX idx_pugs_listed_open ON pugs(created_at) WHERE status = 'gathering' AND listed;

CREATE TRIGGER pugs_updated_at
    BEFORE UPDATE ON pugs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE pug_players (
    pug_id UUID NOT NULL REFERENCES pugs(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    team SMALLINT,                   -- NULL = unassigned bench
    is_captain BOOLEAN NOT NULL DEFAULT FALSE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (pug_id, player_id),
    CONSTRAINT pug_players_check_team CHECK (team IS NULL OR team IN (1, 2))
);

CREATE INDEX idx_pug_players_player ON pug_players(player_id);

-- One wheel nomination per player per pug; upsert to change
CREATE TABLE pug_wheel_entries (
    pug_id UUID NOT NULL REFERENCES pugs(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    map_id VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (pug_id, player_id)
);

-- Audit + deterministic client replay of each spin
CREATE TABLE pug_wheel_spins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pug_id UUID NOT NULL REFERENCES pugs(id) ON DELETE CASCADE,
    game_number INTEGER NOT NULL,
    entries JSONB NOT NULL,          -- [{map_id, weight, nominated_by:[player names]}] snapshot
    winner_map_id VARCHAR(64) NOT NULL,
    spin_seed BIGINT NOT NULL,       -- drives the identical animation on every client
    spun_by_player_id UUID REFERENCES players(id),
    spun_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT pug_wheel_spins_unique_game UNIQUE (pug_id, game_number)
);

COMMENT ON TABLE pugs IS 'Pick-up game lobbies; materialize as kind=pug container tournaments at lock';

-- =============================================================================
-- 4. SERVER PLUMBING (pre-authorized by docs/matchzy-integration.md §6.7)
-- =============================================================================

ALTER TABLE server_reservations
    ADD COLUMN reservation_kind VARCHAR(8) NOT NULL DEFAULT 'match';
ALTER TABLE server_reservations
    ADD CONSTRAINT server_reservations_check_kind CHECK (reservation_kind IN ('match', 'pug'));

-- Admins can fence premium boxes off from PUGs
ALTER TABLE game_servers
    ADD COLUMN allow_pugs BOOLEAN NOT NULL DEFAULT TRUE;

COMMENT ON COLUMN server_reservations.reservation_kind IS 'match = tournament match; pug = pick-up game (queued behind matches)';

-- =============================================================================
-- 5. WHEEL VETO ACTION TYPE
-- =============================================================================

-- 'random' = server-side weighted RNG pick ("the wheel"); recorded as an
-- auto action (performed_by NULL, auto_action_reason='wheel_spin')
ALTER TABLE veto_actions DROP CONSTRAINT veto_actions_check_type;
ALTER TABLE veto_actions
    ADD CONSTRAINT veto_actions_check_type CHECK (action_type IN ('ban', 'pick', 'decider', 'random'));
