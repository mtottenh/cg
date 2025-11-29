-- Migration: Create team_members table
-- Description: Team roster membership

CREATE TABLE team_members (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,

    -- Role (determines permissions within the team)
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    role_title VARCHAR(64),

    -- Founder Flag (only true for the original team creator)
    is_founder BOOLEAN NOT NULL DEFAULT FALSE,

    -- Position (game-specific, plugin-defined)
    primary_position VARCHAR(32),
    secondary_position VARCHAR(32),

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',

    -- Jersey/Number
    jersey_number INTEGER,

    -- Invited By
    invited_by UUID REFERENCES players(id),

    -- Timestamps
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT team_members_unique UNIQUE (team_id, player_id),
    CONSTRAINT team_members_check_role CHECK (role IN (
        'captain', 'officer', 'player', 'substitute', 'coach', 'manager'
    )),
    CONSTRAINT team_members_check_status CHECK (status IN (
        'active', 'inactive', 'benched', 'trial'
    )),
    CONSTRAINT team_members_check_jersey CHECK (
        jersey_number IS NULL OR (jersey_number >= 0 AND jersey_number <= 99)
    )
);

-- Indexes
CREATE INDEX idx_team_members_team ON team_members(team_id);
CREATE INDEX idx_team_members_player ON team_members(player_id);
CREATE INDEX idx_team_members_active ON team_members(team_id, status)
    WHERE status = 'active' AND left_at IS NULL;
CREATE INDEX idx_team_members_role ON team_members(team_id, role);
CREATE INDEX idx_team_members_captains ON team_members(team_id)
    WHERE role = 'captain' AND status = 'active';
