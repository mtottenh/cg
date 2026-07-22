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
    setup_awards_fixture_with_steam(app, slug, P1_STEAM, P2_STEAM).await
}

/// Like [`setup_awards_fixture`], but the two participants' steam ids are
/// caller-chosen (steam ids are globally unique, so a second tournament in
/// the same season needs its own pair).
async fn setup_awards_fixture_with_steam(
    app: &TestApp,
    slug: &str,
    steam1: &str,
    steam2: &str,
) -> AwardsFixture {
    let (tournament_id, match_id, reg1, reg2, _token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(app, slug).await;

    let t = chrono::Utc::now() + chrono::Duration::hours(1);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": t.to_rfc3339() }),
    )
    .await
    .assert_status(StatusCode::OK);

    set_registration_steam_id(app, &reg1, steam1).await;
    set_registration_steam_id(app, &reg2, steam2).await;

    AwardsFixture {
        p1_player_id: registration_player_id(app, &reg1).await,
        p2_player_id: registration_player_id(app, &reg2).await,
        tournament_id,
        match_id,
        match_time: t.to_rfc3339(),
    }
}

/// Stamp a tournament with a season id so it falls inside the season scope.
async fn set_tournament_season(app: &TestApp, tournament_id: &str, season_id: Uuid) {
    sqlx::query("UPDATE tournaments SET season_id = $1 WHERE id = $2")
        .bind(season_id)
        .bind(Uuid::parse_str(tournament_id).unwrap())
        .execute(app.pool())
        .await
        .expect("failed to set tournament season_id");
}

/// Fetch the combined player-stats leaderboard for a league season.
async fn get_season_stats_leaderboard(app: &TestApp, season_id: Uuid, query: &str) -> Vec<Value> {
    let response = app
        .get(&format!(
            "/v1/league-seasons/{season_id}/stats-leaderboard?{query}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().unwrap().clone()
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

/// Full per-player core stats for the combined player-stats leaderboard,
/// where deaths/assists/damage/rounds are all controllable (unlike the
/// single-metric helper, which pins them). `rounds` drives the demo's round
/// count, so ADR (`damage / rounds`) can vary between players.
struct PlayerCoreStats {
    steam_id: &'static str,
    kills: i64,
    deaths: i64,
    assists: i64,
    damage: i64,
}

const fn core_stats(
    steam_id: &'static str,
    kills: i64,
    deaths: i64,
    assists: i64,
    damage: i64,
) -> PlayerCoreStats {
    PlayerCoreStats {
        steam_id,
        kills,
        deaths,
        assists,
        damage,
    }
}

/// Build a `Cs2DemoStats`-shaped body with fully controllable per-player core
/// stats and `rounds` rounds (so `rounds_played == rounds`).
fn core_stats_body(
    players: &[PlayerCoreStats],
    rounds: i64,
    match_date: &str,
    demo_file: &str,
) -> Value {
    let mut player_summaries = serde_json::Map::new();
    let mut player_inputs = Vec::new();
    for (i, p) in players.iter().enumerate() {
        player_summaries.insert(
            p.steam_id.to_string(),
            json!({
                "player_id": p.steam_id.parse::<u64>().unwrap(),
                "player_name": format!("Player{}", i + 1),
                "team": { "team_id": 2, "team_name": "TeamA", "team_side": "T" },
                "kills": p.kills,
                "deaths": p.deaths,
                "assists": p.assists,
                "headshot_kills": 0,
                "damage_dealt": p.damage,
                "adr": 0.0,
                "hs_percentage": 0.0,
                "blind_kills": 0,
                "weapon_kills": {}
            }),
        );
        player_inputs.push(json!({
            "steam_id": p.steam_id,
            "player_name": format!("Player{}", i + 1),
            "team_name": "TeamA",
            "stats": { "kills": p.kills, "deaths": p.deaths }
        }));
    }

    let round_values: Vec<Value> = (1..=rounds).map(minimal_round).collect();

    json!({
        "map_name": "de_dust2",
        "match_date": match_date,
        "team1_name": "TeamA",
        "team2_name": "TeamB",
        "team1_score": 13,
        "team2_score": 7,
        "total_rounds": rounds,
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
            "rounds": round_values,
            "player_summaries": player_summaries
        },
        "players": player_inputs
    })
}

/// Fetch the combined player-stats leaderboard for a tournament.
async fn get_tournament_stats_leaderboard(
    app: &TestApp,
    tournament_id: &str,
    query: &str,
) -> Vec<Value> {
    let response = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/stats-leaderboard?{query}"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    body["data"].as_array().unwrap().clone()
}

/// Locate a player's row in a player-stats leaderboard payload.
fn find_row(rows: &[Value], player_id: Uuid) -> &Value {
    rows.iter()
        .find(|r| r["player_id"] == player_id.to_string())
        .expect("player row present in stats leaderboard")
}

/// f64 field accessor with a tolerant comparison for ADR assertions.
fn approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
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
// COMBINED PLAYER-STATS LEADERBOARD
// =============================================================================

/// A steam id belonging to no player row: its facts never resolve, so it must
/// never appear in a leaderboard.
const UNKNOWN_STEAM: &str = "76561198000099999";
/// The second tournament's *other* participant (its own globally-unique
/// steam id; the shared player keeps `P1_STEAM`).
const P4_STEAM: &str = "76561198000010004";

/// The combined leaderboard sums each core stat into its own column and
/// derives a rounds-weighted ADR; unresolved players are excluded.
#[tokio::test]
async fn test_tournament_stats_leaderboard() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "stats-lb-tournament").await;

    // 10 rounds; P1 out-damages P2. A third, unregistered steam id rides
    // along (keeps auto-link confidence at 2/3) but resolves to no player.
    submit_linked_demo(
        &app,
        &fixture,
        "demos/stats_lb_1.dem",
        &core_stats_body(
            &[
                core_stats(P1_STEAM, 20, 8, 5, 1500),
                core_stats(P2_STEAM, 10, 12, 3, 900),
                core_stats(UNKNOWN_STEAM, 99, 99, 99, 9900),
            ],
            10,
            &fixture.match_time,
            "stats_lb_1.dem",
        ),
    )
    .await;

    let rows = get_tournament_stats_leaderboard(&app, &fixture.tournament_id, "").await;
    // The unresolved steam id contributes no row.
    assert_eq!(rows.len(), 2, "only resolved players rank");
    assert!(
        !rows.iter().any(|r| r["kills"] == 99.0),
        "unresolved player excluded"
    );

    let p1 = find_row(&rows, fixture.p1_player_id);
    assert_eq!(p1["kills"], 20.0);
    assert_eq!(p1["deaths"], 8.0);
    assert_eq!(p1["assists"], 5.0);
    assert_eq!(p1["total_damage"], 1500.0);
    assert_eq!(p1["rounds_played"], 10.0);
    assert_eq!(p1["demos_counted"], 1);
    // ADR is rounds-weighted: total_damage / rounds_played.
    approx(p1["adr"].as_f64().unwrap(), 1500.0 / 10.0);

    let p2 = find_row(&rows, fixture.p2_player_id);
    assert_eq!(p2["kills"], 10.0);
    assert_eq!(p2["deaths"], 12.0);
    assert_eq!(p2["assists"], 3.0);
    assert_eq!(p2["total_damage"], 900.0);
    approx(p2["adr"].as_f64().unwrap(), 900.0 / 10.0);

    // Default sort is kills desc: P1 (20) leads P2 (10).
    assert_eq!(rows[0]["player_id"], fixture.p1_player_id.to_string());
}

/// The season leaderboard sums one player's stats across every tournament in
/// the season, and ADR is weighted over the combined damage and rounds.
#[tokio::test]
async fn test_season_stats_leaderboard_sums_across_tournaments() {
    let app = TestApp::new().await;
    // A league-creation trigger already seeds a `season-1`, so give this one
    // its own slug.
    let season = LeagueSeasonBuilder::new()
        .name("Stats LB Season")
        .slug("stats-lb-season")
        .build_persisted(app.pool())
        .await;

    // Tournament A: P1 + P2. One demo, 10 rounds.
    let fixture_a = setup_awards_fixture(&app, "stats-lb-season-a").await;
    set_tournament_season(&app, &fixture_a.tournament_id, season.id).await;
    submit_linked_demo(
        &app,
        &fixture_a,
        "demos/stats_season_a.dem",
        &core_stats_body(
            &[
                core_stats(P1_STEAM, 20, 8, 5, 1500),
                core_stats(P2_STEAM, 10, 12, 3, 900),
            ],
            10,
            &fixture_a.match_time,
            "stats_season_a.dem",
        ),
    )
    .await;

    // Tournament B shares one participant with A: the authenticated dev user
    // (`Player1`, P1_STEAM) registers in every tournament, so it is the same
    // players row in both. Its other participant is distinct (P4_STEAM), so
    // B's demo links only to B (its steam set {P1,P4} overlaps A's {P1,P2} at
    // just 0.5, below the auto-link threshold).
    let fixture_b =
        setup_awards_fixture_with_steam(&app, "stats-lb-season-b", P1_STEAM, P4_STEAM).await;
    assert_eq!(
        fixture_a.p1_player_id, fixture_b.p1_player_id,
        "the dev user is the shared player across both tournaments"
    );
    set_tournament_season(&app, &fixture_b.tournament_id, season.id).await;
    submit_linked_demo(
        &app,
        &fixture_b,
        "demos/stats_season_b.dem",
        &core_stats_body(
            &[
                core_stats(P1_STEAM, 7, 4, 2, 600),
                core_stats(P4_STEAM, 9, 7, 2, 650),
            ],
            6,
            &fixture_b.match_time,
            "stats_season_b.dem",
        ),
    )
    .await;

    let rows = get_season_stats_leaderboard(&app, season.id, "").await;
    let p1 = find_row(&rows, fixture_a.p1_player_id);
    // Summed across BOTH tournaments' demos.
    assert_eq!(p1["kills"], 27.0, "20 + 7");
    assert_eq!(p1["deaths"], 12.0, "8 + 4");
    assert_eq!(p1["assists"], 7.0, "5 + 2");
    assert_eq!(p1["total_damage"], 2100.0, "1500 + 600");
    assert_eq!(p1["rounds_played"], 16.0, "10 + 6");
    assert_eq!(p1["demos_counted"], 2);
    // Rounds-weighted: (1500 + 600) / (10 + 6), not the mean of 150 and 100.
    approx(p1["adr"].as_f64().unwrap(), 2100.0 / 16.0);
}

/// `sort=adr` orders by the derived ADR; an unknown sort is a 400.
#[tokio::test]
async fn test_stats_leaderboard_sort_and_validation() {
    let app = TestApp::new().await;
    let fixture = setup_awards_fixture(&app, "stats-lb-sort").await;

    // P1 has more kills; P2 has far higher damage (thus ADR) over 10 rounds.
    submit_linked_demo(
        &app,
        &fixture,
        "demos/stats_sort.dem",
        &core_stats_body(
            &[
                core_stats(P1_STEAM, 30, 5, 5, 500),
                core_stats(P2_STEAM, 5, 20, 1, 1500),
            ],
            10,
            &fixture.match_time,
            "stats_sort.dem",
        ),
    )
    .await;

    // Default (kills): P1 leads.
    let by_kills = get_tournament_stats_leaderboard(&app, &fixture.tournament_id, "").await;
    assert_eq!(by_kills[0]["player_id"], fixture.p1_player_id.to_string());

    // sort=adr flips the order: P2 (150) over P1 (50).
    let by_adr = get_tournament_stats_leaderboard(&app, &fixture.tournament_id, "sort=adr").await;
    assert_eq!(by_adr[0]["player_id"], fixture.p2_player_id.to_string());
    approx(by_adr[0]["adr"].as_f64().unwrap(), 150.0);

    // An unknown sort value is rejected.
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/stats-leaderboard?sort=bogus",
            fixture.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
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

// =============================================================================
// LEAGUE-SEASON AWARDS
// =============================================================================

/// A league season on the CS2 game, containing one tournament whose linked
/// demo facts fall inside the season scope.
struct SeasonAwardsFixture {
    season_id: Uuid,
    /// Season name, echoed as `scope_name` on trophies.
    season_name: String,
    p1_player_id: Uuid,
    p2_player_id: Uuid,
}

impl SeasonAwardsFixture {
    fn awards_url(&self) -> String {
        format!("/v1/league-seasons/{}/awards", self.season_id)
    }
}

/// Create a league season whose league is on CS2 — award templates and the
/// stat catalog are resolved through the league's game plugin, so a builder
/// default (`test-plugin`) would have neither.
async fn create_cs2_season(app: &TestApp, slug: &str) -> (Uuid, String) {
    let game_id = get_game_id(app.pool(), "cs2").await;
    let league = LeagueBuilder::new()
        .game_id(game_id)
        .name(format!("League {slug}"))
        .slug(format!("league-{slug}"))
        .build_persisted(app.pool())
        .await;
    let season_name = format!("Season {slug}");
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(season_name.clone())
        .slug(format!("season-{slug}"))
        .build_persisted(app.pool())
        .await;
    (season.id, season_name)
}

/// Build a CS2 league + season, run a tournament inside it, and submit a
/// demo so the season scope has facts to rank. `p1_mag7`/`p2_mag7` drive the
/// `swag7` metric.
async fn setup_season_awards_fixture(
    app: &TestApp,
    slug: &str,
    p1_mag7: i64,
    p2_mag7: i64,
) -> SeasonAwardsFixture {
    let (season_id, season_name) = create_cs2_season(app, slug).await;

    let fixture = setup_awards_fixture(app, slug).await;
    set_tournament_season(app, &fixture.tournament_id, season_id).await;
    submit_linked_demo(
        app,
        &fixture,
        &format!("demos/{slug}.dem"),
        &awards_stats_body(
            &[
                player_stats(P1_STEAM, 20, 9, p1_mag7, 2),
                player_stats(P2_STEAM, 10, 4, p2_mag7, 0),
            ],
            &fixture.match_time,
            &format!("{slug}.dem"),
        ),
    )
    .await;

    SeasonAwardsFixture {
        season_id,
        season_name,
        p1_player_id: fixture.p1_player_id,
        p2_player_id: fixture.p2_player_id,
    }
}

/// Instantiating the seeded `swag7` template in a season carries its
/// branding, lists under the season, and its standings match the plain
/// season leaderboard for the same metric.
#[tokio::test]
async fn test_season_award_from_template_and_standings() {
    let app = TestApp::new().await;
    let fixture = setup_season_awards_fixture(&app, "season-awards-template", 3, 1).await;

    let response = app
        .post_json(&fixture.awards_url(), &json!({ "template_key": "swag7" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let award = &body["data"];
    assert_eq!(award["name"], "Swag 7");
    assert_eq!(award["stat_key"], "kills.weapon.mag7");
    assert_eq!(award["icon"], "mdi-spray");
    assert_eq!(award["status"], "active");
    let award_id = award["id"].as_str().unwrap().to_string();

    // The award lists under its season.
    let response = app.get(&fixture.awards_url()).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let awards = body["data"].as_array().unwrap();
    assert_eq!(awards.len(), 1);
    assert_eq!(awards[0]["id"], award_id);

    // Standings rank the season's linked demo facts.
    let response = app
        .get(&format!("{}/{award_id}/standings", fixture.awards_url()))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let entries = body["data"]["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["player_id"], fixture.p1_player_id.to_string());
    assert_eq!(entries[0]["value"], 3.0);
    assert_eq!(entries[0]["rank"], 1);
    assert_eq!(entries[1]["value"], 1.0);

    // They mirror the plain season leaderboard for the same metric.
    let response = app
        .get(&format!(
            "/v1/league-seasons/{}/leaderboards?stat_key=kills.weapon.mag7&aggregation=sum",
            fixture.season_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"].as_array().unwrap(), &entries);
}

/// Season-award authoring: custom awards from the catalog, rename via PATCH,
/// catalog validation, duplicate names, and voiding.
#[tokio::test]
async fn test_season_custom_award_lifecycle() {
    let app = TestApp::new().await;
    let fixture = setup_season_awards_fixture(&app, "season-awards-custom", 3, 1).await;
    let awards_url = fixture.awards_url();

    // Custom award over a catalog stat.
    let response = app
        .post_json(
            &awards_url,
            &json!({ "name": "Season Blind", "stat_key": "kills.while_blind" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    assert_eq!(body["data"]["name"], "Season Blind");
    assert_eq!(body["data"]["aggregation"], "sum");
    let award_id = body["data"]["id"].as_str().unwrap().to_string();

    // Rename works while active.
    let response = app
        .patch_json(
            &format!("{awards_url}/{award_id}"),
            &json!({ "name": "Season Blind Monk" }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["name"], "Season Blind Monk");

    // Unknown stat keys are rejected against the league game's catalog.
    app.post_json(
        &awards_url,
        &json!({ "name": "Bogus", "stat_key": "not_a_real_stat" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // ... but the open weapon set needs no enumeration.
    app.post_json(
        &awards_url,
        &json!({ "name": "Season Zeus", "stat_key": "kills.weapon.taser" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    // Duplicate names within the season scope map the unique constraint to 409.
    app.post_json(
        &awards_url,
        &json!({ "name": "Season Zeus", "stat_key": "kills.weapon.taser" }),
    )
    .await
    .assert_status(StatusCode::CONFLICT);

    // Voiding an active award is a soft delete: the row survives with a
    // `void` status (the list is deliberately unfiltered).
    let response = app.delete_auth(&format!("{awards_url}/{award_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["status"], "void");

    let response = app.get(&awards_url).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let voided = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == award_id.as_str())
        .expect("voided award still listed");
    assert_eq!(voided["status"], "void");

    // Voiding is idempotent.
    let response = app.delete_auth(&format!("{awards_url}/{award_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["status"], "void");

    // A voided award can no longer be edited.
    app.patch_json(
        &format!("{awards_url}/{award_id}"),
        &json!({ "name": "Resurrected" }),
    )
    .await
    .assert_status(StatusCode::CONFLICT);
}

/// Season-award mutations are gated on `league.seasons.manage`: an unrelated
/// user gets 403 on create/update/void/finalize while reads stay public.
#[tokio::test]
async fn test_season_award_rbac() {
    let app = TestApp::new().await;
    let fixture = setup_season_awards_fixture(&app, "season-awards-rbac", 3, 1).await;
    let awards_url = fixture.awards_url();

    let response = app
        .post_json(&awards_url, &json!({ "template_key": "swag7" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let award_id = body["data"]["id"].as_str().unwrap().to_string();

    let outsider = UserBuilder::new()
        .username("season-awards-outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = create_test_token(
        outsider.id,
        outsider.id,
        "season-awards-outsider",
        TEST_JWT_SECRET,
    );

    app.post_json_with_token(
        &awards_url,
        &json!({ "name": "Sneaky", "stat_key": "kills" }),
        &outsider_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    app.patch_json_with_token(
        &format!("{awards_url}/{award_id}"),
        &json!({ "name": "Hijacked" }),
        &outsider_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    app.delete_with_token(&format!("{awards_url}/{award_id}"), &outsider_token)
        .await
        .assert_status(StatusCode::FORBIDDEN);

    app.post_json_with_token(
        &format!("{awards_url}/{award_id}/finalize"),
        &json!({}),
        &outsider_token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);

    // Anonymous mutations are unauthenticated, not merely forbidden.
    app.post_json_no_auth(&awards_url, &json!({ "template_key": "swag7" }))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Reads stay public.
    app.get_with_token(&awards_url, &outsider_token)
        .await
        .assert_status(StatusCode::OK);
    app.get(&format!("{awards_url}/{award_id}/standings"))
        .await
        .assert_status(StatusCode::OK);

    // The award is untouched by the rejected mutations.
    let response = app.get(&format!("{awards_url}/{award_id}/standings")).await;
    let body: Value = response.json();
    assert_eq!(body["data"]["award"]["name"], "Swag 7");
    assert_eq!(body["data"]["award"]["status"], "active");
}

/// Finalizing a season award snapshots the podium (sharing rank on ties) and
/// the winners' trophy cases carry the season name as scope. Seasons have no
/// completion lock, so a manager can re-finalize to recompute.
#[tokio::test]
async fn test_season_award_finalize_and_trophy_case() {
    let app = TestApp::new().await;
    // Both players land 3 MAG-7 kills: a rank-1 tie.
    let fixture = setup_season_awards_fixture(&app, "season-awards-finalize", 3, 3).await;
    let awards_url = fixture.awards_url();

    let response = app
        .post_json(&awards_url, &json!({ "template_key": "swag7" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let award_id = body["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_auth(&format!("{awards_url}/{award_id}/finalize"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    assert_eq!(body["data"]["award"]["status"], "finalized");
    let results = body["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "tied players both make the podium");
    assert!(results.iter().all(|r| r["rank"] == 1), "ties share rank 1");
    assert!(results.iter().all(|r| r["value"] == 3.0));

    // Both winners' trophy cases carry the season as scope.
    for player_id in [fixture.p1_player_id, fixture.p2_player_id] {
        let response = app.get(&format!("/v1/players/{player_id}/awards")).await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();
        let trophies = body["data"].as_array().unwrap();
        assert_eq!(trophies.len(), 1);
        assert_eq!(trophies[0]["award"]["name"], "Swag 7");
        assert_eq!(trophies[0]["result"]["rank"], 1);
        assert_eq!(trophies[0]["scope_name"], fixture.season_name);
    }

    // Renaming a finalized award is refused.
    app.patch_json(
        &format!("{awards_url}/{award_id}"),
        &json!({ "name": "Too Late" }),
    )
    .await
    .assert_status(StatusCode::CONFLICT);

    // Voiding a finalized award is refused — trophies are permanent.
    app.delete_auth(&format!("{awards_url}/{award_id}"))
        .await
        .assert_status(StatusCode::CONFLICT);

    // Unlike tournaments, a season award can be re-finalized (recompute).
    app.post_auth(&format!("{awards_url}/{award_id}/finalize"))
        .await
        .assert_status(StatusCode::OK);
}

/// Season-scoped award endpoints 404 on an unknown season, and on an award
/// that belongs to a different scope.
#[tokio::test]
async fn test_season_award_scope_isolation() {
    let app = TestApp::new().await;
    let fixture = setup_season_awards_fixture(&app, "season-awards-scope", 3, 1).await;

    let unknown_season = Uuid::now_v7();
    app.get(&format!("/v1/league-seasons/{unknown_season}/awards"))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    app.post_json(
        &format!("/v1/league-seasons/{unknown_season}/awards"),
        &json!({ "template_key": "swag7" }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);

    // An award created in a second season is not addressable through the first.
    let (other_season_id, _) = create_cs2_season(&app, "season-awards-scope-b").await;
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{other_season_id}/awards"),
            &json!({ "template_key": "swag7" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let other_award_id = body["data"]["id"].as_str().unwrap().to_string();

    app.get(&format!(
        "{}/{other_award_id}/standings",
        fixture.awards_url()
    ))
    .await
    .assert_status(StatusCode::NOT_FOUND);
    app.patch_json(
        &format!("{}/{other_award_id}", fixture.awards_url()),
        &json!({ "name": "Cross Scope" }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND);
    app.delete_auth(&format!("{}/{other_award_id}", fixture.awards_url()))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    app.post_auth(&format!(
        "{}/{other_award_id}/finalize",
        fixture.awards_url()
    ))
    .await
    .assert_status(StatusCode::NOT_FOUND);
}

// =============================================================================
// STAT CATALOG + AWARD TEMPLATES
// =============================================================================

/// The game-scoped award-builder surfaces: the plugin's stat catalog and the
/// seeded award templates, both addressable by slug or UUID.
#[tokio::test]
async fn test_stat_catalog_and_award_templates() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await;

    let response = app.get("/v1/games/cs2/stat-catalog").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let catalog = body["data"].as_array().unwrap();
    assert!(!catalog.is_empty(), "CS2 exposes stat definitions");
    let keys: Vec<&str> = catalog.iter().filter_map(|d| d["key"].as_str()).collect();
    assert!(keys.contains(&"kills"), "catalog has kills, got {keys:?}");
    assert!(keys.contains(&"headshot_kills"));
    assert!(keys.contains(&"kills.while_blind"));

    // The UUID form resolves to the same catalog.
    let response = app.get(&format!("/v1/games/{game_id}/stat-catalog")).await;
    response.assert_status(StatusCode::OK);
    let by_uuid: Value = response.json();
    assert_eq!(by_uuid["data"].as_array().unwrap().len(), catalog.len());

    let response = app.get("/v1/games/cs2/award-templates").await;
    response.assert_status(StatusCode::OK);
    let body: Value = response.json();
    let templates = body["data"].as_array().unwrap();
    let swag7 = templates
        .iter()
        .find(|t| t["key"] == "swag7")
        .expect("swag7 template seeded for CS2");
    assert_eq!(swag7["name"], "Swag 7");
    assert_eq!(swag7["stat_key"], "kills.weapon.mag7");

    // Unknown games 404 on both.
    app.get("/v1/games/not-a-game/stat-catalog")
        .await
        .assert_status(StatusCode::NOT_FOUND);
    app.get("/v1/games/not-a-game/award-templates")
        .await
        .assert_status(StatusCode::NOT_FOUND);
}
