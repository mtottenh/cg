-- Migration: Create players table
-- Description: Gaming identity linked to a user account

CREATE TABLE players (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,

    -- Profile Information
    display_name VARCHAR(32) NOT NULL,
    display_name_normalized VARCHAR(32) GENERATED ALWAYS AS (lower(display_name)) STORED,
    avatar_url VARCHAR(512),
    banner_url VARCHAR(512),
    bio TEXT,

    -- Location
    country_code CHAR(2),
    region VARCHAR(64),
    timezone VARCHAR(64),

    -- Social Links
    social_links JSONB DEFAULT '{}',

    -- Privacy Settings
    privacy_settings JSONB DEFAULT '{
        "show_online_status": true,
        "show_match_history": true,
        "show_statistics": true,
        "allow_friend_requests": true,
        "allow_team_invites": true
    }',

    -- Platform Settings
    notification_settings JSONB DEFAULT '{}',
    ui_preferences JSONB DEFAULT '{}',

    -- Steam Integration
    steam_id VARCHAR(32),
    steam_id_64 BIGINT,
    steam_profile JSONB,

    -- Metadata
    featured_badge_id UUID,
    title VARCHAR(64),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT players_check_country_code CHECK (
        country_code IS NULL OR country_code ~ '^[A-Z]{2}$'
    ),
    CONSTRAINT players_steam_id_unique UNIQUE (steam_id),
    CONSTRAINT players_steam_id_64_unique UNIQUE (steam_id_64)
);

-- Indexes
CREATE INDEX idx_players_user_id ON players(user_id);
CREATE INDEX idx_players_display_name ON players(display_name_normalized);
CREATE INDEX idx_players_steam_id ON players(steam_id) WHERE steam_id IS NOT NULL;
CREATE INDEX idx_players_steam_id_64 ON players(steam_id_64) WHERE steam_id_64 IS NOT NULL;
CREATE INDEX idx_players_country ON players(country_code) WHERE country_code IS NOT NULL;

-- Full-text search index
CREATE INDEX idx_players_search ON players USING gin(
    to_tsvector('english', display_name || ' ' || COALESCE(bio, ''))
);

-- Triggers
CREATE TRIGGER players_updated_at
    BEFORE UPDATE ON players
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
