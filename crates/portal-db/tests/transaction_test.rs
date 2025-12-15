//! Transaction tests for match completion.
//!
//! These tests verify that the transactional match completion:
//! - Completes all operations atomically
//! - Rolls back all changes on failure
//! - Properly handles concurrent transactions

use portal_core::types::{
    AdvancementRule, BracketType, MatchFormat, RegistrationType, SchedulingMode, StageFormat,
    TournamentFormat, TournamentMatchStatus, TournamentParticipantType,
    TournamentRegistrationStatus, WithdrawalPolicy,
};
use portal_core::{TournamentBracketId, TournamentMatchId, TournamentRegistrationId};
use portal_db::adapters::{
    PgTournamentBracketRepository, PgTournamentMatchRepository, PgTournamentRegistrationRepository,
    PgTournamentRepository, PgTournamentStageRepository,
};
use portal_db::{begin_transaction, complete_match_in_transaction, MatchCompletionTxInput};
use portal_domain::repositories::tournament::{
    CreateTournamentBracket, CreateTournamentMatch, CreateTournamentRegistration,
    CreateTournamentStage, TournamentBracketRepository, TournamentMatchRepository,
    TournamentRegistrationRepository, TournamentRepository, TournamentStageRepository,
};
use portal_test::database::TestDb;
use portal_test::prelude::*;

// =============================================================================
// TEST HELPERS
// =============================================================================

/// Create a tournament with all necessary structures for testing match completion.
async fn setup_tournament_with_match(
    pool: &portal_db::DbPool,
) -> (
    TournamentMatchId,
    TournamentBracketId,
    TournamentRegistrationId,
    TournamentRegistrationId,
) {
    let tournament_repo = PgTournamentRepository::new(pool.clone());
    let stage_repo = PgTournamentStageRepository::new(pool.clone());
    let bracket_repo = PgTournamentBracketRepository::new(pool.clone());
    let match_repo = PgTournamentMatchRepository::new(pool.clone());
    let registration_repo = PgTournamentRegistrationRepository::new(pool.clone());

    // Create a user for registration
    let unique = uuid::Uuid::new_v4();
    let user = UserBuilder::new()
        .username(&format!("txuser{}", &unique.to_string()[..8]))
        .email(&format!("tx{}@test.com", &unique.to_string()[..8]))
        .build_persisted(pool)
        .await;
    let user_id = portal_core::UserId::from_uuid(user.id);

    // Create a game (get existing CS2)
    let game_row =
        sqlx::query_as::<_, (uuid::Uuid,)>("SELECT id FROM games WHERE slug = 'cs2' LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("CS2 game should exist");
    let game_id = portal_core::GameId::from_uuid(game_row.0);

    // Create tournament
    let unique_slug = format!("tx-test-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let tournament = tournament_repo
        .create(portal_domain::repositories::tournament::CreateTournament {
            game_id,
            league_id: None,
            season_id: None,
            name: "Transaction Test Tournament".to_string(),
            slug: unique_slug,
            description: None,
            format: TournamentFormat::SingleElimination,
            format_settings: serde_json::json!({}),
            participant_type: TournamentParticipantType::Individual,
            team_size: None,
            min_participants: 2,
            max_participants: 8,
            registration_type: RegistrationType::Open,
            registration_start: None,
            registration_end: None,
            check_in_required: false,
            check_in_start: None,
            check_in_end: None,
            scheduling_mode: SchedulingMode::Live,
            starts_at: None,
            default_match_format: MatchFormat::Bo3,
            default_map_veto_format: None,
            withdrawal_policy: WithdrawalPolicy::Forfeit,
            rules_url: None,
            settings: serde_json::json!({}),
            created_by: user_id,
        })
        .await
        .expect("Failed to create tournament");

    // Create stage
    let stage = stage_repo
        .create(CreateTournamentStage {
            tournament_id: tournament.id,
            name: "Main Stage".to_string(),
            stage_order: 1,
            format: StageFormat::SingleElimination,
            format_settings: serde_json::json!({}),
            advancement_count: None,
            advancement_rule: AdvancementRule::TopN,
            match_format: Some(MatchFormat::Bo3),
            map_veto_format: None,
            starts_at: None,
            ends_at: None,
        })
        .await
        .expect("Failed to create stage");

    // Create bracket
    let bracket = bracket_repo
        .create(CreateTournamentBracket {
            stage_id: stage.id,
            tournament_id: tournament.id,
            name: "Main Bracket".to_string(),
            bracket_type: BracketType::SingleElim,
            total_rounds: 1,
            group_number: None,
        })
        .await
        .expect("Failed to create bracket");

    // Create two registrations
    let reg1 = registration_repo
        .create(CreateTournamentRegistration {
            tournament_id: tournament.id,
            team_season_id: None,
            player_id: Some(portal_core::PlayerId::from_uuid(user.id)),
            adhoc_team_id: None,
            participant_name: "Player 1".to_string(),
            participant_logo_url: None,
            registered_by: user_id,
            seed_rating: Some(1500),
        })
        .await
        .expect("Failed to create registration 1");

    // Create second user for second registration
    let user2 = UserBuilder::new()
        .username(&format!("txuser2{}", &uuid::Uuid::new_v4().to_string()[..8]))
        .email(&format!("tx2{}@test.com", &uuid::Uuid::new_v4().to_string()[..8]))
        .build_persisted(pool)
        .await;

    let reg2 = registration_repo
        .create(CreateTournamentRegistration {
            tournament_id: tournament.id,
            team_season_id: None,
            player_id: Some(portal_core::PlayerId::from_uuid(user2.id)),
            adhoc_team_id: None,
            participant_name: "Player 2".to_string(),
            participant_logo_url: None,
            registered_by: user_id,
            seed_rating: Some(1400),
        })
        .await
        .expect("Failed to create registration 2");

    // Approve registrations
    registration_repo
        .update_status(reg1.id, TournamentRegistrationStatus::Approved)
        .await
        .expect("Failed to approve reg1");
    registration_repo
        .update_status(reg2.id, TournamentRegistrationStatus::Approved)
        .await
        .expect("Failed to approve reg2");

    // Create a match
    let match_ = match_repo
        .create(CreateTournamentMatch {
            bracket_id: bracket.id,
            stage_id: stage.id,
            tournament_id: tournament.id,
            round: 1,
            match_number: 1,
            bracket_position: "W1-1".to_string(),
            participant1_registration_id: Some(reg1.id),
            participant2_registration_id: Some(reg2.id),
            participant1_name: Some("Player 1".to_string()),
            participant2_name: Some("Player 2".to_string()),
            participant1_logo_url: None,
            participant2_logo_url: None,
            participant1_seed: Some(1),
            participant2_seed: Some(2),
            participant1_source: None,
            participant2_source: None,
            match_format: MatchFormat::Bo3,
            maps_required: 3,
            winner_progresses_to: None,
            loser_progresses_to: None,
        })
        .await
        .expect("Failed to create match");

    // Transition match to Ready -> InProgress
    match_repo
        .update_status(match_.id, TournamentMatchStatus::Ready)
        .await
        .expect("Failed to set match ready");
    match_repo
        .update_status(match_.id, TournamentMatchStatus::InProgress)
        .await
        .expect("Failed to set match in progress");

    (match_.id, bracket.id, reg1.id, reg2.id)
}

// =============================================================================
// TRANSACTION SUCCESS TESTS
// =============================================================================

#[tokio::test]
async fn test_match_completion_transaction_success() {
    let db = TestDb::new().await;
    let (match_id, _bracket_id, reg1_id, reg2_id) = setup_tournament_with_match(&db.pool).await;

    // Start a transaction
    let mut tx = begin_transaction(&db.pool)
        .await
        .expect("Failed to begin transaction");

    // Complete the match in transaction
    let result = complete_match_in_transaction(
        &mut tx,
        MatchCompletionTxInput {
            match_id,
            winner_registration_id: reg1_id,
            loser_registration_id: reg2_id,
            winner_score: 2,
            loser_score: 1,
        },
    )
    .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Match completion should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    assert_eq!(output.match_id, match_id);
    // Single-elim finals have no next match
    assert!(output.winner_next_match_id.is_none());
    assert!(output.loser_next_match_id.is_none());
    // Single-elim doesn't need standings
    assert!(!output.standings_updated);

    // Commit the transaction
    tx.commit().await.expect("Failed to commit transaction");

    // Verify match is completed in the database
    let match_repo = PgTournamentMatchRepository::new(db.pool.clone());
    let match_ = match_repo
        .find_by_id(match_id)
        .await
        .expect("Query should succeed")
        .expect("Match should exist");

    assert_eq!(match_.status, TournamentMatchStatus::Completed);
    assert_eq!(match_.winner_registration_id, Some(reg1_id));
    assert_eq!(match_.loser_registration_id, Some(reg2_id));
    assert_eq!(match_.participant1_score, 2);
    assert_eq!(match_.participant2_score, 1);
}

// =============================================================================
// TRANSACTION ROLLBACK TESTS
// =============================================================================

#[tokio::test]
async fn test_match_completion_transaction_rollback_on_error() {
    let db = TestDb::new().await;
    let (match_id, _bracket_id, reg1_id, _reg2_id) = setup_tournament_with_match(&db.pool).await;

    // Start a transaction
    let mut tx = begin_transaction(&db.pool)
        .await
        .expect("Failed to begin transaction");

    // Try to complete with same winner and loser (should fail validation)
    let result = complete_match_in_transaction(
        &mut tx,
        MatchCompletionTxInput {
            match_id,
            winner_registration_id: reg1_id,
            loser_registration_id: reg1_id, // Same as winner - invalid!
            winner_score: 2,
            loser_score: 1,
        },
    )
    .await;

    // Should fail
    assert!(result.is_err(), "Should fail when winner equals loser");

    // Transaction should be rolled back (not committed)
    // Check that match is still in original state
    let match_repo = PgTournamentMatchRepository::new(db.pool.clone());
    let match_ = match_repo
        .find_by_id(match_id)
        .await
        .expect("Query should succeed")
        .expect("Match should exist");

    // Match should still be in progress (not completed)
    assert_eq!(
        match_.status,
        TournamentMatchStatus::InProgress,
        "Match should still be in progress after failed transaction"
    );
    assert!(match_.winner_registration_id.is_none());
}

#[tokio::test]
async fn test_match_completion_transaction_rollback_on_invalid_participant() {
    let db = TestDb::new().await;
    let (match_id, _bracket_id, _reg1_id, _reg2_id) = setup_tournament_with_match(&db.pool).await;

    // Create a random registration ID that's not in the match
    let fake_reg_id = TournamentRegistrationId::new();

    // Start a transaction
    let mut tx = begin_transaction(&db.pool)
        .await
        .expect("Failed to begin transaction");

    // Try to complete with invalid participant
    let result = complete_match_in_transaction(
        &mut tx,
        MatchCompletionTxInput {
            match_id,
            winner_registration_id: fake_reg_id, // Not a participant
            loser_registration_id: fake_reg_id,
            winner_score: 2,
            loser_score: 1,
        },
    )
    .await;

    // Should fail
    assert!(result.is_err(), "Should fail with invalid participant");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not a participant"),
        "Error should mention invalid participant: {}",
        err
    );
}

#[tokio::test]
async fn test_match_completion_transaction_rollback_on_drop() {
    let db = TestDb::new().await;
    let (match_id, _bracket_id, reg1_id, reg2_id) = setup_tournament_with_match(&db.pool).await;

    // Start a transaction but don't commit
    {
        let mut tx = begin_transaction(&db.pool)
            .await
            .expect("Failed to begin transaction");

        // Complete the match in transaction
        let result = complete_match_in_transaction(
            &mut tx,
            MatchCompletionTxInput {
                match_id,
                winner_registration_id: reg1_id,
                loser_registration_id: reg2_id,
                winner_score: 2,
                loser_score: 1,
            },
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Match completion should succeed");

        // Don't commit - let tx drop
    }

    // Verify match was NOT completed (rolled back on drop)
    let match_repo = PgTournamentMatchRepository::new(db.pool.clone());
    let match_ = match_repo
        .find_by_id(match_id)
        .await
        .expect("Query should succeed")
        .expect("Match should exist");

    assert_eq!(
        match_.status,
        TournamentMatchStatus::InProgress,
        "Match should still be in progress after dropped transaction"
    );
    assert!(
        match_.winner_registration_id.is_none(),
        "Winner should not be set after dropped transaction"
    );
}

#[tokio::test]
async fn test_match_completion_fails_for_wrong_status() {
    let db = TestDb::new().await;
    let (match_id, _bracket_id, reg1_id, reg2_id) = setup_tournament_with_match(&db.pool).await;

    // Set match back to Pending (invalid for completion)
    let match_repo = PgTournamentMatchRepository::new(db.pool.clone());
    match_repo
        .update_status(match_id, TournamentMatchStatus::Pending)
        .await
        .expect("Failed to reset status");

    // Start a transaction
    let mut tx = begin_transaction(&db.pool)
        .await
        .expect("Failed to begin transaction");

    // Try to complete a pending match
    let result = complete_match_in_transaction(
        &mut tx,
        MatchCompletionTxInput {
            match_id,
            winner_registration_id: reg1_id,
            loser_registration_id: reg2_id,
            winner_score: 2,
            loser_score: 1,
        },
    )
    .await;

    // Should fail
    assert!(result.is_err(), "Should fail for pending match");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("cannot be completed"),
        "Error should mention invalid status: {}",
        err
    );
}
