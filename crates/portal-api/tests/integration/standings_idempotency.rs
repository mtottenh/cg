//! Standings / player-stats idempotency regression tests.
//!
//! `PgTournamentStandingsRepository::update_after_match` is **accumulative**
//! (`points = points + $8`, `matches_played = matches_played + 1`) and has
//! four non-transactional call sites. Every path that can fire twice for the
//! same match therefore double-counts. The same is true of
//! `PgPlayerGameProfileRepository::update_stats_after_match`
//! (`matches_played = matches_played + 1`, `win_streak = win_streak + 1`).
//!
//! Standings only exist for RoundRobin/Swiss brackets (elimination brackets
//! record `{"action":"not_applicable"}`), so the standings fixtures below use
//! a round-robin tournament; the player-stats fixture uses single elimination
//! deliberately, because that is where the re-drive guard is a no-op.

use crate::common::TestApp;
use crate::results::create_rr_match_in_progress;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

/// Fetch `(matches_played, matches_won, matches_lost, points)` for a
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

/// Fetch `(matches_played, wins, losses, win_streak)` for a player's game profile.
async fn profile_for(app: &TestApp, player_id: Uuid, game_id: Uuid) -> (i32, i32, i32, i32) {
    sqlx::query_as(
        "SELECT matches_played, wins, losses, win_streak
         FROM player_game_profiles
         WHERE player_id = $1 AND game_id = $2",
    )
    .bind(player_id)
    .bind(game_id)
    .fetch_one(app.pool())
    .await
    .expect("player game profile row")
}

/// Resolve the player behind a registration.
async fn player_of_registration(app: &TestApp, registration_id: &str) -> Uuid {
    sqlx::query_scalar("SELECT player_id FROM tournament_registrations WHERE id = $1")
        .bind(Uuid::parse_str(registration_id).unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap()
}

/// The dev user's player/user id (`Bearer dev-token`).
const DEV_PLAYER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Acknowledge a result review as the player who actually owns the
/// registration — the dev user via `dev-token`, anyone else via
/// `opponent_token`. Returns the (200 OK) response body.
async fn acknowledge_as_owner(
    app: &TestApp,
    match_id: &str,
    registration_id: &str,
    opponent_token: &str,
) -> serde_json::Value {
    let uri = format!(
        "/v1/matches/{match_id}/result-review/acknowledge?registration_id={registration_id}"
    );
    let owner = player_of_registration(app, registration_id).await;
    let response = if owner == Uuid::parse_str(DEV_PLAYER_ID).unwrap() {
        app.post_auth(&uri).await
    } else {
        app.post_with_token(&uri, opponent_token).await
    };
    response.assert_status(StatusCode::OK);
    response.json()
}

/// Submit a 2-0 claim for `winner_reg` and confirm it as the opponent,
/// completing the match and applying standings once via the completion saga.
async fn complete_match_via_claims(
    app: &TestApp,
    match_id: &str,
    winner_reg: &str,
    opponent_token: &str,
) {
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": winner_reg,
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

/// Mark this match's `match_completion` saga as `failed`, optionally wiping
/// its step history — the state a crash between the (already committed)
/// step effects and `complete_step` leaves behind.
async fn stage_failed_completion_saga(app: &TestApp, match_id: &str, wipe_step_history: bool) {
    let match_uuid = Uuid::parse_str(match_id).unwrap();
    let sql = if wipe_step_history {
        "UPDATE saga_executions
         SET status = 'failed', step_history = '[]'::jsonb, retry_count = 0,
             last_error = 'simulated crash before step_history persisted'
         WHERE match_id = $1 AND saga_type = 'match_completion'
         RETURNING id"
    } else {
        "UPDATE saga_executions
         SET status = 'failed', retry_count = 0,
             last_error = 'simulated crash after effects committed'
         WHERE match_id = $1 AND saga_type = 'match_completion'
         RETURNING id"
    };
    let updated = sqlx::query(sql)
        .bind(match_uuid)
        .fetch_all(app.pool())
        .await
        .unwrap();
    assert!(
        !updated.is_empty(),
        "the confirm should have produced a match_completion saga to re-stage"
    );
}

/// Run one lifecycle pass (the background re-drive) and return the summary.
async fn run_one_lifecycle_pass(app: &TestApp) -> portal_api::background::LifecyclePassSummary {
    use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
    use portal_api::state::AppState;

    let state = AppState::new(app.pool().clone(), TEST_JWT_SECRET).await;
    let cfg = LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
    };
    run_lifecycle_pass(&state, &cfg, false).await
}

/// Create a 2-player **single elimination** tournament with its only match
/// transitioned to InProgress.
///
/// Returns `(tournament_id, match_id, p1_reg, p2_reg, opponent_token)`.
async fn create_se_match_in_progress(
    app: &TestApp,
    slug: &str,
) -> (String, String, String, String, String) {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("SE Idempotency {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
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

    let reg1 = crate::tournaments::register_player(app, &tournament_id, "Player1").await;
    crate::tournaments::approve_registration(app, &tournament_id, &reg1).await;
    let (user2_id, player2_id) =
        crate::tournaments::create_test_player(app, &format!("sep2_{slug}")).await;
    let _reg2 = crate::tournaments::insert_test_registration(
        app,
        &tournament_id,
        player2_id,
        user2_id,
        "Player2",
    )
    .await;

    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/matches"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let match_data = &body["data"].as_array().unwrap()[0];
    let match_id = match_data["id"].as_str().unwrap().to_string();
    let p1_reg = match_data["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let p2_reg = match_data["participant2_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
    crate::tournaments::transition_match_to_ready(app, &tournament_id, &match_id).await;

    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": scheduled_time.to_rfc3339(), "reason": "Test setup" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
        &json!({ "to_status": "in_progress", "override_reason": "Test setup" }),
    )
    .await
    .assert_status(StatusCode::OK);

    let opponent_token = create_test_token(
        user2_id,
        player2_id,
        &format!("sep2_{slug}"),
        TEST_JWT_SECRET,
    );

    (tournament_id, match_id, p1_reg, p2_reg, opponent_token)
}

// ============================================================================
// 1. REVIEW-RESUME DOUBLE-APPLY
// ============================================================================

/// `ResultReviewStatus::is_terminal()` only covers `Approved | Rejected`, so
/// `Acknowledged` is not terminal. Both captains acknowledging a roster-only
/// review fires `resume_saga_after_review` (applying standings + player
/// stats); a subsequent admin `approve` passes the `is_terminal` guard and
/// fires the very same saga a second time. The match is counted twice.
#[tokio::test]
async fn test_acknowledge_then_admin_approve_applies_standings_once() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "idem-review-resume").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: Uuid = match_id.parse().unwrap();
    let tournament_uuid: Uuid = tournament_id.parse().unwrap();

    // A demo whose scores contradict the claim pauses the saga on a review,
    // so no standings have been applied yet.
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("idem_review_demo.dem")
        .cs2_metadata("de_dust2", "Player1", "Player2", 13, 16)
        .tournament_id(tournament_uuid)
        .build_persisted(app.pool())
        .await;
    let demo_link = DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;

    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [
                    { "game_number": 1, "map_id": "de_dust2",
                      "participant1_score": 16, "participant2_score": 10 },
                    { "game_number": 2, "map_id": "de_mirage",
                      "participant1_score": 16, "participant2_score": 8 }
                ],
                "evidence_ids": [],
                "demo_link_ids": [demo_link.id.to_string()]
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
            &opponent_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (0, 0, 0, 0),
        "progression is paused on the review — no standings yet"
    );

    // Restage the review as roster-only so the captain-acknowledgment path
    // (which resumes the saga) is the one exercised. The demo validator
    // adapter never populates `unrecognized_players`, so this is the only
    // way to reach `PendingAcknowledgment` today.
    let flipped = sqlx::query(
        "UPDATE result_reviews
         SET status = 'pending_acknowledgment', roster_mismatch = true,
             score_mismatch = false, winner_mismatch = false
         WHERE match_id = $1
         RETURNING id",
    )
    .bind(match_uuid)
    .fetch_all(app.pool())
    .await
    .unwrap();
    assert_eq!(flipped.len(), 1, "one review should exist for the match");

    // Captain 1 (dev user) acknowledges, then captain 2 — the second
    // acknowledgment flips the review to `Acknowledged` and resumes the saga.
    acknowledge_as_owner(&app, &match_id, &p1_reg, &opponent_token).await;
    let body = acknowledge_as_owner(&app, &match_id, &p2_reg, &opponent_token).await;
    assert_eq!(
        body["data"]["both_acknowledged"], true,
        "both captains must have acknowledged"
    );

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "the acknowledgment resume applies the win exactly once"
    );

    // Now an admin approves the (non-terminal) `Acknowledged` review.
    let review_response = app
        .get_auth(&format!("/v1/matches/{match_id}/result-review"))
        .await;
    review_response.assert_status(StatusCode::OK);
    let review_id = review_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/approve"),
            &json!({ "notes": "Rubber-stamping the acknowledged review" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "admin approval after acknowledgment must NOT count the win a second time"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 0, 1, 0),
        "the loser must not be counted twice either"
    );

    let winner_player = player_of_registration(&app, &p1_reg).await;
    assert_eq!(
        profile_for(&app, winner_player, game_id).await,
        (1, 1, 0, 1),
        "player_game_profiles must not be double-incremented either"
    );
}

// ============================================================================
// 2. ADMIN /progression/process APPLIED TWICE
// ============================================================================

/// `POST /v1/admin/matches/{id}/progression/process` has no
/// already-processed guard: each call runs `update_after_match` again.
#[tokio::test]
async fn test_admin_process_progression_twice_applies_standings_once() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, _opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "idem-process-twice").await;

    // Complete the match directly (no saga, no standings applied yet) — the
    // scenario where an admin drives progression by hand.
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

    let body = json!({
        "winner_registration_id": p1_reg,
        "loser_registration_id": p2_reg
    });
    let uri = format!("/v1/admin/matches/{match_id}/progression/process");

    app.post_json(&uri, &body)
        .await
        .assert_status(StatusCode::OK);
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));

    // Second identical call — a retry, a double-click, an at-least-once
    // delivery. It must be a no-op.
    app.post_json(&uri, &body)
        .await
        .assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "processing the same completed match twice must count it once"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 0, 1, 0),
        "the loser must be counted once too"
    );
}

// ============================================================================
// 3. ADMIN REAPPLY / REVERT
// ============================================================================

/// `reapply_progression` is `revert_progression` + `process_match_completion`
/// as two separate non-transactional calls. Reapplying the *same* winner
/// should be a net no-op.
///
/// NOTE: this one currently PASSES. `revert_after_match` is the exact
/// arithmetic inverse of `update_after_match`, so a single revert+reapply
/// cycle nets to zero. The non-atomicity is still real (a crash between the
/// two halves leaves the standings short by one result) but it cannot be
/// demonstrated without fault injection — see `test_revert_progression_twice_subtracts_once`
/// for the reachable half of this bug.
#[tokio::test]
async fn test_reapply_same_winner_is_net_neutral() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "idem-reapply-neutral").await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    let before_p1 = standing_for(&app, bracket_id, &p1_reg).await;
    let before_p2 = standing_for(&app, bracket_id, &p2_reg).await;
    assert_eq!(before_p1, (1, 1, 0, 3));

    let response = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p1_reg }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        before_p1,
        "revert+reapply of the same result must leave standings unchanged"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        before_p2,
        "revert+reapply of the same result must leave standings unchanged"
    );
}

/// `revert_progression` derives its deltas from `match_.winner_registration_id`,
/// which it never clears — so a second revert subtracts the same deltas again
/// and drives the standings negative.
#[tokio::test]
async fn test_revert_progression_twice_subtracts_once() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "idem-revert-twice").await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));

    let uri = format!("/v1/admin/matches/{match_id}/progression/revert");
    app.post_auth(&uri).await.assert_status(StatusCode::OK);
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (0, 0, 0, 0));

    // Second revert of the same (already reverted) match.
    app.post_auth(&uri).await.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (0, 0, 0, 0),
        "reverting twice must not double-deduct the winner"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (0, 0, 0, 0),
        "reverting twice must not double-deduct the loser"
    );
}

// ============================================================================
// 4. SAGA RE-DRIVE DOUBLE-COUNT (the W3 crash window)
// ============================================================================

/// `redrive_stuck_completion_sagas` only refuses to re-run a saga whose
/// `step_history` records `update_standings` with `"standings_updated": true`.
/// But `step_update_standings` commits its two `update_after_match` writes in
/// their own transactions *before* `complete_step` persists that record. A
/// crash in that window leaves standings applied and no history proving it —
/// exactly the state staged here — and the re-drive applies them again.
#[tokio::test]
async fn test_redrive_after_crash_window_does_not_double_apply_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "idem-redrive-standings").await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "the completion saga applied the standings"
    );

    // The crash window: effects committed, step record not yet written.
    stage_failed_completion_saga(&app, &match_id, true).await;

    let summary = run_one_lifecycle_pass(&app).await;
    assert_eq!(
        summary.sagas_redriven, 1,
        "the failed completion saga should have been re-driven (summary: {summary:?})"
    );

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "re-driving the saga must not apply the winner's standings a second time"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 0, 1, 0),
        "re-driving the saga must not apply the loser's standings a second time"
    );
}

// ============================================================================
// 5. RE-DRIVE DOUBLE-APPLIES PLAYER STATS ON ELIMINATION BRACKETS
// ============================================================================

/// On an elimination bracket `step_update_standings` records
/// `{"action":"not_applicable"}` and never sets `standings_updated: true`, so
/// the re-drive guard never trips — not even with a fully intact step history
/// (which is what this test stages). `step_update_player_stats` is
/// nonetheless accumulative and has no match-scoped key, so every re-drive
/// re-counts the match on both players' profiles.
#[tokio::test]
async fn test_redrive_on_elimination_bracket_does_not_double_apply_player_stats() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token) =
        create_se_match_in_progress(&app, "idem-redrive-stats").await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let winner_player = player_of_registration(&app, &p1_reg).await;
    let loser_player = player_of_registration(&app, &p2_reg).await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(
        profile_for(&app, winner_player, game_id).await,
        (1, 1, 0, 1),
        "the completion saga counted the win once"
    );
    assert_eq!(profile_for(&app, loser_player, game_id).await, (1, 0, 1, 0));

    // Only the saga status is flipped: the step history stays intact, proving
    // the guard is a no-op on elimination brackets.
    stage_failed_completion_saga(&app, &match_id, false).await;

    let summary = run_one_lifecycle_pass(&app).await;
    assert_eq!(
        summary.sagas_redriven, 1,
        "the failed completion saga should have been re-driven (summary: {summary:?})"
    );

    assert_eq!(
        profile_for(&app, winner_player, game_id).await,
        (1, 1, 0, 1),
        "re-driving must not double-count the winner's match / win / streak"
    );
    assert_eq!(
        profile_for(&app, loser_player, game_id).await,
        (1, 0, 1, 0),
        "re-driving must not double-count the loser's match / loss"
    );
}
