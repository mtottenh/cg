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

// ============================================================================
// P-83 — REVERT ACTUALLY ROLLS BACK AN ELIMINATION ADVANCEMENT
// ============================================================================

async fn cs2_game_id(app: &TestApp) -> String {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM games WHERE slug = $1")
        .bind("cs2")
        .fetch_one(app.pool())
        .await
        .expect("cs2 game row");
    id.to_string()
}

/// Whether `registration_id` currently holds either participant slot on `match_id`.
async fn occupies_slot(
    app: &TestApp,
    tournament_id: &str,
    match_id: &str,
    registration_id: &str,
) -> bool {
    let m = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await
        .json::<serde_json::Value>()["data"]
        .clone();
    m["participant1_registration_id"].as_str() == Some(registration_id)
        || m["participant2_registration_id"].as_str() == Some(registration_id)
}

/// P-83: reverting progression on a single-elimination bracket must take the
/// advanced winner back out of the downstream match.
///
/// `revert_progression` used to handle RoundRobin | Swiss only; for elimination
/// it logged "Would revert winner progression - needs implementation" and
/// returned **200**, while the admin UI's confirm dialog promised that
/// "downstream pairings created from it are rolled back". An admin got a
/// success snackbar for work that never happened, on the most
/// destructive-sounding control in the tab.
///
/// The assertion is on the FINAL's participant slot, not on the source match:
/// clearing the source result was the part that already worked, so asserting
/// that would pass against the bug.
#[tokio::test]
async fn test_revert_withdraws_the_winner_from_the_downstream_elimination_match() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "name": "P-83 Revert Elimination",
                "slug": "p83-revert-elim",
                "game_id": cs2_game_id(&app).await,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 4,
                "max_participants": 4,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo1"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let tournament_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Four DISTINCT players — `register_player` registers the calling identity,
    // so looping it would 409 on the second. A 4-player bracket is the minimum
    // that produces a semi-final feeding a final, which is what P-83 is about.
    for name in ["P1", "P2", "P3", "P4"] {
        let (user_id, player_id) =
            crate::tournaments::create_test_player(&app, &format!("p83_{}", name.to_lowercase()))
                .await;
        crate::tournaments::insert_test_registration(
            &app,
            &tournament_id,
            player_id,
            user_id,
            name,
        )
        .await;
    }

    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "method": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    // Semi-finals feed a final. `winner_progresses_to` is not exposed on any
    // DTO, so read the bracket wiring straight from the row — which is also the
    // precise thing this test is about.
    let (semi_uuid, final_uuid, p1_uuid, p2_uuid): (Uuid, Uuid, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT m.id, m.winner_progresses_to, m.participant1_registration_id,
                    m.participant2_registration_id
             FROM tournament_matches m
             WHERE m.tournament_id = $1
               AND m.winner_progresses_to IS NOT NULL
               AND m.participant1_registration_id IS NOT NULL
               AND m.participant2_registration_id IS NOT NULL
             ORDER BY m.created_at
             LIMIT 1",
        )
        .bind(Uuid::parse_str(&tournament_id).unwrap())
        .fetch_one(app.pool())
        .await
        .expect("a seeded semi-final that feeds the final");

    let semi_id = semi_uuid.to_string();
    let final_id = final_uuid.to_string();
    let winner_reg = p1_uuid.unwrap().to_string();
    let loser_reg = p2_uuid.unwrap().to_string();

    // Record the result on the semi. In production progression always follows a
    // COMPLETED match, so the row carries its winner; the admin
    // `progression/process` endpoint is a repair tool pointed at such a match,
    // and it does not itself write a winner. Revert reads that recorded winner
    // to know who to withdraw, so without this the test would be asserting
    // against a state the product never reaches.
    sqlx::query(
        "UPDATE tournament_matches
         SET winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 1, participant2_score = 0,
             status = 'completed', completed_at = NOW()
         WHERE id = $1",
    )
    .bind(semi_uuid)
    .bind(Uuid::parse_str(&winner_reg).unwrap())
    .bind(Uuid::parse_str(&loser_reg).unwrap())
    .execute(app.pool())
    .await
    .expect("record the semi-final result");

    // Record the result on the semi before advancing. In production progression
    // runs off a COMPLETED match, so the row carries its winner; the admin
    // `process` endpoint is a repair tool pointed at such a match, and it does
    // not itself write one. Reverting has to know who advanced, so a row with no
    // winner is not a state this test should invent.
    sqlx::query(
        "UPDATE tournament_matches
         SET winner_registration_id = $2, loser_registration_id = $3,
             status = 'completed', completed_at = NOW()
         WHERE id = $1",
    )
    .bind(semi_uuid)
    .bind(Uuid::parse_str(&winner_reg).unwrap())
    .bind(Uuid::parse_str(&loser_reg).unwrap())
    .execute(app.pool())
    .await
    .expect("record the semi-final result");

    // Advance the winner into the final.
    let response = app
        .post_json(
            &format!("/v1/admin/matches/{semi_id}/progression/process"),
            &json!({
                "winner_registration_id": winner_reg,
                "loser_registration_id": loser_reg
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    assert!(
        occupies_slot(&app, &tournament_id, &final_id, &winner_reg).await,
        "precondition: processing progression must seat the winner in the final"
    );

    // Revert it.
    app.post_json(
        &format!("/v1/admin/matches/{semi_id}/progression/revert"),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);

    assert!(
        !occupies_slot(&app, &tournament_id, &final_id, &winner_reg).await,
        "P-83: after reverting, the winner must no longer occupy a slot in the \
         final — the old implementation logged 'needs implementation' and left \
         them seeded while reporting success"
    );
}
