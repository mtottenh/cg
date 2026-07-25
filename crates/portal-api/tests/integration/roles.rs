//! Roles, Permissions, User Role Assignments, and Admin Stats integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
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
                "email": format!("{username}@example.com"),
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

/// Grant a role (by name) to a user via SQL. Returns the role ID.
async fn grant_role(app: &TestApp, user_id: &str, role_name: &str) -> Uuid {
    let user_uuid = Uuid::parse_str(user_id).unwrap();
    let role_id = role_id_by_name(app, role_name).await;

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user_uuid)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("Failed to assign role");
    role_id
}

/// Look up a seeded role's ID by name.
async fn role_id_by_name(app: &TestApp, role_name: &str) -> Uuid {
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = $1")
        .bind(role_name)
        .fetch_one(app.pool())
        .await
        .expect("role should exist");
    role_row.get("id")
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
    assert!(
        names.contains(&"platform_admin"),
        "should contain platform_admin"
    );
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
    assert!(
        body["data"]["permissions"].is_array(),
        "should include permissions array"
    );
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

    let response = app.delete_auth(&format!("/v1/admin/roles/{role_id}")).await;
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
    assert!(
        add_body["data"]["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["id"].as_str())
            .any(|id| id == perm_id.as_str()),
        "permission should be added"
    );

    // Remove permission from role
    let remove_resp = app
        .delete_auth(&format!("/v1/admin/roles/{role_id}/permissions/{perm_id}"))
        .await;
    remove_resp.assert_status(StatusCode::OK);

    let remove_body: serde_json::Value = remove_resp.json();
    assert!(
        !remove_body["data"]["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["id"].as_str())
            .any(|id| id == perm_id.as_str()),
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
    assert!(
        roles
            .iter()
            .filter_map(|r| r["role"]["name"].as_str())
            .any(|name| name == "moderator"),
        "should include the assigned moderator role"
    );
}

// ============================================================================
// MUTATION AUTHORIZATION (admin.users.manage) + PRIORITY CEILING
// ============================================================================

/// Regression test for the moderator self-escalation vulnerability: a user
/// holding only the READ permission `users.view_all` (via the moderator role)
/// could previously create roles and assign themselves `super_admin`.
#[tokio::test]
async fn test_view_all_only_user_can_read_but_not_mutate_roles() {
    let app = TestApp::new().await;

    // Moderator holds users.view_all but NOT admin.users.manage.
    let (mod_id, mod_token) = register_user(&app, "roles_moderator").await;
    grant_role(&app, &mod_id, "moderator").await;

    // Read surfaces still work (is_admin == users.view_all).
    let response = app.get_with_token("/v1/admin/roles", &mod_token).await;
    response.assert_status(StatusCode::OK);

    // Creating roles is forbidden.
    let response = app
        .post_json_with_token(
            "/v1/admin/roles",
            &json!({
                "name": "escalation_role",
                "display_name": "Escalation Role",
                "category": "custom"
            }),
            &mod_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Self-assigning super_admin is forbidden.
    let super_admin_id = role_id_by_name(&app, "super_admin").await;
    let response = app
        .post_json_with_token(
            &format!("/v1/admin/users/{mod_id}/roles"),
            &json!({ "role_id": super_admin_id.to_string() }),
            &mod_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Priority ceiling: platform_admin (900) may grant lower-priority roles but
/// never roles at or above its own tier (platform_admin, super_admin).
#[tokio::test]
async fn test_platform_admin_priority_ceiling() {
    let app = TestApp::new().await;

    let (admin_id, admin_token) = register_user(&app, "roles_platform_admin").await;
    grant_role(&app, &admin_id, "platform_admin").await;

    let target_id = create_test_user(&app, "ceiling_target").await;

    // Granting a lower-priority role (moderator, 500 < 900) succeeds.
    let moderator_id = role_id_by_name(&app, "moderator").await;
    let response = app
        .post_json_with_token(
            &format!("/v1/admin/users/{target_id}/roles"),
            &json!({ "role_id": moderator_id.to_string() }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Granting super_admin (1000 >= 900) is forbidden.
    let super_admin_id = role_id_by_name(&app, "super_admin").await;
    let response = app
        .post_json_with_token(
            &format!("/v1/admin/users/{target_id}/roles"),
            &json!({ "role_id": super_admin_id.to_string() }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Granting platform_admin (900 >= 900, same tier) is forbidden too.
    let platform_admin_id = role_id_by_name(&app, "platform_admin").await;
    let response = app
        .post_json_with_token(
            &format!("/v1/admin/users/{target_id}/roles"),
            &json!({ "role_id": platform_admin_id.to_string() }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

/// super_admin is exempt from the ceiling and can grant super_admin.
#[tokio::test]
async fn test_super_admin_can_assign_super_admin() {
    let app = TestApp::new().await;

    let (admin_id, admin_token) = register_user(&app, "roles_super_admin").await;
    grant_role(&app, &admin_id, "super_admin").await;

    let target_id = create_test_user(&app, "super_target").await;

    let super_admin_id = role_id_by_name(&app, "super_admin").await;
    let response = app
        .post_json_with_token(
            &format!("/v1/admin/users/{target_id}/roles"),
            &json!({ "role_id": super_admin_id.to_string() }),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
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
    assert!(
        !roles
            .iter()
            .filter_map(|r| r["role"]["name"].as_str())
            .any(|name| name == "moderator"),
        "moderator role should be revoked"
    );
}

// ============================================================================
// P-140 — EVERY DECLARED PERMISSION MUST BE SEEDED AND GRANTED
// ============================================================================

/// A permission constant that no role holds is a gate nobody can pass.
///
/// `admin.system.manage` sat in `portal_core::permissions::admin` from the day
/// admin permissions were introduced and was **never seeded** — so
/// `submit_player_rating`, the only path that can correct a bad scraped rating,
/// returned 403 to every real caller including `super_admin`, for the endpoint's
/// entire life.
///
/// It stayed invisible because of a gap in the *tests*, not the code:
/// `PermissionChecker` short-circuits for the dev user in `test-utils` builds,
/// so every integration test calling an endpoint as `dev-token` passes the gate
/// without ever consulting the `permissions` table. And no UI called it (that
/// was P-68), so no human hit the 403 either. Two independent blind spots
/// covering the same defect.
///
/// This asserts against the DATABASE rather than through a handler, which is
/// what makes it immune to the dev-user bypass that hid the original.
#[tokio::test]
async fn test_every_declared_permission_is_seeded_and_granted() {
    let app = TestApp::new().await;

    // Every permission the code can gate on. The scoped registries are
    // included, not just `admin::ALL`: the P-72 admin score override is gated
    // on `tournament.results.manage`, and an unseeded SCOPED permission fails
    // exactly the same silent-403 way an unseeded admin one does. Covering
    // only `admin::` left four registries where the original defect could
    // recur unnoticed.
    let declared: Vec<&str> = portal_core::permissions::admin::ALL
        .iter()
        .chain(portal_core::permissions::tournament::ALL)
        .chain(portal_core::permissions::league::ALL)
        .chain(portal_core::permissions::team::ALL)
        .chain(portal_core::permissions::match_::ALL)
        .copied()
        .collect();
    assert!(
        declared.len() >= 25,
        "permission registry looks empty ({} entries) — this test would pass vacuously",
        declared.len()
    );

    let mut unseeded = Vec::new();
    let mut ungranted = Vec::new();

    for name in &declared {
        let permission_id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM permissions WHERE name = $1")
                .bind(name)
                .fetch_optional(app.pool())
                .await
                .expect("query permissions");

        match permission_id {
            None => unseeded.push(*name),
            Some(id) => {
                let holders: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM role_permissions WHERE permission_id = $1",
                )
                .bind(id)
                .fetch_one(app.pool())
                .await
                .expect("query role_permissions");
                if holders == 0 {
                    ungranted.push(*name);
                }
            }
        }
    }

    assert!(
        unseeded.is_empty(),
        "these permissions are declared in code but absent from the `permissions` \
         table, so nothing can ever hold them: {unseeded:?}"
    );
    assert!(
        ungranted.is_empty(),
        "these permissions exist but are granted to NO role, so every caller is \
         refused and the endpoints behind them are unreachable: {ungranted:?}"
    );
}

/// P-153: the priority ceiling applied to granting a role but NOT to revoking
/// one, so the asymmetry ran in the dangerous direction — a platform_admin
/// could not GRANT super_admin, but could STRIP one, removing the only role
/// that outranks them from the person holding it. Being unable to promote
/// yourself is worth little if you can demote everyone above you.
///
/// `revoke_role_from_user` checked `admin.users.manage` and nothing else. The
/// admin UI hides revoke buttons for roles that outrank the actor, which is a
/// guard rail, not a boundary: this test calls the endpoint directly, which is
/// what an attacker holding a legitimately-granted platform_admin token would
/// do.
#[tokio::test]
async fn test_revoke_is_subject_to_the_same_priority_ceiling_as_assign() {
    let app = TestApp::new().await;

    let (attacker_id, attacker_token) = register_user(&app, "revoke_ceiling_attacker").await;
    grant_role(&app, &attacker_id, "platform_admin").await;

    // A super_admin the attacker must not be able to demote.
    let victim_id = create_test_user(&app, "revoke_ceiling_victim").await;
    grant_role(&app, &victim_id, "super_admin").await;

    let super_admin_role = role_id_by_name(&app, "super_admin").await;
    let response = app
        .delete_with_token(
            &format!("/v1/admin/users/{victim_id}/roles/{super_admin_role}"),
            &attacker_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Same tier is refused too — mirroring assign, where 900 >= 900 is blocked.
    let peer_id = create_test_user(&app, "revoke_ceiling_peer").await;
    grant_role(&app, &peer_id, "platform_admin").await;
    let platform_admin_role = role_id_by_name(&app, "platform_admin").await;
    let response = app
        .delete_with_token(
            &format!("/v1/admin/users/{peer_id}/roles/{platform_admin_role}"),
            &attacker_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // The refusals above must not be an artefact of revoke being broken for
    // everyone: a role strictly below the attacker still revokes.
    let subordinate_id = create_test_user(&app, "revoke_ceiling_subordinate").await;
    grant_role(&app, &subordinate_id, "moderator").await;
    let moderator_role = role_id_by_name(&app, "moderator").await;
    let response = app
        .delete_with_token(
            &format!("/v1/admin/users/{subordinate_id}/roles/{moderator_role}"),
            &attacker_token,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // And the victim really did keep the role — a 403 that still revoked would
    // pass every assertion above.
    let still_super: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
         WHERE ur.user_id = $1 AND r.name = 'super_admin' AND ur.revoked_at IS NULL",
    )
    .bind(victim_id.parse::<Uuid>().expect("victim id is a uuid"))
    .fetch_one(app.pool())
    .await
    .expect("query user_roles");
    assert_eq!(
        still_super, 1,
        "the super_admin assignment must survive the refused revoke"
    );
}

/// A super_admin is exempt from the ceiling on revoke, exactly as on assign.
#[tokio::test]
async fn test_super_admin_can_revoke_super_admin() {
    let app = TestApp::new().await;

    let (admin_id, admin_token) = register_user(&app, "revoke_super_actor").await;
    grant_role(&app, &admin_id, "super_admin").await;

    let target_id = create_test_user(&app, "revoke_super_target").await;
    grant_role(&app, &target_id, "super_admin").await;

    let super_admin_role = role_id_by_name(&app, "super_admin").await;
    let response = app
        .delete_with_token(
            &format!("/v1/admin/users/{target_id}/roles/{super_admin_role}"),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

/// Revoking a role id that does not exist must stay a 404, not become a 403.
/// The ceiling check needs the role's priority, so it has to resolve the role
/// first — if that resolution 403'd instead of 404ing, the endpoint would leak
/// which role ids exist to any caller who can read the difference.
#[tokio::test]
async fn test_revoking_an_unknown_role_is_still_not_found() {
    let app = TestApp::new().await;

    let (admin_id, admin_token) = register_user(&app, "revoke_unknown_actor").await;
    grant_role(&app, &admin_id, "platform_admin").await;
    let target_id = create_test_user(&app, "revoke_unknown_target").await;

    let response = app
        .delete_with_token(
            &format!("/v1/admin/users/{target_id}/roles/{}", Uuid::now_v7()),
            &admin_token,
        )
        .await;
    response.assert_status(StatusCode::NOT_FOUND);
}

/// P-151 — a permission used as a bare string literal is invisible to the
/// registry, and therefore invisible to
/// `test_every_declared_permission_is_seeded_and_granted`.
///
/// `admin.games.manage` was exactly that: seeded by migration 0019, gated on at
/// six sites in `handlers/games.rs`, and present in no `ALL` array. The P-140
/// guard would not have caught P-139 had it happened to this permission
/// instead, because a registry is only a safety net for what is in it.
///
/// This closes the other direction: every permission string in the code must
/// come from a declared constant. Together the two tests mean a permission
/// cannot exist in code without also existing in the registry, in a migration,
/// and on at least one role.
///
/// Deliberately scans the SOURCE rather than the binary: a literal that is
/// equal to a registered permission's value still fails, because the defect is
/// the missing indirection, not a wrong string. Copying a correct value is what
/// produced all ten sites.
#[tokio::test]
async fn test_no_permission_is_used_as_a_bare_literal() {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Anything shaped like `"segment.segment.segment"` in the permission
    /// namespaces the product uses.
    fn looks_like_a_permission(literal: &str) -> bool {
        const NAMESPACES: &[&str] = &[
            "admin.", "team.", "league.", "tournament.", "match.", "service.",
        ];
        NAMESPACES.iter().any(|ns| literal.starts_with(ns))
            && literal.matches('.').count() >= 2
            && literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
    }

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    assert!(
        files.len() > 50,
        "only {} source files found under {} — the walk is broken and this test \
         would pass vacuously",
        files.len(),
        src.display()
    );

    let mut offenders = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else { continue };
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Documentation is allowed to name a permission — `dto/responses/role.rs`
            // legitimately carries `"team.roster.manage"` as a schema example and in
            // a doc comment. Skipping these is not a loophole: neither reaches a gate.
            if trimmed.starts_with("//") || trimmed.starts_with("#[schema(") {
                continue;
            }
            for literal in line.split('"').skip(1).step_by(2) {
                if looks_like_a_permission(literal) {
                    offenders.push(format!(
                        "{}:{}  {:?}",
                        file.strip_prefix(&src).unwrap_or(file).display(),
                        idx + 1,
                        literal
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "permission strings must come from `portal_core::permissions::*`, not \
         literals — a literal is absent from the registry, so nothing verifies it \
         is seeded or granted to any role (P-139/P-151):\n  {}",
        offenders.join("\n  ")
    );
}
