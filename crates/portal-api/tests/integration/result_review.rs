//! Result review API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use serde_json::json;

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_result_review_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000001";
    let review_id = "00000000-0000-0000-0000-000000000002";

    // Verify get result review for match endpoint exists (authenticated)
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/result-review"))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /matches/:match_id/result-review endpoint should exist"
    );

    // Verify acknowledge result review endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{match_id}/result-review/acknowledge?registration_id=00000000-0000-0000-0000-000000000003"
            ),
            &json!({}),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /matches/:match_id/result-review/acknowledge endpoint should exist"
    );

    // Verify admin list pending reviews endpoint exists (authenticated)
    let response = app.get_auth("/v1/admin/result-reviews").await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /admin/result-reviews endpoint should exist"
    );

    // Verify admin get review by ID endpoint exists (authenticated)
    let response = app
        .get_auth(&format!("/v1/admin/result-reviews/{review_id}"))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /admin/result-reviews/{{id}} endpoint should exist"
    );

    // Verify admin approve endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/approve"),
            &json!({
                "notes": "Approved after review"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/result-reviews/{{id}}/approve endpoint should exist"
    );

    // Verify admin reject endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/reject"),
            &json!({
                "notes": "Rejected after review"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/result-reviews/{{id}}/reject endpoint should exist"
    );
}

// ============================================================================
// GET RESULT REVIEW TESTS
// ============================================================================

#[tokio::test]
async fn test_get_result_review_not_found() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000001";

    // GET review for match with no review
    let response = app
        .get_auth(&format!("/v1/matches/{match_id}/result-review"))
        .await;

    // Should return 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_result_review_invalid_match_id() {
    let app = TestApp::new().await;

    // GET review for invalid match ID
    let response = app.get_auth("/v1/matches/not-a-uuid/result-review").await;

    // Should return 400
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_result_review_unauthorized() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000001";

    // GET without auth should fail
    let response = app
        .get(&format!("/v1/matches/{match_id}/result-review"))
        .await;

    // Should return 401
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ACKNOWLEDGE TESTS
// ============================================================================

#[tokio::test]
async fn test_acknowledge_result_review_not_found() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000001";
    let registration_id = "00000000-0000-0000-0000-000000000002";

    // Try to acknowledge review that doesn't exist
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{match_id}/result-review/acknowledge?registration_id={registration_id}"
            ),
            &json!({}),
        )
        .await;

    // Should return 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_acknowledge_result_review_invalid_registration_id() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000001";

    // Try to acknowledge with invalid registration ID
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result-review/acknowledge?registration_id=not-a-uuid"),
            &json!({}),
        )
        .await;

    // Should return 400
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN ENDPOINTS TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_list_pending_reviews() {
    let app = TestApp::new().await;

    // GET pending reviews (should be empty)
    let response = app.get_auth("/v1/admin/result-reviews").await;

    // Should return 200 with empty list
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_admin_list_pending_reviews_with_pagination() {
    let app = TestApp::new().await;

    // GET pending reviews with pagination
    let response = app
        .get_auth("/v1/admin/result-reviews?page=1&per_page=10")
        .await;

    // Should return 200
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_admin_get_review_not_found() {
    let app = TestApp::new().await;
    let review_id = "00000000-0000-0000-0000-000000000001";

    // GET review that doesn't exist
    let response = app
        .get_auth(&format!("/v1/admin/result-reviews/{review_id}"))
        .await;

    // Should return 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_approve_review_not_found() {
    let app = TestApp::new().await;
    let review_id = "00000000-0000-0000-0000-000000000001";

    // Try to approve review that doesn't exist
    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/approve"),
            &json!({
                "notes": "Approved after review"
            }),
        )
        .await;

    // Should return 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_reject_review_not_found() {
    let app = TestApp::new().await;
    let review_id = "00000000-0000-0000-0000-000000000001";

    // Try to reject review that doesn't exist
    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/reject"),
            &json!({
                "notes": "Rejected after review"
            }),
        )
        .await;

    // Should return 404
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_approve_review_invalid_id() {
    let app = TestApp::new().await;

    // Try to approve with invalid ID
    let response = app
        .post_json(
            "/v1/admin/result-reviews/not-a-uuid/approve",
            &json!({
                "notes": "Approved after review"
            }),
        )
        .await;

    // Should return 400
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_reject_review_invalid_id() {
    let app = TestApp::new().await;

    // Try to reject with invalid ID
    let response = app
        .post_json(
            "/v1/admin/result-reviews/not-a-uuid/reject",
            &json!({
                "notes": "Rejected after review"
            }),
        )
        .await;

    // Should return 400
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// UNAUTHORIZED ACCESS TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_endpoints_require_auth() {
    let app = TestApp::new().await;
    let review_id = "00000000-0000-0000-0000-000000000001";

    // GET admin list without auth
    let response = app.get("/v1/admin/result-reviews").await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // GET admin review by ID without auth
    let response = app
        .get(&format!("/v1/admin/result-reviews/{review_id}"))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ACKNOWLEDGE AUTHORIZATION TESTS
// ============================================================================

/// The acknowledging caller must belong to the registration they acknowledge
/// for (or be an admin). Previously any authenticated user could acknowledge
/// for any registration.
#[tokio::test]
async fn test_acknowledge_requires_registration_binding() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "review-ack-authz")
            .await;

    let outsider = portal_test::builders::UserBuilder::new()
        .username("review_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = portal_test::helpers::create_test_token(
        outsider.id,
        outsider.id,
        "review_outsider",
        portal_test::helpers::TEST_JWT_SECRET,
    );

    // Outsider acknowledging for a participant registration is rejected
    // before the review lookup. Accept 401 or 403 while the NotAuthorized
    // status-code mapping is in flight.
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/result-review/acknowledge?registration_id={reg1}"),
            &outsider_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider acknowledge should be rejected, got {}",
        response.status
    );

    // A participant bound to their own registration passes the auth gate and
    // falls through to 404 (no review exists for this match) — not 403.
    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/result-review/acknowledge?registration_id={reg2}"),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ADMIN QUEUE ORDERING — P-55
// ============================================================================

/// Seed a `pending_admin_review` row whose `created_at` is exactly `age_hours`
/// hours in the past, and return its id.
///
/// Reviews are normally raised by `MatchCompletionSaga::step_validate_demos`
/// off a demo-vs-claim mismatch. Driving that three times would say nothing
/// extra about ORDER while making the timestamps — the whole subject of this
/// test — non-deterministic, so the rows are written directly with the exact
/// `created_at` the assertion depends on.
async fn seed_pending_review(
    app: &TestApp,
    match_id: &str,
    reg1: &str,
    reg2: &str,
    submitter_user_id: uuid::Uuid,
    age_hours: i64,
) -> String {
    let match_uuid: uuid::Uuid = match_id.parse().expect("match id");
    let reg1_uuid: uuid::Uuid = reg1.parse().expect("reg1 id");
    let reg2_uuid: uuid::Uuid = reg2.parse().expect("reg2 id");
    let created_at = chrono::Utc::now() - chrono::Duration::hours(age_hours);

    let claim_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO result_claims (
            match_id, submitted_by_registration_id, submitted_by_user_id,
            claimed_winner_registration_id, claimed_participant1_score,
            claimed_participant2_score, created_at
        )
        VALUES ($1, $2, $3, $2, 2, 0, $4)
        RETURNING id
        ",
    )
    .bind(match_uuid)
    .bind(reg1_uuid)
    .bind(submitter_user_id)
    .bind(created_at)
    .fetch_one(app.pool())
    .await
    .expect("insert result claim");

    let review_id: uuid::Uuid = sqlx::query_scalar(
        r"
        INSERT INTO result_reviews (
            result_claim_id, match_id, score_mismatch, status,
            captain1_registration_id, captain2_registration_id, created_at
        )
        VALUES ($1, $2, true, 'pending_admin_review', $3, $4, $5)
        RETURNING id
        ",
    )
    .bind(claim_id)
    .bind(match_uuid)
    .bind(reg1_uuid)
    .bind(reg2_uuid)
    .bind(created_at)
    .fetch_one(app.pool())
    .await
    .expect("insert result review");

    review_id.to_string()
}

/// P-55 — the admin review queue must return NEWEST FIRST.
///
/// It returned `created_at ASC`. Combined with server-side pagination that
/// meant the review which had just escalated sorted onto the LAST page, and an
/// admin working the queue from the top saw it last — the same shape as P-43,
/// where anything past page one was in practice unreachable.
///
/// The assertion is on ORDER, not on membership: an ASC queue contains exactly
/// the same three rows, so `contains` would pass against the bug.
#[tokio::test]
async fn test_admin_pending_reviews_are_newest_first() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "review-order-p55")
            .await;
    let submitter = portal_test::helpers::get_dev_user_id(app.pool()).await;

    // Seeded oldest-first so insertion order is the WRONG answer: if the
    // handler ever returns rows in physical/insertion order the assertion
    // still fails.
    let oldest = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 72).await;
    let middle = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 24).await;
    let newest = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 1).await;

    let response = app.get_auth("/v1/admin/result-reviews").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let reviews = body["data"]["reviews"]
        .as_array()
        .expect("reviews array")
        .clone();
    assert_eq!(body["data"]["total"], 3, "all three reviews are pending");

    let ids: Vec<String> = reviews
        .iter()
        .map(|r| r["id"].as_str().expect("review id").to_string())
        .collect();
    assert_eq!(
        ids,
        vec![newest.clone(), middle.clone(), oldest.clone()],
        "admin queue must be newest-first, got {ids:?}"
    );

    // And the `created_at` values the page renders must descend, so the column
    // header ("Created (newest first)") is not a claim the data contradicts.
    let timestamps: Vec<&str> = reviews
        .iter()
        .map(|r| r["created_at"].as_str().expect("created_at"))
        .collect();
    let mut descending = timestamps.clone();
    descending.sort_unstable();
    descending.reverse();
    assert_eq!(timestamps, descending, "created_at must descend");
}

/// The half of P-55 that pagination makes user-visible: the FIRST page must
/// hold the newest escalation. With `created_at ASC` and one row per page, page
/// 1 held the oldest and the fresh escalation was on the last page.
#[tokio::test]
async fn test_admin_pending_reviews_first_page_holds_the_newest() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2, _player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "review-page-p55")
            .await;
    let submitter = portal_test::helpers::get_dev_user_id(app.pool()).await;

    let _oldest = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 72).await;
    let _middle = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 24).await;
    let newest = seed_pending_review(&app, &match_id, &reg1, &reg2, submitter, 1).await;

    let response = app
        .get_auth("/v1/admin/result-reviews?page=1&per_page=1")
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let reviews = body["data"]["reviews"].as_array().expect("reviews array");
    assert_eq!(reviews.len(), 1, "per_page=1 returns one row");
    assert_eq!(
        reviews[0]["id"].as_str().expect("review id"),
        newest,
        "page 1 of the queue must be the most recent escalation"
    );
    assert_eq!(
        body["data"]["total"], 3,
        "total still counts every pending review"
    );
}
