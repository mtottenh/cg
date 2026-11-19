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
