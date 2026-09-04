//! Tests for LeagueTeamService.

use super::helpers::{make_member, make_season, make_team, make_team_season};
use crate::entities::league_team::CreateLeagueTeamCommand;
use crate::repositories::audit::MockEntityChangeRepository;
use crate::repositories::league_team::{
    MockLeagueSeasonRepository, MockLeagueTeamMemberRepository, MockLeagueTeamRepository,
    MockLeagueTeamSeasonRepository,
};
use crate::services::league_team::LeagueTeamService;
use portal_core::types::{LeagueTeamRole, LeagueTeamStatus, SeasonStatus};
use portal_core::{
    DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId,
};
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
        // No test in this file exercises the admin override, so a bare mock
        // that expects nothing is correct: any audit write would be an
        // unexpected call and would fail the test.
        Arc::new(MockEntityChangeRepository::new()),
        // §9.3: fixtures stage members-in-good-standing; the membership
        // check defaults to true. The refusal side is integration-pinned.
        Arc::new({
            let mut m = crate::repositories::league::MockLeagueMemberRepository::new();
            m.expect_is_member_by_player().returning(|_, _| Ok(true));
            m
        }),
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
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
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
    team_repo.expect_name_exists().returning(|_, _| Ok(false));
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
        DomainError::LeagueSeasonNotOpen
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

    team_repo.expect_name_exists().returning(|_, _| Ok(true)); // Name already taken

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

    team_repo.expect_name_exists().returning(|_, _| Ok(false));
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
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = make_season(league_id);
    let team_id = LeagueTeamId::new();
    let team_season = make_team_season(team_id, season.id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();

    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));

    // Promotion is now lock-checked (P-16), so the season is resolved.
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

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

    let result = service
        .promote_to_captain(team_season_id, player_id, None)
        .await;

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

// ============================================================================
// P-148 / P-147 — the roster lock is the control, and founding goes through it
// ============================================================================
//
// The owner's ruling: *"I think the roster lock should really be an 'optional'
// thing/thing that is a per tournament decision, (again, this is a casual
// league, so adding team members half way through may be okay)."*
//
// What the rule WAS: `LeagueSeason::allows_*_roster_changes()` ANDed the lock
// with `SeasonStatus::allows_roster_changes()` (`draft | registration`), so
// season phase was the outer gate and the lock only had a say before the
// competition started.
//
// What the rule IS: the lock decides. Every non-terminal phase defers to
// `roster_lock_status`; `completed` / `cancelled` refuse everything.
//
// The granularity is per **league season** (`league_seasons.roster_lock_status`,
// migration 0025). There is no tournament-level roster lock in the schema.

/// Build the four repo mocks for a successful `add_member_authorized` of a
/// primary member onto `season`, so the tests below differ only in the season.
fn add_member_service_for(
    season: crate::entities::league_team::LeagueSeason,
) -> (
    LeagueTeamService<
        MockLeagueTeamRepository,
        MockLeagueTeamSeasonRepository,
        MockLeagueTeamMemberRepository,
        MockLeagueSeasonRepository,
    >,
    LeagueTeamSeasonId,
    PlayerId,
) {
    let team_repo = MockLeagueTeamRepository::new();
    let mut team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let team_season = make_team_season(LeagueTeamId::new(), season.id);
    let team_season_id = team_season.id;
    let player_id = PlayerId::new();

    team_season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(team_season.clone())));
    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));

    member_repo.expect_is_member().returning(|_, _| Ok(false));
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));
    member_repo
        .expect_count_primary_members()
        .returning(|_| Ok(0));
    member_repo.expect_count_substitutes().returning(|_| Ok(0));

    let seated = make_member(team_season_id, player_id);
    member_repo
        .expect_add_member()
        .returning(move |_| Ok(seated.clone()));

    (
        create_service(team_repo, team_season_repo, member_repo, season_repo),
        team_season_id,
        player_id,
    )
}

fn season_in(
    status: SeasonStatus,
    lock: portal_core::types::RosterLockStatus,
) -> crate::entities::league_team::LeagueSeason {
    let mut season = make_season(LeagueId::new());
    season.status = status;
    season.roster_lock_status = lock;
    season
}

/// P-148 — **this is the assertion that failed before the ruling was
/// implemented.** A casual league leaves the lock `open` (the DB default) and
/// must be able to add a player once the season is under way.
#[tokio::test]
async fn test_open_lock_permits_a_roster_change_mid_season() {
    for status in [SeasonStatus::Active, SeasonStatus::Playoffs] {
        let (service, team_season_id, player_id) = add_member_service_for(season_in(
            status,
            portal_core::types::RosterLockStatus::Open,
        ));

        let result = service
            .add_member_authorized(
                team_season_id,
                player_id,
                LeagueTeamRole::Player,
                portal_core::UserId::new(),
                None,
            )
            .await;

        assert!(
            result.is_ok(),
            "an open lock refused a roster change in a '{status}' season: {:?}",
            result.err()
        );
    }
}

/// P-148 — a league that wants strictness sets the lock, and the lock is what
/// bites. Not the phase: the season here is `active`, which the old rule froze
/// on its own.
#[tokio::test]
async fn test_hard_lock_refuses_a_roster_change_mid_season() {
    let (service, team_season_id, player_id) = add_member_service_for(season_in(
        SeasonStatus::Active,
        portal_core::types::RosterLockStatus::HardLock,
    ));

    let err = service
        .add_member_authorized(
            team_season_id,
            player_id,
            LeagueTeamRole::Player,
            portal_core::UserId::new(),
            None,
        )
        .await
        .expect_err("a hard lock must freeze an active season's roster");

    match err {
        DomainError::InvalidState(msg) => assert!(
            msg.contains("roster is locked"),
            "refusal must name the lock, not the phase: {msg}"
        ),
        other => panic!("expected InvalidState, got {other:?}"),
    }
}

/// P-148 — `soft_lock` keeps its documented meaning ("substitutes only") in a
/// live season: the substitute goes on, the primary member does not.
#[tokio::test]
async fn test_soft_lock_permits_substitutes_only_mid_season() {
    let (service, team_season_id, player_id) = add_member_service_for(season_in(
        SeasonStatus::Active,
        portal_core::types::RosterLockStatus::SoftLock,
    ));
    assert!(
        service
            .add_member_authorized(
                team_season_id,
                player_id,
                LeagueTeamRole::Substitute,
                portal_core::UserId::new(),
                None,
            )
            .await
            .is_ok(),
        "a soft lock must still admit substitutes in an active season"
    );

    let (service, team_season_id, player_id) = add_member_service_for(season_in(
        SeasonStatus::Active,
        portal_core::types::RosterLockStatus::SoftLock,
    ));
    assert!(
        matches!(
            service
                .add_member_authorized(
                    team_season_id,
                    player_id,
                    LeagueTeamRole::Player,
                    portal_core::UserId::new(),
                    None,
                )
                .await,
            Err(DomainError::InvalidState(_))
        ),
        "a soft lock must freeze the primary roster in an active season"
    );
}

/// P-148 — the one status rule that survives. "Optional" was never meant to
/// include editing the record of a season that has already been played, so a
/// terminal season refuses even with the lock wide open.
#[tokio::test]
async fn test_a_terminal_season_refuses_roster_changes_whatever_the_lock_says() {
    for status in [SeasonStatus::Completed, SeasonStatus::Cancelled] {
        let (service, team_season_id, player_id) = add_member_service_for(season_in(
            status,
            portal_core::types::RosterLockStatus::Open,
        ));

        let err = service
            .add_member_authorized(
                team_season_id,
                player_id,
                LeagueTeamRole::Player,
                portal_core::UserId::new(),
                None,
            )
            .await
            .err()
            .unwrap_or_else(|| {
                panic!("a '{status}' season gained a member; history is not editable")
            });

        match err {
            DomainError::InvalidState(msg) => assert!(
                msg.contains(&format!("season status '{status}'")),
                "a terminal refusal must name the status, not the (open) lock: {msg}"
            ),
            other => panic!("expected InvalidState, got {other:?}"),
        }
    }
}

/// P-147 — founding a roster goes through the enforcement point, and the
/// enforcement point rules it exempt from the lock: whether a season takes new
/// teams is decided once, by `can_register_team()`. A hard-locked season that
/// is still inside its registration window therefore still accepts a new team.
#[tokio::test]
async fn test_founding_a_team_is_governed_by_registration_not_by_the_lock() {
    let mut team_repo = MockLeagueTeamRepository::new();
    let team_season_repo = MockLeagueTeamSeasonRepository::new();
    let mut member_repo = MockLeagueTeamMemberRepository::new();
    let mut season_repo = MockLeagueSeasonRepository::new();

    let league_id = LeagueId::new();
    let season = season_in(
        SeasonStatus::Registration,
        portal_core::types::RosterLockStatus::HardLock,
    );
    let season_id = season.id;
    let owner_player_id = PlayerId::new();

    season_repo
        .expect_find_by_id()
        .returning(move |_| Ok(Some(season.clone())));
    team_repo.expect_name_exists().returning(|_, _| Ok(false));
    team_repo.expect_tag_exists().returning(|_, _| Ok(false));
    member_repo
        .expect_find_primary_team_in_season()
        .returning(|_, _| Ok(None));

    let team = make_team(league_id);
    let team_season = make_team_season(team.id, season_id);
    let created = (team, team_season);
    team_repo
        .expect_create_team_with_season_and_captain()
        .returning(move |_, _, _| Ok(created.clone()));

    let service = create_service(team_repo, team_season_repo, member_repo, season_repo);

    assert!(
        service
            .create_team(
                owner_player_id,
                CreateLeagueTeamCommand {
                    league_id,
                    season_id,
                    name: "Late Arrival".to_string(),
                    tag: "LAT".to_string(),
                    description: None,
                    logo_url: None,
                    primary_color: None,
                    secondary_color: None,
                },
            )
            .await
            .is_ok(),
        "the lock must not become a second, invisible veto on registration"
    );
}
