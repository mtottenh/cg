-- Migration: Create Tournament System
-- Description: Core tournament infrastructure for competitive gaming portal

-- =============================================================================
-- 1. TOURNAMENTS
-- =============================================================================

CREATE TABLE tournaments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id),

    -- Optional league/season linkage
    league_id UUID REFERENCES leagues(id) ON DELETE SET NULL,
    season_id UUID REFERENCES league_seasons(id) ON DELETE SET NULL,

    -- Identity
    name VARCHAR(128) NOT NULL,
    slug VARCHAR(128) NOT NULL,
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),

    -- Format
    format VARCHAR(64) NOT NULL,                    -- 'single_elimination', 'double_elimination', etc.
    format_settings JSONB NOT NULL DEFAULT '{}',   -- Format-specific config
    participant_type VARCHAR(32) NOT NULL DEFAULT 'team',
    team_size INTEGER,

    -- Capacity
    min_participants INTEGER NOT NULL DEFAULT 2,
    max_participants INTEGER NOT NULL DEFAULT 64,

    -- Registration
    registration_type VARCHAR(32) NOT NULL DEFAULT 'open',
    registration_start TIMESTAMPTZ,
    registration_end TIMESTAMPTZ,
    check_in_start TIMESTAMPTZ,
    check_in_end TIMESTAMPTZ,
    check_in_required BOOLEAN NOT NULL DEFAULT false,

    -- Scheduling
    scheduling_mode VARCHAR(32) NOT NULL DEFAULT 'live',
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    timezone_hint VARCHAR(64),

    -- Match settings
    default_match_format VARCHAR(16) NOT NULL DEFAULT 'bo1',
    default_map_veto_format VARCHAR(64),

    -- Prize pool (JSONB for flexibility)
    prize_pool JSONB,

    -- Rules
    rules_url VARCHAR(512),
    settings JSONB NOT NULL DEFAULT '{}',

    -- Policies
    withdrawal_policy VARCHAR(32) NOT NULL DEFAULT 'forfeit',

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'draft',

    -- Ownership
    created_by UUID NOT NULL REFERENCES users(id),
    organization_id UUID,  -- Future: REFERENCES organizations(id)

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT tournaments_slug_unique UNIQUE (slug),
    CONSTRAINT tournaments_check_status CHECK (status IN (
        'draft', 'published', 'registration', 'check_in',
        'seeding', 'in_progress', 'completed', 'cancelled'
    )),
    CONSTRAINT tournaments_check_format CHECK (format IN (
        'single_elimination', 'double_elimination', 'round_robin',
        'swiss', 'groups_and_playoffs', 'custom'
    )),
    CONSTRAINT tournaments_check_participant_type CHECK (participant_type IN (
        'team', 'individual', 'adhoc'
    )),
    CONSTRAINT tournaments_check_registration_type CHECK (registration_type IN (
        'open', 'invite_only', 'qualification', 'approval'
    )),
    CONSTRAINT tournaments_check_scheduling_mode CHECK (scheduling_mode IN (
        'live', 'self_scheduled', 'hybrid'
    )),
    CONSTRAINT tournaments_check_withdrawal_policy CHECK (withdrawal_policy IN (
        'forfeit', 'reseeding', 'waitlist_promotion', 'admin_decision'
    )),
    CONSTRAINT tournaments_check_participants CHECK (
        min_participants >= 2 AND
        max_participants >= min_participants
    ),
    CONSTRAINT tournaments_check_dates CHECK (
        (registration_start IS NULL OR registration_end IS NULL OR registration_start < registration_end) AND
        (registration_end IS NULL OR starts_at IS NULL OR registration_end <= starts_at)
    )
);

CREATE INDEX idx_tournaments_game ON tournaments(game_id);
CREATE INDEX idx_tournaments_league ON tournaments(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX idx_tournaments_season ON tournaments(season_id) WHERE season_id IS NOT NULL;
CREATE INDEX idx_tournaments_status ON tournaments(status);
CREATE INDEX idx_tournaments_starts_at ON tournaments(starts_at) WHERE starts_at IS NOT NULL;
CREATE INDEX idx_tournaments_slug ON tournaments(slug);

CREATE TRIGGER tournaments_updated_at
    BEFORE UPDATE ON tournaments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournaments IS 'Tournament events with configurable formats and registration';

-- =============================================================================
-- 2. TOURNAMENT STAGES
-- =============================================================================

CREATE TABLE tournament_stages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Identity
    name VARCHAR(64) NOT NULL,
    stage_order INTEGER NOT NULL,

    -- Format
    format VARCHAR(64) NOT NULL,
    format_settings JSONB NOT NULL DEFAULT '{}',

    -- Advancement
    advancement_count INTEGER,
    advancement_rule VARCHAR(32) NOT NULL DEFAULT 'top_n',

    -- Match settings (override tournament defaults)
    match_format VARCHAR(16),
    map_veto_format VARCHAR(64),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timing
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_stages_unique_order UNIQUE (tournament_id, stage_order),
    CONSTRAINT tournament_stages_check_status CHECK (status IN (
        'pending', 'active', 'completed', 'cancelled'
    )),
    CONSTRAINT tournament_stages_check_format CHECK (format IN (
        'single_elimination', 'double_elimination', 'round_robin',
        'swiss', 'group_stage'
    )),
    CONSTRAINT tournament_stages_check_advancement CHECK (advancement_rule IN (
        'top_n', 'top_n_per_group', 'manual'
    ))
);

CREATE INDEX idx_tournament_stages_tournament ON tournament_stages(tournament_id);
CREATE INDEX idx_tournament_stages_order ON tournament_stages(tournament_id, stage_order);

CREATE TRIGGER tournament_stages_updated_at
    BEFORE UPDATE ON tournament_stages
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_stages IS 'Stages within a multi-stage tournament';

-- =============================================================================
-- 3. TOURNAMENT BRACKETS
-- =============================================================================

CREATE TABLE tournament_brackets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stage_id UUID NOT NULL REFERENCES tournament_stages(id) ON DELETE CASCADE,
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Identity
    name VARCHAR(64) NOT NULL,
    bracket_type VARCHAR(32) NOT NULL,

    -- Structure
    total_rounds INTEGER NOT NULL DEFAULT 1,
    current_round INTEGER NOT NULL DEFAULT 0,

    -- For groups
    group_number INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_brackets_check_type CHECK (bracket_type IN (
        'winners', 'losers', 'single_elim', 'round_robin', 'swiss', 'grand_final'
    )),
    CONSTRAINT tournament_brackets_check_status CHECK (status IN (
        'pending', 'active', 'completed', 'cancelled'
    ))
);

CREATE INDEX idx_tournament_brackets_stage ON tournament_brackets(stage_id);
CREATE INDEX idx_tournament_brackets_tournament ON tournament_brackets(tournament_id);

CREATE TRIGGER tournament_brackets_updated_at
    BEFORE UPDATE ON tournament_brackets
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_brackets IS 'Brackets within tournament stages';

-- =============================================================================
-- 4. TOURNAMENT REGISTRATIONS
-- =============================================================================

CREATE TABLE tournament_registrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Participant identity (exactly one should be set based on tournament type)
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    adhoc_team_id UUID,  -- Future: REFERENCES tournament_adhoc_teams(id)

    -- Denormalized display info
    participant_name VARCHAR(128) NOT NULL,
    participant_logo_url VARCHAR(512),

    -- Registration
    registered_by UUID NOT NULL REFERENCES users(id),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Check-in
    checked_in BOOLEAN NOT NULL DEFAULT false,
    checked_in_at TIMESTAMPTZ,
    checked_in_by UUID REFERENCES users(id),

    -- Seeding
    seed INTEGER,
    seed_rating INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Admin
    admin_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    withdrawn_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT tournament_registrations_one_participant CHECK (
        (team_season_id IS NOT NULL)::int +
        (player_id IS NOT NULL)::int +
        (adhoc_team_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT tournament_registrations_unique_team UNIQUE (tournament_id, team_season_id),
    CONSTRAINT tournament_registrations_unique_player UNIQUE (tournament_id, player_id),
    CONSTRAINT tournament_registrations_unique_adhoc UNIQUE (tournament_id, adhoc_team_id),
    CONSTRAINT tournament_registrations_check_status CHECK (status IN (
        'pending', 'approved', 'checked_in', 'active',
        'eliminated', 'disqualified', 'withdrawn', 'no_show'
    ))
);

CREATE INDEX idx_tournament_registrations_tournament ON tournament_registrations(tournament_id);
CREATE INDEX idx_tournament_registrations_team_season ON tournament_registrations(team_season_id)
    WHERE team_season_id IS NOT NULL;
CREATE INDEX idx_tournament_registrations_player ON tournament_registrations(player_id)
    WHERE player_id IS NOT NULL;
CREATE INDEX idx_tournament_registrations_status ON tournament_registrations(tournament_id, status);
CREATE INDEX idx_tournament_registrations_seed ON tournament_registrations(tournament_id, seed)
    WHERE seed IS NOT NULL;

CREATE TRIGGER tournament_registrations_updated_at
    BEFORE UPDATE ON tournament_registrations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_registrations IS 'Participant registrations for tournaments';

-- =============================================================================
-- 5. TOURNAMENT MATCHES
-- =============================================================================

CREATE TABLE tournament_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL REFERENCES tournament_brackets(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL REFERENCES tournament_stages(id) ON DELETE CASCADE,
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Position in bracket
    round INTEGER NOT NULL,
    match_number INTEGER NOT NULL,
    bracket_position VARCHAR(16) NOT NULL,  -- e.g., "W1-1", "L2-3", "GF"

    -- Participants
    participant1_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,
    participant2_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,

    -- Denormalized participant info (cached for bracket display)
    participant1_name VARCHAR(128),
    participant1_logo_url VARCHAR(512),
    participant1_seed INTEGER,
    participant2_name VARCHAR(128),
    participant2_logo_url VARCHAR(512),
    participant2_seed INTEGER,

    -- Source tracking (JSONB for flexibility)
    participant1_source JSONB,
    participant2_source JSONB,

    -- Match format
    match_format VARCHAR(16) NOT NULL DEFAULT 'bo1',
    maps_required INTEGER NOT NULL DEFAULT 1,

    -- Scheduling
    scheduled_at TIMESTAMPTZ,
    schedule_deadline TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Results
    participant1_score INTEGER NOT NULL DEFAULT 0,
    participant2_score INTEGER NOT NULL DEFAULT 0,
    winner_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,
    loser_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,

    -- Progression
    winner_progresses_to UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    loser_progresses_to UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Disputes
    disputed BOOLEAN NOT NULL DEFAULT false,
    dispute_reason TEXT,
    dispute_resolved_by UUID REFERENCES users(id),
    dispute_resolution TEXT,
    dispute_resolved_at TIMESTAMPTZ,

    -- VOD/Stream
    stream_url VARCHAR(512),
    vod_url VARCHAR(512),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_matches_unique_position UNIQUE (bracket_id, bracket_position),
    CONSTRAINT tournament_matches_check_status CHECK (status IN (
        'pending', 'ready', 'scheduled', 'checking_in', 'pick_ban',
        'in_progress', 'awaiting_result', 'completed', 'cancelled', 'forfeit', 'disputed'
    )),
    CONSTRAINT tournament_matches_check_format CHECK (match_format IN (
        'bo1', 'bo3', 'bo5', 'bo7'
    ))
);

CREATE INDEX idx_tournament_matches_bracket ON tournament_matches(bracket_id);
CREATE INDEX idx_tournament_matches_stage ON tournament_matches(stage_id);
CREATE INDEX idx_tournament_matches_tournament ON tournament_matches(tournament_id);
CREATE INDEX idx_tournament_matches_status ON tournament_matches(status);
CREATE INDEX idx_tournament_matches_scheduled ON tournament_matches(scheduled_at)
    WHERE scheduled_at IS NOT NULL;
CREATE INDEX idx_tournament_matches_participant1 ON tournament_matches(participant1_registration_id)
    WHERE participant1_registration_id IS NOT NULL;
CREATE INDEX idx_tournament_matches_participant2 ON tournament_matches(participant2_registration_id)
    WHERE participant2_registration_id IS NOT NULL;

CREATE TRIGGER tournament_matches_updated_at
    BEFORE UPDATE ON tournament_matches
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_matches IS 'Individual matches within tournament brackets';

-- =============================================================================
-- 6. TOURNAMENT MATCH GAMES
-- =============================================================================

CREATE TABLE tournament_match_games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Game number in series
    game_number INTEGER NOT NULL,

    -- Map selection
    map_id VARCHAR(64),
    map_picked_by UUID REFERENCES tournament_registrations(id),
    side_selection_by UUID REFERENCES tournament_registrations(id),

    -- Results
    participant1_score INTEGER,
    participant2_score INTEGER,
    winner_registration_id UUID REFERENCES tournament_registrations(id),

    -- Timing
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    duration_seconds INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Game-specific data
    game_data JSONB NOT NULL DEFAULT '{}',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_match_games_unique UNIQUE (match_id, game_number),
    CONSTRAINT tournament_match_games_check_status CHECK (status IN (
        'pending', 'map_veto', 'in_progress', 'completed', 'cancelled'
    )),
    CONSTRAINT tournament_match_games_check_number CHECK (game_number >= 1)
);

CREATE INDEX idx_tournament_match_games_match ON tournament_match_games(match_id);

CREATE TRIGGER tournament_match_games_updated_at
    BEFORE UPDATE ON tournament_match_games
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_match_games IS 'Individual games within a tournament match series';

-- =============================================================================
-- 7. TOURNAMENT MAP POOLS
-- =============================================================================

CREATE TABLE tournament_map_pools (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    stage_id UUID REFERENCES tournament_stages(id) ON DELETE CASCADE,

    -- Maps (array of map IDs from game plugin)
    maps TEXT[] NOT NULL,

    -- Veto format
    veto_format_id VARCHAR(64),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_map_pools_unique UNIQUE (tournament_id, stage_id)
);

CREATE INDEX idx_tournament_map_pools_tournament ON tournament_map_pools(tournament_id);

CREATE TRIGGER tournament_map_pools_updated_at
    BEFORE UPDATE ON tournament_map_pools
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_map_pools IS 'Map pool configurations for tournaments';

-- =============================================================================
-- 8. TOURNAMENT MAP VETO LOG
-- =============================================================================

CREATE TABLE tournament_map_veto_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    game_number INTEGER,  -- NULL for whole-match veto

    -- Action
    action_number INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,  -- 'ban', 'pick', 'decider'
    map_id VARCHAR(64) NOT NULL,
    performed_by UUID REFERENCES tournament_registrations(id),

    -- Timestamps
    performed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_map_veto_logs_unique UNIQUE (match_id, game_number, action_number),
    CONSTRAINT tournament_map_veto_logs_check_action CHECK (action_type IN (
        'ban', 'pick', 'decider'
    ))
);

CREATE INDEX idx_tournament_map_veto_logs_match ON tournament_map_veto_logs(match_id);

COMMENT ON TABLE tournament_map_veto_logs IS 'Log of map veto actions in matches';

-- =============================================================================
-- 9. TOURNAMENT STANDINGS (For round robin/swiss)
-- =============================================================================

CREATE TABLE tournament_standings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL REFERENCES tournament_brackets(id) ON DELETE CASCADE,
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- Position
    position INTEGER NOT NULL,

    -- Stats
    matches_played INTEGER NOT NULL DEFAULT 0,
    matches_won INTEGER NOT NULL DEFAULT 0,
    matches_lost INTEGER NOT NULL DEFAULT 0,
    matches_drawn INTEGER NOT NULL DEFAULT 0,

    -- Tiebreakers
    game_wins INTEGER NOT NULL DEFAULT 0,
    game_losses INTEGER NOT NULL DEFAULT 0,
    game_differential INTEGER NOT NULL DEFAULT 0,
    buchholz_score DECIMAL(10,2),  -- For Swiss
    opponent_match_wins DECIMAL(10,2),  -- OMW%

    -- Points (for round robin)
    points INTEGER NOT NULL DEFAULT 0,

    -- Timestamps
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_standings_unique UNIQUE (bracket_id, registration_id)
);

CREATE INDEX idx_tournament_standings_bracket ON tournament_standings(bracket_id);
CREATE INDEX idx_tournament_standings_position ON tournament_standings(bracket_id, position);

CREATE TRIGGER tournament_standings_updated_at
    BEFORE UPDATE ON tournament_standings
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_standings IS 'Current standings for round robin/swiss brackets';

-- =============================================================================
-- 10. PERMISSIONS
-- =============================================================================

INSERT INTO permissions (id, name, display_name, description, category)
VALUES
    (gen_random_uuid(), 'tournament.create', 'Create Tournaments', 'Create new tournaments', 'tournament'),
    (gen_random_uuid(), 'tournament.manage', 'Manage Tournaments', 'Full tournament administration', 'tournament'),
    (gen_random_uuid(), 'tournament.brackets.manage', 'Manage Brackets', 'Edit brackets and matches', 'tournament'),
    (gen_random_uuid(), 'tournament.registrations.manage', 'Manage Registrations', 'Approve/deny registrations', 'tournament'),
    (gen_random_uuid(), 'tournament.results.submit', 'Submit Results', 'Submit match results', 'tournament'),
    (gen_random_uuid(), 'tournament.disputes.resolve', 'Resolve Disputes', 'Handle match disputes', 'tournament')
ON CONFLICT (name) DO NOTHING;

-- =============================================================================
-- 11. MATERIALIZED VIEW FOR BRACKET DISPLAY
-- =============================================================================

-- Materialized view for efficient bracket rendering
CREATE MATERIALIZED VIEW mv_tournament_bracket_display AS
SELECT
    tm.id AS match_id,
    tm.bracket_id,
    tm.stage_id,
    tm.tournament_id,
    tm.round,
    tm.match_number,
    tm.bracket_position,
    tm.status,
    tm.scheduled_at,
    tm.participant1_name,
    tm.participant1_logo_url,
    tm.participant1_seed,
    tm.participant1_score,
    tm.participant2_name,
    tm.participant2_logo_url,
    tm.participant2_seed,
    tm.participant2_score,
    tm.winner_registration_id,
    tm.match_format,
    tb.bracket_type,
    tb.name AS bracket_name,
    ts.name AS stage_name,
    t.name AS tournament_name
FROM tournament_matches tm
JOIN tournament_brackets tb ON tb.id = tm.bracket_id
JOIN tournament_stages ts ON ts.id = tm.stage_id
JOIN tournaments t ON t.id = tm.tournament_id;

CREATE UNIQUE INDEX idx_mv_bracket_display_match ON mv_tournament_bracket_display(match_id);
CREATE INDEX idx_mv_bracket_display_bracket ON mv_tournament_bracket_display(bracket_id);
CREATE INDEX idx_mv_bracket_display_tournament ON mv_tournament_bracket_display(tournament_id);

COMMENT ON MATERIALIZED VIEW mv_tournament_bracket_display IS
    'Pre-computed bracket display data for efficient client rendering';

-- Function to refresh the materialized view (call from application when needed)
CREATE OR REPLACE FUNCTION refresh_bracket_display()
RETURNS VOID AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_tournament_bracket_display;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION refresh_bracket_display IS 'Refresh bracket display materialized view';
