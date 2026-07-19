//! Awards + stat leaderboards integration tests.
//!
//! Exercises the full pipeline: demo stats submission → plugin fact
//! extraction (EAV) → auto-linking into a tournament scope → leaderboard
//! aggregation → award authoring/standings/finalization → trophy case.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

// =============================================================================
// HELPERS
// =============================================================================

/// Per-player demo stats used to build a submission body.
struct PlayerDemoStats {
    steam_id: &'static str,
    kills: i64,
    headshot_kills: i64,
    mag7_kills: i64,
    blind_kills: i64,
}

const fn player_stats(
    steam_id: &'static str,
    kills: i64,
    headshot_kills: i64,
    mag7_kills: i64,
    blind_kills: i64,
) -> PlayerDemoStats {
    PlayerDemoStats {
        steam_id,
        kills,
        headshot_kills,
        mag7_kills,
        blind_kills,
    }
}

/// A minimal round in `Cs2DemoStats` shape.
fn minimal_round(n: i64) -> Value {
    json!({
        "round_number": n,
        "winner_team": "TeamA",
        "winner_side": "T",
        "round_score": {},
        "player_states": {},
        "events": [],
        "player_stats": {}
    })
}

/// Build a full `Cs2DemoStats`-shaped stats submission body so the plugin
/// fact extractor parses `raw_stats` and auto-linking sees the players.
fn awards_stats_body(players: &[PlayerDemoStats], match_date: &str, demo_file: &str) -> Value {
    let mut player_summaries = serde_json::Map::new();
    let mut player_inputs = Vec::new();
    for (i, p) in players.iter().enumerate() {
        let mut weapon_kills = serde_json::Map::new();
        if p.mag7_kills > 0 {
            weapon_kills.insert("mag7".to_string(), json!(p.mag7_kills));
        }
        player_summaries.insert(
            p.steam_id.to_string(),
            json!({
                "player_id": p.steam_id.parse::<u64>().unwrap(),
                "player_name": format!("Player{}", i + 1),
                "team": { "team_id": 2, "team_name": "TeamA", "team_side": "T" },
                "kills": p.kills,
                "deaths": 5,
                "assists": 2,
                "headshot_kills": p.headshot_kills,
                "damage_dealt": 800,
                "adr": 80.0,
                "hs_percentage": 40.0,
                "blind_kills": p.blind_kills,
                "weapon_kills": weapon_kills
            }),
        );
        player_inputs.push(json!({
            "steam_id": p.steam_id,
            "player_name": format!("Player{}", i + 1),
            "team_name": "TeamA",
            "stats": { "kills": p.kills, "deaths": 5 }
        }));
    }

    json!({
        "map_name": "de_dust2",
        "match_date": match_date,
        "team1_name": "TeamA",
        "team2_name": "TeamB",
        "team1_score": 13,
        "team2_score": 7,
        "total_rounds": 20,
        "raw_stats": {
            "map": "de_dust2",
            "match_date": match_date,
            "demo_file": demo_file,
            "match_id": demo_file,
            "teams": {
                "TeamA": { "team_id": 2, "team_name": "TeamA", "team_side": "T" },
                "TeamB": { "team_id": 3, "team_name": "TeamB", "team_side": "CT" }
            },
            "final_score": { "TeamA": 13, "TeamB": 7 },
            "rounds": [minimal_round(1), minimal_round(2)],
            "player_summaries": player_summaries
        },
        "players": player_inputs
    })
}

/// Set a player's `steam_id_64` from a tournament registration ID.
async fn set_registration_steam_id(app: &TestApp, registration_id: &str, steam_id: &str) {
    let reg_uuid = Uuid::parse_str(registration_id).unwrap();
    sqlx::query(
        "UPDATE players SET steam_id_64 = $1
         WHERE id = (SELECT player_id FROM tournament_registrations WHERE id = $2)",
    )
    .bind(steam_id.parse::<i64>().unwrap())
    .bind(reg_uuid)
    .execute(app.pool())
    .await
    .expect("failed to set steam_id_64");
}

/// Look up the player behind a tournament registration.
async fn registration_player_id(app: &TestApp, registration_id: &str) -> Uuid {
    let reg_uuid = Uuid::parse_str(registration_id).unwrap();
    sqlx::query_scalar::<_, Uuid>("SELECT player_id FROM tournament_registrations WHERE id = $1")
        .bind(reg_uuid)
        .fetch_one(app.pool())
        .await
        .expect("failed to resolve registration player")
}

/// Catalog a demo and return its id.
async fn catalog_demo(app: &TestApp, s3_key: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await;
    let response = app
        .post_json(
            "/v1/admin/demos",
            &json!({
                "game_id": game_id.to_string(),
                "file_name": s3_key.rsplit('/').next().unwrap(),
                "s3_bucket": "test-bucket",
                "s3_key": s3_key,
                "file_size_bytes": 42_000_000
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// A tournament with a scheduled match and two steam-identified players,
/// ready to receive auto-linked demo facts.
struct AwardsFixture {
    tournament_id: String,
    #[allow(dead_code)]
    match_id: String,
    p1_player_id: Uuid,
    p2_player_id: Uuid,
    /// The match's scheduled time; demo `match_date`s near it auto-link.
    match_time: String,
}

const P1_STEAM: &str = "76561198000010001";
const P2_STEAM: &str = "76561198000010002";

/// Create a started tournament with a scheduled match, wire both players'
/// steam ids, and return the scope handles.
async fn setup_awards_fixture(app: &TestApp, slug: &str) -> AwardsFixture {
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(app, slug).await;

    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    set_registration_steam_id(app, &reg1, P1_STEAM).await;
    set_registration_steam_id(app, &reg2, P2_STEAM).await;

    AwardsFixture {
        p1_player_id: registration_player_id(app, &reg1).await,
        p2_player_id: registration_player_id(app, &reg2).await,
        tournament_id,
        match_id,
        match_time: t.to_rfc3339(),
    }
}

/// Catalog + submit a demo whose stats auto-link into the fixture's
/// tournament, asserting the link actually happened (facts land in scope).
async fn submit_linked_demo(app: &TestApp, fixture: &AwardsFixture, s3_key: &str, body: &Value) {
    let demo_id = catalog_demo(app, s3_key).await;
    let response = app
        .post_json(&format!("/v1/admin/demos/{demo_id}/stats"), body)
        .await;
    response.assert_status(StatusCode::OK);
    let demo: Value = response.json();
    assert_eq!(
        demo["data"]["tournament_id"], fixture.tournament_id,
        "demo should auto-link into the tournament scope"
    );
}

/// Fetch a leaderboard and return its entries.
async fn get_leaderboard(app: &TestApp, tournament_id: &str, query: &str) -> Vec<Value> {
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/leaderboards?{query}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().unwrap().clone()
}

// =============================================================================
// LEADERBOARDS
// =============================================================================

/// Facts extracted on stats submission produce correct leaderboards; a
/// second demo accumulates under `sum` and `max_single_demo` picks the max.
#[tokio::test]
async fn test_leaderboard_ranks_and_accumulates() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-leaderboard").await;

    submit_linked_demo(
        &app,
        &fixture,
        "demos/awards_lb_1.dem",
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 20, 9, 3, 2),
                player_stats(P2_STEAM, 10, 4, 1, 0),
            ],
            &fixture.match_time,
            "awards_lb_1.dem",
        ),
    )
    .await;

    // Headshot leaderboard: P1 (9) over P2 (4), one demo counted each.
    let entries = get_leaderboard(
        &app,
        &fixture.tournament_id,
        "stat_key=headshot_kills&aggregation=sum&direction=desc",
    )
    .await;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["player_id"], fixture.p1_player_id.to_string());
    assert_eq!(entries[0]["value"], 9.0);
    assert_eq!(entries[0]["rank"], 1);
    assert_eq!(entries[0]["demos_counted"], 1);
    assert_eq!(entries[1]["player_id"], fixture.p2_player_id.to_string());
    assert_eq!(entries[1]["value"], 4.0);
    assert_eq!(entries[1]["rank"], 2);

    // Open-set weapon key: MAG-7 kills.
    let entries = get_leaderboard(&app, &fixture.tournament_id, "stat_key=kills.weapon.mag7").await;
    assert_eq!(entries[0]["player_id"], fixture.p1_player_id.to_string());
    assert_eq!(entries[0]["value"], 3.0);
    assert_eq!(entries[1]["value"], 1.0);

    // Second demo accumulates: P1 +5 hs / +2 mag7, P2 +6 hs / +5 mag7.
    submit_linked_demo(
        &app,
        &fixture,
        "demos/awards_lb_2.dem",
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 12, 5, 2, 1),
                player_stats(P2_STEAM, 15, 6, 5, 0),
            ],
            &fixture.match_time,
            "awards_lb_2.dem",
        ),
    )
    .await;

    // Sum: P1 14 hs vs P2 10 hs, both across 2 demos.
    let entries = get_leaderboard(
        &app,
        &fixture.tournament_id,
        "stat_key=headshot_kills&aggregation=sum",
    )
    .await;
    assert_eq!(entries[0]["value"], 14.0);
    assert_eq!(entries[0]["demos_counted"], 2);
    assert_eq!(entries[1]["value"], 10.0);
    assert_eq!(entries[1]["demos_counted"], 2);

    // MAG-7 sum flips the order: P2 6 over P1 5.
    let entries = get_leaderboard(
        &app,
        &fixture.tournament_id,
        "stat_key=kills.weapon.mag7&aggregation=sum",
    )
    .await;
    assert_eq!(entries[0]["player_id"], fixture.p2_player_id.to_string());
    assert_eq!(entries[0]["value"], 6.0);
    assert_eq!(entries[1]["value"], 5.0);

    // max_single_demo picks each player's best single demo.
    let entries = get_leaderboard(
        &app,
        &fixture.tournament_id,
        "stat_key=headshot_kills&aggregation=max_single_demo",
    )
    .await;
    assert_eq!(entries[0]["value"], 9.0);
    assert_eq!(entries[1]["value"], 6.0);
}

// =============================================================================
// AWARD AUTHORING
// =============================================================================

/// Instantiating the seeded `swag7` template carries its branding, and the
/// award's standings match the plain MAG-7 leaderboard.
#[tokio::test]
async fn test_award_from_template_swag7() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-template").await;

    submit_linked_demo(
        &app,
        &fixture,
        "demos/awards_tpl.dem",
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 20, 9, 3, 2),
                player_stats(P2_STEAM, 10, 4, 1, 0),
            ],
            &fixture.match_time,
            "awards_tpl.dem",
        ),
    )
    .await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/awards", fixture.tournament_id),
            &json!({ "template_key": "swag7" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let award = &body["data"];
    assert_eq!(award["name"], "Swag 7");
    assert_eq!(award["stat_key"], "kills.weapon.mag7");
    assert_eq!(award["icon"], "mdi-spray");
    assert_eq!(award["status"], "active");
    assert!(award["template_id"].is_string());
    let award_id = award["id"].as_str().unwrap();

    // Standings mirror the plain leaderboard for the same metric.
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/awards/{award_id}/standings",
            fixture.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["player_id"], fixture.p1_player_id.to_string());
    assert_eq!(entries[0]["value"], 3.0);
    assert_eq!(entries[0]["rank"], 1);
    assert_eq!(entries[1]["value"], 1.0);

    let leaderboard = get_leaderboard(
        &app,
        &fixture.tournament_id,
        "stat_key=kills.weapon.mag7&aggregation=sum",
    )
    .await;
    assert_eq!(entries, &leaderboard);
}

/// Custom awards: creation from the open stat catalog, rename via PATCH,
/// catalog validation, and organizer-scoped RBAC (outsiders get 403).
#[tokio::test]
async fn test_custom_award_lifecycle_and_rbac() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-custom").await;
    let awards_url = format!("/v1/tournaments/{}/awards", fixture.tournament_id);

    // Custom award over a catalog stat.
    let response = app
        .post_json(
            &awards_url,
            &json!({ "name": "Test Blind", "stat_key": "kills.while_blind" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["data"]["name"], "Test Blind");
    assert_eq!(body["data"]["aggregation"], "sum");
    let award_id = body["data"]["id"].as_str().unwrap().to_string();

    // Rename works while active.
    let response = app
        .patch_json(
            &format!("{awards_url}/{award_id}"),
            &json!({ "name": "Blind Monk Jr" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["name"], "Blind Monk Jr");

    // Unknown stat keys are rejected against the plugin catalog.
    let response = app
        .post_json(
            &awards_url,
            &json!({ "name": "Bogus", "stat_key": "not_a_real_stat" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // ... but the open weapon set is allowed without enumeration.
    let response = app
        .post_json(
            &awards_url,
            &json!({ "name": "Zeus Lord", "stat_key": "kills.weapon.taser" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // An unrelated user without any role gets 403 on every mutation.
    let outsider = UserBuilder::new()
        .username("awards-outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token =
        create_test_token(outsider.id, outsider.id, "awards-outsider", TEST_JWT_SECRET);

    let response = app
        .post_json_with_token(
            &awards_url,
            &json!({ "name": "Sneaky", "stat_key": "kills" }),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .patch_json_with_token(
            &format!("{awards_url}/{award_id}"),
            &json!({ "name": "Hijacked" }),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .post_json_with_token(
            &format!("{awards_url}/{award_id}/finalize"),
            &json!({}),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Reads stay public: the outsider can list awards and view standings.
    let response = app.get_with_token(&awards_url, &outsider_token).await;
    response.assert_status(StatusCode::OK);
}

/// Duplicate award names within one scope map the unique constraint to 409.
#[tokio::test]
async fn test_duplicate_award_name_conflict() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-dup").await;
    let awards_url = format!("/v1/tournaments/{}/awards", fixture.tournament_id);

    let body = json!({ "name": "One Of A Kind", "stat_key": "kills" });
    app.post_json(&awards_url, &body)
        .await
        .assert_status(StatusCode::CREATED);
    app.post_json(&awards_url, &body)
        .await
        .assert_status(StatusCode::CONFLICT);
}

// =============================================================================
// FINALIZATION + TROPHY CASE
// =============================================================================

/// Finalize snapshots the podium with shared ranks on ties, flips status,
/// and the winners' trophy cases show the award with tournament context.
#[tokio::test]
async fn test_finalize_tie_and_trophy_case() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-finalize").await;

    // Both players land 3 MAG-7 kills: a rank-1 tie.
    submit_linked_demo(
        &app,
        &fixture,
        "demos/awards_tie.dem",
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 20, 9, 3, 2),
                player_stats(P2_STEAM, 10, 4, 3, 0),
            ],
            &fixture.match_time,
            "awards_tie.dem",
        ),
    )
    .await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/awards", fixture.tournament_id),
            &json!({ "template_key": "swag7" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let award_id = body["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/awards/{award_id}/finalize",
            fixture.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["award"]["status"], "finalized");
    let results = body["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "tied players both make the podium");
    assert!(results.iter().all(|r| r["rank"] == 1), "ties share rank 1");
    assert!(results.iter().all(|r| r["value"] == 3.0));

    // Both winners' trophy cases show the award with tournament context.
    for player_id in [fixture.p1_player_id, fixture.p2_player_id] {
        let response = app.get(&format!("/v1/players/{player_id}/awards")).await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        let trophies = body["data"].as_array().unwrap();
        assert_eq!(trophies.len(), 1);
        assert_eq!(trophies[0]["award"]["name"], "Swag 7");
        assert_eq!(trophies[0]["result"]["rank"], 1);
        assert!(
            trophies[0]["scope_name"]
                .as_str()
                .unwrap()
                .contains("awards-finalize"),
            "trophy carries the tournament name"
        );
    }

    // Voiding a finalized award is refused — trophies are permanent.
    let response = app
        .delete_auth(&format!(
            "/v1/tournaments/{}/awards/{award_id}",
            fixture.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

/// Completing a tournament auto-finalizes its remaining active awards.
#[tokio::test]
async fn test_complete_tournament_auto_finalizes_awards() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "awards-complete").await;

    submit_linked_demo(
        &app,
        &fixture,
        "demos/awards_complete.dem",
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 20, 9, 0, 0),
                player_stats(P2_STEAM, 10, 4, 0, 0),
            ],
            &fixture.match_time,
            "awards_complete.dem",
        ),
    )
    .await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/awards", fixture.tournament_id),
            &json!({ "name": "Headshot Hero", "stat_key": "headshot_kills" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // Drive completion through the lifecycle endpoint.
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/complete",
            fixture.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    // The award is now finalized without a manual trigger.
    let response = app
        .get(&format!("/v1/tournaments/{}/awards", fixture.tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let awards = body["data"].as_array().unwrap();
    assert_eq!(awards.len(), 1);
    assert_eq!(awards[0]["status"], "finalized");

    // The winner's trophy case reflects it.
    let response = app
        .get(&format!("/v1/players/{}/awards", fixture.p1_player_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let trophies = body["data"].as_array().unwrap();
    assert_eq!(trophies.len(), 1);
    assert_eq!(trophies[0]["award"]["name"], "Headshot Hero");
    assert_eq!(trophies[0]["result"]["rank"], 1);
    assert_eq!(trophies[0]["result"]["value"], 9.0);
}
