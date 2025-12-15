-- Migration: Match Lifecycle
-- Description: Add check-in and lifecycle fields to tournament_matches, create status log table

-- =============================================================================
-- 1. ADD CHECK-IN AND LIFECYCLE FIELDS TO TOURNAMENT MATCHES
-- =============================================================================

-- Check-in window timing
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_opens_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_deadline TIMESTAMPTZ;

-- Participant check-in tracking
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant1_checked_in_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant2_checked_in_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant1_checked_in_by UUID REFERENCES users(id);
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant2_checked_in_by UUID REFERENCES users(id);

-- Match requirements
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS veto_required BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_required BOOLEAN NOT NULL DEFAULT false;

-- Index for finding matches needing check-in window opened
CREATE INDEX IF NOT EXISTS idx_tournament_matches_check_in_opens
    ON tournament_matches(check_in_opens_at)
    WHERE status = 'scheduled' AND check_in_opens_at IS NOT NULL;

-- Index for finding matches with expired check-in deadlines
CREATE INDEX IF NOT EXISTS idx_tournament_matches_check_in_deadline
    ON tournament_matches(check_in_deadline)
    WHERE status = 'checking_in' AND check_in_deadline IS NOT NULL;

-- =============================================================================
-- 2. MATCH STATUS LOG TABLE
-- =============================================================================

CREATE TABLE match_status_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Transition details
    from_status VARCHAR(32) NOT NULL,
    to_status VARCHAR(32) NOT NULL,
    transition_reason VARCHAR(256),

    -- Who triggered the transition
    triggered_by_user_id UUID REFERENCES users(id),
    triggered_by_system BOOLEAN NOT NULL DEFAULT false,

    -- Additional context (job name for system triggers, override reason for admin, etc.)
    metadata JSONB NOT NULL DEFAULT '{}',

    -- Timestamp
    transitioned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT match_status_log_check_trigger CHECK (
        (triggered_by_user_id IS NOT NULL AND NOT triggered_by_system) OR
        (triggered_by_user_id IS NULL AND triggered_by_system) OR
        (triggered_by_user_id IS NOT NULL AND triggered_by_system) -- Admin override
    )
);

CREATE INDEX idx_match_status_log_match ON match_status_log(match_id);
CREATE INDEX idx_match_status_log_time ON match_status_log(transitioned_at);

COMMENT ON TABLE match_status_log IS 'Audit log of match status transitions';
COMMENT ON COLUMN match_status_log.from_status IS 'Previous match status before transition';
COMMENT ON COLUMN match_status_log.to_status IS 'New match status after transition';
COMMENT ON COLUMN match_status_log.transition_reason IS 'Human-readable reason for the transition';
COMMENT ON COLUMN match_status_log.triggered_by_user_id IS 'User who triggered the transition (null for system)';
COMMENT ON COLUMN match_status_log.triggered_by_system IS 'Whether the transition was triggered by a background job';
COMMENT ON COLUMN match_status_log.metadata IS 'Additional context (job name, override reason, etc.)';
