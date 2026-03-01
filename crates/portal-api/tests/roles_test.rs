//! Roles, Permissions, User Role Assignments, and Admin Stats integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Grant platform_admin role to the seeded dev user.
async fn grant_admin_permission(app: &TestApp) {
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'platform_admin'")
        .fetch_one(app.pool())
        .await
        .expect("platform_admin role should exist");
    let role_id: Uuid = role_row.get("id");

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(dev_user_id)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("Failed to assign role");
}

/// Register a test user via the API and return their user ID.
async fn create_test_user(app: &TestApp, username: &str) -> String {
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": "SecurePass123!",
                "display_name": username
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    body["data"]["user"]["id"].as_str().unwrap().to_string()
}

// ============================================================================
// ADMIN STATS
// ============================================================================

#[tokio::test]
async fn test_get_stats_requires_auth() {
    let app = TestApp::new().await;

    let response = app.get("/v1/admin/stats").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_stats_requires_admin() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/admin/stats").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_get_stats() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/stats").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(
        body["data"]["total_users"].as_i64().unwrap() >= 1,
        "should have at least 1 user (dev user)"
    );
    assert!(
        body["data"]["total_players"].as_i64().unwrap() >= 1,
        "should have at least 1 player (dev player)"
    );
}

// ============================================================================
// ROLE CRUD
// ============================================================================

#[tokio::test]
async fn test_list_roles_requires_admin() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/admin/roles").await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_list_roles() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/roles").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let roles = body["data"].as_array().expect("data should be an array");

    let names: Vec<&str> = roles.iter().filter_map(|r| r["name"].as_str()).collect();
    assert!(names.contains(&"super_admin"), "should contain super_admin");
    assert!(names.contains(&"platform_admin"), "should contain platform_admin");
    assert!(names.contains(&"moderator"), "should contain moderator");
    assert!(names.contains(&"user"), "should contain user");
}

#[tokio::test]
async fn test_create_role() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app
        .post_json(
            "/v1/admin/roles",
            &json!({
                "name": "test_custom_role",
                "display_name": "Test Custom Role",
                "category": "custom"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "test_custom_role");
    assert_eq!(body["data"]["display_name"], "Test Custom Role");
    assert_eq!(body["data"]["is_system"], false);
}

#[tokio::test]
async fn test_create_role_duplicate_name() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let payload = json!({
        "name": "dup_role",
        "display_name": "Duplicate Role",
        "category": "custom"
    });

    let response = app.post_json("/v1/admin/roles", &payload).await;
    response.assert_status(StatusCode::CREATED);

    let response = app.post_json("/v1/admin/roles", &payload).await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_get_role_with_permissions() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create a role and fetch it by ID
    let create = app
        .post_json(
            "/v1/admin/roles",
            &json!({
                "name": "get_role_test",
                "display_name": "Get Role Test",
                "category": "custom"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);

    let role_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app.get_auth(&format!("/v1/admin/roles/{role_id}")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], role_id);
    assert!(body["data"]["permissions"].is_array(), "should include permissions array");
}

#[tokio::test]
async fn test_update_role() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let create = app
        .post_json(
            "/v1/admin/roles",
            &json!({
                "name": "update_role_test",
                "display_name": "Before Update",
                "category": "custom"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);

    let role_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .patch_json(
            &format!("/v1/admin/roles/{role_id}"),
            &json!({
                "display_name": "After Update",
                "priority": 42
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["display_name"], "After Update");
    assert_eq!(body["data"]["priority"], 42);
}

#[tokio::test]
async fn test_delete_role() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let create = app
        .post_json(
            "/v1/admin/roles",
            &json!({
                "name": "delete_me",
                "display_name": "Delete Me",
                "category": "custom"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);

    let role_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app.delete_auth(&format!("/v1/admin/roles/{role_id}")).await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Confirm it's gone
    let response = app.get_auth(&format!("/v1/admin/roles/{role_id}")).await;
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_system_role_fails() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Look up the super_admin role ID
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'super_admin'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let role_id: Uuid = role_row.get("id");

    let response = app
        .delete_auth(&format!("/v1/admin/roles/{role_id}"))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// PERMISSIONS
// ============================================================================

#[tokio::test]
async fn test_list_permissions() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let response = app.get_auth("/v1/admin/permissions").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let perms = body["data"].as_array().expect("data should be an array");
    assert!(!perms.is_empty(), "should have seeded permissions");

    // Each permission should have expected fields
    let first = &perms[0];
    assert!(first["id"].is_string());
    assert!(first["name"].is_string());
    assert!(first["category"].is_string());
}

#[tokio::test]
async fn test_add_and_remove_permission() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    // Create a custom role
    let create = app
        .post_json(
            "/v1/admin/roles",
            &json!({
                "name": "perm_test_role",
                "display_name": "Perm Test Role",
                "category": "custom"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);

    let role_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Pick a permission to add
    let perms_resp = app.get_auth("/v1/admin/permissions").await;
    perms_resp.assert_status(StatusCode::OK);
    let perms_body: serde_json::Value = perms_resp.json();
    let perm_id = perms_body["data"][0]["id"].as_str().unwrap().to_string();

    // Add permission to role
    let add_resp = app
        .post_json(
            &format!("/v1/admin/roles/{role_id}/permissions"),
            &json!({ "permission_id": perm_id }),
        )
        .await;
    add_resp.assert_status(StatusCode::OK);

    let add_body: serde_json::Value = add_resp.json();
    let perm_names: Vec<&str> = add_body["data"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(perm_names.contains(&perm_id.as_str()), "permission should be added");

    // Remove permission from role
    let remove_resp = app
        .delete_auth(&format!("/v1/admin/roles/{role_id}/permissions/{perm_id}"))
        .await;
    remove_resp.assert_status(StatusCode::OK);

    let remove_body: serde_json::Value = remove_resp.json();
    let perm_ids_after: Vec<&str> = remove_body["data"]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(
        !perm_ids_after.contains(&perm_id.as_str()),
        "permission should be removed"
    );
}

// ============================================================================
// USER ROLE ASSIGNMENTS
// ============================================================================

#[tokio::test]
async fn test_assign_role_to_user() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "assign_role_user").await;

    // Get the moderator role ID
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'moderator'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let role_id: Uuid = role_row.get("id");

    let response = app
        .post_json(
            &format!("/v1/admin/users/{user_id}/roles"),
            &json!({ "role_id": role_id.to_string() }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["role"]["name"], "moderator");
}

#[tokio::test]
async fn test_get_user_roles() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "get_roles_user").await;

    // Assign moderator role
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'moderator'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let role_id: Uuid = role_row.get("id");

    app.post_json(
        &format!("/v1/admin/users/{user_id}/roles"),
        &json!({ "role_id": role_id.to_string() }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get user roles
    let response = app
        .get_auth(&format!("/v1/admin/users/{user_id}/roles"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let roles = body["data"].as_array().unwrap();
    let role_names: Vec<&str> = roles
        .iter()
        .filter_map(|r| r["role"]["name"].as_str())
        .collect();
    assert!(
        role_names.contains(&"moderator"),
        "should include the assigned moderator role"
    );
}

#[tokio::test]
async fn test_revoke_role_from_user() {
    let app = TestApp::new().await;
    grant_admin_permission(&app).await;

    let user_id = create_test_user(&app, "revoke_role_user").await;

    // Assign moderator role
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'moderator'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let role_id: Uuid = role_row.get("id");

    app.post_json(
        &format!("/v1/admin/users/{user_id}/roles"),
        &json!({ "role_id": role_id.to_string() }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Revoke the role
    let response = app
        .delete_auth(&format!("/v1/admin/users/{user_id}/roles/{role_id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify it's gone
    let get_resp = app
        .get_auth(&format!("/v1/admin/users/{user_id}/roles"))
        .await;
    get_resp.assert_status(StatusCode::OK);

    let body: serde_json::Value = get_resp.json();
    let roles = body["data"].as_array().unwrap();
    let role_names: Vec<&str> = roles
        .iter()
        .filter_map(|r| r["role"]["name"].as_str())
        .collect();
    assert!(
        !role_names.contains(&"moderator"),
        "moderator role should be revoked"
    );
}
