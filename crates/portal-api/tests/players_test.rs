//! Player API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_test::prelude::*;

#[tokio::test]
async fn test_get_player() {
    let app = TestApp::new().await;

    // Create a player
    let user = UserBuilder::new()
        .username("testplayer")
        .email("player@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("TestPlayer123")
        .country("US")
        .build_persisted(app.pool())
        .await;

    // Get the player
    let response = app.get(&format!("/v1/players/{}", player.id)).await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["display_name"], "TestPlayer123");
    assert_eq!(body["data"]["country_code"], "US");
}

#[tokio::test]
async fn test_get_player_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/players/00000000-0000-0000-0000-000000000099")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_players() {
    let app = TestApp::new().await;

    // Create some players
    for i in 1..=3 {
        let user = UserBuilder::new()
            .username(&format!("searchuser{}", i))
            .email(&format!("search{}@example.com", i))
            .build_persisted(app.pool())
            .await;

        PlayerBuilder::new()
            .user_id(user.id)
            .display_name(&format!("SearchPlayer{}", i))
            .build_persisted(app.pool())
            .await;
    }

    // Search for players
    let response = app.get("/v1/players?q=searchplayer").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    assert_eq!(body["pagination"]["total_items"], 3);
}

#[tokio::test]
async fn test_search_players_empty() {
    let app = TestApp::new().await;

    let response = app.get("/v1/players?q=nonexistent").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
    assert_eq!(body["pagination"]["total_items"], 0);
}

#[tokio::test]
async fn test_get_player_teams() {
    let app = TestApp::new().await;

    // Create a player
    let user = UserBuilder::new()
        .username("teamplayer")
        .email("teamplayer@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("TeamPlayer")
        .build_persisted(app.pool())
        .await;

    // Create teams for the player
    TeamBuilder::new()
        .name("Player Team 1")
        .tag("PT1")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    TeamBuilder::new()
        .name("Player Team 2")
        .tag("PT2")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Get player's teams
    let response = app.get(&format!("/v1/players/{}/teams", player.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 2);
}

#[tokio::test]
async fn test_get_player_teams_returns_membership_info() {
    let app = TestApp::new().await;

    // Create a player
    let user = UserBuilder::new()
        .username("membershipplayer")
        .email("membershipplayer@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("MembershipPlayer")
        .build_persisted(app.pool())
        .await;

    // Create a team where this player is the founder (captain)
    let team = TeamBuilder::new()
        .name("Captain Team")
        .tag("CPT")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Get player's teams
    let response = app.get(&format!("/v1/players/{}/teams", player.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 1);

    // Verify the response contains all required fields for PlayerTeamMembershipResponse
    let membership = &body[0];
    assert_eq!(membership["team_id"], team.team.id.to_string());
    assert_eq!(membership["team_name"], "Captain Team");
    assert_eq!(membership["team_tag"], "CPT");
    assert_eq!(membership["role"], "captain"); // Founder is always captain
    assert!(membership["joined_at"].is_string());
    // team_logo_url should be present (even if null)
    assert!(membership.get("team_logo_url").is_some());
}

#[tokio::test]
async fn test_get_player_teams_returns_correct_role_for_non_captain() {
    let app = TestApp::new().await;

    // Create the founder player
    let founder_user = UserBuilder::new()
        .username("teamfounder")
        .email("teamfounder@example.com")
        .build_persisted(app.pool())
        .await;

    let founder = PlayerBuilder::new()
        .user_id(founder_user.id)
        .display_name("TeamFounder")
        .build_persisted(app.pool())
        .await;

    // Create a regular member player
    let member_user = UserBuilder::new()
        .username("regularmember")
        .email("regularmember@example.com")
        .build_persisted(app.pool())
        .await;

    let member = PlayerBuilder::new()
        .user_id(member_user.id)
        .display_name("RegularMember")
        .build_persisted(app.pool())
        .await;

    // Create a team with founder and add the member as a "player" role
    TeamBuilder::new()
        .name("Mixed Role Team")
        .tag("MRT")
        .with_founder(founder.id)
        .with_player(Some(member.id))
        .build_persisted(app.pool())
        .await;

    // Get the member's teams (not the founder)
    let response = app.get(&format!("/v1/players/{}/teams", member.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 1);

    // Verify the member has "player" role, not "captain"
    let membership = &body[0];
    assert_eq!(membership["team_name"], "Mixed Role Team");
    assert_eq!(membership["role"], "player");
}

#[tokio::test]
async fn test_get_player_teams_with_multiple_roles() {
    let app = TestApp::new().await;

    // Create a player who will have different roles in different teams
    let user = UserBuilder::new()
        .username("multirole")
        .email("multirole@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("MultiRolePlayer")
        .build_persisted(app.pool())
        .await;

    // Create another user to be founder of one team
    let other_user = UserBuilder::new()
        .username("otherfounder")
        .email("otherfounder@example.com")
        .build_persisted(app.pool())
        .await;

    let other_founder = PlayerBuilder::new()
        .user_id(other_user.id)
        .display_name("OtherFounder")
        .build_persisted(app.pool())
        .await;

    // Team 1: player is the founder (captain)
    TeamBuilder::new()
        .name("Own Team")
        .tag("OWN")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Team 2: player is just a member (player role)
    TeamBuilder::new()
        .name("Other Team")
        .tag("OTH")
        .with_founder(other_founder.id)
        .with_player(Some(player.id))
        .build_persisted(app.pool())
        .await;

    // Get player's teams
    let response = app.get(&format!("/v1/players/{}/teams", player.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 2);

    // Find each team and verify roles
    let own_team = body.iter().find(|t| t["team_name"] == "Own Team").unwrap();
    let other_team = body.iter().find(|t| t["team_name"] == "Other Team").unwrap();

    assert_eq!(own_team["role"], "captain");
    assert_eq!(other_team["role"], "player");
}

#[tokio::test]
async fn test_get_player_teams_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/players/00000000-0000-0000-0000-000000000099/teams")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_player_teams_empty() {
    let app = TestApp::new().await;

    // Create a player with no teams
    let user = UserBuilder::new()
        .username("noteamplayer")
        .email("noteamplayer@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("NoTeamPlayer")
        .build_persisted(app.pool())
        .await;

    // Get player's teams
    let response = app.get(&format!("/v1/players/{}/teams", player.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert!(body.is_empty());
}
