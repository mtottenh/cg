-- Migration: archiving for leagues, seasons, tournaments and league teams.
--
-- The operator need is "make this stop being visible to players, without
-- destroying it or its history". Nothing supported that: a mis-created league
-- or a season that never ran stayed in every public listing forever, and the
-- only alternative was a DELETE that would cascade through matches, rosters
-- and results.
--
-- WHY A TIMESTAMP AND NOT A STATUS VALUE
--
-- Each of these tables already has a `status` that means something else:
-- where a season or tournament IS in its life (draft → active → completed),
-- and for teams whether they disbanded. Folding "archived" into that column
-- would destroy the answer to "what was this when it was put away" — restore
-- would have to guess between `completed` and `cancelled`. Archiving is
-- orthogonal to lifecycle, so it gets its own column, and restore is exactly
-- `archived_at = NULL` with the row's own status untouched.
--
-- `leagues.status` does have an 'archived' value predating this. It stays a
-- legal value (nothing writes it now) and the backfill below gives those rows
-- an `archived_at`, so one filter — `archived_at IS NULL` — is the whole
-- visibility rule everywhere.
--
-- CASCADE
--
-- Deliberately NOT propagated by writing children. A child of an archived
-- league is hidden because its parent is (the listing queries join and
-- check), so restoring the league restores exactly what it hid — and a
-- season archived on its own stays archived when its league comes back.
-- Mass-updating children would lose that distinction permanently.

ALTER TABLE leagues
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD COLUMN archived_by UUID REFERENCES users(id);

ALTER TABLE league_seasons
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD COLUMN archived_by UUID REFERENCES users(id);

ALTER TABLE tournaments
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD COLUMN archived_by UUID REFERENCES users(id);

ALTER TABLE league_teams
    ADD COLUMN archived_at TIMESTAMPTZ,
    ADD COLUMN archived_by UUID REFERENCES users(id);

COMMENT ON COLUMN leagues.archived_at IS
    'When this was archived. NULL means live. Archived rows are hidden from every player-facing listing and from their children''s, and are visible to operators with an explicit include-archived filter.';
COMMENT ON COLUMN league_seasons.archived_at IS 'See leagues.archived_at.';
COMMENT ON COLUMN tournaments.archived_at IS 'See leagues.archived_at.';
COMMENT ON COLUMN league_teams.archived_at IS 'See leagues.archived_at.';

-- Leagues already carrying the legacy status get a timestamp so the new
-- filter alone is enough. `updated_at` is the closest thing to when it
-- happened that the row records.
UPDATE leagues
   SET archived_at = updated_at
 WHERE status = 'archived'
   AND archived_at IS NULL;

-- Listing indexes: every player-facing query gains `archived_at IS NULL`, so
-- the partial index is the one that matters.
CREATE INDEX idx_leagues_live ON leagues (game_id, name) WHERE archived_at IS NULL;
CREATE INDEX idx_league_seasons_live ON league_seasons (league_id) WHERE archived_at IS NULL;
CREATE INDEX idx_tournaments_live ON tournaments (game_id, starts_at) WHERE archived_at IS NULL;
CREATE INDEX idx_league_teams_live ON league_teams (league_id) WHERE archived_at IS NULL;
