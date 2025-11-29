-- Migration: Create teams table
-- Description: Team organizations

CREATE TABLE teams (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Identity
    name VARCHAR(64) NOT NULL,
    name_normalized VARCHAR(64) GENERATED ALWAYS AS (lower(name)) STORED,
    tag VARCHAR(5) NOT NULL,
    tag_normalized VARCHAR(5) GENERATED ALWAYS AS (lower(tag)) STORED,

    -- Profile
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    primary_color VARCHAR(7),
    secondary_color VARCHAR(7),

    -- Founding Captain
    created_by UUID NOT NULL REFERENCES players(id) ON DELETE RESTRICT,

    -- Game Association (NULL for multi-game teams)
    game_id VARCHAR(32) REFERENCES games(id) ON DELETE SET NULL,

    -- Settings
    settings JSONB DEFAULT '{
        "require_approval": true,
        "public_roster": true,
        "allow_scrim_requests": true,
        "max_roster_size": 10
    }',

    -- Social
    social_links JSONB DEFAULT '{}',
    website_url VARCHAR(512),

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    disbanded_at TIMESTAMPTZ,
    disbanded_reason TEXT,

    -- Statistics
    total_matches INTEGER NOT NULL DEFAULT 0,
    total_wins INTEGER NOT NULL DEFAULT 0,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT teams_name_unique UNIQUE (name_normalized),
    CONSTRAINT teams_tag_format CHECK (tag ~ '^[a-zA-Z0-9]{2,5}$'),
    CONSTRAINT teams_check_status CHECK (status IN (
        'active', 'inactive', 'disbanded', 'suspended'
    )),
    CONSTRAINT teams_check_colors CHECK (
        (primary_color IS NULL OR primary_color ~ '^#[0-9A-Fa-f]{6}$') AND
        (secondary_color IS NULL OR secondary_color ~ '^#[0-9A-Fa-f]{6}$')
    )
);

-- Indexes
CREATE INDEX idx_teams_created_by ON teams(created_by);
CREATE INDEX idx_teams_game ON teams(game_id) WHERE game_id IS NOT NULL;
CREATE INDEX idx_teams_status ON teams(status) WHERE status = 'active';
CREATE INDEX idx_teams_name ON teams(name_normalized);
CREATE INDEX idx_teams_tag ON teams(tag_normalized);

-- Full-text search
CREATE INDEX idx_teams_search ON teams USING gin(
    to_tsvector('english', name || ' ' || tag || ' ' || COALESCE(description, ''))
);

-- Triggers
CREATE TRIGGER teams_updated_at
    BEFORE UPDATE ON teams
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
