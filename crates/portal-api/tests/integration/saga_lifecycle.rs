//! Saga lifecycle / crash-recovery integration tests.
//!
//! These tests cover the consequences of `SagaExecution::start()` never being
//! called in production code: `start_saga` INSERTs the row as `'pending'` and
//! nothing ever transitions it to `'running'` or sets `started_at`.
//!
//! 1. A saga is never marked `running` and `started_at` stays NULL forever.
//! 2. A hard-crashed saga (left `'pending'`) matches neither branch of
//!    `find_retryable` (`status = 'failed' OR (status = 'running' AND ...)`),
//!    so it is never re-driven.
//! 3. A stale `'pending'` row trips the partial unique index
//!    `uq_saga_executions_live_per_match`, permanently blocking every future
//!    `match_completion` saga for that match.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::tournaments::transition_match_to_ready;

// ============================================================================
// HELPERS (duplicated from match_completion_saga.rs — those are module-private)
// ============================================================================

async fn transition_match_to_in_progress(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": scheduled_time.to_rfc3339(),
                "reason": "Test setup"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
            &json!({
                "to_status": "in_progress",
                "override_reason": "Test setup"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

async fn register_player(app: &TestApp, tournament_id: &str, participant_name: &str) -> String {
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": participant_name }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

async fn approve_registration(app: &TestApp, tournament_id: &str, registration_id: &str) {
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

struct FourPlayerTournament {
    tournament_id: String,
    test_match_id: String,
    final_match_id: String,
    dev_reg_id: String,
    opponent_user_id: Uuid,
    dev_is_p1: bool,
}

/// Create a 4-player single-elimination tournament with the dev user's
/// semifinal transitioned to InProgress.
async fn create_4player_tournament(app: &TestApp, slug: &str) -> FourPlayerTournament {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("Saga Lifecycle {}", slug),
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

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();
    let tournament_uuid: Uuid = tournament_id.parse().unwrap();

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);

    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    let dev_reg_id = register_player(app, &tournament_id, "Player1").await;
    approve_registration(app, &tournament_id, &dev_reg_id).await;

    for (n, name) in [(2, "Player2"), (3, "Player3"), (4, "Player4")] {
        let user = UserBuilder::new()
            .username(format!("player{n}_{slug}"))
            .build_persisted(app.pool())
            .await;
        let _reg = TournamentRegistrationBuilder::new()
            .tournament_id_from_uuid(tournament_uuid)
            .player_id_from_uuid(user.id)
            .participant_name(name)
            .registered_by_uuid(user.id)
            .approved()
            .build_persisted(app.pool())
            .await;
    }

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
    let matches = body["data"].as_array().unwrap();

    let mut semis = Vec::new();
    let mut final_match = None;
    for m in matches {
        let round = m["round"].as_i64().unwrap();
        if round == 1 {
            semis.push(m.clone());
        } else if round == 2 {
            final_match = Some(m.clone());
        }
    }
    assert_eq!(semis.len(), 2, "Should have 2 semifinal matches");
    let final_match = final_match.expect("Should have a final match");
    let final_match_id = final_match["id"].as_str().unwrap().to_string();

    let mut test_match = None;
    for m in &semis {
        let p1 = m["participant1_registration_id"].as_str().unwrap_or("");
        let p2 = m["participant2_registration_id"].as_str().unwrap_or("");
        if p1 == dev_reg_id || p2 == dev_reg_id {
            test_match = Some(m.clone());
        }
    }
    let test_match = test_match.expect("Dev user should be in one of the semifinals");
    let test_match_id = test_match["id"].as_str().unwrap().to_string();

    let p1 = test_match["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let p2 = test_match["participant2_registration_id"]
        .as_str()
        .unwrap()
        .to_string();
    let dev_is_p1 = p1 == dev_reg_id;
    let opponent_reg_id = if dev_is_p1 { p2 } else { p1 };

    let opponent_reg_uuid: Uuid = opponent_reg_id.parse().unwrap();
    let row = sqlx::query("SELECT registered_by FROM tournament_registrations WHERE id = $1")
        .bind(opponent_reg_uuid)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let opponent_user_id: Uuid = row.get("registered_by");

    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;

    transition_match_to_ready(app, &tournament_id, &test_match_id).await;
    transition_match_to_in_progress(app, &tournament_id, &test_match_id).await;

    FourPlayerTournament {
        tournament_id,
        test_match_id,
        final_match_id,
        dev_reg_id,
        opponent_user_id,
        dev_is_p1,
    }
}

/// Submit a BO3 result claim (winner takes 2-1) and return the claim ID.
async fn submit_claim(
    app: &TestApp,
    match_id: &str,
    winner_reg_id: &str,
    winner_is_p1: bool,
) -> String {
    let (p1_score, p2_score) = if winner_is_p1 { (2, 1) } else { (1, 2) };
    let (g1_p1, g1_p2) = if winner_is_p1 { (16, 10) } else { (10, 16) };
    let (g2_p1, g2_p2) = if winner_is_p1 { (12, 16) } else { (16, 12) };
    let (g3_p1, g3_p2) = if winner_is_p1 { (16, 8) } else { (8, 16) };

    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": winner_reg_id,
                "participant1_score": p1_score,
                "participant2_score": p2_score,
                "game_results": [
                    { "game_number": 1, "map_id": "de_dust2",  "participant1_score": g1_p1, "participant2_score": g1_p2 },
                    { "game_number": 2, "map_id": "de_mirage", "participant1_score": g2_p1, "participant2_score": g2_p2 },
                    { "game_number": 3, "map_id": "de_inferno","participant1_score": g3_p1, "participant2_score": g3_p2 }
                ],
                "evidence_ids": [],
                "demo_link_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["claim"]["id"].as_str().unwrap().to_string()
}

async fn confirm_claim_as_user(
    app: &TestApp,
    match_id: &str,
    claim_id: &str,
    user_id: Uuid,
) -> serde_json::Value {
    let token = create_test_token(user_id, user_id, "confirmer", TEST_JWT_SECRET);
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
            &json!({}),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    response.json()
}

/// Fetch `(status, started_at)` for the match's completion saga rows.
async fn completion_saga_rows(
    app: &TestApp,
    match_uuid: Uuid,
) -> Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> {
    sqlx::query(
        "SELECT status, started_at FROM saga_executions
         WHERE match_id = $1 AND saga_type = 'match_completion'
         ORDER BY created_at ASC",
    )
    .bind(match_uuid)
    .fetch_all(app.pool())
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.get("status"), r.get("started_at")))
    .collect()
}

fn lifecycle_cfg() -> portal_api::background::LifecycleConfig {
    portal_api::background::LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
    }
}

// ============================================================================
// BUG 1: a saga is never marked `running` / `started_at` is never set
// ============================================================================

/// `SagaCoordinator::start_saga` builds a `SagaExecution` (status `Pending`,
/// `started_at: None`), persists it as `'pending'`, and returns — it never
/// calls `SagaExecution::start()`. `start()` has zero callers in production
/// code, so no saga ever reaches `'running'` and `started_at` is NULL for the
/// saga's entire life, including after a successful completion.
///
/// `started_at` is the only timing signal the recovery machinery has:
/// `find_stuck` and the `running` branch of `find_retryable` both key off it.
#[tokio::test]
async fn test_completed_saga_records_started_at() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-started-at").await;

    let claim_id = submit_claim(&app, &t.test_match_id, &t.dev_reg_id, t.dev_is_p1).await;
    let body = confirm_claim_as_user(&app, &t.test_match_id, &claim_id, t.opponent_user_id).await;

    // Sanity: the completion really did run end to end.
    assert_eq!(
        body["data"]["bracket_advanced"], true,
        "precondition: the completion saga should have advanced the bracket"
    );

    let match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let rows = completion_saga_rows(&app, match_uuid).await;
    assert_eq!(
        rows.len(),
        1,
        "exactly one match_completion saga should exist for the match"
    );
    let (status, started_at) = &rows[0];

    assert_eq!(
        status, "completed",
        "precondition: the saga should have reached a terminal completed state"
    );
    assert!(
        started_at.is_some(),
        "a saga that ran to completion must have recorded started_at \
         (SagaExecution::start() is never called, so it stays NULL and the \
          saga never passes through 'running'); row = (status={status}, started_at={started_at:?})"
    );
}

// ============================================================================
// BUG 2: a hard-crashed (`pending`) saga is never re-driven
// ============================================================================

/// A process SIGKILLed mid-saga leaves the row exactly as `start_saga` wrote
/// it: `status = 'pending'`, `started_at = NULL`, empty `step_history`.
///
/// `find_retryable` selects
/// `status = 'failed' OR (status = 'running' AND started_at < $2)`.
/// A `'pending'` row matches NEITHER branch, so the lifecycle re-drive pass
/// silently ignores it forever and the bracket stalls until an admin drives
/// the progression endpoints by hand.
///
/// This is the same scenario as
/// `match_completion_saga::test_lifecycle_redrives_failed_completion_saga`,
/// which passes — the *only* difference is the staged status.
#[tokio::test]
async fn test_lifecycle_redrives_crashed_pending_saga() {
    use portal_api::background::run_lifecycle_pass;
    use portal_api::state::AppState;

    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-pending-redrive").await;

    let claim_id = submit_claim(&app, &t.test_match_id, &t.dev_reg_id, t.dev_is_p1).await;
    confirm_claim_as_user(&app, &t.test_match_id, &claim_id, t.opponent_user_id).await;

    let final_match_uuid: Uuid = t.final_match_id.parse().unwrap();
    let test_match_uuid: Uuid = t.test_match_id.parse().unwrap();

    // Undo the advancement: the crash happened before progression landed.
    sqlx::query(
        "UPDATE tournament_matches
         SET participant1_registration_id = NULL, participant2_registration_id = NULL,
             status = 'pending'
         WHERE id = $1",
    )
    .bind(final_match_uuid)
    .execute(app.pool())
    .await
    .unwrap();

    // Re-stage the saga row as a hard crash leaves it: still 'pending',
    // started_at NULL, no step history, created an hour ago.
    let updated = sqlx::query(
        "UPDATE saga_executions
         SET status = 'pending', step_history = '[]'::jsonb, retry_count = 0,
             started_at = NULL, completed_at = NULL, last_error = NULL,
             current_step = 0, created_at = NOW() - INTERVAL '1 hour'
         WHERE match_id = $1 AND saga_type = 'match_completion'
         RETURNING id",
    )
    .bind(test_match_uuid)
    .fetch_all(app.pool())
    .await
    .unwrap();
    assert!(
        !updated.is_empty(),
        "the confirm should have produced a match_completion saga to re-stage"
    );

    let state = AppState::new(app.pool().clone(), TEST_JWT_SECRET).await;
    let summary = run_lifecycle_pass(&state, &lifecycle_cfg(), false).await;

    assert_eq!(
        summary.sagas_redriven, 1,
        "a saga left 'pending' by a crashed process must be re-driven, but \
         find_retryable only looks at 'failed' and 'running' rows so it is \
         ignored forever (summary: {summary:?})"
    );

    let row = sqlx::query(
        "SELECT participant1_registration_id, participant2_registration_id
         FROM tournament_matches WHERE id = $1",
    )
    .bind(final_match_uuid)
    .fetch_one(app.pool())
    .await
    .unwrap();
    let p1: Option<Uuid> = row.get("participant1_registration_id");
    let p2: Option<Uuid> = row.get("participant2_registration_id");
    let winner: Uuid = t.dev_reg_id.parse().unwrap();
    assert!(
        p1 == Some(winner) || p2 == Some(winner),
        "the re-drive should have re-advanced the winner into the final \
         (p1={p1:?}, p2={p2:?})"
    );
}

// ============================================================================
// BUG 3: a stale `pending` saga permanently blocks all future sagas
// ============================================================================

/// `uq_saga_executions_live_per_match` (migration 0068) is a partial unique
/// index over `(match_id, saga_type)` where `status IN ('pending','running')`.
///
/// Because sagas are created `'pending'` and nothing ever moves them off it
/// except a completed run, a crashed saga leaves a permanently "live" row.
/// Every subsequent `start_saga` for that match then violates the index;
/// `PgSagaExecutionRepository::create` maps it to
/// `DomainError::Conflict("A completion is already in progress for this
/// match")`, so the match can never be completed again by ANY path — the
/// confirm handler, the lifecycle re-drive, or the review resume.
///
/// The confirm handler swallows the saga error and still returns 200 with
/// `bracket_advanced: false`, so the assertion is on progression, not status.
#[tokio::test]
async fn test_stale_pending_saga_does_not_block_new_completion() {
    let app = TestApp::new().await;
    let t = create_4player_tournament(&app, "saga-pending-block").await;

    let tournament_uuid: Uuid = t.tournament_id.parse().unwrap();
    let test_match_uuid: Uuid = t.test_match_id.parse().unwrap();
    let final_match_uuid: Uuid = t.final_match_id.parse().unwrap();

    // Stage the leftover of a process that died mid-saga an hour ago.
    sqlx::query(
        "INSERT INTO saga_executions
           (saga_type, saga_version, tournament_id, match_id, input_data,
            status, step_history, retry_count, max_retries, created_at, updated_at)
         VALUES ('match_completion', 2, $1, $2, '{}'::jsonb,
                 'pending', '[]'::jsonb, 0, 3,
                 NOW() - INTERVAL '1 hour', NOW() - INTERVAL '1 hour')",
    )
    .bind(tournament_uuid)
    .bind(test_match_uuid)
    .execute(app.pool())
    .await
    .expect("staging a stale pending saga row should succeed");

    // A completely legitimate completion of the same match.
    let claim_id = submit_claim(&app, &t.test_match_id, &t.dev_reg_id, t.dev_is_p1).await;
    let body = confirm_claim_as_user(&app, &t.test_match_id, &claim_id, t.opponent_user_id).await;

    assert_eq!(
        body["data"]["bracket_advanced"], true,
        "a stale 'pending' saga row from a previous crash must not block a new \
         completion for the same match; uq_saga_executions_live_per_match turns \
         start_saga into a Conflict and the match becomes permanently \
         uncompletable (response: {body})"
    );

    let row = sqlx::query(
        "SELECT participant1_registration_id, participant2_registration_id
         FROM tournament_matches WHERE id = $1",
    )
    .bind(final_match_uuid)
    .fetch_one(app.pool())
    .await
    .unwrap();
    let p1: Option<Uuid> = row.get("participant1_registration_id");
    let p2: Option<Uuid> = row.get("participant2_registration_id");
    let winner: Uuid = t.dev_reg_id.parse().unwrap();
    assert!(
        p1 == Some(winner) || p2 == Some(winner),
        "the winner should have been advanced into the final despite the stale \
         pending saga row (p1={p1:?}, p2={p2:?})"
    );
}
