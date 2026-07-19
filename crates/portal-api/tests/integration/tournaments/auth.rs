use super::*;

#[tokio::test]
async fn test_create_tournament_unauthorized() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Try to create without auth
    let response = app
        .post_json_no_auth(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Unauthorized Test",
                "slug": "unauthorized-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_update_tournament_unauthorized() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament (with auth)
    let create_response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Update Unauth Test",
                "slug": "update-unauth-test",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 4,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap();

    // Try to update without auth
    let response = app
        .patch_json_no_auth(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({
                "name": "Hacked Name"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// SCOPED RBAC TESTS
// ============================================================================

fn tournament_body(game_id: &str, name: &str, slug: &str) -> serde_json::Value {
    json!({
        "game_id": game_id,
        "name": name,
        "slug": slug,
        "format": "single_elimination",
        "participant_type": "individual",
        "min_participants": 2,
        "max_participants": 16,
        "registration_type": "open",
        "scheduling_mode": "live",
        "default_match_format": "bo3"
    })
}

/// A registered user who neither created the tournament nor holds any
/// admin role must be denied on all guarded management endpoints.
#[tokio::test]
async fn test_non_creator_forbidden_on_tournament_management() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Dev user creates a draft tournament.
    let response = app
        .post_json(
            "/v1/tournaments",
            &tournament_body(&game_id, "RBAC Outsider Test", "rbac-outsider-test"),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // A different registered user with no roles at all.
    let (user_id, player_id) = create_test_player(&app, "rbac_outsider").await;
    let token = create_test_token(user_id, player_id, "rbac_outsider", TEST_JWT_SECRET);

    // Publish → 403
    let response = app
        .post_with_token(&format!("/v1/tournaments/{tournament_id}/publish"), &token)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Auto-seed → 403
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
            &json!({ "algorithm": "random" }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Set up a pending registration (as the creator) to probe approve.
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await;
    response.assert_status(StatusCode::OK);
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/open-registration"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let registration_id = super::register_player(&app, &tournament_id, "DevPlayer").await;

    // Approve registration → 403
    let response = app
        .post_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

/// The creator of a tournament — with no global roles — gets the
/// `tournament_admin` scoped role on creation and can therefore manage
/// their own tournament end-to-end.
#[tokio::test]
async fn test_creator_scoped_grant_allows_managing_own_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let (user_id, player_id) = create_test_player(&app, "rbac_creator").await;
    let token = create_test_token(user_id, player_id, "rbac_creator", TEST_JWT_SECRET);

    // Create as a plain user (no global roles).
    let response = app
        .post_json_with_token(
            "/v1/tournaments",
            &tournament_body(&game_id, "RBAC Creator Test", "rbac-creator-test"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish own tournament → 200
    let response = app
        .post_with_token(&format!("/v1/tournaments/{tournament_id}/publish"), &token)
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration → 200
    let response = app
        .post_with_token(
            &format!("/v1/tournaments/{tournament_id}/open-registration"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Register themselves, then approve their own tournament's registration.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Creator" }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let registration_id = body["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// A platform_admin who did not create the tournament can still manage it
/// via the `admin.tournaments.manage_any` override.
#[tokio::test]
async fn test_platform_admin_can_publish_others_tournament() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Dev user creates a draft tournament.
    let response = app
        .post_json(
            "/v1/tournaments",
            &tournament_body(
                &game_id,
                "RBAC Admin Override Test",
                "rbac-admin-override-test",
            ),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // A different user holding the global platform_admin role.
    let (user_id, player_id) = create_test_player(&app, "rbac_padmin").await;
    assign_role_to_user(app.pool(), user_id, "platform_admin").await;
    let token = create_test_token(user_id, player_id, "rbac_padmin", TEST_JWT_SECRET);

    let response = app
        .post_with_token(&format!("/v1/tournaments/{tournament_id}/publish"), &token)
        .await;
    response.assert_status(StatusCode::OK);
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_tournament_missing_required_fields() {
    let app = TestApp::new().await;

    // Missing game_id and other required fields
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "name": "Incomplete Tournament"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_tournament_invalid_participant_range() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // min_participants > max_participants
    // This is validated by a database constraint, so we get a 500 error
    // (In a production system, this would ideally be validated before hitting the DB)
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Invalid Range Tournament",
                "slug": "invalid-range",
                "format": "single_elimination",
                "participant_type": "team",
                "min_participants": 32,
                "max_participants": 8,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;

    // The database constraint catches this, resulting in an internal error
    // A proper validation layer would return 400 instead
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}
