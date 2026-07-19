//! Tests for LeagueTeamInvitationService.

use super::helpers::{make_invitation, make_member, make_season, make_team, make_team_season};
use crate::repositories::league_team::{
    MockLeagueSeasonRepository, MockLeagueTeamInvitationRepository, MockLeagueTeamMemberRepository,
    MockLeagueTeamRepository, MockLeagueTeamSeasonRepository,
};
use crate::services::league_team::LeagueTeamInvitationService;
use portal_core::types::{LeagueTeamInvitationStatus, LeagueTeamRole};
use portal_core::{
    DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId, UserId,
};
use std::sync::Arc;

fn create_service(
    invitation_repo: MockLeagueTeamInvitationRepository,
    team_repo: MockLeagueTeamRepository,
    team_season_repo: MockLeagueTeamSeasonRepository,
    member_repo: MockLeagueTeamMemberRepository,
    season_repo: MockLeagueSeasonRepository,
) -> LeagueTeamInvitationService<
    MockLeagueTeamInvitationRepository,
    MockLeagueTeamRepository,
    MockLeagueTeamSeasonRepository,
    MockLeagueTeamMemberRepository,
    MockLeagueSeasonRepository,
> {
    LeagueTeamInvitationService::new(
        Arc::new(invitation_repo),
        Arc::new(team_repo),
        Arc::new(team_season_repo),
        Arc::new(member_repo),
        Arc::new(season_repo),
    )
}

#[tokio::test]
async fn test_create_invitation_success() {
    let mut invitation_repo = MockLeagueTeamInvitationRepository::new();
    let mut team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let team_id = team.id;
    let season = make_season(league_id);
    let season_id = season.id;
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let inviter_id = UserId::new();
    let invitee_id = PlayerId::new();

    // Get team season
    let ts_clone = team_season.clone();
    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(ts_clone.clone())));

    // Get team (for owner check)
    team_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team.clone())));

    // Get season (for roster lock check)
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    // Check invitee not already member
    member_repo.expect_is_member().returning(|_, _| Ok(false));

    // For primary roles, check one-team-per-season (Player is not primary, but let's add the mock to be safe)
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));

    // Check no pending invitation
    invitation_repo
        .expect_find_existing_pending()
        .returning(|_, _| Ok(None));

    // Create invitation
    let invitation = make_invitation(team_season_id, invitee_id);
    let expected_id = invitation.id;
    invitation_repo
        .expect_create()
        .returning(move |_| Ok(invitation.clone()));

    let service = create_service(
        invitation_repo,
        team_repo,
        team_season_repo,
        member_repo,
        season_repo,
    );

    let result = service
        .create_invitation(
            team_season_id,
            invitee_id,
            LeagueTeamRole::Player,
            None,
            inviter_id,
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, expected_id);
}

#[tokio::test]
async fn test_create_invitation_player_already_member() {
    let invitation_repo = MockLeagueTeamInvitationRepository::new();
    let mut team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let team_id = team.id;
    let season = make_season(league_id);
    let season_id = season.id;
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let inviter_id = UserId::new();
    let invitee_id = PlayerId::new();

    let ts_clone = team_season.clone();
    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(ts_clone.clone())));

    team_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team.clone())));

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    // Player already a member
    member_repo.expect_is_member().returning(|_, _| Ok(true));

    let service = create_service(
        invitation_repo,
        team_repo,
        team_season_repo,
        member_repo,
        season_repo,
    );

    let result = service
        .create_invitation(
            team_season_id,
            invitee_id,
            LeagueTeamRole::Player,
            None,
            inviter_id,
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::AlreadyTeamMember
    ));
}

#[tokio::test]
async fn test_accept_invitation_success() {
    let mut invitation_repo = MockLeagueTeamInvitationRepository::new();
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team_id = LeagueTeamId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();
    let invitation = make_invitation(team_season_id, player_id);
    let invitation_id = invitation.id;

    // Get invitation
    invitation_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(invitation.clone())));

    // Get team season
    let ts_clone = team_season.clone();
    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(ts_clone.clone())));

    // Get season
    let s_clone = season.clone();
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(s_clone.clone())));

    // Check player not already in another team this season
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));

    // Check team size limit (Player role is primary)
    member_repo
        .expect_count_primary_members()
        .returning(|_| Ok(3)); // Below the max of 7

    // Atomic accept + add member (see audit I5). The service now
    // collapses the prior `update_status(Accepted) + add_member` pair
    // into a single repo method that runs both writes in one
    // transaction, so the mock expects one call here instead of two.
    let member = make_member(team_season_id, player_id);
    invitation_repo
        .expect_accept_and_add_member()
        .returning(move |_, _| Ok(member.clone()));

    let service = create_service(
        invitation_repo,
        team_repo,
        team_season_repo,
        member_repo,
        season_repo,
    );

    let result = service.accept_invitation(invitation_id, player_id).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_accept_invitation_wrong_player() {
    let mut invitation_repo = MockLeagueTeamInvitationRepository::new();
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let _league_id = LeagueId::new();
    let team_id = LeagueTeamId::new();
    let season_id = LeagueSeasonId::new();
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();
    let wrong_player_id = PlayerId::new();
    let invitation = make_invitation(team_season_id, player_id);
    let invitation_id = invitation.id;

    invitation_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(invitation.clone())));

    // Need to get team_season before authorization check
    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));

    let service = create_service(
        invitation_repo,
        team_repo,
        team_season_repo,
        member_repo,
        season_repo,
    );

    let result = service
        .accept_invitation(invitation_id, wrong_player_id)
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::NotAuthorized(_)));
}

#[tokio::test]
async fn test_decline_invitation_success() {
    let mut invitation_repo = MockLeagueTeamInvitationRepository::new();
    let team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let team_season_id = LeagueTeamSeasonId::new();
    let player_id = PlayerId::new();
    let invitation = make_invitation(team_season_id, player_id);
    let invitation_id = invitation.id;

    invitation_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(invitation.clone())));

    let mut declined = make_invitation(team_season_id, player_id);
    declined.status = LeagueTeamInvitationStatus::Declined;
    invitation_repo
        .expect_update_status()
        .returning(move |_, _, _| Ok(declined.clone()));

    let service = create_service(
        invitation_repo,
        team_repo,
        team_season_repo,
        member_repo,
        season_repo,
    );

    let result = service
        .decline_invitation(invitation_id, player_id, None)
        .await;

    assert!(result.is_ok());
}
