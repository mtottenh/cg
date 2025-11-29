-- Migration: Create team_invitations table
-- Description: Team invitation and join request management

CREATE TABLE team_invitations (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,

    -- Invitation Details
    type VARCHAR(20) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    message TEXT,

    -- Sender
    invited_by UUID REFERENCES players(id) ON DELETE SET NULL,

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',

    -- Response
    responded_at TIMESTAMPTZ,
    response_message TEXT,

    -- Expiration
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT team_invitations_check_type CHECK (type IN ('invite', 'request')),
    CONSTRAINT team_invitations_check_status CHECK (status IN (
        'pending', 'accepted', 'declined', 'expired', 'cancelled'
    ))
);

-- Indexes
CREATE INDEX idx_team_invitations_team ON team_invitations(team_id);
CREATE INDEX idx_team_invitations_player ON team_invitations(player_id);
CREATE INDEX idx_team_invitations_pending ON team_invitations(player_id, status)
    WHERE status = 'pending';
CREATE INDEX idx_team_invitations_expires ON team_invitations(expires_at)
    WHERE status = 'pending';
