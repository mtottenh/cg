use super::*;

// ============================================================================
// TEAM REGISTRATION
// ============================================================================

/// Create a team-participant tournament via the API, publish it, and open
/// registration. Returns the tournament ID.
async fn create_team_tournament(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": format!("Team Reg Test {}", slug),
                "slug": slug,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "min_participants": 2,
                "max_participants": 8,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

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

/// Create a league + season + team with a captain member.
/// Returns (team_season_id, captain JWT token).
async fn create_team_with_captain(app: &TestApp, tag: &str) -> (String, String) {
    let league = LeagueBuilder::new()
        .name(format!("Reg League {tag}"))
        .build_persisted(app.pool())
        .await;
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(format!("Reg Season {tag}"))
        .registration()
        .build_persisted(app.pool())
        .await;
    let owner = UserBuilder::new()
        .username(format!("owner_{tag}"))
        .build_persisted(app.pool())
        .await;
    let captain = UserBuilder::new()
        .username(format!("captain_{tag}"))
        .build_persisted(app.pool())
        .await;
    let team = LeagueTeamBuilder::new()
        .name(format!("Team {tag}"))
        .tag(tag)
        .league_id(league.id)
        .owner(owner.id)
        .build_persisted(app.pool())
        .await;
    let team_season = LeagueTeamSeasonBuilder::new()
        .team_id(team.id)
        .season_id(season.id)
        .build_persisted(app.pool())
        .await;
    LeagueTeamMemberBuilder::new()
        .team_season_id(team_season.id)
        .player_id(captain.id)
        .captain()
        .build_persisted(app.pool())
        .await;

    let token = create_test_token(
        captain.id,
        captain.id,
        &format!("captain_{tag}"),
        TEST_JWT_SECRET,
    );
    (team_season.id.to_string(), token)
}

#[tokio::test]
async fn test_register_team_happy_path() {
    let app = TestApp::new().await;
    let tournament_id = create_team_tournament(&app, "team-reg-happy").await;
    let (team_season_id, captain_token) = create_team_with_captain(&app, "trha").await;

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/team"),
            &json!({
                "team_season_id": team_season_id,
                "participant_name": "The Happy Team"
            }),
            &captain_token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["participant_name"], "The Happy Team");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["tournament_id"], tournament_id);

    // The registration shows up in the tournament's registration list.
    let response = app
        .get_auth(&format!("/v1/tournaments/{tournament_id}/registrations"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let regs = body["data"].as_array().unwrap();
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0]["participant_name"], "The Happy Team");
}

#[tokio::test]
async fn test_register_team_twice_conflicts() {
    let app = TestApp::new().await;
    let tournament_id = create_team_tournament(&app, "team-reg-dup").await;
    let (team_season_id, captain_token) = create_team_with_captain(&app, "trdup").await;

    let body = json!({
        "team_season_id": team_season_id,
        "participant_name": "Dup Team"
    });
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/registrations/team"),
        &body,
        &captain_token,
    )
    .await
    .assert_status(StatusCode::CREATED);

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/team"),
            &body,
            &captain_token,
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_register_team_before_registration_opens_rejected() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    // Draft tournament — registration never opened.
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Team Reg Closed",
                "slug": "team-reg-closed",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "min_participants": 2,
                "max_participants": 8,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    let (team_season_id, captain_token) = create_team_with_captain(&app, "trcl").await;

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/team"),
            &json!({
                "team_season_id": team_season_id,
                "participant_name": "Too Early Team"
            }),
            &captain_token,
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_withdraw_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "withdraw-test").await;

    // Register a player
    let registration_id = register_player(&app, &tournament_id, "Player1").await;

    // Withdraw
    let response = app
        .delete_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}"
        ))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "withdrawn");
}

// ============================================================================
// ADMIN MODERATION: REJECT / DISQUALIFY / ADMIN CHECK-IN
// ============================================================================

#[tokio::test]
async fn test_reject_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "reject-reg-test").await;

    // Register a player (pending status)
    let registration_id = register_player(&app, &tournament_id, "RejectMe").await;

    // Reject it with a reason
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/reject"),
            &json!({ "reason": "Roster incomplete" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Rejected registrations are stored as withdrawn
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["id"], registration_id);
    assert_eq!(body["data"]["status"], "withdrawn");

    // Rejecting a non-pending registration is invalid
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/reject"),
            &json!({ "reason": "Again" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_disqualify_approved_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "dq-reg-test").await;

    // Register and approve a player
    let registration_id = register_player(&app, &tournament_id, "DqMe").await;
    approve_registration(&app, &tournament_id, &registration_id).await;

    // Disqualify (reason is required)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/disqualify"),
            &json!({ "reason": "Cheating detected" }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "disqualified");

    // Disqualified is terminal — a second disqualify is invalid
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/disqualify"),
            &json!({ "reason": "Still cheating" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_check_in_sets_checked_in() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "admin-checkin-test").await;

    // Register and approve a player (not checked in yet)
    let registration_id = register_player(&app, &tournament_id, "CheckMeIn").await;
    approve_registration(&app, &tournament_id, &registration_id).await;

    // Admin check-in bypasses the check-in window
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/admin-check-in"
        ))
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["checked_in"], true);
    assert!(
        body["data"]["checked_in_at"].is_string(),
        "checked_in_at should be set"
    );

    // Checking in twice conflicts
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/admin-check-in"
        ))
        .await;
    response.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_get_check_in_status() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "checkin-status-test").await;

    // Register 2 players (required min_participants is 2)
    // First player via API (dev user) - needs approval for eligibility
    let reg1 = register_player(&app, &tournament_id, "Player1").await;
    approve_registration(&app, &tournament_id, &reg1).await;

    // Second player via direct DB insertion (already approved)
    let (user2_id, player2_id) = create_test_player(&app, "player2_checkin").await;
    insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Player2").await;

    // Get check-in status
    let response = app
        .get(&format!("/v1/tournaments/{tournament_id}/check-in-status"))
        .await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["tournament_id"], tournament_id);
    assert!(body["data"]["total_eligible"].as_i64().unwrap() >= 2);
}
