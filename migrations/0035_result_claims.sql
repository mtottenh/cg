-- Migration: Create result claims for match result submission workflow
-- Part of Phase 3, Sub-Phase 3.6: Result Submission

-- Result claims track match result submissions awaiting confirmation
CREATE TABLE result_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who submitted the claim
    submitted_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    submitted_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Claimed result
    claimed_winner_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    claimed_participant1_score INTEGER NOT NULL,
    claimed_participant2_score INTEGER NOT NULL,

    -- Game-by-game results (for series matches)
    game_results JSONB NOT NULL DEFAULT '[]',

    -- Status: pending, confirmed, disputed, superseded, cancelled
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Confirmation tracking
    confirmed_at TIMESTAMPTZ,
    confirmed_by_registration_id UUID REFERENCES tournament_registrations(id),
    confirmed_by_user_id UUID REFERENCES users(id),

    -- Auto-confirmation
    auto_confirm_at TIMESTAMPTZ,
    was_auto_confirmed BOOLEAN NOT NULL DEFAULT false,

    -- Evidence links (demo files, screenshots, etc.)
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- Notes
    submitter_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT result_claims_check_status CHECK (status IN (
        'pending', 'confirmed', 'disputed', 'superseded', 'cancelled'
    )),
    CONSTRAINT result_claims_scores_non_negative CHECK (
        claimed_participant1_score >= 0 AND claimed_participant2_score >= 0
    )
);

-- Indexes for efficient queries
CREATE INDEX idx_result_claims_match ON result_claims(match_id);
CREATE INDEX idx_result_claims_status ON result_claims(status);
CREATE INDEX idx_result_claims_auto_confirm ON result_claims(auto_confirm_at)
    WHERE status = 'pending';
CREATE INDEX idx_result_claims_submitted_by ON result_claims(submitted_by_registration_id);

-- Trigger to update updated_at timestamp
CREATE TRIGGER result_claims_updated_at
    BEFORE UPDATE ON result_claims
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Comments
COMMENT ON TABLE result_claims IS 'Match result claims awaiting confirmation';
COMMENT ON COLUMN result_claims.game_results IS 'JSON array of game-by-game results for series matches';
COMMENT ON COLUMN result_claims.auto_confirm_at IS 'When auto-confirmation will trigger if no response';
COMMENT ON COLUMN result_claims.evidence_ids IS 'Array of evidence file IDs linked to this claim';
COMMENT ON COLUMN result_claims.status IS 'pending: awaiting response, confirmed: accepted, disputed: contested, superseded: replaced by newer claim, cancelled: withdrawn';
