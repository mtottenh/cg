//! Type conversions from database rows to domain entities.

use crate::entities::league_team::{
    LeagueSeasonParticipantRow, LeagueSeasonRow, LeagueTeamInvitationRow,
    LeagueTeamInvitationWithTeamRow, LeagueTeamMemberRow, LeagueTeamMemberWithPlayerRow,
    LeagueTeamRow, LeagueTeamSeasonRow, LeagueTeamSummaryRow, PlayerLeagueTeamMembershipRow,
};
use portal_core::{
    LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId, LeagueTeamMemberId,
    LeagueTeamSeasonId, PlayerId, UserId,
};
use portal_domain::entities::league_team::{
    LeagueSeason, LeagueSeasonParticipant, LeagueSeasonParticipantStatus, LeagueTeam,
    LeagueTeamInvitation, LeagueTeamInvitationWithTeam, LeagueTeamMember,
    LeagueTeamMemberWithPlayer, LeagueTeamSeason, LeagueTeamSummary, PlayerLeagueTeamMembership,
};

impl From<LeagueSeasonRow> for LeagueSeason {
    fn from(row: LeagueSeasonRow) -> Self {
        Self {
            id: LeagueSeasonId::from_uuid(row.id),
            league_id: LeagueId::from_uuid(row.league_id),
            name: row.name,
            slug: row.slug,
            description: row.description,
            registration_start: row.registration_start,
            registration_end: row.registration_end,
            season_start: row.season_start,
            season_end: row.season_end,
            team_size_min: row.team_size_min,
            team_size_max: row.team_size_max,
            max_substitutes: row.max_substitutes,
            max_teams: row.max_teams,
            roster_lock_status: row.roster_lock_status.parse().unwrap_or_default(),
            roster_locked_at: row.roster_locked_at,
            roster_locked_by: row.roster_locked_by.map(UserId::from_uuid),
            status: row.status.parse().unwrap_or_default(),
            settings: row.settings,
            created_by: UserId::from_uuid(row.created_by),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<LeagueTeamRow> for LeagueTeam {
    fn from(row: LeagueTeamRow) -> Self {
        Self {
            id: LeagueTeamId::from_uuid(row.id),
            league_id: LeagueId::from_uuid(row.league_id),
            name: row.name,
            tag: row.tag,
            description: row.description,
            logo_url: row.logo_url,
            banner_url: row.banner_url,
            primary_color: row.primary_color,
            secondary_color: row.secondary_color,
            owner_player_id: PlayerId::from_uuid(row.owner_player_id),
            status: row.status.parse().unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            disbanded_at: row.disbanded_at,
        }
    }
}

impl From<LeagueTeamSeasonRow> for LeagueTeamSeason {
    fn from(row: LeagueTeamSeasonRow) -> Self {
        Self {
            id: LeagueTeamSeasonId::from_uuid(row.id),
            team_id: LeagueTeamId::from_uuid(row.team_id),
            season_id: LeagueSeasonId::from_uuid(row.season_id),
            status: row.status.parse().unwrap_or_default(),
            registered_at: row.registered_at,
            registration_notes: row.registration_notes,
            matches_played: row.matches_played,
            matches_won: row.matches_won,
            matches_lost: row.matches_lost,
            matches_drawn: row.matches_drawn,
            seed: row.seed,
            rating: row.rating,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<LeagueTeamMemberRow> for LeagueTeamMember {
    fn from(row: LeagueTeamMemberRow) -> Self {
        Self {
            id: LeagueTeamMemberId::from_uuid(row.id),
            team_season_id: LeagueTeamSeasonId::from_uuid(row.team_season_id),
            season_id: LeagueSeasonId::from_uuid(row.season_id),
            player_id: PlayerId::from_uuid(row.player_id),
            role: row.role.parse().unwrap_or_default(),
            position: row.position,
            jersey_number: row.jersey_number,
            status: row.status.parse().unwrap_or_default(),
            joined_at: row.joined_at,
            left_at: row.left_at,
            added_by: row.added_by.map(UserId::from_uuid),
        }
    }
}

impl From<LeagueTeamMemberWithPlayerRow> for LeagueTeamMemberWithPlayer {
    fn from(row: LeagueTeamMemberWithPlayerRow) -> Self {
        Self {
            id: LeagueTeamMemberId::from_uuid(row.id),
            team_season_id: LeagueTeamSeasonId::from_uuid(row.team_season_id),
            player_id: PlayerId::from_uuid(row.player_id),
            role: row.role.parse().unwrap_or_default(),
            position: row.position,
            jersey_number: row.jersey_number,
            status: row.status.parse().unwrap_or_default(),
            joined_at: row.joined_at,
            left_at: row.left_at,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
        }
    }
}

impl From<LeagueTeamInvitationRow> for LeagueTeamInvitation {
    fn from(row: LeagueTeamInvitationRow) -> Self {
        Self {
            id: LeagueTeamInvitationId::from_uuid(row.id),
            team_season_id: LeagueTeamSeasonId::from_uuid(row.team_season_id),
            player_id: PlayerId::from_uuid(row.player_id),
            invitation_type: row.invitation_type.parse().unwrap_or_default(),
            role: row.role.parse().unwrap_or_default(),
            message: row.message,
            response_message: row.response_message,
            invited_by: row.invited_by.map(UserId::from_uuid),
            status: row.status.parse().unwrap_or_default(),
            responded_at: row.responded_at,
            expires_at: row.expires_at,
            created_at: row.created_at,
        }
    }
}

impl From<LeagueTeamInvitationWithTeamRow> for LeagueTeamInvitationWithTeam {
    fn from(row: LeagueTeamInvitationWithTeamRow) -> Self {
        Self {
            id: LeagueTeamInvitationId::from_uuid(row.id),
            team_season_id: LeagueTeamSeasonId::from_uuid(row.team_season_id),
            player_id: PlayerId::from_uuid(row.player_id),
            invitation_type: row.invitation_type.parse().unwrap_or_default(),
            role: row.role.parse().unwrap_or_default(),
            message: row.message,
            invited_by: row.invited_by.map(UserId::from_uuid),
            status: row.status.parse().unwrap_or_default(),
            responded_at: row.responded_at,
            expires_at: row.expires_at,
            created_at: row.created_at,
            team_id: LeagueTeamId::from_uuid(row.team_id),
            team_name: row.team_name,
            team_tag: row.team_tag,
            team_logo_url: row.team_logo_url,
            season_id: LeagueSeasonId::from_uuid(row.season_id),
            season_name: row.season_name,
            league_id: LeagueId::from_uuid(row.league_id),
            league_name: row.league_name,
        }
    }
}

impl From<LeagueTeamSummaryRow> for LeagueTeamSummary {
    fn from(row: LeagueTeamSummaryRow) -> Self {
        Self {
            team_id: LeagueTeamId::from_uuid(row.team_id),
            league_id: LeagueId::from_uuid(row.league_id),
            team_name: row.team_name,
            team_tag: row.team_tag,
            team_logo_url: row.team_logo_url,
            owner_player_id: PlayerId::from_uuid(row.owner_player_id),
            team_status: row.team_status.parse().unwrap_or_default(),
            team_season_id: row.team_season_id.map(LeagueTeamSeasonId::from_uuid),
            season_id: row.season_id.map(LeagueSeasonId::from_uuid),
            season_status: row.season_status.and_then(|s| s.parse().ok()),
            active_member_count: row.active_member_count,
            captain_count: row.captain_count,
            player_count: row.player_count,
            substitute_count: row.substitute_count,
            team_size_min: row.team_size_min,
            team_size_max: row.team_size_max,
            roster_lock_status: row.roster_lock_status.and_then(|s| s.parse().ok()),
        }
    }
}

impl From<PlayerLeagueTeamMembershipRow> for PlayerLeagueTeamMembership {
    fn from(row: PlayerLeagueTeamMembershipRow) -> Self {
        Self {
            player_id: PlayerId::from_uuid(row.player_id),
            team_id: LeagueTeamId::from_uuid(row.team_id),
            team_name: row.team_name,
            team_tag: row.team_tag,
            team_logo_url: row.team_logo_url,
            team_season_id: LeagueTeamSeasonId::from_uuid(row.team_season_id),
            team_season_status: row.team_season_status.parse().unwrap_or_default(),
            role: row.role.parse().unwrap_or_default(),
            status: row.membership_status.parse().unwrap_or_default(),
            joined_at: row.joined_at,
            season_id: LeagueSeasonId::from_uuid(row.season_id),
            season_name: row.season_name,
            season_status: row.season_status.parse().unwrap_or_default(),
            league_id: LeagueId::from_uuid(row.league_id),
            league_name: row.league_name,
        }
    }
}

impl From<LeagueSeasonParticipantRow> for LeagueSeasonParticipant {
    fn from(row: LeagueSeasonParticipantRow) -> Self {
        Self {
            id: row.id,
            season_id: LeagueSeasonId::from_uuid(row.season_id),
            player_id: PlayerId::from_uuid(row.player_id),
            status: row
                .status
                .parse()
                .unwrap_or(LeagueSeasonParticipantStatus::Registered),
            seed: row.seed,
            rating: row.rating,
            matches_played: row.matches_played,
            matches_won: row.matches_won,
            matches_lost: row.matches_lost,
            matches_drawn: row.matches_drawn,
            registered_at: row.registered_at,
            withdrawn_at: row.withdrawn_at,
        }
    }
}
