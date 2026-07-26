-- Migration: Create tournament_invitations table
-- Description: Backs `registration_type = 'invite_only'` with a real invite list.
--
-- Until now `invite_only` was decorative: `TournamentService::register_team` /
-- `register_player` checked only `is_registration_open()`, so an invite-only
-- tournament behaved exactly like `approval` — anyone could register and an
-- organiser had to reject the ones they did not want (audit P-27). Leagues have
-- enforced the same concept since 0022 (`league_invitations` +
-- `DomainError::LeagueInviteOnly`); tournaments had no invite storage at all.
--
-- An invitation targets EITHER a user (individual tournaments) or a team-season
-- (team tournaments), mirroring `tournament_registrations`, which likewise
-- carries a nullable `player_id` / `team_season_id` pair.

CREATE TABLE tournament_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Invite target: exactly one of these is set (see the check below).
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,

    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    message TEXT,
    invited_by UUID NOT NULL REFERENCES users(id),
    accepted_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT tournament_invitations_check_status
        CHECK (status IN ('pending', 'accepted', 'revoked')),
    CONSTRAINT tournament_invitations_check_target
        CHECK (num_nonnulls(user_id, team_season_id) = 1)
);

-- One live invitation per target. Revoked invitations are kept for audit, so
-- the uniqueness is partial: an organiser may re-invite after revoking.
CREATE UNIQUE INDEX idx_tournament_invitations_user_unique
    ON tournament_invitations(tournament_id, user_id)
    WHERE user_id IS NOT NULL AND status <> 'revoked';

CREATE UNIQUE INDEX idx_tournament_invitations_team_unique
    ON tournament_invitations(tournament_id, team_season_id)
    WHERE team_season_id IS NOT NULL AND status <> 'revoked';

CREATE INDEX idx_tournament_invitations_tournament_id
    ON tournament_invitations(tournament_id);

COMMENT ON TABLE tournament_invitations IS
    'Invite list for tournaments with registration_type = invite_only (audit P-27)';
COMMENT ON COLUMN tournament_invitations.user_id IS
    'Invited user — used for individual tournaments; mutually exclusive with team_season_id';
COMMENT ON COLUMN tournament_invitations.team_season_id IS
    'Invited team-season — used for team tournaments; mutually exclusive with user_id';
COMMENT ON COLUMN tournament_invitations.status IS
    'pending=outstanding, accepted=consumed by a registration, revoked=withdrawn by an organiser';
