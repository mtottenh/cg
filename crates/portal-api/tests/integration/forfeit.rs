//! Forfeit API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use serde_json::json;

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_forfeit_endpoints_exist() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let registration_id = "00000000-0000-0000-0000-000000000001";
    let match_id = "00000000-0000-0000-0000-000000000002";

    // Verify withdraw endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/tournaments/{}/registrations/{}/withdraw",
                tournament_id, registration_id
            ),
            &json!({
                "reason": "Test withdrawal"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /withdraw endpoint should exist"
    );

    // Verify admin forfeit endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/forfeit",
                tournament_id, match_id
            ),
            &json!({
                "forfeiting_registration_id": registration_id,
                "forfeit_type": "no_show",
                "reason": "Test forfeit"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/forfeit endpoint should exist"
    );

    // Verify admin double-forfeit endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{}/double-forfeit",
                tournament_id, match_id
            ),
            &json!({
                "reason": "Both teams failed to show"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/double-forfeit endpoint should exist"
    );

    // Verify admin disqualify endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/registrations/{}/disqualify",
                tournament_id, registration_id
            ),
            &json!({
                "reason": "Cheating detected during tournament"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disqualify endpoint should exist"
    );
}

// ============================================================================
// WITHDRAW TESTS
// ============================================================================

#[tokio::test]
async fn test_withdraw_invalid_tournament_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/not-a-uuid/registrations/00000000-0000-0000-0000-000000000001/withdraw",
            &json!({
                "reason": "Test withdrawal"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_withdraw_invalid_registration_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/registrations/not-a-uuid/withdraw",
            &json!({
                "reason": "Test withdrawal"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_withdraw_nonexistent_registration() {
    let app = TestApp::new().await;

    // Try to withdraw a non-existent registration
    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/registrations/00000000-0000-0000-0000-000000000001/withdraw",
            &json!({
                "reason": "Test withdrawal"
            }),
        )
        .await;

    // Should return not found
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_withdraw_requires_auth() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/registrations/00000000-0000-0000-0000-000000000001/withdraw",
            &json!({
                "reason": "Test withdrawal"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ADMIN FORFEIT TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_forfeit_invalid_match_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/not-a-uuid/forfeit",
            &json!({
                "forfeiting_registration_id": "00000000-0000-0000-0000-000000000001",
                "forfeit_type": "no_show",
                "reason": "Test forfeit"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_forfeit_invalid_registration_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/forfeit",
            &json!({
                "forfeiting_registration_id": "not-a-uuid",
                "forfeit_type": "no_show",
                "reason": "Test forfeit"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_forfeit_invalid_forfeit_type() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/forfeit",
            &json!({
                "forfeiting_registration_id": "00000000-0000-0000-0000-000000000001",
                "forfeit_type": "invalid_type",
                "reason": "Test forfeit"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_forfeit_nonexistent_match() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/forfeit",
            &json!({
                "forfeiting_registration_id": "00000000-0000-0000-0000-000000000001",
                "forfeit_type": "no_show",
                "reason": "Test forfeit"
            }),
        )
        .await;

    // Should return not found for non-existent match
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_forfeit_missing_reason() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/forfeit",
            &json!({
                "forfeiting_registration_id": "00000000-0000-0000-0000-000000000001",
                "forfeit_type": "no_show"
            }),
        )
        .await;

    // Missing required field
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN DOUBLE FORFEIT TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_double_forfeit_invalid_match_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/not-a-uuid/double-forfeit",
            &json!({
                "reason": "Both teams failed to show"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_double_forfeit_nonexistent_match() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/double-forfeit",
            &json!({
                "reason": "Both teams failed to show"
            }),
        )
        .await;

    // Should return not found for non-existent match
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_double_forfeit_missing_reason() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000002/double-forfeit",
            &json!({}),
        )
        .await;

    // Missing required field
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN DISQUALIFY TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_disqualify_invalid_registration_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/registrations/not-a-uuid/disqualify",
            &json!({
                "reason": "Cheating detected during tournament"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_disqualify_nonexistent_registration() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/registrations/00000000-0000-0000-0000-000000000001/disqualify",
            &json!({
                "reason": "Cheating detected during tournament"
            }),
        )
        .await;

    // Should return not found for non-existent registration
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_disqualify_missing_reason() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/registrations/00000000-0000-0000-0000-000000000001/disqualify",
            &json!({}),
        )
        .await;

    // Missing required field
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_disqualify_reason_too_short() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/tournaments/00000000-0000-0000-0000-000000000000/registrations/00000000-0000-0000-0000-000000000001/disqualify",
            &json!({
                "reason": "Short"  // Less than 10 characters
            }),
        )
        .await;

    // Validation should fail
    response.assert_status(StatusCode::BAD_REQUEST);
}
