//! Integration tests for the provisional lineup declaration write path (Phase B).
//!
//! Covers: declaring a lineup for a registration, roster-snapshot resolution
//! (`was_rostered` / `is_substitute` for a declared non-rostered player),
//! opponent visibility gating (a lineup is opponent-visible only once locked),
//! and locking on match start.

use super::*;

/// The dev token identity (see `extractors/auth.rs`). The dev user registers
/// `reg1` in `create_tournament_with_matches`, so this is `reg1`'s player.
const DEV_PLAYER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Schedule the match so declaration/check-in is possible.
async fn schedule(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_time.to_rfc3339() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_declare_lineup_rostered_player() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-declare").await;
    schedule(&app, &tournament_id, &match_id).await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({
                "registration_id": reg1,
                "player_ids": [DEV_PLAYER_ID],
                "submit": true
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "submitted");
    assert_eq!(body["data"]["players_visible"], true);
    let players = body["data"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0]["player_id"], DEV_PLAYER_ID);
    assert_eq!(players[0]["source"], "declared");
    // The registered player is on the (individual) roster.
    assert_eq!(players[0]["was_rostered"], true);
    assert_eq!(players[0]["is_substitute"], false);
}

#[tokio::test]
async fn test_declared_non_rostered_player_is_substitute() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-sub").await;
    schedule(&app, &tournament_id, &match_id).await;

    // A real player who is NOT the registered participant -> a substitute.
    let (_sub_user, sub_player) = create_test_player(&app, "lineup_sub_player").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({
                "registration_id": reg1,
                "player_ids": [DEV_PLAYER_ID, sub_player.to_string()],
                "submit": false
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "draft");
    let players = body["data"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);

    let sub = players
        .iter()
        .find(|p| p["player_id"] == sub_player.to_string())
        .expect("substitute row present");
    assert_eq!(sub["was_rostered"], false);
    assert_eq!(
        sub["is_substitute"], true,
        "a declared non-rostered player is provisionally a substitute"
    );
}

#[tokio::test]
async fn test_redeclaring_replaces_players() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-replace").await;
    schedule(&app, &tournament_id, &match_id).await;

    let (_u, extra) = create_test_player(&app, "lineup_replace_extra").await;
    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup");

    // First declaration: two players.
    let r = app
        .post_json(
            &url,
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID, extra.to_string()] }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["players"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Re-declare with one player: the old rows are replaced, not appended.
    let r = app
        .post_json(
            &url,
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID] }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["players"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_opponent_cannot_see_unlocked_lineup() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-visibility").await;
    schedule(&app, &tournament_id, &match_id).await;

    // reg1 (dev) declares a lineup while the match has not started.
    let r = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID], "submit": true }),
        )
        .await;
    r.assert_status(StatusCode::OK);

    // The opponent (player2) lists lineups: reg1's is unlocked, so its players
    // must be withheld.
    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineups"),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let lineups = body["data"].as_array().unwrap();
    let reg1_lineup = lineups
        .iter()
        .find(|l| l["registration_id"] == reg1)
        .expect("reg1 lineup listed");
    assert_eq!(
        reg1_lineup["players_visible"], false,
        "opponent must not see an unlocked lineup's players"
    );
    assert!(reg1_lineup["players"].as_array().unwrap().is_empty());

    // Sanity: reg2 is not implicated by reg1's declaration.
    let _ = reg2;
}

#[tokio::test]
async fn test_lineup_locks_on_match_start() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-lock").await;
    schedule(&app, &tournament_id, &match_id).await;

    // Declare for reg1.
    let r = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID], "submit": true }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["locked_at"],
        serde_json::Value::Null
    );

    // Both participants check in -> the match auto-advances (PickBan/InProgress),
    // which locks the lineups.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg2 }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // reg1's lineup should now be locked (and thus opponent-visible).
    let body: serde_json::Value = app
        .get_auth(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/lineups"
        ))
        .await
        .json();
    let reg1_lineup = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["registration_id"] == reg1)
        .expect("reg1 lineup listed");
    assert_eq!(reg1_lineup["status"], "locked");
    assert!(
        reg1_lineup["locked_at"].is_string(),
        "locked_at must be stamped on match start"
    );
}

#[tokio::test]
async fn test_cannot_declare_after_lock() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-after-lock").await;
    schedule(&app, &tournament_id, &match_id).await;

    // Start the match via check-in of both sides.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg2 }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // Declaration after the match has started is refused.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID] }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::OK,
        "declaration must be refused once the match has started"
    );
}
