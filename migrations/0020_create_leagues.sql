-- Migration: Create leagues table
-- Description: Leagues are game-specific organizations that can host tournaments

CREATE TABLE leagues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id VARCHAR(64) NOT NULL REFERENCES games(id),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    logo_url VARCHAR(500),
    access_type VARCHAR(32) NOT NULL DEFAULT 'open',
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    settings JSONB NOT NULL DEFAULT '{}',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT leagues_check_access_type CHECK (access_type IN ('open', 'invite_only', 'application')),
    CONSTRAINT leagues_check_status CHECK (status IN ('active', 'archived', 'suspended'))
);

-- Indexes for common queries
CREATE INDEX idx_leagues_game_id ON leagues(game_id);
CREATE INDEX idx_leagues_status ON leagues(status);
CREATE INDEX idx_leagues_created_by ON leagues(created_by);

-- Trigger to update updated_at timestamp
CREATE TRIGGER leagues_updated_at
    BEFORE UPDATE ON leagues
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE leagues IS 'Game-specific leagues that organize tournaments and competitions';
COMMENT ON COLUMN leagues.access_type IS 'open=anyone can join, invite_only=must be invited, application=requires admin approval';
COMMENT ON COLUMN leagues.status IS 'active=operational, archived=read-only, suspended=temporarily disabled';
COMMENT ON COLUMN leagues.settings IS 'League-specific settings like default map pool, rules, etc.';
