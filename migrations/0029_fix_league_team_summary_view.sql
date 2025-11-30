-- Migration: Fix league team summary view
-- Description: Recreate v_league_team_summary view with updated structure to match
-- the new team/team_season model from migration 0026.

-- Drop existing view if it exists
DROP VIEW IF EXISTS v_league_team_summary CASCADE;

-- Create the updated view
-- This view provides a summary of teams registered in a season with member counts
CREATE OR REPLACE VIEW v_league_team_summary AS
SELECT
    -- Team info (persistent identity)
    t.id AS team_id,
    t.league_id,
    t.name AS team_name,
    t.tag AS team_tag,
    t.logo_url AS team_logo_url,
    t.owner_player_id,
    t.status AS team_status,

    -- Season participation info (from team_season)
    ts.id AS team_season_id,
    ts.season_id,
    ts.status AS season_status,

    -- Member counts (for this season)
    COUNT(DISTINCT m.id) FILTER (WHERE m.status = 'active') AS active_member_count,
    COUNT(DISTINCT m.id) FILTER (WHERE m.role = 'captain' AND m.status = 'active') AS captain_count,
    COUNT(DISTINCT m.id) FILTER (WHERE m.role = 'player' AND m.status = 'active') AS player_count,
    COUNT(DISTINCT m.id) FILTER (WHERE m.role = 'substitute' AND m.status = 'active') AS substitute_count,

    -- Season settings
    s.team_size_min,
    s.team_size_max,
    s.roster_lock_status
FROM league_teams t
LEFT JOIN league_team_seasons ts ON ts.team_id = t.id
LEFT JOIN league_seasons s ON s.id = ts.season_id
LEFT JOIN league_team_members m ON m.team_season_id = ts.id
GROUP BY
    t.id, t.league_id, t.name, t.tag, t.logo_url, t.owner_player_id, t.status,
    ts.id, ts.season_id, ts.status,
    s.team_size_min, s.team_size_max, s.roster_lock_status;

COMMENT ON VIEW v_league_team_summary IS 'Summary view of teams with member counts per season';
