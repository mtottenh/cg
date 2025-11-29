-- Migration: Create league_members table
-- Description: Tracks membership in leagues with role hierarchy

CREATE TABLE league_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    membership_type VARCHAR(32) NOT NULL DEFAULT 'member',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT league_members_unique UNIQUE (league_id, user_id),
    CONSTRAINT league_members_check_type CHECK (membership_type IN ('admin', 'moderator', 'member'))
);

-- Indexes for common queries
CREATE INDEX idx_league_members_league_id ON league_members(league_id);
CREATE INDEX idx_league_members_user_id ON league_members(user_id);
CREATE INDEX idx_league_members_type ON league_members(membership_type);

COMMENT ON TABLE league_members IS 'League membership with role hierarchy: admin > moderator > member';
COMMENT ON COLUMN league_members.membership_type IS 'admin=full control, moderator=manage members, member=participant';
