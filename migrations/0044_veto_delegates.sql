-- Veto delegation table for delegating pick/ban authority to team members
-- A delegate can perform veto actions (picks/bans) on behalf of the team

CREATE TABLE veto_delegates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Which team-season this delegation is for
    team_season_id UUID NOT NULL REFERENCES league_team_seasons(id) ON DELETE CASCADE,

    -- The player who is being delegated authority
    player_id UUID NOT NULL REFERENCES players(id),

    -- Who created this delegation and under what authority
    delegated_by_user_id UUID NOT NULL REFERENCES users(id),
    delegated_by_role VARCHAR(32) NOT NULL,

    -- Optional scope to specific tournament (NULL = all tournaments for this team)
    tournament_id UUID REFERENCES tournaments(id),

    -- Revocation tracking (delegation is active until revoked)
    revoked_at TIMESTAMPTZ,
    revoked_by_user_id UUID REFERENCES users(id),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Valid roles: captain, owner, tournament_admin
    CONSTRAINT veto_delegates_check_role CHECK (
        delegated_by_role IN ('captain', 'owner', 'tournament_admin')
    )
);

-- Ensure only one active delegation per player per team-season
-- (a player can be re-delegated after revocation)
CREATE UNIQUE INDEX idx_veto_delegates_unique_active
    ON veto_delegates(team_season_id, player_id)
    WHERE revoked_at IS NULL;

-- Index for looking up delegations by team
CREATE INDEX idx_veto_delegates_team_season ON veto_delegates(team_season_id);

-- Index for looking up delegations by player
CREATE INDEX idx_veto_delegates_player ON veto_delegates(player_id);

-- Index for tournament-scoped lookups
CREATE INDEX idx_veto_delegates_tournament ON veto_delegates(tournament_id)
    WHERE tournament_id IS NOT NULL;

-- Comments for documentation
COMMENT ON TABLE veto_delegates IS 'Delegation of veto (pick/ban) authority to team members';
COMMENT ON COLUMN veto_delegates.delegated_by_role IS 'Role that authorized the delegation: captain, owner, or tournament_admin';
COMMENT ON COLUMN veto_delegates.tournament_id IS 'Optional scope - NULL means delegation applies to all tournaments for this team-season';
COMMENT ON COLUMN veto_delegates.revoked_at IS 'When set, delegation is no longer active';
