//! S3 evidence upload integration tests using MinIO.
//!
//! Tests the full presigned-URL upload lifecycle:
//! initiate_upload → PUT to presigned URL → complete_upload → access
//!
//! Verifies:
//! - Pending → Active status transitions
//! - Pending evidence excluded from list queries
//! - complete_upload fails without actual file
//! - Human-readable S3 key structure
//! - Deletion removes S3 objects

use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;

use crate::common::TestApp;
use crate::common::minio::{create_bucket, create_s3_client, start_minio};

const EVIDENCE_BUCKET: &str = "test-evidence";

// ============================================================================
// TOURNAMENT SETUP HELPER
// ============================================================================

struct TestMatchInfo {
    #[allow(dead_code)]
    tournament_id: String,
    match_id: String,
}

/// Create a started CS2 tournament with at least one match.
async fn create_tournament_with_match(app: &TestApp, slug: &str) -> TestMatchInfo {
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id.to_string(),
                "name": format!("S3 Evidence Test {}", slug),
                "slug": slug,
                "format": "single_elimination",
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
    let tournament_uuid: uuid::Uuid = tournament_id.parse().unwrap();

    // Publish
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Register player 1 (dev user)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Player1" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let reg1 = body["data"]["id"].as_str().unwrap().to_string();

    // Approve registration 1
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/registrations/{reg1}/approve"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Register player 2 (via builder)
    let user2 = UserBuilder::new()
        .username(format!("s3_player2_{slug}"))
        .build_persisted(app.pool())
        .await;

    let _reg2 = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(user2.id)
        .participant_name("Player2")
        .registered_by_uuid(user2.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    // Seed and start
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    // Get match info
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/matches"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(
        !matches.is_empty(),
        "Tournament should have at least one match"
    );

    let match_id = matches[0]["id"].as_str().unwrap().to_string();

    TestMatchInfo {
        tournament_id,
        match_id,
    }
}

// ============================================================================
// TESTS
// ============================================================================

/// Full S3 evidence upload lifecycle: initiate → PUT → complete → access → download.
#[tokio::test]
async fn test_evidence_upload_s3_full_flow() {
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket(&s3_client, EVIDENCE_BUCKET).await;

    let app = TestApp::new_with_s3(&minio_endpoint, EVIDENCE_BUCKET).await;
    let info = create_tournament_with_match(&app, "s3-full-flow").await;

    let file_content = b"this is test demo content for S3 upload";

    // 1. Initiate upload
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", info.match_id),
            &json!({
                "evidence_type": "demo",
                "file_name": "match_demo.dem",
                "file_size_bytes": file_content.len(),
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["evidence_id"].as_str().unwrap().to_string();
    let upload_url = body["data"]["upload_url"].as_str().unwrap().to_string();

    // upload_url should be a presigned S3 URL pointing at MinIO
    assert!(
        upload_url.contains(&minio_endpoint.replace("http://", "")),
        "Presigned URL should point to MinIO, got: {upload_url}"
    );

    // 2. Verify evidence is pending (GET detail should show pending status)
    let response = app
        .get(&format!(
            "/v1/matches/{}/evidence/{}",
            info.match_id, evidence_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "pending");

    // 3. PUT file bytes to presigned URL
    let http_client = reqwest::Client::new();
    let put_response = http_client
        .put(&upload_url)
        .header("Content-Type", "application/octet-stream")
        .body(file_content.to_vec())
        .send()
        .await
        .expect("PUT to presigned URL failed");
    assert!(
        put_response.status().is_success(),
        "PUT to MinIO should succeed, got {}",
        put_response.status()
    );

    // 4. Complete upload
    let response = app
        .post_auth(&format!(
            "/v1/matches/{}/evidence/{}/complete",
            info.match_id, evidence_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "active");

    // 5. Get access URL
    let response = app
        .get_auth(&format!(
            "/v1/matches/{}/evidence/{}/access",
            info.match_id, evidence_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let access_url = body["data"]["url"].as_str().unwrap();

    // 6. Download via presigned GET URL and verify content
    let get_response = http_client
        .get(access_url)
        .send()
        .await
        .expect("GET from presigned URL failed");
    assert!(get_response.status().is_success());

    let downloaded = get_response.bytes().await.unwrap();
    assert_eq!(
        downloaded.as_ref(),
        file_content,
        "Downloaded content should match uploaded content"
    );
}

/// Pending evidence is excluded from list queries.
#[tokio::test]
async fn test_evidence_upload_s3_pending_not_listed() {
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket(&s3_client, EVIDENCE_BUCKET).await;

    let app = TestApp::new_with_s3(&minio_endpoint, EVIDENCE_BUCKET).await;
    let info = create_tournament_with_match(&app, "s3-pending-filter").await;

    // Initiate upload (creates Pending record)
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", info.match_id),
            &json!({
                "evidence_type": "screenshot",
                "file_name": "scoreboard.png",
                "file_size_bytes": 1024,
                "mime_type": "image/png"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["evidence_id"].as_str().unwrap().to_string();
    let upload_url = body["data"]["upload_url"].as_str().unwrap().to_string();

    // List evidence — pending should be excluded
    let response = app
        .get(&format!("/v1/matches/{}/evidence", info.match_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "Pending evidence should not appear in list"
    );

    // Upload the file and complete
    let http_client = reqwest::Client::new();
    http_client
        .put(&upload_url)
        .header("Content-Type", "image/png")
        .body(vec![0u8; 1024])
        .send()
        .await
        .unwrap();

    app.post_auth(&format!(
        "/v1/matches/{}/evidence/{}/complete",
        info.match_id, evidence_id
    ))
    .await
    .assert_status(StatusCode::OK);

    // Now list should include the evidence
    let response = app
        .get(&format!("/v1/matches/{}/evidence", info.match_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let evidence_list = body["data"].as_array().unwrap();
    assert_eq!(
        evidence_list.len(),
        1,
        "Active evidence should appear in list"
    );
}

/// complete_upload fails if the file was never PUT to S3.
#[tokio::test]
async fn test_evidence_upload_s3_complete_without_file() {
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket(&s3_client, EVIDENCE_BUCKET).await;

    let app = TestApp::new_with_s3(&minio_endpoint, EVIDENCE_BUCKET).await;
    let info = create_tournament_with_match(&app, "s3-no-file").await;

    // Initiate upload but don't PUT the file
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", info.match_id),
            &json!({
                "evidence_type": "demo",
                "file_name": "missing.dem",
                "file_size_bytes": 5000,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["evidence_id"].as_str().unwrap();

    // Try to complete without uploading — should fail
    let response = app
        .post_auth(&format!(
            "/v1/matches/{}/evidence/{}/complete",
            info.match_id, evidence_id
        ))
        .await;

    // Should be 400 (InvalidState → maps to BadRequest or similar)
    assert!(
        response.status == StatusCode::BAD_REQUEST
            || response.status == StatusCode::UNPROCESSABLE_ENTITY
            || response.status == StatusCode::INTERNAL_SERVER_ERROR,
        "complete_upload without file should fail, got {}",
        response.status
    );

    let body: serde_json::Value = response.json();
    let detail = body["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("not found in storage") || detail.contains("file not found"),
        "Error should mention missing file, got: {detail}"
    );
}

/// Presigned URL contains human-readable key path from tournament slug.
#[tokio::test]
async fn test_evidence_upload_s3_human_readable_key() {
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket(&s3_client, EVIDENCE_BUCKET).await;

    let app = TestApp::new_with_s3(&minio_endpoint, EVIDENCE_BUCKET).await;
    let info = create_tournament_with_match(&app, "s3-slug-keys").await;

    // Initiate upload
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", info.match_id),
            &json!({
                "evidence_type": "demo",
                "file_name": "replay.dem",
                "file_size_bytes": 2048,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let upload_url = body["data"]["upload_url"].as_str().unwrap();

    // URL-decode the presigned URL path to check the key
    // The key should contain the tournament slug and evidence type directory
    // Format: {tournament-slug}/evidence/demos/R{round}M{match}/...
    assert!(
        upload_url.contains("s3-slug-keys") && upload_url.contains("/evidence/demos/"),
        "Presigned URL should contain tournament slug and evidence type dir.\nGot: {upload_url}"
    );
}

/// Deleting evidence removes the S3 object.
#[tokio::test]
async fn test_evidence_delete_s3() {
    let (_minio, minio_endpoint) = start_minio().await;
    let s3_client = create_s3_client(&minio_endpoint).await;
    create_bucket(&s3_client, EVIDENCE_BUCKET).await;

    let app = TestApp::new_with_s3(&minio_endpoint, EVIDENCE_BUCKET).await;
    let info = create_tournament_with_match(&app, "s3-delete").await;

    let file_content = b"evidence file to delete";

    // Full upload flow
    let response = app
        .post_json(
            &format!("/v1/matches/{}/evidence/upload", info.match_id),
            &json!({
                "evidence_type": "screenshot",
                "file_name": "scoreboard.png",
                "file_size_bytes": file_content.len(),
                "mime_type": "image/png"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let evidence_id = body["data"]["evidence_id"].as_str().unwrap().to_string();
    let upload_url = body["data"]["upload_url"].as_str().unwrap().to_string();

    // PUT the file
    let http_client = reqwest::Client::new();
    http_client
        .put(&upload_url)
        .header("Content-Type", "image/png")
        .body(file_content.to_vec())
        .send()
        .await
        .unwrap();

    // Complete upload
    app.post_auth(&format!(
        "/v1/matches/{}/evidence/{}/complete",
        info.match_id, evidence_id
    ))
    .await
    .assert_status(StatusCode::OK);

    // Count objects in bucket before deletion
    let objects_before = s3_client
        .list_objects_v2()
        .bucket(EVIDENCE_BUCKET)
        .send()
        .await
        .unwrap();
    let count_before = objects_before.contents().len();
    assert!(
        count_before > 0,
        "Should have at least one object in bucket before deletion"
    );

    // Delete evidence
    let response = app
        .delete_auth(&format!(
            "/v1/matches/{}/evidence/{}",
            info.match_id, evidence_id
        ))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify file is removed from S3
    let objects_after = s3_client
        .list_objects_v2()
        .bucket(EVIDENCE_BUCKET)
        .send()
        .await
        .unwrap();
    let count_after = objects_after.contents().len();
    assert_eq!(
        count_after,
        count_before - 1,
        "S3 object should be removed after evidence deletion"
    );
}
