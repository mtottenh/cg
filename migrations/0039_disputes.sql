-- Migration: Disputes System
-- Description: Match result disputes and admin resolution workflow

CREATE TABLE disputes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    result_claim_id UUID REFERENCES result_claims(id) ON DELETE SET NULL,

    -- Who disputed
    disputed_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    disputed_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Dispute details
    reason VARCHAR(64) NOT NULL,
    description TEXT NOT NULL,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- What was claimed vs disputed
    original_winner_registration_id UUID REFERENCES tournament_registrations(id),
    original_participant1_score INTEGER,
    original_participant2_score INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    priority VARCHAR(16) NOT NULL DEFAULT 'normal',

    -- Resolution
    resolved_at TIMESTAMPTZ,
    resolved_by_user_id UUID REFERENCES users(id),
    resolution_type VARCHAR(32),
    resolution_notes TEXT,

    -- For overturned results
    new_winner_registration_id UUID REFERENCES tournament_registrations(id),
    new_participant1_score INTEGER,
    new_participant2_score INTEGER,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT disputes_check_status CHECK (status IN (
        'pending', 'under_review', 'resolved', 'cancelled'
    )),
    CONSTRAINT disputes_check_reason CHECK (reason IN (
        'wrong_score', 'wrong_winner', 'cheating', 'rule_violation',
        'technical_issue', 'player_misconduct', 'other'
    )),
    CONSTRAINT disputes_check_resolution CHECK (
        status != 'resolved' OR resolution_type IS NOT NULL
    ),
    CONSTRAINT disputes_check_resolution_type CHECK (
        resolution_type IS NULL OR resolution_type IN (
            'upheld', 'overturned', 'rematch', 'adjusted', 'double_dq'
        )
    ),
    CONSTRAINT disputes_check_priority CHECK (priority IN (
        'low', 'normal', 'high', 'urgent'
    ))
);

CREATE INDEX idx_disputes_match ON disputes(match_id);
CREATE INDEX idx_disputes_status ON disputes(status);
CREATE INDEX idx_disputes_priority ON disputes(priority, created_at)
    WHERE status IN ('pending', 'under_review');

CREATE TRIGGER disputes_updated_at
    BEFORE UPDATE ON disputes
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE disputes IS 'Match result disputes for admin resolution';
COMMENT ON COLUMN disputes.reason IS 'Reason: wrong_score, wrong_winner, cheating, rule_violation, technical_issue, player_misconduct, other';
COMMENT ON COLUMN disputes.status IS 'Status: pending, under_review, resolved, cancelled';
COMMENT ON COLUMN disputes.priority IS 'Priority: low, normal, high, urgent';
COMMENT ON COLUMN disputes.resolution_type IS 'Resolution: upheld, overturned, rematch, adjusted, double_dq';

-- Dispute messages for communication thread
CREATE TABLE dispute_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dispute_id UUID NOT NULL REFERENCES disputes(id) ON DELETE CASCADE,

    -- Author
    author_user_id UUID NOT NULL REFERENCES users(id),
    author_type VARCHAR(16) NOT NULL,  -- 'participant', 'admin', 'system'

    -- Content
    message TEXT NOT NULL,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- Visibility
    is_internal BOOLEAN NOT NULL DEFAULT false,  -- Admin-only notes

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT dispute_messages_check_author_type CHECK (
        author_type IN ('participant', 'admin', 'system')
    )
);

CREATE INDEX idx_dispute_messages_dispute ON dispute_messages(dispute_id);

COMMENT ON TABLE dispute_messages IS 'Communication thread for dispute resolution';
COMMENT ON COLUMN dispute_messages.author_type IS 'Author type: participant, admin, system';
COMMENT ON COLUMN dispute_messages.is_internal IS 'If true, only visible to admins';
