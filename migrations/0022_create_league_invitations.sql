-- Migration: Create league_invitations table
-- Description: Handles both outgoing invitations and incoming applications

CREATE TABLE league_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    invitation_type VARCHAR(32) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    message TEXT,
    invited_by UUID REFERENCES users(id),
    responded_by UUID REFERENCES users(id),
    responded_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT league_invitations_check_type CHECK (invitation_type IN ('invite', 'application')),
    CONSTRAINT league_invitations_check_status CHECK (status IN ('pending', 'accepted', 'rejected', 'expired'))
);

-- Indexes for common queries
CREATE INDEX idx_league_invitations_league_id ON league_invitations(league_id);
CREATE INDEX idx_league_invitations_user_id ON league_invitations(user_id);
CREATE INDEX idx_league_invitations_status ON league_invitations(status);
CREATE INDEX idx_league_invitations_type_status ON league_invitations(invitation_type, status);

COMMENT ON TABLE league_invitations IS 'League invitations (from admin) and applications (from users)';
COMMENT ON COLUMN league_invitations.invitation_type IS 'invite=admin invites user, application=user applies to join';
COMMENT ON COLUMN league_invitations.invited_by IS 'For invite type: admin who sent the invitation';
COMMENT ON COLUMN league_invitations.responded_by IS 'For application type: admin who approved/rejected';
