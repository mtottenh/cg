//! Bans API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper to grant platform_admin role to dev user (required for ban operations).
///
/// The is_admin() check in PermissionService looks for "users.view_all" permission,
/// which is granted to the platform_admin role.
async fn grant_admin_permission(app: &TestApp) {
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    // Get platform_admin role ID
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'platform_admin'")
        .fetch_one(app.pool())
        .await
        .expect("platform_admin role should exist");
    let role_id: Uuid = role_row.get("id");

    // Assign role to dev user
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(dev_user_id)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("Failed to assign role");
}

/// Helper to create a test user and return their ID
async fn create_test_user(app: &TestApp, username: &str) -> String {
    let (user_id, _token) = register_user(app, username).await;
    user_id
}

/// Register a user via the API and return (`user_id`, `access_token`).
async fn register_user(app: &TestApp, username: &str) -> (String, String) {
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
    let body: serde_json::Value = response.json();
    (
        body["data"]["user"]["id"].as_str().unwrap().to_string(),
        body["data"]["access_token"].as_str().unwrap().to_string(),
    )
}

/// Grant a role (by name) to a user via SQL.
async fn grant_role(app: &TestApp, user_id: &str, role_name: &str) {
    let user_uuid = Uuid::parse_str(user_id).unwrap();
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = $1")
        .bind(role_name)
        .fetch_one(app.pool())
        .await
        .expect("role should exist");
    let role_id: Uuid = role_row.get("id");

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user_uuid)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("Failed to assign role");
}

// ============================================================================
// AUTHORIZATION TESTS
// ============================================================================

#[tokio::test]
async fn test_list_bans_requires_admin() {
    let app = TestApp::new().await;

    // Without admin permission should fail
    let response = app.get_auth("/v1/admin/bans").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_get_ban_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/admin/bans/00000000-0000-0000-0000-000000000001")
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_create_ban_requires_admin() {
    let app = TestApp::new().await;

    // A real (non-dev) user with no roles cannot create bans.
    let (_user_id, token) = register_user(&app, "ban_create_noperm").await;

    let response = app
        .post_json_with_token(
            "/v1/admin/bans",
            &json!({
                "user_id": "00000000-0000-0000-0000-000000000002",
                "ban_type": "platform",
                "reason": "Test ban"
            }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_lift_ban_requires_admin() {
    let app = TestApp::new().await;

    // A real (non-dev) user with no roles cannot lift bans.
    let (_user_id, token) = register_user(&app, "ban_lift_noperm").await;

    let response = app
        .post_json_with_token(
            "/v1/admin/bans/00000000-0000-0000-0000-000000000001/lift",
            &json!({
                "reason": "Test lift"
            }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_view_all_only_user_cannot_mutate_bans() {
    let app = TestApp::new().await;

    // Moderator holds users.view_all (the read-level is_admin check) but NOT
    // admin.bans.manage — reads must work, mutations must be forbidden.
    let (mod_id, mod_token) = register_user(&app, "ban_moderator").await;
    grant_role(&app, &mod_id, "moderator").await;

    let target_id = create_test_user(&app, "ban_mod_target").await;

    // Read surface: list bans is allowed.
    let response = app.get_with_token("/v1/admin/bans", &mod_token).await;
    response.assert_status(StatusCode::OK);

    // Mutations: create is forbidden.
    let response = app
        .post_json_with_token(
            "/v1/admin/bans",
            &json!({
                "user_id": target_id,
                "ban_type": "platform",
                "reason": "Moderator escalation attempt"
            }),
            &mod_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Lift is forbidden too.
    let response = app
        .post_json_with_token(
            "/v1/admin/bans/00000000-0000-0000-0000-000000000001/lift",
            &json!({ "reason": "Moderator lift attempt" }),
            &mod_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_platform_admin_can_create_and_lift_bans() {
    let app = TestApp::new().await;

    // platform_admin holds admin.bans.manage (seeded in 0062).
    let (admin_id, admin_token) = register_user(&app, "ban_platform_admin").await;
    grant_role(&app, &admin_id, "platform_admin").await;

    let target_id = create_test_user(&app, "ban_pa_target").await;

    let response = app
        .post_json_with_token(
            "/v1/admin/bans",
            &json!({
                "user_id": target_id,
                "ban_type": "platform",
                "reason": "Platform admin ban"
            }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let ban_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json_with_token(
            &format!("/v1/admin/bans/{ban_id}/lift"),
            &json!({ "reason": "Appeal accepted" }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_get_user_bans_requires_admin() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/admin/users/00000000-0000-0000-0000-000000000001/bans")
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_ban_endpoints_require_authentication() {
    let app = TestApp::new().await;

    // List bans without auth
    let response = app.get("/v1/admin/bans").await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // Get ban without auth
    let response = app
        .get("/v1/admin/bans/00000000-0000-0000-0000-000000000001")
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// CREATE BAN TESTS
// ============================================================================

#[tokio::test]
async fn test_create_platform_ban() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create a user to ban
    let user_id = create_test_user(&app, "bannable_user").await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "platform",
                "reason": "Cheating detected"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["user_id"], user_id);
    assert_eq!(body["data"]["ban_type"], "platform");
    assert_eq!(body["data"]["reason"], "Cheating detected");
    assert_eq!(body["data"]["is_active"], true);
    assert_eq!(body["data"]["is_permanent"], true); // No duration means permanent
    assert!(body["data"]["id"].is_string());
}

#[tokio::test]
async fn test_create_temporary_ban() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "temp_ban_user").await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "matchmaking",
                "reason": "Leaving matches early",
                "duration_seconds": 3600  // 1 hour ban
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["ban_type"], "matchmaking");
    assert_eq!(body["data"]["is_active"], true);
    assert_eq!(body["data"]["is_permanent"], false);
    assert!(body["data"]["ends_at"].is_string()); // Should have an end date
}

#[tokio::test]
async fn test_create_chat_ban() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "chat_ban_user").await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "chat",
                "reason": "Toxic behavior in chat"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["ban_type"], "chat");
}

#[tokio::test]
async fn test_create_scoped_ban() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "scoped_ban_user").await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "league",
                "reason": "Violating league rules",
                "scope_type": "league",
                "scope_id": "00000000-0000-0000-0000-000000000099"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["ban_type"], "league");
    assert_eq!(body["data"]["scope_type"], "league");
    assert_eq!(
        body["data"]["scope_id"],
        "00000000-0000-0000-0000-000000000099"
    );
}

#[tokio::test]
async fn test_create_ban_invalid_type() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "invalid_ban_user").await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "invalid_type",
                "reason": "Test"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_ban_invalid_user_id() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": "not-a-valid-uuid",
                "ban_type": "platform",
                "reason": "Test"
            }),
        )
        .await;

    // 422 Unprocessable Entity is returned for JSON deserialization failures (invalid UUID format)
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

// ============================================================================
// LIST BANS TESTS
// ============================================================================

#[tokio::test]
async fn test_list_bans_empty() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/bans").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["items"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["pagination"]["total_items"], 0);
}

#[tokio::test]
async fn test_list_bans_with_data() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create a user and ban them
    let user_id = create_test_user(&app, "list_bans_user").await;
    let create_response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "platform",
                "reason": "Test ban for listing"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    // List bans
    let response = app.get_auth("/v1/admin/bans").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = body["data"]["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(body["data"]["pagination"]["total_items"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn test_list_bans_filter_by_user() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create two users and ban them
    let user1_id = create_test_user(&app, "filter_user1").await;
    let user2_id = create_test_user(&app, "filter_user2").await;

    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user1_id,
            "ban_type": "platform",
            "reason": "Ban user 1"
        }),
    )
    .await;

    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user2_id,
            "ban_type": "platform",
            "reason": "Ban user 2"
        }),
    )
    .await;

    // Filter by user1
    let response = app
        .get_auth(&format!("/v1/admin/bans?user_id={user1_id}"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["user_id"], user1_id);
}

#[tokio::test]
async fn test_list_bans_filter_by_type() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "type_filter_user").await;

    // Create platform and chat bans
    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user_id,
            "ban_type": "platform",
            "reason": "Platform ban"
        }),
    )
    .await;

    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user_id,
            "ban_type": "chat",
            "reason": "Chat ban"
        }),
    )
    .await;

    // Filter by platform type
    let response = app.get_auth("/v1/admin/bans?ban_type=platform").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = body["data"]["items"].as_array().unwrap();
    for item in items {
        assert_eq!(item["ban_type"], "platform");
    }
}

#[tokio::test]
async fn test_list_bans_pagination() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create multiple users and bans
    for i in 0..5 {
        let user_id = create_test_user(&app, &format!("pagination_user_{i}")).await;
        app.post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "platform",
                "reason": format!("Ban {}", i)
            }),
        )
        .await;
    }

    // Get first page with 2 items
    let response = app.get_auth("/v1/admin/bans?page=1&per_page=2").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(body["data"]["pagination"]["page"], 1);
    assert_eq!(body["data"]["pagination"]["per_page"], 2);
    assert!(body["data"]["pagination"]["total_items"].as_i64().unwrap() >= 5);
}

// ============================================================================
// GET BAN TESTS
// ============================================================================

#[tokio::test]
async fn test_get_ban_by_id() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "get_ban_user").await;

    // Create a ban
    let create_response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "platform",
                "reason": "Test ban"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = create_response.json();
    let ban_id = create_body["data"]["id"].as_str().unwrap();

    // Get the ban
    let response = app.get_auth(&format!("/v1/admin/bans/{ban_id}")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], ban_id);
    assert_eq!(body["data"]["user_id"], user_id);
    assert_eq!(body["data"]["ban_type"], "platform");
}

#[tokio::test]
async fn test_get_ban_not_found() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app
        .get_auth("/v1/admin/bans/00000000-0000-0000-0000-000000000999")
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_ban_invalid_id() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/bans/not-a-uuid").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// LIFT BAN TESTS
// ============================================================================

#[tokio::test]
async fn test_lift_ban() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "lift_ban_user").await;

    // Create a ban
    let create_response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "platform",
                "reason": "Original ban reason"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = create_response.json();
    let ban_id = create_body["data"]["id"].as_str().unwrap();

    // Lift the ban
    let response = app
        .post_json(
            &format!("/v1/admin/bans/{ban_id}/lift"),
            &json!({
                "reason": "User appealed successfully"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["is_active"], false);
    assert!(body["data"]["lifted_at"].is_string());
    assert!(body["data"]["lifted_by"].is_string());
    assert_eq!(body["data"]["lift_reason"], "User appealed successfully");
}

#[tokio::test]
async fn test_lift_ban_without_reason() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "lift_no_reason_user").await;

    // Create and lift a ban without providing a reason
    let create_response = app
        .post_json(
            "/v1/admin/bans",
            &json!({
                "user_id": user_id,
                "ban_type": "chat",
                "reason": "Chat ban"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let create_body: serde_json::Value = create_response.json();
    let ban_id = create_body["data"]["id"].as_str().unwrap();

    let response = app
        .post_json(&format!("/v1/admin/bans/{ban_id}/lift"), &json!({}))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["is_active"], false);
    assert!(body["data"]["lift_reason"].is_null());
}

#[tokio::test]
async fn test_lift_ban_not_found() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app
        .post_json(
            "/v1/admin/bans/00000000-0000-0000-0000-000000000999/lift",
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// USER BAN HISTORY TESTS
// ============================================================================

#[tokio::test]
async fn test_get_user_ban_history() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "history_user").await;

    // Create multiple bans for the same user
    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user_id,
            "ban_type": "chat",
            "reason": "First offense"
        }),
    )
    .await;

    app.post_json(
        "/v1/admin/bans",
        &json!({
            "user_id": user_id,
            "ban_type": "matchmaking",
            "reason": "Second offense"
        }),
    )
    .await;

    // Get ban history
    let response = app
        .get_auth(&format!("/v1/admin/users/{user_id}/bans"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let bans = body["data"].as_array().unwrap();
    assert_eq!(bans.len(), 2);

    // All bans should be for this user
    for ban in bans {
        assert_eq!(ban["user_id"], user_id);
    }
}

#[tokio::test]
async fn test_get_user_ban_history_empty() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "clean_user").await;

    let response = app
        .get_auth(&format!("/v1/admin/users/{user_id}/bans"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let bans = body["data"].as_array().unwrap();
    assert!(bans.is_empty());
}

#[tokio::test]
async fn test_get_user_ban_history_invalid_user_id() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/users/not-a-uuid/bans").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
