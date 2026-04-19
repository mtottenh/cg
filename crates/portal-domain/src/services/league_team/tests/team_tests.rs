//! Tests for LeagueTeamService.

use super::helpers::{make_member, make_season, make_team, make_team_season};
use crate::entities::league_team::CreateLeagueTeamCommand;
use crate::repositories::league_team::{
    MockLeagueSeasonRepository, MockLeagueTeamMemberRepository, MockLeagueTeamRepository,
    MockLeagueTeamSeasonRepository,
};
use crate::services::league_team::LeagueTeamService;
use portal_core::types::{LeagueTeamRole, LeagueTeamStatus, SeasonStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId};
use std::sync::Arc;

fn create_service(
    team_repo: MockLeagueTeamRepository,
    team_season_repo: MockLeagueTeamSeasonRepository,
    member_repo: MockLeagueTeamMemberRepository,
    season_repo: MockLeagueSeasonRepository,
) -> LeagueTeamService<
    MockLeagueTeamRepository,
    MockLeagueTeamSeasonRepository,
    MockLeagueTeamMemberRepository,
    MockLeagueSeasonRepository,
> {
    LeagueTeamService::new(
        Arc::new(team_repo),
        Arc::new(team_season_repo),
        Arc::new(member_repo),
        Arc::new(season_repo),
    )
}

#[tokio::test]
async fn test_get_team_found() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let team_id = team.id;
    let expected = team.clone();

    team_repo
        .expect_find_by_id()
        .with(mockall::predicate::eq(team_id))
        .returning(move |_| Ok(Some(team.clone())));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.get_team(team_id).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, expected.id);
}

#[tokio::test]
async fn test_get_team_not_found() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let team_id = LeagueTeamId::new();

    team_repo.expect_find_by_id().returning(|_| Ok(None));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.get_team(team_id).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::LeagueTeamNotFound(_)
    ));
}

#[tokio::test]
async fn test_create_team_success() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let owner_player_id = PlayerId::new();

    // Season lookup
    let season_clone = season.clone();
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season_clone.clone())));

    // Check name/tag uniqueness
    team_repo
        .expect_name_exists()
        .returning(|_, _| Ok(false));
    team_repo.expect_tag_exists().returning(|_, _| Ok(false));

    // Check player not already in a team this season
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));

    // Atomic create: team + team_season + captain member in one transaction.
    let team = make_team(league_id);
    let team_id = team.id;
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let result_clone = (team.clone(), team_season.clone());
    team_repo
        .expect_create_team_with_season_and_captain()
        .returning(move |_, _, _| Ok(result_clone.clone()));

    // `make_member` was previously used to seed the separate add_member mock;
    // keep it here so the test still exercises the helper but discard the
    // value since the atomic method doesn't return the member.
    let _ = make_member(team_season_id, owner_player_id);

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .create_team(
            owner_player_id,
            CreateLeagueTeamCommand {
                league_id,
                season_id,
                name: "Test Team".to_string(),
                tag: "TST".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
            },
        )
        .await;

    assert!(result.is_ok());
    let (created_team, created_ts) = result.unwrap();
    assert_eq!(created_team.id, team_id);
    assert_eq!(created_ts.id, team_season_id);
}

#[tokio::test]
async fn test_create_team_season_not_in_registration() {
    let team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Completed; // Not accepting registrations
    let season_id = season.id;
    let owner_player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .create_team(
            owner_player_id,
            CreateLeagueTeamCommand {
                league_id,
                season_id,
                name: "Test Team".to_string(),
                tag: "TST".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
            },
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::RegistrationClosed
    ));
}

#[tokio::test]
async fn test_create_team_name_taken() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let owner_player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    // Player not in another team (checked before name)
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));

    team_repo
        .expect_name_exists()
        .returning(|_, _| Ok(true)); // Name already taken

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .create_team(
            owner_player_id,
            CreateLeagueTeamCommand {
                league_id,
                season_id,
                name: "Existing Team".to_string(),
                tag: "NEW".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
            },
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::Conflict(_)));
}

#[tokio::test]
async fn test_create_team_player_already_in_team() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let owner_player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    team_repo
        .expect_name_exists()
        .returning(|_, _| Ok(false));
    team_repo.expect_tag_exists().returning(|_, _| Ok(false));

    // Player already in another team
    member_repo
        .expect_find_primary_team_in_season()
        .returning(move |_, _| Ok(Some(LeagueTeamSeasonId::new())));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .create_team(
            owner_player_id,
            CreateLeagueTeamCommand {
                league_id,
                season_id,
                name: "New Team".to_string(),
                tag: "NEW".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
            },
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::Conflict(_)));
}

#[tokio::test]
async fn test_transfer_ownership_success() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let old_owner_id = team.owner_player_id;
    let team_id = team.id;
    let new_owner_id = PlayerId::new();

    // Get team
    team_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team.clone())));

    // Get active team seasons
    let team_season = make_team_season(team_id, LeagueSeasonId::new());
    let team_season_id = team_season.id;
    team_season_repo
        .expect_list_by_team()
        .returning(move |_| Ok(vec![team_season.clone()]));

    // Check new owner is member
    let member = make_member(team_season_id, new_owner_id);
    member_repo
        .expect_find_member()
        .returning(move |_, _| Ok(Some(member.clone())));

    // Transfer ownership
    let mut updated_team = make_team(league_id);
    updated_team.owner_player_id = new_owner_id;
    team_repo
        .expect_transfer_ownership()
        .returning(move |_, _| Ok(updated_team.clone()));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .transfer_ownership(team_id, old_owner_id, new_owner_id)
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().owner_player_id, new_owner_id);
}

#[tokio::test]
async fn test_transfer_ownership_not_owner() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let team_id = team.id;
    let wrong_caller_id = PlayerId::new(); // Not the owner
    let new_owner_id = PlayerId::new();

    team_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team.clone())));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service
        .transfer_ownership(team_id, wrong_caller_id, new_owner_id)
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::NotAuthorized(_)));
}

#[tokio::test]
async fn test_leave_team_success() {
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team_id = LeagueTeamId::new();
    let season_id = LeagueSeasonId::new();
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();

    // Get team season
    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));

    // Get season (to check roster lock)
    let season = make_season(league_id);
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    // Find member
    let member = make_member(team_season_id, player_id);
    member_repo
        .expect_find_member()
        .returning(move |_, _| Ok(Some(member.clone())));

    // Count captains (more than one)
    member_repo.expect_count_captains().returning(|_| Ok(2));

    // Remove member
    member_repo.expect_remove_member().returning(|_, _| Ok(()));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.leave_team(team_season_id, player_id).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_leave_team_last_captain() {
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team_id = LeagueTeamId::new();
    let season_id = LeagueSeasonId::new();
    let team_season = make_team_season(team_id, season_id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();

    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));

    let season = make_season(league_id);
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    // Member is captain
    let mut member = make_member(team_season_id, player_id);
    member.role = LeagueTeamRole::Captain;
    member_repo
        .expect_find_member()
        .returning(move |_, _| Ok(Some(member.clone())));

    // Only one captain
    member_repo.expect_count_captains().returning(|_| Ok(1));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.leave_team(team_season_id, player_id).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::Conflict(_)));
}

#[tokio::test]
async fn test_promote_to_captain_success() {
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let team_id = LeagueTeamId::new();
    let team_season = make_team_season(team_id, LeagueSeasonId::new());
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();

    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));

    let member = make_member(team_season_id, player_id);
    let mut promoted = member.clone();
    promoted.role = LeagueTeamRole::Captain;

    member_repo
        .expect_find_member()
        .returning(move |_, _| Ok(Some(member.clone())));

    member_repo
        .expect_update_role()
        .returning(move |_, _, _| Ok(promoted.clone()));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.promote_to_captain(team_season_id, player_id).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().role, LeagueTeamRole::Captain);
}

#[tokio::test]
async fn test_disband_team_success() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let member_repo = MockLeagueTeamMemberRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let team = make_team(league_id);
    let team_id = team.id;
    let team_clone = team.clone();

    team_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_clone.clone())));

    let mut disbanded_team = team.clone();
    disbanded_team.status = LeagueTeamStatus::Disbanded;
    team_repo
        .expect_update_status()
        .returning(move |_, _| Ok(disbanded_team.clone()));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    let result = service.disband_team(team_id).await;

    assert!(result.is_ok());
}
