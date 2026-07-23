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
    // `create_team_tournament` uses `registration_type: open`, which
    // auto-approves (P-2).
    assert_eq!(body["data"]["status"], "approved");
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
// P-2: INITIAL STATUS FOLLOWS registration_type
// ============================================================================

/// `registration_type: open` means "anyone may enter", so a registration
/// must land `approved` with nothing for an organiser to click.
///
/// Regression guard for P-2: `initial_status_for_tournament` implemented
/// this rule but was never called, and the INSERT omitted `status`
/// entirely, so the `'pending'` column default always won.
#[tokio::test]
async fn test_open_tournament_auto_approves_player_registration() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "open-auto-approve", "open").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "AutoApproved" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["status"], "approved",
        "open registration must auto-approve"
    );
    let registration_id = body["data"]["id"].as_str().unwrap().to_string();

    // The status was persisted, not just echoed back by the handler.
    let reg_uuid: Uuid = registration_id.parse().unwrap();
    let stored: String =
        sqlx::query_scalar("SELECT status FROM tournament_registrations WHERE id = $1")
            .bind(reg_uuid)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(stored, "approved", "DB row must carry the approved status");

    // Approving it again is a no-op, not an error.
    //
    // This assertion originally required 400. That was the behaviour when this
    // test was written, but P-36 deliberately changed the specification: because
    // P-2 auto-approves open registrations, an organiser pressing Approve on a
    // row they can see was always getting a 400. Approve is now idempotent for an
    // already-approved registration, so the expected status here is 200.
    //
    // Changed because the SPEC changed, not to make a failing test pass — the
    // narrow case (terminal statuses still rejected) is pinned separately by
    // `test_approve_still_rejects_a_withdrawn_registration`.
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
        ))
        .await;
    response.assert_status(StatusCode::OK);
}

/// Every non-`open` registration type still requires a decision, so the
/// registration must stay `pending` and remain approvable.
///
/// `invite_only` is covered separately by
/// `test_invite_only_admits_invited_player` rather than here: since P-27 an
/// uninvited caller cannot create the registration at all, so it cannot be
/// driven through this loop's "register, then approve" shape. The
/// pending-then-approvable guarantee itself is unchanged and is asserted
/// there.
#[tokio::test]
async fn test_non_open_tournaments_still_require_approval() {
    for registration_type in ["approval", "qualification"] {
        let app = TestApp::new().await;
        let tournament_id = create_tournament_with_registration_type(
            &app,
            &format!("needs-approval-{}", registration_type.replace('_', "-")),
            registration_type,
        )
        .await;

        let response = app
            .post_json(
                &format!("/v1/tournaments/{tournament_id}/registrations/player"),
                &json!({ "participant_name": "NeedsApproval" }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["data"]["status"], "pending",
            "{registration_type} must not auto-approve"
        );

        // Still approvable by an organiser.
        let registration_id = body["data"]["id"].as_str().unwrap();
        let response = app
            .post_auth(&format!(
                "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
            ))
            .await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["data"]["status"], "approved");
    }
}

// ============================================================================
// ADMIN MODERATION: REJECT / DISQUALIFY / ADMIN CHECK-IN
// ============================================================================

#[tokio::test]
async fn test_reject_registration() {
    let app = TestApp::new().await;
    // `approval` (not `open`) — only a pending registration can be
    // rejected, and `open` now auto-approves (P-2).
    let tournament_id =
        create_tournament_with_registration_type(&app, "reject-reg-test", "approval").await;

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

// ============================================================================
// CAPACITY RACE (audit: count-then-insert overflowed max_participants)
// ============================================================================

/// `max_participants` must hold under concurrent registration.
///
/// The old `count_registrations()` + `create()` pair ran on two separate
/// pool connections, so N+1 racers all read the same pre-insert count and
/// all inserted. `create_with_capacity_check` does both inside one
/// transaction behind `SELECT ... FOR UPDATE` on the tournament row.
#[tokio::test]
async fn test_concurrent_registrations_cannot_exceed_max_participants() {
    use portal_core::types::TournamentRegistrationStatus;
    use portal_db::adapters::PgTournamentRegistrationRepository;
    use portal_domain::repositories::tournament::{
        CreateTournamentRegistration, TournamentRegistrationRepository,
    };
    use std::sync::Arc;

    const CAPACITY: i32 = 4;
    const RACERS: i32 = CAPACITY + 3;

    let app = TestApp::new().await;

    let tournament = TournamentBuilder::new()
        .slug(format!("cap-race-{}", Uuid::new_v4()))
        .individual()
        .participants(2, CAPACITY)
        .registration_open()
        .build_persisted(app.pool())
        .await;

    // Distinct players so the per-tournament uniqueness constraints are
    // not what limits the winners.
    // `UserBuilder::build_persisted` also creates the 1:1 player row with
    // the same UUID.
    let mut players = Vec::new();
    for _ in 0..RACERS {
        let user = UserBuilder::new().build_persisted(app.pool()).await;
        players.push((user.id, user.id));
    }

    let repo = Arc::new(PgTournamentRegistrationRepository::new(app.pool().clone()));

    let mut handles = Vec::new();
    for (i, (user_id, player_id)) in players.into_iter().enumerate() {
        let repo = Arc::clone(&repo);
        let tournament_id = TournamentId::from(tournament.id);
        handles.push(tokio::spawn(async move {
            repo.create_with_capacity_check(
                CreateTournamentRegistration {
                    tournament_id,
                    team_season_id: None,
                    player_id: Some(PlayerId::from(player_id)),
                    adhoc_team_id: None,
                    participant_name: format!("Racer {i}"),
                    participant_logo_url: None,
                    registered_by: UserId::from(user_id),
                    seed_rating: None,
                    status: TournamentRegistrationStatus::Pending,
                },
                None,
            )
            .await
        }));
    }

    let mut succeeded = 0;
    let mut full = 0;
    for handle in handles {
        match handle.await.expect("task panicked") {
            Ok(_) => succeeded += 1,
            Err(DomainError::TournamentFull) => full += 1,
            Err(e) => panic!("unexpected error from concurrent registration: {e}"),
        }
    }

    assert_eq!(
        succeeded, CAPACITY,
        "exactly max_participants registrations should succeed"
    );
    assert_eq!(full, RACERS - CAPACITY, "the rest must see TournamentFull");

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tournament_registrations
         WHERE tournament_id = $1 AND status NOT IN ('withdrawn', 'rejected')",
    )
    .bind(tournament.id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        stored,
        i64::from(CAPACITY),
        "database must not hold more registrations than max_participants"
    );
}

// ============================================================================
// P-24 — participant check-in authorization
// ============================================================================

/// The participant-facing `POST /registrations/{id}/check-in` had the
/// same hole as match check-in: any authenticated caller could check in
/// any registration. (The `admin-check-in` sibling was already gated by
/// `tournament.participants.manage`.)
#[tokio::test]
async fn test_registration_check_in_requires_authority_over_registration() {
    let app = TestApp::new().await;
    let tournament_id = create_tournament_with_registration(&app, "reg-checkin-authz").await;

    // A registration owned by somebody other than the dev fixture user.
    let (user2_id, player2_id) = create_test_player(&app, "reg_checkin_owner").await;
    let registration_id =
        insert_test_registration(&app, &tournament_id, player2_id, user2_id, "Owner").await;
    let owner_token = create_test_token(user2_id, player2_id, "reg_checkin_owner", TEST_JWT_SECRET);

    // Open the check-in window so a denial can only be about authority.
    sqlx::query(
        "UPDATE tournaments SET check_in_required = true, \
         check_in_start = NOW() - INTERVAL '1 minute', \
         check_in_end = NOW() + INTERVAL '1 hour' WHERE id = $1",
    )
    .bind(Uuid::parse_str(&tournament_id).unwrap())
    .execute(app.pool())
    .await
    .expect("failed to open check-in window");

    let url = format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/check-in");

    // Unrelated authenticated user: 403.
    let (outsider_user, outsider_player) = create_test_player(&app, "reg_checkin_outsider").await;
    let outsider_token = create_test_token(
        outsider_user,
        outsider_player,
        "reg_checkin_outsider",
        TEST_JWT_SECRET,
    );
    let response = app.post_with_token(&url, &outsider_token).await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Anonymous: 401.
    let response = app.post_json_no_auth(&url, &json!({})).await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // Nothing was written.
    let checked_in: bool =
        sqlx::query_scalar("SELECT checked_in FROM tournament_registrations WHERE id = $1")
            .bind(Uuid::parse_str(&registration_id).unwrap())
            .fetch_one(app.pool())
            .await
            .expect("registration should exist");
    assert!(
        !checked_in,
        "a denied check-in must not mark the registration checked in"
    );

    // The registered player still can.
    let response = app.post_with_token(&url, &owner_token).await;
    response.assert_status(StatusCode::OK);

    let checked_in: bool =
        sqlx::query_scalar("SELECT checked_in FROM tournament_registrations WHERE id = $1")
            .bind(Uuid::parse_str(&registration_id).unwrap())
            .fetch_one(app.pool())
            .await
            .expect("registration should exist");
    assert!(checked_in, "the registered player's check-in must land");
}

/// Approving an already-approved registration must succeed, not 400.
///
/// P-2 made `Open` tournaments auto-approve on signup, so an organiser pressing
/// Approve on such a registration was hitting
/// `400 Cannot approve registration in approved status`. The same applied to a
/// double-click or two organisers acting simultaneously. See P-36.
#[tokio::test]
async fn test_approve_is_idempotent_for_an_already_approved_registration() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "approve-idempotent", "open").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "AlreadyApproved" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["status"], "approved",
        "P-2: open auto-approves"
    );
    let registration_id = body["data"]["id"].as_str().unwrap().to_string();

    // Pressing Approve on it must succeed and leave it approved.
    let url = format!("/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve");
    let response = app.post_auth(&url).await;
    response.assert_status(StatusCode::OK);

    // And again — genuinely idempotent, not merely tolerant of the second call.
    let response = app.post_auth(&url).await;
    response.assert_status(StatusCode::OK);

    let stored: String =
        sqlx::query_scalar("SELECT status FROM tournament_registrations WHERE id = $1")
            .bind(registration_id.parse::<Uuid>().unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(stored, "approved", "still approved after repeated approves");
}

/// A terminal registration is still NOT approvable — the idempotency above is
/// deliberately narrow and must not have opened that up.
#[tokio::test]
async fn test_approve_still_rejects_a_withdrawn_registration() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "approve-withdrawn", "approval").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Withdrawer" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let registration_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    sqlx::query("UPDATE tournament_registrations SET status = 'withdrawn' WHERE id = $1")
        .bind(registration_id.parse::<Uuid>().unwrap())
        .execute(app.pool())
        .await
        .unwrap();

    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// P-27: invite_only IS ENFORCED
//
// `register_team` / `register_player` checked only `is_registration_open()`,
// so `invite_only` behaved exactly like `approval` — anyone could register
// and the organiser had to reject the ones they had not invited. The setting
// is now backed by `tournament_invitations` (migration 0078).
// ============================================================================

/// Create a user with a player record plus a JWT for them.
async fn create_user_with_token(app: &TestApp, tag: &str) -> (Uuid, String) {
    let user = UserBuilder::new()
        .username(format!("invitee_{tag}"))
        .build_persisted(app.pool())
        .await;
    let token = create_test_token(user.id, user.id, &format!("invitee_{tag}"), TEST_JWT_SECRET);
    (user.id, token)
}

/// An uninvited user is refused: 403, and no registration row is written.
#[tokio::test]
async fn test_invite_only_refuses_uninvited_player() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-only-refuses", "invite_only").await;
    let (_user_id, token) = create_user_with_token(&app, "refused").await;

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Gatecrasher" }),
            &token,
        )
        .await;

    response.assert_status(StatusCode::FORBIDDEN);

    // Refused, not merely flagged for an organiser: nothing was persisted.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tournament_registrations WHERE tournament_id = $1",
    )
    .bind(tournament_id.parse::<Uuid>().unwrap())
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(count, 0, "an uninvited registration must not be stored");
}

/// An invited user gets in — and still lands `pending`, because an
/// invitation is permission to enter, not the organiser's approval.
#[tokio::test]
async fn test_invite_only_admits_invited_player() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-only-admits", "invite_only").await;
    let (user_id, token) = create_user_with_token(&app, "admitted").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &json!({ "user_id": user_id.to_string(), "message": "you're in" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let invitation_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Invited" }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["status"], "pending",
        "an invitation is not an approval"
    );

    // Registering consumed the invitation.
    let stored: String =
        sqlx::query_scalar("SELECT status FROM tournament_invitations WHERE id = $1")
            .bind(invitation_id.parse::<Uuid>().unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(stored, "accepted");

    // Still approvable by the organiser, exactly like `approval`.
    let registration_id = body["data"]["id"].as_str().unwrap();
    let response = app
        .post_auth(&format!(
            "/v1/tournaments/{tournament_id}/registrations/{registration_id}/approve"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]["status"],
        "approved"
    );
}

/// Revoking closes the door again.
#[tokio::test]
async fn test_invite_only_refuses_after_invitation_revoked() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-only-revoked", "invite_only").await;
    let (user_id, token) = create_user_with_token(&app, "revoked").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &json!({ "user_id": user_id.to_string() }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let invitation_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.delete_auth(&format!(
        "/v1/tournaments/{tournament_id}/invitations/{invitation_id}"
    ))
    .await
    .assert_status(StatusCode::OK);

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Uninvited Again" }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

/// The invite list is per-target: inviting one user does not admit another.
#[tokio::test]
async fn test_invite_only_invitation_is_not_transferable() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-only-per-user", "invite_only").await;
    let (invited_id, _invited_token) = create_user_with_token(&app, "holder").await;
    let (_other_id, other_token) = create_user_with_token(&app, "other").await;

    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/invitations"),
        &json!({ "user_id": invited_id.to_string() }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/player"),
            &json!({ "participant_name": "Borrowed Invite" }),
            &other_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

/// Team tournaments gate on the team-season, not the captain's account.
#[tokio::test]
async fn test_invite_only_team_registration_requires_invitation() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Invite Only Team Cup",
                "slug": "invite-only-team-cup",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "min_participants": 2,
                "max_participants": 8,
                "registration_type": "invite_only",
                "scheduling_mode": "live",
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

    let (team_season_id, captain_token) = create_team_with_captain(&app, "iotc").await;
    let body = json!({
        "team_season_id": team_season_id,
        "participant_name": "Uninvited Squad"
    });

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/registrations/team"),
            &body,
            &captain_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // Invite the team-season, and the same request now succeeds.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/invitations"),
        &json!({ "team_season_id": team_season_id }),
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
    response.assert_status(StatusCode::CREATED);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]["participant_name"],
        "Uninvited Squad"
    );
}

/// The other registration types are unaffected — the invite list gates
/// `invite_only` only.
#[tokio::test]
async fn test_non_invite_only_tournaments_need_no_invitation() {
    for registration_type in ["open", "approval", "qualification"] {
        let app = TestApp::new().await;
        let tournament_id = create_tournament_with_registration_type(
            &app,
            &format!("no-invite-needed-{}", registration_type.replace('_', "-")),
            registration_type,
        )
        .await;
        let (_user_id, token) = create_user_with_token(&app, registration_type).await;

        let response = app
            .post_json_with_token(
                &format!("/v1/tournaments/{tournament_id}/registrations/player"),
                &json!({ "participant_name": "NoInviteNeeded" }),
                &token,
            )
            .await;
        response.assert_status(StatusCode::CREATED);
    }
}

/// Only someone who can manage participants may hand out invitations.
#[tokio::test]
async fn test_invitation_requires_participants_manage() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-perm", "invite_only").await;
    let (target_id, _target_token) = create_user_with_token(&app, "target").await;
    let (_outsider_id, outsider_token) = create_user_with_token(&app, "outsider").await;

    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &json!({ "user_id": target_id.to_string() }),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    // P-51: the invite list now self-scopes rather than returning 403 to a
    // non-organiser. This outsider cannot manage participants AND was invited
    // to nothing, so they get 200 with an empty list — they can act on their
    // own (absent) invitation but still learn nothing about who else was
    // invited. (Reversed the prior 403 assertion deliberately; see the
    // `list_invitations` handler doc and `test_invitation_list_self_scopes_to_invitee`.)
    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &outsider_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        0,
        "a non-organiser with no invitation must see an empty list, not everyone's invites"
    );
}

/// P-51: the invite-list endpoint self-scopes for non-organisers. An invited
/// caller sees ONLY their own invitation (not everyone's); an uninvited caller
/// sees none; the organiser still sees the full list. This is the
/// invitee-readable signal the registration-card gate needs to become a hard
/// block instead of the soft P-47 precondition.
#[tokio::test]
async fn test_invitation_list_self_scopes_to_invitee() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-self-scope", "invite_only").await;
    let (invitee_id, invitee_token) = create_user_with_token(&app, "invitee").await;
    let (other_id, _other_token) = create_user_with_token(&app, "other-invitee").await;
    let (_uninvited_id, uninvited_token) = create_user_with_token(&app, "uninvited").await;

    // Organiser (default dev/admin token) invites two different users.
    for target in [invitee_id, other_id] {
        app.post_json(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &json!({ "user_id": target.to_string() }),
        )
        .await
        .assert_status(StatusCode::CREATED);
    }

    // The invitee sees exactly their own invitation — not the other person's.
    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &invitee_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "invitee sees only their own invitation");
    assert_eq!(
        rows[0]["user_id"].as_str().unwrap(),
        invitee_id.to_string(),
        "the single visible row must be the caller's own invitation"
    );

    // An uninvited caller sees nothing.
    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{tournament_id}/invitations"),
            &uninvited_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "an uninvited caller sees no invitations"
    );

    // The organiser still sees the full list (both invitations).
    let response = app
        .get_auth(&format!("/v1/tournaments/{tournament_id}/invitations"))
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(
        response.json::<serde_json::Value>()["data"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "the organiser sees every invitation"
    );
}

/// An invitation must name exactly one target, and it must match the
/// tournament's participant type.
#[tokio::test]
async fn test_invitation_target_validation() {
    let app = TestApp::new().await;
    let tournament_id =
        create_tournament_with_registration_type(&app, "invite-target-validation", "invite_only")
            .await;
    let (user_id, _token) = create_user_with_token(&app, "validation").await;
    let (team_season_id, _captain_token) = create_team_with_captain(&app, "itv").await;

    // Neither target.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/invitations"),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // Both targets.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/invitations"),
        &json!({ "user_id": user_id.to_string(), "team_season_id": team_season_id }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);

    // Team target on an individual tournament.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/invitations"),
        &json!({ "team_season_id": team_season_id }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

/// An invitation belongs to its tournament: it cannot be revoked through a
/// different tournament the caller happens to administer.
#[tokio::test]
async fn test_invitation_revoke_is_scoped_to_its_tournament() {
    let app = TestApp::new().await;
    let tournament_a =
        create_tournament_with_registration_type(&app, "invite-scope-a", "invite_only").await;
    let tournament_b =
        create_tournament_with_registration_type(&app, "invite-scope-b", "invite_only").await;
    let (user_id, token) = create_user_with_token(&app, "scoped").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_a}/invitations"),
            &json!({ "user_id": user_id.to_string() }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let invitation_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.delete_auth(&format!(
        "/v1/tournaments/{tournament_b}/invitations/{invitation_id}"
    ))
    .await
    .assert_status(StatusCode::NOT_FOUND);

    // The invitation is untouched — it still admits its holder to A.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_a}/registrations/player"),
            &json!({ "participant_name": "Still Invited" }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
}
