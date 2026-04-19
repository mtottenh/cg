//! Shared test helpers for league team service tests.

use crate::entities::league::{League, LeagueAccessType, LeagueStatus};
use crate::entities::league_team::{
    LeagueSeason, LeagueSeasonParticipant, LeagueSeasonParticipantStatus, LeagueTeam,
    LeagueTeamInvitation, LeagueTeamMember, LeagueTeamSeason,
};
use chrono::Utc;
use portal_core::ids::{GameId, LeagueTeamMemberId};
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{
    LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamInvitationId, LeagueTeamSeasonId,
    PlayerId, UserId,
};

pub fn make_league() -> League {
    League {
        id: LeagueId::new(),
        game_id: GameId::new(),
        name: "Test League".to_string(),
        slug: "test-league".to_string(),
        description: None,
        logo_url: None,
        access_type: LeagueAccessType::Open,
        status: LeagueStatus::Active,
        current_season_id: None,
        settings: serde_json::json!({}),
        created_by: UserId::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_season(league_id: LeagueId) -> LeagueSeason {
    LeagueSeason {
        id: LeagueSeasonId::new(),
        league_id,
        name: "Season 1".to_string(),
        slug: "season-1".to_string(),
        description: None,
        registration_start: None,
        registration_end: None,
        season_start: None,
        season_end: None,
        team_size_min: Some(5),
        team_size_max: Some(7),
        max_substitutes: Some(2),
        max_teams: None,
        roster_lock_status: RosterLockStatus::Open,
        roster_locked_at: None,
        roster_locked_by: None,
        status: SeasonStatus::Registration,
        settings: serde_json::json!({}),
        created_by: UserId::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_team(league_id: LeagueId) -> LeagueTeam {
    LeagueTeam {
        id: LeagueTeamId::new(),
        league_id,
        name: "Test Team".to_string(),
        tag: "TST".to_string(),
        description: None,
        logo_url: None,
        banner_url: None,
        primary_color: None,
        secondary_color: None,
        owner_player_id: PlayerId::new(),
        status: LeagueTeamStatus::Active,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        disbanded_at: None,
    }
}

pub fn make_team_season(team_id: LeagueTeamId, season_id: LeagueSeasonId) -> LeagueTeamSeason {
    LeagueTeamSeason {
        id: LeagueTeamSeasonId::new(),
        team_id,
        season_id,
        status: LeagueTeamSeasonStatus::Forming,
        registered_at: Some(Utc::now()),
        registration_notes: None,
        matches_played: 0,
        matches_won: 0,
        matches_lost: 0,
        matches_drawn: 0,
        seed: None,
        rating: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn make_member(team_season_id: LeagueTeamSeasonId, player_id: PlayerId) -> LeagueTeamMember {
    LeagueTeamMember {
        id: LeagueTeamMemberId::new(),
        team_season_id,
        player_id,
        season_id: LeagueSeasonId::new(),
        role: LeagueTeamRole::Player,
        position: None,
        jersey_number: None,
        status: portal_core::types::LeagueTeamMemberStatus::Active,
        joined_at: Utc::now(),
        left_at: None,
        added_by: None,
    }
}

pub fn make_invitation(
    team_season_id: LeagueTeamSeasonId,
    player_id: PlayerId,
) -> LeagueTeamInvitation {
    LeagueTeamInvitation {
        id: LeagueTeamInvitationId::new(),
        team_season_id,
        player_id,
        invitation_type: LeagueTeamInvitationType::Invite,
        role: LeagueTeamRole::Player,
        message: None,
        response_message: None,
        invited_by: Some(UserId::new()),
        status: LeagueTeamInvitationStatus::Pending,
        responded_at: None,
        expires_at: Utc::now() + chrono::Duration::days(7),
        created_at: Utc::now(),
    }
}

pub fn make_participant(
    season_id: LeagueSeasonId,
    player_id: PlayerId,
) -> LeagueSeasonParticipant {
    LeagueSeasonParticipant {
        id: uuid::Uuid::new_v4(),
        season_id,
        player_id,
        status: LeagueSeasonParticipantStatus::Registered,
        seed: None,
        rating: None,
        matches_played: 0,
        matches_won: 0,
        matches_lost: 0,
        matches_drawn: 0,
        registered_at: Utc::now(),
        withdrawn_at: None,
    }
}
