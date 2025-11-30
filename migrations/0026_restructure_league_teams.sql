-- Migration: Restructure League Teams
-- Description:
--   1. Drop old global team tables (teams, team_members, team_invitations)
--   2. Restructure league_teams to be league-scoped (persistent identity)
--   3. Create league_team_seasons for seasonal participation
--   4. Update league_team_members to reference team_season
--   5. Add league_season_participants for individual format leagues
--   6. Add format_type to leagues
--   7. Auto-create default season on league creation
--
-- KEY CHANGES:
--   - Teams now belong to LEAGUES (persistent identity across seasons)
--   - Rosters are per-season via league_team_seasons
--   - Multiple captains supported (captain is a role, not a field)
--   - Individual format leagues supported via league_season_participants

-- =============================================================================
-- 1. DROP OLD GLOBAL TEAM TABLES
-- =============================================================================
-- These are deprecated and being replaced by the league-scoped system

-- First drop dependent views if any
DROP VIEW IF EXISTS v_league_team_summary CASCADE;
DROP VIEW IF EXISTS v_player_league_teams CASCADE;

-- Drop old team invitations (has FK to old teams)
DROP TABLE IF EXISTS team_invitations CASCADE;

-- Drop old team members (has FK to old teams)
DROP TABLE IF EXISTS team_members CASCADE;

-- Drop old teams table
DROP TABLE IF EXISTS teams CASCADE;

-- =============================================================================
-- 2. DROP CURRENT LEAGUE TEAM TABLES (will recreate with new structure)
-- =============================================================================

DROP TABLE IF EXISTS league_team_invitations CASCADE;
DROP TABLE IF EXISTS league_team_members CASCADE;
DROP TABLE IF EXISTS league_teams CASCADE;

-- =============================================================================
-- 3. ADD FORMAT TYPE TO LEAGUES
-- =============================================================================

ALTER TABLE leagues
ADD COLUMN IF NOT EXISTS format_type VARCHAR(20) NOT NULL DEFAULT 'team';

ALTER TABLE leagues
ADD COLUMN IF NOT EXISTS default_team_size_min INTEGER DEFAULT 1;

ALTER TABLE leagues
ADD COLUMN IF NOT EXISTS default_team_size_max INTEGER DEFAULT 5;

ALTER TABLE leagues
ADD COLUMN IF NOT EXISTS default_max_substitutes INTEGER DEFAULT 2;

-- Add constraint for format_type
ALTER TABLE leagues
ADD CONSTRAINT leagues_check_format_type
CHECK (format_type IN ('team', 'individual'));

-- Add constraint for team settings (only required for team format)
ALTER TABLE leagues
ADD CONSTRAINT leagues_check_team_settings
CHECK (
    format_type = 'individual'
    OR (default_team_size_min IS NOT NULL
        AND default_team_size_max IS NOT NULL
        AND default_team_size_min > 0
        AND default_team_size_max >= default_team_size_min)
);

COMMENT ON COLUMN leagues.format_type IS 'team: requires team registration, individual: players register directly (1v1 tournaments)';

-- =============================================================================
-- 4. LEAGUE TEAMS (Persistent Identity - Scoped to League)
-- =============================================================================

CREATE TABLE league_teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,

    -- Identity (persistent across seasons, unique within league)
    name VARCHAR(64) NOT NULL,
    name_normalized VARCHAR(64) GENERATED ALWAYS AS (lower(name)) STORED,
    tag VARCHAR(5) NOT NULL,
    tag_normalized VARCHAR(5) GENERATED ALWAYS AS (lower(tag)) STORED,

    -- Profile (persistent)
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    primary_color VARCHAR(7),
    secondary_color VARCHAR(7),

    -- Ownership (permanent owner, can transfer)
    owner_player_id UUID NOT NULL REFERENCES players(id),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'active',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    disbanded_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT league_teams_name_unique UNIQUE (league_id, name_normalized),
    CONSTRAINT league_teams_tag_unique UNIQUE (league_id, tag_normalized),
    CONSTRAINT league_teams_tag_format CHECK (tag ~ '^[a-zA-Z0-9]{2,5}$'),
    CONSTRAINT league_teams_check_status CHECK (status IN (
        'active',          -- Team is active and can participate
        'inactive',        -- Temporarily inactive
        'disbanded'        -- Permanently disbanded
    )),
    CONSTRAINT league_teams_check_colors CHECK (
        (primary_color IS NULL OR primary_color ~ '^#[0-9A-Fa-f]{6}$') AND
        (secondary_color IS NULL OR secondary_color ~ '^#[0-9A-Fa-f]{6}$')
    )
);

CREATE INDEX idx_league_teams_league ON league_teams(league_id);
CREATE INDEX idx_league_teams_owner ON league_teams(owner_player_id);
CREATE INDEX idx_league_teams_status ON league_teams(status);
CREATE INDEX idx_league_teams_active ON league_teams(league_id) WHERE status = 'active';
CREATE INDEX idx_league_teams_name ON league_teams(league_id, name_normalized);
CREATE INDEX idx_league_teams_tag ON league_teams(league_id, tag_normalized);

CREATE TRIGGER league_teams_updated_at
    BEFORE UPDATE ON league_teams
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE league_teams IS 'Teams with persistent identity, scoped to a league (not season)';
COMMENT ON COLUMN league_teams.owner_player_id IS 'Permanent team owner - can transfer ownership, delete team';

-- =============================================================================
-- 5. LEAGUE TEAM SEASONS (Seasonal Participation)
-- =============================================================================

CREATE TABLE league_team_seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES league_teams(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,

    -- Registration status for this season
    status VARCHAR(32) NOT NULL DEFAULT 'forming',

    -- Registration tracking
    registered_at TIMESTAMPTZ,
    registration_notes TEXT,

    -- Season-specific statistics
    matches_played INTEGER NOT NULL DEFAULT 0,
    matches_won INTEGER NOT NULL DEFAULT 0,
    matches_lost INTEGER NOT NULL DEFAULT 0,
    matches_drawn INTEGER NOT NULL DEFAULT 0,

    -- Seed/Ranking for this season
    seed INTEGER,
    rating INTEGER,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT league_team_seasons_unique UNIQUE (team_id, season_id),
    CONSTRAINT league_team_seasons_check_status CHECK (status IN (
        'forming',         -- Still recruiting, roster incomplete
        'pending',         -- Submitted for registration review
        'registered',      -- Registered but season not started
        'active',          -- Competing in season
        'eliminated',      -- Eliminated from tournament/playoffs
        'disqualified',    -- Removed from competition
        'withdrawn'        -- Voluntarily withdrew from season
    ))
);

CREATE INDEX idx_league_team_seasons_team ON league_team_seasons(team_id);
CREATE INDEX idx_league_team_seasons_season ON league_team_seasons(season_id);
CREATE INDEX idx_league_team_seasons_status ON league_team_seasons(status);
CREATE INDEX idx_league_team_seasons_active ON league_team_seasons(season_id)
    WHERE status IN ('forming', 'pending', 'registered', 'active');

CREATE TRIGGER league_team_seasons_updated_at
    BEFORE UPDATE ON league_team_seasons
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE league_team_seasons IS 'Team participation in a specific season with seasonal stats';
COMMENT ON COLUMN league_team_seasons.status IS 'forming=recruiting, pending=awaiting approval, registered=ready, active=competing';

-- =============================================================================
-- 6. LEAGUE TEAM MEMBERS (Seasonal Roster)
-- =============================================================================

CREATE TABLE league_team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_season_id UUID NOT NULL REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id),

    -- Denormalized season_id for unique constraint enforcement
    season_id UUID NOT NULL REFERENCES league_seasons(id),

    -- Role within team (multiple captains allowed)
    role VARCHAR(32) NOT NULL DEFAULT 'player',

    -- Position (game-specific, e.g., "AWP", "IGL", "Support")
    position VARCHAR(64),

    -- Jersey/Number
    jersey_number INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'active',

    -- Timestamps
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,

    -- Added by (user_id since this is an admin/captain action)
    added_by UUID REFERENCES users(id),

    -- Constraints
    CONSTRAINT league_team_members_unique UNIQUE (team_season_id, player_id),
    CONSTRAINT league_team_members_check_role CHECK (role IN (
        'captain',         -- Team captain, can manage roster (multiple allowed)
        'player',          -- Primary roster player
        'substitute'       -- Backup player (can be on multiple teams)
    )),
    CONSTRAINT league_team_members_check_status CHECK (status IN (
        'active',          -- Currently on roster
        'inactive',        -- Temporarily unavailable
        'left',            -- Left the team
        'removed'          -- Removed by captain/admin
    )),
    CONSTRAINT league_team_members_check_jersey CHECK (
        jersey_number IS NULL OR (jersey_number >= 0 AND jersey_number <= 99)
    )
);

CREATE INDEX idx_league_team_members_team_season ON league_team_members(team_season_id);
CREATE INDEX idx_league_team_members_player ON league_team_members(player_id);
CREATE INDEX idx_league_team_members_season ON league_team_members(season_id);
CREATE INDEX idx_league_team_members_active ON league_team_members(team_season_id)
    WHERE status = 'active';
CREATE INDEX idx_league_team_members_role ON league_team_members(team_season_id, role);

-- Trigger to auto-populate season_id from team_season
CREATE OR REPLACE FUNCTION set_league_team_member_season_id()
RETURNS TRIGGER AS $$
BEGIN
    SELECT season_id INTO NEW.season_id
    FROM league_team_seasons WHERE id = NEW.team_season_id;

    IF NEW.season_id IS NULL THEN
        RAISE EXCEPTION 'Team season % not found', NEW.team_season_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_league_team_members_set_season_id
    BEFORE INSERT ON league_team_members
    FOR EACH ROW
    EXECUTE FUNCTION set_league_team_member_season_id();

-- Unique constraint: One primary team per player per season
-- Captains and players can only be on ONE team per season
-- Substitutes can be on multiple teams
CREATE UNIQUE INDEX idx_one_primary_team_per_season
ON league_team_members (player_id, season_id)
WHERE role IN ('captain', 'player') AND status = 'active';

COMMENT ON TABLE league_team_members IS 'Roster of players on a league team for a specific season';
COMMENT ON COLUMN league_team_members.role IS 'captain=can manage roster (multiple allowed), player=primary roster, substitute=backup (can be on multiple teams)';
COMMENT ON INDEX idx_one_primary_team_per_season IS 'Ensures a player can only be captain/player on one team per season';

-- =============================================================================
-- 7. LEAGUE TEAM INVITATIONS
-- =============================================================================

CREATE TABLE league_team_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_season_id UUID NOT NULL REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id),

    -- Invitation type
    invitation_type VARCHAR(20) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    message TEXT,

    -- Who sent it
    invited_by UUID REFERENCES users(id),

    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    responded_at TIMESTAMPTZ,
    response_message TEXT,

    -- Expiration
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT league_team_invitations_check_type CHECK (invitation_type IN ('invite', 'request')),
    CONSTRAINT league_team_invitations_check_role CHECK (role IN ('captain', 'player', 'substitute')),
    CONSTRAINT league_team_invitations_check_status CHECK (status IN (
        'pending', 'accepted', 'declined', 'expired', 'cancelled'
    ))
);

CREATE INDEX idx_league_team_invitations_team_season ON league_team_invitations(team_season_id);
CREATE INDEX idx_league_team_invitations_player ON league_team_invitations(player_id);
CREATE INDEX idx_league_team_invitations_pending ON league_team_invitations(player_id, status)
    WHERE status = 'pending';

COMMENT ON TABLE league_team_invitations IS 'Invitations and join requests for league team rosters';

-- =============================================================================
-- 8. LEAGUE SEASON PARTICIPANTS (For Individual Format)
-- =============================================================================

CREATE TABLE league_season_participants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id),

    -- Registration status
    status VARCHAR(32) NOT NULL DEFAULT 'registered',

    -- Seed/Rating
    seed INTEGER,
    rating INTEGER,

    -- Statistics
    matches_played INTEGER NOT NULL DEFAULT 0,
    matches_won INTEGER NOT NULL DEFAULT 0,
    matches_lost INTEGER NOT NULL DEFAULT 0,
    matches_drawn INTEGER NOT NULL DEFAULT 0,

    -- Timestamps
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    withdrawn_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT league_season_participants_unique UNIQUE (season_id, player_id),
    CONSTRAINT league_season_participants_check_status CHECK (status IN (
        'registered',      -- Registered for season
        'active',          -- Competing
        'eliminated',      -- Eliminated from tournament
        'disqualified',    -- Removed from competition
        'withdrawn'        -- Voluntarily withdrew
    ))
);

CREATE INDEX idx_league_season_participants_season ON league_season_participants(season_id);
CREATE INDEX idx_league_season_participants_player ON league_season_participants(player_id);
CREATE INDEX idx_league_season_participants_active ON league_season_participants(season_id)
    WHERE status IN ('registered', 'active');

COMMENT ON TABLE league_season_participants IS 'Individual players registered for a season (for individual format leagues)';

-- =============================================================================
-- 9. AUTO-CREATE DEFAULT SEASON ON LEAGUE CREATION
-- =============================================================================

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

    -- Set as current season
    NEW.current_season_id := new_season_id;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Use BEFORE INSERT so we can set current_season_id
CREATE TRIGGER trg_leagues_create_default_season
    BEFORE INSERT ON leagues
    FOR EACH ROW
    EXECUTE FUNCTION create_default_league_season();

COMMENT ON FUNCTION create_default_league_season IS 'Auto-creates Season 1 when a new league is created';

-- =============================================================================
-- 10. HELPFUL VIEWS
-- =============================================================================

-- View: Team summary with member counts for current season participation
CREATE OR REPLACE VIEW v_league_team_summary AS
SELECT
    lt.id AS team_id,
    lt.league_id,
    lt.name AS team_name,
    lt.tag AS team_tag,
    lt.logo_url,
    lt.owner_player_id,
    lt.status AS team_status,
    lts.id AS team_season_id,
    lts.season_id,
    lts.status AS season_status,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.status = 'active') AS active_member_count,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.role = 'captain' AND ltm.status = 'active') AS captain_count,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.role = 'player' AND ltm.status = 'active') AS player_count,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.role = 'substitute' AND ltm.status = 'active') AS substitute_count,
    ls.team_size_min,
    ls.team_size_max,
    ls.roster_lock_status
FROM league_teams lt
LEFT JOIN league_team_seasons lts ON lts.team_id = lt.id
LEFT JOIN league_seasons ls ON ls.id = lts.season_id
LEFT JOIN league_team_members ltm ON ltm.team_season_id = lts.id
GROUP BY lt.id, lt.league_id, lt.name, lt.tag, lt.logo_url, lt.owner_player_id, lt.status,
         lts.id, lts.season_id, lts.status, ls.team_size_min, ls.team_size_max, ls.roster_lock_status;

COMMENT ON VIEW v_league_team_summary IS 'Summary of league teams with member counts per season';

-- View: Player's team memberships across all leagues and seasons
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
JOIN games g ON g.id = l.game_id;

COMMENT ON VIEW v_player_league_teams IS 'All league team memberships for a player across all seasons';

-- =============================================================================
-- 11. UPDATE PERMISSIONS
-- =============================================================================

-- Add new permission for individual format registration
INSERT INTO permissions (id, name, display_name, description, category)
VALUES
    (gen_random_uuid(), 'league.participants.manage', 'Manage Participants', 'Manage individual participants in a league season', 'league')
ON CONFLICT (name) DO NOTHING;

-- Add permission to league_admin role
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name = 'league_admin'
AND p.name = 'league.participants.manage'
ON CONFLICT DO NOTHING;
