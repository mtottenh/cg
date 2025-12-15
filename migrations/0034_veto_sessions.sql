-- Migration: Create veto sessions for map pick/ban system
-- Part of Phase 3, Sub-Phase 3.4: Pick-Ban Core

-- Veto sessions track map pick/ban state for tournament matches
CREATE TABLE veto_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Format configuration
    veto_format_id VARCHAR(64) NOT NULL,
    map_pool TEXT[] NOT NULL,

    -- Coin flip / first action determination
    first_action_registration_id UUID REFERENCES tournament_registrations(id),
    coin_flip_winner_registration_id UUID REFERENCES tournament_registrations(id),

    -- Current state
    current_action_number INTEGER NOT NULL DEFAULT 0,
    current_team_turn UUID REFERENCES tournament_registrations(id),

    -- Remaining maps (updated as veto progresses)
    remaining_maps TEXT[] NOT NULL,

    -- Selected maps (in play order)
    selected_maps TEXT[] NOT NULL DEFAULT '{}',

    -- Status: pending, coin_flip, in_progress, completed, cancelled
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timing
    action_deadline TIMESTAMPTZ,
    timeout_seconds INTEGER NOT NULL DEFAULT 30,

    -- Timestamps
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT veto_sessions_unique_match UNIQUE (match_id),
    CONSTRAINT veto_sessions_check_status CHECK (status IN (
        'pending', 'coin_flip', 'in_progress', 'completed', 'cancelled'
    ))
);

-- Veto actions record individual ban/pick/decider actions
CREATE TABLE veto_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES veto_sessions(id) ON DELETE CASCADE,

    -- Action details
    action_number INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,
    map_id VARCHAR(64) NOT NULL,

    -- Who performed the action
    performed_by_registration_id UUID REFERENCES tournament_registrations(id),
    performed_by_user_id UUID REFERENCES users(id),

    -- Side selection (for picks)
    side_selection VARCHAR(16),
    side_selected_by_registration_id UUID REFERENCES tournament_registrations(id),

    -- Auto-action (timeout)
    was_auto_action BOOLEAN NOT NULL DEFAULT false,
    auto_action_reason VARCHAR(64),

    -- Timestamps
    performed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    side_selected_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT veto_actions_unique UNIQUE (session_id, action_number),
    CONSTRAINT veto_actions_check_type CHECK (action_type IN ('ban', 'pick', 'decider')),
    CONSTRAINT veto_actions_check_number CHECK (action_number >= 1)
);

-- Indexes for efficient queries
CREATE INDEX idx_veto_sessions_match ON veto_sessions(match_id);
CREATE INDEX idx_veto_sessions_status ON veto_sessions(status);
CREATE INDEX idx_veto_sessions_deadline ON veto_sessions(action_deadline)
    WHERE status = 'in_progress';
CREATE INDEX idx_veto_actions_session ON veto_actions(session_id);

-- Trigger to update updated_at timestamp
CREATE TRIGGER veto_sessions_updated_at
    BEFORE UPDATE ON veto_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Comments
COMMENT ON TABLE veto_sessions IS 'Map veto sessions for tournament matches';
COMMENT ON TABLE veto_actions IS 'Individual actions in a map veto session';
COMMENT ON COLUMN veto_sessions.veto_format_id IS 'Veto format identifier from game plugin (e.g., bo1_veto, bo3_veto)';
COMMENT ON COLUMN veto_sessions.map_pool IS 'Starting map pool for this veto session';
COMMENT ON COLUMN veto_sessions.remaining_maps IS 'Maps still available for selection';
COMMENT ON COLUMN veto_sessions.selected_maps IS 'Maps selected for play, in order';
COMMENT ON COLUMN veto_actions.action_type IS 'Type of action: ban, pick, or decider';
COMMENT ON COLUMN veto_actions.was_auto_action IS 'Whether this action was performed automatically due to timeout';
