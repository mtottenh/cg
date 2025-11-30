-- Drop old standalone teams system (replaced by league_teams)
-- The new system has league-scoped teams with seasonal participation:
--   - league_teams (persistent team identity within a league)
--   - league_team_seasons (team participation in a season)
--   - league_team_members (seasonal roster)
--   - league_team_invitations (seasonal invitations)

-- Drop tables in order to respect foreign key constraints
DROP TABLE IF EXISTS team_invitations CASCADE;
DROP TABLE IF EXISTS team_members CASCADE;
DROP TABLE IF EXISTS teams CASCADE;
