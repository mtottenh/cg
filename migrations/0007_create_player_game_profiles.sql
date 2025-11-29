-- Migration: Create player_game_profiles table
-- Description: Per-game statistics and rankings for each player

CREATE TABLE player_game_profiles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,

    -- Rating System (Glicko-2) - Platform managed
    rating INTEGER NOT NULL DEFAULT 1500,
    rating_deviation INTEGER NOT NULL DEFAULT 350,
    volatility DOUBLE PRECISION NOT NULL DEFAULT 0.06,
    peak_rating INTEGER NOT NULL DEFAULT 1500,
    peak_rating_at TIMESTAMPTZ,

    -- Rank Display (plugin defines tiers, platform calculates placement)
    rank_tier VARCHAR(32),
    rank_division INTEGER,
    rank_points INTEGER DEFAULT 0,
    rank_updated_at TIMESTAMPTZ,

    -- Match Statistics - Platform managed
    matches_played INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    win_streak INTEGER NOT NULL DEFAULT 0,
    best_win_streak INTEGER NOT NULL DEFAULT 0,

    -- Time Statistics
    total_playtime_minutes INTEGER NOT NULL DEFAULT 0,
    avg_match_duration_minutes INTEGER,

    -- Game-Specific Stats (defined and populated by game plugin)
    game_specific_stats JSONB DEFAULT '{}',

    -- Achievements & Badges (plugin-defined)
    achievements JSONB DEFAULT '[]',
    equipped_badge_id VARCHAR(64),

    -- Timestamps
    first_match_at TIMESTAMPTZ,
    last_match_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT player_game_profiles_unique UNIQUE (player_id, game_id),
    CONSTRAINT player_game_profiles_check_rating CHECK (rating >= 0 AND rating <= 5000),
    CONSTRAINT player_game_profiles_check_rd CHECK (rating_deviation >= 0),
    CONSTRAINT player_game_profiles_check_wins CHECK (wins >= 0),
    CONSTRAINT player_game_profiles_check_losses CHECK (losses >= 0)
);

-- Indexes
CREATE INDEX idx_player_game_profiles_player ON player_game_profiles(player_id);
CREATE INDEX idx_player_game_profiles_game ON player_game_profiles(game_id);
CREATE INDEX idx_player_game_profiles_rating ON player_game_profiles(game_id, rating DESC);
CREATE INDEX idx_player_game_profiles_matches ON player_game_profiles(game_id, matches_played DESC);

-- Triggers
CREATE TRIGGER player_game_profiles_updated_at
    BEFORE UPDATE ON player_game_profiles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
