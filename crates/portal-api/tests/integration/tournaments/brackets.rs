use super::*;

async fn create_de_tournament(app: &TestApp, slug: &str, min_participants: i32) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("DE Test {}", slug),
                "slug": slug,
                "format": "double_elimination",
                "participant_type": "individual",
                "min_participants": min_participants,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/open-registration",
            tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

/// Helper to register N players for a tournament and return registration IDs.
async fn register_n_players(
    app: &TestApp,
    tournament_id: &str,
    count: usize,
    slug: &str,
) -> Vec<String> {
    let mut reg_ids = Vec::new();

    // First player uses the dev user
    let reg1 = register_player(app, tournament_id, "Player1").await;
    approve_registration(app, tournament_id, &reg1).await;
    reg_ids.push(reg1);

    // Remaining players use new users
    for i in 2..=count {
        let (user_id, player_id) =
            create_test_player(app, &format!("de_player{}_{}", i, slug)).await;
        let reg = insert_test_registration(
            app,
            tournament_id,
            player_id,
            user_id,
            &format!("Player{}", i),
        )
        .await;
        reg_ids.push(reg);
    }

    reg_ids
}

#[tokio::test]
async fn test_start_double_elimination_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-start-test", 4).await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "de-start").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Verify tournament status is "in_progress"
    let response = app
        .get(&format!("/v1/tournaments/{}", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
    assert_eq!(body["data"]["format"], "double_elimination");

    // Get brackets - should have 3 (Winners, Losers, Grand Final)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 3, "Should have 3 brackets (WB, LB, GF)");

    // Verify bracket types
    let bracket_types: Vec<&str> = brackets
        .iter()
        .map(|b| b["bracket_type"].as_str().unwrap())
        .collect();
    assert!(bracket_types.contains(&"winners"));
    assert!(bracket_types.contains(&"losers"));
    assert!(bracket_types.contains(&"grand_final"));

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 8 teams: WB=7, LB=6, GF=1 = 14 total
    assert_eq!(
        all_matches.len(),
        14,
        "8-team DE should have 14 matches total"
    );

    // Get matches per bracket
    let wb_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "winners")
        .unwrap();
    let lb_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "losers")
        .unwrap();
    let gf_bracket = brackets
        .iter()
        .find(|b| b["bracket_type"] == "grand_final")
        .unwrap();

    let wb_id = wb_bracket["id"].as_str().unwrap();
    let lb_id = lb_bracket["id"].as_str().unwrap();
    let gf_id = gf_bracket["id"].as_str().unwrap();

    let wb_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == wb_id)
        .collect();
    let lb_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == lb_id)
        .collect();
    let gf_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == gf_id)
        .collect();

    assert_eq!(wb_matches.len(), 7, "WB should have 7 matches");
    assert_eq!(lb_matches.len(), 6, "LB should have 6 matches");
    assert_eq!(gf_matches.len(), 1, "GF should have 1 match");

    // Verify WR1 matches have participants assigned
    let wr1_matches: Vec<_> = wb_matches
        .iter()
        .filter(|m| m["round"].as_i64() == Some(1))
        .collect();
    assert_eq!(wr1_matches.len(), 4, "WR1 should have 4 matches");

    for m in &wr1_matches {
        assert!(
            m["participant1_name"].is_string(),
            "WR1 match should have participant 1 assigned"
        );
        assert!(
            m["participant2_name"].is_string(),
            "WR1 match should have participant 2 assigned"
        );
    }

    // Verify GF match has no participants yet (they'll be filled after WB/LB finals)
    let gf_match = &gf_matches[0];
    assert!(
        gf_match["participant1_name"].is_null(),
        "GF should not have participants yet"
    );
}

#[tokio::test]
async fn test_start_double_elimination_4_teams() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-4teams-test", 2).await;

    // Register 4 players
    let _reg_ids = register_n_players(&app, &tournament_id, 4, "de-4teams").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 4 teams: WB=3, LB=2, GF=1 = 6 total
    assert_eq!(
        all_matches.len(),
        6,
        "4-team DE should have 6 matches total"
    );
}

#[tokio::test]
async fn test_double_elimination_with_byes() {
    let app = TestApp::new().await;
    let tournament_id = create_de_tournament(&app, "de-byes-test", 2).await;

    // Register 6 players (needs 8-bracket, 2 byes)
    let _reg_ids = register_n_players(&app, &tournament_id, 6, "de-byes").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get all matches - bracket size is 8, so same match count as 8-team
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 8-bracket: WB=7, LB=6, GF=1 = 14 total
    assert_eq!(
        all_matches.len(),
        14,
        "6-team DE (8-bracket) should have 14 matches total"
    );

    // Get brackets
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    let wb = brackets
        .iter()
        .find(|b| b["bracket_type"] == "winners")
        .unwrap();
    let wb_id = wb["id"].as_str().unwrap();

    // Some WR2 matches should have participants from byes
    let wr2_matches: Vec<_> = all_matches
        .iter()
        .filter(|m| m["bracket_id"] == wb_id && m["round"].as_i64() == Some(2))
        .collect();

    // At least one WR2 match should have a bye-advanced participant
    let has_bye_advanced = wr2_matches
        .iter()
        .any(|m| m["participant1_name"].is_string() || m["participant2_name"].is_string());
    assert!(
        has_bye_advanced,
        "At least one WR2 match should have a bye-advanced participant"
    );
}

// ============================================================================
// ROUND ROBIN TESTS
// ============================================================================

async fn create_rr_tournament(app: &TestApp, slug: &str, min_participants: i32) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("RR Test {}", slug),
                "slug": slug,
                "format": "round_robin",
                "participant_type": "individual",
                "min_participants": min_participants,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/open-registration",
            tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

#[tokio::test]
async fn test_start_round_robin_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_rr_tournament(&app, "rr-start-test", 2).await;

    // Register 4 players
    let _reg_ids = register_n_players(&app, &tournament_id, 4, "rr-start").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Verify tournament status
    let response = app
        .get(&format!("/v1/tournaments/{}", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
    assert_eq!(body["data"]["format"], "round_robin");

    // Get brackets - should have 1 (RoundRobin)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 1, "Should have 1 bracket (RoundRobin)");
    assert_eq!(brackets[0]["bracket_type"], "round_robin");

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 4 teams: 3 rounds, 6 matches total (N*(N-1)/2 = 4*3/2 = 6)
    assert_eq!(
        all_matches.len(),
        6,
        "4-team RR should have 6 matches total"
    );

    // Verify all matches have both participants assigned
    for m in all_matches {
        assert!(
            m["participant1_name"].is_string(),
            "RR match {} should have participant 1 assigned",
            m["bracket_position"]
        );
        assert!(
            m["participant2_name"].is_string(),
            "RR match {} should have participant 2 assigned",
            m["bracket_position"]
        );
    }

    // Verify bracket positions use RR prefix
    for m in all_matches {
        let pos = m["bracket_position"].as_str().unwrap();
        assert!(
            pos.starts_with("RR"),
            "RR match position should start with 'RR', got: {pos}"
        );
    }
}

#[tokio::test]
async fn test_get_bracket_standings() {
    let app = TestApp::new().await;
    let tournament_id = create_rr_tournament(&app, "rr-standings-test", 2).await;

    // Register 4 players
    let _reg_ids = register_n_players(&app, &tournament_id, 4, "rr-standings").await;

    // Auto-seed and start
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Get bracket ID
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    let bracket_id = brackets[0]["id"].as_str().unwrap();

    // Get standings
    let response = app
        .get(&format!(
            "/v1/tournaments/{}/brackets/{}/standings",
            tournament_id, bracket_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let standings_body: serde_json::Value = response.json();
    let standings = standings_body["data"].as_array().unwrap();

    // Should have standings for all 4 participants
    assert_eq!(standings.len(), 4, "Should have 4 standings entries");

    // All standings should start with 0 points (no matches played yet)
    for s in standings {
        assert_eq!(s["points"], 0);
        assert_eq!(s["matches_played"], 0);
        assert!(s["registration_id"].is_string());
        assert!(s["participant_name"].is_string());
    }
}

// ============================================================================
// SWISS TESTS
// ============================================================================

async fn create_swiss_tournament(app: &TestApp, slug: &str, min_participants: i32) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Swiss Test {}", slug),
                "slug": slug,
                "format": "swiss",
                "participant_type": "individual",
                "min_participants": min_participants,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Open registration
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{}/open-registration",
            tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);

    tournament_id
}

#[tokio::test]
async fn test_start_swiss_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_swiss_tournament(&app, "swiss-start-test", 2).await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "swiss-start").await;

    // Auto-seed
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Start tournament
    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Verify tournament status
    let response = app
        .get(&format!("/v1/tournaments/{}", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
    assert_eq!(body["data"]["format"], "swiss");

    // Get brackets - should have 1 (Swiss)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 1, "Should have 1 bracket (Swiss)");
    assert_eq!(brackets[0]["bracket_type"], "swiss");

    // Get all matches - should have 4 (round 1: 8 teams / 2 = 4 matches)
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    assert_eq!(
        all_matches.len(),
        4,
        "Swiss R1 with 8 teams should have 4 matches"
    );

    // Verify all R1 matches have participants assigned
    for m in all_matches {
        assert!(
            m["participant1_name"].is_string(),
            "Swiss R1 match should have participant 1"
        );
        assert!(
            m["participant2_name"].is_string(),
            "Swiss R1 match should have participant 2"
        );
        assert_eq!(m["round"], 1);
    }

    // Verify bracket positions use SW prefix
    for m in all_matches {
        let pos = m["bracket_position"].as_str().unwrap();
        assert!(
            pos.starts_with("SW1M"),
            "Swiss R1 position should start with 'SW1M', got: {pos}"
        );
    }
}

#[tokio::test]
async fn test_swiss_generate_next_round_not_all_complete() {
    let app = TestApp::new().await;
    let tournament_id = create_swiss_tournament(&app, "swiss-early-gen-test", 2).await;

    // Register 4 players
    let _reg_ids = register_n_players(&app, &tournament_id, 4, "swiss-early").await;

    // Auto-seed and start
    let response = app
        .post_json(
            &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
            &json!({ "algorithm": "random" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let response = app
        .post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);

    // Try to generate next round without completing R1 matches - should fail
    let response = app
        .post_auth(&format!(
            "/v1/admin/tournaments/{}/generate-next-round",
            tournament_id
        ))
        .await;
    assert_ne!(
        response.status,
        StatusCode::OK,
        "Should not be able to generate next round when R1 matches are incomplete"
    );
}

// ============================================================================
// GROUPS + PLAYOFFS TESTS
// ============================================================================

async fn create_gp_tournament(
    app: &TestApp,
    slug: &str,
    min_participants: i32,
    format_settings: serde_json::Value,
) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("G+P Test {}", slug),
                "slug": slug,
                "format": "groups_and_playoffs",
                "format_settings": format_settings,
                "participant_type": "individual",
                "min_participants": min_participants,
                "max_participants": 32,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    // Publish
    app.post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!(
        "/v1/tournaments/{}/open-registration",
        tournament_id
    ))
    .await
    .assert_status(StatusCode::OK);

    tournament_id
}

#[tokio::test]
async fn test_start_groups_and_playoffs_tournament() {
    let app = TestApp::new().await;
    let tournament_id = create_gp_tournament(
        &app,
        "gp-start-test",
        4,
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "round_robin",
            "playoff_format": "single_elimination"
        }),
    )
    .await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "gp-start").await;

    // Auto-seed
    app.post_json(
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Start tournament
    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Verify tournament status
    let response = app
        .get(&format!("/v1/tournaments/{}", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "in_progress");
    assert_eq!(body["data"]["format"], "groups_and_playoffs");

    // Get stages - should have 2 (Group Stage + Playoffs)
    let response = app
        .get(&format!("/v1/tournaments/{}/stages", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let stages_body: serde_json::Value = response.json();
    let stages = stages_body["data"].as_array().unwrap();
    assert_eq!(stages.len(), 2, "Should have 2 stages");
    assert_eq!(stages[0]["name"], "Group Stage");
    assert_eq!(stages[0]["format"], "group_stage");
    assert_eq!(stages[0]["status"], "active");
    assert_eq!(stages[1]["name"], "Playoffs");
    assert_eq!(stages[1]["status"], "pending");

    // Get brackets - should have 2 group brackets (RR type)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 2, "Should have 2 group brackets");

    // Verify bracket types and group numbers
    let mut group_brackets: Vec<&serde_json::Value> = brackets
        .iter()
        .filter(|b| b["bracket_type"] == "round_robin")
        .collect();
    assert_eq!(group_brackets.len(), 2, "Should have 2 RR brackets");

    group_brackets.sort_by_key(|b| b["group_number"].as_i64().unwrap());
    assert_eq!(group_brackets[0]["group_number"], 1);
    assert_eq!(group_brackets[0]["name"], "Group A");
    assert_eq!(group_brackets[1]["group_number"], 2);
    assert_eq!(group_brackets[1]["name"], "Group B");

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 2 groups of 4: each group has 4*3/2 = 6 RR matches, total = 12
    assert_eq!(
        all_matches.len(),
        12,
        "2 groups of 4 should have 12 RR matches total"
    );

    // Verify all matches have both participants assigned (RR assigns immediately)
    for m in all_matches {
        assert!(
            m["participant1_name"].is_string(),
            "Match {} should have participant 1",
            m["bracket_position"]
        );
        assert!(
            m["participant2_name"].is_string(),
            "Match {} should have participant 2",
            m["bracket_position"]
        );
    }

    // Get standings for each group bracket
    for bracket in &group_brackets {
        let bracket_id = bracket["id"].as_str().unwrap();
        let response = app
            .get(&format!(
                "/v1/tournaments/{}/brackets/{}/standings",
                tournament_id, bracket_id
            ))
            .await;
        response.assert_status(StatusCode::OK);
        let standings_body: serde_json::Value = response.json();
        let standings = standings_body["data"].as_array().unwrap();

        assert_eq!(
            standings.len(),
            4,
            "Each group should have 4 standings entries"
        );
    }
}

#[tokio::test]
async fn test_groups_and_playoffs_with_6_players() {
    let app = TestApp::new().await;
    let tournament_id = create_gp_tournament(
        &app,
        "gp-6player-test",
        4,
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "round_robin",
            "playoff_format": "single_elimination"
        }),
    )
    .await;

    // Register 6 players (uneven groups: 3+3)
    let _reg_ids = register_n_players(&app, &tournament_id, 6, "gp-6player").await;

    // Auto-seed
    app.post_json(
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Start tournament
    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Get brackets
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 2, "Should have 2 group brackets");

    // Get all matches
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 2 groups of 3: each group has 3*2/2 = 3 RR matches, total = 6
    assert_eq!(
        all_matches.len(),
        6,
        "2 groups of 3 should have 6 RR matches total"
    );
}

#[tokio::test]
async fn test_groups_with_swiss_groups() {
    let app = TestApp::new().await;
    let tournament_id = create_gp_tournament(
        &app,
        "gp-swiss-test",
        4,
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "swiss",
            "playoff_format": "single_elimination"
        }),
    )
    .await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "gp-swiss").await;

    // Auto-seed
    app.post_json(
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Start tournament
    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Get brackets - should have 2 Swiss brackets
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(brackets.len(), 2, "Should have 2 group brackets");

    let swiss_brackets: Vec<&serde_json::Value> = brackets
        .iter()
        .filter(|b| b["bracket_type"] == "swiss")
        .collect();
    assert_eq!(swiss_brackets.len(), 2, "Should have 2 Swiss brackets");

    // Get all matches - Swiss only generates R1
    let response = app
        .get(&format!("/v1/tournaments/{}/matches", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let matches_body: serde_json::Value = response.json();
    let all_matches = matches_body["data"].as_array().unwrap();

    // 2 groups of 4: Swiss R1 has 2 matches per group, total = 4
    assert_eq!(
        all_matches.len(),
        4,
        "Swiss R1 with 2 groups of 4 should have 4 matches"
    );

    // Verify bracket positions use SW prefix
    for m in all_matches {
        let pos = m["bracket_position"].as_str().unwrap();
        assert!(
            pos.starts_with("SW"),
            "Swiss position should start with 'SW', got: {pos}"
        );
    }
}

#[tokio::test]
async fn test_groups_with_de_playoffs() {
    let app = TestApp::new().await;
    let tournament_id = create_gp_tournament(
        &app,
        "gp-de-playoffs-test",
        4,
        json!({
            "group_count": 2,
            "advance_per_group": 2,
            "group_format": "round_robin",
            "playoff_format": "double_elimination"
        }),
    )
    .await;

    // Register 8 players
    let _reg_ids = register_n_players(&app, &tournament_id, 8, "gp-de").await;

    // Auto-seed
    app.post_json(
        &format!("/v1/tournaments/{}/seeding/auto", tournament_id),
        &json!({ "algorithm": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // Start tournament
    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Verify stages are created correctly
    let response = app
        .get(&format!("/v1/tournaments/{}/stages", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let stages_body: serde_json::Value = response.json();
    let stages = stages_body["data"].as_array().unwrap();
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0]["format"], "group_stage");
    assert_eq!(stages[1]["format"], "double_elimination");
    assert_eq!(stages[1]["status"], "pending");

    // Get brackets - should have 2 RR group brackets (no playoff brackets yet)
    let response = app
        .get(&format!("/v1/tournaments/{}/brackets", tournament_id))
        .await;
    response.assert_status(StatusCode::OK);
    let brackets_body: serde_json::Value = response.json();
    let brackets = brackets_body["data"].as_array().unwrap();
    assert_eq!(
        brackets.len(),
        2,
        "Should have 2 group brackets (no playoffs yet)"
    );
}
