-- Migration: release the roster seats of teams that have left a season.
--
-- Disbanding a team, withdrawing it from a season and archiving it used to
-- change the team (or its season registration) and leave its roster seats
-- `active`. `idx_one_primary_team_per_season` counts exactly those seats, so
-- every player on such a team stayed a primary member of it and could not
-- join another team that season — the service check and the index both
-- said "already on a team", naming one nobody could see any more.
--
-- Those three operations now release the seats in the same transaction
-- (portal-db `league_team::roster_release`). This brings existing rows in
-- line with what they would have done. Only LIVE participation is touched,
-- as there: a registration that is not terminal, in a season that is not
-- completed or cancelled. Finished seasons keep the roster they ended with.
--
-- Statement order matters: each predicate reads the registration status the
-- last statement for that case writes, so registrations change last.

-- 1. Disbanded teams. Disbanding now withdraws the team from every season
--    it is still in, releases the seats ('left' — final) and withdraws its
--    entries in cups that have no bracket yet. An entry in a cup that has
--    one is left for the organiser: there, the live code refuses to disband.
UPDATE tournament_registrations tr
SET status = 'withdrawn', withdrawn_at = NOW(), updated_at = NOW()
FROM league_team_seasons lts, league_teams t, league_seasons s, tournaments tn
WHERE tr.team_season_id = lts.id
  AND t.id = lts.team_id
  AND s.id = lts.season_id
  AND tn.id = tr.tournament_id
  AND t.status = 'disbanded'
  AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
  AND s.status NOT IN ('completed', 'cancelled')
  AND tr.status IN ('pending', 'approved', 'checked_in', 'active')
  AND tn.status::text IN ('draft', 'published', 'registration');

UPDATE league_team_members m
SET status = 'left', left_at = COALESCE(t.disbanded_at, NOW())
FROM league_team_seasons lts
JOIN league_teams t ON t.id = lts.team_id
JOIN league_seasons s ON s.id = lts.season_id
WHERE m.team_season_id = lts.id
  AND m.left_at IS NULL
  AND t.status = 'disbanded'
  AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
  AND s.status NOT IN ('completed', 'cancelled');

UPDATE league_team_seasons lts
SET status = 'withdrawn', updated_at = NOW()
FROM league_teams t, league_seasons s
WHERE t.id = lts.team_id
  AND s.id = lts.season_id
  AND t.status = 'disbanded'
  AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
  AND s.status NOT IN ('completed', 'cancelled');

-- 2. Archived (not disbanded) teams. Archiving releases seats as
--    'inactive' with left_at = archived_at — the pair restore keys on to
--    bring exactly those seats back — and withdraws open cup entries.
UPDATE tournament_registrations tr
SET status = 'withdrawn', withdrawn_at = NOW(), updated_at = NOW()
FROM league_team_seasons lts, league_teams t, league_seasons s, tournaments tn
WHERE tr.team_season_id = lts.id
  AND t.id = lts.team_id
  AND s.id = lts.season_id
  AND tn.id = tr.tournament_id
  AND t.archived_at IS NOT NULL
  AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
  AND s.status NOT IN ('completed', 'cancelled')
  AND tr.status IN ('pending', 'approved', 'checked_in', 'active')
  AND tn.status::text IN ('draft', 'published', 'registration');

UPDATE league_team_members m
SET status = 'inactive', left_at = t.archived_at
FROM league_team_seasons lts
JOIN league_teams t ON t.id = lts.team_id
JOIN league_seasons s ON s.id = lts.season_id
WHERE m.team_season_id = lts.id
  AND m.left_at IS NULL
  AND t.archived_at IS NOT NULL
  AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
  AND s.status NOT IN ('completed', 'cancelled');

-- 3. Registrations withdrawn from a season: the seats go with them.
UPDATE league_team_members m
SET status = 'left', left_at = lts.updated_at
FROM league_team_seasons lts
JOIN league_seasons s ON s.id = lts.season_id
WHERE m.team_season_id = lts.id
  AND m.left_at IS NULL
  AND lts.status = 'withdrawn'
  AND s.status NOT IN ('completed', 'cancelled');
