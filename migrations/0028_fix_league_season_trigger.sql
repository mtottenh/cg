-- Migration: Fix league season auto-creation trigger
-- Description: The BEFORE INSERT trigger cannot work because it tries to insert
-- into league_seasons before the league row exists. Convert to AFTER INSERT.

-- Drop the broken trigger
DROP TRIGGER IF EXISTS trg_leagues_create_default_season ON leagues;
DROP FUNCTION IF EXISTS create_default_league_season();

-- Create a proper AFTER INSERT trigger function
CREATE OR REPLACE FUNCTION create_default_league_season()
RETURNS TRIGGER AS $$
DECLARE
    new_season_id UUID;
BEGIN
    -- Create default "Season 1" in registration status
    INSERT INTO league_seasons (
        id,
        league_id,
        name,
        slug,
        status,
        roster_lock_status,
        team_size_min,
        team_size_max,
        max_substitutes,
        created_by
    ) VALUES (
        gen_random_uuid(),
        NEW.id,
        'Season 1',
        'season-1',
        'registration',
        'open',
        NEW.default_team_size_min,
        NEW.default_team_size_max,
        NEW.default_max_substitutes,
        NEW.created_by
    )
    RETURNING id INTO new_season_id;

    -- Update the league with the current_season_id
    UPDATE leagues
    SET current_season_id = new_season_id
    WHERE id = NEW.id;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create as AFTER INSERT trigger (league row now exists for FK)
CREATE TRIGGER trg_leagues_create_default_season
    AFTER INSERT ON leagues
    FOR EACH ROW
    EXECUTE FUNCTION create_default_league_season();

COMMENT ON FUNCTION create_default_league_season IS 'Auto-creates Season 1 when a new league is created';
