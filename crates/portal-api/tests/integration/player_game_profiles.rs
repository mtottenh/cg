//! Player game profile integration tests.
//!
//! Covers the seven `/v1/players/...` game-profile endpoints: profile
//! list/get, `/me/games`, the admin rating submission, rating history,
//! public matchmaking stats and public match history.

use crate::common::TestApp;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use portal_test::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

// =============================================================================
// HELPERS
// =============================================================================

/// A player UUID that exists in no database.
const UNKNOWN_PLAYER: &str = "00000000-0000-0000-0000-00000000dead";

/// The well-known dev player seeded by migration 0013 — the player behind
/// `Bearer dev-token`.
///
/// Resolved from the constant rather than `portal_test::get_dev_player_id`,
/// which looks the row up by `username = 'dev'` while the migration seeds
/// `'devuser'`, so that helper always panics.
fn dev_player_id() -> Uuid {
    "00000000-0000-0000-0000-000000000001"
        .parse()
        .expect("valid dev player id")
}

/// Submit a rating for a player as the (dev) admin user.
async fn submit_rating(
    app: &TestApp,
    player_id: Uuid,
    game: &str,
    rating: i32,
    source: &str,
    recorded_at: chrono::DateTime<Utc>,
) -> Value {
    let response = app
        .post_json(
            &format!("/v1/players/{player_id}/games/{game}/rating"),
            &json!({
                "rating": rating,
                "source": source,
                "recorded_at": recorded_at.to_rfc3339(),
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}

/// Find a display stat by key in a profile response.
fn display_stat<'a>(profile: &'a Value, key: &str) -> &'a Value {
    profile["display_stats"]
        .as_array()
        .expect("display_stats present")
        .iter()
        .find(|s| s["key"] == key)
        .unwrap_or_else(|| panic!("display stat '{key}' present"))
}

/// Create a non-privileged user and return its bearer token.
async fn outsider_token(app: &TestApp, username: &str) -> String {
    let user = UserBuilder::new()
        .username(username)
        .build_persisted(app.pool())
        .await;
    create_test_token(user.id, user.id, username, TEST_JWT_SECRET)
}

/// Insert an aggregate public-matchmaking stats row for a player.
#[allow(clippy::too_many_arguments)]
async fn insert_mm_stats(
    app: &TestApp,
    player_id: Uuid,
    game_id: Uuid,
    matches_played: i32,
    wins: i32,
    losses: i32,
    kills: i32,
    deaths: i32,
    headshots: i32,
) {
    sqlx::query(
        "INSERT INTO player_mm_stats (
             player_id, game_id, matches_played, wins, losses, draws,
             kills, deaths, assists, headshots, mvps, entry_3k, entry_4k, entry_5k
         ) VALUES ($1, $2, $3, $4, $5, 0, $6, $7, 40, $8, 12, 3, 1, 0)",
    )
    .bind(player_id)
    .bind(game_id)
    .bind(matches_played)
    .bind(wins)
    .bind(losses)
    .bind(kills)
    .bind(deaths)
    .bind(headshots)
    .execute(app.pool())
    .await
    .expect("failed to insert player_mm_stats");
}

/// Insert a discovered public match (with its steam-tracking parent) and
/// return its id, so `player_match_history` rows can reference it.
async fn insert_discovered_match(
    app: &TestApp,
    player_id: Uuid,
    game_id: Uuid,
    tracking_id: Uuid,
    share_code: &str,
) -> Uuid {
    sqlx::query(
        "INSERT INTO steam_tracking (id, player_id, game_id, steam_id_64, game_auth_code)
         VALUES ($1, $2, $3, $4, 'AAAA-AAAAA-AAAA')
         ON CONFLICT (player_id, game_id) DO NOTHING",
    )
    .bind(tracking_id)
    .bind(player_id)
    .bind(game_id)
    .bind(7_656_119_800_001_000_i64)
    .execute(app.pool())
    .await
    .expect("failed to insert steam_tracking");

    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO discovered_matches (tracking_id, game_id, share_code, match_id, outcome_id, token)
         VALUES ($1, $2, $3, 1, 1, 1) RETURNING id",
    )
    .bind(tracking_id)
    .bind(game_id)
    .bind(share_code)
    .fetch_one(app.pool())
    .await
    .expect("failed to insert discovered_match")
}

/// Insert a materialized public match-history row for a player.
async fn insert_match_history(
    app: &TestApp,
    player_id: Uuid,
    game_id: Uuid,
    discovered_match_id: Uuid,
    map: &str,
    match_time: chrono::DateTime<Utc>,
    kills: i32,
) {
    sqlx::query(
        "INSERT INTO player_match_history (
             player_id, game_id, discovered_match_id, map, match_time,
             team_scores, match_duration_secs, match_result,
             kills, deaths, assists, score, headshots, mvps
         ) VALUES ($1, $2, $3, $4, $5, ARRAY[13, 7], 2100, 'win', $6, 8, 4, 60, 9, 3)",
    )
    .bind(player_id)
    .bind(game_id)
    .bind(discovered_match_id)
    .bind(map)
    .bind(match_time)
    .bind(kills)
    .execute(app.pool())
    .await
    .expect("failed to insert player_match_history");
}

// =============================================================================
// RATING SUBMISSION + PROFILE READS
// =============================================================================

/// Submitting a rating creates the profile on demand, records history, and
/// surfaces the plugin-derived rank tier; the profile then shows up on the
/// per-player list, the single-profile read and `/me/games`.
#[tokio::test]
async fn test_submit_rating_creates_profile_and_reads() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();
    let game_id = get_game_id(app.pool(), "cs2").await;
    let now = Utc::now();

    let body = submit_rating(&app, player_id, "cs2", 15_000, "mm_demo", now).await;
    let profile = &body["data"];
    assert_eq!(profile["player_id"], player_id.to_string());
    assert_eq!(profile["game_id"], game_id.to_string());

    // The plugin formats the history-derived rating and its CS2 tier color.
    assert_eq!(display_stat(profile, "elo_current")["value"], "15000");
    assert_eq!(display_stat(profile, "elo_current")["color"], "#9932CC");
    assert_eq!(display_stat(profile, "elo_peak")["value"], "15000");

    // The game path segment accepts a UUID as well as a slug.
    let response = app
        .get(&format!("/v1/players/{player_id}/games/{game_id}"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["game_id"], game_id.to_string());

    // The profile appears on the player's profile list...
    let response = app.get(&format!("/v1/players/{player_id}/games")).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let profiles = body["data"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0]["game_id"], game_id.to_string());

    // ... and on `/me/games` for the authenticated player.
    let response = app.get_auth("/v1/players/me/games").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let mine = body["data"].as_array().unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["player_id"], player_id.to_string());
}

/// Rating history is newest-first, `limit` truncates it, and current/peak
/// ratings are derived from the whole series rather than the last write.
#[tokio::test]
async fn test_rating_history_ordering_and_limit() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();
    let now = Utc::now();

    submit_rating(
        &app,
        player_id,
        "cs2",
        12_000,
        "bot_sync",
        now - Duration::days(2),
    )
    .await;
    submit_rating(
        &app,
        player_id,
        "cs2",
        18_000,
        "bot_sync",
        now - Duration::days(1),
    )
    .await;
    let latest = submit_rating(&app, player_id, "cs2", 16_000, "bot_sync", now).await;

    // Peak comes from the series max, current from the newest entry.
    assert_eq!(
        display_stat(&latest["data"], "elo_current")["value"],
        "16000"
    );
    assert_eq!(display_stat(&latest["data"], "elo_peak")["value"], "18000");

    let response = app
        .get(&format!("/v1/players/{player_id}/games/cs2/rating-history"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["rating"], 16_000, "newest first");
    assert_eq!(entries[1]["rating"], 18_000);
    assert_eq!(entries[2]["rating"], 12_000);
    assert_eq!(entries[0]["source"], "bot_sync");
    assert_eq!(entries[0]["player_id"], player_id.to_string());

    let response = app
        .get(&format!(
            "/v1/players/{player_id}/games/cs2/rating-history?limit=1"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["rating"], 16_000);
}

/// Rating submission is admin-only: anonymous callers get 401, a plain
/// registered user gets 403, and neither write reaches the history.
#[tokio::test]
async fn test_submit_rating_requires_admin_permission() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();
    let url = format!("/v1/players/{player_id}/games/cs2/rating");
    let payload = json!({
        "rating": 15_000,
        "source": "manual",
        "recorded_at": Utc::now().to_rfc3339(),
    });

    app.post_json_no_auth(&url, &payload)
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    let token = outsider_token(&app, "rating-outsider").await;
    app.post_json_with_token(&url, &payload, &token)
        .await
        .assert_status(StatusCode::FORBIDDEN);

    // Neither rejected call created history.
    let response = app
        .get(&format!("/v1/players/{player_id}/games/cs2/rating-history"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}

/// `/me/games` requires authentication.
#[tokio::test]
async fn test_my_game_profiles_requires_auth() {
    let app = TestApp::new().await;

    app.get("/v1/players/me/games")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
}

// =============================================================================
// PUBLIC MATCHMAKING STATS + MATCH HISTORY
// =============================================================================

/// The MM stats card mixes stored aggregates with derived ratios and the
/// rating-history-derived rank tier.
#[tokio::test]
async fn test_player_mm_stats() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();
    let game_id = get_game_id(app.pool(), "cs2").await;

    insert_mm_stats(&app, player_id, game_id, 100, 60, 40, 1000, 800, 450).await;
    submit_rating(&app, player_id, "cs2", 15_000, "mm_demo", Utc::now()).await;

    let response = app
        .get(&format!("/v1/players/{player_id}/games/cs2/mm-stats"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let stats = &body["data"];

    assert_eq!(stats["matches_played"], 100);
    assert_eq!(stats["wins"], 60);
    assert_eq!(stats["losses"], 40);
    assert_eq!(stats["win_rate"], 60.0);
    assert_eq!(stats["kills"], 1000);
    assert_eq!(stats["deaths"], 800);
    assert_eq!(stats["kd_ratio"], 1.25);
    assert_eq!(stats["headshots"], 450);
    assert_eq!(stats["hs_percent"], 45.0);
    assert_eq!(stats["mvps"], 12);
    assert_eq!(stats["entry_3k"], 3);

    // Rating and tier come from rating history via the plugin, not the row.
    assert_eq!(stats["rating"], 15_000);
    assert_eq!(stats["peak_rating"], 15_000);
    assert_eq!(stats["rank_tier"], "Purple");
    assert_eq!(stats["rank_color"], "#9932CC");
}

/// A player with no MM stats row is a 404 rather than an empty card.
#[tokio::test]
async fn test_player_mm_stats_absent_is_not_found() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();

    app.get(&format!("/v1/players/{player_id}/games/cs2/mm-stats"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
}

/// Public match history is newest-first and honours `limit`/`offset`.
#[tokio::test]
async fn test_player_match_history() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();
    let game_id = get_game_id(app.pool(), "cs2").await;
    let tracking_id = Uuid::now_v7();
    let now = Utc::now();

    for (i, (map, kills)) in [("de_dust2", 30), ("de_mirage", 20), ("de_inferno", 10)]
        .iter()
        .enumerate()
    {
        let share_code = format!("CSGO-match-history-{i}");
        let discovered =
            insert_discovered_match(&app, player_id, game_id, tracking_id, &share_code).await;
        insert_match_history(
            &app,
            player_id,
            game_id,
            discovered,
            map,
            now - Duration::hours(i as i64),
            *kills,
        )
        .await;
    }

    let response = app
        .get(&format!("/v1/players/{player_id}/games/cs2/match-history"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["map"], "de_dust2", "newest match first");
    assert_eq!(entries[0]["kills"], 30);
    assert_eq!(entries[0]["deaths"], 8);
    assert_eq!(entries[0]["assists"], 4);
    assert_eq!(entries[0]["headshots"], 9);
    assert_eq!(entries[0]["match_result"], "win");
    assert_eq!(entries[0]["team_scores"], json!([13, 7]));
    assert_eq!(entries[0]["match_duration_secs"], 2100);
    assert_eq!(entries[2]["map"], "de_inferno");

    // Pagination walks the same ordering.
    let response = app
        .get(&format!(
            "/v1/players/{player_id}/games/cs2/match-history?limit=1&offset=1"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["map"], "de_mirage");
}

// =============================================================================
// NOT-FOUND PATHS
// =============================================================================

/// Every read endpoint 404s on an unknown player.
#[tokio::test]
async fn test_unknown_player_is_not_found() {
    let app = TestApp::new().await;

    for uri in [
        format!("/v1/players/{UNKNOWN_PLAYER}/games"),
        format!("/v1/players/{UNKNOWN_PLAYER}/games/cs2"),
        format!("/v1/players/{UNKNOWN_PLAYER}/games/cs2/rating-history"),
        format!("/v1/players/{UNKNOWN_PLAYER}/games/cs2/mm-stats"),
        format!("/v1/players/{UNKNOWN_PLAYER}/games/cs2/match-history"),
    ] {
        let response = app.get(&uri).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "expected 404 from {uri}, body: {}",
            response.text()
        );
    }

    // The mutating endpoint 404s too (after the permission check passes).
    app.post_json(
        &format!("/v1/players/{UNKNOWN_PLAYER}/games/cs2/rating"),
        &json!({
            "rating": 15_000,
            "source": "manual",
            "recorded_at": Utc::now().to_rfc3339(),
        }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);
}

/// Every game-scoped endpoint 404s on an unknown game slug.
#[tokio::test]
async fn test_unknown_game_is_not_found() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();

    for uri in [
        format!("/v1/players/{player_id}/games/not-a-game"),
        format!("/v1/players/{player_id}/games/not-a-game/rating-history"),
        format!("/v1/players/{player_id}/games/not-a-game/mm-stats"),
        format!("/v1/players/{player_id}/games/not-a-game/match-history"),
    ] {
        let response = app.get(&uri).await;
        assert_eq!(
            response.status,
            StatusCode::NOT_FOUND,
            "expected 404 from {uri}, body: {}",
            response.text()
        );
    }

    app.post_json(
        &format!("/v1/players/{player_id}/games/not-a-game/rating"),
        &json!({
            "rating": 15_000,
            "source": "manual",
            "recorded_at": Utc::now().to_rfc3339(),
        }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);
}

/// A known player with no profile for a known game is a 404, not an empty
/// profile.
#[tokio::test]
async fn test_missing_profile_for_known_game_is_not_found() {
    let app = TestApp::new().await;
    let player_id = dev_player_id();

    app.get(&format!("/v1/players/{player_id}/games/cs2"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // The list variant is an empty collection rather than a 404.
    let response = app.get(&format!("/v1/players/{player_id}/games")).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert!(body["data"].as_array().unwrap().is_empty());
}
