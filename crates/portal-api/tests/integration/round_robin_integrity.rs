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

// ============================================================================
// 11. A DEPARTED TEAM PLAYS NO FURTHER MATCH
// ============================================================================

async fn withdraw(app: &TestApp, cup: &Cup, reg: &str) {
    app.post_json_with_token(
        &format!(
            "/v1/tournaments/{}/registrations/{reg}/withdraw",
            cup.tournament_id
        ),
        &json!({ "reason": "Cannot make the rest of the event" }),
        &cup.tokens[reg],
    )
    .await
    .assert_status(StatusCode::OK);
}

fn seats(m: &Value, reg: &str) -> bool {
    m["participant1_registration_id"] == reg || m["participant2_registration_id"] == reg
}

fn is_settled(m: &Value) -> bool {
    matches!(
        m["status"].as_str(),
        Some("completed" | "forfeit" | "cancelled")
    )
}

/// Single elimination: a team wins its semi-final, is seated in the final
/// (still waiting for the other semi) and then withdraws. Nothing could be
/// forfeited at that moment — the final had no opponent. When the other
/// semi finishes, the final must be a walkover, not a playable match
/// against a team that has left.
#[tokio::test]
async fn withdrawn_team_seated_in_pending_final_forfeits_it() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "se-wd-pending-final",
        "single_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let semis: Vec<Value> = all.iter().filter(|m| m["round"] == 1).cloned().collect();
    assert_eq!(semis.len(), 2);

    let leaver = p1(&semis[0]);
    confirm_win(&app, &cup, &semis[0], &leaver, (2, 0)).await;
    withdraw(&app, &cup, &leaver).await;
    let pending_final = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 2)
        .unwrap();
    assert_eq!(pending_final["status"], "pending");
    assert!(seats(&pending_final, &leaver));

    let finalist = p1(&semis[1]);
    confirm_win(&app, &cup, &semis[1], &finalist, (2, 0)).await;

    let final_match = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 2)
        .unwrap();
    assert_eq!(
        final_match["status"], "forfeit",
        "the final against a withdrawn team is a walkover: {final_match:?}"
    );
    assert_eq!(final_match["winner_registration_id"], finalist.as_str());
    let record: (String, bool) = sqlx::query_as(
        "SELECT forfeit_type, triggered_by_system FROM forfeit_records WHERE match_id = $1",
    )
    .bind(Uuid::parse_str(&str_of(&final_match, "id")).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(record, ("withdrawal".to_string(), true));
}

/// Double elimination: forfeiting a withdrawn team's winners-bracket match
/// drops it into the losers bracket like any loser. It must not stay
/// playable there — every match it is seated in ends up settled, and its
/// losers-bracket opponent gets the walkover.
#[tokio::test]
async fn withdrawn_team_routed_into_losers_bracket_forfeits_there() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "de-wd-losers", "double_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let openers: Vec<Value> = all
        .iter()
        .filter(|m| {
            m["participant1_registration_id"].is_string()
                && m["participant2_registration_id"].is_string()
        })
        .cloned()
        .collect();
    assert_eq!(openers.len(), 2, "{all:?}");

    let leaver = p1(&openers[0]);
    withdraw(&app, &cup, &leaver).await;
    // The other opener's loser is who meets the leaver in the losers bracket.
    let other_winner = p1(&openers[1]);
    let other_loser = p2(&openers[1]);
    confirm_win(&app, &cup, &openers[1], &other_winner, (2, 0)).await;

    let all = matches(&app, &cup.tournament_id).await;
    let with_leaver: Vec<&Value> = all.iter().filter(|m| seats(m, &leaver)).collect();
    assert_eq!(
        with_leaver.len(),
        2,
        "the opener and one losers-bracket match: {with_leaver:?}"
    );
    for m in &with_leaver {
        assert!(
            is_settled(m),
            "a withdrawn team still has a playable match: {m:?}"
        );
        assert_ne!(m["winner_registration_id"], leaver.as_str(), "{m:?}");
    }
    let losers_match = with_leaver
        .iter()
        .find(|m| m["id"] != openers[0]["id"])
        .unwrap();
    assert_eq!(losers_match["status"], "forfeit", "{losers_match:?}");
    assert_eq!(
        losers_match["winner_registration_id"],
        other_loser.as_str(),
        "the walkover goes to the team it would have played"
    );
    // ... and that walkover was settled too: its winner moved on.
    let advanced = all.iter().any(|m| {
        m["id"] != losers_match["id"] && m["id"] != openers[1]["id"] && seats(m, &other_loser)
    });
    assert!(advanced, "the walkover winner was not advanced: {all:?}");
}

/// Both finalists have left by the time the final becomes playable: it is
/// called off (the double-forfeit path), not left open.
#[tokio::test]
async fn final_between_two_departed_teams_is_called_off() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-both-departed", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let semis: Vec<Value> = all.iter().filter(|m| m["round"] == 1).cloned().collect();

    let first = p1(&semis[0]);
    confirm_win(&app, &cup, &semis[0], &first, (2, 0)).await;
    withdraw(&app, &cup, &first).await;

    // The second finalist is disqualified between winning the semi and the
    // result being processed: stage the result without progression, then
    // disqualify, then let the saga run.
    let second = p1(&semis[1]);
    let semi2_id = str_of(&semis[1], "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &semi2_id).await;
    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'completed', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 2, participant2_score = 0, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&semi2_id).unwrap())
    .bind(Uuid::parse_str(&second).unwrap())
    .bind(Uuid::parse_str(&p2(&semis[1])).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/registrations/{second}/disqualify",
            cup.tournament_id
        ),
        &json!({ "reason": "Cheating confirmed by the admin team" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/matches/{semi2_id}/progression/process"),
        &json!({
            "winner_registration_id": second,
            "loser_registration_id": p2(&semis[1])
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    let final_match = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 2)
        .unwrap();
    assert!(seats(&final_match, &first) && seats(&final_match, &second));
    assert_eq!(final_match["status"], "cancelled", "{final_match:?}");
    assert!(final_match["winner_registration_id"].is_null());
}

// ============================================================================
// 12. SETTLEMENT THAT FAILED IS RETRIED
// ============================================================================

/// Make the tournament's matches (and their result reviews) look untouched
/// for an hour — what the lifecycle's stage sweep means by "idle".
/// (`updated_at` is maintained by triggers, so backdating needs them off.)
async fn make_stage_idle(app: &TestApp, cup: &Cup) {
    sqlx::raw_sql(&format!(
        "ALTER TABLE tournament_matches DISABLE TRIGGER tournament_matches_updated_at;
         UPDATE tournament_matches SET updated_at = NOW() - INTERVAL '1 hour'
         WHERE tournament_id = '{0}';
         ALTER TABLE tournament_matches ENABLE TRIGGER tournament_matches_updated_at;
         ALTER TABLE result_reviews DISABLE TRIGGER set_result_reviews_updated_at;
         UPDATE result_reviews SET updated_at = NOW() - INTERVAL '1 hour'
         WHERE match_id IN (SELECT id FROM tournament_matches WHERE tournament_id = '{0}');
         ALTER TABLE result_reviews ENABLE TRIGGER set_result_reviews_updated_at;",
        cup.tournament_id
    ))
    .execute(app.pool())
    .await
    .unwrap();
}

/// A forfeit lands on a group's last match but its settlement never runs
/// (the request died, or the settle failed, after the forfeit committed).
/// No saga row exists to re-drive and no further result will ever arrive,
/// so the lifecycle pass must notice the finished-but-open stage itself.
#[tokio::test]
async fn lifecycle_settles_a_finished_stage_whose_settlement_was_lost() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-lost-settle", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;

    // The forfeit write, with nothing after it.
    let m = &a[0];
    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'forfeit', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 0, participant2_score = 2, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&str_of(m, "id")).unwrap())
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .bind(Uuid::parse_str(&p1(m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "active");
    assert_eq!(st[1]["status"], "pending");

    // A stage that was touched moments ago is left to its live settlement.
    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 0, "{summary:?}");
    assert_eq!(
        stages(&app, &cup.tournament_id).await[1]["status"],
        "pending"
    );

    // Once it has sat idle past the threshold, the pass settles it.
    make_stage_idle(&app, &cup).await;
    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 1, "{summary:?}");

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "completed", "{st:?}");
    assert_eq!(st[1]["status"], "active", "{st:?}");
    let rows = standings(&app, &cup.tournament_id, gb[0]["id"].as_str().unwrap()).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    let mut expected = vec![p2(m), p1(&b[0])];
    expected.sort();
    assert_eq!(
        playoff_participants(
            &matches(&app, &cup.tournament_id).await,
            &brackets(&app, &cup.tournament_id).await
        ),
        expected
    );

    // Nothing left to do on the next pass.
    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 0, "{summary:?}");
}

// ============================================================================
// 13. A PENDING RESULT REVIEW IS NOT BYPASSED
// ============================================================================

/// A match whose completion is waiting on a result review must not be
/// progressed by a dispute ruling on the same match: "no completed saga"
/// there means "paused for review", not "never ran". The ruling is recorded;
/// the winner advances when the review is resolved.
#[tokio::test]
async fn dispute_ruling_does_not_bypass_a_pending_result_review() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-review-gate", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = all.iter().find(|m| m["round"] == 1).unwrap().clone();
    let match_id = str_of(&m, "id");
    let match_uuid = Uuid::parse_str(&match_id).unwrap();

    // p1 claims, p2 disputes; meanwhile the claim is under result review.
    let claim_id = submit_claim(&app, &cup, &m, &p1(&m)).await;
    app.post_json_with_token(
        &format!("/v1/matches/{match_id}/result/{claim_id}/dispute"),
        &json!({
            "reason": "wrong_score",
            "description": "We won that series, the claim is backwards.",
            "evidence_ids": []
        }),
        &cup.tokens[&p2(&m)],
    )
    .await
    .assert_status(StatusCode::OK);
    let review_id: Uuid = sqlx::query_scalar(
        "INSERT INTO result_reviews
             (result_claim_id, match_id, score_mismatch, status,
              captain1_registration_id, captain2_registration_id)
         VALUES ($1, $2, true, 'pending_admin_review', $3, $4)
         RETURNING id",
    )
    .bind(Uuid::parse_str(&claim_id).unwrap())
    .bind(match_uuid)
    .bind(Uuid::parse_str(&p1(&m)).unwrap())
    .bind(Uuid::parse_str(&p2(&m)).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();

    let dispute_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM disputes WHERE match_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(match_uuid)
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

    // The ruling is on the match ...
    let ruled = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(ruled["status"], "completed");
    assert_eq!(ruled["winner_registration_id"], p2(&m).as_str());
    // ... but nobody has been advanced past the review.
    let final_match = |all: Vec<Value>| all.into_iter().find(|x| x["round"] == 2).unwrap();
    let before = final_match(matches(&app, &cup.tournament_id).await);
    assert!(
        !seats(&before, &p1(&m)) && !seats(&before, &p2(&m)),
        "progression ran past a pending result review: {before:?}"
    );

    // Resolving the review resumes progression with the ruled result.
    app.post_json(
        &format!("/v1/admin/result-reviews/{review_id}/approve"),
        &json!({ "notes": "Approved after review" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let after = final_match(matches(&app, &cup.tournament_id).await);
    assert!(seats(&after, &p2(&m)), "{after:?}");
    assert!(!seats(&after, &p1(&m)), "{after:?}");
}

// ============================================================================
// 14. TAKING A RESULT BACK
// ============================================================================

/// A rematch ordered on a match of a finished standalone round robin opens
/// the event again: the stage and bracket go back to `active`, exactly as
/// they do when the result is reverted.
#[tokio::test]
async fn rematch_ruling_reopens_a_finished_round_robin() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-rematch-reopens", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    for m in &all {
        confirm_win(&app, &cup, m, &p1(m), (2, 0)).await;
    }
    assert_eq!(
        stages(&app, &cup.tournament_id).await[0]["status"],
        "completed"
    );
    assert_eq!(
        brackets(&app, &cup.tournament_id).await[0]["status"],
        "completed"
    );

    let last = all.last().unwrap();
    let match_id = str_of(last, "id");
    app.post_json_with_token(
        &format!(
            "/v1/tournaments/{}/matches/{match_id}/dispute",
            cup.tournament_id
        ),
        &json!({
            "registration_id": p2(last),
            "reason": "technical_issue",
            "description": "The server crashed twice during the deciding map.",
            "evidence_ids": []
        }),
        &cup.tokens[&p2(last)],
    )
    .await
    .assert_status(StatusCode::CREATED);
    let dispute_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM disputes WHERE match_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    app.post_json(
        &format!("/v1/admin/disputes/{dispute_id}/resolve/rematch"),
        &json!({ "notes": "Server fault confirmed, the match is to be replayed." }),
    )
    .await
    .assert_status(StatusCode::OK);

    let reopened = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(reopened["status"], "ready");
    assert!(reopened["winner_registration_id"].is_null());
    assert_eq!(
        stages(&app, &cup.tournament_id).await[0]["status"],
        "active",
        "a round robin with a match to replay is not finished"
    );
    assert_eq!(
        brackets(&app, &cup.tournament_id).await[0]["status"],
        "active"
    );
    let rows = standings(&app, &cup.tournament_id, &str_of(last, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p1(last))["matches_played"], 1, "{rows:?}");

    // Replaying it finishes the event again.
    confirm_win(&app, &cup, last, &p2(last), (2, 1)).await;
    assert_eq!(
        stages(&app, &cup.tournament_id).await[0]["status"],
        "completed"
    );
}

/// Reverting a forfeit clears its walkover score with it: the match is open
/// for play again and used to sit there still showing 2–0. A *played*
/// result's score is kept across a revert, because revert → reapply ("same
/// series, other winner") re-records the winner over it.
#[tokio::test]
async fn reverted_forfeit_does_not_keep_its_walkover_score() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-revert-score", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let score = |m: &Value| {
        (
            m["participant1_score"].as_i64().unwrap(),
            m["participant2_score"].as_i64().unwrap(),
        )
    };
    let revert = |id: String| {
        let app = &app;
        async move {
            app.post_auth(&format!("/v1/admin/matches/{id}/progression/revert"))
                .await
                .assert_status(StatusCode::OK);
        }
    };

    let forfeited = &all[0];
    let forfeited_id = str_of(forfeited, "id");
    admin_forfeit(&app, &cup, &forfeited_id, &p2(forfeited)).await;
    assert_eq!(
        score(&get_match(&app, &cup.tournament_id, &forfeited_id).await),
        (2, 0)
    );
    revert(forfeited_id.clone()).await;
    let reverted = get_match(&app, &cup.tournament_id, &forfeited_id).await;
    assert_eq!(reverted["status"], "awaiting_result");
    assert!(reverted["winner_registration_id"].is_null());
    assert_eq!(score(&reverted), (0, 0), "{reverted:?}");

    // A played result: reverted, then reapplied for the other side.
    let played = &all[1];
    let played_id = str_of(played, "id");
    confirm_win(&app, &cup, played, &p1(played), (2, 1)).await;
    revert(played_id.clone()).await;
    let reverted = get_match(&app, &cup.tournament_id, &played_id).await;
    assert_eq!(reverted["status"], "awaiting_result");
    assert_eq!(score(&reverted), (2, 1));
    confirm_win(&app, &cup, played, &p1(played), (2, 1)).await;
    app.post_json(
        &format!("/v1/admin/matches/{played_id}/progression/reapply"),
        &json!({ "new_winner_registration_id": p2(played) }),
    )
    .await
    .assert_status(StatusCode::OK);
    let reapplied = get_match(&app, &cup.tournament_id, &played_id).await;
    assert_eq!(reapplied["status"], "completed");
    assert_eq!(reapplied["winner_registration_id"], p2(played).as_str());
    assert_eq!(score(&reapplied), (2, 1));
}

// ============================================================================
// 15. A STAGE DOES NOT ADVANCE PAST A RESULT UNDER REVIEW
// ============================================================================

/// Put `m` in the state the confirm flow leaves when demo validation flags
/// the result: claim confirmed, match `completed` for participant 1, a
/// pending admin review on the claim, and no progression run. Returns the
/// review id.
async fn complete_under_review(app: &TestApp, cup: &Cup, m: &Value) -> Uuid {
    let match_uuid = Uuid::parse_str(&str_of(m, "id")).unwrap();
    let claim_id = submit_claim(app, cup, m, &p1(m)).await;
    let claim_uuid = Uuid::parse_str(&claim_id).unwrap();
    sqlx::query(
        "UPDATE result_claims
         SET status = 'confirmed', confirmed_at = NOW(), confirmed_by_registration_id = $2
         WHERE id = $1",
    )
    .bind(claim_uuid)
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'completed', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 2, participant2_score = 1, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(match_uuid)
    .bind(Uuid::parse_str(&p1(m)).unwrap())
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO result_reviews
             (result_claim_id, match_id, score_mismatch, status,
              captain1_registration_id, captain2_registration_id)
         VALUES ($1, $2, true, 'pending_admin_review', $3, $4)
         RETURNING id",
    )
    .bind(claim_uuid)
    .bind(match_uuid)
    .bind(Uuid::parse_str(&p1(m)).unwrap())
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap()
}

/// One group's last result is under review when the other group's last
/// result is confirmed the normal way. Every match row is `completed`, but
/// the playoffs must not be seeded from a result nobody has signed off.
/// Approving the review then advances the stage.
#[tokio::test]
async fn stage_waits_for_a_result_under_review_then_advances_on_approval() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-review-gate", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());

    let review_id = complete_under_review(&app, &cup, &a[0]).await;
    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;

    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(
        (st[0]["status"].as_str(), st[1]["status"].as_str()),
        (Some("active"), Some("pending")),
        "playoffs were seeded past a pending result review"
    );
    assert_eq!(brackets(&app, &cup.tournament_id).await.len(), 2);

    app.post_json(
        &format!("/v1/admin/result-reviews/{review_id}/approve"),
        &json!({ "notes": "Approved after review" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(
        (st[0]["status"].as_str(), st[1]["status"].as_str()),
        (Some("completed"), Some("active")),
        "approving the review releases the stage: {st:?}"
    );
    let mut expected = vec![p1(&a[0]), p1(&b[0])];
    expected.sort();
    assert_eq!(
        playoff_participants(
            &matches(&app, &cup.tournament_id).await,
            &brackets(&app, &cup.tournament_id).await
        ),
        expected
    );
}

/// The lifecycle's finished-stage sweep applies the same rule: a stage whose
/// last result is under review is not "settled", however long it sits. If
/// the review is then rejected the match reopens and the stage stays open.
#[tokio::test]
async fn lifecycle_sweep_does_not_advance_past_a_result_under_review() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-review-sweep", 4, 2, 1).await;
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());

    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;
    let review_id = complete_under_review(&app, &cup, &a[0]).await;
    make_stage_idle(&app, &cup).await;

    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 0, "{summary:?}");
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "active", "{st:?}");
    assert_eq!(st[1]["status"], "pending", "{st:?}");
    assert_eq!(brackets(&app, &cup.tournament_id).await.len(), 2);

    app.post_json(
        &format!("/v1/admin/result-reviews/{review_id}/reject"),
        &json!({ "notes": "Demo does not match the claimed result" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let reopened = get_match(&app, &cup.tournament_id, &str_of(&a[0], "id")).await;
    assert_eq!(reopened["status"], "in_progress");
    make_stage_idle(&app, &cup).await;
    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 0, "{summary:?}");
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "active", "{st:?}");
    assert_eq!(st[1]["status"], "pending", "{st:?}");
}

// ============================================================================
// 16. A FORFEIT IS NOT RE-RECORDED AS A PLAYED RESULT
// ============================================================================

/// Reapplying a forfeit for the other side used to revert it (clearing the
/// walkover score) and record a `completed` 0–0 with a winner: three points
/// and no game wins. Like the admin score override, reapply now refuses a
/// forfeit; the supported route is revert, then forfeit the other side.
#[tokio::test]
async fn reapply_refuses_a_forfeit() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-reapply-forfeit", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let match_id = str_of(m, "id");
    admin_forfeit(&app, &cup, &match_id, &p2(m)).await;

    let refused = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p2(m) }),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(refused.text().contains("forfeit"), "{}", refused.text());
    let refused = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": p2(m),
                "loser_registration_id": p1(m)
            }),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );

    // Untouched: still the walkover for participant 1.
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "forfeit");
    assert_eq!(after["winner_registration_id"], p1(m).as_str());
    assert_eq!(after["participant1_score"], 2);
    assert_eq!(after["participant2_score"], 0);
    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p1(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p2(m))["points"], 0, "{rows:?}");

    // The supported correction: revert, then forfeit the other side.
    app.post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await
        .assert_status(StatusCode::OK);
    admin_forfeit(&app, &cup, &match_id, &p1(m)).await;
    let corrected = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(corrected["status"], "forfeit");
    assert_eq!(corrected["winner_registration_id"], p2(m).as_str());
    assert_eq!(corrected["participant1_score"], 0);
    assert_eq!(corrected["participant2_score"], 2);
    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p1(m))["points"], 0, "{rows:?}");
}

// ============================================================================
// 17. AN INTERRUPTED FORFEIT CAN BE FINISHED
// ============================================================================

/// The forfeit's match write landed but nothing after it did (the record
/// insert failed, or the process died): the match is `forfeit`, there is no
/// forfeit record and the winner was never advanced. Retrying the same
/// forfeit must finish the job instead of being refused because the match
/// is already terminal.
#[tokio::test]
async fn interrupted_forfeit_is_completed_by_a_retry() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-forfeit-retry", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = all.iter().find(|m| m["round"] == 1).unwrap().clone();
    let match_id = str_of(&m, "id");
    let match_uuid = Uuid::parse_str(&match_id).unwrap();

    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'forfeit', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 2, participant2_score = 0, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(match_uuid)
    .bind(Uuid::parse_str(&p1(&m)).unwrap())
    .bind(Uuid::parse_str(&p2(&m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    let final_match = |all: Vec<Value>| all.into_iter().find(|x| x["round"] == 2).unwrap();
    assert!(!seats(
        &final_match(matches(&app, &cup.tournament_id).await),
        &p1(&m)
    ));

    // The retry (same side) completes it.
    let body = admin_forfeit(&app, &cup, &match_id, &p2(&m)).await;
    assert_eq!(body["data"]["progression_triggered"], true, "{body}");
    let records: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM forfeit_records WHERE match_id = $1")
            .bind(match_uuid)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(records, 1);
    assert!(
        seats(
            &final_match(matches(&app, &cup.tournament_id).await),
            &p1(&m)
        ),
        "the walkover winner was advanced by the retry"
    );

    // A forfeit that is fully recorded is still refused, as is the other side.
    for reg in [p2(&m), p1(&m)] {
        let again = app
            .post_json(
                &format!(
                    "/v1/admin/tournaments/{}/matches/{match_id}/forfeit",
                    cup.tournament_id
                ),
                &json!({
                    "forfeiting_registration_id": reg,
                    "forfeit_type": "no_show",
                    "reason": "Did not turn up to the match"
                }),
            )
            .await;
        assert_eq!(again.status, StatusCode::BAD_REQUEST, "{}", again.text());
    }
}

// ============================================================================
// 18. A STAGE THAT CANNOT ADVANCE IS NOT RETRIED FOREVER
// ============================================================================

/// Every group match is settled but the playoffs cannot be seeded (one whole
/// group has been disqualified, leaving a single finisher). The sweep must
/// not flip the stage `active → completed → active` on each attempt, and it
/// must stop after three attempts — until something about the stage's
/// matches changes, at which point it tries again.
#[tokio::test]
async fn unadvanceable_stage_is_not_thrashed_or_retried_forever() {
    let app = TestApp::new().await;
    let cup = groups(&app, "rr-stalled-stage", 4, 2, 1).await;
    let tournament = Uuid::parse_str(&cup.tournament_id).unwrap();
    let all = matches(&app, &cup.tournament_id).await;
    let gb = group_brackets(&brackets(&app, &cup.tournament_id).await);
    let a = bracket_matches(&all, gb[0]["id"].as_str().unwrap());
    let b = bracket_matches(&all, gb[1]["id"].as_str().unwrap());
    confirm_win(&app, &cup, &b[0], &p1(&b[0]), (2, 0)).await;

    // Group A: decided, never settled, and both of its players are out.
    let m = &a[0];
    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'forfeit', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 2, participant2_score = 0, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&str_of(m, "id")).unwrap())
    .bind(Uuid::parse_str(&p1(m)).unwrap())
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE tournament_registrations SET status = 'disqualified' WHERE id = ANY($1)")
        .bind(vec![
            Uuid::parse_str(&p1(m)).unwrap(),
            Uuid::parse_str(&p2(m)).unwrap(),
        ])
        .execute(app.pool())
        .await
        .unwrap();

    // Log every stage status change.
    sqlx::raw_sql(
        r"
        CREATE TABLE test_stage_flips (stage_id UUID, old_status TEXT, new_status TEXT);
        CREATE FUNCTION test_record_stage_flip() RETURNS trigger AS $$
        BEGIN
            INSERT INTO test_stage_flips VALUES (NEW.id, OLD.status, NEW.status);
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER test_record_stage_flip AFTER UPDATE ON tournament_stages
            FOR EACH ROW WHEN (NEW.status <> OLD.status)
            EXECUTE FUNCTION test_record_stage_flip();
        ",
    )
    .execute(app.pool())
    .await
    .unwrap();
    // Age the last attempt past the idle threshold, keeping it newer than
    // the (hour-old) matches.
    let age_attempt = |minutes: i32| {
        let app = &app;
        async move {
            sqlx::query(
                "UPDATE tournament_stages
                 SET settlement_attempted_at = NOW() - ($2 * INTERVAL '1 minute')
                 WHERE tournament_id = $1 AND settlement_attempted_at IS NOT NULL",
            )
            .bind(tournament)
            .bind(f64::from(minutes))
            .execute(app.pool())
            .await
            .unwrap();
        }
    };
    let attempts = || {
        let app = &app;
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT settlement_attempts FROM tournament_stages
                 WHERE tournament_id = $1 AND stage_order = 1",
            )
            .bind(tournament)
            .fetch_one(app.pool())
            .await
            .unwrap()
        }
    };

    make_stage_idle(&app, &cup).await;
    for attempt in 1..=3 {
        let summary = run_lifecycle(&app).await;
        assert_eq!(summary.stages_advanced, 0, "{summary:?}");
        assert_eq!(attempts().await, attempt, "{summary:?}");
        assert_eq!(
            summary.stages_stalled,
            u32::from(attempt == 3),
            "attempt {attempt}: {summary:?}"
        );
        // Straight away: not retried inside the idle period.
        let summary = run_lifecycle(&app).await;
        assert_eq!(attempts().await, attempt, "{summary:?}");
        age_attempt(30).await;
    }

    // Given up: no fourth attempt however long it waits.
    let summary = run_lifecycle(&app).await;
    assert_eq!(attempts().await, 3, "{summary:?}");
    assert_eq!(summary.stages_stalled, 0, "{summary:?}");
    assert_eq!(summary.errors, 0, "{summary:?}");

    let flips: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_stage_flips")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(
        flips, 0,
        "a failed advancement must not flip the stage status"
    );
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "active");
    assert_eq!(st[1]["status"], "pending");

    // An admin reinstates a player; the match change re-arms the sweep.
    sqlx::query("UPDATE tournament_registrations SET status = 'approved' WHERE id = $1")
        .bind(Uuid::parse_str(&p1(m)).unwrap())
        .execute(app.pool())
        .await
        .unwrap();
    age_attempt(120).await;
    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.stages_advanced, 1, "{summary:?}");
    let st = stages(&app, &cup.tournament_id).await;
    assert_eq!(st[0]["status"], "completed", "{st:?}");
    assert_eq!(st[1]["status"], "active", "{st:?}");
}

// ============================================================================
// 19. RULINGS ON A MATCH UNDER REVIEW
// ============================================================================

async fn insert_pending_review(app: &TestApp, m: &Value, claim_id: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO result_reviews
             (result_claim_id, match_id, score_mismatch, status,
              captain1_registration_id, captain2_registration_id)
         VALUES ($1, $2, true, 'pending_admin_review', $3, $4)
         RETURNING id",
    )
    .bind(Uuid::parse_str(claim_id).unwrap())
    .bind(Uuid::parse_str(&str_of(m, "id")).unwrap())
    .bind(Uuid::parse_str(&p1(m)).unwrap())
    .bind(Uuid::parse_str(&p2(m)).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap()
}

async fn dispute_and_adjust_to_p2_win(app: &TestApp, cup: &Cup, m: &Value, claim_id: &str) {
    let match_id = str_of(m, "id");
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
}

/// A ruling on a round-robin match that is under review defers progression,
/// but the standings are derived from the match rows and must follow the
/// ruling straight away.
#[tokio::test]
async fn ruling_on_a_match_under_review_still_recomputes_standings() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-review-standings", 3).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = &all[0];
    let claim_id = submit_claim(&app, &cup, m, &p1(m)).await;
    insert_pending_review(&app, m, &claim_id).await;
    dispute_and_adjust_to_p2_win(&app, &cup, m, &claim_id).await;

    let rows = standings(&app, &cup.tournament_id, &str_of(m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p2(m))["points"], 3, "{rows:?}");
    assert_eq!(row_for(&rows, &p2(m))["matches_won"], 1, "{rows:?}");
    assert_eq!(row_for(&rows, &p1(m))["points"], 0, "{rows:?}");
}

/// Only a review of the match's current result holds progression back. A
/// review still pending on an earlier, superseded claim must not.
#[tokio::test]
async fn stale_review_on_an_earlier_claim_does_not_defer_a_ruling() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-stale-review", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let m = all.iter().find(|m| m["round"] == 1).unwrap().clone();
    let match_id = str_of(&m, "id");

    // First claim, with a review left pending on it ...
    let first_claim = submit_claim(&app, &cup, &m, &p1(&m)).await;
    insert_pending_review(&app, &m, &first_claim).await;
    // ... then replaced by a fresh claim, which is the one that gets ruled on.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1(&m),
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
            &cup.tokens[&p1(&m)],
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let second_claim = response.json::<Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_claim, second_claim);
    dispute_and_adjust_to_p2_win(&app, &cup, &m, &second_claim).await;

    let final_match = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    assert!(
        seats(&final_match, &p2(&m)),
        "a stale review held the ruling's winner back: {final_match:?}"
    );
}

// ============================================================================
// 20. MANUAL PROGRESSION OPENS THE MATCHES IT FEEDS
// ============================================================================

/// `progression/process` on a winners-bracket match drops its loser into the
/// losers bracket — a different bracket from the one the endpoint scanned
/// for newly ready matches. That losers-bracket match must be opened once
/// both its participants are seated, and walked over if one of them has
/// left the tournament, exactly as the completion saga does.
#[tokio::test]
async fn manual_progression_opens_and_settles_cross_bracket_matches() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "de-manual-progress",
        "double_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let openers: Vec<Value> = all
        .iter()
        .filter(|m| {
            m["participant1_registration_id"].is_string()
                && m["participant2_registration_id"].is_string()
        })
        .cloned()
        .collect();
    assert_eq!(openers.len(), 2, "{all:?}");

    // Record both openers by hand (participant 1 wins), through the manual
    // progression endpoint only.
    let process = |m: Value| {
        let app = &app;
        let cup = &cup;
        async move {
            let match_id = str_of(&m, "id");
            transition_match_to_in_progress(app, &cup.tournament_id, &match_id).await;
            app.post_json(
                &format!("/v1/admin/matches/{match_id}/progression/process"),
                &json!({
                    "winner_registration_id": p1(&m),
                    "loser_registration_id": p2(&m)
                }),
            )
            .await
            .assert_status(StatusCode::OK);
        }
    };
    process(openers[0].clone()).await;

    // The first loser now waits in the losers bracket — and is disqualified.
    let departed = p2(&openers[0]);
    app.post_json(
        &format!(
            "/v1/admin/tournaments/{}/registrations/{departed}/disqualify",
            cup.tournament_id
        ),
        &json!({ "reason": "Cheating confirmed by the admin team" }),
    )
    .await
    .assert_status(StatusCode::OK);

    process(openers[1].clone()).await;
    let other_loser = p2(&openers[1]);

    let all = matches(&app, &cup.tournament_id).await;
    let losers_match = all
        .iter()
        .find(|m| m["id"] != openers[0]["id"] && seats(m, &departed))
        .expect("the loser was dropped into the losers bracket");
    assert!(seats(losers_match, &other_loser), "{losers_match:?}");
    assert_eq!(
        losers_match["status"], "forfeit",
        "the losers-bracket match was opened and walked over: {losers_match:?}"
    );
    assert_eq!(losers_match["winner_registration_id"], other_loser.as_str());
    // The winners' final was opened too.
    let winners_final = all
        .iter()
        .find(|m| seats(m, &p1(&openers[0])) && seats(m, &p1(&openers[1])))
        .expect("both winners meet");
    assert_eq!(winners_final["status"], "ready", "{winners_final:?}");
}

// ============================================================================
// 19. CONCURRENT STANDINGS RECOMPUTES DO NOT DEADLOCK
// ============================================================================

/// Every settlement re-derives its bracket's standings (`recompute_bracket`:
/// derive, then re-rank, in one transaction). Two of them on the same bracket
/// at once used to lock the standings rows in whatever order each plan
/// visited them, so they could deadlock; Postgres aborted one after
/// `deadlock_timeout` and the request behind it returned a 500 — the
/// intermittent failure of `concurrent_stage_advancement_seeds_playoffs_once`.
/// Many concurrent recomputes of one bracket must all succeed.
#[tokio::test]
async fn concurrent_standings_recomputes_do_not_deadlock() {
    use portal_domain::repositories::TournamentStandingsRepository;

    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-recompute-race", 8).await;
    let all = matches(&app, &cup.tournament_id).await;
    let bracket_id: portal_core::TournamentBracketId =
        str_of(&all[0], "bracket_id").parse().unwrap();
    // Decide half the matches so the derive and the re-rank both move rows.
    for m in all.iter().step_by(2) {
        sqlx::query(
            "UPDATE tournament_matches SET participant1_score = 2, participant2_score = 1,
                    winner_registration_id = participant1_registration_id,
                    loser_registration_id = participant2_registration_id,
                    status = 'completed', completed_at = NOW()
             WHERE id = $1",
        )
        .bind(Uuid::parse_str(&str_of(m, "id")).unwrap())
        .execute(app.pool())
        .await
        .unwrap();
    }

    let repo = portal_db::PgTournamentStandingsRepository::new(app.pool().clone());
    for round in 0..15 {
        let handles: Vec<_> = (0..12)
            .map(|_| {
                let repo = repo.clone();
                tokio::spawn(async move { repo.recompute_bracket(bracket_id).await })
            })
            .collect();
        for h in handles {
            let result = h.await.unwrap();
            assert!(
                result.is_ok(),
                "round {round}: a concurrent recompute failed: {result:?}"
            );
        }
    }
}

// ============================================================================
// 20. A WRONG FORFEIT CAN BE CORRECTED IN ELIMINATION FORMATS
// ============================================================================

/// `(forfeiting registration, superseded?)` for every forfeit record of a
/// match, oldest first.
async fn forfeit_records_of(app: &TestApp, match_id: &str) -> Vec<(String, bool)> {
    let rows: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT forfeiting_registration_id, superseded_at IS NOT NULL
         FROM forfeit_records WHERE match_id = $1 ORDER BY forfeited_at, id",
    )
    .bind(Uuid::parse_str(match_id).unwrap())
    .fetch_all(app.pool())
    .await
    .unwrap();
    rows.into_iter().map(|(r, s)| (r.to_string(), s)).collect()
}

/// Revert a forfeit and check it reopened the match: no winner, no walkover
/// score, open for a result again.
async fn revert_and_expect_reopened(app: &TestApp, cup: &Cup, match_id: &str) {
    app.post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await
        .assert_status(StatusCode::OK);
    let reopened = get_match(app, &cup.tournament_id, match_id).await;
    assert_eq!(reopened["status"], "awaiting_result", "{reopened:?}");
    assert!(reopened["winner_registration_id"].is_null(), "{reopened:?}");
    assert!(reopened["loser_registration_id"].is_null(), "{reopened:?}");
    assert_eq!(reopened["participant1_score"], 0, "{reopened:?}");
    assert_eq!(reopened["participant2_score"], 0, "{reopened:?}");
}

/// Single elimination, the final already half-seated by the other semi: the
/// admin forfeits the wrong side of a semi and the wrong team walks into the
/// final. Reapply refuses a forfeit and tells the admin to revert and forfeit
/// the other side — so that has to work end to end: the revert reopens the
/// semi (it used to stay `forfeit`, which nothing can forfeit again or move
/// out of), the forfeit for the other side is accepted, and the right team
/// ends up in the final.
#[tokio::test]
async fn wrong_forfeit_is_corrected_in_single_elimination() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-fix-forfeit", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let semis: Vec<Value> = all.iter().filter(|m| m["round"] == 1).cloned().collect();
    let (m, other) = (&semis[0], &semis[1]);
    confirm_win(&app, &cup, other, &p1(other), (2, 0)).await;
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    let final_of = |all: Vec<Value>| all.into_iter().find(|x| x["round"] == 2).unwrap();

    // The wrong side: p2 forfeits, p1 walks over into the final.
    admin_forfeit(&app, &cup, &match_id, &p2(m)).await;
    let fin = final_of(matches(&app, &cup.tournament_id).await);
    assert!(seats(&fin, &p1(m)), "{fin:?}");
    assert_eq!(fin["status"], "ready", "{fin:?}");

    revert_and_expect_reopened(&app, &cup, &match_id).await;
    let fin = final_of(matches(&app, &cup.tournament_id).await);
    assert!(!seats(&fin, &p1(m)), "{fin:?}");
    assert_eq!(fin["status"], "pending", "{fin:?}");

    // The correction: forfeit the other side.
    admin_forfeit(&app, &cup, &match_id, &p1(m)).await;
    let corrected = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(corrected["status"], "forfeit", "{corrected:?}");
    assert_eq!(corrected["winner_registration_id"], p2(m).as_str());
    let fin = final_of(matches(&app, &cup.tournament_id).await);
    assert!(
        seats(&fin, &p2(m)),
        "the right team reaches the final: {fin:?}"
    );
    assert!(!seats(&fin, &p1(m)), "{fin:?}");
    assert!(seats(&fin, &p1(other)), "{fin:?}");
    assert_eq!(fin["status"], "ready", "{fin:?}");

    // The audit trail keeps the reverted forfeit, superseded.
    assert_eq!(
        forfeit_records_of(&app, &match_id).await,
        vec![(p2(m), true), (p1(m), false)]
    );
}

/// Double elimination: the wrongly forfeited side was also routed into the
/// losers bracket. Revert takes both out (winners final and losers match);
/// forfeiting the other side then routes each team to the right bracket.
#[tokio::test]
async fn wrong_forfeit_is_corrected_in_double_elimination() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "de-fix-forfeit", "double_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let openers: Vec<Value> = all
        .iter()
        .filter(|m| {
            m["participant1_registration_id"].is_string()
                && m["participant2_registration_id"].is_string()
        })
        .cloned()
        .collect();
    assert_eq!(openers.len(), 2, "{all:?}");
    let (m, other) = (&openers[0], &openers[1]);
    confirm_win(&app, &cup, other, &p1(other), (2, 0)).await;
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;

    // Where each of m's outputs lands.
    let (winner_target, loser_target): (Uuid, Uuid) = sqlx::query_as(
        "SELECT winner_progresses_to, loser_progresses_to FROM tournament_matches WHERE id = $1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    let (winner_target, loser_target) = (winner_target.to_string(), loser_target.to_string());
    let at = |all: &[Value], id: &str| all.iter().find(|x| x["id"] == id).unwrap().clone();

    admin_forfeit(&app, &cup, &match_id, &p2(m)).await;
    let all = matches(&app, &cup.tournament_id).await;
    assert!(seats(&at(&all, &winner_target), &p1(m)));
    assert!(seats(&at(&all, &loser_target), &p2(m)));

    revert_and_expect_reopened(&app, &cup, &match_id).await;
    let all = matches(&app, &cup.tournament_id).await;
    for target in [&winner_target, &loser_target] {
        let t = at(&all, target);
        assert!(!seats(&t, &p1(m)) && !seats(&t, &p2(m)), "{t:?}");
        assert_eq!(t["status"], "pending", "{t:?}");
    }

    admin_forfeit(&app, &cup, &match_id, &p1(m)).await;
    let all = matches(&app, &cup.tournament_id).await;
    let wf = at(&all, &winner_target);
    let lb = at(&all, &loser_target);
    assert!(seats(&wf, &p2(m)) && !seats(&wf, &p1(m)), "{wf:?}");
    assert!(seats(&lb, &p1(m)) && !seats(&lb, &p2(m)), "{lb:?}");
    assert_eq!(wf["status"], "ready", "{wf:?}");
    assert_eq!(lb["status"], "ready", "{lb:?}");
    assert_eq!(
        forfeit_records_of(&app, &match_id).await,
        vec![(p2(m), true), (p1(m), false)]
    );
}

// ============================================================================
// 21. EVERY FORFEIT IS RECORDED; A SETTLED ONE IS NEVER RE-COMPLETED
// ============================================================================

async fn captain_forfeit(
    app: &TestApp,
    cup: &Cup,
    match_id: &str,
    reg: &str,
) -> (StatusCode, String) {
    let r = app
        .post_json_with_token(
            &format!(
                "/v1/tournaments/{}/matches/{match_id}/forfeit",
                cup.tournament_id
            ),
            &json!({ "registration_id": reg }),
            &cup.tokens[reg],
        )
        .await;
    (r.status, r.text())
}

async fn raw_admin_forfeit(
    app: &TestApp,
    cup: &Cup,
    match_id: &str,
    reg: &str,
    forfeit_type: &str,
) -> (StatusCode, String) {
    let r = app
        .post_json(
            &format!(
                "/v1/admin/tournaments/{}/matches/{match_id}/forfeit",
                cup.tournament_id
            ),
            &json!({
                "forfeiting_registration_id": reg,
                "forfeit_type": forfeit_type,
                "reason": "Ruled out by the tournament admin"
            }),
        )
        .await;
    (r.status, r.text())
}

/// A captain's own forfeit used to write no forfeit record, so a later admin
/// forfeit for the same side took the settled match for an interrupted
/// forfeit: it returned 200, wrote a `disqualification` record onto it and
/// re-ran settlement. The captain's forfeit is now recorded, and an admin
/// forfeit of it — either side — is refused without touching anything.
#[tokio::test]
async fn admin_forfeit_after_a_captain_forfeit_is_refused() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "se-capt-then-admin",
        "single_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let m = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 1)
        .unwrap();
    let match_id = str_of(&m, "id");
    let match_uuid = Uuid::parse_str(&match_id).unwrap();
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    let forfeiter = p1(&m);

    let (st, text) = captain_forfeit(&app, &cup, &match_id, &forfeiter).await;
    assert_eq!(st, StatusCode::OK, "{text}");
    let records = || async {
        sqlx::query_as::<_, (String, String, bool)>(
            "SELECT forfeiting_registration_id::text, forfeit_type, triggered_by_system
             FROM forfeit_records WHERE match_id = $1",
        )
        .bind(match_uuid)
        .fetch_all(app.pool())
        .await
        .unwrap()
    };
    let expected = vec![(forfeiter.clone(), "withdrawal".to_string(), false)];
    assert_eq!(
        records().await,
        expected,
        "the captain's forfeit is recorded"
    );
    let sagas = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM saga_executions WHERE match_id = $1")
            .bind(match_uuid)
            .fetch_one(app.pool())
            .await
            .unwrap()
    };
    let sagas_before = sagas().await;
    assert_eq!(sagas_before, 1, "settled once");
    let final_before = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    assert!(seats(&final_before, &p2(&m)), "{final_before:?}");

    for reg in [forfeiter.clone(), p2(&m)] {
        let (st, text) = raw_admin_forfeit(&app, &cup, &match_id, &reg, "disqualification").await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{text}");
    }
    assert_eq!(records().await, expected, "no record was added");
    assert_eq!(sagas().await, sagas_before, "settlement did not run again");
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "forfeit");
    assert_eq!(after["winner_registration_id"], p2(&m).as_str());
    let final_after = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    assert_eq!(
        (
            &final_after["participant1_registration_id"],
            &final_after["participant2_registration_id"],
            &final_after["status"]
        ),
        (
            &final_before["participant1_registration_id"],
            &final_before["participant2_registration_id"],
            &final_before["status"]
        )
    );
}

/// The captain's forfeit died after its match write (no record, never
/// settled). The captain's retry finishes it: records it and advances the
/// walkover winner, rather than being refused as already forfeited.
#[tokio::test]
async fn interrupted_captain_forfeit_is_finished_by_its_retry() {
    let app = TestApp::new().await;
    let cup = create_cup(&app, "se-capt-retry", "single_elimination", 4, json!({})).await;
    start(&app, &cup).await;
    let m = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 1)
        .unwrap();
    let match_id = str_of(&m, "id");
    let match_uuid = Uuid::parse_str(&match_id).unwrap();
    sqlx::query(
        "UPDATE tournament_matches
         SET status = 'forfeit', winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = 0, participant2_score = 2, completed_at = NOW()
         WHERE id = $1",
    )
    .bind(match_uuid)
    .bind(Uuid::parse_str(&p2(&m)).unwrap())
    .bind(Uuid::parse_str(&p1(&m)).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let (st, text) = captain_forfeit(&app, &cup, &match_id, &p1(&m)).await;
    assert_eq!(st, StatusCode::OK, "{text}");
    let records: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM forfeit_records WHERE match_id = $1 AND superseded_at IS NULL",
    )
    .bind(match_uuid)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(records, 1);
    let fin = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    assert!(
        seats(&fin, &p2(&m)),
        "the walkover winner was advanced: {fin:?}"
    );

    // Now fully recorded: a second retry is refused.
    let (st, text) = captain_forfeit(&app, &cup, &match_id, &p1(&m)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{text}");
}

// ============================================================================
// 22. A REFUSED REVERT CHANGES NOTHING
// ============================================================================

/// `(status, winner, participant1, participant2)` of a match, for comparing
/// before and after a refused revert.
fn shape(m: &Value) -> (Value, Value, Value, Value) {
    (
        m["status"].clone(),
        m["winner_registration_id"].clone(),
        m["participant1_registration_id"].clone(),
        m["participant2_registration_id"].clone(),
    )
}

async fn shapes(app: &TestApp, cup: &Cup) -> Vec<(String, (Value, Value, Value, Value))> {
    let mut out: Vec<_> = matches(app, &cup.tournament_id)
        .await
        .iter()
        .map(|m| (str_of(m, "id"), shape(m)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

async fn refused_revert(app: &TestApp, match_id: &str) {
    let r = app
        .post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
}

/// Single elimination: the forfeit's walkover winner has already played the
/// final. The revert is refused and nothing moves.
#[tokio::test]
async fn refused_forfeit_revert_changes_nothing_in_single_elimination() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "se-revert-refused",
        "single_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let semis: Vec<Value> = all.iter().filter(|m| m["round"] == 1).cloned().collect();
    let (m, other) = (&semis[0], &semis[1]);
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p2(m)).await;
    confirm_win(&app, &cup, other, &p1(other), (2, 0)).await;
    let fin = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    confirm_win(&app, &cup, &fin, &p1(m), (2, 1)).await;

    let before = shapes(&app, &cup).await;
    refused_revert(&app, &match_id).await;
    assert_eq!(shapes(&app, &cup).await, before);
    assert_eq!(
        forfeit_records_of(&app, &match_id).await,
        vec![(p2(m), false)]
    );
}

/// Double elimination: the winners-bracket target is untouched but the
/// losers-bracket match the loser was routed into has been played. The
/// revert used to withdraw the winner first and only then refuse, leaving the
/// next round short a participant while the match stayed decided.
#[tokio::test]
async fn refused_forfeit_revert_changes_nothing_in_double_elimination() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "de-revert-refused",
        "double_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let all = matches(&app, &cup.tournament_id).await;
    let openers: Vec<Value> = all
        .iter()
        .filter(|m| {
            m["participant1_registration_id"].is_string()
                && m["participant2_registration_id"].is_string()
        })
        .cloned()
        .collect();
    let (m, other) = (&openers[0], &openers[1]);
    let match_id = str_of(m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p2(m)).await;
    confirm_win(&app, &cup, other, &p1(other), (2, 0)).await;

    // Play the losers-bracket match m's loser was routed into.
    let loser_target: Uuid =
        sqlx::query_scalar("SELECT loser_progresses_to FROM tournament_matches WHERE id = $1")
            .bind(Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    let lb = get_match(&app, &cup.tournament_id, &loser_target.to_string()).await;
    assert!(seats(&lb, &p2(m)), "{lb:?}");
    confirm_win(&app, &cup, &lb, &p2(m), (2, 0)).await;

    let before = shapes(&app, &cup).await;
    refused_revert(&app, &match_id).await;
    assert_eq!(shapes(&app, &cup).await, before);
    assert_eq!(
        forfeit_records_of(&app, &match_id).await,
        vec![(p2(m), false)]
    );
}

// ============================================================================
// 23. A REVERTED RESULT IS NOT RESURRECTED BY ITS OWN COMPLETION SAGA
// ============================================================================

async fn stage_failed_saga(app: &TestApp, match_id: &str) {
    let n = sqlx::query(
        "UPDATE saga_executions SET status = 'failed', retry_count = 0,
                last_error = 'simulated failure'
         WHERE match_id = $1 AND saga_type = 'match_completion'",
    )
    .bind(Uuid::parse_str(match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap()
    .rows_affected();
    assert!(n > 0, "no completion saga to stage");
}

async fn saga_statuses(app: &TestApp, match_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT status FROM saga_executions
         WHERE match_id = $1 AND saga_type = 'match_completion' ORDER BY created_at",
    )
    .bind(Uuid::parse_str(match_id).unwrap())
    .fetch_all(app.pool())
    .await
    .unwrap()
}

/// A forfeit whose completion saga failed is reverted. The lifecycle pass
/// used to re-drive that failed saga, which completed the reopened match
/// with the reverted result (p1 2-0) and seated p1 in the final again.
#[tokio::test]
async fn failed_saga_does_not_resurrect_a_reverted_forfeit() {
    let app = TestApp::new().await;
    let cup = create_cup(
        &app,
        "se-revert-failed-saga",
        "single_elimination",
        4,
        json!({}),
    )
    .await;
    start(&app, &cup).await;
    let m = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|m| m["round"] == 1)
        .unwrap();
    let match_id = str_of(&m, "id");
    transition_match_to_in_progress(&app, &cup.tournament_id, &match_id).await;
    admin_forfeit(&app, &cup, &match_id, &p2(&m)).await;
    stage_failed_saga(&app, &match_id).await;

    revert_and_expect_reopened(&app, &cup, &match_id).await;
    assert!(
        saga_statuses(&app, &match_id)
            .await
            .iter()
            .all(|s| s == "compensated"),
        "the revert retires every completion saga of the match"
    );

    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.sagas_redriven, 0, "{summary:?}");
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "awaiting_result", "{after:?}");
    assert!(after["winner_registration_id"].is_null(), "{after:?}");
    let fin = matches(&app, &cup.tournament_id)
        .await
        .into_iter()
        .find(|x| x["round"] == 2)
        .unwrap();
    assert!(!seats(&fin, &p1(&m)), "{fin:?}");
}

/// Defence in depth: a completion saga that fails *after* its result was
/// reverted (so the revert could not retire it) is retired by the re-drive,
/// not run — the saga refuses a result its match no longer carries.
#[tokio::test]
async fn saga_failed_after_a_revert_is_retired_not_redriven() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-stale-saga", 3).await;
    let m = matches(&app, &cup.tournament_id).await[0].clone();
    let match_id = str_of(&m, "id");
    admin_forfeit(&app, &cup, &match_id, &p2(&m)).await;
    revert_and_expect_reopened(&app, &cup, &match_id).await;
    stage_failed_saga(&app, &match_id).await;

    let summary = run_lifecycle(&app).await;
    assert_eq!(summary.sagas_redriven, 0, "{summary:?}");
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "awaiting_result", "{after:?}");
    assert!(after["winner_registration_id"].is_null(), "{after:?}");
    let rows = standings(&app, &cup.tournament_id, &str_of(&m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p1(&m))["points"], 0, "{rows:?}");
    let (retries, max): (i32, i32) = sqlx::query_as(
        "SELECT retry_count, max_retries FROM saga_executions
         WHERE match_id = $1 AND saga_type = 'match_completion'",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert!(retries >= max, "the stale saga is retired: {retries}/{max}");
}

/// A round-robin result under review is reverted before the review is
/// resolved. Approving the review afterwards must not record it again.
#[tokio::test]
async fn approving_a_review_does_not_resurrect_a_reverted_result() {
    let app = TestApp::new().await;
    let cup = round_robin(&app, "rr-review-revert", 3).await;
    let m = matches(&app, &cup.tournament_id).await[0].clone();
    let match_id = str_of(&m, "id");
    let review_id = complete_under_review(&app, &cup, &m).await;
    app.post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await
        .assert_status(StatusCode::OK);

    let _ = app
        .post_json(
            &format!("/v1/admin/result-reviews/{review_id}/approve"),
            &json!({ "notes": "Approved after review" }),
        )
        .await;
    let after = get_match(&app, &cup.tournament_id, &match_id).await;
    assert_eq!(after["status"], "awaiting_result", "{after:?}");
    assert!(after["winner_registration_id"].is_null(), "{after:?}");
    let rows = standings(&app, &cup.tournament_id, &str_of(&m, "bracket_id")).await;
    assert_eq!(row_for(&rows, &p1(&m))["points"], 0, "{rows:?}");
}
