//! Swiss round progression — standalone Swiss and Groups + Playoffs with
//! `group_format = "swiss"`.
//!
//! A Swiss bracket only has its *current* round's matches on disk, so "every
//! match in the bracket is complete" is not the same as "the bracket is
//! complete". These tests pin:
//!
//! * `generate-next-round` advances every Swiss group, not just the first;
//! * the group stage does not hand off to the playoffs until each group has
//!   played its configured number of rounds;
//! * `max_rounds` is honoured for Swiss groups (and clamped to the number of
//!   rounds that can be played without a rematch);
//! * byes in odd-sized Swiss groups rotate per group.

use super::brackets::confirm_win_via_claims;
use super::*;
use std::collections::{HashMap, HashSet};

/// Create, publish and open a tournament of `format` with `format_settings`.
async fn create_tournament(
    app: &TestApp,
    slug: &str,
    format: &str,
    format_settings: serde_json::Value,
) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Swiss Groups {slug}"),
                "slug": slug,
                "format": format,
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "format_settings": format_settings,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 32,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let tournament_id = response.json::<serde_json::Value>()["data"]["id"]
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
    tournament_id
}

/// Register `n` players (each with their own token), seed and start.
/// Returns registration id → bearer token.
async fn register_and_start(
    app: &TestApp,
    tournament_id: &str,
    n: usize,
    slug: &str,
) -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    for i in 1..=n {
        let username = format!("sw_{slug}_{i}");
        let (user_id, player_id) = create_test_player(app, &username).await;
        let reg = insert_test_registration(app, tournament_id, player_id, user_id, &username).await;
        tokens.insert(
            reg,
            create_test_token(user_id, player_id, &username, TEST_JWT_SECRET),
        );
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
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);
    tokens
}

async fn list_matches(app: &TestApp, tournament_id: &str) -> Vec<serde_json::Value> {
    let body: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches?per_page=100"
        ))
        .await
        .json();
    body["data"].as_array().unwrap().clone()
}

async fn list_brackets(app: &TestApp, tournament_id: &str) -> Vec<serde_json::Value> {
    let body: serde_json::Value = app
        .get(&format!("/v1/tournaments/{tournament_id}/brackets"))
        .await
        .json();
    body["data"].as_array().unwrap().clone()
}

async fn stage_statuses(app: &TestApp, tournament_id: &str) -> Vec<String> {
    let body: serde_json::Value = app
        .get(&format!("/v1/tournaments/{tournament_id}/stages"))
        .await
        .json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["status"].as_str().unwrap().to_string())
        .collect()
}

/// Play every unsettled match in `bracket_ids` (participant 1 wins).
async fn play_open_matches(
    app: &TestApp,
    tournament_id: &str,
    bracket_ids: &HashSet<String>,
    tokens: &HashMap<String, String>,
) -> usize {
    let mut played = 0;
    for m in list_matches(app, tournament_id).await {
        if m["status"] == "completed"
            || m["status"] == "cancelled"
            || !bracket_ids.contains(m["bracket_id"].as_str().unwrap())
        {
            continue;
        }
        let p1 = m["participant1_registration_id"].as_str().unwrap();
        let p2 = m["participant2_registration_id"].as_str().unwrap();
        confirm_win_via_claims(
            app,
            tournament_id,
            m["id"].as_str().unwrap(),
            p1,
            &tokens[p1],
            &tokens[p2],
            true,
        )
        .await;
        played += 1;
    }
    played
}

fn swiss_bracket_ids(brackets: &[serde_json::Value]) -> HashSet<String> {
    brackets
        .iter()
        .filter(|b| b["bracket_type"] == "swiss")
        .map(|b| b["id"].as_str().unwrap().to_string())
        .collect()
}

async fn generate_next_round(app: &TestApp, tournament_id: &str) -> crate::common::TestResponse {
    app.post_auth(&format!(
        "/v1/admin/tournaments/{tournament_id}/generate-next-round"
    ))
    .await
}

/// Two Swiss groups of four play the default ceil(log2(4)) = 2 rounds each;
/// round 2 must be generated for *both* groups, and the playoffs must not
/// be seeded until both groups finish round 2.
#[tokio::test]
async fn test_swiss_groups_every_group_advances_and_playoffs_wait_for_final_round() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-advance-all",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "swiss",
            "playoff_format": "single_elimination"
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 8, "adv").await;

    let groups = swiss_bracket_ids(&list_brackets(&app, &tournament_id).await);
    assert_eq!(groups.len(), 2);
    for b in list_brackets(&app, &tournament_id).await {
        assert_eq!(
            b["total_rounds"], 2,
            "default Swiss rounds for a group of 4"
        );
    }

    // Round 1 in both groups.
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        4
    );
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["active", "pending"],
        "round 1 of 2 finished — the group stage is not over yet"
    );
    assert_eq!(
        list_brackets(&app, &tournament_id).await.len(),
        2,
        "no playoff bracket may be seeded after round 1"
    );

    // Round 2 is generated for every group in one call.
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    let new_matches = response.json::<serde_json::Value>()["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(new_matches.len(), 4, "two round-2 matches per group");
    let per_group: HashSet<&str> = new_matches
        .iter()
        .map(|m| m["bracket_id"].as_str().unwrap())
        .collect();
    assert_eq!(per_group.len(), 2, "both groups got a round 2");
    for b in list_brackets(&app, &tournament_id).await {
        assert_eq!(b["current_round"], 2, "{} advanced", b["name"]);
    }

    // Generating again before round 2 is played is refused.
    let response = generate_next_round(&app, &tournament_id).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);

    // Round 2 finishes the groups and seeds the playoffs.
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        4
    );
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"]
    );
    let brackets = list_brackets(&app, &tournament_id).await;
    assert!(brackets.iter().any(|b| b["bracket_type"] == "single_elim"));

    // Nothing left to generate once the Swiss stage is over.
    let response = generate_next_round(&app, &tournament_id).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

/// Groups advance independently: a group whose round is complete can move
/// on while another group is still playing.
#[tokio::test]
async fn test_swiss_groups_advance_independently() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-independent",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "swiss",
            "max_rounds": 3
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 8, "ind").await;

    let brackets = list_brackets(&app, &tournament_id).await;
    let mut sorted = brackets.clone();
    sorted.sort_by_key(|b| b["group_number"].as_i64());
    let group_b: HashSet<String> = [sorted[1]["id"].as_str().unwrap().to_string()].into();

    // Only group B finishes round 1.
    assert_eq!(
        play_open_matches(&app, &tournament_id, &group_b, &tokens).await,
        2
    );
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    let new_matches = response.json::<serde_json::Value>()["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(new_matches.len(), 2);
    assert!(
        new_matches
            .iter()
            .all(|m| m["bracket_id"] == sorted[1]["id"] && m["round"] == 2)
    );
    let after: HashMap<String, i64> = list_brackets(&app, &tournament_id)
        .await
        .iter()
        .map(|b| {
            (
                b["id"].as_str().unwrap().to_string(),
                b["current_round"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(after[sorted[0]["id"].as_str().unwrap()], 1);
    assert_eq!(after[sorted[1]["id"].as_str().unwrap()], 2);

    // Group A catches up: it finishes round 1 while B is mid round 2, so the
    // next call advances A alone.
    let group_a: HashSet<String> = [sorted[0]["id"].as_str().unwrap().to_string()].into();
    assert_eq!(
        play_open_matches(&app, &tournament_id, &group_a, &tokens).await,
        2
    );
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    let new_matches = response.json::<serde_json::Value>()["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(new_matches.len(), 2);
    assert!(
        new_matches
            .iter()
            .all(|m| m["bracket_id"] == sorted[0]["id"] && m["round"] == 2)
    );
    for b in list_brackets(&app, &tournament_id).await {
        assert_eq!(b["current_round"], 2, "{} is on round 2", b["name"]);
    }

    // From here the groups are level: round 2 everywhere, then round 3 for
    // both in one call, and the final round hands over to the playoffs.
    let groups = swiss_bracket_ids(&brackets);
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        4
    );
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["active", "pending"]
    );
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        4
    );
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"]
    );
}

/// A double-forfeited (Cancelled) match settles its round just like a
/// played one. Requiring `Completed` stalled the group — and with it the
/// whole group stage — forever.
#[tokio::test]
async fn test_swiss_group_with_double_forfeit_still_advances() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-double-forfeit",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "swiss",
            "playoff_format": "single_elimination"
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 8, "dfx").await;

    let mut brackets = list_brackets(&app, &tournament_id).await;
    brackets.sort_by_key(|b| b["group_number"].as_i64());
    let group_b_id = brackets[1]["id"].as_str().unwrap().to_string();

    // Double-forfeit one round-1 match in group B.
    let cancelled = list_matches(&app, &tournament_id)
        .await
        .into_iter()
        .find(|m| m["bracket_id"] == group_b_id.as_str())
        .unwrap();
    let cancelled_id = cancelled["id"].as_str().unwrap();
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{cancelled_id}/double-forfeit"),
        &json!({ "reason": "Neither side showed up for the match" }),
    )
    .await
    .assert_status(StatusCode::OK);
    let cancelled_pair: HashSet<String> = [
        cancelled["participant1_registration_id"]
            .as_str()
            .unwrap()
            .to_string(),
        cancelled["participant2_registration_id"]
            .as_str()
            .unwrap()
            .to_string(),
    ]
    .into();

    // Play the rest of round 1; both groups advance in one call.
    let groups = swiss_bracket_ids(&brackets);
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        3
    );
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    let new_matches = response.json::<serde_json::Value>()["data"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(new_matches.len(), 4, "both groups get round 2");

    // The cancelled pair carries no result and no points, and both are
    // still paired in round 2 (just not with each other).
    let standings: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/brackets/{group_b_id}/standings"
        ))
        .await
        .json();
    for row in standings["data"].as_array().unwrap() {
        if cancelled_pair.contains(row["registration_id"].as_str().unwrap()) {
            assert_eq!(row["points"], 0, "{row}");
            assert_eq!(row["matches_played"], 0, "{row}");
        }
    }
    let round2_b: Vec<&serde_json::Value> = new_matches
        .iter()
        .filter(|m| m["bracket_id"] == group_b_id.as_str())
        .collect();
    let paired: HashSet<&str> = round2_b
        .iter()
        .flat_map(|m| {
            [
                m["participant1_registration_id"].as_str().unwrap(),
                m["participant2_registration_id"].as_str().unwrap(),
            ]
        })
        .collect();
    assert_eq!(paired.len(), 4, "all four of group B play round 2");
    for m in &round2_b {
        let pair: HashSet<String> = [
            m["participant1_registration_id"]
                .as_str()
                .unwrap()
                .to_string(),
            m["participant2_registration_id"]
                .as_str()
                .unwrap()
                .to_string(),
        ]
        .into();
        assert_ne!(
            pair, cancelled_pair,
            "the cancelled pairing is not replayed"
        );
    }

    // The final round completes the group stage.
    assert_eq!(
        play_open_matches(&app, &tournament_id, &groups, &tokens).await,
        4
    );
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"]
    );
}

/// Two simultaneous generate calls pair the round once: the compare-and-set
/// claim on `current_round` lets exactly one through, and the other gets a
/// documented 409 (or 400 if it arrives after the round already exists).
#[tokio::test]
async fn test_concurrent_generate_next_round_pairs_once() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(&app, "sw-concurrent", "swiss", json!({})).await;
    let tokens = register_and_start(&app, &tournament_id, 8, "conc").await;
    let ids = swiss_bracket_ids(&list_brackets(&app, &tournament_id).await);
    assert_eq!(
        play_open_matches(&app, &tournament_id, &ids, &tokens).await,
        4
    );

    let (a, b) = tokio::join!(
        generate_next_round(&app, &tournament_id),
        generate_next_round(&app, &tournament_id)
    );
    let statuses = [a.status, b.status];
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "exactly one call generates the round: {statuses:?}"
    );
    for s in statuses {
        assert!(
            [
                StatusCode::OK,
                StatusCode::CONFLICT,
                StatusCode::BAD_REQUEST
            ]
            .contains(&s),
            "{s}"
        );
    }

    let round2 = list_matches(&app, &tournament_id)
        .await
        .into_iter()
        .filter(|m| m["round"] == 2)
        .count();
    assert_eq!(round2, 4, "round 2 exists exactly once");
    assert_eq!(
        list_brackets(&app, &tournament_id).await[0]["current_round"],
        2
    );
}

/// `max_rounds` applies to Swiss groups, clamped to the rematch-free limit
/// (n - 1 for an even group).
#[tokio::test]
async fn test_swiss_groups_honour_max_rounds() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-max-rounds",
        "groups_and_playoffs",
        json!({ "group_count": 2, "group_format": "swiss", "max_rounds": 3 }),
    )
    .await;
    register_and_start(&app, &tournament_id, 8, "mr").await;
    for b in list_brackets(&app, &tournament_id).await {
        assert_eq!(b["total_rounds"], 3, "max_rounds=3 for {}", b["name"]);
    }

    let app2 = TestApp::new().await;
    let tournament_id = create_tournament(
        &app2,
        "swg-max-rounds-clamp",
        "groups_and_playoffs",
        json!({ "group_count": 2, "group_format": "swiss", "max_rounds": 10 }),
    )
    .await;
    register_and_start(&app2, &tournament_id, 8, "mrc").await;
    for b in list_brackets(&app2, &tournament_id).await {
        assert_eq!(
            b["total_rounds"], 3,
            "a group of 4 has only 3 rematch-free rounds"
        );
    }
}

/// Odd Swiss groups (3 players): with max_rounds = 3 every player sits out
/// exactly one round and meets both opponents exactly once — per group.
#[tokio::test]
async fn test_swiss_groups_odd_size_rotate_byes() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-odd-byes",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 1,
            "group_format": "swiss",
            "max_rounds": 3
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 6, "odd").await;
    let groups = swiss_bracket_ids(&list_brackets(&app, &tournament_id).await);

    for round in 1..=3 {
        assert_eq!(
            play_open_matches(&app, &tournament_id, &groups, &tokens).await,
            2,
            "one match per group of 3 in round {round}"
        );
        if round < 3 {
            assert_eq!(
                stage_statuses(&app, &tournament_id).await,
                vec!["active", "pending"]
            );
            let response = generate_next_round(&app, &tournament_id).await;
            response.assert_status(StatusCode::OK);
        }
    }

    // Every pairing within each group is distinct, and each player played
    // exactly two matches (one bye each).
    let matches = list_matches(&app, &tournament_id).await;
    for group in &groups {
        let mut pairs = HashSet::new();
        let mut played: HashMap<String, usize> = HashMap::new();
        for m in matches.iter().filter(|m| m["bracket_id"] == group.as_str()) {
            let a = m["participant1_registration_id"]
                .as_str()
                .unwrap()
                .to_string();
            let b = m["participant2_registration_id"]
                .as_str()
                .unwrap()
                .to_string();
            *played.entry(a.clone()).or_default() += 1;
            *played.entry(b.clone()).or_default() += 1;
            let key = if a < b { (a, b) } else { (b, a) };
            assert!(pairs.insert(key), "rematch in group {group}");
        }
        assert_eq!(pairs.len(), 3);
        assert!(played.values().all(|&n| n == 2), "{played:?}");
    }

    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"]
    );
}

/// Standalone Swiss still runs end to end through the same endpoint.
#[tokio::test]
async fn test_standalone_swiss_runs_all_rounds() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(&app, "sw-standalone", "swiss", json!({})).await;
    let tokens = register_and_start(&app, &tournament_id, 4, "solo").await;
    let brackets = list_brackets(&app, &tournament_id).await;
    assert_eq!(brackets.len(), 1);
    assert_eq!(brackets[0]["total_rounds"], 2);
    let ids = swiss_bracket_ids(&brackets);

    assert_eq!(
        play_open_matches(&app, &tournament_id, &ids, &tokens).await,
        2
    );
    let response = generate_next_round(&app, &tournament_id).await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        play_open_matches(&app, &tournament_id, &ids, &tokens).await,
        2
    );

    let response = generate_next_round(&app, &tournament_id).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert!(
        body.to_string().contains("Maximum number of rounds"),
        "{body}"
    );
}

// ============================================================================
// Format-settings validation and start rollback
// ============================================================================

async fn tournament_status(app: &TestApp, tournament_id: &str) -> String {
    let body: serde_json::Value = app
        .get(&format!("/v1/tournaments/{tournament_id}"))
        .await
        .json();
    body["data"]["status"].as_str().unwrap().to_string()
}

async fn set_format_settings(app: &TestApp, tournament_id: &str, settings: serde_json::Value) {
    sqlx::query("UPDATE tournaments SET format_settings = $2 WHERE id = $1")
        .bind(uuid::Uuid::parse_str(tournament_id).unwrap())
        .bind(settings)
        .execute(app.pool())
        .await
        .unwrap();
}

/// Register `n` players and seed, without starting.
async fn register_and_seed(app: &TestApp, tournament_id: &str, n: usize, slug: &str) {
    for i in 1..=n {
        let username = format!("sw_{slug}_{i}");
        let (user_id, player_id) = create_test_player(app, &username).await;
        insert_test_registration(app, tournament_id, player_id, user_id, &username).await;
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
}

/// `max_rounds` that is not a positive whole number is refused when the
/// tournament is created or updated — for standalone Swiss and for Swiss
/// groups — instead of surfacing at start. A whole number sent as a float
/// (`3.0`) is fine.
#[tokio::test]
async fn test_invalid_swiss_max_rounds_is_rejected_at_create_and_update() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let create = |slug: String, format: &'static str, settings: serde_json::Value| {
        let game_id = game_id.clone();
        let app = &app;
        async move {
            app.post_json(
                "/v1/tournaments",
                &json!({
                    "game_id": game_id,
                    "name": format!("Swiss validation {slug}"),
                    "slug": slug,
                    "format": format,
                    "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                    "format_settings": settings,
                    "participant_type": "individual",
                    "min_participants": 2,
                    "max_participants": 32,
                    "registration_type": "open",
                    "scheduling_mode": "self_scheduled",
                    "default_match_format": "bo3"
                }),
            )
            .await
        }
    };

    for (i, bad) in [json!(0), json!(-1), json!("3"), json!(2.5)]
        .into_iter()
        .enumerate()
    {
        let standalone = create(
            format!("sw-bad-rounds-{i}"),
            "swiss",
            json!({ "max_rounds": bad }),
        )
        .await;
        assert_eq!(
            standalone.status,
            StatusCode::BAD_REQUEST,
            "swiss max_rounds={bad}: {}",
            standalone.text()
        );
        let grouped = create(
            format!("swg-bad-rounds-{i}"),
            "groups_and_playoffs",
            json!({ "group_count": 2, "group_format": "swiss", "max_rounds": bad }),
        )
        .await;
        assert_eq!(
            grouped.status,
            StatusCode::BAD_REQUEST,
            "swiss groups max_rounds={bad}: {}",
            grouped.text()
        );
    }

    // An integral float is a whole number.
    let ok = create(
        "sw-float-rounds".to_string(),
        "swiss",
        json!({ "max_rounds": 3.0 }),
    )
    .await;
    ok.assert_status(StatusCode::CREATED);
    let tournament_id = ok.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // ... and an update cannot smuggle a bad value in either.
    let response = app
        .patch_json(
            &format!("/v1/tournaments/{tournament_id}"),
            &json!({ "format_settings": { "max_rounds": 0 } }),
        )
        .await;
    assert_eq!(
        response.status,
        StatusCode::BAD_REQUEST,
        "{}",
        response.text()
    );
    app.patch_json(
        &format!("/v1/tournaments/{tournament_id}"),
        &json!({ "format_settings": { "max_rounds": 4.0 } }),
    )
    .await
    .assert_status(StatusCode::OK);
}

/// A stored `max_rounds` that is invalid (a row that predates the create-time
/// check) must fail the start *before* the tournament is claimed: it used to
/// fail inside generation, leaving the tournament `in_progress` with no
/// brackets — and a standalone Swiss with an orphaned stage — so every retry
/// answered "already started".
#[tokio::test]
async fn test_invalid_stored_max_rounds_does_not_strand_the_start() {
    let app = TestApp::new().await;
    // Count every time a tournament is claimed (moved to `in_progress`): a
    // start that is refused up front never claims, whereas one that fails in
    // generation claims and then has to be rolled back.
    sqlx::raw_sql(
        r"
        CREATE TABLE test_start_claims (tournament_id UUID NOT NULL);
        CREATE FUNCTION test_record_start_claim() RETURNS trigger AS $$
        BEGIN
            INSERT INTO test_start_claims VALUES (NEW.id);
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER test_record_start_claim AFTER UPDATE ON tournaments
            FOR EACH ROW
            WHEN (NEW.status = 'in_progress' AND OLD.status <> 'in_progress')
            EXECUTE FUNCTION test_record_start_claim();
        ",
    )
    .execute(app.pool())
    .await
    .unwrap();
    let claims = |tournament_id: String| {
        let app = &app;
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM test_start_claims WHERE tournament_id = $1",
            )
            .bind(uuid::Uuid::parse_str(&tournament_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap()
        }
    };
    for (slug, format, good, bad) in [
        (
            "sw-stored-bad",
            "swiss",
            json!({}),
            json!({ "max_rounds": 0 }),
        ),
        (
            "swg-stored-bad",
            "groups_and_playoffs",
            json!({ "group_count": 2, "group_format": "swiss" }),
            json!({ "group_count": 2, "group_format": "swiss", "max_rounds": "3" }),
        ),
    ] {
        let tournament_id = create_tournament(&app, slug, format, good.clone()).await;
        register_and_seed(&app, &tournament_id, 8, &slug.replace('-', "_")).await;
        set_format_settings(&app, &tournament_id, bad).await;

        let response = app
            .post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
            .await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{slug}: {}",
            response.text()
        );
        assert_eq!(
            claims(tournament_id.clone()).await,
            0,
            "{slug}: invalid settings must be refused before the start is claimed"
        );
        assert_eq!(
            tournament_status(&app, &tournament_id).await,
            "registration",
            "{slug}: a refused start must leave the tournament startable"
        );
        assert!(list_brackets(&app, &tournament_id).await.is_empty());
        assert!(
            stage_statuses(&app, &tournament_id).await.is_empty(),
            "{slug}: no stage may be left behind"
        );

        // Fix the settings; the same tournament starts.
        set_format_settings(&app, &tournament_id, good).await;
        app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
            .await
            .assert_status(StatusCode::OK);
        assert_eq!(tournament_status(&app, &tournament_id).await, "in_progress");
        assert_eq!(claims(tournament_id.clone()).await, 1);
        assert!(!list_brackets(&app, &tournament_id).await.is_empty());
    }
}

/// If generation fails *after* the start was claimed, for whatever reason,
/// the start is undone — status restored, the stage and brackets this
/// attempt created removed — so a retry starts from clean.
#[tokio::test]
async fn test_generation_failure_after_the_claim_rolls_the_start_back() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(&app, "sw-start-rollback", "swiss", json!({})).await;
    register_and_seed(&app, &tournament_id, 4, "rollback").await;

    // Make the round-1 match insert fail: the stage and the bracket already
    // exist by then.
    sqlx::raw_sql(
        r"
        CREATE FUNCTION test_reject_match_insert() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'injected match insert failure'; END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER test_reject_match_insert BEFORE INSERT ON tournament_matches
            FOR EACH ROW EXECUTE FUNCTION test_reject_match_insert();
        ",
    )
    .execute(app.pool())
    .await
    .unwrap();

    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await;
    assert!(
        response.status.is_server_error() || response.status.is_client_error(),
        "start must fail: {}",
        response.text()
    );
    assert_eq!(
        tournament_status(&app, &tournament_id).await,
        "registration",
        "a failed start must be rolled back"
    );
    assert!(list_brackets(&app, &tournament_id).await.is_empty());
    assert!(stage_statuses(&app, &tournament_id).await.is_empty());

    sqlx::raw_sql("DROP TRIGGER test_reject_match_insert ON tournament_matches")
        .execute(app.pool())
        .await
        .unwrap();
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);
    assert_eq!(tournament_status(&app, &tournament_id).await, "in_progress");
    assert_eq!(list_brackets(&app, &tournament_id).await.len(), 1);
    assert_eq!(list_matches(&app, &tournament_id).await.len(), 2);
}

// ============================================================================
// Forfeits settle Swiss rounds
// ============================================================================

async fn admin_forfeit(app: &TestApp, tournament_id: &str, match_id: &str, forfeiter: &str) {
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/forfeit"),
        &json!({
            "forfeiting_registration_id": forfeiter,
            "forfeit_type": "no_show",
            "reason": "Did not turn up to the match"
        }),
    )
    .await
    .assert_status(StatusCode::OK);
}

async fn bracket_standings(
    app: &TestApp,
    tournament_id: &str,
    bracket_id: &str,
) -> Vec<serde_json::Value> {
    let body: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings"
        ))
        .await
        .json();
    body["data"].as_array().unwrap().clone()
}

/// A one-sided forfeit is a decided result: it settles its Swiss round (the
/// next round can be generated) and the walkover winner gets the win's
/// points — in a standalone Swiss and in a Swiss group alike.
#[tokio::test]
async fn test_one_sided_forfeit_settles_a_swiss_round_and_scores() {
    let app = TestApp::new().await;
    for (slug, format, settings, players) in [
        ("sw-forfeit-round", "swiss", json!({}), 4),
        (
            "swg-forfeit-round",
            "groups_and_playoffs",
            json!({
                "group_count": 2,
                "advance_per_group": 2,
                "group_format": "swiss",
                "playoff_format": "single_elimination"
            }),
            8,
        ),
    ] {
        let tournament_id = create_tournament(&app, slug, format, settings).await;
        let tokens =
            register_and_start(&app, &tournament_id, players, &slug.replace('-', "_")).await;
        let groups = swiss_bracket_ids(&list_brackets(&app, &tournament_id).await);

        // Forfeit one round-1 match (participant 2 does not show).
        let forfeited = list_matches(&app, &tournament_id)
            .await
            .into_iter()
            .next()
            .unwrap();
        let bracket_id = forfeited["bracket_id"].as_str().unwrap().to_string();
        let winner = forfeited["participant1_registration_id"]
            .as_str()
            .unwrap()
            .to_string();
        let loser = forfeited["participant2_registration_id"]
            .as_str()
            .unwrap()
            .to_string();
        admin_forfeit(
            &app,
            &tournament_id,
            forfeited["id"].as_str().unwrap(),
            &loser,
        )
        .await;

        let rows = bracket_standings(&app, &tournament_id, &bracket_id).await;
        let row = |reg: &str| {
            rows.iter()
                .find(|r| r["registration_id"] == reg)
                .unwrap_or_else(|| panic!("{slug}: no standings row for {reg}"))
                .clone()
        };
        assert_eq!(row(&winner)["points"], 3, "{slug}: {rows:?}");
        assert_eq!(row(&winner)["matches_won"], 1, "{slug}: {rows:?}");
        assert_eq!(row(&loser)["points"], 0, "{slug}: {rows:?}");

        // Play the rest of round 1 — the forfeit is not "open".
        for m in list_matches(&app, &tournament_id).await {
            if m["status"] == "forfeit" {
                continue;
            }
            let p1 = m["participant1_registration_id"].as_str().unwrap();
            let p2 = m["participant2_registration_id"].as_str().unwrap();
            confirm_win_via_claims(
                &app,
                &tournament_id,
                m["id"].as_str().unwrap(),
                p1,
                &tokens[p1],
                &tokens[p2],
                true,
            )
            .await;
        }

        let response = generate_next_round(&app, &tournament_id).await;
        assert_eq!(
            response.status,
            StatusCode::OK,
            "{slug}: a forfeit must settle the round: {}",
            response.text()
        );
        let new_matches = response.json::<serde_json::Value>()["data"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(new_matches, players / 2, "{slug}");
        for b in list_brackets(&app, &tournament_id).await {
            if groups.contains(b["id"].as_str().unwrap()) {
                assert_eq!(b["current_round"], 2, "{slug}: {}", b["name"]);
            }
        }
    }
}

/// A double forfeit (the match is cancelled, nobody wins) on the last
/// unfinished match of the group stage finishes the stage and opens the
/// playoffs.
#[tokio::test]
async fn test_double_forfeit_on_last_group_match_opens_playoffs() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-double-forfeit-last",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "swiss",
            "playoff_format": "single_elimination",
            "max_rounds": 1
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 8, "dfl").await;

    // Play everything except one match.
    let all = list_matches(&app, &tournament_id).await;
    let (last, rest) = all.split_last().unwrap();
    for m in rest {
        let p1 = m["participant1_registration_id"].as_str().unwrap();
        let p2 = m["participant2_registration_id"].as_str().unwrap();
        confirm_win_via_claims(
            &app,
            &tournament_id,
            m["id"].as_str().unwrap(),
            p1,
            &tokens[p1],
            &tokens[p2],
            true,
        )
        .await;
    }
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["active", "pending"]
    );

    app.post_json(
        &format!(
            "/v1/admin/tournaments/{tournament_id}/matches/{}/double-forfeit",
            last["id"].as_str().unwrap()
        ),
        &json!({ "reason": "Neither side showed up for the match" }),
    )
    .await
    .assert_status(StatusCode::OK);

    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"],
        "the cancelled match was the last one the group stage waited on"
    );
    assert_eq!(
        list_brackets(&app, &tournament_id).await.len(),
        3,
        "the playoff bracket was seeded"
    );
}

// ============================================================================
// Deploy safety: group stages advanced under the old round rule
// ============================================================================

/// Tournaments in flight when Swiss brackets started having to play every
/// round have group brackets with `current_round < total_rounds` in a group
/// stage that already handed over to the playoffs. Those brackets must not
/// keep the tournament from ever reading as complete.
#[tokio::test]
async fn test_completed_swiss_group_stage_does_not_block_tournament_completion() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament(
        &app,
        "swg-legacy-complete",
        "groups_and_playoffs",
        json!({
            "group_count": 2,
            "advance_per_group": 1,
            "group_format": "swiss",
            "playoff_format": "single_elimination",
            "max_rounds": 1
        }),
    )
    .await;
    let tokens = register_and_start(&app, &tournament_id, 4, "legacy").await;
    let groups = swiss_bracket_ids(&list_brackets(&app, &tournament_id).await);
    play_open_matches(&app, &tournament_id, &groups, &tokens).await;
    assert_eq!(
        stage_statuses(&app, &tournament_id).await,
        vec!["completed", "active"]
    );

    // What the old code left behind: the playoffs were seeded after round 1
    // of a group that was configured for more rounds.
    sqlx::query(
        "UPDATE tournament_brackets SET total_rounds = 3
         WHERE tournament_id = $1 AND bracket_type = 'swiss'",
    )
    .bind(uuid::Uuid::parse_str(&tournament_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    // Play the final (two advancing teams → one playoff match).
    let final_match = list_matches(&app, &tournament_id)
        .await
        .into_iter()
        .find(|m| !groups.contains(m["bracket_id"].as_str().unwrap()))
        .expect("playoff final");
    let winner = final_match["participant1_registration_id"]
        .as_str()
        .unwrap();
    let loser = final_match["participant2_registration_id"]
        .as_str()
        .unwrap();
    let final_id = final_match["id"].as_str().unwrap();
    confirm_win_via_claims(
        &app,
        &tournament_id,
        final_id,
        winner,
        &tokens[winner],
        &tokens[loser],
        true,
    )
    .await;

    // The progression read-out for the final says the tournament is done.
    let response = app
        .post_json(
            &format!("/v1/admin/matches/{final_id}/progression/process"),
            &json!({
                "winner_registration_id": winner,
                "loser_registration_id": loser
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["bracket_complete"], true, "{body}");
    assert_eq!(
        body["data"]["tournament_complete"], true,
        "a finished group stage must not hold the tournament open: {body}"
    );
}
