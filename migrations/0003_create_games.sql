-- Migration: Create games table
-- Description: Game definitions with plugin configuration

CREATE TABLE games (
    -- Primary Key (slug-based)
    id VARCHAR(32) PRIMARY KEY,

    -- Display Information
    display_name VARCHAR(64) NOT NULL,
    short_name VARCHAR(16),
    description TEXT,

    -- Media
    icon_url VARCHAR(512),
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),

    -- Configuration
    config JSONB DEFAULT '{}',
    default_queue_config JSONB DEFAULT '{}',
    default_lobby_config JSONB DEFAULT '{}',

    -- Plugin Reference
    plugin_id VARCHAR(64) NOT NULL,
    plugin_version VARCHAR(32) NOT NULL DEFAULT '1.0.0',

    -- Team Configuration
    team_size_min INTEGER NOT NULL DEFAULT 1,
    team_size_max INTEGER NOT NULL DEFAULT 5,
    team_size_default INTEGER NOT NULL DEFAULT 5,

    -- Maps
    available_maps JSONB DEFAULT '[]',
    default_map_pool JSONB DEFAULT '[]',

    -- Ranking Configuration
    rank_tiers JSONB DEFAULT '[]',

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    is_featured BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT games_check_status CHECK (status IN ('active', 'inactive', 'beta', 'deprecated', 'maintenance')),
    CONSTRAINT games_check_team_size CHECK (
        team_size_min > 0 AND
        team_size_min <= team_size_default AND
        team_size_default <= team_size_max
    )
);

-- Indexes
CREATE INDEX idx_games_status ON games(status) WHERE status = 'active';
CREATE INDEX idx_games_featured ON games(is_featured, sort_order) WHERE is_featured = TRUE;

-- Triggers
CREATE TRIGGER games_updated_at
    BEFORE UPDATE ON games
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert default games (CS2 and AoE4)
INSERT INTO games (id, display_name, short_name, plugin_id, team_size_min, team_size_max, team_size_default, is_featured, sort_order) VALUES
('cs2', 'Counter-Strike 2', 'CS2', 'cs2', 5, 5, 5, TRUE, 1),
('aoe4', 'Age of Empires IV', 'AoE4', 'aoe4', 1, 4, 1, TRUE, 2);
