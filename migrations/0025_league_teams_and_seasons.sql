-- Migration: League Teams and Seasons
-- Description: Restructure teams to be league-scoped with seasons support
--
-- KEY CHANGES:
-- 1. Deprecate global `teams` table (keep for historical data, mark as legacy)
-- 2. Add `league_seasons` table for season management
-- 3. Add `league_teams` table (teams scoped to a league/season)
-- 4. Add `league_team_members` table with substitute support
-- 5. Add roster lock mechanism
--
-- RELATIONSHIPS:
--   Game -> League -> Season -> LeagueTeam -> LeagueTeamMember
--
-- CONSTRAINTS:
--   - A user can only be PRIMARY member (captain/player) of ONE team per league season
--   - Substitutes can be on multiple teams (but cannot play against their primary team)
--   - Rosters can be locked at various stages (formation, signup, tournament start)

-- =============================================================================
-- 1. LEAGUE SEASONS
-- =============================================================================
-- Seasons allow leagues to reset, reform teams, and track historical data

CREATE TABLE league_seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,

    -- Identity
    name VARCHAR(100) NOT NULL,           -- e.g., "Season 1", "Winter 2024"
    slug VARCHAR(100) NOT NULL,           -- URL-friendly, unique within league
    description TEXT,

    -- Timing
    registration_start TIMESTAMPTZ,       -- When teams can start forming
    registration_end TIMESTAMPTZ,         -- Deadline to finalize rosters
    season_start TIMESTAMPTZ,             -- Competition begins
    season_end TIMESTAMPTZ,               -- Competition ends

    -- Team Settings (inherited from league, can override)
    team_size_min INTEGER NOT NULL DEFAULT 1,
    team_size_max INTEGER NOT NULL DEFAULT 5,
    max_substitutes INTEGER NOT NULL DEFAULT 2,
    max_teams INTEGER,                    -- NULL = unlimited

    -- Roster Lock Status
    roster_lock_status VARCHAR(32) NOT NULL DEFAULT 'open',
    roster_locked_at TIMESTAMPTZ,
    roster_locked_by UUID REFERENCES users(id),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'draft',

    -- Metadata
    settings JSONB NOT NULL DEFAULT '{}',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT league_seasons_slug_unique UNIQUE (league_id, slug),
    CONSTRAINT league_seasons_check_status CHECK (status IN (
        'draft',           -- Being configured
        'registration',    -- Open for team formation
        'active',          -- Competition in progress
        'playoffs',        -- Playoff stage
        'completed',       -- Season finished
        'cancelled'        -- Season cancelled
    )),
    CONSTRAINT league_seasons_check_roster_lock CHECK (roster_lock_status IN (
        'open',            -- Teams can modify rosters freely
        'soft_lock',       -- Minor changes allowed (substitutes only)
        'hard_lock'        -- No roster changes allowed
    )),
    CONSTRAINT league_seasons_check_team_size CHECK (
        team_size_min > 0 AND
        team_size_max >= team_size_min AND
        max_substitutes >= 0
    ),
    CONSTRAINT league_seasons_check_dates CHECK (
        (registration_start IS NULL OR registration_end IS NULL OR registration_start < registration_end) AND
        (registration_end IS NULL OR season_start IS NULL OR registration_end <= season_start) AND
        (season_start IS NULL OR season_end IS NULL OR season_start < season_end)
    )
);

CREATE INDEX idx_league_seasons_league ON league_seasons(league_id);
CREATE INDEX idx_league_seasons_status ON league_seasons(status);
CREATE INDEX idx_league_seasons_active ON league_seasons(league_id)
    WHERE status IN ('registration', 'active', 'playoffs');

CREATE TRIGGER league_seasons_updated_at
    BEFORE UPDATE ON league_seasons
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE league_seasons IS 'Seasons within a league for organizing competitions with team rosters';
COMMENT ON COLUMN league_seasons.roster_lock_status IS 'Controls roster modifications: open=free changes, soft_lock=subs only, hard_lock=frozen';

-- =============================================================================
-- 2. LEAGUE TEAMS
-- =============================================================================
-- Teams are scoped to a specific league season

CREATE TABLE league_teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,

    -- Identity (unique within the season)
    name VARCHAR(64) NOT NULL,
    name_normalized VARCHAR(64) GENERATED ALWAYS AS (lower(name)) STORED,
    tag VARCHAR(5) NOT NULL,
    tag_normalized VARCHAR(5) GENERATED ALWAYS AS (lower(tag)) STORED,

    -- Profile
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    primary_color VARCHAR(7),
    secondary_color VARCHAR(7),

    -- Captain/Creator (references player, not user - semantic purity)
    captain_player_id UUID NOT NULL REFERENCES players(id),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'forming',

    -- Registration
    registered_at TIMESTAMPTZ,            -- When team completed registration
    registration_notes TEXT,              -- Admin notes during registration review

    -- Statistics (season-specific)
    matches_played INTEGER NOT NULL DEFAULT 0,
    matches_won INTEGER NOT NULL DEFAULT 0,
    matches_lost INTEGER NOT NULL DEFAULT 0,
    matches_drawn INTEGER NOT NULL DEFAULT 0,

    -- Seed/Ranking (for tournament seeding)
    seed INTEGER,
    rating INTEGER,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    disbanded_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT league_teams_name_unique UNIQUE (season_id, name_normalized),
    CONSTRAINT league_teams_tag_unique UNIQUE (season_id, tag_normalized),
    CONSTRAINT league_teams_tag_format CHECK (tag ~ '^[a-zA-Z0-9]{2,5}$'),
    CONSTRAINT league_teams_check_status CHECK (status IN (
        'forming',         -- Still recruiting, roster incomplete
        'pending',         -- Submitted for registration review
        'active',          -- Fully registered and active
        'disqualified',    -- Removed from competition
        'disbanded',       -- Voluntarily disbanded
        'eliminated'       -- Eliminated from tournament/playoffs
    )),
    CONSTRAINT league_teams_check_colors CHECK (
        (primary_color IS NULL OR primary_color ~ '^#[0-9A-Fa-f]{6}$') AND
        (secondary_color IS NULL OR secondary_color ~ '^#[0-9A-Fa-f]{6}$')
    )
);

CREATE INDEX idx_league_teams_season ON league_teams(season_id);
CREATE INDEX idx_league_teams_captain ON league_teams(captain_player_id);
CREATE INDEX idx_league_teams_status ON league_teams(status);
CREATE INDEX idx_league_teams_active ON league_teams(season_id)
    WHERE status = 'active';
CREATE INDEX idx_league_teams_name ON league_teams(season_id, name_normalized);
CREATE INDEX idx_league_teams_tag ON league_teams(season_id, tag_normalized);

CREATE TRIGGER league_teams_updated_at
    BEFORE UPDATE ON league_teams
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE league_teams IS 'Teams participating in a specific league season';
COMMENT ON COLUMN league_teams.status IS 'forming=recruiting, pending=awaiting approval, active=competing, disbanded/eliminated/disqualified=inactive';

-- =============================================================================
-- 3. LEAGUE TEAM MEMBERS
-- =============================================================================
-- Members of league teams with role and substitute support

CREATE TABLE league_team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES league_teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id),

    -- Denormalized season_id for unique constraint enforcement
    -- Kept in sync with league_teams.season_id via trigger
    season_id UUID NOT NULL REFERENCES league_seasons(id),

    -- Role within team
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

    -- Invited/Added by (user_id since this is an admin/captain action)
    added_by UUID REFERENCES users(id),

    -- Constraints
    CONSTRAINT league_team_members_unique UNIQUE (team_id, player_id),
    CONSTRAINT league_team_members_check_role CHECK (role IN (
        'captain',         -- Team leader, can manage roster
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

CREATE INDEX idx_league_team_members_team ON league_team_members(team_id);
CREATE INDEX idx_league_team_members_player ON league_team_members(player_id);
CREATE INDEX idx_league_team_members_active ON league_team_members(team_id)
    WHERE status = 'active';
CREATE INDEX idx_league_team_members_role ON league_team_members(team_id, role);

COMMENT ON TABLE league_team_members IS 'Roster of players on a league team';
COMMENT ON COLUMN league_team_members.role IS 'captain=team leader, player=primary roster, substitute=backup (can be on multiple teams)';

-- =============================================================================
-- 4. UNIQUE CONSTRAINT: ONE PRIMARY TEAM PER PLAYER PER SEASON
-- =============================================================================
-- A player can only be captain or player (not substitute) on ONE team per season
-- We use the denormalized season_id column for efficient index-based enforcement

-- Trigger to auto-populate season_id from team on insert
CREATE OR REPLACE FUNCTION set_league_team_member_season_id()
RETURNS TRIGGER AS $$
BEGIN
    -- Get the season_id from the team
    SELECT season_id INTO NEW.season_id
    FROM league_teams WHERE id = NEW.team_id;

    IF NEW.season_id IS NULL THEN
        RAISE EXCEPTION 'Team % not found', NEW.team_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_league_team_members_set_season_id
    BEFORE INSERT ON league_team_members
    FOR EACH ROW
    EXECUTE FUNCTION set_league_team_member_season_id();

-- Create a unique index that enforces: one primary team per player per season
-- Using the denormalized season_id column
CREATE UNIQUE INDEX idx_one_primary_team_per_season
ON league_team_members (player_id, season_id)
WHERE role IN ('captain', 'player') AND status = 'active';

COMMENT ON INDEX idx_one_primary_team_per_season IS
    'Ensures a player can only be a primary member (captain/player) of one team per season';

-- =============================================================================
-- 5. LEAGUE TEAM INVITATIONS
-- =============================================================================
-- Invitations to join league teams

CREATE TABLE league_team_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES league_teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id),

    -- Invitation type
    type VARCHAR(20) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    message TEXT,

    -- Who sent it (user_id since this is an admin/captain action)
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
    CONSTRAINT league_team_invitations_check_type CHECK (type IN ('invite', 'request')),
    CONSTRAINT league_team_invitations_check_role CHECK (role IN ('player', 'substitute')),
    CONSTRAINT league_team_invitations_check_status CHECK (status IN (
        'pending', 'accepted', 'declined', 'expired', 'cancelled'
    ))
);

CREATE INDEX idx_league_team_invitations_team ON league_team_invitations(team_id);
CREATE INDEX idx_league_team_invitations_player ON league_team_invitations(player_id);
CREATE INDEX idx_league_team_invitations_pending ON league_team_invitations(player_id, status)
    WHERE status = 'pending';

COMMENT ON TABLE league_team_invitations IS 'Invitations and join requests for league teams';

-- =============================================================================
-- 6. ADD CURRENT SEASON REFERENCE TO LEAGUES
-- =============================================================================

ALTER TABLE leagues
ADD COLUMN current_season_id UUID REFERENCES league_seasons(id);

CREATE INDEX idx_leagues_current_season ON leagues(current_season_id)
    WHERE current_season_id IS NOT NULL;

COMMENT ON COLUMN leagues.current_season_id IS 'Reference to the currently active season for quick access';

-- =============================================================================
-- 7. DEPRECATE OLD TEAMS TABLE
-- =============================================================================
-- We keep the old tables but mark them as deprecated
-- The data can be migrated or referenced historically

COMMENT ON TABLE teams IS 'DEPRECATED: Legacy global teams table. Use league_teams for new functionality.';
COMMENT ON TABLE team_members IS 'DEPRECATED: Legacy team membership. Use league_team_members for new functionality.';
COMMENT ON TABLE team_invitations IS 'DEPRECATED: Legacy team invitations. Use league_team_invitations for new functionality.';

-- Add deprecation flag columns
ALTER TABLE teams ADD COLUMN is_deprecated BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE teams ADD COLUMN migrated_to_league_team_id UUID REFERENCES league_teams(id);

-- =============================================================================
-- 8. SEED LEAGUE PERMISSIONS FOR TEAM MANAGEMENT
-- =============================================================================

-- Add new permissions for league team management
INSERT INTO permissions (id, name, display_name, description, category)
VALUES
    (gen_random_uuid(), 'league.teams.create', 'Create League Teams', 'Create teams in a league season', 'league'),
    (gen_random_uuid(), 'league.teams.manage', 'Manage League Teams', 'Manage all teams in a league (admin override)', 'league'),
    (gen_random_uuid(), 'league.rosters.lock', 'Lock Rosters', 'Lock/unlock rosters for a season', 'league'),
    (gen_random_uuid(), 'league.seasons.manage', 'Manage League Seasons', 'Create and manage league seasons', 'league')
ON CONFLICT (name) DO NOTHING;

-- Add permissions to league_admin role
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
CROSS JOIN permissions p
WHERE r.name = 'league_admin'
AND p.name IN (
    'league.teams.create',
    'league.teams.manage',
    'league.rosters.lock',
    'league.seasons.manage'
)
ON CONFLICT DO NOTHING;

-- =============================================================================
-- 9. HELPFUL VIEWS
-- =============================================================================

-- View: Active teams with member counts for a season
CREATE OR REPLACE VIEW v_league_team_summary AS
SELECT
    lt.id AS team_id,
    lt.season_id,
    ls.league_id,
    lt.name AS team_name,
    lt.tag AS team_tag,
    lt.status AS team_status,
    lt.captain_player_id,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.status = 'active') AS active_member_count,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.role IN ('captain', 'player') AND ltm.status = 'active') AS primary_member_count,
    COUNT(DISTINCT ltm.id) FILTER (WHERE ltm.role = 'substitute' AND ltm.status = 'active') AS substitute_count,
    ls.team_size_min,
    ls.team_size_max,
    ls.roster_lock_status
FROM league_teams lt
JOIN league_seasons ls ON ls.id = lt.season_id
LEFT JOIN league_team_members ltm ON ltm.team_id = lt.id
GROUP BY lt.id, lt.season_id, ls.league_id, lt.name, lt.tag, lt.status, lt.captain_player_id,
         ls.team_size_min, ls.team_size_max, ls.roster_lock_status;

COMMENT ON VIEW v_league_team_summary IS 'Summary of league teams with member counts and roster status';

-- View: Player's team memberships across all seasons
CREATE OR REPLACE VIEW v_player_league_teams AS
SELECT
    ltm.player_id,
    ltm.team_id,
    lt.name AS team_name,
    lt.tag AS team_tag,
    lt.logo_url AS team_logo_url,
    ltm.role,
    ltm.status AS membership_status,
    ltm.joined_at,
    lt.status AS team_status,
    ls.id AS season_id,
    ls.name AS season_name,
    ls.status AS season_status,
    l.id AS league_id,
    l.name AS league_name,
    g.id AS game_id,
    g.display_name AS game_name
FROM league_team_members ltm
JOIN league_teams lt ON lt.id = ltm.team_id
JOIN league_seasons ls ON ls.id = lt.season_id
JOIN leagues l ON l.id = ls.league_id
JOIN games g ON g.id = l.game_id;

COMMENT ON VIEW v_player_league_teams IS 'All league team memberships for a player across all seasons';
