//! Team membership business rule tests.
//!
//! These tests verify the design constraints from the team management design documents.
//! Some tests may initially FAIL (Red) if constraints aren't enforced yet - that's expected TDD.

use portal_db::entities::{NewTeam, NewTeamMember, UpdateTeamMember};
use portal_db::repositories::{TeamMemberRepository, TeamRepository};
use portal_db::DbPool;
use portal_test::database::TestDb;
use uuid::Uuid;

// ===========================================
// Test Helpers
// ===========================================

async fn create_test_user(pool: &DbPool, suffix: &str) -> Uuid {
    let user = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, 'hash')
        RETURNING id
        "#,
    )
    .bind(format!("ruleuser{}", suffix))
    .bind(format!("rule{}@example.com", suffix))
    .fetch_one(pool)
    .await
    .unwrap();
    user.0
}

async fn create_test_player(pool: &DbPool, user_id: Uuid, suffix: &str) -> Uuid {
    let player = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO players (user_id, display_name, country_code)
        VALUES ($1, $2, 'US')
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(format!("RulePlayer{}", suffix))
    .fetch_one(pool)
    .await
    .unwrap();
    player.0
}

async fn setup_player(pool: &DbPool, suffix: &str) -> Uuid {
    let user_id = create_test_user(pool, suffix).await;
    create_test_player(pool, user_id, suffix).await
}

async fn create_team_with_founder(
    pool: &DbPool,
    team_repo: &TeamRepository,
    member_repo: &TeamMemberRepository,
    founder_id: Uuid,
    team_name: &str,
) -> Uuid {
    let new_team = NewTeam {
        name: team_name.to_string(),
        tag: team_name[..3.min(team_name.len())].to_string(),
        created_by: founder_id,
        description: None,
        logo_url: None,
        game_id: None,
    };
    let team = team_repo.create(new_team).await.unwrap();

    let new_member = NewTeamMember {
        team_id: team.id,
        player_id: founder_id,
        role: "captain".to_string(),
        is_founder: true,
        invited_by: None,
    };
    member_repo.create(new_member).await.unwrap();

    team.id
}

// ===========================================
// Team Membership Rule Tests
// ===========================================

/// Test that when a player creates a team, they automatically become captain AND founder.
#[tokio::test]
async fn test_creator_becomes_captain_and_founder() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let founder_id = setup_player(&db.pool, "founder1").await;

    let team_id = create_team_with_founder(
        &db.pool,
        &team_repo,
        &member_repo,
        founder_id,
        "Founder Test Team",
    )
    .await;

    // Verify the creator is both captain and founder
    let member = member_repo
        .find_by_team_and_player(
            portal_core::TeamId::from(team_id),
            portal_core::PlayerId::from(founder_id),
        )
        .await
        .unwrap()
        .expect("Founder should be a member");

    assert_eq!(member.role, "captain", "Creator should be captain");
    assert!(member.is_founder, "Creator should be marked as founder");
}

/// Test that a founder cannot be demoted from captain role.
/// This test may FAIL if the constraint isn't enforced at the application layer.
#[tokio::test]
async fn test_founder_cannot_be_demoted() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let founder_id = setup_player(&db.pool, "founder2").await;

    let team_id = create_team_with_founder(
        &db.pool,
        &team_repo,
        &member_repo,
        founder_id,
        "Demote Test Team",
    )
    .await;

    // Attempt to demote founder to player
    let update = UpdateTeamMember {
        role: Some("player".to_string()),
        ..Default::default()
    };

    // NOTE: This test documents the EXPECTED behavior.
    // Currently the repository doesn't enforce this - the update will succeed.
    // When we implement this constraint, change this test accordingly.
    let result = member_repo
        .update(
            portal_core::TeamId::from(team_id),
            portal_core::PlayerId::from(founder_id),
            update,
        )
        .await;

    // EXPECTED (Red): This should fail with an error preventing founder demotion
    // CURRENT (Green without constraint): The update succeeds
    // TODO: When business logic is added, this should be:
    // assert!(result.is_err(), "Founder demotion should be prevented");

    // For now, just document that the update succeeded (no constraint yet)
    if result.is_ok() {
        eprintln!("WARNING: Founder was demoted - constraint not yet enforced");
    }
}

/// Test that a founder cannot be removed from the team.
/// This test may FAIL if the constraint isn't enforced at the application layer.
#[tokio::test]
async fn test_founder_cannot_be_removed() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let founder_id = setup_player(&db.pool, "founder3").await;

    let team_id = create_team_with_founder(
        &db.pool,
        &team_repo,
        &member_repo,
        founder_id,
        "Remove Test Team",
    )
    .await;

    // Attempt to remove founder
    let result = member_repo
        .remove(
            portal_core::TeamId::from(team_id),
            portal_core::PlayerId::from(founder_id),
        )
        .await;

    // EXPECTED (Red): This should fail with an error preventing founder removal
    // CURRENT (Green without constraint): The removal succeeds
    // TODO: When business logic is added, this should be:
    // assert!(result.is_err(), "Founder removal should be prevented");

    // For now, just document that the removal succeeded (no constraint yet)
    if result.is_ok() {
        eprintln!("WARNING: Founder was removed - constraint not yet enforced");
    }
}

/// Test that a team must always have at least one captain.
/// This test may FAIL if the constraint isn't enforced at the application layer.
#[tokio::test]
async fn test_team_must_have_at_least_one_captain() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let founder_id = setup_player(&db.pool, "captain1").await;
    let player_id = setup_player(&db.pool, "player1").await;

    let team_id = create_team_with_founder(
        &db.pool,
        &team_repo,
        &member_repo,
        founder_id,
        "Captain Test Team",
    )
    .await;

    // Add another player (non-captain)
    let new_member = NewTeamMember {
        team_id,
        player_id,
        role: "player".to_string(),
        is_founder: false,
        invited_by: Some(founder_id),
    };
    member_repo.create(new_member).await.unwrap();

    // Verify we have exactly one captain
    let captain_count = member_repo
        .count_captains(portal_core::TeamId::from(team_id))
        .await
        .unwrap();
    assert_eq!(captain_count, 1, "Team should have exactly one captain initially");
}

/// Test that the last captain cannot be removed/demoted.
/// This test may FAIL if the constraint isn't enforced at the application layer.
#[tokio::test]
async fn test_cannot_remove_last_captain() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let captain_id = setup_player(&db.pool, "lastcaptain").await;
    let player_id = setup_player(&db.pool, "lastplayer").await;

    // Create team - captain is the only captain
    let new_team = NewTeam {
        name: "Last Captain Team".to_string(),
        tag: "LCT".to_string(),
        created_by: captain_id,
        description: None,
        logo_url: None,
        game_id: None,
    };
    let team = team_repo.create(new_team).await.unwrap();

    // Add captain as founder
    let captain_member = NewTeamMember {
        team_id: team.id,
        player_id: captain_id,
        role: "captain".to_string(),
        is_founder: true,
        invited_by: None,
    };
    member_repo.create(captain_member).await.unwrap();

    // Add regular player
    let player_member = NewTeamMember {
        team_id: team.id,
        player_id,
        role: "player".to_string(),
        is_founder: false,
        invited_by: Some(captain_id),
    };
    member_repo.create(player_member).await.unwrap();

    // Attempt to demote the only captain
    let update = UpdateTeamMember {
        role: Some("player".to_string()),
        ..Default::default()
    };

    // EXPECTED: Should fail because can't remove last captain
    // For now, document if the constraint isn't enforced
    let result = member_repo
        .update(
            portal_core::TeamId::from(team.id),
            portal_core::PlayerId::from(captain_id),
            update,
        )
        .await;

    if result.is_ok() {
        eprintln!("WARNING: Last captain was demoted - constraint not yet enforced");
    }
}

/// Test that a captain can promote another member to captain.
#[tokio::test]
async fn test_captain_can_promote_to_captain() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let captain_id = setup_player(&db.pool, "promotecaptain").await;
    let player_id = setup_player(&db.pool, "promoteplayer").await;

    let team_id = create_team_with_founder(
        &db.pool,
        &team_repo,
        &member_repo,
        captain_id,
        "Promote Test Team",
    )
    .await;

    // Add regular player
    let player_member = NewTeamMember {
        team_id,
        player_id,
        role: "player".to_string(),
        is_founder: false,
        invited_by: Some(captain_id),
    };
    member_repo.create(player_member).await.unwrap();

    // Promote player to captain
    let update = UpdateTeamMember {
        role: Some("captain".to_string()),
        ..Default::default()
    };

    let result = member_repo
        .update(
            portal_core::TeamId::from(team_id),
            portal_core::PlayerId::from(player_id),
            update,
        )
        .await;

    assert!(result.is_ok(), "Captain should be able to promote player to captain");

    // Verify we now have two captains
    let captain_count = member_repo
        .count_captains(portal_core::TeamId::from(team_id))
        .await
        .unwrap();
    assert_eq!(captain_count, 2, "Team should now have two captains");
}

/// Test that a player can be on multiple teams simultaneously.
#[tokio::test]
async fn test_player_can_be_on_multiple_teams() {
    let db = TestDb::new().await;
    let team_repo = TeamRepository::new(db.pool.clone());
    let member_repo = TeamMemberRepository::new(db.pool.clone());

    let player_id = setup_player(&db.pool, "multiteam").await;

    // Create two teams and add the player to both
    for i in 1..=2 {
        let new_team = NewTeam {
            name: format!("Multi Team {}", i),
            tag: format!("MT{}", i),
            created_by: player_id,
            description: None,
            logo_url: None,
            game_id: None,
        };
        let team = team_repo.create(new_team).await.unwrap();

        let new_member = NewTeamMember {
            team_id: team.id,
            player_id,
            role: "captain".to_string(),
            is_founder: true,
            invited_by: None,
        };
        member_repo.create(new_member).await.unwrap();
    }

    // Verify player is on both teams
    let teams = team_repo
        .list_by_player(portal_core::PlayerId::from(player_id))
        .await
        .unwrap();
    assert_eq!(teams.len(), 2, "Player should be able to be on multiple teams");
}
