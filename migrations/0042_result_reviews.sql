-- Result Review System
-- Tracks validation issues requiring human review before match completion

CREATE TYPE result_review_status AS ENUM (
    'pending_acknowledgment',
    'pending_admin_review',
    'acknowledged',
    'approved',
    'rejected'
);

CREATE TABLE result_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    result_claim_id UUID NOT NULL REFERENCES result_claims(id) ON DELETE CASCADE,
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Review triggers
    roster_mismatch BOOLEAN NOT NULL DEFAULT false,
    score_mismatch BOOLEAN NOT NULL DEFAULT false,
    winner_mismatch BOOLEAN NOT NULL DEFAULT false,

    -- Demo validation details
    demo_link_id UUID REFERENCES demo_match_links(id),
    validation_result JSONB,
    unrecognized_players JSONB NOT NULL DEFAULT '[]',

    -- Status
    status result_review_status NOT NULL,

    -- Captain 1 acknowledgment
    captain1_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    captain1_acknowledged BOOLEAN NOT NULL DEFAULT false,
    captain1_acknowledged_at TIMESTAMPTZ,
    captain1_acknowledged_by_user_id UUID REFERENCES users(id),

    -- Captain 2 acknowledgment
    captain2_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    captain2_acknowledged BOOLEAN NOT NULL DEFAULT false,
    captain2_acknowledged_at TIMESTAMPTZ,
    captain2_acknowledged_by_user_id UUID REFERENCES users(id),

    -- Admin resolution
    reviewed_by_user_id UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    admin_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_captain_acknowledgment CHECK (
        (NOT captain1_acknowledged OR captain1_acknowledged_at IS NOT NULL)
        AND (NOT captain2_acknowledged OR captain2_acknowledged_at IS NOT NULL)
    ),
    CONSTRAINT valid_admin_review CHECK (
        (status NOT IN ('approved', 'rejected')) OR reviewed_by_user_id IS NOT NULL
    )
);

-- Indexes
CREATE INDEX idx_result_reviews_match ON result_reviews(match_id);
CREATE INDEX idx_result_reviews_claim ON result_reviews(result_claim_id);
CREATE INDEX idx_result_reviews_status ON result_reviews(status);

-- Partial index for pending reviews (admin queue)
CREATE INDEX idx_result_reviews_pending_admin ON result_reviews(created_at)
    WHERE status = 'pending_admin_review';

-- Partial index for pending acknowledgments
CREATE INDEX idx_result_reviews_pending_ack ON result_reviews(created_at)
    WHERE status = 'pending_acknowledgment';

-- Trigger for updated_at
CREATE TRIGGER set_result_reviews_updated_at
    BEFORE UPDATE ON result_reviews
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE result_reviews IS
    'Tracks validation issues requiring human review before match completion';
COMMENT ON COLUMN result_reviews.roster_mismatch IS
    'Demo contains players not on either team registered roster';
COMMENT ON COLUMN result_reviews.score_mismatch IS
    'Demo final score differs from claimed score';
COMMENT ON COLUMN result_reviews.winner_mismatch IS
    'Demo winner differs from claimed winner';
