//! League team API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;

/// Helper to get a game's UUID by slug.
async fn get_game_uuid(app: &TestApp, slug: &str) -> String {
    let row = sqlx::query("SELECT id FROM games WHERE slug = $1")
        .bind(slug)
        .fetch_one(app.pool())
        .await
        .expect("Game should exist");
    let id: uuid::Uuid = row.get("id");
    id.to_string()
}

/// Helper to create a league for testing.
async fn create_test_league(app: &TestApp, game_id: &str, slug: &str) -> serde_json::Value {
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": format!("Test League {}", slug),
                "slug": slug,
                "access_type": "open"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}

/// Helper to create a season for testing.
/// Creates a season in "registration" status so teams can be created.
async fn create_test_season(app: &TestApp, league_id: &str, slug: &str) -> serde_json::Value {
    // First create the season (starts in draft status)
    let response = app
        .post_json(
            "/v1/league-seasons",
            &json!({
                "league_id": league_id,
                "name": format!("Season {}", slug),
                "slug": slug,
                "team_size_min": 5,
                "team_size_max": 7,
                "max_substitutes": 2
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let season: serde_json::Value = response.json();
    let season_id = season["data"]["id"].as_str().unwrap();

    // Update to registration status so teams can be created
    let update_response = app
        .patch_json(
            &format!("/v1/league-seasons/{}", season_id),
            &json!({
                "status": "registration"
            }),
        )
        .await;
    update_response.assert_status(StatusCode::OK);
    update_response.json()
}

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
        let row = sqlx::query(
            "INSERT INTO roles (id, name, description, is_global) VALUES (gen_random_uuid(), 'league_admin', 'League administrator', false) RETURNING id"
        )
        .fetch_one(app.pool())
        .await
        .expect("Failed to create role");
        row.get("id")
    };

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(dev_user_id)
    .bind(role_id)
    .execute(app.pool())
    .await
    .expect("Failed to assign role");
}

/// Create a JWT token for a user.
/// The user_id and player_id are assumed to be the same (as per UserBuilder behavior).
fn create_token_for_user(user_id: uuid::Uuid) -> String {
    use portal_domain::generate_access_token;

    // User and player have the same ID per UserBuilder
    generate_access_token(user_id, user_id, "testuser", "test-jwt-secret")
        .expect("Failed to create token")
}

/// Helper to create a team and return team info.
/// Returns (team_id, team_season_id).
async fn create_test_team(
    app: &TestApp,
    season_id: &str,
    name: &str,
    tag: &str,
) -> (String, String) {
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{}/teams", season_id),
            &json!({
                "name": name,
                "tag": tag
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let team_id = body["data"]["team"]["id"].as_str().unwrap().to_string();
    let team_season_id = body["data"]["team_season"]["id"].as_str().unwrap().to_string();
    (team_id, team_season_id)
}

// ============================================================================
// SEASON TESTS
// ============================================================================

#[tokio::test]
async fn test_create_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;

    // Grant league admin permission
    grant_league_admin_permission(&app).await;

    // Create a league
    let league = create_test_league(&app, &game_id, "season-test-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();

    // Create a season (use season-2 since the league trigger auto-creates season-1)
    let response = app
        .post_json(
            "/v1/league-seasons",
            &json!({
                "league_id": league_id,
                "name": "Season 2",
                "slug": "season-2",
                "description": "Second season",
                "team_size_min": 5,
                "team_size_max": 7,
                "max_substitutes": 2,
                "max_teams": 16
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Season 2");
    assert_eq!(body["data"]["slug"], "season-2");
    assert_eq!(body["data"]["team_size_min"], 5);
    assert_eq!(body["data"]["team_size_max"], 7);
    assert_eq!(body["data"]["max_substitutes"], 2);
    assert_eq!(body["data"]["max_teams"], 16);
    assert_eq!(body["data"]["status"], "draft");
    assert_eq!(body["data"]["roster_lock_status"], "open");
}

#[tokio::test]
async fn test_get_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-season-1").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app.get(&format!("/v1/league-seasons/{}", season_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["slug"], "get-season-1");
}

#[tokio::test]
async fn test_list_seasons() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "list-seasons-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();

    // Create multiple seasons
    create_test_season(&app, league_id, "list-season-1").await;
    create_test_season(&app, league_id, "list-season-2").await;

    let response = app
        .get(&format!("/v1/league-seasons?league_id={}", league_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn test_update_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "update-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "update-season-1").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .patch_json(
            &format!("/v1/league-seasons/{}", season_id),
            &json!({
                "name": "Updated Season Name",
                "status": "registration"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Updated Season Name");
    assert_eq!(body["data"]["status"], "registration");
}

// ============================================================================
// TEAM TESTS
// ============================================================================

#[tokio::test]
async fn test_create_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "create-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "create-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{}/teams", season_id),
            &json!({
                "name": "Test Team",
                "tag": "TST",
                "description": "A test team",
                "primary_color": "#FF0000",
                "secondary_color": "#0000FF"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    // New response structure has team and team_season
    assert_eq!(body["data"]["team"]["name"], "Test Team");
    assert_eq!(body["data"]["team"]["tag"], "TST");
    assert_eq!(body["data"]["team"]["status"], "active");
    assert_eq!(body["data"]["team"]["primary_color"], "#FF0000");
    // Team season status starts as "forming"
    assert_eq!(body["data"]["team_season"]["status"], "forming");
}

#[tokio::test]
async fn test_get_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, _team_season_id) = create_test_team(&app, season_id, "Get Test Team", "GTT").await;

    // Get the team (persistent identity)
    let response = app.get(&format!("/v1/league-teams/{}", team_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Get Test Team");
    assert_eq!(body["data"]["tag"], "GTT");
}

#[tokio::test]
async fn test_list_teams_in_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "list-teams-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "list-teams-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create first team
    create_test_team(&app, season_id, "Team Alpha", "ALP").await;

    // Need a different user for the second team (one team per player per season)
    let user2 = UserBuilder::new()
        .username("team2creator")
        .email("team2@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    app.post_json_with_token(
        &format!("/v1/league-seasons/{}/teams", season_id),
        &json!({
            "name": "Team Beta",
            "tag": "BET"
        }),
        &token2,
    )
    .await
    .assert_status(StatusCode::CREATED);

    // List teams
    let response = app
        .get(&format!("/v1/league-seasons/{}/teams", season_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_update_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "update-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "update-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (team_id, _team_season_id) = create_test_team(&app, season_id, "Original Name", "ORG").await;

    // Update the team (as owner)
    let response = app
        .patch_json(
            &format!("/v1/league-teams/{}", team_id),
            &json!({
                "name": "Updated Name",
                "description": "New description"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Updated Name");
    assert_eq!(body["data"]["description"], "New description");
}

// ============================================================================
// TEAM SEASON TESTS
// ============================================================================

#[tokio::test]
async fn test_get_team_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-team-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-team-season-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Team Season Test", "TST").await;

    // Get the team season (seasonal participation)
    let response = app.get(&format!("/v1/league-team-seasons/{}", team_season_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "forming");
}

// ============================================================================
// TEAM MEMBER TESTS
// ============================================================================

#[tokio::test]
async fn test_get_team_members() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "members-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "members-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Members Test Team", "MTT").await;

    // Get team members via team_season_id (should have captain/owner)
    let response = app.get(&format!("/v1/league-team-seasons/{}/members", team_season_id)).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let members = body["data"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["role"], "captain");
}

#[tokio::test]
async fn test_leave_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "leave-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "leave-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Leave Test Team", "LTT").await;

    // Create a second user to join the team
    let user2 = UserBuilder::new()
        .username("leaver")
        .email("leaver@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    // Get user2's player ID
    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain invites user2 via team_season
    app.post_json(
        &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
        &json!({
            "player_id": player2_id.to_string(),
            "role": "player"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get the invitation ID (captain gets team invitations)
    let invitations_response = app
        .get_auth(&format!("/v1/league-team-seasons/{}/invitations", team_season_id))
        .await;
    invitations_response.assert_status(StatusCode::OK);
    let invitations: serde_json::Value = invitations_response.json();
    let invitation_id = invitations["data"][0]["id"]
        .as_str()
        .expect("Should have at least one invitation");

    // User2 accepts the invitation
    app.post_with_token(
        &format!("/v1/league-team-invitations/{}/accept", invitation_id),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Verify user2 is now a member
    let members_response = app.get(&format!("/v1/league-team-seasons/{}/members", team_season_id)).await;
    let members: serde_json::Value = members_response.json();
    assert_eq!(members["data"].as_array().unwrap().len(), 2);

    // User2 leaves the team via team_season_id
    let response = app
        .post_with_token(&format!("/v1/league-team-seasons/{}/leave", team_season_id), &token2)
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify user2 is no longer an active member
    let members_response = app.get(&format!("/v1/league-team-seasons/{}/members", team_season_id)).await;
    let members: serde_json::Value = members_response.json();
    let active_members: Vec<_> = members["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["status"] == "active")
        .collect();
    assert_eq!(active_members.len(), 1);
}

// ============================================================================
// INVITATION TESTS
// ============================================================================

#[tokio::test]
async fn test_invite_player_to_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Invite Test Team", "ITT").await;

    // Create a player to invite
    let user2 = UserBuilder::new()
        .username("invitee")
        .email("invitee@example.com")
        .build_persisted(app.pool())
        .await;

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain invites the player via team_season
    let response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player",
                "message": "Join our team!"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["invitation_type"], "invite");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["role"], "player");
}

#[tokio::test]
async fn test_accept_team_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "accept-invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "accept-invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Accept Invite Team", "AIT").await;

    // Create invitee
    let user2 = UserBuilder::new()
        .username("accept-invitee")
        .email("accept-invitee@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain invites
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player"
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    // User2 accepts
    let response = app
        .post_with_token(
            &format!("/v1/league-team-invitations/{}/accept", invitation_id),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["role"], "player");
    assert_eq!(body["data"]["status"], "active");
}

#[tokio::test]
async fn test_apply_to_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "apply-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "apply-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Apply Test Team", "ATT").await;

    // Create applicant
    let user2 = UserBuilder::new()
        .username("applicant")
        .email("applicant@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    // User2 applies to the team via team_season
    let response = app
        .post_json_with_token(
            &format!("/v1/league-team-seasons/{}/apply", team_season_id),
            &json!({
                "role": "player",
                "message": "I want to join!"
            }),
            &token2,
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["invitation_type"], "request");
    assert_eq!(body["data"]["status"], "pending");
}

#[tokio::test]
async fn test_decline_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "decline-invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "decline-invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Decline Invite Team", "DIT").await;

    let user2 = UserBuilder::new()
        .username("decliner")
        .email("decliner@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain invites via team_season
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string()
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    // User2 declines
    let response = app
        .post_json_with_token(
            &format!("/v1/league-team-invitations/{}/decline", invitation_id),
            &json!({}),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_get_my_team_invitations() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "my-invitations-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "my-invitations-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "My Invitations Team", "MIT").await;

    let user2 = UserBuilder::new()
        .username("my-invitations-user")
        .email("myinvitations@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain invites user2 via team_season
    app.post_json(
        &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
        &json!({
            "player_id": player2_id.to_string()
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // User2 gets their invitations
    let response = app
        .get_with_token("/v1/league-team-invitations/me", &token2)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let invitations = body["data"].as_array().unwrap();
    assert!(!invitations.is_empty());
    assert_eq!(invitations[0]["invitation_type"], "invite");
}

// ============================================================================
// PLAYER LEAGUE TEAMS TESTS
// ============================================================================

#[tokio::test]
async fn test_get_my_league_teams() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "my-teams-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "my-teams-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team (dev user becomes owner/captain)
    create_test_team(&app, season_id, "My Teams Test", "MTT").await;

    // Get my league teams via /players/me/league-teams
    let response = app.get_auth("/v1/players/me/league-teams").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let teams = body["data"].as_array().unwrap();
    assert!(!teams.is_empty());
    assert!(teams.iter().any(|t| t["team_name"] == "My Teams Test"));
}

// ============================================================================
// OWNERSHIP TESTS
// ============================================================================

#[tokio::test]
async fn test_transfer_ownership() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "transfer-ownership-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "transfer-ownership-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, team_season_id) = create_test_team(&app, season_id, "Transfer Owner Team", "TOT").await;

    // Create a second user to transfer ownership to
    let user2 = UserBuilder::new()
        .username("new-owner")
        .email("newowner@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // First, invite and accept user2 to the team
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player"
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    app.post_with_token(
        &format!("/v1/league-team-invitations/{}/accept", invitation_id),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Transfer ownership
    let response = app
        .post_json(
            &format!("/v1/league-teams/{}/transfer-ownership", team_id),
            &json!({
                "new_owner_player_id": player2_id.to_string()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify the team has a new owner
    let team_response = app.get(&format!("/v1/league-teams/{}", team_id)).await;
    let team: serde_json::Value = team_response.json();
    assert_eq!(team["data"]["owner_player_id"], player2_id.to_string());
}

#[tokio::test]
async fn test_disband_team() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "disband-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "disband-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, _team_season_id) = create_test_team(&app, season_id, "Disband Test Team", "DIS").await;

    // Disband the team (DELETE instead of POST /disband)
    let response = app
        .delete_auth(&format!("/v1/league-teams/{}", team_id))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify team is now disbanded
    let get_response = app.get(&format!("/v1/league-teams/{}", team_id)).await;
    let body: serde_json::Value = get_response.json();
    assert_eq!(body["data"]["status"], "disbanded");
}

// ============================================================================
// CAPTAIN PROMOTION/DEMOTION TESTS
// ============================================================================

#[tokio::test]
async fn test_promote_to_captain() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "promote-captain-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "promote-captain-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Promote Captain Team", "PCT").await;

    // Create and add a second user to the team
    let user2 = UserBuilder::new()
        .username("to-promote")
        .email("topromote@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Invite and accept
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player"
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    app.post_with_token(
        &format!("/v1/league-team-invitations/{}/accept", invitation_id),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Promote to captain
    let response = app
        .post_auth(&format!("/v1/league-team-seasons/{}/members/{}/promote", team_season_id, player2_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["role"], "captain");
}

#[tokio::test]
async fn test_demote_from_captain() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "demote-captain-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "demote-captain-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Demote Captain Team", "DCT").await;

    // Create and add a second user as captain
    let user2 = UserBuilder::new()
        .username("to-demote")
        .email("todemote@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Invite as player and accept
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{}/invitations", team_season_id),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player"
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);

    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    app.post_with_token(
        &format!("/v1/league-team-invitations/{}/accept", invitation_id),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Promote to captain first
    app.post_auth(&format!("/v1/league-team-seasons/{}/members/{}/promote", team_season_id, player2_id))
        .await
        .assert_status(StatusCode::OK);

    // Demote back to player
    let response = app
        .post_auth(&format!("/v1/league-team-seasons/{}/members/{}/demote", team_season_id, player2_id))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["role"], "player");
}

// ============================================================================
// VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_create_team_requires_auth() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "auth-test-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "auth-test-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .post_json_no_auth(
            &format!("/v1/league-seasons/{}/teams", season_id),
            &json!({
                "name": "No Auth Team",
                "tag": "NAT"
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_create_team_validation_error() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "validation-test-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "validation-test-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Tag too short
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{}/teams", season_id),
            &json!({
                "name": "Valid Name",
                "tag": "X"  // Too short (min 2)
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_cannot_join_two_teams_same_season() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "two-teams-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "two-teams-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create first team (dev user becomes owner/captain)
    create_test_team(&app, season_id, "First Team", "FT1").await;

    // Try to create second team with same user - should fail
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{}/teams", season_id),
            &json!({
                "name": "Second Team",
                "tag": "ST2"
            }),
        )
        .await;

    response.assert_status(StatusCode::CONFLICT);
}
