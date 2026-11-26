//! Player notifications: who hears about what, the caller's own list, and
//! the organiser's Discord delivery settings.

use crate::admin_rescue::team_match;
use crate::common::TestApp;
use crate::common::agent::wait_for;
use crate::game_server_flow::{get_game_id_str, seed_allocated_reservation, seed_available_server};
use crate::tournaments::create_tournament_with_matches_and_opponent;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
use portal_api::state::AppState;
use portal_core::{TournamentMatchId, UserId};
use portal_domain::entities::notification::{NewNotification, NotificationKind};
use portal_domain::repositories::NotificationRepository;
use portal_test::prelude::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// =============================================================================
// Helpers
// =============================================================================

fn lifecycle_config() -> LifecycleConfig {
    LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: Duration::minutes(15),
        check_in_grace: Duration::minutes(10),
        evidence_stale_max_age: Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: Duration::minutes(10),
        batch_limit: 100,
        veto_coin_flip_grace: Duration::seconds(15),
    }
}

/// The caller's notifications (newest first).
async fn mine(app: &TestApp, token: &str) -> Value {
    let response = app.get_with_token("/v1/me/notifications", token).await;
    response.assert_status(StatusCode::OK);
    response.json::<Value>()["data"].clone()
}

fn kinds(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap().to_string())
        .collect()
}

fn count_of(list: &Value, kind: &str) -> usize {
    kinds(list).iter().filter(|k| *k == kind).count()
}

async fn user_with_token(app: &TestApp, label: &str) -> (Uuid, String) {
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();
    let user = UserBuilder::new()
        .username(format!("notif_{label}_{short}"))
        .build_persisted(app.pool())
        .await;
    let token = create_test_token(user.id, user.id, &user.username, TEST_JWT_SECRET);
    (user.id, token)
}

fn note(title: &str, key: Option<&str>) -> NewNotification {
    NewNotification {
        kind: NotificationKind::ProposalReceived,
        title: title.to_string(),
        body: format!("{title} body"),
        link: None,
        tournament_id: None,
        match_id: None,
        dedupe_key: key.map(str::to_string),
    }
}

/// A local stand-in for Discord: records every post, answers 204, or 404
/// when the webhook path contains `missing`.
async fn discord_stub() -> (String, Arc<Mutex<Vec<(String, Value)>>>) {
    use axum::extract::Path;
    let seen: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&seen);
    let router = axum::Router::new().route(
        "/api/webhooks/{*rest}",
        axum::routing::post(
            move |Path(rest): Path<String>, axum::Json(body): axum::Json<Value>| {
                let recorded = Arc::clone(&recorded);
                async move {
                    let missing = rest.contains("missing");
                    recorded.lock().unwrap().push((rest, body));
                    if missing {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::NO_CONTENT
                    }
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (format!("http://{addr}"), seen)
}

// =============================================================================
// Who is told what
// =============================================================================

/// A proposal goes to the people who can answer it — the other team's
/// captain, not its plain members and not the proposing team — and an
/// agreed time goes to everyone on both rosters.
#[tokio::test]
async fn proposals_reach_the_opposing_schedulers_and_agreement_both_rosters() {
    let app = TestApp::new().await;
    let fx = team_match(&app).await;
    let base = format!(
        "/v1/tournaments/{}/matches/{}/schedule",
        fx.tournament_id, fx.match_id
    );
    let when = Utc::now() + Duration::hours(24);
    let response = app
        .post_json_with_token(
            &format!("{base}/propose"),
            &json!({ "proposed_times": [when.to_rfc3339()] }),
            &fx.a_captain_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let proposal = response.json::<Value>()["data"].clone();

    let b_captain = mine(&app, &fx.b_captain_token).await;
    assert_eq!(kinds(&b_captain), vec!["proposal_received"]);
    assert_eq!(b_captain["unread_count"], 1);
    let n = &b_captain["items"][0];
    assert!(
        n["link"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/matches/{}", fx.match_id)),
        "{n}"
    );
    assert_eq!(n["match_id"], fx.match_id.to_string());
    assert!(n["body"].as_str().unwrap().contains("Team AAA"), "{n}");
    for token in [&fx.b_member_token, &fx.a_captain_token, &fx.a_member_token] {
        assert_eq!(mine(&app, token).await["items"], json!([]));
    }

    app.post_json_with_token(
        &format!("{base}/accept"),
        &json!({
            "proposal_id": proposal["id"],
            "selected_time": proposal["proposed_times"][0]
        }),
        &fx.b_captain_token,
    )
    .await
    .assert_status(StatusCode::OK);

    for token in [
        &fx.a_captain_token,
        &fx.a_cocaptain_token,
        &fx.a_member_token,
        &fx.b_captain_token,
        &fx.b_member_token,
    ] {
        assert_eq!(
            count_of(&mine(&app, token).await, "proposal_accepted"),
            1,
            "every roster member hears the agreed time"
        );
    }
}

/// Opening the window tells both sides' captains once, however many passes
/// see it open; near the deadline only the side that has not checked in is
/// reminded, and only once.
#[tokio::test]
async fn check_in_opens_once_and_reminds_only_the_absent_side() {
    let app = TestApp::new().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let fx = team_match(&app).await;
    let (tid, mid) = (fx.tournament_id, fx.match_id);
    let cfg = lifecycle_config();

    app.post_json(
        &format!("/v1/admin/tournaments/{tid}/matches/{mid}/schedule"),
        &json!({ "scheduled_at": (Utc::now() + Duration::minutes(5)).to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);
    for _ in 0..3 {
        run_lifecycle_pass(&state, &cfg, false).await;
    }

    let m = app
        .get(&format!("/v1/tournaments/{tid}/matches/{mid}"))
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(m["status"], "checking_in");
    for token in [&fx.a_captain_token, &fx.b_captain_token] {
        assert_eq!(count_of(&mine(&app, token).await, "checkin_open"), 1);
    }
    assert_eq!(
        count_of(&mine(&app, &fx.b_member_token).await, "checkin_open"),
        0,
        "plain members cannot check in, so they are not asked to"
    );

    // Team AAA checks in; the deadline comes close.
    app.post_json_with_token(
        &format!("/v1/tournaments/{tid}/matches/{mid}/check-in"),
        &json!({ "registration_id": m["participant1_registration_id"] }),
        &fx.a_captain_token,
    )
    .await
    .assert_status(StatusCode::OK);
    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() + INTERVAL '5 minutes' WHERE id = $1",
    )
    .bind(mid)
    .execute(app.pool())
    .await
    .unwrap();
    run_lifecycle_pass(&state, &cfg, false).await;
    run_lifecycle_pass(&state, &cfg, false).await;

    assert_eq!(
        count_of(&mine(&app, &fx.b_captain_token).await, "checkin_reminder"),
        1
    );
    assert_eq!(
        count_of(&mine(&app, &fx.a_captain_token).await, "checkin_reminder"),
        0,
        "the side that checked in is not reminded"
    );
}

/// The roster hears that the server is ready, and nothing in the message is
/// the server's address or password.
#[tokio::test]
async fn server_ready_reaches_the_roster_without_connect_details() {
    let app = TestApp::new().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (tid, mid, _reg1, _reg2, opponent_token) =
        create_tournament_with_matches_and_opponent(&app, "notif-server-ready").await;
    let game_id = get_game_id_str(&app).await;
    seed_available_server(&state, &game_id, "Notify box").await;
    app.post_json(
        &format!("/v1/admin/tournaments/{tid}/matches/{mid}/schedule"),
        &json!({ "scheduled_at": (Utc::now() + Duration::days(7)).to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/tournaments/{tid}/matches/{mid}/transition"),
        &json!({ "to_status": "pick_ban", "override_reason": "test setup" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let (reservation, token) = seed_allocated_reservation(&state, &mid, &tid, &game_id).await;

    let response = app
        .post_json_with_header(
            "/v1/gameserver/events",
            &json!({ "event": "series_start", "matchid": reservation.matchzy_id }),
            "authorization",
            &format!("Bearer {token}"),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let list = mine(&app, &opponent_token).await;
    assert_eq!(count_of(&list, "server_ready"), 1, "{list}");
    let text = list.to_string();
    assert!(!text.contains(&reservation.connect_password), "{text}");
    assert!(!text.contains("connect "), "{text}");
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE match_id = $1 AND kind = 'server_ready'",
    )
    .bind(Uuid::parse_str(&mid).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(rows, 2, "both players of the match");
}

// =============================================================================
// The caller's own list
// =============================================================================

#[tokio::test]
async fn list_count_and_read_are_per_user() {
    let app = TestApp::new().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let repo = state.notifier.service.repo();
    let (u1, t1) = user_with_token(&app, "one").await;
    let (u2, t2) = user_with_token(&app, "two").await;
    let (u1, u2) = (UserId::from_uuid(u1), UserId::from_uuid(u2));
    for i in 0..3 {
        repo.insert_for_users(&[u1], &note(&format!("n{i}"), None))
            .await
            .unwrap();
    }
    repo.insert_for_users(&[u2], &note("other", None))
        .await
        .unwrap();
    // A repeat of the same event is written once.
    for _ in 0..2 {
        repo.insert_for_users(&[u1], &note("once", Some("k1")))
            .await
            .unwrap();
    }

    let count = app
        .get_with_token("/v1/me/notifications/unread-count", &t1)
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(count["unread_count"], 4);
    let latest = count["latest_id"].as_str().unwrap().to_string();

    // Pages of two, newest first, following the cursor.
    let first = app
        .get_with_token("/v1/me/notifications?limit=2", &t1)
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(first["has_more"], true);
    assert_eq!(first["items"][0]["id"], latest);
    assert_eq!(first["items"][0]["title"], "once");
    let cursor = first["items"][1]["id"].as_str().unwrap().to_string();
    let second = app
        .get_with_token(
            &format!("/v1/me/notifications?limit=2&before={cursor}"),
            &t1,
        )
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(second["has_more"], false);
    let titles: Vec<&str> = second["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["n1", "n0"]);

    // Another user cannot page through, or mark, user one's notifications.
    let other = app
        .get_with_token(&format!("/v1/me/notifications?before={cursor}"), &t2)
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(other["items"], json!([]));
    let marked = app
        .post_json_with_token(
            "/v1/me/notifications/read",
            &json!({ "ids": [latest] }),
            &t2,
        )
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(marked["updated"], 0);

    // Mark one, then all.
    let marked = app
        .post_json_with_token(
            "/v1/me/notifications/read",
            &json!({ "ids": [latest] }),
            &t1,
        )
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(marked["updated"], 1);
    let unread = app
        .get_with_token("/v1/me/notifications?unread_only=true", &t1)
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(unread["items"].as_array().unwrap().len(), 3);
    assert_eq!(unread["unread_count"], 3);
    let marked = app
        .post_json_with_token("/v1/me/notifications/read", &json!({ "all": true }), &t1)
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(marked["updated"], 3);
    assert_eq!(mine(&app, &t1).await["unread_count"], 0);
    assert_eq!(
        mine(&app, &t2).await["unread_count"],
        1,
        "user two is untouched"
    );

    // Neither or both of ids/all is refused; so is an anonymous caller.
    app.post_json_with_token("/v1/me/notifications/read", &json!({}), &t1)
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    app.post_json_with_token(
        "/v1/me/notifications/read",
        &json!({ "ids": [latest], "all": true }),
        &t1,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    app.get("/v1/me/notifications")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn retention_drops_old_read_and_very_old_notifications() {
    let app = TestApp::new().await;
    let state = AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let (u, _) = user_with_token(&app, "ret").await;
    let uid = UserId::from_uuid(u);
    let repo = state.notifier.service.repo();
    for title in ["recent read", "old read", "old unread", "ancient unread"] {
        repo.insert_for_users(&[uid], &note(title, None))
            .await
            .unwrap();
    }
    let age = |title: &str, days: i64, read: bool| {
        let pool = app.pool().clone();
        let title = title.to_string();
        async move {
            sqlx::query(
                "UPDATE notifications SET created_at = NOW() - make_interval(days => $2),
                     read_at = CASE WHEN $3 THEN NOW() ELSE NULL END
                 WHERE title = $1",
            )
            .bind(title)
            .bind(i32::try_from(days).unwrap())
            .bind(read)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    age("recent read", 1, true).await;
    age("old read", 40, true).await;
    age("old unread", 40, false).await;
    age("ancient unread", 100, false).await;

    state.notifier.sweep().await;

    let left: Vec<String> =
        sqlx::query_scalar("SELECT title FROM notifications WHERE user_id = $1 ORDER BY title")
            .bind(u)
            .fetch_all(app.pool())
            .await
            .unwrap();
    assert_eq!(left, vec!["old unread", "recent read"]);
}

// =============================================================================
// Organiser settings and Discord delivery
// =============================================================================

#[tokio::test]
async fn notification_settings_validate_and_never_return_the_webhook() {
    let app = TestApp::new().await;
    let (tid, _mid, _r1, _r2, _) =
        create_tournament_with_matches_and_opponent(&app, "notif-settings").await;
    let url = format!("/v1/admin/tournaments/{tid}/notification-settings");
    let secret = "https://discord.com/api/webhooks/123/very-secret-token-abc123";

    // Nothing saved yet.
    let fresh = app.get_auth(&url).await.json::<Value>()["data"].clone();
    assert_eq!(fresh["discord_enabled"], false);
    assert_eq!(fresh["discord_webhook_configured"], false);

    // Not a Discord webhook; Discord on with no URL.
    app.put_json(
        &url,
        &json!({ "discord_enabled": true, "discord_webhook_url": "https://example.com/hook" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    app.put_json(&url, &json!({ "discord_enabled": true }))
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    let saved = app
        .put_json(
            &url,
            &json!({ "discord_enabled": true, "discord_webhook_url": secret }),
        )
        .await;
    saved.assert_status(StatusCode::OK);
    let saved = saved.json::<Value>()["data"].clone();
    assert_eq!(saved["discord_enabled"], true);
    assert_eq!(saved["discord_webhook_configured"], true);
    assert_eq!(saved["discord_webhook_hint"], "abc123");

    // Omitting the URL keeps it; the URL is never sent back anywhere.
    let kept = app
        .put_json(&url, &json!({ "discord_enabled": false }))
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(kept["discord_webhook_configured"], true);
    let read_back = app.get_auth(&url).await.text();
    assert!(!read_back.contains("very-secret"), "{read_back}");
    for public in [
        format!("/v1/tournaments/{tid}"),
        "/v1/tournaments".to_string(),
    ] {
        let body = app.get(&public).await.text();
        assert!(!body.contains("very-secret"), "{public}: {body}");
    }

    // null clears it.
    let cleared = app
        .put_json(
            &url,
            &json!({ "discord_enabled": false, "discord_webhook_url": null }),
        )
        .await
        .json::<Value>()["data"]
        .clone();
    assert_eq!(cleared["discord_webhook_configured"], false);

    // Not an organiser of this tournament.
    let (_, outsider) = user_with_token(&app, "outsider").await;
    app.get_with_token(&url, &outsider)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    app.put_json_with_token(&url, &json!({ "discord_enabled": false }), &outsider)
        .await
        .assert_status(StatusCode::FORBIDDEN);
    app.post_with_token(&format!("{url}/test"), &outsider)
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

/// The test button reports what Discord said without echoing the URL, and a
/// Discord-worthy event is posted to the channel once Discord is on.
#[tokio::test]
async fn discord_delivery_posts_events_and_reports_failures() {
    let (app, state) = TestApp::new_with_agent_dev_auth_and_state().await;
    let (stub, seen) = discord_stub().await;
    state.notifier.redirect_discord_for_tests(&stub);
    let (tid, mid, _r1, _r2, _opponent) =
        create_tournament_with_matches_and_opponent(&app, "notif-discord").await;
    let url = format!("/v1/admin/tournaments/{tid}/notification-settings");

    // A webhook Discord does not know: delivered false, reason without URL.
    app.put_json(
        &url,
        &json!({
            "discord_enabled": true,
            "discord_webhook_url": "https://discord.com/api/webhooks/1/missing-token"
        }),
    )
    .await
    .assert_status(StatusCode::OK);
    let failed = app.post_auth(&format!("{url}/test")).await;
    failed.assert_status(StatusCode::OK);
    let failed = failed.json::<Value>()["data"].clone();
    assert_eq!(failed["delivered"], false);
    let detail = failed["detail"].as_str().unwrap();
    assert!(detail.contains("404"), "{detail}");
    assert!(!detail.contains("missing-token"), "{detail}");

    // A good webhook.
    app.put_json(
        &url,
        &json!({
            "discord_enabled": true,
            "discord_webhook_url": "https://discord.com/api/webhooks/2/good-token"
        }),
    )
    .await
    .assert_status(StatusCode::OK);
    let ok = app.post_auth(&format!("{url}/test")).await.json::<Value>()["data"].clone();
    assert_eq!(ok["delivered"], true, "{ok}");

    // A proposal is Discord-worthy: it lands in the channel.
    let when = Utc::now() + Duration::hours(30);
    app.post_json(
        &format!("/v1/tournaments/{tid}/matches/{mid}/schedule/propose"),
        &json!({ "proposed_times": [when.to_rfc3339()] }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    let posted = wait_for(5, || {
        let seen = Arc::clone(&seen);
        async move {
            let seen = seen.lock().unwrap();
            seen.iter()
                .find(|(path, body)| {
                    path == "2/good-token"
                        && body["content"]
                            .as_str()
                            .is_some_and(|c| c.contains("New match time proposed"))
                })
                .cloned()
        }
    })
    .await;
    let content = posted.1["content"].as_str().unwrap();
    assert!(
        content.starts_with("**Match Test notif-discord**"),
        "{content}"
    );
    assert_eq!(posted.1["allowed_mentions"], json!({ "parse": [] }));

    // The same event again posts nothing new.
    let before = seen.lock().unwrap().len();
    state
        .notifier
        .emit(portal_domain::services::NotificationEvent::ProposalReceived {
            match_id: mid.parse::<TournamentMatchId>().unwrap(),
            proposal_id: sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM schedule_proposals WHERE match_id = $1",
            )
            .bind(Uuid::parse_str(&mid).unwrap())
            .fetch_one(app.pool())
            .await
            .map(portal_core::ScheduleProposalId::from_uuid)
            .unwrap(),
            proposed_by: sqlx::query_scalar::<_, Uuid>(
                "SELECT proposed_by_registration_id FROM schedule_proposals WHERE match_id = $1",
            )
            .bind(Uuid::parse_str(&mid).unwrap())
            .fetch_one(app.pool())
            .await
            .map(portal_core::TournamentRegistrationId::from_uuid)
            .unwrap(),
            counter: false,
        })
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(seen.lock().unwrap().len(), before);
}
