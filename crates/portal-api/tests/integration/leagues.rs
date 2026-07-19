//! League API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;

// ============================================================================
// CREATE LEAGUE TESTS
// ============================================================================

#[tokio::test]
async fn test_create_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league as the dev user
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Test League",
                "slug": "test-league",
                "description": "A test league for CS2",
                "access_type": "open"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Test League");
    assert_eq!(body["data"]["slug"], "test-league");
    assert_eq!(body["data"]["access_type"], "open");
    assert_eq!(body["data"]["status"], "active");
}

#[tokio::test]
async fn test_create_league_validation_error() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Try to create a league with invalid name (too short)
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "AB",  // Too short (min 3)
                "slug": "ab"   // Too short (min 3)
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_league_invalid_slug() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Slug with invalid characters
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Test League",
                "slug": "Test League!"  // Invalid characters
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_league_duplicate_slug() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create first league
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "First League",
                "slug": "unique-slug"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Try to create another league with the same slug
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Second League",
                "slug": "unique-slug"  // Duplicate
            }),
        )
        .await;

    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_create_league_requires_auth() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Try to create without auth
    let response = app
        .post_json_no_auth(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Test League",
                "slug": "test-league"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// GET LEAGUE TESTS
// ============================================================================

#[tokio::test]
async fn test_get_league_by_id() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Get Test League",
                "slug": "get-test-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Get the league by ID
    let response = app.get(&format!("/v1/leagues/{}", league_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Get Test League");
    assert_eq!(body["data"]["slug"], "get-test-league");
}

#[tokio::test]
async fn test_get_league_by_slug() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Slug Test League",
                "slug": "slug-test-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    // Get the league by slug
    let response = app.get("/v1/leagues/by-slug/slug-test-league").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Slug Test League");
}

#[tokio::test]
async fn test_get_league_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/leagues/00000000-0000-0000-0000-000000000099")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// LIST LEAGUES TESTS
// ============================================================================

#[tokio::test]
async fn test_list_leagues() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a couple of leagues
    app.post_json(
        "/v1/leagues",
        &json!({
            "game_id": game_id,
            "name": "List League 1",
            "slug": "list-league-1"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    app.post_json(
        "/v1/leagues",
        &json!({
            "game_id": game_id,
            "name": "List League 2",
            "slug": "list-league-2"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // List leagues (public endpoint)
    let response = app.get("/v1/leagues").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn test_list_leagues_by_game() {
    let app = TestApp::new().await;
    let cs2_game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league for CS2
    app.post_json(
        "/v1/leagues",
        &json!({
            "game_id": cs2_game_id,
            "name": "CS2 League",
            "slug": "cs2-league-filter"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // List leagues filtered by game
    let response = app
        .get(&format!("/v1/leagues?game_id={}", cs2_game_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let leagues = body["data"].as_array().unwrap();
    assert!(leagues.iter().all(|l| l["game_id"] == cs2_game_id));
}

// ============================================================================
// JOIN LEAGUE TESTS
// ============================================================================

#[tokio::test]
async fn test_join_open_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create user2 with their own token
    let user2 = UserBuilder::new()
        .username("joiner")
        .email("joiner@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an open league as dev user
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Open League",
                "slug": "open-league-join",
                "access_type": "open"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 joins the open league
    let response = app
        .post_with_token(&format!("/v1/leagues/{}/join", league_id), &token2)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["membership_type"], "member");
}

#[tokio::test]
async fn test_join_invite_only_league_fails() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create user2
    let user2 = UserBuilder::new()
        .username("blocked-joiner")
        .email("blocked@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an invite-only league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Invite Only League",
                "slug": "invite-only-join",
                "access_type": "invite_only"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 tries to join - should fail (league is invite-only)
    let response = app
        .post_with_token(&format!("/v1/leagues/{}/join", league_id), &token2)
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// MEMBER MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_list_members() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league (dev user becomes admin)
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Members Test League",
                "slug": "members-test"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // List members
    let response = app.get(&format!("/v1/leagues/{}/members", league_id)).await;
    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["membership_type"], "admin");
}

#[tokio::test]
async fn test_leave_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create user2
    let user2 = UserBuilder::new()
        .username("leaver")
        .email("leaver@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an open league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Leave Test League",
                "slug": "leave-test",
                "access_type": "open"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 joins
    app.post_with_token(&format!("/v1/leagues/{}/join", league_id), &token2)
        .await
        .assert_status(StatusCode::OK);

    // User2 leaves
    let response = app
        .post_with_token(&format!("/v1/leagues/{}/leave", league_id), &token2)
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

// ============================================================================
// UPDATE LEAGUE TESTS
// ============================================================================

#[tokio::test]
async fn test_update_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Update Test League",
                "slug": "update-test-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Update the league
    let response = app
        .patch_json(
            &format!("/v1/leagues/{}", league_id),
            &json!({
                "name": "Updated League Name",
                "description": "New description"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Updated League Name");
    assert_eq!(body["data"]["description"], "New description");
}

#[tokio::test]
async fn test_update_league_requires_permission() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create user2 (not an admin)
    let user2 = UserBuilder::new()
        .username("nonadmin")
        .email("nonadmin@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create a league as dev user
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Permission Test League",
                "slug": "permission-test-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 tries to update - should fail
    let response = app
        .patch_json_with_token(
            &format!("/v1/leagues/{}", league_id),
            &json!({
                "name": "Unauthorized Update"
            }),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// APPLICATION TESTS
// ============================================================================

#[tokio::test]
async fn test_apply_to_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create user2
    let user2 = UserBuilder::new()
        .username("applicant")
        .email("applicant@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an application-based league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Application League",
                "slug": "application-league",
                "access_type": "application"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 applies
    let response = app
        .post_json_with_token(
            &format!("/v1/leagues/{}/apply", league_id),
            &json!({
                "message": "I would like to join!"
            }),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["invitation_type"], "application");
    assert_eq!(body["data"]["status"], "pending");
}

#[tokio::test]
async fn test_approve_application() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (applicant)
    let user2 = UserBuilder::new()
        .username("approved-applicant")
        .email("approved@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an application-based league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Approve Test League",
                "slug": "approve-test-league",
                "access_type": "application"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 applies
    let apply_response = app
        .post_json_with_token(
            &format!("/v1/leagues/{}/apply", league_id),
            &json!({ "message": "Please accept me!" }),
            &token2,
        )
        .await;
    apply_response.assert_status(StatusCode::CREATED);

    let application: serde_json::Value = apply_response.json();
    let application_id = application["data"]["id"].as_str().unwrap();

    // Admin approves the application
    let response = app
        .post_auth(&format!(
            "/v1/leagues/{}/applications/{}/approve",
            league_id, application_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["membership_type"], "member");
}

#[tokio::test]
async fn test_reject_application() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (applicant)
    let user2 = UserBuilder::new()
        .username("rejected-applicant")
        .email("rejected@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an application-based league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Reject Test League",
                "slug": "reject-test-league",
                "access_type": "application"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 applies
    let apply_response = app
        .post_json_with_token(
            &format!("/v1/leagues/{}/apply", league_id),
            &json!({ "message": "Please accept me!" }),
            &token2,
        )
        .await;
    apply_response.assert_status(StatusCode::CREATED);

    let application: serde_json::Value = apply_response.json();
    let application_id = application["data"]["id"].as_str().unwrap();

    // Admin rejects the application
    let response = app
        .post_auth(&format!(
            "/v1/leagues/{}/applications/{}/reject",
            league_id, application_id
        ))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

// ============================================================================
// INVITATION TESTS
// ============================================================================

#[tokio::test]
async fn test_invite_user_to_league() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (invitee)
    let user2 = UserBuilder::new()
        .username("invitee")
        .email("invitee@example.com")
        .build_persisted(app.pool())
        .await;

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Invite Test League",
                "slug": "invite-test-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Invite user2
    let response = app
        .post_json(
            &format!("/v1/leagues/{}/invitations", league_id),
            &json!({
                "user_id": user2.id.to_string(),
                "message": "Join our league!"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["invitation_type"], "invite");
    assert_eq!(body["data"]["status"], "pending");
}

#[tokio::test]
async fn test_accept_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (invitee)
    let user2 = UserBuilder::new()
        .username("accept-invitee")
        .email("accept@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Accept Invite League",
                "slug": "accept-invite-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Invite user2
    let invite_response = app
        .post_json(
            &format!("/v1/leagues/{}/invitations", league_id),
            &json!({
                "user_id": user2.id.to_string()
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    // User2 accepts the invitation
    let response = app
        .post_with_token(
            &format!("/v1/league-invitations/{}/accept", invitation_id),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["membership_type"], "member");
}

#[tokio::test]
async fn test_decline_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (invitee)
    let user2 = UserBuilder::new()
        .username("decline-invitee")
        .email("decline@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Decline Invite League",
                "slug": "decline-invite-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Invite user2
    let invite_response = app
        .post_json(
            &format!("/v1/leagues/{}/invitations", league_id),
            &json!({
                "user_id": user2.id.to_string()
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    // User2 declines the invitation
    let response = app
        .post_with_token(
            &format!("/v1/league-invitations/{}/decline", invitation_id),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

// ============================================================================
// USER-CENTRIC TESTS
// ============================================================================

#[tokio::test]
async fn test_get_my_leagues() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create a league (dev user becomes admin)
    app.post_json(
        "/v1/leagues",
        &json!({
            "game_id": game_id,
            "name": "My League",
            "slug": "my-league"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get my leagues
    let response = app.get_auth("/v1/users/me/leagues").await;
    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert!(body.iter().any(|l| l["league_name"] == "My League"));
}

#[tokio::test]
async fn test_get_my_league_invitations() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2 (invitee)
    let user2 = UserBuilder::new()
        .username("my-invitations-user")
        .email("myinvitations@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create a league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Invitation Check League",
                "slug": "invitation-check-league"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // Invite user2
    app.post_json(
        &format!("/v1/leagues/{}/invitations", league_id),
        &json!({
            "user_id": user2.id.to_string()
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // User2 gets their invitations
    let response = app
        .get_with_token("/v1/users/me/league-invitations", &token2)
        .await;
    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert!(!body.is_empty());
    assert_eq!(body[0]["invitation_type"], "invite");
}

// ============================================================================
// MEMBER ROLE MANAGEMENT TESTS
// ============================================================================

#[tokio::test]
async fn test_update_member_role() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2
    let user2 = UserBuilder::new()
        .username("role-update-user")
        .email("roleupdate@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an open league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Role Update League",
                "slug": "role-update-league",
                "access_type": "open"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 joins
    app.post_with_token(&format!("/v1/leagues/{}/join", league_id), &token2)
        .await
        .assert_status(StatusCode::OK);

    // Update user2's role to moderator
    let response = app
        .patch_json(
            &format!("/v1/leagues/{}/members/{}", league_id, user2.id),
            &json!({
                "membership_type": "moderator"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["membership_type"], "moderator");
}

#[tokio::test]
async fn test_remove_member() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Grant league admin permission to dev user
    grant_league_admin_permission(&app).await;

    // Create user2
    let user2 = UserBuilder::new()
        .username("remove-user")
        .email("removeuser@example.com")
        .build_persisted(app.pool())
        .await;

    let token2 = create_token_for_user(&app, user2.id).await;

    // Create an open league
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Remove Member League",
                "slug": "remove-member-league",
                "access_type": "open"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = create_response.json();
    let league_id = created["data"]["id"].as_str().unwrap();

    // User2 joins
    app.post_with_token(&format!("/v1/leagues/{}/join", league_id), &token2)
        .await
        .assert_status(StatusCode::OK);

    // Admin removes user2
    let response = app
        .delete_auth(&format!("/v1/leagues/{}/members/{}", league_id, user2.id))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify user2 is no longer a member
    let members_response = app.get(&format!("/v1/leagues/{}/members", league_id)).await;
    let members: Vec<serde_json::Value> = members_response.json();
    assert!(!members.iter().any(|m| m["user_id"] == user2.id.to_string()));
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper to grant league admin permission to dev user.
async fn grant_league_admin_permission(app: &TestApp) {
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    // Get or create league_admin role
    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'league_admin'")
        .fetch_optional(app.pool())
        .await
        .expect("Query should succeed");

    let role_id: uuid::Uuid = if let Some(row) = role_row {
        row.get("id")
    } else {
        // Create the role if it doesn't exist
        let row = sqlx::query(
            "INSERT INTO roles (id, name, description, is_global) VALUES (gen_random_uuid(), 'league_admin', 'League administrator', false) RETURNING id"
        )
        .fetch_one(app.pool())
        .await
        .expect("Failed to create role");
        row.get("id")
    };

    // The dev user should already have scoped permissions from creating the league
    // But for tests that need explicit permission, we can assign the global role
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(dev_user_id)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("Failed to assign role");
}

/// Create a JWT token for a user.
/// The user_id and player_id are assumed to be the same (as per UserBuilder behavior).
async fn create_token_for_user(_app: &TestApp, user_id: uuid::Uuid) -> String {
    use portal_domain::generate_access_token;

    // User and player have the same ID per UserBuilder
    generate_access_token(user_id, user_id, "testuser", "test-jwt-secret")
        .expect("Failed to create token")
}
