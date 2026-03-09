-- Backfill: set season_id on tournaments that are linked to a league
-- but missing a season. Uses the league's current_season_id.

UPDATE tournaments t
SET season_id = l.current_season_id
FROM leagues l
WHERE t.league_id = l.id
  AND t.season_id IS NULL
  AND l.current_season_id IS NOT NULL;
