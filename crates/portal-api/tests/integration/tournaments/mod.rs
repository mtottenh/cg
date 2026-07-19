//! Tournament API integration tests.

mod auth;
mod brackets;
mod crud;
mod lifecycle;
mod matches;
mod registration;
mod scheduling;
mod seeding;

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

/// Helper to transition a match to Ready status using admin endpoint.
///
/// Bracket generation may already emit matches in `ready` (both slots
/// filled), and the state machine rejects same-state transitions — so this
/// is a no-op when the match is already ready.
pub async fn transition_match_to_ready(app: &TestApp, tournament_id: &str, match_id: &str) {
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/status"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    if body["data"]["current_status"].as_str() == Some("ready") {
        return;
    }

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
            &json!({
                "to_status": "ready",
                "override_reason": "Test setup: transitioning match to Ready for scheduling tests"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Helper to create a test player using UserBuilder and return (user_id, player_id).
/// UserBuilder creates user and player with the same ID.
pub async fn create_test_player(app: &TestApp, username: &str) -> (Uuid, Uuid) {
    let user = UserBuilder::new()
        .username(username)
        .build_persisted(app.pool())
        .await;
    // UserBuilder creates player with same ID as user
    (user.id, user.id)
}

/// Helper to insert a registration directly using TournamentRegistrationBuilder.
/// Creates an approved registration (ready to participate).
pub async fn insert_test_registration(
    app: &TestApp,
    tournament_id: &str,
    player_id: Uuid,
    user_id: Uuid,
    participant_name: &str,
) -> String {
    let tournament_uuid: Uuid = tournament_id.parse().expect("Invalid tournament ID");

    let reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament_uuid)
        .player_id_from_uuid(player_id)
        .participant_name(participant_name)
        .registered_by_uuid(user_id)
        .approved() // Must be approved to participate
        .build_persisted(app.pool())
        .await;

    reg.id.as_uuid().to_string()
}

/// Helper to create a tournament and open registration.
pub async fn create_tournament_with_registration(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament with min_participants: 2 (the minimum for any tournament)
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Registration Test {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/open-registration"
        ))
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

/// Helper to register a player and return the registration ID.
/// By default, registrations are created with 'pending' status.
pub async fn register_player(app: &TestApp, tournament_id: &str, participant_name: &str) -> String {
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({
                "participant_name": participant_name
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// Helper to approve a registration (for seeding tests).
pub async fn approve_registration(app: &TestApp, tournament_id: &str, registration_id: &str) {
    // Approve via the API
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

/// Helper to create a started tournament with matches.
/// Returns (tournament_id, match_id, registration_id1, registration_id2).
pub async fn create_tournament_with_matches(
    app: &TestApp,
    slug: &str,
) -> (String, String, String, String) {
    let (tournament_id, match_id, reg1, reg2, _player2_token) =
        create_tournament_with_matches_and_opponent(app, slug).await;
    (tournament_id, match_id, reg1, reg2)
}

/// Like [`create_tournament_with_matches`], but also returns a JWT for the
/// second participant so tests can act as the opponent (e.g. respond to
/// schedule proposals made by the dev user).
/// Returns (tournament_id, match_id, reg1, reg2, player2_token).
pub async fn create_tournament_with_matches_and_opponent(
    app: &TestApp,
    slug: &str,
) -> (String, String, String, String, String) {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Create tournament with min_participants: 2
    // Use "self_scheduled" mode for tests that need scheduled matches
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Match Test {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/open-registration"
        ))
        .await;
    response.assert_status(StatusCode::OK);

    // Register 2 players
    let reg1 = register_player(app, &tournament_id, "Player1").await;
    approve_registration(app, &tournament_id, &reg1).await;

    let (user2_id, player2_id) = create_test_player(app, &format!("player2_{slug}")).await;
    let reg2 = insert_test_registration(app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament (creates matches)
    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await;
    response.assert_status(StatusCode::OK);

    // Get matches to find the match ID
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/matches"))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let matches = body["data"].as_array().unwrap();
    assert!(
        !matches.is_empty(),
        "Tournament should have at least one match"
    );

    let match_id = matches[0]["id"].as_str().unwrap().to_string();

    // Grant admin permissions and transition match to Ready status
    // (matches are created in Pending status, need to be Ready for scheduling)
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assign_role_to_user(app.pool(), dev_user_id, "platform_admin").await;
    transition_match_to_ready(app, &tournament_id, &match_id).await;

    let player2_token = create_test_token(
        user2_id,
        player2_id,
        &format!("player2_{slug}"),
        TEST_JWT_SECRET,
    );

    (tournament_id, match_id, reg1, reg2, player2_token)
}
