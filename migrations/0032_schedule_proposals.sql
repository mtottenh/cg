-- Migration: 0032_schedule_proposals.sql
-- Description: Schedule proposal system for match time negotiation
-- Phase: 3.2 - Match Scheduling

-- ============================================================================
-- SCHEDULE PROPOSALS TABLE
-- ============================================================================

CREATE TABLE schedule_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who proposed
    proposed_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    proposed_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Proposed time slots (array of up to 5 options)
    proposed_times TIMESTAMPTZ[] NOT NULL,

    -- Selected time (set when accepted)
    selected_time TIMESTAMPTZ,

    -- Response tracking
    responded_at TIMESTAMPTZ,
    responded_by_user_id UUID REFERENCES users(id),

    -- Counter-proposal reference
    counter_proposal_id UUID REFERENCES schedule_proposals(id),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Expiration
    expires_at TIMESTAMPTZ NOT NULL,

    -- Admin notes
    notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT schedule_proposals_check_status CHECK (status IN (
        'pending', 'accepted', 'rejected', 'counter_proposed', 'expired', 'cancelled'
    )),
    CONSTRAINT schedule_proposals_check_times CHECK (
        array_length(proposed_times, 1) >= 1 AND
        array_length(proposed_times, 1) <= 5
    )
);

-- Indexes
CREATE INDEX idx_schedule_proposals_match ON schedule_proposals(match_id);
CREATE INDEX idx_schedule_proposals_status ON schedule_proposals(status);
CREATE INDEX idx_schedule_proposals_expires ON schedule_proposals(expires_at)
    WHERE status = 'pending';
CREATE INDEX idx_schedule_proposals_proposed_by ON schedule_proposals(proposed_by_registration_id);

-- Updated at trigger
CREATE TRIGGER schedule_proposals_updated_at
    BEFORE UPDATE ON schedule_proposals
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE schedule_proposals IS 'Match scheduling proposals between teams for negotiating match times';
COMMENT ON COLUMN schedule_proposals.proposed_times IS 'Array of 1-5 proposed time slots for the match';
COMMENT ON COLUMN schedule_proposals.selected_time IS 'The time selected when proposal is accepted';
COMMENT ON COLUMN schedule_proposals.counter_proposal_id IS 'Reference to counter-proposal if this was counter-proposed';
COMMENT ON COLUMN schedule_proposals.expires_at IS 'When this proposal expires if not responded to';
