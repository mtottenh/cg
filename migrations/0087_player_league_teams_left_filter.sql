-- P-194: v_player_league_teams had no left_at filter while `is_member` does,
-- so GET /v1/players/me/league-teams returned teams the player had LEFT (or
-- been removed from) — feeding myTeams, hasEligibleTeams and the team picker
-- with rosters the player is no longer on.
--
-- Recreate with the same column list plus the missing predicate. A membership
-- row records departure by stamping left_at; an active membership is left_at
-- IS NULL, which is exactly the filter is_member applies.
CREATE OR REPLACE VIEW v_player_league_teams AS
SELECT
    ltm.player_id,
    ltm.team_season_id,
    lt.id AS team_id,
    lt.name AS team_name,
    lt.tag AS team_tag,
    lt.logo_url AS team_logo_url,
    ltm.role,
    ltm.status AS membership_status,
    ltm.joined_at,
    lts.status AS team_season_status,
    ls.id AS season_id,
    ls.name AS season_name,
    ls.status AS season_status,
    l.id AS league_id,
    l.name AS league_name,
    g.id AS game_id,
    g.display_name AS game_name
FROM league_team_members ltm
JOIN league_team_seasons lts ON lts.id = ltm.team_season_id
JOIN league_teams lt ON lt.id = lts.team_id
JOIN league_seasons ls ON ls.id = lts.season_id
JOIN leagues l ON l.id = ls.league_id
JOIN games g ON g.id = l.game_id
WHERE ltm.left_at IS NULL;

COMMENT ON VIEW v_player_league_teams IS 'Current (not left) league team memberships for a player across all seasons';
