//! League API integration tests.

use crate::common::TestApp;
use crate::tournaments::create_test_player;
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
    let response = app.get(&format!("/v1/leagues/{league_id}")).await;
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
    let response = app.get(&format!("/v1/leagues?game_id={cs2_game_id}")).await;
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

    let token2 = create_token_for_user(&app, user2.id);

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
        .post_with_token(&format!("/v1/leagues/{league_id}/join"), &token2)
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

    let token2 = create_token_for_user(&app, user2.id);

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

    // User2 tries to join - should fail (league is invite-only).
    // P-46: this is 403, not 400 — the request is well-formed, the caller is
    // simply not permitted to enter without an invitation. Previously leagues
    // returned 400 while tournaments returned 403 for the same conceptual
    // refusal; the two are now aligned on 403 (spec change, not a weakening).
    let response = app
        .post_with_token(&format!("/v1/leagues/{league_id}/join"), &token2)
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
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

    // List members. Authenticated: P-37 closed this endpoint to anonymous callers,
    // which this test was incidentally relying on.
    let response = app
        .get_auth(&format!("/v1/leagues/{league_id}/members"))
        .await;
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

    let token2 = create_token_for_user(&app, user2.id);

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
    app.post_with_token(&format!("/v1/leagues/{league_id}/join"), &token2)
        .await
        .assert_status(StatusCode::OK);

    // User2 leaves
    let response = app
        .post_with_token(&format!("/v1/leagues/{league_id}/leave"), &token2)
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
            &format!("/v1/leagues/{league_id}"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}/apply"),
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

/// P-48: a user's own pending APPLICATION must be visible to them via
/// `GET /v1/users/me/league-invitations`. The repository's `list_pending_for_user`
/// previously filtered `invitation_type = 'invite'`, so an applicant's own
/// application was invisible — the frontend's `myApplications` was permanently
/// empty and the "application pending" UI branch was dead. The application and
/// any admin-sent invites now both come back, tagged by `invitation_type`.
#[tokio::test]
async fn test_my_league_invitations_includes_own_application() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let user2 = UserBuilder::new()
        .username("self-applicant")
        .email("self-applicant@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(&app, user2.id);

    // Before applying, the applicant has no pending invitations.
    let before = app
        .get_with_token("/v1/users/me/league-invitations", &token2)
        .await;
    before.assert_status(StatusCode::OK);
    assert_eq!(
        before.json::<serde_json::Value>().as_array().unwrap().len(),
        0,
        "no pending invitations before applying"
    );

    // Create an application-based league and apply to it.
    let create_response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Self Application League",
                "slug": "self-application-league",
                "access_type": "application"
            }),
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);
    let league_id = create_response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let apply = app
        .post_json_with_token(
            &format!("/v1/leagues/{league_id}/apply"),
            &json!({ "message": "let me in" }),
            &token2,
        )
        .await;
    apply.assert_status(StatusCode::CREATED);
    let application_id = apply.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The applicant now sees their own pending application in the me-list.
    let after = app
        .get_with_token("/v1/users/me/league-invitations", &token2)
        .await;
    after.assert_status(StatusCode::OK);
    let list: serde_json::Value = after.json();
    let rows = list.as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "the applicant sees exactly their application"
    );
    assert_eq!(rows[0]["id"].as_str().unwrap(), application_id);
    assert_eq!(rows[0]["invitation_type"], "application");
    assert_eq!(rows[0]["league_id"].as_str().unwrap(), league_id);
    assert_eq!(rows[0]["status"], "pending");
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}/apply"),
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
            "/v1/leagues/{league_id}/applications/{application_id}/approve"
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}/apply"),
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
            "/v1/leagues/{league_id}/applications/{application_id}/reject"
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
            &format!("/v1/leagues/{league_id}/invitations"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}/invitations"),
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
            &format!("/v1/league-invitations/{invitation_id}/accept"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
            &format!("/v1/leagues/{league_id}/invitations"),
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
            &format!("/v1/league-invitations/{invitation_id}/decline"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
        &format!("/v1/leagues/{league_id}/invitations"),
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

    let token2 = create_token_for_user(&app, user2.id);

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
    app.post_with_token(&format!("/v1/leagues/{league_id}/join"), &token2)
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

    let token2 = create_token_for_user(&app, user2.id);

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
    app.post_with_token(&format!("/v1/leagues/{league_id}/join"), &token2)
        .await
        .assert_status(StatusCode::OK);

    // Admin removes user2
    let response = app
        .delete_auth(&format!("/v1/leagues/{}/members/{}", league_id, user2.id))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify user2 is no longer a member
    // Authenticated: see P-37.
    let members_response = app
        .get_auth(&format!("/v1/leagues/{league_id}/members"))
        .await;
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
fn create_token_for_user(_app: &TestApp, user_id: uuid::Uuid) -> String {
    use portal_domain::generate_access_token;

    // User and player have the same ID per UserBuilder
    generate_access_token(user_id, user_id, "testuser", "test-jwt-secret")
        .expect("Failed to create token")
}

/// P-37: the member list must not be readable anonymously, and must never
/// expose email addresses to callers who do not manage the league.
///
/// This endpoint previously took no auth extractor at all and always included
/// `email`, so anyone could enumerate member email addresses with a bare GET.
#[tokio::test]
async fn test_league_members_requires_auth_and_hides_email_from_unprivileged_callers() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "P37 Members League",
                "slug": "p37-members-league",
                "access_type": "open"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let league_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let url = format!("/v1/leagues/{league_id}/members");

    // 1. Anonymous is rejected outright.
    app.get(&url).await.assert_status(StatusCode::UNAUTHORIZED);

    // 2. An authenticated non-manager may see the roster but NOT the emails.
    let (outsider_user, outsider_player) = create_test_player(&app, "p37_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "p37_outsider",
        TEST_JWT_SECRET,
    );
    let response = app.get_with_token(&url, &outsider_token).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body.as_array()
            .expect("member list")
            .iter()
            .all(|m| m.get("email").is_none()),
        "email must be omitted for callers without league.members.manage, got: {body}"
    );

    // 3. The managing admin still gets it -- the admin members modal needs it.
    let response = app.get_auth(&url).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body.as_array()
            .expect("member list")
            .iter()
            .any(|m| m.get("email").is_some()),
        "a league manager must still receive member emails, got: {body}"
    );
}

// ============================================================================
// P-38 / P-39 / P-54 REGRESSION TESTS (league invitations & members paging)
// ============================================================================

/// P-38: every league-invitation response must carry `league_name`, so two
/// pending invitations are distinguishable on the invitations page. The DTO
/// previously exposed only `league_id`, so the UI rendered a hardcoded
/// "League Invitation" and accept/decline was a blind choice.
#[tokio::test]
async fn test_league_invitation_response_carries_league_name() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("named-invitee")
        .email("named-invitee@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(&app, user2.id);

    // Two leagues with distinct names, both inviting the same user.
    let mut league_ids = Vec::new();
    for (name, slug) in [
        ("Alpha Named League", "alpha-named-league"),
        ("Beta Named League", "beta-named-league"),
    ] {
        let create = app
            .post_json(
                "/v1/leagues",
                &json!({
                    "game_id": game_id,
                    "name": name,
                    "slug": slug,
                    "access_type": "invite_only"
                }),
            )
            .await;
        create.assert_status(StatusCode::CREATED);
        let league_id = create.json::<serde_json::Value>()["data"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        // The create-invitation response itself must carry the name.
        let invite = app
            .post_json(
                &format!("/v1/leagues/{league_id}/invitations"),
                &json!({ "user_id": user2.id.to_string() }),
            )
            .await;
        invite.assert_status(StatusCode::CREATED);
        assert_eq!(
            invite.json::<serde_json::Value>()["data"]["league_name"],
            name,
            "invitation response must name its league"
        );
        league_ids.push((league_id, name));
    }

    // The invitee's own list distinguishes the two by name.
    let response = app
        .get_with_token("/v1/users/me/league-invitations", &token2)
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let rows = body.as_array().expect("invitation list");
    assert_eq!(rows.len(), 2, "both invitations visible: {body}");
    for (league_id, name) in &league_ids {
        let row = rows
            .iter()
            .find(|r| r["league_id"] == league_id.as_str())
            .unwrap_or_else(|| panic!("invitation for {name} present: {body}"));
        assert_eq!(row["league_name"], *name, "row must carry the league name");
    }

    // The admin listing carries it too.
    let (league_id, name) = &league_ids[0];
    let admin_list = app
        .get_auth(&format!("/v1/leagues/{league_id}/invitations"))
        .await;
    admin_list.assert_status(StatusCode::OK);
    let admin_body: serde_json::Value = admin_list.json();
    assert_eq!(
        admin_body.as_array().expect("admin list")[0]["league_name"],
        *name
    );
}

/// P-39: answered (terminal) invitations must remain listable by an admin.
/// The listing previously called `get_pending_by_league_authorized`, so a
/// declined invitation vanished and no endpoint reported its terminal status
/// — an admin could not tell "they declined" from "never invited". The
/// default stays pending-only (backward compatible); `?status=` reaches the
/// rest.
#[tokio::test]
async fn test_admin_lists_answered_invitations_with_status_filter() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("status-decliner")
        .email("status-decliner@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(&app, user2.id);

    let create = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Status Filter League",
                "slug": "status-filter-league",
                "access_type": "invite_only"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);
    let league_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let invite = app
        .post_json(
            &format!("/v1/leagues/{league_id}/invitations"),
            &json!({ "user_id": user2.id.to_string() }),
        )
        .await;
    invite.assert_status(StatusCode::CREATED);
    let invitation_id = invite.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The invitee declines.
    app.post_with_token(
        &format!("/v1/league-invitations/{invitation_id}/decline"),
        &token2,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    // Default view stays pending-only (backward compatible): declined row gone.
    let pending = app
        .get_auth(&format!("/v1/leagues/{league_id}/invitations"))
        .await;
    pending.assert_status(StatusCode::OK);
    assert_eq!(
        pending
            .json::<serde_json::Value>()
            .as_array()
            .unwrap()
            .len(),
        0,
        "declined invitation must not appear in the pending-only default"
    );

    // ?status=all reaches the answered row, with its terminal status.
    let all = app
        .get_auth(&format!("/v1/leagues/{league_id}/invitations?status=all"))
        .await;
    all.assert_status(StatusCode::OK);
    let all_body: serde_json::Value = all.json();
    let rows = all_body.as_array().expect("invitation list");
    assert_eq!(rows.len(), 1, "answered invitation listable: {all_body}");
    assert_eq!(rows[0]["id"], invitation_id.as_str());
    assert_eq!(rows[0]["status"], "rejected", "terminal status reported");

    // ?status=rejected narrows to exactly that state...
    let rejected = app
        .get_auth(&format!(
            "/v1/leagues/{league_id}/invitations?status=rejected"
        ))
        .await;
    rejected.assert_status(StatusCode::OK);
    assert_eq!(
        rejected
            .json::<serde_json::Value>()
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // ...while ?status=accepted correctly finds nothing.
    let accepted = app
        .get_auth(&format!(
            "/v1/leagues/{league_id}/invitations?status=accepted"
        ))
        .await;
    accepted.assert_status(StatusCode::OK);
    assert_eq!(
        accepted
            .json::<serde_json::Value>()
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // Nonsense filter is a 400, not a silent empty list.
    app.get_auth(&format!("/v1/leagues/{league_id}/invitations?status=bogus"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

/// P-54: `GET /v1/leagues/{id}/members` always honoured `page`/`per_page`,
/// but the OpenAPI spec did not declare them, so generated clients typed the
/// query as `never` and a roster past the default 20 rows was unreachable by
/// construction. The spec must declare the params, and a member past row 20
/// must be reachable through them.
#[tokio::test]
#[allow(clippy::literal_string_with_formatting_args)] // "{league_id}" is an OpenAPI path template, not a format arg
async fn test_list_members_pagination_declared_and_reaches_past_default_page() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // 1. The spec declares the pagination params (this is the actual P-54
    // defect — the behavior below always worked).
    let spec = app.get("/api-docs/openapi.json").await;
    spec.assert_status(StatusCode::OK);
    let spec_body: serde_json::Value = spec.json();
    let params = &spec_body["paths"]["/v1/leagues/{league_id}/members"]["get"]["parameters"];
    let declared: Vec<&str> = params
        .as_array()
        .expect("members GET must declare parameters")
        .iter()
        .filter_map(|p| p["name"].as_str())
        .collect();
    assert!(
        declared.contains(&"page") && declared.contains(&"per_page"),
        "spec must declare page/per_page on the members listing, got {declared:?}"
    );

    // 2. A member past row 20 is reachable via the declared params.
    let create = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "Paging League",
                "slug": "paging-league",
                "access_type": "open"
            }),
        )
        .await;
    create.assert_status(StatusCode::CREATED);
    let league_id = create.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let league_uuid = uuid::Uuid::parse_str(&league_id).unwrap();

    // Seed 24 members directly (25 total with the creator) — past the
    // default page of 20.
    for i in 0..24 {
        let user = UserBuilder::new()
            .username(format!("pager-{i:02}"))
            .email(format!("pager-{i:02}@example.com"))
            .build_persisted(app.pool())
            .await;
        sqlx::query(
            "INSERT INTO league_members (league_id, user_id, membership_type) VALUES ($1, $2, 'member')",
        )
        .bind(league_uuid)
        .bind(user.id)
        .execute(app.pool())
        .await
        .expect("seed league member");
    }

    // Default request: capped at 20 — the last members are invisible.
    let default_page = app
        .get_auth(&format!("/v1/leagues/{league_id}/members"))
        .await;
    default_page.assert_status(StatusCode::OK);
    let default_rows = default_page.json::<serde_json::Value>();
    assert_eq!(
        default_rows.as_array().unwrap().len(),
        20,
        "default page size is 20"
    );

    // per_page=100 reaches all 25.
    let big_page = app
        .get_auth(&format!("/v1/leagues/{league_id}/members?per_page=100"))
        .await;
    big_page.assert_status(StatusCode::OK);
    let big_rows = big_page.json::<serde_json::Value>();
    assert_eq!(big_rows.as_array().unwrap().len(), 25);

    // page=2 with per_page=20 reaches the tail past the default window.
    let page2 = app
        .get_auth(&format!(
            "/v1/leagues/{league_id}/members?page=2&per_page=20"
        ))
        .await;
    page2.assert_status(StatusCode::OK);
    let page2_body = page2.json::<serde_json::Value>();
    let page2_rows = page2_body.as_array().unwrap();
    assert_eq!(page2_rows.len(), 5, "5 members past the first page");

    // A member on page 2 is one the default window could not show.
    let default_names: Vec<&str> = default_rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["username"].as_str())
        .collect();
    for row in page2_rows {
        let name = row["username"].as_str().unwrap();
        assert!(
            !default_names.contains(&name),
            "{name} must not also be on page 1"
        );
    }
}
