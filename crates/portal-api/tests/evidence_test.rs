//! Evidence API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

// ============================================================================
// EVIDENCE UPLOAD TESTS
// ============================================================================

#[tokio::test]
async fn test_initiate_upload_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to initiate upload for non-existent match
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "demo",
                "file_name": "test_demo.dem",
                "file_size_bytes": 1024000,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_initiate_upload_invalid_evidence_type() {
    let app = TestApp::new().await;

    // Try to initiate upload with invalid evidence type
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "invalid_type",
                "file_name": "test.txt",
                "file_size_bytes": 1024,
                "mime_type": "text/plain"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_complete_upload_invalid_evidence_id() {
    let app = TestApp::new().await;

    // Try to complete upload for non-existent evidence
    let response = app
        .post_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001/complete",
        )
        .await;

    // Should return internal error (evidence not found)
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// EVIDENCE LINK TESTS
// ============================================================================

#[tokio::test]
async fn test_add_link_evidence_invalid_match_id() {
    let app = TestApp::new().await;

    // Try to add link evidence for non-existent match
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "video",
                "url": "https://www.youtube.com/watch?v=test123",
                "name": "Match VOD"
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_add_link_evidence_invalid_evidence_type() {
    let app = TestApp::new().await;

    // Try to add link evidence with invalid type (demo can't be a link)
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "demo",
                "url": "https://example.com/demo",
                "name": "Invalid demo link"
            }),
        )
        .await;

    // Should fail because demo type cannot be a URL
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// EVIDENCE RETRIEVAL TESTS
// ============================================================================

#[tokio::test]
async fn test_list_evidence_for_nonexistent_match() {
    let app = TestApp::new().await;

    // List evidence for non-existent match
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/evidence")
        .await;

    // Returns 200 with empty array (endpoint doesn't verify match existence)
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_evidence_not_found() {
    let app = TestApp::new().await;

    // Get non-existent evidence
    let response = app
        .get("/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_access_url_not_found() {
    let app = TestApp::new().await;

    // Get access URL for non-existent evidence
    let response = app
        .get_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001/access",
        )
        .await;

    // Should return internal error (evidence not found)
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_delete_evidence_not_found() {
    let app = TestApp::new().await;

    // Delete non-existent evidence
    let response = app
        .delete_auth(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/00000000-0000-0000-0000-000000000001",
        )
        .await;

    // Should return internal error (evidence not found)
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// EVIDENCE DISCOVERY TESTS
// ============================================================================

#[tokio::test]
async fn test_discover_evidence_not_implemented() {
    let app = TestApp::new().await;

    // Discovery requires plugin integration
    let response = app
        .get_auth("/v1/matches/00000000-0000-0000-0000-000000000000/evidence/discover")
        .await;

    // Should return not implemented (501)
    response.assert_status(StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn test_link_discovered_evidence_not_implemented() {
    let app = TestApp::new().await;

    // Linking discovered evidence requires plugin integration
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link-discovered",
            &json!({
                "external_id": "demo_12345"
            }),
        )
        .await;

    // Should return not implemented (501)
    response.assert_status(StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn test_validate_evidence_not_implemented() {
    let app = TestApp::new().await;

    // Validation requires plugin integration
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/validate",
            &json!({
                "evidence_ids": ["00000000-0000-0000-0000-000000000001"],
                "claimed_result": {
                    "participant1_score": 2,
                    "participant2_score": 1
                }
            }),
        )
        .await;

    // Should return not implemented (501)
    response.assert_status(StatusCode::NOT_IMPLEMENTED);
}

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_evidence_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";

    // Verify GET /evidence endpoint exists
    let response = app
        .get(&format!("/v1/matches/{}/evidence", match_id))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence endpoint should exist"
    );

    // Verify POST /evidence/upload endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", match_id),
            &json!({
                "evidence_type": "demo",
                "file_name": "test.dem",
                "file_size_bytes": 1024,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/upload endpoint should exist"
    );

    // Verify POST /evidence/link endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/link", match_id),
            &json!({
                "evidence_type": "video",
                "url": "https://example.com/video",
                "name": "Test video"
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /evidence/link endpoint should exist"
    );
}

#[tokio::test]
async fn test_evidence_detail_endpoints_exist() {
    let app = TestApp::new().await;
    let match_id = "00000000-0000-0000-0000-000000000000";
    let evidence_id = "00000000-0000-0000-0000-000000000001";

    // Verify GET /evidence/{evidence_id} endpoint exists
    let response = app
        .get(&format!(
            "/v1/matches/{}/evidence/{}",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}} endpoint should exist"
    );

    // Verify GET /evidence/{evidence_id}/access endpoint exists (authenticated)
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/{}/access",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /evidence/{{id}}/access endpoint should exist"
    );

    // Verify DELETE /evidence/{evidence_id} endpoint exists (authenticated)
    let response = app
        .delete_auth(&format!(
            "/v1/matches/{}/evidence/{}",
            match_id, evidence_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "DELETE /evidence/{{id}} endpoint should exist"
    );
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_initiate_upload_missing_required_fields() {
    let app = TestApp::new().await;

    // Try to initiate upload without required fields
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/upload",
            &json!({
                "evidence_type": "demo"
                // Missing file_name, file_size_bytes, mime_type
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_link_evidence_missing_url() {
    let app = TestApp::new().await;

    // Try to add link evidence without URL
    let response = app
        .post_json(
            "/v1/matches/00000000-0000-0000-0000-000000000000/evidence/link",
            &json!({
                "evidence_type": "video",
                "name": "Test video"
                // Missing url
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}
