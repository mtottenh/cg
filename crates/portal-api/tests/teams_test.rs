//! Team API integration tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use portal_test::prelude::*;
use serde_json::json;

#[tokio::test]
async fn test_create_team() {
    let app = TestApp::new().await;

    // Dev user/player is already seeded by migration 0013_seed_dev_user.sql
    // Create a team as the dev user
    let response = app
        .post_json(
            "/v1/teams",
            &json!({
                "name": "Test Team",
                "tag": "TST",
                "description": "A test team"
            }),
        )
        .await;

    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Test Team");
    assert_eq!(body["data"]["tag"], "TST");
    assert_eq!(body["data"]["status"], "active");
}

#[tokio::test]
async fn test_create_team_validation_error() {
    let app = TestApp::new().await;

    // Dev user/player is already seeded by migration 0013_seed_dev_user.sql
    // Try to create a team with invalid name (too short)
    let response = app
        .post_json(
            "/v1/teams",
            &json!({
                "name": "AB",  // Too short
                "tag": "T"     // Too short
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_team() {
    let app = TestApp::new().await;

    // Create a player
    let user = UserBuilder::new()
        .username("testuser")
        .email("test@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("TestPlayer")
        .build_persisted(app.pool())
        .await;

    // Create a team
    let team = TeamBuilder::new()
        .name("Get Team Test")
        .tag("GTT")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Get the team
    let response = app.get(&format!("/v1/teams/{}", team.team.id)).await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["name"], "Get Team Test");
    assert_eq!(body["data"]["tag"], "GTT");
}

#[tokio::test]
async fn test_get_team_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get("/v1/teams/00000000-0000-0000-0000-000000000099")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_teams() {
    let app = TestApp::new().await;

    // Create players and teams
    let user1 = UserBuilder::new()
        .username("listuser1")
        .email("list1@example.com")
        .build_persisted(app.pool())
        .await;

    let player1 = PlayerBuilder::new()
        .user_id(user1.id)
        .display_name("ListPlayer1")
        .build_persisted(app.pool())
        .await;

    let _team1 = TeamBuilder::new()
        .name("Alpha Team")
        .tag("ALP")
        .with_founder(player1.id)
        .build_persisted(app.pool())
        .await;

    let user2 = UserBuilder::new()
        .username("listuser2")
        .email("list2@example.com")
        .build_persisted(app.pool())
        .await;

    let player2 = PlayerBuilder::new()
        .user_id(user2.id)
        .display_name("ListPlayer2")
        .build_persisted(app.pool())
        .await;

    let _team2 = TeamBuilder::new()
        .name("Beta Team")
        .tag("BET")
        .with_founder(player2.id)
        .build_persisted(app.pool())
        .await;

    // List teams without auth (should work - list is public)
    let response = app.get("/v1/teams").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"].as_array().unwrap().len() >= 2);
    assert!(body["pagination"]["total_items"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn test_list_teams_with_search() {
    let app = TestApp::new().await;

    // Create a player and team
    let user = UserBuilder::new()
        .username("searchuser")
        .email("search@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("SearchPlayer")
        .build_persisted(app.pool())
        .await;

    let _team = TeamBuilder::new()
        .name("Unique Search Team")
        .tag("UST")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Search for the team by name
    let response = app.get("/v1/teams?search=unique").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let teams = body["data"].as_array().unwrap();
    assert!(teams.iter().any(|t| t["name"] == "Unique Search Team"));
}

#[tokio::test]
async fn test_list_team_members() {
    let app = TestApp::new().await;

    // Create a player and team with members
    let user = UserBuilder::new()
        .username("captain")
        .email("captain@example.com")
        .build_persisted(app.pool())
        .await;

    let player = PlayerBuilder::new()
        .user_id(user.id)
        .display_name("Captain")
        .build_persisted(app.pool())
        .await;

    let team = TeamBuilder::new()
        .name("Members Test")
        .tag("MBR")
        .with_founder(player.id)
        .build_persisted(app.pool())
        .await;

    // Get members
    let response = app.get(&format!("/v1/teams/{}/members", team.team.id)).await;

    response.assert_status(StatusCode::OK);

    let body: Vec<serde_json::Value> = response.json();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["role"], "captain");
    assert!(body[0]["is_founder"].as_bool().unwrap());
}

#[tokio::test]
async fn test_unauthorized_without_token() {
    let app = TestApp::new().await;

    // Try to create a team without auth
    let response = app
        .request_raw(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/teams")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&json!({
                        "name": "Test",
                        "tag": "TST"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

impl TestApp {
    /// Make a raw request (used for testing unauthorized access).
    async fn request_raw(&self, request: axum::http::Request<axum::body::Body>) -> common::TestResponse {
        use http_body_util::BodyExt;
        use tower::util::ServiceExt;

        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("request failed");

        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();

        common::TestResponse {
            status,
            body: body.to_vec(),
        }
    }
}
