//! Tests for LeagueSeasonParticipantService.

use super::helpers::{make_participant, make_season};
use crate::entities::league_team::LeagueSeasonParticipantStatus;
use crate::repositories::league_team::{
    MockLeagueSeasonParticipantRepository, MockLeagueSeasonRepository,
};
use crate::services::league_team::LeagueSeasonParticipantService;
use portal_core::types::SeasonStatus;
use portal_core::{DomainError, LeagueId, LeagueSeasonId, PlayerId};
use std::sync::Arc;

fn create_service(
    participant_repo: MockLeagueSeasonParticipantRepository,
    season_repo: MockLeagueSeasonRepository,
) -> LeagueSeasonParticipantService<MockLeagueSeasonParticipantRepository, MockLeagueSeasonRepository>
{
    LeagueSeasonParticipantService::new(Arc::new(participant_repo), Arc::new(season_repo))
}

#[tokio::test]
async fn test_register_success() {
    let mut participant_repo = MockLeagueSeasonParticipantRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    participant_repo
        .expect_is_registered()
        .returning(|_, _| Ok(false));

    let participant = make_participant(season_id, player_id);
    let expected_id = participant.id;
    participant_repo
        .expect_register()
        .returning(move |_| Ok(participant.clone()));

    let service = create_service(participant_repo, season_repo);

    let result = service.register(season_id, player_id).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, expected_id);
}

#[tokio::test]
async fn test_register_already_registered() {
    let mut participant_repo = MockLeagueSeasonParticipantRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    participant_repo
        .expect_is_registered()
        .returning(|_, _| Ok(true)); // Already registered

    let service = create_service(participant_repo, season_repo);

    let result = service.register(season_id, player_id).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::Conflict(_)));
}

#[tokio::test]
async fn test_register_season_closed() {
    let participant_repo = MockLeagueSeasonParticipantRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Completed; // Not accepting registrations
    let season_id = season.id;
    let player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    let service = create_service(participant_repo, season_repo);

    let result = service.register(season_id, player_id).await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::RegistrationClosed
    ));
}

#[tokio::test]
async fn test_withdraw_success() {
    let mut participant_repo = MockLeagueSeasonParticipantRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let _league_id = LeagueId::new();
    let season_id = LeagueSeasonId::new();
    let player_id = PlayerId::new();

    let participant = make_participant(season_id, player_id);
    let participant_id = participant.id;

    // Find by id
    participant_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(participant.clone())));

    // Withdraw returns updated participant
    let mut withdrawn = make_participant(season_id, player_id);
    withdrawn.status = LeagueSeasonParticipantStatus::Withdrawn;
    participant_repo
        .expect_withdraw()
        .returning(move |_| Ok(withdrawn.clone()));

    let service = create_service(participant_repo, season_repo);

    let result = service.withdraw(participant_id, player_id).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_registered() {
    let mut participant_repo = MockLeagueSeasonParticipantRepository::new();
    let season_repo = MockLeagueSeasonRepository::new();

    let season_id = LeagueSeasonId::new();
    let player_id = PlayerId::new();

    participant_repo
        .expect_is_registered()
        .returning(|_, _| Ok(true));

    let service = create_service(participant_repo, season_repo);

    let result = service.is_registered(season_id, player_id).await;

    assert!(result.is_ok());
    assert!(result.unwrap());
}
