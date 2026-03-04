//! Players API integration tests.


use axum::http::StatusCode;
use crate::common::TestApp;
use serde_json::json;

/// Generate a valid PNG image of the given dimensions.
fn generate_test_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([100, 150, 200, 255]));
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png).expect("failed to write test PNG");
    buf
}

// ============================================================================
// SEARCH PLAYERS
// ============================================================================

#[tokio::test]
async fn test_search_players() {
    let app = TestApp::new().await;

    // The dev player is seeded; a search with no filters should return it
    let response = app.get("/v1/players").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = &body["data"];
    assert!(items.is_array(), "data should be an array");
    assert!(!items.as_array().unwrap().is_empty(), "should contain at least the dev player");
}

#[tokio::test]
async fn test_search_players_with_query() {
    let app = TestApp::new().await;

    // Register two additional players
    app.post_json_no_auth(
        "/v1/auth/register",
        &json!({
            "username": "alphauser",
            "email": "alpha@example.com",
            "password": "SecurePass123!",
            "display_name": "AlphaPlayer"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    app.post_json_no_auth(
        "/v1/auth/register",
        &json!({
            "username": "betauser",
            "email": "beta@example.com",
            "password": "SecurePass123!",
            "display_name": "BetaPlayer"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Search for "Alpha"
    let response = app.get("/v1/players?q=Alpha").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = body["data"].as_array().unwrap();

    let names: Vec<&str> = items.iter().filter_map(|i| i["display_name"].as_str()).collect();
    assert!(names.contains(&"AlphaPlayer"), "should find AlphaPlayer");
    assert!(!names.contains(&"BetaPlayer"), "should not find BetaPlayer");
}

// ============================================================================
// GET PLAYER BY ID
// ============================================================================

#[tokio::test]
async fn test_get_player_by_id() {
    let app = TestApp::new().await;

    // Register a user and look up the player row
    let reg = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "getbyiduser",
                "email": "getbyid@example.com",
                "password": "SecurePass123!",
                "display_name": "GetByIdPlayer"
            }),
        )
        .await;
    reg.assert_status(StatusCode::CREATED);

    let reg_body: serde_json::Value = reg.json();
    let player_id = reg_body["data"]["player"]["id"].as_str().unwrap();

    let response = app.get(&format!("/v1/players/{player_id}")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], player_id);
    assert_eq!(body["data"]["display_name"], "GetByIdPlayer");
}

#[tokio::test]
async fn test_get_player_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/players/00000000-0000-0000-0000-ffffffffffff")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// GET MY PROFILE
// ============================================================================

#[tokio::test]
async fn test_get_my_profile() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/players/me").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["display_name"], "DevPlayer");
}

#[tokio::test]
async fn test_get_my_profile_unauthorized() {
    let app = TestApp::new().await;

    let response = app.get("/v1/players/me").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// UPDATE MY PROFILE
// ============================================================================

#[tokio::test]
async fn test_update_profile() {
    let app = TestApp::new().await;

    let response = app
        .patch_json(
            "/v1/players/me",
            &json!({
                "display_name": "UpdatedName",
                "bio": "My new bio",
                "country_code": "GB"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["display_name"], "UpdatedName");
    assert_eq!(body["data"]["bio"], "My new bio");
    assert_eq!(body["data"]["country_code"], "GB");
}

#[tokio::test]
async fn test_update_profile_validation_errors() {
    let app = TestApp::new().await;

    // display_name too short (< 3 chars)
    let response = app
        .patch_json("/v1/players/me", &json!({ "display_name": "ab" }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // display_name too long (> 32 chars)
    let long_name = "x".repeat(33);
    let response = app
        .patch_json("/v1/players/me", &json!({ "display_name": long_name }))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_update_profile_unauthorized() {
    let app = TestApp::new().await;

    let response = app
        .patch_json_no_auth("/v1/players/me", &json!({ "display_name": "Nope" }))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// UPLOAD AVATAR / BANNER
// ============================================================================

#[tokio::test]
async fn test_upload_avatar() {
    let app = TestApp::new().await;

    let png = generate_test_png(256, 256);
    let response = app
        .post_multipart_auth("/v1/players/me/avatar", "file", "avatar.png", "image/png", &png)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["avatar_url"].is_string(),
        "avatar_url should be set after upload"
    );
}

#[tokio::test]
async fn test_upload_banner() {
    let app = TestApp::new().await;

    // 4:1 aspect ratio within allowed range (3.5-4.5)
    let png = generate_test_png(400, 100);
    let response = app
        .post_multipart_auth("/v1/players/me/banner", "file", "banner.png", "image/png", &png)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["banner_url"].is_string(),
        "banner_url should be set after upload"
    );
}

#[tokio::test]
async fn test_upload_avatar_no_file_field() {
    let app = TestApp::new().await;

    let png = generate_test_png(256, 256);
    // Use wrong field name
    let response = app
        .post_multipart_auth(
            "/v1/players/me/avatar",
            "wrong_field",
            "avatar.png",
            "image/png",
            &png,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
