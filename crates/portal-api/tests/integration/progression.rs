//! Admin progression endpoint integration tests.
//!
//! Covers `/v1/admin/matches/{id}/progression/{process,reapply,revert}` —
//! including the launch-blocker regression that the admin/dispute-reapply
//! path must PERSIST standings (the old implementation mutated an
//! in-memory Vec and re-ranked unchanged stored points), plus the
//! previously untested permission-denied cases.

use crate::common::TestApp;
use crate::results::create_rr_match_in_progress;
use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

/// Register a plain (non-admin) user via the API and return their token.
async fn register_plain_user(app: &TestApp, username: &str) -> String {
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": username,
                "email": format!("{}@example.com", username),
                "password": "SecurePass123!",
                "display_name": username
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<serde_json::Value>()["data"]["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Confirm a 2-0 win for participant 1 through the claim flow, completing
/// the match and applying standings once via the completion saga.
async fn complete_match_via_claims(
    app: &TestApp,
    match_id: &str,
    p1_reg: &str,
    opponent_token: &str,
) {
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
            opponent_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Fetch (matches_played, matches_won, matches_lost, points) for a
/// registration in a bracket.
async fn standing_for(
    app: &TestApp,
    bracket_id: Uuid,
    registration_id: &str,
) -> (i32, i32, i32, i32) {
    sqlx::query_as(
        "SELECT matches_played, matches_won, matches_lost, points
         FROM tournament_standings
         WHERE bracket_id = $1 AND registration_id = $2",
    )
    .bind(bracket_id)
    .bind(Uuid::parse_str(registration_id).unwrap())
    .fetch_one(app.pool())
    .await
    .expect("standing row")
}

// ============================================================================
// PERMISSION-DENIED CASES
// ============================================================================

#[tokio::test]
async fn test_progression_admin_endpoints_require_permission() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, _opp, _bracket) =
        create_rr_match_in_progress(&app, "prog-perm-denied").await;
    let token = register_plain_user(&app, "prog_regular_user").await;

    let response = app
        .post_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/revert"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .post_json_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p2_reg }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .post_json_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": p1_reg,
                "loser_registration_id": p2_reg
            }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_progression_admin_endpoints_require_authentication() {
    let app = TestApp::new().await;
    let match_id = Uuid::now_v7();

    let response = app
        .post_json_no_auth(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": Uuid::now_v7().to_string(),
                "loser_registration_id": Uuid::now_v7().to_string()
            }),
        )
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// STANDINGS PERSISTENCE (launch blocker #7 regression)
// ============================================================================

/// The admin process endpoint must PERSIST the standings it reports.
#[tokio::test]
async fn test_process_progression_persists_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, _opp, bracket_id) =
        create_rr_match_in_progress(&app, "prog-process-persist").await;

    // Complete the match directly (no saga, no standings applied) — the
    // scenario where an admin drives progression manually.
    sqlx::query(
        "UPDATE tournament_matches SET
            participant1_score = 2, participant2_score = 1,
            winner_registration_id = $2, loser_registration_id = $3,
            status = 'completed', completed_at = NOW(), updated_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .bind(Uuid::parse_str(&p1_reg).unwrap())
    .bind(Uuid::parse_str(&p2_reg).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    // Standings start at zero.
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (0, 0, 0, 0));

    let response = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": p1_reg,
                "loser_registration_id": p2_reg
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["updated_standings_count"], 2);

    // The reported standings exist in the database.
    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "winner standings must be persisted"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 0, 1, 0),
        "loser standings must be persisted"
    );
}

/// Reapplying with a different winner must move the persisted points:
/// the recorded result's deltas are reverted, the new winner's applied.
#[tokio::test]
async fn test_reapply_progression_swaps_persisted_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "prog-reapply-swap").await;

    // Normal flow: participant 1 wins, standings applied once by the saga.
    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));
    assert_eq!(standing_for(&app, bracket_id, &p2_reg).await, (1, 0, 1, 0));

    // Admin overturns the result: participant 2 actually won.
    let response = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p2_reg }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Old winner's deltas reverted, new winner's applied — exactly once.
    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 0, 1, 0),
        "overturned winner must lose the win and the points"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 1, 0, 3),
        "new winner must gain the win and the points"
    );
}

/// Revert alone must subtract the recorded result's standings deltas.
#[tokio::test]
async fn test_revert_progression_subtracts_persisted_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "prog-revert-subtract").await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));

    let response = app
        .post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await;
    response.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (0, 0, 0, 0),
        "revert must subtract the winner's persisted deltas"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (0, 0, 0, 0),
        "revert must subtract the loser's persisted deltas"
    );
}
