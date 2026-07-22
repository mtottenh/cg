//! Partial-write recovery tests (audit: multi-write sequences with no
//! transaction).
//!
//! Each scenario below is a service method that performs two or more writes
//! without a transaction, guarded by a read that the FIRST write already
//! satisfies. If the process dies between the writes — or the second write
//! simply fails — the guard rejects every subsequent retry and the entity is
//! stranded forever.
//!
//! The tests stage the post-crash database state directly with sqlx (or force
//! the second write to fail through a real constraint) and then assert the
//! system recovers. No fault-injection framework is involved.

use crate::common::TestApp;
use crate::tournaments::create_tournament_with_matches;
use axum::http::StatusCode;
use portal_api::background::{LifecycleConfig, run_lifecycle_pass};
use portal_api::state::AppState;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

// ===========================================================================
// SHARED HELPERS
// ===========================================================================

async fn state_for(app: &TestApp) -> AppState {
    AppState::new(app.pool().clone(), "test-jwt-secret").await
}

fn test_config() -> LifecycleConfig {
    LifecycleConfig {
        tick_interval: std::time::Duration::from_secs(30),
        check_in_lead: chrono::Duration::minutes(15),
        check_in_grace: chrono::Duration::minutes(10),
        evidence_stale_max_age: chrono::Duration::hours(24),
        evidence_sweep_every: 20,
        saga_stuck_after: chrono::Duration::minutes(10),
        batch_limit: 100,
    }
}

async fn schedule_at(app: &TestApp, tournament_id: &str, match_id: &str, offset_minutes: i64) {
    let at = chrono::Utc::now() + chrono::Duration::minutes(offset_minutes);
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({
                "scheduled_at": at.to_rfc3339(),
                "reason": "partial-write recovery test"
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

async fn match_status(app: &TestApp, match_id: &str) -> String {
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM tournament_matches WHERE id = $1")
            .bind(Uuid::parse_str(match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    status
}

async fn forfeit_record_count(app: &TestApp, match_id: &str) -> i64 {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM forfeit_records WHERE match_id = $1")
            .bind(Uuid::parse_str(match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    count
}

/// Stage the post-crash state of `process_forfeit`: the `forfeit_records`
/// insert committed, the `tournament_matches` update never happened.
async fn stage_orphan_forfeit_record(app: &TestApp, match_id: &str, registration_id: &str) {
    sqlx::query(
        r"INSERT INTO forfeit_records
            (match_id, forfeiting_registration_id, forfeit_type, reason, triggered_by_system)
          VALUES ($1, $2, 'no_show', 'staged partial write (crash before match update)', true)",
    )
    .bind(Uuid::parse_str(match_id).unwrap())
    .bind(Uuid::parse_str(registration_id).unwrap())
    .execute(app.pool())
    .await
    .expect("staging the orphan forfeit record must succeed");
}

fn create_token_for_user(user_id: Uuid) -> String {
    use portal_domain::generate_access_token;
    generate_access_token(user_id, user_id, "testuser", TEST_JWT_SECRET)
        .expect("failed to create token")
}

/// Give the dev user the global `league_admin` role (mirrors the helper in
/// `leagues.rs`, duplicated here so that file stays untouched).
async fn grant_league_admin_permission(app: &TestApp) {
    use sqlx::Row;

    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let role_row = sqlx::query("SELECT id FROM roles WHERE name = 'league_admin'")
        .fetch_optional(app.pool())
        .await
        .expect("query should succeed");

    let role_id: Uuid = if let Some(row) = role_row {
        row.get("id")
    } else {
        let row = sqlx::query(
            "INSERT INTO roles (id, name, description, is_global) VALUES (gen_random_uuid(), 'league_admin', 'League administrator', false) RETURNING id",
        )
        .fetch_one(app.pool())
        .await
        .expect("failed to create role");
        row.get("id")
    };

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(dev_user_id)
        .bind(role_id)
        .execute(app.pool())
        .await
        .expect("failed to assign role");
}

async fn is_league_member(app: &TestApp, league_id: &str, user_id: Uuid) -> bool {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM league_members WHERE league_id = $1 AND user_id = $2")
            .bind(Uuid::parse_str(league_id).unwrap())
            .bind(user_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    count > 0
}

async fn invitation_status(app: &TestApp, invitation_id: &str) -> String {
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM league_invitations WHERE id = $1")
            .bind(Uuid::parse_str(invitation_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();
    status
}

// ===========================================================================
// BUG 1 — FORFEIT RECORDS STRAND THE MATCH
//
// services/tournament/forfeit.rs:44 process_forfeit
//   exists_for_match (read guard, :59) -> forfeit_repo.create (:70)
//                                      -> match_repo.forfeit (:82)
// migrations/0038_forfeits.sql has no unique constraint on forfeit_records,
// so the only protection is that application-level read. Once the insert
// commits, every retry is rejected with InvalidState and the match never
// leaves its pre-forfeit status.
// ===========================================================================

/// A forfeit record exists for the match but the match was never flipped to
/// `forfeit`. Re-running the forfeit must RECOVER the match.
#[tokio::test]
async fn test_forfeit_recovers_when_record_already_exists() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "pwr-forfeit-strand").await;

    // `ready` is not a forfeitable status — put the match in `scheduled`
    // (well outside the check-in lead window) so the only thing standing
    // between the retry and success is the exists_for_match guard.
    schedule_at(&app, &tournament_id, &match_id, 120).await;
    assert_eq!(match_status(&app, &match_id).await, "scheduled");

    // Post-crash state: record written, match untouched.
    stage_orphan_forfeit_record(&app, &match_id, &reg1).await;
    assert_ne!(
        match_status(&app, &match_id).await,
        "forfeit",
        "precondition: the staged match must NOT be forfeited yet"
    );

    // Retry the forfeit exactly as an admin (or the no-show pass) would.
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/forfeit"),
            &json!({
                "forfeiting_registration_id": reg1,
                "forfeit_type": "no_show",
                "reason": "Retry after partial write"
            }),
        )
        .await;

    let status = match_status(&app, &match_id).await;
    assert_eq!(
        status,
        "forfeit",
        "match must recover to `forfeit` after retrying a forfeit whose record \
         was already written (HTTP {} / body {}); today the exists_for_match \
         guard refuses the retry and the match is stranded forever",
        response.status,
        response.text()
    );
}

/// The same strand seen through the background no-show pass
/// (`background/mod.rs:605`): the pass re-errors on every tick and the match
/// stays in `checking_in` indefinitely.
#[tokio::test]
async fn test_lifecycle_no_show_recovers_when_forfeit_record_already_exists() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, reg1, reg2) =
        create_tournament_with_matches(&app, "pwr-noshow-strand").await;

    schedule_at(&app, &tournament_id, &match_id, 5).await;
    run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(match_status(&app, &match_id).await, "checking_in");

    // Participant 1 (the dev user) checks in; participant 2 is the no-show.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
            &json!({ "registration_id": reg1 }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Post-crash state from an earlier tick: the no-show forfeit record was
    // written but the match status update never landed.
    stage_orphan_forfeit_record(&app, &match_id, &reg2).await;

    sqlx::query(
        "UPDATE tournament_matches SET check_in_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    let summary = run_lifecycle_pass(&state, &test_config(), false).await;

    assert_eq!(
        match_status(&app, &match_id).await,
        "forfeit",
        "the no-show pass must recover the stranded match instead of \
         re-erroring every 30s (summary: {summary:?})"
    );
    assert_eq!(
        summary.errors, 0,
        "the no-show pass must not error on a match whose forfeit record \
         already exists: {summary:?}"
    );
}

/// `process_double_forfeit` (forfeit.rs:123) writes TWO forfeit records and
/// then updates the match. A crash after the first insert strands the match
/// the same way.
#[tokio::test]
async fn test_double_forfeit_recovers_after_partial_first_insert() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "pwr-dblff-strand").await;

    // Post-crash state: only participant 1's record was inserted.
    stage_orphan_forfeit_record(&app, &match_id, &reg1).await;
    assert_eq!(forfeit_record_count(&app, &match_id).await, 1);

    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/double-forfeit"),
            &json!({ "reason": "Retry after partial double-forfeit write" }),
        )
        .await;

    let status = match_status(&app, &match_id).await;
    assert_eq!(
        status,
        "cancelled",
        "double forfeit must recover the match to `cancelled` after a partial \
         first insert (HTTP {} / body {}); today exists_for_match refuses the \
         retry and the match is stranded with one orphan record",
        response.status,
        response.text()
    );
    assert_eq!(
        forfeit_record_count(&app, &match_id).await,
        2,
        "both participants must end up with exactly one forfeit record"
    );
}

// ===========================================================================
// BUG 2 — VETO SESSION WEDGES ON THE ACTION CURSOR
//
// services/tournament/veto.rs:593 record_action_internal
//   action_repo.create (:606) -> [update_side_selection (:639)]
//                             -> session_repo.update (:686)
// migrations/0034_veto_sessions.sql:74 has
//   CONSTRAINT veto_actions_unique UNIQUE (session_id, action_number)
// so if the process dies after the action insert but before the session
// cursor advances, every retry hits the unique violation and the session is
// permanently wedged — including process_timeout (veto.rs:376).
// ===========================================================================

#[tokio::test]
async fn test_veto_recovers_when_action_row_already_exists() {
    let app = TestApp::new().await;
    let fixture = TwoTeamMatchFixture::with_veto(app.pool(), TEST_JWT_SECRET).await;
    let session_id = fixture
        .veto_session_id
        .expect("fixture must create a veto session");
    let match_id = fixture.match_id;
    let map = "de_ancient";

    // Precondition: session is in progress at action 1, team A to act.
    let (cursor,): (i32,) =
        sqlx::query_as("SELECT current_action_number FROM veto_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(cursor, 1, "precondition: session starts at action 1");

    // Post-crash state: action 1 is committed, the session cursor is still 1.
    sqlx::query(
        r"INSERT INTO veto_actions
            (session_id, action_number, action_type, map_id,
             performed_by_registration_id, was_auto_action)
          VALUES ($1, 1, 'ban', $2, $3, false)",
    )
    .bind(session_id)
    .bind(map)
    .bind(fixture.reg_a_id)
    .execute(app.pool())
    .await
    .expect("staging the orphan veto action must succeed");

    // The correct team retries the same action.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/veto/action"),
            &json!({ "map_id": map }),
            &fixture.team_a.captain.token,
        )
        .await;

    let (cursor, remaining): (i32, Vec<String>) = sqlx::query_as(
        "SELECT current_action_number, remaining_maps FROM veto_sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_one(app.pool())
    .await
    .unwrap();

    assert_eq!(
        cursor,
        2,
        "the veto session cursor must advance past an action row that already \
         exists (HTTP {} / body {}); today the retry hits veto_actions_unique \
         and the session is wedged forever",
        response.status,
        response.text()
    );
    assert!(
        !remaining.contains(&map.to_string()),
        "the banned map must be removed from remaining_maps on recovery"
    );
}

/// The same wedge blocks the timeout path: with the cursor stuck the
/// background timeout sweep can never clear the session either.
#[tokio::test]
async fn test_veto_timeout_recovers_when_action_row_already_exists() {
    let app = TestApp::new().await;
    let fixture = TwoTeamMatchFixture::with_veto(app.pool(), TEST_JWT_SECRET).await;
    let session_id = fixture
        .veto_session_id
        .expect("fixture must create a veto session");
    let map = "de_ancient";

    sqlx::query(
        r"INSERT INTO veto_actions
            (session_id, action_number, action_type, map_id,
             performed_by_registration_id, was_auto_action)
          VALUES ($1, 1, 'ban', $2, $3, true)",
    )
    .bind(session_id)
    .bind(map)
    .bind(fixture.reg_a_id)
    .execute(app.pool())
    .await
    .expect("staging the orphan veto action must succeed");

    // Expire the action deadline so the session is a timeout candidate.
    sqlx::query(
        "UPDATE veto_sessions SET action_deadline = NOW() - INTERVAL '1 minute' WHERE id = $1",
    )
    .bind(session_id)
    .execute(app.pool())
    .await
    .unwrap();

    let state = state_for(&app).await;
    let timed_out = state
        .veto_service
        .find_timed_out_sessions()
        .await
        .expect("listing timed-out sessions must succeed");
    assert!(
        timed_out.iter().any(|s| s.id.as_uuid() == session_id),
        "precondition: the wedged session must be visible to the timeout sweep"
    );

    let result = state
        .veto_service
        .process_timeout(portal_core::VetoSessionId::from(session_id))
        .await;

    assert!(
        result.is_ok(),
        "process_timeout must recover a session whose action row already \
         exists, got: {:?}",
        result.err()
    );

    let (cursor,): (i32,) =
        sqlx::query_as("SELECT current_action_number FROM veto_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        cursor, 2,
        "the timeout sweep must advance the wedged cursor rather than \
         re-failing on veto_actions_unique every tick"
    );
}

// ===========================================================================
// BUG 3 — CHECK-IN WINDOW ORPHANS THE MATCH
//
// background/mod.rs:225-275 open_check_in_window transitions the match to
// CheckingIn (:233) and then separately stamps check_in_deadline (:255). On
// failure of the second write it logs but does NOT return (:259-262) and
// still counts the window as opened (:264).
//
// list_scheduled_due requires status='scheduled'
//   (adapters/tournament/match_.rs:216-219)
// list_checkin_expired requires check_in_deadline IS NOT NULL (:241-244)
//
// A match in checking_in with a NULL deadline is therefore invisible to BOTH
// passes — forever.
// ===========================================================================

#[tokio::test]
async fn test_check_in_window_orphan_is_not_stranded() {
    let app = TestApp::new().await;
    let state = state_for(&app).await;
    let (tournament_id, match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "pwr-checkin-orphan").await;

    schedule_at(&app, &tournament_id, &match_id, 5).await;
    run_lifecycle_pass(&state, &test_config(), false).await;
    assert_eq!(match_status(&app, &match_id).await, "checking_in");

    // Post-crash state: the status transition landed, the deadline stamp did
    // not (exactly what open_check_in_window leaves behind when
    // set_check_in_deadline fails).
    sqlx::query("UPDATE tournament_matches SET check_in_deadline = NULL WHERE id = $1")
        .bind(Uuid::parse_str(&match_id).unwrap())
        .execute(app.pool())
        .await
        .unwrap();

    // Give the automation several ticks to notice and repair.
    for _ in 0..3 {
        run_lifecycle_pass(&state, &test_config(), false).await;
    }

    let (status, deadline): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, check_in_deadline FROM tournament_matches WHERE id = $1")
            .bind(Uuid::parse_str(&match_id).unwrap())
            .fetch_one(app.pool())
            .await
            .unwrap();

    let recovered = deadline.is_some() || matches!(status.as_str(), "forfeit" | "cancelled");
    assert!(
        recovered,
        "a match left in `checking_in` with a NULL check_in_deadline must be \
         re-windowed or no-showed by the lifecycle automation; it is instead \
         invisible to list_scheduled_due (status != 'scheduled') AND to \
         list_checkin_expired (check_in_deadline IS NULL) — status={status}, \
         deadline={deadline:?}"
    );
}

// ===========================================================================
// BUG 4 — LEAGUE INVITATION ACCEPTANCE STRANDS THE USER
//
// services/league.rs:502 accept_invitation
//   invitation_repo.update_status(Accepted) (:531)
//     -> member_repo.add_member (:536)
// adapters/league.rs:409-413 add_member is a bare INSERT with no ON CONFLICT,
// and league_members carries UNIQUE (league_id, user_id)
// (migrations/0021_create_league_members.sql:11).
//
// The identical bug was already fixed for league TEAMS via the atomic
// accept_and_add_member (services/league_team/invitation.rs:332-351 /
// adapters/league_team/invitation.rs:244).
//
// Same shape in approve_application_authorized (league.rs:557).
// ===========================================================================

/// (a) Post-crash state: the invitation is Accepted but no membership row
/// exists. The user must be able to recover their membership.
#[tokio::test]
async fn test_accept_invitation_recovers_when_invitation_already_accepted() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("pwr-accept-stranded")
        .email("pwr-accept-stranded@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let created: serde_json::Value = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "PWR Accept Stranded League",
                "slug": "pwr-accept-stranded",
                "access_type": "invite_only"
            }),
        )
        .await
        .json();
    let league_id = created["data"]["id"].as_str().unwrap().to_string();

    let invite: serde_json::Value = app
        .post_json(
            &format!("/v1/leagues/{league_id}/invitations"),
            &json!({ "user_id": user2.id.to_string() }),
        )
        .await
        .json();
    let invitation_id = invite["data"]["id"].as_str().unwrap().to_string();

    // Post-crash state: status flipped, membership never inserted.
    sqlx::query(
        "UPDATE league_invitations SET status = 'accepted', responded_at = NOW() WHERE id = $1",
    )
    .bind(Uuid::parse_str(&invitation_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    assert!(
        !is_league_member(&app, &league_id, user2.id).await,
        "precondition: the user must not be a member yet"
    );

    // The user retries acceptance.
    let response = app
        .post_with_token(
            &format!("/v1/league-invitations/{invitation_id}/accept"),
            &token2,
        )
        .await;

    assert!(
        is_league_member(&app, &league_id, user2.id).await,
        "a user whose invitation is Accepted but who has no membership row \
         must be able to recover membership by retrying accept (HTTP {} / \
         body {}); today the `status != Pending` guard at league.rs:519 \
         refuses the retry and the user is locked out of an invite-only \
         league forever",
        response.status,
        response.text()
    );
}

/// (b) Force the second write to fail naturally: the user is already a
/// member, so `add_member`'s bare INSERT violates
/// `league_members_unique (league_id, user_id)`. The invitation must NOT be
/// left marked Accepted.
#[tokio::test]
async fn test_accept_invitation_is_atomic_when_membership_insert_fails() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("pwr-accept-atomic")
        .email("pwr-accept-atomic@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let created: serde_json::Value = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "PWR Accept Atomic League",
                "slug": "pwr-accept-atomic",
                "access_type": "invite_only"
            }),
        )
        .await
        .json();
    let league_id = created["data"]["id"].as_str().unwrap().to_string();

    let invite: serde_json::Value = app
        .post_json(
            &format!("/v1/leagues/{league_id}/invitations"),
            &json!({ "user_id": user2.id.to_string() }),
        )
        .await
        .json();
    let invitation_id = invite["data"]["id"].as_str().unwrap().to_string();

    // Make the SECOND write fail for real: seat the user first.
    sqlx::query(
        "INSERT INTO league_members (league_id, user_id, membership_type) VALUES ($1, $2, 'member')",
    )
    .bind(Uuid::parse_str(&league_id).unwrap())
    .bind(user2.id)
    .execute(app.pool())
    .await
    .unwrap();

    let response = app
        .post_with_token(
            &format!("/v1/league-invitations/{invitation_id}/accept"),
            &token2,
        )
        .await;

    let status = invitation_status(&app, &invitation_id).await;
    assert_ne!(
        status,
        "accepted",
        "when add_member fails the invitation must not be left marked \
         `accepted` (HTTP {} / body {}); the status flip and the membership \
         insert must commit together, exactly like the league-team fix in \
         adapters/league_team/invitation.rs:244 accept_and_add_member",
        response.status,
        response.text()
    );
}

/// (a) for `approve_application_authorized` (league.rs:557) — same shape.
#[tokio::test]
async fn test_approve_application_recovers_when_application_already_accepted() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("pwr-approve-stranded")
        .email("pwr-approve-stranded@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let created: serde_json::Value = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "PWR Approve Stranded League",
                "slug": "pwr-approve-stranded",
                "access_type": "application"
            }),
        )
        .await
        .json();
    let league_id = created["data"]["id"].as_str().unwrap().to_string();

    let application: serde_json::Value = app
        .post_json_with_token(
            &format!("/v1/leagues/{league_id}/apply"),
            &json!({ "message": "Please accept me!" }),
            &token2,
        )
        .await
        .json();
    let application_id = application["data"]["id"].as_str().unwrap().to_string();

    // Post-crash state: status flipped, membership never inserted.
    sqlx::query(
        "UPDATE league_invitations SET status = 'accepted', responded_at = NOW() WHERE id = $1",
    )
    .bind(Uuid::parse_str(&application_id).unwrap())
    .execute(app.pool())
    .await
    .unwrap();
    assert!(!is_league_member(&app, &league_id, user2.id).await);

    let response = app
        .post_auth(&format!(
            "/v1/leagues/{league_id}/applications/{application_id}/approve"
        ))
        .await;

    assert!(
        is_league_member(&app, &league_id, user2.id).await,
        "an application marked Accepted with no membership row must be \
         recoverable by re-approving (HTTP {} / body {}); today the \
         `status != Pending` guard at league.rs:569 refuses the retry and the \
         applicant is stranded",
        response.status,
        response.text()
    );
}

/// (b) for `approve_application_authorized`: natural constraint violation on
/// the second write must not leave the application marked Accepted.
#[tokio::test]
async fn test_approve_application_is_atomic_when_membership_insert_fails() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    grant_league_admin_permission(&app).await;

    let user2 = UserBuilder::new()
        .username("pwr-approve-atomic")
        .email("pwr-approve-atomic@example.com")
        .build_persisted(app.pool())
        .await;
    let token2 = create_token_for_user(user2.id);

    let created: serde_json::Value = app
        .post_json(
            "/v1/leagues",
            &json!({
                "game_id": game_id,
                "name": "PWR Approve Atomic League",
                "slug": "pwr-approve-atomic",
                "access_type": "application"
            }),
        )
        .await
        .json();
    let league_id = created["data"]["id"].as_str().unwrap().to_string();

    let application: serde_json::Value = app
        .post_json_with_token(
            &format!("/v1/leagues/{league_id}/apply"),
            &json!({ "message": "Please accept me!" }),
            &token2,
        )
        .await
        .json();
    let application_id = application["data"]["id"].as_str().unwrap().to_string();

    sqlx::query(
        "INSERT INTO league_members (league_id, user_id, membership_type) VALUES ($1, $2, 'member')",
    )
    .bind(Uuid::parse_str(&league_id).unwrap())
    .bind(user2.id)
    .execute(app.pool())
    .await
    .unwrap();

    let response = app
        .post_auth(&format!(
            "/v1/leagues/{league_id}/applications/{application_id}/approve"
        ))
        .await;

    let status = invitation_status(&app, &application_id).await;
    assert_ne!(
        status,
        "accepted",
        "when add_member fails the application must not be left marked \
         `accepted` (HTTP {} / body {})",
        response.status,
        response.text()
    );
}
