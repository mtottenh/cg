-- Migration: Forfeit Records
-- Description: Track forfeits due to no-show, withdrawal, disqualification, or technical default

CREATE TABLE forfeit_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who forfeited
    forfeiting_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),

    -- Type and reason
    forfeit_type VARCHAR(32) NOT NULL,
    reason TEXT,

    -- Triggered by
    triggered_by_user_id UUID REFERENCES users(id),
    triggered_by_system BOOLEAN NOT NULL DEFAULT false,

    -- Timestamps
    forfeited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT forfeit_records_check_type CHECK (forfeit_type IN (
        'no_show', 'withdrawal', 'disqualification', 'technical_default'
    ))
);

CREATE INDEX idx_forfeit_records_match ON forfeit_records(match_id);
CREATE INDEX idx_forfeit_records_registration ON forfeit_records(forfeiting_registration_id);

COMMENT ON TABLE forfeit_records IS 'Record of match forfeits';
COMMENT ON COLUMN forfeit_records.forfeit_type IS 'Type: no_show, withdrawal, disqualification, technical_default';
COMMENT ON COLUMN forfeit_records.triggered_by_system IS 'True if triggered automatically (e.g., check-in timeout)';
