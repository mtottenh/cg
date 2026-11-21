//! Round-robin / group-stage result integrity.
//!
//! Every test here drives a standalone round robin or the round-robin groups
//! of a groups-and-playoffs cup through the public API (plus the lifecycle
//! pass, for no-shows) and asserts on what the standings, the stage statuses
//! and the playoff bracket end up saying. They cover the ways a *decided*
//! result used to fall out of the derived standings / stage advancement:
//! forfeits, withdrawals and disqualifications, reverts, admin overrides,
//! dispute resolutions, and racing stage transitions.

use std::collections::HashMap;

use crate::common::TestApp;
use crate::results::transition_match_to_in_progress;
use crate::tournaments::{create_test_player, insert_test_registration};
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

// ============================================================================
// SETUP
// ============================================================================

struct Cup {
    tournament_id: String,
    /// registration id → bearer token of the player behind it.
    tokens: HashMap<String, String>,
    /// registration ids in creation order.
    regs: Vec<String>,
}

async fn create_cup(
    app: &TestApp,
    slug: &str,
    format: &str,
    players: usize,
    format_settings: Value,
) -> Cup {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("RR integrity {slug}"),
                "slug": slug,
                "format": format,
                "format_settings": format_settings,
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
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
    let tournament_id = response.json::<Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!(
        "/v1/tournaments/{tournament_id}/open-registration"
    ))
    .await
    .assert_status(StatusCode::OK);

    let mut tokens = HashMap::new();
    let mut regs = Vec::new();
    for i in 1..=players {
        let username = format!("{}_{i}", slug.replace('-', "_"));
        let (user_id, player_id) = create_test_player(app, &username).await;
        let reg =
            insert_test_registration(app, &tournament_id, player_id, user_id, &username).await;
        tokens.insert(
            reg.clone(),
            create_test_token(user_id, player_id, &username, TEST_JWT_SECRET),
        );
        regs.push(reg);
    }
    assign_role_to_user(
        app.pool(),
        get_dev_user_id(app.pool()).await,
        "platform_admin",
    )
    .await;
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    Cup {
        tournament_id,
        tokens,
        regs,
    }
}

async fn start(app: &TestApp, cup: &Cup) {
    app.post_auth(&format!("/v1/tournaments/{}/start", cup.tournament_id))
        .await
        .assert_status(StatusCode::OK);
}

async fn round_robin(app: &TestApp, slug: &str, players: usize) -> Cup {
    let cup = create_cup(app, slug, "round_robin", players, json!({})).await;
    start(app, &cup).await;
    cup
}

/// Groups of round robins feeding a single-elimination playoff.
async fn groups(
    app: &TestApp,
    slug: &str,
    players: usize,
    group_count: usize,
    advance: usize,
) -> Cup {
    let cup = create_cup(
        app,
        slug,
        "groups_and_playoffs",
        players,
        json!({
            "group_count": group_count,
            "advance_per_group": advance,
            "group_format": "round_robin",
            "playoff_format": "single_elimination"
        }),
    )
    .await;
    start(app, &cup).await;
    cup
}

async fn matches(app: &TestApp, tournament_id: &str) -> Vec<Value> {
    app.get(&format!(
        "/v1/tournaments/{tournament_id}/matches?per_page=100"
    ))
    .await
    .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .clone()
}

async fn get_match(app: &TestApp, tournament_id: &str, match_id: &str) -> Value {
    app.get(&format!(
        "/v1/tournaments/{tournament_id}/matches/{match_id}"
    ))
    .await
    .json::<Value>()["data"]
        .clone()
}

async fn brackets(app: &TestApp, tournament_id: &str) -> Vec<Value> {
    app.get(&format!("/v1/tournaments/{tournament_id}/brackets"))
        .await
        .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .clone()
}

async fn stages(app: &TestApp, tournament_id: &str) -> Vec<Value> {
    app.get(&format!("/v1/tournaments/{tournament_id}/stages"))
        .await
        .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .clone()
}

async fn standings(app: &TestApp, tournament_id: &str, bracket_id: &str) -> Vec<Value> {
    app.get(&format!(
        "/v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings"
    ))
    .await
    .json::<Value>()["data"]
        .as_array()
        .unwrap()
        .clone()
}

fn row_for<'a>(rows: &'a [Value], reg: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["registration_id"] == reg)
        .unwrap_or_else(|| panic!("no standings row for {reg}"))
}

fn str_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn p1(m: &Value) -> String {
    str_of(m, "participant1_registration_id")
}

fn p2(m: &Value) -> String {
    str_of(m, "participant2_registration_id")
}

/// Submit a claim for `winner` and have the opponent confirm it — the normal
/// production path (claim → confirm → completion saga).
async fn confirm_win(app: &TestApp, cup: &Cup, m: &Value, winner: &str, score: (i32, i32)) {
    let match_id = str_of(m, "id");
    let loser = if p1(m) == winner { p2(m) } else { p1(m) };
    let fresh = get_match(app, &cup.tournament_id, &match_id).await;
    if fresh["status"] != "in_progress" && fresh["status"] != "awaiting_result" {
        transition_match_to_in_progress(app, &cup.tournament_id, &match_id).await;
    }
    let (s1, s2) = if p1(m) == winner {
        score
    } else {
        (score.1, score.0)
    };
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": winner,
                "participant1_score": s1,
                "participant2_score": s2,
                "game_results": [],
                "evidence_ids": []
            }),
            &cup.tokens[winner],
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post_with_token(
        &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
        &cup.tokens[&loser],
    )
    .await
    .assert_status(StatusCode::OK);
}

async fn admin_forfeit(app: &TestApp, cup: &Cup, match_id: &str, forfeiter: &str) -> Value {
    let response = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{match_id}/forfeit",
                cup.tournament_id
            ),
            &json!({
                "forfeiting_registration_id": forfeiter,
                "forfeit_type": "no_show",
                "reason": "Did not turn up to the match"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    response.json()
}

fn group_brackets(all: &[Value]) -> Vec<Value> {
    let mut g: Vec<Value> = all
        .iter()
        .filter(|b| b["bracket_type"] == "round_robin")
        .cloned()
        .collect();
    g.sort_by_key(|b| b["group_number"].as_i64().unwrap_or(0));
    g
}

fn bracket_matches(all: &[Value], bracket_id: &str) -> Vec<Value> {
    all.iter()
        .filter(|m| m["bracket_id"] == bracket_id)
        .cloned()
        .collect()
}

fn playoff_participants(all_matches: &[Value], all_brackets: &[Value]) -> Vec<String> {
    let Some(playoff) = all_brackets
        .iter()
        .find(|b| b["bracket_type"] == "single_elim")
    else {
        return Vec::new();
    };
    let mut out: Vec<String> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == playoff["id"])
        .flat_map(|m| {
            [
                m["participant1_registration_id"].as_str().map(String::from),
                m["participant2_registration_id"].as_str().map(String::from),
            ]
        })
        .flatten()
        .collect();
    out.sort();
    out.dedup();
    out
}

// ============================================================================
// 1. FORFEITS ARE DECIDED RESULTS
// ============================================================================

/// An admin forfeit in a group's last match must count for standings (3
/// points, a wins_required-0 walkover) and must seed the playoffs exactly as a
/// played result would.
#[tokio::test]
async fn admin_forfeit_in_last_group_match_counts_and_seeds_playoffs() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-forfeit-seeds", 4, 2, 1).await;

    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    assert_eq!(gb.len(), 2);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    assert_eq!((a.len(), b.len()), (1, 1), "two groups of two");

    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;

    // Group A's only match: participant 2 no-shows mid-match.
    let match_id = str_of(&a[0], "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p2(&a[0])).await;

    let rows = standings(&app, &cup.tournament_id, gb[0]["id"].as_str().unwrap()).await;
    let winner = row_for(&rows, &p1(&a[0]));
    assert_eq!(winner["points"], 3, "forfeit winner gets the win: {rows:?}");
    assert_eq!(winner["matches_won"], 1);
    assert_eq!(
        winner["game_wins"], 2,
        "walkover is recorded as 2-0 in a Bo3"
    );
    let loser = row_for(&rows, &p2(&a[0]));
    assert_eq!(loser["matches_lost"], 1);
    assert_eq!(loser["points"], 0);

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "completed", "group stage closes: {st:?}");
    assert_eq!(st[1]["status"], "active", "playoffs open");
    let mut expected = vec![p1(&a[0]), p1(&b[0])];
    expected.sort();
    assert_eq!(
        playoff_participants(
            &matches(&app, &cup.tournament_id).await,
            &brackets(&app, &cup.tournament_id).await
        ),
        expected,
        "the forfeit winner and group B's winner meet in the final"
    );
}

/// A captain conceding through the participant forfeit endpoint feeds the
/// standings like any other decided result.
#[tokio::test]
async fn user_forfeit_counts_in_standings() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-user-forfeit", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;

    let forfeiter = p1(m);
    app.post_json_with_token(
        &format!(
            "/v1/tournaments/{}/matches/{match_id}/forfeit",
            cup.tournament_id
        ),
        &json!({ "registration_id": forfeiter }),
        &cup.tokens[&forfeiter],
    )
    .await
    .assert_status(StatusCode::OK);

    let bracket_id = str_of(m, "bracket_id");
    let rows = standings(&app, &cup.tournament_id, &bracket_id).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p1(m))["matches_lost"], 1);
}

/// The lifecycle no-show forfeit in a group's last match seeds the playoffs.
#[tokio::test]
async fn lifecycle_no_show_in_last_group_match_seeds_playoffs() {
    use portal_api::background::{LifecycleConfig, run_lifecycle_pass};

    let app = TestApp::new().await;
    let cup = groups(&app, "rr-noshow-seeds", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    confirm_win(&app, &cup, &b[0], &p2(&b[0]), (2, 1)).await;

    // Group A's match is in check-in, only participant 1 checked in, and the
    // deadline has passed.
    sqlx::query(
        "UPDATE tournament_matches SET status = 'checking_in',
                scheduled_at = NOW() - INTERVAL '20 minutes',
                participant1_checked_in_at = NOW() - INTERVAL '5 minutes',
                check_in_deadline = NOW() - INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(a[0]["id"].as_str().unwrap()).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let state = portal_api::state::AppState::new(app.pool().clone(), "test-jwt-secret").await;
    let cfg = LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
        veto_coin_flip_grace: chrono::Duration::seconds(15),
    };
    let summary = run_lifecycle_pass(&state, &cfg, false).await;
    assert_eq!(summary.no_shows_forfeited, 1, "{summary:?}");

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[1]["status"], "active", "playoffs open: {st:?}");
    let mut expected = vec![p1(&a[0]), p2(&b[0])];
    expected.sort();
    assert_eq!(
        playoff_participants(
            &matches(&app, &cup.tournament_id).await,
            &brackets(&app, &cup.tournament_id).await
        ),
        expected
    );
}

/// Elimination formats: a forfeit must advance the winner into the next round
/// (it used to leave the bracket stuck with the next match half-empty).
#[tokio::test]
async fn admin_forfeit_in_single_elimination_advances_the_winner() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-forfeit-adv", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let r1: Vec<&Value> = all.iter().filter(|m| m["round"] == 1).collect();
    let m = r1[0];
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p1(m)).await;

    let next_id: Option<Uuid> =
        sqlx::query_scalar("SELECT winner_progresses_to FROM tournament_matches WHERE id = $1")
            .bind(Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    let next_id = next_id.expect("R1 match links to the final").to_string();
    let next = get_match(&app, &cup.tournament_id, &next_id).await;
    let seated = [
        next["participant1_registration_id"].as_str(),
        next["participant2_registration_id"].as_str(),
    ];
    assert!(
        seated.contains(&Some(p2(m).as_str())),
        "the forfeit winner is seated in the final: {next:?}"
    );
}

// ============================================================================
// 2. WITHDRAWAL / DISQUALIFICATION FORFEIT WHAT IS LEFT
// ============================================================================

#[tokio::test]
async fn withdrawal_forfeits_every_unplayed_round_robin_match() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-withdraw", 3).await;
    let leaver = cup.regs[0].clone();

    let response = app
        .post_json_with_token(
            &format!(
                "/v1/tournaments/{}/registrations/{leaver}/withdraw",
                cup.tournament_id
            ),
            &json!({ "reason": "Team disbanded" }),
            &cup.tokens[&leaver],
        )
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<Value>()["data"]["matches_forfeited"],
        2,
        "both of the leaver's (ready) matches are forfeited"
    );

    let all = matches(&app, &cup.tournament_id).await;
    let bracket_id = str_of(&all[0], "bracket_id");
    let rows = standings(&app, &cup.tournament_id, &bracket_id).await;
    for reg in &cup.regs[1..] {
        assert_eq!(row_for(&rows, reg)["points"], 3, "{rows:?}");
    }
    assert_eq!(row_for(&rows, &leaver)["matches_lost"], 2);
}

/// The tournament-scoped disqualify endpoint used to flip the registration
/// status and nothing else.
#[tokio::test]
async fn registration_disqualify_forfeits_remaining_matches() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-dq", 3).await;
    let cheater = cup.regs[1].clone();

    app.post_json(
        &format!(
            "/v1/tournaments/{}/registrations/{cheater}/disqualify",
            cup.tournament_id
        ),
        &json!({ "reason": "Caught using third-party software" }),
    )
    .await
    .assert_status(StatusCode::OK);

    let all = matches(&app, &cup.tournament_id).await;
    for m in all.iter().filter(|m| p1(m) == cheater || p2(m) == cheater) {
        assert_eq!(m["status"], "forfeit", "{m:?}");
        assert_ne!(m["winner_registration_id"], cheater.as_str());
    }
}

// ============================================================================
// 3. WITHDRAWN PARTICIPANTS DO NOT TAKE A PLAYOFF SLOT
// ============================================================================

#[tokio::test]
async fn withdrawn_group_winner_does_not_advance() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-withdrawn-adv", 6, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());

    // Pick group A's eventual winner and let them win both their matches.
    let star = p1(&a[0]);
    for m in &a {
        let w = if p1(m) == star || p2(m) == star {
            star.clone()
        } else {
            p1(m)
        };
        confirm_win(&app, &cup, m, &w, (2, 0)).await;
    }
    // Group A is decided; the star then withdraws (nothing left to forfeit).
    app.post_json_with_token(
        &format!(
            "/v1/tournaments/{}/registrations/{star}/withdraw",
            cup.tournament_id
        ),
        &json!({ "reason": "Visa problems" }),
        &cup.tokens[&star],
    )
    .await
    .assert_status(StatusCode::OK);

    for m in &b {
        confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;
    }

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[1]["status"], "active", "{st:?}");
    let seeded = playoff_participants(
        &matches(&app, &cup.tournament_id).await,
        &brackets(&app, &cup.tournament_id).await,
    );
    assert_eq!(seeded.len(), 2, "{seeded:?}");
    assert!(
        !seeded.contains(&star),
        "a withdrawn participant must not take a playoff slot"
    );
}

// ============================================================================
// 4. REVERT REOPENS A ROUND-ROBIN MATCH
// ============================================================================

#[tokio::test]
async fn reverted_round_robin_match_can_be_reported_again() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-revert-reopen", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;

    for _ in 0..2 {
        app.post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
            .await
            .assert_status(StatusCode::OK);
    }
    let reverted = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_ne!(reverted["status"], "completed", "{reverted:?}");
    assert!(reverted["winner_registration_id"].is_null());

    // The captains can now report the corrected result.
    confirm_win(&app, &cup, m, &p2(m), (2, 1)).await;
    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p1(m))["points"], 0);

    // Reapply still works on top of a revert.
    app.post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await
        .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/matches/{match_id}/progression/reapply"),
        &json!({ "new_winner_registration_id": p1(m) }),
    )
    .await
    .assert_status(StatusCode::OK);
    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p1(m))["points"], 3, "{rows:?}");
    assert_eq!(
        get_match(&app, &cup.tournament_id, &match_id).await["status"],
        "completed"
    );
}

// ============================================================================
// 5. OVERRIDE / DISPUTE RESOLUTION RECOMPUTE STANDINGS AND ADVANCE STAGES
// ============================================================================

#[tokio::test]
async fn admin_override_recomputes_round_robin_standings() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-override", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;

    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/matches/{match_id}/result-override",
            cup.tournament_id
        ),
        &json!({
            "participant1_score": 1,
            "participant2_score": 2,
            "reason": "Captains confirmed the wrong score"
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p1(m))["points"], 0);
    assert_eq!(row_for(&rows, &p2(m))["game_wins"], 2);
}

/// Overriding a forfeit's score would silently turn a walkover into a played
/// result; that is refused (revert/reapply own forfeits).
#[tokio::test]
async fn admin_override_refuses_a_forfeit() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-override-forfeit", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p1(m)).await;

    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/matches/{match_id}/result-override",
            cup.tournament_id
        ),
        &json!({
            "participant1_score": 2,
            "participant2_score": 0,
            "reason": "Trying to rewrite a walkover"
        }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(
        get_match(&app, &cup.tournament_id, &match_id).await["status"],
        "forfeit"
    );
}

/// A dispute resolved on a group's last match must seed the playoffs.
#[tokio::test]
async fn dispute_resolution_on_last_group_match_seeds_playoffs() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-dispute-seeds", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;

    // Group A: p1 claims a win, p2 disputes, the admin adjusts to a p2 win.
    let m = &a[0];
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    let claim = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1(m),
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
            &cup.tokens[&p1(m)],
        )
        .await;
    claim.assert_status(StatusCode::CREATED);
    let claim_id = claim.json::<Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post_json_with_token(
        &format!("/v1/matches/{match_id}/result/{claim_id}/dispute"),
        &json!({
            "reason": "wrong_score",
            "description": "We won that series, the claim is backwards.",
            "evidence_ids": []
        }),
        &cup.tokens[&p2(m)],
    )
    .await
    .assert_status(StatusCode::OK);
    let dispute_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM disputes WHERE match_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    app.post_json(
        &format!("/v1/admin/disputes/{dispute_id}/resolve/adjusted"),
        &json!({
            "new_participant1_score": 1,
            "new_participant2_score": 2,
            "notes": "Demo shows participant two won the series."
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    let rows = standings(&app, &cup.tournament_id, gb[0]["id"].as_str().unwrap()).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[1]["status"], "active", "{st:?}");
    let mut expected = vec![p2(m), p1(&b[0])];
    expected.sort();
    assert_eq!(
        playoff_participants(
            &matches(&app, &cup.tournament_id).await,
            &brackets(&app, &cup.tournament_id).await
        ),
        expected
    );
}

// ============================================================================
// 6. PLAYOFFS ARE SEEDED EXACTLY ONCE
// ============================================================================

#[tokio::test]
async fn concurrent_stage_advancement_seeds_playoffs_once() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-seed-once", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    confirm_win(&app, &cup, &a[0], &p1(&a[0]), (2, 0)).await;

    // Record group B's result directly, then race several admin "process"
    // calls — every one of them sees a finished group stage.
    let mb = &b[0];
    sqlx::query(
        "UPDATE tournament_matches SET participant1_score = 2, participant2_score = 0,
                winner_registration_id = participant1_registration_id,
                loser_registration_id = participant2_registration_id,
                status = 'completed', completed_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(mb["id"].as_str().unwrap()).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    let uri = format!("/v1/admin/matches/{}/progression/process", str_of(mb, "id"));
    let body = json!({
        "winner_registration_id": p1(mb),
        "loser_registration_id": p2(mb)
    });
    let (r1, r2, r3, r4) = tokio::join!(
        app.post_json(&uri, &body),
        app.post_json(&uri, &body),
        app.post_json(&uri, &body),
        app.post_json(&uri, &body)
    );
    for r in [&r1, &r2, &r3, &r4] {
        assert!(
            r.status == StatusCode::OK || r.status == StatusCode::CONFLICT,
            "unexpected {}",
            r.status
        );
    }

    let playoff_brackets = brackets(&app, &cup.tournament_id)
        .await
        .into_iter()
        .filter(|b| b["bracket_type"] == "single_elim")
        .count();
    assert_eq!(playoff_brackets, 1, "playoffs generated exactly once");
}

// ============================================================================
// 7. A START THAT CANNOT SUCCEED DOES NOT CLAIM THE TOURNAMENT
// ============================================================================

#[tokio::test]
async fn impossible_group_config_is_rejected_before_start_claims() {
    let app = TestApp::new().await;
    // 4 players in 2 groups → groups of 2; advancing 3 per group is impossible.
    let cup = create_cup(
        &app,
        "rr-bad-start",
        "groups_and_playoffs",
        4,
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "round_robin",
            "playoff_format": "single_elimination"
        }),
    )
    .await;
    sqlx::query(
        "UPDATE tournaments SET format_settings = jsonb_set(format_settings, '{advance_per_group}', '3') WHERE id = $1",
    )
    .bind(Uuid::parse_str(&cup.tournament_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", cup.tournament_id))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    let t = app
        .get(&format!("/v1/tournaments/{}", cup.tournament_id))
        .await
        .json::<Value>();
    assert_eq!(
        t["data"]["status"], "registration",
        "a failed start must leave the tournament startable"
    );
    assert!(brackets(&app, &cup.tournament_id).await.is_empty());

    // Fix the config and start for real.
    sqlx::query(
        "UPDATE tournaments SET format_settings = jsonb_set(format_settings, '{advance_per_group}', '1') WHERE id = $1",
    )
    .bind(Uuid::parse_str(&cup.tournament_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    start(&app, &cup).await;
    assert_eq!(
        group_brackets(&brackets(&app, &cup.tournament_id).await).len(),
        2
    );
}

#[tokio::test]
async fn invalid_format_settings_are_rejected_at_create() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let base = |slug: &str, format: &str, settings: Value| {
        json!({
            "game_id": game_id,
            "name": format!("bad settings {slug}"),
            "slug": slug,
            "format": format,
            "format_settings": settings,
            "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
            "participant_type": "individual",
            "min_participants": 2,
            "max_participants": 8,
            "registration_type": "open",
            "scheduling_mode": "self_scheduled",
            "default_match_format": "bo3"
        })
    };
    // 8 max participants over 4 groups → at most 2 per group; advancing 3 is impossible.
    app.post_json(
        "/v1/tournaments",
        &base(
            "bad-groups",
            "groups_and_playoffs",
            json!({ "group_count": 4, "advance_per_group": 3 }),
        ),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    app.post_json(
        "/v1/tournaments",
        &base(
            "bad-round-formats",
            "round_robin",
            json!({ "round_formats": { "1": "bo4" } }),
        ),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// 8. DETERMINISTIC TIEBREAKERS
// ============================================================================

/// A and B finish level on points, game differential and wins; A beat B, so
/// head-to-head puts A first even though B holds the better seed.
#[tokio::test]
async fn head_to_head_then_seed_breaks_ties_deterministically() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-tiebreak", 4).await;
    let all = matches(&app, &cup.tournament_id).await;
    let bracket_id = str_of(&all[0], "bracket_id");
    let [a, b, c, d] = [&cup.regs[0], &cup.regs[1], &cup.regs[2], &cup.regs[3]];
    // B gets the best seed, A the worst.
    for (reg, seed) in [(b, 1), (c, 2), (d, 3), (a, 4)] {
        sqlx::query("UPDATE tournament_registrations SET seed = $2 WHERE id = $1")
            .bind(Uuid::parse_str(reg).unwrap())
            .bind(seed)
            .execute(app.pool())
            .await
            .unwrap();
    }
    let find = |x: &str, y: &str| {
        all.iter()
            .find(|m| (p1(m) == x && p2(m) == y) || (p1(m) == y && p2(m) == x))
            .unwrap()
            .clone()
    };
    // A: beats B 2-1, loses to C 1-2, beats D 2-0  → 6 pts, GD +2
    // B: beats C 2-0, beats D 2-1                   → 6 pts, GD +2
    // C: beats A 2-1, D beats C 2-0                 → 3 pts
    // D: beats C 2-0                                → 3 pts
    confirm_win(&app, &cup, &find(a, b), a, (2, 1)).await;
    confirm_win(&app, &cup, &find(a, c), c, (2, 1)).await;
    confirm_win(&app, &cup, &find(a, d), a, (2, 0)).await;
    confirm_win(&app, &cup, &find(b, c), b, (2, 0)).await;
    confirm_win(&app, &cup, &find(b, d), b, (2, 1)).await;
    confirm_win(&app, &cup, &find(c, d), d, (2, 0)).await;

    let rows = standings(&app, &cup.tournament_id, &bracket_id).await;
    let pos = |reg: &str| row_for(&rows, reg)["position"].as_i64().unwrap();
    assert_eq!(pos(a), 1, "head-to-head: A beat B — {rows:?}");
    assert_eq!(pos(b), 2);
    // C and D: level on points; D has the better GD (-1 vs -3).
    assert_eq!(pos(d), 3);
    assert_eq!(pos(c), 4);

    // Stable across recomputes.
    for _ in 0..3 {
        app.post_auth(&format!(
            "/v1/admin/matches/{}/progression/revert",
            str_of(&find(c, d), "id")
        ))
        .await
        .assert_status(StatusCode::OK);
        app.post_json(
            &format!(
                "/v1/admin/matches/{}/progression/reapply",
                str_of(&find(c, d), "id")
            ),
            &json!({ "new_winner_registration_id": d }),
        )
        .await
        .assert_status(StatusCode::OK);
        let again = standings(&app, &cup.tournament_id, &bracket_id).await;
        let order: Vec<_> = again.iter().map(|r| r["registration_id"].clone()).collect();
        let first: Vec<_> = rows.iter().map(|r| r["registration_id"].clone()).collect();
        assert_eq!(order, first);
    }
}

/// Fully level participants (a three-way cycle) fall back to seed order.
#[tokio::test]
async fn complete_tie_falls_back_to_seed() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-cycle", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let bracket_id = str_of(&all[0], "bracket_id");
    let [a, b, c] = [&cup.regs[0], &cup.regs[1], &cup.regs[2]];
    for (reg, seed) in [(c, 1), (a, 2), (b, 3)] {
        sqlx::query("UPDATE tournament_registrations SET seed = $2 WHERE id = $1")
            .bind(Uuid::parse_str(reg).unwrap())
            .bind(seed)
            .execute(app.pool())
            .await
            .unwrap();
    }
    let find = |x: &str, y: &str| {
        all.iter()
            .find(|m| (p1(m) == x && p2(m) == y) || (p1(m) == y && p2(m) == x))
            .unwrap()
            .clone()
    };
    confirm_win(&app, &cup, &find(a, b), a, (2, 0)).await;
    confirm_win(&app, &cup, &find(b, c), b, (2, 0)).await;
    confirm_win(&app, &cup, &find(c, a), c, (2, 0)).await;
    let rows = standings(&app, &cup.tournament_id, &bracket_id).await;
    let order: Vec<String> = rows.iter().map(|r| str_of(r, "registration_id")).collect();
    assert_eq!(order, vec![c.clone(), a.clone(), b.clone()], "{rows:?}");
}

// ============================================================================
// 9. SERIES SCORES MUST BE A REAL BEST-OF RESULT
// ============================================================================

#[tokio::test]
async fn impossible_best_of_scores_are_rejected() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-scores", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    let winner = p1(m);
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": winner,
                "participant1_score": 3,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
            &cup.tokens[&winner],
        )
        .await;
    assert_eq!(
        response.status,
        StatusCode::BAD_REQUEST,
        "3-0 is not a Bo3 result"
    );

    confirm_win(&app, &cup, m, &winner, (2, 1)).await;
    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/matches/{match_id}/result-override",
            cup.tournament_id
        ),
        &json!({
            "participant1_score": 3,
            "participant2_score": 0,
            "reason": "Typo-ed correction"
        }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// 10. A STANDALONE ROUND ROBIN FINISHES
// ============================================================================

#[tokio::test]
async fn standalone_round_robin_marks_bracket_and_stage_completed() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-finishes", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let (last, rest) = all.split_last().unwrap();
    for m in rest {
        confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;
    }
    assert_eq!(
        stages(&app, &cup.tournament_id).await[0]["status"],
        "active"
    );

    // The last one is a forfeit — it still finishes the event.
    let match_id = str_of(last, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p2(last)).await;

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "completed", "{st:?}");
    let br = brackets(&app, &cup.tournament_id).await;
    assert_eq!(br[0]["status"], "completed", "{br:?}");
    // Tournament completion stays an explicit admin action, as it is for
    // elimination formats.
    let t = app
        .get(&format!("/v1/tournaments/{}", cup.tournament_id))
        .await
        .json::<Value>();
    assert_eq!(t["data"]["status"], "in_progress");
}

// ============================================================================
// 10. A FORFEIT IS FINAL: STALE CLAIMS AND RACING FORFEITS
// ============================================================================

fn lifecycle_config() -> portal_api::background::LifecycleConfig {
    portal_api::background::LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
        veto_coin_flip_grace: chrono::Duration::seconds(15),
    }
}

async fn run_lifecycle(app: &TestApp) -> portal_api::background::LifecyclePassSummary {
    let state = portal_api::state::AppState::new(app.pool().clone(), "test-jwt-secret").await;
    portal_api::background::run_lifecycle_pass(&state, &lifecycle_config(), false).await
}

/// File a result claim for `winner` (2–1) and return its id.
async fn submit_claim(app: &TestApp, cup: &Cup, m: &Value, winner: &str) -> String {
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(app, &cup.tournament_id, &match_id).await;
    let (s1, s2) = if p1(m) == winner { (2, 1) } else { (1, 2) };
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": winner,
                "participant1_score": s1,
                "participant2_score": s2,
                "game_results": [],
                "evidence_ids": []
            }),
            &cup.tokens[winner],
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn claim_status(app: &TestApp, claim_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM result_claims WHERE id = $1")
        .bind(Uuid::parse_str(claim_id).unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap()
}

/// A team files a result claim and is then disqualified: the match is
/// forfeited to the opponent. The claim it left behind must die with it —
/// it used to stay pending, get auto-confirmed at its deadline, and rewrite
/// the forfeit as a win for the disqualified team.
#[tokio::test]
async fn pending_claim_cannot_overwrite_a_forfeit() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-claim-vs-forfeit", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    let (cheater, victim) = (p1(m), p2(m));
    let claim_id = submit_claim(&app, &cup, m, &cheater).await;

    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/registrations/{cheater}/disqualify",
            cup.tournament_id
        ),
        &json!({ "reason": "Cheating confirmed by the admin team" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let forfeited = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(forfeited["status"], "forfeit");
    assert_eq!(forfeited["winner_registration_id"], victim.as_str());
    assert_eq!(
        claim_status(&app, &claim_id).await,
        "superseded",
        "the forfeit supersedes the pending claim"
    );

    // Even a claim that is somehow still pending past its deadline (one that
    // predates the supersede) must not be applied over the forfeit — neither
    // by the auto-confirm sweep nor by the opponent confirming it.
    sqlx::query(
        "UPDATE result_claims
         SET status = 'pending', auto_confirm_at = NOW() - INTERVAL '1 minute'
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&claim_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    let confirm = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
            &cup.tokens[&victim],
        )
        .await;
    assert_eq!(confirm.status, StatusCode::CONFLICT, "{}", confirm.text());
    assert_eq!(claim_status(&app, &claim_id).await, "pending");

    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.claims_auto_confirmed, 0, "{summary:?}");
    assert_eq!(
        claim_status(&app, &claim_id).await,
        "superseded",
        "the sweep retires a claim on a match that is already decided"
    );

    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "forfeit");
    assert_eq!(after["winner_registration_id"], victim.as_str());
    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &victim)["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &cheater)["points"], 0, "{rows:?}");
}

/// Two admins forfeit the same match at the same moment, one per side.
/// Exactly one forfeit may land; the other is a 409 that writes nothing.
/// Both used to succeed: two forfeit records, and the match row naming one
/// winner while the other had been advanced into the final.
#[tokio::test]
async fn concurrent_opposite_forfeits_only_one_wins() {
    let app = TestApp::new().await;
    for i in 0..6 {
        let cup = create_cup(
            &app,
            &format!("se-race-forfeit-{i}"),
            "single_elimination",
            4,
            json!({}),
        )
        .await;
        start(&app, &cup).await;
        let all = matches(&app, &cup.tournament_id).await;
        let m = all.iter().find(|m| m["round"] == 1).unwrap().clone();
        let match_id = str_of(&m, "id");
        let url = format!(
            "/v1/admin/tournaments/{}/matches/{match_id}/forfeit",
            cup.tournament_id
        );
        let body = |reg: String| {
            json!({
                "forfeiting_registration_id": reg,
                "forfeit_type": "no_show",
                "reason": "Did not turn up to the match"
            })
        };
        let (b1, b2) = (body(p1(&m)), body(p2(&m)));
        let (r1, r2) = tokio::join!(app.post_json(&url, &b1), app.post_json(&url, &b2));

        let mut statuses = [r1.status, r2.status];
        statuses.sort();
        assert!(
            statuses[0] == StatusCode::OK
                && (statuses[1] == StatusCode::CONFLICT || statuses[1] == StatusCode::BAD_REQUEST),
            "run {i}: exactly one forfeit may succeed, got {:?} / {:?}: {} | {}",
            r1.status,
            r2.status,
            r1.text(),
            r2.text()
        );
        let expected_winner = if r1.status == StatusCode::OK {
            p2(&m)
        } else {
            p1(&m)
        };

        let records: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM forfeit_records WHERE match_id = $1")
                .bind(Uuid::parse_str(&match_id).unwrap())
                .fetch_one(app.pool())
                .await
                .unwrap();
        assert_eq!(records, 1, "run {i}: one forfeit, one record");

        let decided = get_match(&app, &cup.tournament_id, &match_id).await;
        assert_eq!(decided["status"], "forfeit");
        assert_eq!(
            decided["winner_registration_id"],
            expected_winner.as_str(),
            "run {i}"
        );
        let final_match = matches(&app, &cup.tournament_id)
            .await
            .into_iter()
            .find(|x| x["round"] == 2)
            .unwrap();
        let seated = [
            final_match["participant1_registration_id"].as_str(),
            final_match["participant2_registration_id"].as_str(),
        ];
        assert!(
            seated.contains(&Some(expected_winner.as_str())),
            "run {i}: the recorded winner is the one advanced: {final_match:?}"
        );
        let loser = if expected_winner == p1(&m) {
            p2(&m)
        } else {
            p1(&m)
        };
        assert!(
            !seated.contains(&Some(loser.as_str())),
            "run {i}: the forfeiting side was advanced too: {final_match:?}"
        );
    }
}

/// The guard itself, without the race: a forfeit against a match that has
/// been decided since the caller looked is a 409, not a silent overwrite.
#[tokio::test]
async fn forfeit_write_is_guarded_on_match_status() {
    use portal_domain::repositories::tournament::TournamentMatchRepository as _;

    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-forfeit-guard", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;

    let repo = portal_db::PgTournamentMatchRepository::new(app.pool().clone());
    let err = repo
        .forfeit(
            match_id.parse().unwrap(),
            p2(m).parse().unwrap(),
            p1(m).parse().unwrap(),
            2,
            0,
        )
        .await
        .expect_err("a completed match cannot be forfeited");
    assert!(
        matches!(err, portal_core::DomainError::Conflict(_)),
        "expected Conflict, got {err:?}"
    );
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "completed");
    assert_eq!(after["winner_registration_id"], p1(m).as_str());
}
