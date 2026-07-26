//! League team API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use sqlx::Row;

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
            &format!("/v1/league-seasons/{season_id}"),
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
    assign_role_to_user(app.pool(), dev_user_id, "league_admin").await;
}

/// Create a JWT token for a user.
/// The user_id and player_id are assumed to be the same (as per UserBuilder behavior).
fn create_token_for_user(user_id: uuid::Uuid) -> String {
    create_test_token(user_id, user_id, "testuser", TEST_JWT_SECRET)
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
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({
                "name": name,
                "tag": tag
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    let team_id = body["data"]["team"]["id"].as_str().unwrap().to_string();
    let team_season_id = body["data"]["team_season"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (team_id, team_season_id)
}

// ============================================================================
// SEASON TESTS
// ============================================================================

#[tokio::test]
async fn test_create_season() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-season-1").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app.get(&format!("/v1/league-seasons/{season_id}")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["slug"], "get-season-1");
}

#[tokio::test]
async fn test_list_seasons() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "list-seasons-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();

    // Create multiple seasons
    create_test_season(&app, league_id, "list-season-1").await;
    create_test_season(&app, league_id, "list-season-2").await;

    let response = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn test_update_season() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "update-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "update-season-1").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .patch_json(
            &format!("/v1/league-seasons/{season_id}"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "create-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "create-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Get Test Team", "GTT").await;

    // Get the team (persistent identity)
    let response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Get Test Team");
    assert_eq!(body["data"]["tag"], "GTT");
}

#[tokio::test]
async fn test_list_teams_in_season() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
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
        &format!("/v1/league-seasons/{season_id}/teams"),
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
        .get(&format!("/v1/league-seasons/{season_id}/teams"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_update_team() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "update-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "update-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Original Name", "ORG").await;

    // Update the team (as owner)
    let response = app
        .patch_json(
            &format!("/v1/league-teams/{team_id}"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "get-team-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "get-team-season-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Team Season Test", "TST").await;

    // Get the team season (seasonal participation)
    let response = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}"))
        .await;
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "members-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "members-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Members Test Team", "MTT").await;

    // Get team members via team_season_id (should have captain/owner)
    let response = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let members = body["data"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["role"], "captain");
}

#[tokio::test]
async fn test_leave_team() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "leave-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "leave-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Leave Test Team", "LTT").await;

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
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
        &json!({
            "player_id": player2_id.to_string(),
            "role": "player"
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Get the invitation ID (captain gets team invitations)
    let invitations_response = app
        .get_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/invitations"
        ))
        .await;
    invitations_response.assert_status(StatusCode::OK);
    let invitations: serde_json::Value = invitations_response.json();
    let invitation_id = invitations["data"][0]["id"]
        .as_str()
        .expect("Should have at least one invitation");

    // User2 accepts the invitation
    app.post_with_token(
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Verify user2 is now a member
    let members_response = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    let members: serde_json::Value = members_response.json();
    assert_eq!(members["data"].as_array().unwrap().len(), 2);

    // User2 leaves the team via team_season_id
    let response = app
        .post_with_token(
            &format!("/v1/league-team-seasons/{team_season_id}/leave"),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify user2 is no longer an active member
    let members_response = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    let members: serde_json::Value = members_response.json();
    let active_members = members["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["status"] == "active")
        .count();
    assert_eq!(active_members, 1);
}

// ============================================================================
// INVITATION TESTS
// ============================================================================

#[tokio::test]
async fn test_invite_player_to_team() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Invite Test Team", "ITT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "accept-invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "accept-invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Accept Invite Team", "AIT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
            &format!("/v1/league-team-invitations/{invitation_id}/accept"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "apply-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "apply-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Apply Test Team", "ATT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/apply"),
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

/// P-49: the team-season invitations list mixes captain-sent invites and
/// player-sent join requests; each row carries `invitation_type` so the UI can
/// tell them apart (before this, TeamDetailPage rendered every row as a
/// "Pending Invitation" the captain could only cancel, so a join request was
/// mislabelled and the captain's only affordance rejected it). A captain
/// accepting a `request` via the shared accept endpoint seats the applicant on
/// the roster.
#[tokio::test]
async fn test_team_invitations_distinguish_request_from_invite_and_accept_request() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "reqinv-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "reqinv-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // The dev user creates the team and is therefore its captain.
    let (_team_id, team_season_id) = create_test_team(&app, season_id, "ReqInv Team", "RIT").await;

    // Applicant (sends a join request) and invitee (receives a captain invite).
    let applicant = UserBuilder::new()
        .username("reqinv-applicant")
        .email("reqinv-applicant@example.com")
        .build_persisted(app.pool())
        .await;
    let applicant_token = create_token_for_user(applicant.id);
    let applicant_player_id: uuid::Uuid = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(applicant.id)
        .fetch_one(app.pool())
        .await
        .unwrap()
        .get("id");

    let invitee = UserBuilder::new()
        .username("reqinv-invitee")
        .email("reqinv-invitee@example.com")
        .build_persisted(app.pool())
        .await;
    let invitee_player_id: uuid::Uuid = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(invitee.id)
        .fetch_one(app.pool())
        .await
        .unwrap()
        .get("id");

    // Applicant sends a join REQUEST.
    let apply = app
        .post_json_with_token(
            &format!("/v1/league-team-seasons/{team_season_id}/apply"),
            &json!({ "role": "player", "message": "let me in" }),
            &applicant_token,
        )
        .await;
    apply.assert_status(StatusCode::CREATED);
    let request_id = apply.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Captain sends an INVITE to the invitee.
    let invite = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({ "player_id": invitee_player_id.to_string() }),
        )
        .await;
    invite.assert_status(StatusCode::CREATED);

    // The captain's list returns both, distinguishable by `invitation_type`.
    let list = app
        .get_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/invitations"
        ))
        .await;
    list.assert_status(StatusCode::OK);
    let rows = list.json::<serde_json::Value>()["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 2, "both the request and the invite are listed");

    let request_row = rows
        .iter()
        .find(|r| r["invitation_type"] == "request")
        .expect("a request row must be present");
    let invite_row = rows
        .iter()
        .find(|r| r["invitation_type"] == "invite")
        .expect("an invite row must be present");
    assert_eq!(
        request_row["player_id"].as_str().unwrap(),
        applicant_player_id.to_string(),
        "the request belongs to the applicant"
    );
    assert_eq!(
        invite_row["player_id"].as_str().unwrap(),
        invitee_player_id.to_string(),
        "the invite belongs to the invitee"
    );

    // The captain accepts the REQUEST (default dev auth = captain) and the
    // applicant is seated on the roster.
    let accept = app
        .post_json(
            &format!("/v1/league-team-invitations/{request_id}/accept"),
            &json!({}),
        )
        .await;
    accept.assert_status(StatusCode::OK);

    let members = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    members.assert_status(StatusCode::OK);
    let members_body: serde_json::Value = members.json();
    let applicant_is_member = members_body["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["player_id"].as_str() == Some(applicant_player_id.to_string().as_str()));
    assert!(
        applicant_is_member,
        "the applicant becomes a team member after the captain accepts the request"
    );
}

#[tokio::test]
async fn test_decline_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "decline-invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "decline-invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Decline Invite Team", "DIT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
            &format!("/v1/league-team-invitations/{invitation_id}/decline"),
            &json!({}),
            &token2,
        )
        .await;
    response.assert_status(StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_get_my_team_invitations() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "my-invitations-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "my-invitations-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "My Invitations Team", "MIT").await;

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
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "transfer-ownership-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "transfer-ownership-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, team_season_id) =
        create_test_team(&app, season_id, "Transfer Owner Team", "TOT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Transfer ownership
    let response = app
        .post_json(
            &format!("/v1/league-teams/{team_id}/transfer-ownership"),
            &json!({
                "new_owner_player_id": player2_id.to_string()
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Verify the team has a new owner
    let team_response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    let team: serde_json::Value = team_response.json();
    assert_eq!(team["data"]["owner_player_id"], player2_id.to_string());
}

/// P-113: transferring ownership must move the AUTHORITY, not just the column.
///
/// `update_team`, `disband_team` and `register_team_for_season` all gate on
/// `require_team_settings_manage` → the team-scoped `team_captain` RBAC grant,
/// which is written when the team is created. `transfer_ownership` used to
/// update `league_teams.owner_player_id` and touch nothing else, so after a
/// transfer the new owner 403'd on every owner action while the OLD owner kept
/// the power to disband a team they no longer owned.
///
/// This asserts the consequences, deliberately — not the presence of a row in
/// `user_roles`. A test that only checked "the grant moved" would also pass
/// against a fix that granted without revoking, which leaves the more dangerous
/// half of the defect (the previous owner's retained disband power) intact.
///
/// Both accounts are real JWT users, never `dev-token`: the dev user short
/// circuits `is_dev_user` in `PermissionChecker` and every assertion below
/// would pass for the wrong reason.
#[tokio::test]
async fn test_transfer_ownership_moves_team_settings_authority() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "transfer-authority-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "transfer-authority-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // The outgoing owner creates the team, so the RBAC grant lands on them the
    // way it does in production.
    let old_owner = UserBuilder::new()
        .username("authority-old-owner")
        .email("authority-old-owner@example.com")
        .build_persisted(app.pool())
        .await;
    let old_token = create_token_for_user(old_owner.id);

    let create_response = app
        .post_json_with_token(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "Authority Transfer Team", "tag": "ATT" }),
            &old_token,
        )
        .await;
    create_response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = create_response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();
    let team_season_id = created["data"]["team_season"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The incoming owner joins the roster first (transfer targets a player, and
    // a captain-run invite is how they get there in the product).
    let new_owner = UserBuilder::new()
        .username("authority-new-owner")
        .email("authority-new-owner@example.com")
        .build_persisted(app.pool())
        .await;
    let new_token = create_token_for_user(new_owner.id);
    let new_owner_player_id = new_owner.id;

    let invite_response = app
        .post_json_with_token(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({ "player_id": new_owner_player_id.to_string(), "role": "player" }),
            &old_token,
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);
    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    app.post_with_token(
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &new_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // Baseline: before the transfer the authority is the old owner's alone.
    app.patch_json_with_token(
        &format!("/v1/league-teams/{team_id}"),
        &json!({ "description": "written by the incoming owner, too early" }),
        &new_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    app.patch_json_with_token(
        &format!("/v1/league-teams/{team_id}"),
        &json!({ "description": "written by the founding owner" }),
        &old_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // Act.
    app.post_json_with_token(
        &format!("/v1/league-teams/{team_id}/transfer-ownership"),
        &json!({ "new_owner_player_id": new_owner_player_id.to_string() }),
        &old_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // 1. The new owner can now edit the team they own.
    let new_owner_update = app
        .patch_json_with_token(
            &format!("/v1/league-teams/{team_id}"),
            &json!({ "description": "written by the new owner" }),
            &new_token,
        )
        .await;
    new_owner_update.assert_status(StatusCode::OK);
    let updated: serde_json::Value = new_owner_update.json();
    assert_eq!(updated["data"]["description"], "written by the new owner");

    // 2. The old owner cannot edit a team they no longer own.
    app.patch_json_with_token(
        &format!("/v1/league-teams/{team_id}"),
        &json!({ "description": "written by the former owner" }),
        &old_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    // 3. The dangerous half: the old owner cannot disband it either...
    app.delete_with_token(&format!("/v1/league-teams/{team_id}"), &old_token)
        .await
        .assert_status(StatusCode::FORBIDDEN);

    // ...and the team really is untouched, not merely reported as refused.
    let after_refusal = app.get(&format!("/v1/league-teams/{team_id}")).await;
    let after_refusal_body: serde_json::Value = after_refusal.json();
    assert_eq!(after_refusal_body["data"]["status"], "active");
    assert_eq!(
        after_refusal_body["data"]["description"], "written by the new owner",
        "the former owner's rejected edit must not have landed"
    );

    // 4. The new owner can disband, which is the whole point of owning it.
    app.delete_with_token(&format!("/v1/league-teams/{team_id}"), &new_token)
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let final_team = app.get(&format!("/v1/league-teams/{team_id}")).await;
    let final_body: serde_json::Value = final_team.json();
    assert_eq!(final_body["data"]["status"], "disbanded");
}

#[tokio::test]
async fn test_disband_team() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "disband-team-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "disband-team-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Create a team
    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Disband Test Team", "DIS").await;

    // Disband the team (DELETE instead of POST /disband)
    let response = app
        .delete_auth(&format!("/v1/league-teams/{team_id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // Verify team is now disbanded
    let get_response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    let body: serde_json::Value = get_response.json();
    assert_eq!(body["data"]["status"], "disbanded");
}

/// P-126 — a disbanded team must not be editable.
///
/// `disband_team` and `withdraw_from_season` both refuse to act on a terminal
/// row; `update_team_authorized` had no such check, so the persistent identity
/// of a permanently disbanded team stayed fully mutable. The sharp edge is the
/// league-scoped uniqueness probe: the dead team's name and tag still occupy
/// the league namespace, so a disbanded row could be renamed onto — or kept
/// squatting — a name a live team wants.
///
/// The rename below is the same request `test_update_team` makes successfully
/// against a live team, so the two tests together pin exactly what changed:
/// disbanded refuses, active still succeeds.
#[tokio::test]
async fn test_disbanded_team_cannot_be_updated() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "disbanded-update-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "disbanded-update-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Doomed Squad", "DOOM").await;

    app.delete_auth(&format!("/v1/league-teams/{team_id}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // The action under test: rename a team that no longer exists.
    let response = app
        .patch_json(
            &format!("/v1/league-teams/{team_id}"),
            &json!({
                "name": "Resurrected Squad",
                "tag": "RES"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let error_body: serde_json::Value = response.json();
    let detail = error_body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("disbanded"),
        "refusal must say why the team cannot be edited, got: {detail}"
    );

    // Cross-check the row itself: the write was refused, not merely reported
    // as refused. Status stays terminal and the identity is untouched.
    let get_response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    let body: serde_json::Value = get_response.json();
    assert_eq!(body["data"]["status"], "disbanded");
    assert_eq!(body["data"]["name"], "Doomed Squad");
    assert_eq!(body["data"]["tag"], "DOOM");
}

// ============================================================================
// CAPTAIN PROMOTION/DEMOTION TESTS
// ============================================================================

#[tokio::test]
async fn test_promote_to_captain() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "promote-captain-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "promote-captain-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Promote Captain Team", "PCT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Promote to captain
    let response = app
        .post_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/members/{player2_id}/promote"
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["role"], "captain");
}

#[tokio::test]
async fn test_demote_from_captain() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "demote-captain-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "demote-captain-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Demote Captain Team", "DCT").await;

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
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
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
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &token2,
    )
    .await
    .assert_status(StatusCode::OK);

    // Promote to captain first
    app.post_auth(&format!(
        "/v1/league-team-seasons/{team_season_id}/members/{player2_id}/promote"
    ))
    .await
    .assert_status(StatusCode::OK);

    // Demote back to player
    let response = app
        .post_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/members/{player2_id}/demote"
        ))
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "auth-test-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "auth-test-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let response = app
        .post_json_no_auth(
            &format!("/v1/league-seasons/{season_id}/teams"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "validation-test-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "validation-test-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Tag too short
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
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
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
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
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({
                "name": "Second Team",
                "tag": "ST2"
            }),
        )
        .await;

    response.assert_status(StatusCode::CONFLICT);
}

// ============================================================================
// TEAM IMAGE UPLOAD TESTS
// ============================================================================

/// Generate a valid PNG image of the given dimensions.
fn generate_test_png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([40, 90, 160, 255]));
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .expect("failed to write test PNG");
    buf
}

#[tokio::test]
async fn test_upload_team_logo() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "logo-upload-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "logo-upload-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Dev user creates the team and becomes owner (has team.settings.manage)
    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Logo Upload Team", "LUT").await;

    // Square PNG within TeamLogo constraints (min 64x64, ~1:1 aspect)
    let png = generate_test_png(512, 512);
    let response = app
        .post_multipart_auth(
            &format!("/v1/league-teams/{team_id}/logo"),
            "file",
            "logo.png",
            "image/png",
            &png,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let logo_url = body["data"]["logo_url"]
        .as_str()
        .expect("logo_url should be set after upload");
    assert!(!logo_url.is_empty());

    // Persisted on the team
    let response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["logo_url"], logo_url);
}

#[tokio::test]
async fn test_upload_team_banner() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "banner-upload-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "banner-upload-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Banner Upload Team", "BUT").await;

    // 4:1 aspect PNG within TeamBanner constraints (min 480x120)
    let png = generate_test_png(1920, 480);
    let response = app
        .post_multipart_auth(
            &format!("/v1/league-teams/{team_id}/banner"),
            "file",
            "banner.png",
            "image/png",
            &png,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let banner_url = body["data"]["banner_url"]
        .as_str()
        .expect("banner_url should be set after upload");
    assert!(!banner_url.is_empty());

    // Persisted on the team
    let response = app.get(&format!("/v1/league-teams/{team_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["banner_url"], banner_url);
}

#[tokio::test]
async fn test_upload_team_logo_rejects_invalid_image() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "bad-logo-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "bad-logo-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (team_id, _team_season_id) =
        create_test_team(&app, season_id, "Bad Logo Team", "BLT").await;

    // Below the 64x64 minimum for TeamLogo
    let png = generate_test_png(32, 32);
    let response = app
        .post_multipart_auth(
            &format!("/v1/league-teams/{team_id}/logo"),
            "file",
            "tiny.png",
            "image/png",
            &png,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// SEASON RE-REGISTRATION TESTS
// ============================================================================

#[tokio::test]
async fn test_register_team_for_new_season() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "reregister-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();

    // Team is created (and owned by the dev user) in season 1
    let season1 = create_test_season(&app, league_id, "reregister-season-1").await;
    let season1_id = season1["data"]["id"].as_str().unwrap();
    let (team_id, _team_season_id) =
        create_test_team(&app, season1_id, "Reregister Team", "RRT").await;

    // A second season opens for registration
    let season2 = create_test_season(&app, league_id, "reregister-season-2").await;
    let season2_id = season2["data"]["id"].as_str().unwrap();

    // Owner registers the existing team for the new season
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season2_id}/teams/register"),
            &json!({ "team_id": team_id }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["team_id"], team_id);
    assert_eq!(body["data"]["season_id"], *season2_id);
    assert_eq!(body["data"]["status"], "forming");

    // Registering twice for the same season conflicts
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season2_id}/teams/register"),
            &json!({ "team_id": team_id }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

// ============================================================================
// INVITATION CANCELLATION TESTS
// ============================================================================

#[tokio::test]
async fn test_cancel_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "cancel-invite-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "cancel-invite-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Cancel Invite Team", "CIT").await;

    // Create an invitee
    let user2 = UserBuilder::new()
        .username("cancel-invitee")
        .email("cancel-invitee@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user2.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player2_id: uuid::Uuid = player_row.get("id");

    // Captain (dev user) invites
    let invite_response = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({
                "player_id": player2_id.to_string(),
                "role": "player"
            }),
        )
        .await;
    invite_response.assert_status(StatusCode::CREATED);
    let invitation: serde_json::Value = invite_response.json();
    let invitation_id = invitation["data"]["id"].as_str().unwrap();

    // Captain cancels the invitation
    let response = app
        .delete_auth(&format!("/v1/league-team-invitations/{invitation_id}"))
        .await;
    response.assert_status(StatusCode::NO_CONTENT);

    // The invitee no longer sees a pending invitation
    let response = app
        .get_with_token("/v1/league-team-invitations/me", &token2)
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "cancelled invitation should not appear in the invitee's pending list"
    );

    // Accepting a cancelled invitation fails
    let response = app
        .post_with_token(
            &format!("/v1/league-team-invitations/{invitation_id}/accept"),
            &token2,
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::OK,
        "accepting a cancelled invitation must not succeed; body: {}",
        response.text()
    );
}

// ============================================================================
// PLAYER LEAGUE TEAMS (BY PLAYER ID) TESTS
// ============================================================================

#[tokio::test]
async fn test_get_player_league_teams() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "player-teams-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "player-teams-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Dev user creates a team (dev player id == dev user id)
    create_test_team(&app, season_id, "Player Teams Test", "PTT").await;
    let dev_player_id = "00000000-0000-0000-0000-000000000001";

    // Public endpoint — no auth required
    let response = app
        .get(&format!("/v1/players/{dev_player_id}/league-teams"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let teams = body["data"].as_array().unwrap();
    assert!(!teams.is_empty());
    assert!(teams.iter().any(|t| t["team_name"] == "Player Teams Test"));

    // A player with no memberships gets an empty list
    let user2 = UserBuilder::new()
        .username("teamless-player")
        .email("teamless@example.com")
        .build_persisted(app.pool())
        .await;
    let response = app
        .get(&format!("/v1/players/{}/league-teams", user2.id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ============================================================================
// ROSTER LOCK — P-14 / P-15 / P-16 / P-18
//
// These four findings are one mechanism, so they are tested as one block.
// The register's framing matters for what these tests must prove:
//
//   P-14  the lock could not be set by ANY caller, so every check below was
//         unreachable code. A test that only asserts "hard_lock refuses X" can
//         pass against a build where the lock can never be turned on.
//   P-15  the direct path and the invitation path disagreed — `add_member`
//         applied both lock predicates, `create_invitation`/`accept_invitation`
//         applied only the primary one. So the decisive case is a SUBSTITUTE
//         under a hard lock, exercised through BOTH paths: a test that only
//         drives `add_member` passes against exactly the half-fix that created
//         the finding.
//   P-16  role changes were not lock-checked at all.
//   P-18  the check was unconditional, with no audited admin escape.
// ============================================================================

/// Create a user + player and return `(player_id, token)`.
async fn create_player_with_token(
    app: &TestApp,
    username: &str,
    email: &str,
) -> (uuid::Uuid, String) {
    let user = UserBuilder::new()
        .username(username)
        .email(email)
        .build_persisted(app.pool())
        .await;
    let token = create_token_for_user(user.id);

    let player_row = sqlx::query("SELECT id FROM players WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let player_id: uuid::Uuid = player_row.get("id");

    (player_id, token)
}

/// Set a season's roster lock through the public PATCH endpoint.
async fn set_roster_lock(app: &TestApp, season_id: &str, lock: &str) {
    let response = app
        .patch_json(
            &format!("/v1/league-seasons/{season_id}"),
            &json!({ "roster_lock_status": lock }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["roster_lock_status"], lock,
        "PATCH reported a roster lock it did not apply"
    );
}

/// P-14 — the roster lock must be settable through the existing season PATCH,
/// must persist, and must record who locked it.
#[tokio::test]
async fn test_roster_lock_is_settable_and_attributed() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-lock-set-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-lock-set-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // Baseline: a fresh season is open.
    assert_eq!(season["data"]["roster_lock_status"], "open");

    set_roster_lock(&app, season_id, "hard_lock").await;

    // It persists — the PATCH response is not enough, P-14 was a dropped field.
    let fetched = app.get(&format!("/v1/league-seasons/{season_id}")).await;
    fetched.assert_status(StatusCode::OK);
    let fetched: serde_json::Value = fetched.json();
    assert_eq!(fetched["data"]["roster_lock_status"], "hard_lock");

    // ...and it is attributed. `roster_locked_by` existed as an audit column
    // that nothing ever wrote.
    let row = sqlx::query(
        "SELECT roster_lock_status, roster_locked_at, roster_locked_by \
         FROM league_seasons WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(season_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();

    let locked_by: Option<uuid::Uuid> = row.get("roster_locked_by");
    let locked_at: Option<chrono::DateTime<chrono::Utc>> = row.get("roster_locked_at");
    assert_eq!(
        locked_by,
        Some(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()),
        "roster_locked_by was not stamped with the admin who set the lock"
    );
    assert!(
        locked_at.is_some(),
        "roster_locked_at was not stamped on a hard lock"
    );

    // The lock can also be lifted again.
    set_roster_lock(&app, season_id, "open").await;
    let fetched = app.get(&format!("/v1/league-seasons/{season_id}")).await;
    let fetched: serde_json::Value = fetched.json();
    assert_eq!(fetched["data"]["roster_lock_status"], "open");
}

/// P-15 — a hard lock must refuse a **substitute** through BOTH the direct
/// member path and the invitation path.
///
/// The substitute role is the decisive case: `add_member_authorized` already
/// refused it, `create_invitation` / `accept_invitation` did not. Driving only
/// one path here would certify the half-fix.
#[tokio::test]
async fn test_hard_lock_refuses_substitutes_on_both_roster_paths() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-lock-both-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-lock-both-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Both Paths Team", "BPT").await;

    let (sub_a, _token_a) =
        create_player_with_token(&app, "lock-sub-a", "lock-sub-a@example.com").await;
    let (sub_b, token_b) =
        create_player_with_token(&app, "lock-sub-b", "lock-sub-b@example.com").await;
    let (sub_c, token_c) =
        create_player_with_token(&app, "lock-sub-c", "lock-sub-c@example.com").await;

    // An invitation issued while the roster is open, held pending across the
    // lock. This is how the invitation path smuggled a player onto a frozen
    // roster: the check at creation time no longer reflects reality.
    let pre_lock_invite = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({ "player_id": sub_c.to_string(), "role": "substitute" }),
        )
        .await;
    pre_lock_invite.assert_status(StatusCode::CREATED);
    let pre_lock_invite: serde_json::Value = pre_lock_invite.json();
    let pending_invitation_id = pre_lock_invite["data"]["id"].as_str().unwrap().to_string();

    set_roster_lock(&app, season_id, "hard_lock").await;

    // Path 1 — direct add.
    let direct = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/members"),
            &json!({ "player_id": sub_a.to_string(), "role": "substitute" }),
        )
        .await;
    direct.assert_status(StatusCode::BAD_REQUEST);

    // Path 2 — invite a substitute. THIS is the half that used to succeed.
    let invite = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({ "player_id": sub_b.to_string(), "role": "substitute" }),
        )
        .await;
    invite.assert_status(StatusCode::BAD_REQUEST);

    // Path 3 — accept an invitation that predates the lock. Also used to
    // succeed, and this one actually seats the player.
    let accept = app
        .post_with_token(
            &format!("/v1/league-team-invitations/{pending_invitation_id}/accept"),
            &token_c,
        )
        .await;
    accept.assert_status(StatusCode::BAD_REQUEST);

    // Path 4 — a player applying to join.
    let apply = app
        .post_json_with_token(
            &format!("/v1/league-team-seasons/{team_season_id}/apply"),
            &json!({ "role": "substitute" }),
            &token_b,
        )
        .await;
    apply.assert_status(StatusCode::BAD_REQUEST);

    // Cross-check the mutation that matters: nobody made it onto the roster.
    let members = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    members.assert_status(StatusCode::OK);
    let members: serde_json::Value = members.json();
    let members = members["data"].as_array().unwrap();
    assert_eq!(
        members.len(),
        1,
        "a locked roster gained members: {members:?}"
    );
}

/// P-15 — a soft lock must behave identically on both paths too: primary
/// members frozen, substitutes still allowed. This pins that the fix did not
/// simply tighten everything to `hard_lock` semantics.
#[tokio::test]
async fn test_soft_lock_agrees_across_both_roster_paths() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-soft-lock-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-soft-lock-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Soft Lock Team", "SLT").await;

    let (p_direct, _t1) =
        create_player_with_token(&app, "soft-primary-a", "soft-primary-a@example.com").await;
    let (p_invite, _t2) =
        create_player_with_token(&app, "soft-primary-b", "soft-primary-b@example.com").await;
    let (s_direct, _t3) =
        create_player_with_token(&app, "soft-sub-a", "soft-sub-a@example.com").await;
    let (s_invite, _t4) =
        create_player_with_token(&app, "soft-sub-b", "soft-sub-b@example.com").await;

    set_roster_lock(&app, season_id, "soft_lock").await;

    // Primary members are frozen on both paths.
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": p_direct.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
        &json!({ "player_id": p_invite.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // Substitutes still move on both paths — soft lock means "minor changes".
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": s_direct.to_string(), "role": "substitute" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
        &json!({ "player_id": s_invite.to_string(), "role": "substitute" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
}

/// P-16 — role changes are lock-checked, and the gate matches the lock's own
/// semantics: refused under `hard_lock`, permitted under `soft_lock`.
#[tokio::test]
async fn test_captaincy_changes_are_gated_on_the_hard_lock_only() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-role-lock-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-role-lock-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Role Lock Team", "RLT").await;

    let (mate, _token) =
        create_player_with_token(&app, "role-lock-mate", "role-lock-mate@example.com").await;

    // Seat them while the roster is open.
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": mate.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Hard lock: captaincy cannot move. Captaincy IS the authority that makes
    // roster changes, so allowing it to move during a freeze reopens the freeze
    // by proxy.
    set_roster_lock(&app, season_id, "hard_lock").await;
    app.post_auth(&format!(
        "/v1/league-team-seasons/{team_season_id}/members/{mate}/promote"
    ))
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // Soft lock: permitted. A promotion moves a member between two *primary*
    // roles, so it never changes who is eligible to play — it is strictly less
    // impactful than the substitute swap a soft lock already allows.
    set_roster_lock(&app, season_id, "soft_lock").await;
    let promoted = app
        .post_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/members/{mate}/promote"
        ))
        .await;
    promoted.assert_status(StatusCode::OK);
    let promoted: serde_json::Value = promoted.json();
    assert_eq!(promoted["data"]["role"], "captain");

    // Demotion is gated identically: refused under a hard lock...
    set_roster_lock(&app, season_id, "hard_lock").await;
    app.post_auth(&format!(
        "/v1/league-team-seasons/{team_season_id}/members/{mate}/demote"
    ))
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // ...and permitted once it is softened.
    set_roster_lock(&app, season_id, "soft_lock").await;
    let demoted = app
        .post_auth(&format!(
            "/v1/league-team-seasons/{team_season_id}/members/{mate}/demote"
        ))
        .await;
    demoted.assert_status(StatusCode::OK);
    let demoted: serde_json::Value = demoted.json();
    assert_eq!(demoted["data"]["role"], "player");
}

/// P-18 — the admin override works AND is recorded.
///
/// The recording is the assertion that matters: an override nobody can audit
/// is the lock quietly ceasing to mean anything. This drives the register's own
/// scenario — a player has to come off a frozen roster and a substitute has to
/// go on.
#[tokio::test]
async fn test_admin_roster_lock_override_is_recorded() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-override-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-override-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Override Team", "OVT").await;

    let (banned, _t1) =
        create_player_with_token(&app, "override-banned", "override-banned@example.com").await;
    let (replacement, _t2) = create_player_with_token(
        &app,
        "override-replacement",
        "override-replacement@example.com",
    )
    .await;

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": banned.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    set_roster_lock(&app, season_id, "hard_lock").await;

    // Without the override the emergency substitution is impossible.
    app.delete_auth(&format!(
        "/v1/league-team-seasons/{team_season_id}/members/{banned}"
    ))
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    let remove_reason = "Player banned for cheating mid-playoffs, ruling PL-2291";
    let add_reason = "Approved substitute for banned player, ruling PL-2291";

    app.delete_auth(&format!(
        "/v1/league-team-seasons/{team_season_id}/members/{banned}\
         ?override_roster_lock=true&override_reason={}",
        urlencoding_lite(remove_reason)
    ))
    .await
    .assert_status(StatusCode::NO_CONTENT);

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({
            "player_id": replacement.to_string(),
            "role": "player",
            "override_roster_lock": true,
            "override_reason": add_reason,
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // The roster really moved.
    let members = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    let members: serde_json::Value = members.json();
    let member_ids: Vec<String> = members["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["player_id"].as_str().unwrap().to_string())
        .collect();
    assert!(!member_ids.contains(&banned.to_string()));
    assert!(member_ids.contains(&replacement.to_string()));

    // ...and BOTH bypasses are on the record: who, when, why.
    let rows = sqlx::query(
        "SELECT changed_by, created_at, old_value, new_value \
         FROM entity_changes \
         WHERE entity_type = 'league_team_season' \
           AND entity_id = $1 \
           AND field_name = 'roster_lock_override' \
         ORDER BY created_at",
    )
    .bind(uuid::Uuid::parse_str(&team_season_id).unwrap())
    .fetch_all(app.pool())
    .await
    .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "expected one audit row per overridden lock, got {}",
        rows.len()
    );

    let dev_player_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let reasons: Vec<String> = rows
        .iter()
        .map(|r| {
            let changed_by: uuid::Uuid = r.get("changed_by");
            assert_eq!(
                changed_by, dev_player_id,
                "override recorded the wrong actor"
            );

            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            assert!(
                created_at <= chrono::Utc::now(),
                "override recorded an impossible timestamp"
            );

            let old_value: serde_json::Value = r.get("old_value");
            assert_eq!(
                old_value["roster_lock_status"], "hard_lock",
                "override did not record which lock it bypassed"
            );

            let new_value: serde_json::Value = r.get("new_value");
            new_value["reason"].as_str().unwrap().to_string()
        })
        .collect();

    assert!(
        reasons.contains(&remove_reason.to_string()),
        "the removal override's reason was not recorded: {reasons:?}"
    );
    assert!(
        reasons.contains(&add_reason.to_string()),
        "the addition override's reason was not recorded: {reasons:?}"
    );
}

/// P-18 — the override is not a hole in the lock: it needs platform team-admin
/// rights and a justification.
#[tokio::test]
async fn test_roster_lock_override_requires_admin_and_a_reason() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "roster-override-authz-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "roster-override-authz-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    // The team is founded by a NON-admin, so they are its captain but hold no
    // platform override. A captain unlocking their own roster would make the
    // lock worthless.
    let (captain, captain_token) =
        create_player_with_token(&app, "override-captain", "override-captain@example.com").await;
    let create = app
        .post_json_with_token(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "Captain Owned", "tag": "COT" }),
            &captain_token,
        )
        .await;
    create.assert_status(StatusCode::CREATED);
    let create: serde_json::Value = create.json();
    let team_season_id = create["data"]["team_season"]["id"].as_str().unwrap();

    let (recruit, _t) =
        create_player_with_token(&app, "override-recruit", "override-recruit@example.com").await;

    set_roster_lock(&app, season_id, "hard_lock").await;

    // Captain, with a perfectly good reason, is still refused.
    app.post_json_with_token(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({
            "player_id": recruit.to_string(),
            "role": "player",
            "override_roster_lock": true,
            "override_reason": "We really need this player for the final",
        }),
        &captain_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    // An admin without a justification is refused too.
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({
            "player_id": recruit.to_string(),
            "role": "player",
            "override_roster_lock": true,
        }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // Nothing was written to the audit trail by a refused override.
    let audited: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_changes \
         WHERE entity_type = 'league_team_season' AND entity_id = $1 \
           AND field_name = 'roster_lock_override'",
    )
    .bind(uuid::Uuid::parse_str(team_season_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(audited, 0, "a refused override still wrote an audit row");

    // The roster is untouched: still just the founding captain.
    let members = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    let members: serde_json::Value = members.json();
    let members = members["data"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["player_id"], captain.to_string());
}

/// Percent-encode the handful of characters our override reasons contain.
/// Deliberately tiny — pulling in a URL-encoding crate for two test strings
/// would be a dependency for nothing.
fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

// ============================================================================
// P-148 / P-147 — the lock is the control, not the season phase
// ============================================================================
//
// The owner's ruling: *"I think the roster lock should really be an 'optional'
// thing/thing that is a per tournament decision, (again, this is a casual
// league, so adding team members half way through may be okay)."*
//
// What the rule WAS: every roster predicate ANDed the lock with
// `SeasonStatus::allows_roster_changes()` (`draft | registration`). Season
// phase was the outer gate, so once a season reached `active` or `playoffs`
// NO roster change was possible whatever the lock said — and the lock, sold as
// the thing that stops mid-playoffs player swaps, was inert in exactly that
// window.
//
// What the rule IS: `draft` / `registration` / `active` / `playoffs` all defer
// to the season's `roster_lock_status`, whose DB default is `open` (migration
// 0025), so a casual league gets casual behaviour for free. `completed` and
// `cancelled` refuse everything regardless of the lock, because a played
// season's roster is the record of who played it.
//
// The knob is per LEAGUE SEASON (`league_seasons.roster_lock_status`). The
// ruling said "per tournament"; no tournament-level roster lock exists.

/// Move a season to `status` through the public PATCH endpoint.
async fn set_season_status(app: &TestApp, season_id: &str, status: &str) {
    // P-199: the generic PATCH now enforces the same transition chain as the
    // dedicated status endpoint, so this fixture can no longer jump a season
    // straight to its target — that single-PATCH shortcut was exactly the
    // bypass P-199 closed. Walk the legal chain from wherever the season
    // stands; each hop asserts, so a broken chain fails loudly here.
    const CHAIN: [&str; 5] = ["draft", "registration", "active", "playoffs", "completed"];

    let fetched = app.get(&format!("/v1/league-seasons/{season_id}")).await;
    fetched.assert_status(StatusCode::OK);
    let fetched: serde_json::Value = fetched.json();
    let current = fetched["data"]["status"].as_str().unwrap().to_string();

    let hops: Vec<&str> = if status == "cancelled" {
        // Legal from any non-terminal status in one hop.
        vec!["cancelled"]
    } else {
        let from = CHAIN
            .iter()
            .position(|s| *s == current)
            .unwrap_or_else(|| panic!("season in unexpected status {current}"));
        let to = CHAIN
            .iter()
            .position(|s| *s == status)
            .unwrap_or_else(|| panic!("unknown target season status {status}"));
        assert!(
            to >= from,
            "cannot walk a season backwards ({current} -> {status})"
        );
        CHAIN[from + 1..=to].to_vec()
    };

    for hop in hops {
        let response = app
            .patch_json(
                &format!("/v1/league-seasons/{season_id}"),
                &json!({ "status": hop }),
            )
            .await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["data"]["status"], hop,
            "PATCH reported a season status it did not apply"
        );
    }
}

/// Count active members on a seasonal roster.
async fn active_member_count(app: &TestApp, team_season_id: &str) -> usize {
    let response = app
        .get(&format!("/v1/league-team-seasons/{team_season_id}/members"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["status"] == "active")
        .count()
}

/// P-148 — **the assertion that fails against the old rule.** A casual league
/// leaves the lock `open` and adds a player half way through the season, on
/// both `active` and `playoffs`, through both the direct member path and the
/// invitation path.
#[tokio::test]
async fn test_an_open_lock_lets_a_casual_league_change_its_roster_mid_season() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "casual-open-lock-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "casual-open-lock-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "Casual Crew", "CAS").await;

    let (late_a, _token_a) =
        create_player_with_token(&app, "casual-late-a", "casual-late-a@example.com").await;
    let (late_b, token_b) =
        create_player_with_token(&app, "casual-late-b", "casual-late-b@example.com").await;

    // The competition is under way, and the lock is untouched — the DB default
    // is `open`, which is the whole point of "optional".
    set_season_status(&app, season_id, "active").await;
    assert_eq!(
        season["data"]["roster_lock_status"], "open",
        "the default lock must be open, or this test proves nothing"
    );

    // Path 1 — direct add of a PRIMARY member mid-season. Under the old rule
    // this was a 400 for every season past `registration`.
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": late_a.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Path 2 — invite, then accept, in playoffs. Both halves used to refuse.
    set_season_status(&app, season_id, "playoffs").await;

    let invite = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
            &json!({ "player_id": late_b.to_string(), "role": "substitute" }),
        )
        .await;
    invite.assert_status(StatusCode::CREATED);
    let invite: serde_json::Value = invite.json();
    let invitation_id = invite["data"]["id"].as_str().unwrap().to_string();

    app.post_with_token(
        &format!("/v1/league-team-invitations/{invitation_id}/accept"),
        &token_b,
    )
    .await
    .assert_status(StatusCode::OK);

    // The mutation that matters: both players are really on the roster.
    assert_eq!(
        active_member_count(&app, &team_season_id).await,
        3,
        "an open lock did not actually seat the mid-season additions"
    );
}

/// P-148 — a league that wants strictness sets the lock, and the LOCK is what
/// bites. The lock is set while the season is already `active`, which the old
/// `ensure_lock_change_allowed` refused outright.
#[tokio::test]
async fn test_a_lock_set_mid_season_is_what_freezes_the_roster() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "midseason-lock-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "midseason-lock-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Serious Squad", "SRS").await;

    let (blocked, _t1) =
        create_player_with_token(&app, "midlock-blocked", "midlock-blocked@example.com").await;
    let (sub, _t2) = create_player_with_token(&app, "midlock-sub", "midlock-sub@example.com").await;

    set_season_status(&app, season_id, "active").await;

    // Setting the lock on a live season must work — that is when a league
    // decides its rosters are final.
    set_roster_lock(&app, season_id, "hard_lock").await;

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": blocked.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
        &json!({ "player_id": sub.to_string(), "role": "substitute" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // A soft lock keeps its documented meaning mid-season: substitutes only.
    set_roster_lock(&app, season_id, "soft_lock").await;

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": blocked.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": sub.to_string(), "role": "substitute" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    assert_eq!(
        active_member_count(&app, &team_season_id).await,
        2,
        "soft lock admitted the wrong set of players"
    );
}

/// P-148 — the one status rule that survives. "Optional" does not extend to
/// rewriting a season that has already been played, so a `completed` season
/// refuses every roster change even with the lock wide open, and the refusal
/// names the STATUS (saying "the roster is locked" about an open lock would be
/// a lie).
#[tokio::test]
async fn test_a_completed_season_refuses_roster_changes_with_the_lock_open() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "finished-season-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "finished-season-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    let (_team_id, team_season_id) = create_test_team(&app, season_id, "History Team", "HIS").await;
    let (latecomer, _token) =
        create_player_with_token(&app, "too-late", "too-late@example.com").await;

    set_season_status(&app, season_id, "completed").await;

    let fetched = app.get(&format!("/v1/league-seasons/{season_id}")).await;
    let fetched: serde_json::Value = fetched.json();
    assert_eq!(
        fetched["data"]["roster_lock_status"], "open",
        "the lock must be open, or this test is not testing the status rule"
    );

    let refused = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season_id}/members"),
            &json!({ "player_id": latecomer.to_string(), "role": "player" }),
        )
        .await;
    refused.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = refused.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("season status 'completed'"),
        "refusal must name the status, not the open lock: {body}"
    );

    // The invite path agrees, and the roster really did not move.
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/invitations"),
        &json!({ "player_id": latecomer.to_string(), "role": "substitute" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    assert_eq!(
        active_member_count(&app, &team_season_id).await,
        1,
        "a completed season's roster moved"
    );
}

/// P-147 — `create_team` / `register_for_season` seat a founding captain, and
/// used to do so without the enforcement point ever being consulted. They now
/// go through it, and it rules them exempt from the lock: whether a season
/// takes new teams is decided once, by `is_registration_open()`. Giving the
/// lock a second veto would let a season advertise `status: "registration"`
/// while `POST .../teams` returned 400 for a reason nothing in its payload
/// explains.
#[tokio::test]
async fn test_a_hard_locked_season_still_accepts_new_teams_while_registration_is_open() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let league = create_test_league(&app, &game_id, "founding-under-lock-league").await;
    let league_id = league["data"]["id"].as_str().unwrap();
    let season = create_test_season(&app, league_id, "founding-under-lock-season").await;
    let season_id = season["data"]["id"].as_str().unwrap();

    set_roster_lock(&app, season_id, "hard_lock").await;

    // Registration is still open, so the team is accepted and its founding
    // captain is seated...
    let (_team_id, team_season_id) =
        create_test_team(&app, season_id, "Founded Under Lock", "FUL").await;
    assert_eq!(active_member_count(&app, &team_season_id).await, 1);

    // ...but the roster it founded is frozen from that moment on, which is what
    // the lock actually governs.
    let (other, _token) =
        create_player_with_token(&app, "ful-other", "ful-other@example.com").await;
    app.post_json(
        &format!("/v1/league-team-seasons/{team_season_id}/members"),
        &json!({ "player_id": other.to_string(), "role": "player" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}
