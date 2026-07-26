//! PUG (pick-up game) API integration tests.
//!
//! Exercises the full lifecycle against a real database: create → share-link
//! join → teams → lock (materializes the hidden kind='pug' container
//! tournament) → veto/wheel → per-participant authorization, plus the
//! separation guarantees (hidden from tournament lists, separate stats).

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

const JWT_SECRET: &str = "test-jwt-secret";

/// A user + player (same UUID) with a linked Steam ID and a bearer token.
struct PugUser {
    user_id: Uuid,
    token: String,
}

async fn pug_user(app: &TestApp, name: &str, steam_suffix: u32) -> PugUser {
    let user = UserBuilder::new()
        .username(name)
        .build_persisted(app.pool())
        .await;
    // Lock requires every rostered player to have a linked Steam account.
    sqlx::query("UPDATE players SET steam_id = $2, steam_id_64 = $3 WHERE id = $1")
        .bind(user.id)
        .bind(format!("STEAM_1:0:{steam_suffix}"))
        .bind(76_561_197_960_265_728_i64 + i64::from(steam_suffix))
        .execute(app.pool())
        .await
        .expect("set steam id");
    let token = create_test_token(user.id, user.id, name, JWT_SECRET);
    PugUser {
        user_id: user.id,
        token,
    }
}

async fn create_pug(app: &TestApp, creator: &PugUser, body: Value) -> Value {
    let response = app
        .post_json_with_token("/v1/pugs", &body, &creator.token)
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<Value>()["data"].clone()
}

fn pug_id(detail: &Value) -> String {
    detail["pug"]["id"].as_str().expect("pug id").to_string()
}

fn join_code(detail: &Value) -> String {
    detail["pug"]["join_code"]
        .as_str()
        .expect("join code visible to creator")
        .to_string()
}

// ============================================================================
// LIFECYCLE — WHEEL MODE
// ============================================================================

#[tokio::test]
async fn test_pug_wheel_bo1_full_lifecycle() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;

    let alice = pug_user(&app, "pug_wheel_alice", 9001).await;
    let bob = pug_user(&app, "pug_wheel_bob", 9002).await;

    // Create a 1v1 wheel bo1.
    let detail = create_pug(
        &app,
        &alice,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "wheel",
            "team_size": 1
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);
    assert_eq!(detail["pug"]["my_role"], "creator");
    assert_eq!(detail["pug"]["status"], "gathering");
    assert_eq!(detail["players"].as_array().unwrap().len(), 1);

    // Unauthenticated share-link preview.
    let preview = app.get(&format!("/v1/pugs/code/{code}")).await;
    preview.assert_status(StatusCode::OK);
    let preview = preview.json::<Value>();
    assert_eq!(preview["data"]["players_count"], 1);
    assert_eq!(preview["data"]["slots_total"], 2);

    // Bob joins via the code and picks team 2.
    let joined = app
        .post_json_with_token(
            &format!("/v1/pugs/code/{code}/join"),
            &json!({}),
            &bob.token,
        )
        .await;
    joined.assert_status(StatusCode::OK);
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/team"),
        &json!({ "team": 2 }),
        &bob.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    // Nominations: both nominate — duplicates would weight the wheel.
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/wheel-entry"),
        &json!({ "map_id": "de_mirage" }),
        &alice.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/wheel-entry"),
        &json!({ "map_id": "de_nuke" }),
        &bob.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    // Lock: materializes the hidden container tournament + veto session.
    let locked = app
        .post_json_with_token(&format!("/v1/pugs/{id}/lock"), &json!({}), &alice.token)
        .await;
    locked.assert_status(StatusCode::OK);
    let locked = locked.json::<Value>()["data"].clone();
    assert_eq!(locked["pug"]["status"], "map_selection");
    let match_id = locked["pug"]["match_id"].as_str().expect("match id").to_string();
    assert!(
        locked["my_registration_id"].is_string(),
        "creator must resolve a registration for the veto UI"
    );

    // The container tournament never appears in public listings.
    let tournaments = app.get("/v1/tournaments").await;
    tournaments.assert_status(StatusCode::OK);
    let listing = tournaments.text();
    assert!(
        !listing.contains("PUG #"),
        "kind='pug' containers must be hidden from tournament lists"
    );

    // Wheel session: in_progress, no coin flip, awaiting the spin.
    let veto = app.get(&format!("/v1/matches/{match_id}/veto")).await;
    veto.assert_status(StatusCode::OK);
    let veto = veto.json::<Value>();
    assert_eq!(veto["data"]["session"]["status"], "in_progress");
    assert_eq!(veto["data"]["session"]["veto_format_id"], "wheel_bo1");

    // Bob (participant, not creator... but a captain? no — alice is the only
    // captain) cannot spin; alice can.
    let forbidden = app
        .post_json_with_token(&format!("/v1/pugs/{id}/spin"), &json!({}), &bob.token)
        .await;
    forbidden.assert_status(StatusCode::FORBIDDEN);

    let spin = app
        .post_json_with_token(&format!("/v1/pugs/{id}/spin"), &json!({}), &alice.token)
        .await;
    spin.assert_status(StatusCode::OK);
    let spin = spin.json::<Value>()["data"].clone();
    let winner = spin["winner_map_id"].as_str().expect("winner");
    assert!(
        winner == "de_mirage" || winner == "de_nuke",
        "wheel must land on a nominated map, got {winner}"
    );
    assert_eq!(spin["is_complete"], true, "bo1 completes after one spin");
    assert_eq!(spin["game_number"], 1);
    assert!(spin["spin_seed"].is_i64());

    // The spin is recorded as a 'random' veto action and the session closed.
    let veto = app.get(&format!("/v1/matches/{match_id}/veto")).await.json::<Value>();
    assert_eq!(veto["data"]["session"]["status"], "completed");
    let actions = veto["data"]["actions"].as_array().expect("actions");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["action_type"], "random");
    assert_eq!(actions[0]["was_auto_action"], true);
    assert_eq!(actions[0]["map_id"], winner);

    // Spin audit row is replayable from the lobby state.
    let detail = app
        .get_with_token(&format!("/v1/pugs/{id}"), &alice.token)
        .await
        .json::<Value>();
    let spins = detail["data"]["spins"].as_array().expect("spins");
    assert_eq!(spins.len(), 1);
    assert_eq!(spins[0]["winner_map_id"], winner);

    // Spinning again once complete is rejected.
    let again = app
        .post_json_with_token(&format!("/v1/pugs/{id}/spin"), &json!({}), &alice.token)
        .await;
    again.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// LIFECYCLE — VETO MODE + AD-HOC AUTHORIZATION
// ============================================================================

#[tokio::test]
async fn test_pug_veto_mode_adhoc_members_can_ban() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;

    let carol = pug_user(&app, "pug_veto_carol", 9101).await;
    let dave = pug_user(&app, "pug_veto_dave", 9102).await;

    let detail = create_pug(
        &app,
        &carol,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "veto",
            "team_size": 1
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);

    app.post_json_with_token(&format!("/v1/pugs/code/{code}/join"), &json!({}), &dave.token)
        .await
        .assert_status(StatusCode::OK);
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/team"),
        &json!({ "team": 2 }),
        &dave.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    let locked = app
        .post_json_with_token(&format!("/v1/pugs/{id}/lock"), &json!({}), &carol.token)
        .await;
    locked.assert_status(StatusCode::OK);
    let locked = locked.json::<Value>()["data"].clone();
    let match_id = locked["pug"]["match_id"].as_str().unwrap().to_string();

    // Standard bo1 session: coin flip recorded by the materializer, session
    // in progress on the CS2 default 7-map pool.
    let veto = app.get(&format!("/v1/matches/{match_id}/veto")).await.json::<Value>();
    let session = &veto["data"]["session"];
    assert_eq!(session["status"], "in_progress");
    assert_eq!(session["veto_format_id"], "bo1_standard");
    assert_eq!(session["map_pool"].as_array().unwrap().len(), 7);
    let turn = session["current_team_turn"].as_str().expect("turn set").to_string();

    // Whoever holds the turn bans a map through the standard veto endpoint —
    // this is the ad-hoc speaks-for branch end to end.
    let carol_reg = locked["my_registration_id"].as_str().unwrap();
    let (actor, bystander) = if turn == carol_reg {
        (&carol, &dave)
    } else {
        (&dave, &carol)
    };
    let map = session["remaining_maps"][0].as_str().unwrap().to_string();

    // The other participant is refused (not their turn)...
    app.post_json_with_token(
        &format!("/v1/matches/{match_id}/veto/action"),
        &json!({ "map_id": map }),
        &bystander.token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    // ...while the turn-holder's ban lands.
    let ban = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/action"),
            &json!({ "map_id": map }),
            &actor.token,
        )
        .await;
    ban.assert_status(StatusCode::OK);
    let ban = ban.json::<Value>();
    assert_eq!(ban["data"]["action"]["action_type"], "ban");
}

#[tokio::test]
async fn test_pug_lock_requires_steam_linked_players() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;

    // No steam id for this creator.
    let user = UserBuilder::new()
        .username("pug_nosteam")
        .build_persisted(app.pool())
        .await;
    let token = create_test_token(user.id, user.id, "pug_nosteam", JWT_SECRET);
    let creator = PugUser {
        user_id: user.id,
        token,
    };
    let eve = pug_user(&app, "pug_nosteam_eve", 9201).await;

    let detail = create_pug(
        &app,
        &creator,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "veto",
            "team_size": 1
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);
    app.post_json_with_token(&format!("/v1/pugs/code/{code}/join"), &json!({}), &eve.token)
        .await
        .assert_status(StatusCode::OK);
    app.put_json_with_token(&format!("/v1/pugs/{id}/team"), &json!({"team": 2}), &eve.token)
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let locked = app
        .post_json_with_token(&format!("/v1/pugs/{id}/lock"), &json!({}), &creator.token)
        .await;
    locked.assert_status(StatusCode::BAD_REQUEST);
    assert!(
        locked.text().contains("Steam"),
        "lock must name the players missing Steam links"
    );
}

// ============================================================================
// AUTHORIZATION + CAPS
// ============================================================================

#[tokio::test]
async fn test_pug_creator_only_controls_and_cancel() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;

    let frank = pug_user(&app, "pug_ctl_frank", 9301).await;
    let grace = pug_user(&app, "pug_ctl_grace", 9302).await;

    let detail = create_pug(
        &app,
        &frank,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo3",
            "map_selection_mode": "veto",
            "team_size": 5
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);

    app.post_json_with_token(&format!("/v1/pugs/code/{code}/join"), &json!({}), &grace.token)
        .await
        .assert_status(StatusCode::OK);

    // Non-creator: no shuffle, no swap, no kick, no code rotation.
    for (method_is_put, uri, body) in [
        (false, format!("/v1/pugs/{id}/shuffle"), json!({})),
        (false, format!("/v1/pugs/{id}/swap-teams"), json!({})),
        (
            false,
            format!("/v1/pugs/{id}/kick"),
            json!({ "player_id": frank.user_id.to_string() }),
        ),
        (false, format!("/v1/pugs/{id}/code/rotate"), json!({})),
    ] {
        let response = if method_is_put {
            app.put_json_with_token(&uri, &body, &grace.token).await
        } else {
            app.post_json_with_token(&uri, &body, &grace.token).await
        };
        response.assert_status(StatusCode::FORBIDDEN);
    }

    // Grace can't move Frank; Frank can move Grace.
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/team"),
        &json!({ "player_id": frank.user_id.to_string(), "team": 2 }),
        &grace.token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/team"),
        &json!({ "player_id": grace.user_id.to_string(), "team": 2 }),
        &frank.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    // Rotating the code kills the old link.
    let rotated = app
        .post_json_with_token(&format!("/v1/pugs/{id}/code/rotate"), &json!({}), &frank.token)
        .await;
    rotated.assert_status(StatusCode::OK);
    let new_code = rotated.json::<Value>()["data"]["join_code"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(new_code, code);
    app.get(&format!("/v1/pugs/code/{code}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Cancel (creator), then the lobby stops accepting joins.
    let henry = pug_user(&app, "pug_ctl_henry", 9303).await;
    app.post_json_with_token(&format!("/v1/pugs/{id}/cancel"), &json!({}), &frank.token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    app.post_json_with_token(
        &format!("/v1/pugs/code/{new_code}/join"),
        &json!({}),
        &henry.token,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_pug_active_creation_cap() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;
    let creator = pug_user(&app, "pug_cap_creator", 9401).await;

    let body = json!({
        "game_id": game_id.to_string(),
        "match_format": "bo1",
        "map_selection_mode": "veto",
        "team_size": 1
    });
    for _ in 0..2 {
        create_pug(&app, &creator, body.clone()).await;
    }
    let third = app.post_json_with_token("/v1/pugs", &body, &creator.token).await;
    third.assert_status(StatusCode::CONFLICT);
}

// ============================================================================
// STATS (separate feed)
// ============================================================================

#[tokio::test]
async fn test_pug_stats_endpoint_zeroes_for_fresh_player() {
    let app = TestApp::new().await;
    let user = pug_user(&app, "pug_stats_fresh", 9501).await;

    let response = app
        .get(&format!("/v1/players/{}/pug-stats", user.user_id))
        .await;
    response.assert_status(StatusCode::OK);
    let stats = response.json::<Value>();
    assert_eq!(stats["data"]["matches_played"], 0);
    assert_eq!(stats["data"]["wins"], 0);
    assert_eq!(stats["data"]["demos_counted"], 0);
}

#[tokio::test]
async fn test_my_pugs_and_mine_visibility() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;
    let creator = pug_user(&app, "pug_mine_creator", 9601).await;

    let detail = create_pug(
        &app,
        &creator,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "wheel",
            "team_size": 2
        }),
    )
    .await;
    let id = pug_id(&detail);

    let mine = app.get_with_token("/v1/pugs/mine", &creator.token).await;
    mine.assert_status(StatusCode::OK);
    let mine = mine.json::<Value>();
    let items = mine["data"].as_array().unwrap();
    assert!(items.iter().any(|p| p["id"] == id.as_str()));
    assert!(items.iter().all(|p| p["my_role"].is_string()));

    // Unlisted gathering pugs are private to non-participants without a code.
    let other = pug_user(&app, "pug_mine_other", 9602).await;
    app.get_with_token(&format!("/v1/pugs/{id}"), &other.token)
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

// ============================================================================
// CAPTAINS DRAFT
// ============================================================================

#[tokio::test]
async fn test_pug_captains_draft_alternates_by_roster_size() {
    let app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;

    let host = pug_user(&app, "pug_draft_host", 9701).await;
    let cap2 = pug_user(&app, "pug_draft_cap2", 9702).await;
    let bench_a = pug_user(&app, "pug_draft_a", 9703).await;
    let bench_b = pug_user(&app, "pug_draft_b", 9704).await;

    let detail = create_pug(
        &app,
        &host,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "veto",
            "team_size": 2
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);

    for user in [&cap2, &bench_a, &bench_b] {
        app.post_json_with_token(&format!("/v1/pugs/code/{code}/join"), &json!({}), &user.token)
            .await
            .assert_status(StatusCode::OK);
    }
    // cap2 anchors team 2 and gets the armband.
    app.put_json_with_token(&format!("/v1/pugs/{id}/team"), &json!({"team": 2}), &cap2.token)
        .await
        .assert_status(StatusCode::NO_CONTENT);
    app.put_json_with_token(
        &format!("/v1/pugs/{id}/captain"),
        &json!({"player_id": cap2.user_id.to_string(), "is_captain": true}),
        &host.token,
    )
    .await
    .assert_status(StatusCode::NO_CONTENT);

    // Rosters are 1v1 → team 1 picks first (tie → team 1). Team 2's captain
    // trying to jump the queue is refused.
    let jumped = app
        .post_json_with_token(
            &format!("/v1/pugs/{id}/draft"),
            &json!({"player_id": bench_a.user_id.to_string()}),
            &cap2.token,
        )
        .await;
    jumped.assert_status(StatusCode::FORBIDDEN);

    // Team 1's captain (the host) drafts → lands on team 1.
    let first = app
        .post_json_with_token(
            &format!("/v1/pugs/{id}/draft"),
            &json!({"player_id": bench_a.user_id.to_string()}),
            &host.token,
        )
        .await;
    first.assert_status(StatusCode::OK);
    assert_eq!(first.json::<Value>()["data"]["team"], 1);

    // Now 2v1 → team 2 picks; cap2's draft lands on team 2.
    let second = app
        .post_json_with_token(
            &format!("/v1/pugs/{id}/draft"),
            &json!({"player_id": bench_b.user_id.to_string()}),
            &cap2.token,
        )
        .await;
    second.assert_status(StatusCode::OK);
    assert_eq!(second.json::<Value>()["data"]["team"], 2);

    // Bench empty → drafting an already-teamed player is rejected.
    let dry = app
        .post_json_with_token(
            &format!("/v1/pugs/{id}/draft"),
            &json!({"player_id": bench_a.user_id.to_string()}),
            &host.token,
        )
        .await;
    dry.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// LOBBY WEBSOCKET (doorbell frames)
// ============================================================================

#[tokio::test]
async fn test_pug_ws_doorbell_on_join_and_rejects_outsiders() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::time::{Duration, timeout};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let mut app = TestApp::new().await;
    let game_id = get_cs2_game_id(app.pool()).await;
    let addr = app.start_server().await;

    let host = pug_user(&app, "pug_ws_host", 9801).await;
    let guest = pug_user(&app, "pug_ws_guest", 9802).await;
    let outsider = pug_user(&app, "pug_ws_outsider", 9803).await;

    let detail = create_pug(
        &app,
        &host,
        json!({
            "game_id": game_id.to_string(),
            "match_format": "bo1",
            "map_selection_mode": "wheel",
            "team_size": 2
        }),
    )
    .await;
    let id = pug_id(&detail);
    let code = join_code(&detail);

    async fn next_json(
        ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
              + Unpin),
    ) -> Value {
        loop {
            let frame = timeout(Duration::from_secs(5), ws.next())
                .await
                .expect("ws frame timeout")
                .expect("ws closed")
                .expect("ws error");
            if let Message::Text(text) = frame {
                return serde_json::from_str(&text).expect("json frame");
            }
        }
    }

    // Host connects and authenticates.
    let (mut ws, _) = connect_async(format!("ws://{addr}/v1/ws/pug/{id}"))
        .await
        .expect("connect");
    ws.send(Message::Text(
        json!({"type": "auth", "token": host.token}).to_string().into(),
    ))
    .await
    .unwrap();
    let hello = next_json(&mut ws).await;
    assert_eq!(hello["type"], "auth_success");

    // An outsider without the code is refused (private gathering lobby).
    let (mut outsider_ws, _) = connect_async(format!("ws://{addr}/v1/ws/pug/{id}"))
        .await
        .expect("connect");
    outsider_ws
        .send(Message::Text(
            json!({"type": "auth", "token": outsider.token}).to_string().into(),
        ))
        .await
        .unwrap();
    let refused = next_json(&mut outsider_ws).await;
    assert_eq!(refused["type"], "auth_error");

    // ...but the same outsider WITH the invite code may watch.
    let (mut watcher_ws, _) = connect_async(format!("ws://{addr}/v1/ws/pug/{id}"))
        .await
        .expect("connect");
    watcher_ws
        .send(Message::Text(
            json!({"type": "auth", "token": outsider.token, "code": code})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let watching = next_json(&mut watcher_ws).await;
    assert_eq!(watching["type"], "auth_success");

    // A REST mutation rings the doorbell on every connection.
    app.post_json_with_token(&format!("/v1/pugs/code/{code}/join"), &json!({}), &guest.token)
        .await
        .assert_status(StatusCode::OK);

    let ding = next_json(&mut ws).await;
    assert_eq!(ding["type"], "pug_changed");
    assert_eq!(ding["reason"], "player_joined");
    let ding = next_json(&mut watcher_ws).await;
    assert_eq!(ding["type"], "pug_changed");
}
