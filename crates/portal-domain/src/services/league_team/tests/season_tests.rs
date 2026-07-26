//! Tests for LeagueSeasonService.

use super::helpers::{make_league, make_season};
use crate::entities::league_team::CreateLeagueSeasonCommand;
use crate::repositories::league::MockLeagueRepository;
use crate::repositories::league_team::MockLeagueSeasonRepository;
use crate::services::league_team::LeagueSeasonService;
use portal_core::types::{RosterLockStatus, SeasonStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, UserId};
use std::sync::Arc;

#[tokio::test]
async fn test_get_season_found() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let season_id = season.id;
    let expected = season.clone();

    season_repo
        .expect_find_by_id()
        .with(mockall::predicate::eq(season_id))
        .returning(move |_| Ok(Some(season.clone())));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service.get_season(season_id).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, expected.id);
}

#[tokio::test]
async fn test_get_season_not_found() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let season_id = LeagueSeasonId::new();

    season_repo.expect_find_by_id().returning(|_| Ok(None));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service.get_season(season_id).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::LeagueSeasonNotFound(_)
    ));
}

#[tokio::test]
async fn test_create_season_success() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let mut league_repo = MockLeagueRepository::new();

    let league = make_league();
    let league_id = league.id;
    let creator_id = UserId::new();

    // Expect league lookup
    league_repo
        .expect_find_by_id()
        .with(mockall::predicate::eq(league_id))
        .returning(move |_| Ok(Some(league.clone())));

    // Expect slug check
    season_repo.expect_slug_exists().returning(|_, _| Ok(false));

    // Expect create
    let expected_season = make_season(league_id);
    let expected_id = expected_season.id;
    season_repo
        .expect_create()
        .returning(move |_| Ok(expected_season.clone()));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service
        .create_season(
            creator_id,
            CreateLeagueSeasonCommand {
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
            },
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().id, expected_id);
}

#[tokio::test]
async fn test_create_season_league_not_found() {
    let season_repo = MockLeagueSeasonRepository::new();
    let mut league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let creator_id = UserId::new();

    league_repo.expect_find_by_id().returning(|_| Ok(None));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service
        .create_season(
            creator_id,
            CreateLeagueSeasonCommand {
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
            },
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::LeagueNotFound(_)
    ));
}

#[tokio::test]
async fn test_create_season_slug_taken() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let mut league_repo = MockLeagueRepository::new();

    let league = make_league();
    let league_id = league.id;
    let creator_id = UserId::new();

    league_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(league.clone())));

    season_repo.expect_slug_exists().returning(|_, _| Ok(true)); // Slug already taken

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service
        .create_season(
            creator_id,
            CreateLeagueSeasonCommand {
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
            },
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::Conflict(_)));
}

#[tokio::test]
async fn test_update_roster_lock_success() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Registration; // Season must be in registration to lock roster
    let season_id = season.id;
    let locker_id = UserId::new();

    let original_season = season.clone();
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(original_season.clone())));

    let mut locked_season = season.clone();
    locked_season.roster_lock_status = RosterLockStatus::HardLock;
    let expected = locked_season.clone();

    season_repo
        .expect_update_roster_lock()
        .returning(move |_, _, _| Ok(locked_season.clone()));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service
        .update_roster_lock(season_id, RosterLockStatus::HardLock, locker_id)
        .await;

    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().roster_lock_status,
        expected.roster_lock_status
    );
}

#[tokio::test]
async fn test_update_roster_lock_invalid_state() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Completed; // Can't lock roster in completed season
    let season_id = season.id;
    let locker_id = UserId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let result = service
        .update_roster_lock(season_id, RosterLockStatus::HardLock, locker_id)
        .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::InvalidState(_)));
}

/// P-14 — `update_season` must forward `roster_lock_status`, and must do it
/// through `update_roster_lock` so the `roster_locked_by` audit column is
/// stamped. Before the fix the field was accepted, validated and then silently
/// dropped, which left every roster-lock check downstream unreachable.
#[tokio::test]
async fn test_update_season_forwards_the_roster_lock_with_the_actor() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Registration;
    let season_id = season.id;
    let actor = UserId::new();

    let found = season.clone();
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(found.clone())));

    // P-198: the lock rides in the SAME repo update as the generic fields —
    // one atomic statement, not a second call a mid-flight error could
    // split. The pin is therefore on the update command carrying the lock
    // and the actor.
    let mut locked = season.clone();
    locked.roster_lock_status = RosterLockStatus::HardLock;
    season_repo
        .expect_update()
        .withf(move |_, update| {
            update.roster_lock_status == Some(RosterLockStatus::HardLock)
                && update.roster_locked_by == Some(actor)
        })
        .times(1)
        .returning(move |_, _| Ok(locked.clone()));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let cmd = crate::entities::league_team::UpdateLeagueSeasonCommand {
        roster_lock_status: Some(RosterLockStatus::HardLock),
        ..Default::default()
    };

    let result = service.update_season(season_id, cmd, actor).await;

    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().roster_lock_status,
        RosterLockStatus::HardLock
    );
}

/// P-14 — a lock the season state forbids is rejected **before** the generic
/// field update runs, so a refused PATCH does not half-apply the rest of the
/// body. `expect_update` is never set up, so any call to it fails the test.
#[tokio::test]
async fn test_update_season_rejects_an_illegal_lock_without_writing_anything() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Completed;
    let season_id = season.id;

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));
    season_repo.expect_update().never();
    season_repo.expect_update_roster_lock().never();

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let cmd = crate::entities::league_team::UpdateLeagueSeasonCommand {
        name: Some("Renamed Season".to_string()),
        roster_lock_status: Some(RosterLockStatus::HardLock),
        ..Default::default()
    };

    let result = service.update_season(season_id, cmd, UserId::new()).await;

    assert!(matches!(result.unwrap_err(), DomainError::InvalidState(_)));
}

/// P-14 — a combined `status` + `roster_lock_status` PATCH is validated against
/// the status the request moves TO. Locking a season as it enters registration
/// must work even though it is leaving `draft`.
///
/// **Spec change (P-148 — the owner's ruling that "the roster lock should
/// really be an 'optional' thing ... this is a casual league, so adding team
/// members half way through may be okay").** This test previously moved the
/// season to `active` and asserted the combined PATCH was REFUSED, because
/// `active` did not allow roster changes and so could not carry a lock. Under
/// the ruling that is precisely backwards: locking a season as it goes live is
/// the canonical use of the control, so the same combined PATCH must now
/// SUCCEED and forward the lock. The refusal case it used to cover is kept, and
/// moved onto the status that still refuses — see
/// `test_update_season_refuses_a_lock_on_a_season_being_completed` below.
#[tokio::test]
async fn test_update_season_validates_the_lock_against_the_incoming_status() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Registration;
    let season_id = season.id;

    let found = season.clone();
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(found.clone())));

    // P-198/P-199: one atomic update carrying BOTH the status transition and
    // the lock; the lock is validated against the INCOMING status
    // (registration -> active is chain-legal, and hard-locking an active
    // season is P-201's whole point).
    let mut locked = season.clone();
    locked.status = SeasonStatus::Active;
    locked.roster_lock_status = RosterLockStatus::HardLock;
    season_repo
        .expect_update()
        .withf(|_, update| {
            update.status == Some(SeasonStatus::Active)
                && update.roster_lock_status == Some(RosterLockStatus::HardLock)
        })
        .times(1)
        .returning(move |_, _| Ok(locked.clone()));

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let cmd = crate::entities::league_team::UpdateLeagueSeasonCommand {
        status: Some(SeasonStatus::Active),
        roster_lock_status: Some(RosterLockStatus::HardLock),
        ..Default::default()
    };

    let result = service
        .update_season(season_id, cmd, UserId::new())
        .await
        .expect("locking a season as it goes live is the point of the control");
    assert_eq!(result.roster_lock_status, RosterLockStatus::HardLock);
    assert_eq!(result.status, SeasonStatus::Active);
}

/// P-148 — the surviving refusal. A combined PATCH is still validated against
/// the status it moves TO, but the only status that refuses a lock is a
/// terminal one: a finished season has nothing left to lock, and its roster is
/// frozen by the enforcement point whatever this column says. Refused before
/// the generic field update, so nothing half-applies.
#[tokio::test]
async fn test_update_season_refuses_a_lock_on_a_season_being_completed() {
    let mut season_repo = MockLeagueSeasonRepository::new();
    let league_repo = MockLeagueRepository::new();

    let league_id = LeagueId::new();
    let mut season = make_season(league_id);
    season.status = SeasonStatus::Active;
    let season_id = season.id;

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));
    season_repo.expect_update().never();
    season_repo.expect_update_roster_lock().never();

    let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

    let cmd = crate::entities::league_team::UpdateLeagueSeasonCommand {
        status: Some(SeasonStatus::Completed),
        roster_lock_status: Some(RosterLockStatus::HardLock),
        ..Default::default()
    };

    assert!(matches!(
        service
            .update_season(season_id, cmd, UserId::new())
            .await
            .unwrap_err(),
        DomainError::InvalidState(_)
    ));
}

/// P-148 — the lock must be settable on a season that has already started.
/// That is the case the old rule made impossible, and it is the one a league
/// that wants strictness actually needs: rosters go final when the competition
/// begins, not before.
#[tokio::test]
async fn test_roster_lock_can_be_tightened_on_a_live_season() {
    for status in [SeasonStatus::Active, SeasonStatus::Playoffs] {
        let mut season_repo = MockLeagueSeasonRepository::new();
        let league_repo = MockLeagueRepository::new();

        let league_id = LeagueId::new();
        let mut season = make_season(league_id);
        season.status = status;
        let season_id = season.id;

        let found = season.clone();
        season_repo
            .expect_find_by_id()
            .returning(move |_| Ok(Some(found.clone())));

        let mut locked = season.clone();
        locked.roster_lock_status = RosterLockStatus::HardLock;
        season_repo
            .expect_update_roster_lock()
            .times(1)
            .returning(move |_, _, _| Ok(locked.clone()));

        let service = LeagueSeasonService::new(Arc::new(season_repo), Arc::new(league_repo));

        let result = service
            .update_roster_lock(season_id, RosterLockStatus::HardLock, UserId::new())
            .await
            .unwrap_or_else(|e| panic!("a '{status}' season refused a hard lock: {e:?}"));
        assert_eq!(result.roster_lock_status, RosterLockStatus::HardLock);
    }
}
