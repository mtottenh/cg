//! Result review API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
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
        .get_auth(&format!("/v1/matches/{}/result-review", match_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /matches/{{match_id}}/result-review endpoint should exist"
    );

    // Verify acknowledge result review endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/matches/{}/result-review/acknowledge?registration_id=00000000-0000-0000-0000-000000000003",
                match_id
            ),
            &json!({}),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /matches/{{match_id}}/result-review/acknowledge endpoint should exist"
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
        .get_auth(&format!("/v1/admin/result-reviews/{}", review_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /admin/result-reviews/{{id}} endpoint should exist"
    );

    // Verify admin approve endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/admin/result-reviews/{}/approve", review_id),
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
            &format!("/v1/admin/result-reviews/{}/reject", review_id),
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
        .get_auth(&format!("/v1/matches/{}/result-review", match_id))
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
    let response = app.get(&format!("/v1/matches/{}/result-review", match_id)).await;

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
                "/v1/matches/{}/result-review/acknowledge?registration_id={}",
                match_id, registration_id
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
            &format!(
                "/v1/matches/{}/result-review/acknowledge?registration_id=not-a-uuid",
                match_id
            ),
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
        .get_auth(&format!("/v1/admin/result-reviews/{}", review_id))
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
            &format!("/v1/admin/result-reviews/{}/approve", review_id),
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
            &format!("/v1/admin/result-reviews/{}/reject", review_id),
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
        .get(&format!("/v1/admin/result-reviews/{}", review_id))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}
