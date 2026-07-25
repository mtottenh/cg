//! Admin progression endpoint integration tests.
//!
//! Covers `/v1/admin/matches/{id}/progression/{process,reapply,revert}` —
//! including the launch-blocker regression that the admin/dispute-reapply
//! path must PERSIST standings (the old implementation mutated an
//! in-memory Vec and re-ranked unchanged stored points), plus the
//! previously untested permission-denied cases.

use crate::common::TestApp;
use crate::results::create_rr_match_in_progress;
use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

/// Register a plain (non-admin) user via the API and return their token.
async fn register_plain_user(app: &TestApp, username: &str) -> String {
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": username,
                "email": format!("{}@example.com", username),
                "password": "SecurePass123!",
                "display_name": username
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json::<serde_json::Value>()["data"]["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Confirm a 2-0 win for participant 1 through the claim flow, completing
/// the match and applying standings once via the completion saga.
async fn complete_match_via_claims(
    app: &TestApp,
    match_id: &str,
    p1_reg: &str,
    opponent_token: &str,
) {
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_with_token(
            &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
            opponent_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
}

/// Fetch (matches_played, matches_won, matches_lost, points) for a
/// registration in a bracket.
async fn standing_for(
    app: &TestApp,
    bracket_id: Uuid,
    registration_id: &str,
) -> (i32, i32, i32, i32) {
    sqlx::query_as(
        "SELECT matches_played, matches_won, matches_lost, points
         FROM tournament_standings
         WHERE bracket_id = $1 AND registration_id = $2",
    )
    .bind(bracket_id)
    .bind(Uuid::parse_str(registration_id).unwrap())
    .fetch_one(app.pool())
    .await
    .expect("standing row")
}

// ============================================================================
// PERMISSION-DENIED CASES
// ============================================================================

#[tokio::test]
async fn test_progression_admin_endpoints_require_permission() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, _opp, _bracket) =
        create_rr_match_in_progress(&app, "prog-perm-denied").await;
    let token = register_plain_user(&app, "prog_regular_user").await;

    let response = app
        .post_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/revert"),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .post_json_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p2_reg }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);

    let response = app
        .post_json_with_token(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": p1_reg,
                "loser_registration_id": p2_reg
            }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_progression_admin_endpoints_require_authentication() {
    let app = TestApp::new().await;
    let match_id = Uuid::now_v7();

    let response = app
        .post_json_no_auth(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": Uuid::now_v7().to_string(),
                "loser_registration_id": Uuid::now_v7().to_string()
            }),
        )
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// STANDINGS PERSISTENCE (launch blocker #7 regression)
// ============================================================================

/// The admin process endpoint must PERSIST the standings it reports.
#[tokio::test]
async fn test_process_progression_persists_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, _opp, bracket_id) =
        create_rr_match_in_progress(&app, "prog-process-persist").await;

    // Complete the match directly (no saga, no standings applied) — the
    // scenario where an admin drives progression manually.
    sqlx::query(
        "UPDATE tournament_matches SET
            participant1_score = 2, participant2_score = 1,
            winner_registration_id = $2, loser_registration_id = $3,
            status = 'completed', completed_at = NOW(), updated_at = NOW()
         WHERE id = $1",
    )
    .bind(Uuid::parse_str(&match_id).unwrap())
    .bind(Uuid::parse_str(&p1_reg).unwrap())
    .bind(Uuid::parse_str(&p2_reg).unwrap())
    .execute(app.pool())
    .await
    .unwrap();

    // Standings start at zero.
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (0, 0, 0, 0));

    let response = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/process"),
            &json!({
                "winner_registration_id": p1_reg,
                "loser_registration_id": p2_reg
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["updated_standings_count"], 2);

    // The reported standings exist in the database.
    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 1, 0, 3),
        "winner standings must be persisted"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 0, 1, 0),
        "loser standings must be persisted"
    );
}

/// Reapplying with a different winner must move the persisted points:
/// the recorded result's deltas are reverted, the new winner's applied.
#[tokio::test]
async fn test_reapply_progression_swaps_persisted_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "prog-reapply-swap").await;

    // Normal flow: participant 1 wins, standings applied once by the saga.
    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));
    assert_eq!(standing_for(&app, bracket_id, &p2_reg).await, (1, 0, 1, 0));

    // Admin overturns the result: participant 2 actually won.
    let response = app
        .post_json(
            &format!("/v1/admin/matches/{match_id}/progression/reapply"),
            &json!({ "new_winner_registration_id": p2_reg }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Old winner's deltas reverted, new winner's applied — exactly once.
    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (1, 0, 1, 0),
        "overturned winner must lose the win and the points"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (1, 1, 0, 3),
        "new winner must gain the win and the points"
    );
}

/// Revert alone must subtract the recorded result's standings deltas.
#[tokio::test]
async fn test_revert_progression_subtracts_persisted_standings() {
    let app = TestApp::new().await;
    let (_t, match_id, p1_reg, p2_reg, opponent_token, bracket_id) =
        create_rr_match_in_progress(&app, "prog-revert-subtract").await;

    complete_match_via_claims(&app, &match_id, &p1_reg, &opponent_token).await;
    assert_eq!(standing_for(&app, bracket_id, &p1_reg).await, (1, 1, 0, 3));

    let response = app
        .post_auth(&format!("/v1/admin/matches/{match_id}/progression/revert"))
        .await;
    response.assert_status(StatusCode::OK);

    assert_eq!(
        standing_for(&app, bracket_id, &p1_reg).await,
        (0, 0, 0, 0),
        "revert must subtract the winner's persisted deltas"
    );
    assert_eq!(
        standing_for(&app, bracket_id, &p2_reg).await,
        (0, 0, 0, 0),
        "revert must subtract the loser's persisted deltas"
    );
}

// ============================================================================
// P-83 — REVERT ACTUALLY ROLLS BACK AN ELIMINATION ADVANCEMENT
// ============================================================================

async fn cs2_game_id(app: &TestApp) -> String {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM games WHERE slug = $1")
        .bind("cs2")
        .fetch_one(app.pool())
        .await
        .expect("cs2 game row");
    id.to_string()
}

/// Whether `registration_id` currently holds either participant slot on `match_id`.
async fn occupies_slot(
    app: &TestApp,
    tournament_id: &str,
    match_id: &str,
    registration_id: &str,
) -> bool {
    let m = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await
        .json::<serde_json::Value>()["data"]
        .clone();
    m["participant1_registration_id"].as_str() == Some(registration_id)
        || m["participant2_registration_id"].as_str() == Some(registration_id)
}

/// Create, publish, seed and start a `format` tournament with `players`
/// distinct registrations, and return its id.
///
/// `register_player` registers the *calling* identity, so the registrations are
/// inserted directly — looping the endpoint would 409 on the second player.
async fn start_bracket_tournament(
    app: &TestApp,
    slug: &str,
    format: &str,
    players: usize,
) -> String {
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "name": format!("Progression {slug}"),
                "slug": slug,
                "game_id": cs2_game_id(app).await,
                "format": format,
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": players,
                "max_participants": players,
                "registration_type": "open",
                "scheduling_mode": "self_scheduled",
                "default_match_format": "bo1"
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

    let prefix = slug.replace('-', "_");
    for i in 0..players {
        let (user_id, player_id) =
            crate::tournaments::create_test_player(app, &format!("{prefix}_{i}")).await;
        crate::tournaments::insert_test_registration(
            app,
            &tournament_id,
            player_id,
            user_id,
            &format!("P{}", i + 1),
        )
        .await;
    }

    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/seeding/auto"),
        &json!({ "method": "random" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{tournament_id}/start"))
        .await
        .assert_status(StatusCode::OK);

    tournament_id
}

/// A seeded match that feeds at least one downstream match.
///
/// The progression links are read straight off the row: `winner_progresses_to`
/// is not exposed on any DTO, and it is the precise wiring these tests are
/// about.
#[derive(Debug, Clone, Copy)]
struct FeedingMatch {
    id: Uuid,
    winner_target: Uuid,
    loser_target: Option<Uuid>,
    p1_reg: Uuid,
    p2_reg: Uuid,
}

/// Every playable match in the tournament that advances its winner somewhere,
/// oldest first (`R1M1`, `R1M2`, … for single elimination).
async fn feeding_matches(app: &TestApp, tournament_id: &str) -> Vec<FeedingMatch> {
    let rows: Vec<(Uuid, Uuid, Option<Uuid>, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, winner_progresses_to, loser_progresses_to,
                participant1_registration_id, participant2_registration_id
         FROM tournament_matches
         WHERE tournament_id = $1
           AND winner_progresses_to IS NOT NULL
           AND participant1_registration_id IS NOT NULL
           AND participant2_registration_id IS NOT NULL
         ORDER BY created_at",
    )
    .bind(Uuid::parse_str(tournament_id).unwrap())
    .fetch_all(app.pool())
    .await
    .expect("read the bracket wiring");

    rows.into_iter()
        .map(|(id, winner_target, loser_target, p1, p2)| FeedingMatch {
            id,
            winner_target,
            loser_target,
            p1_reg: p1.unwrap(),
            p2_reg: p2.unwrap(),
        })
        .collect()
}

/// Record a result on a match the way a completed match carries one.
///
/// Progression always follows a COMPLETED match in production, so the row holds
/// its winner before anything advances; the admin `progression/process`
/// endpoint is a repair tool pointed at such a match.
async fn record_result(app: &TestApp, match_id: Uuid, winner: Uuid, loser: Uuid, p1: i32, p2: i32) {
    sqlx::query(
        "UPDATE tournament_matches
         SET winner_registration_id = $2, loser_registration_id = $3,
             participant1_score = $4, participant2_score = $5,
             status = 'completed', completed_at = NOW()
         WHERE id = $1",
    )
    .bind(match_id)
    .bind(winner)
    .bind(loser)
    .bind(p1)
    .bind(p2)
    .execute(app.pool())
    .await
    .expect("record the match result");
}

/// Both participant slots of a match, straight off the row.
async fn slots(app: &TestApp, match_id: Uuid) -> (Option<Uuid>, Option<Uuid>) {
    sqlx::query_as(
        "SELECT participant1_registration_id, participant2_registration_id
         FROM tournament_matches WHERE id = $1",
    )
    .bind(match_id)
    .fetch_one(app.pool())
    .await
    .expect("read the match slots")
}

async fn match_status(app: &TestApp, match_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM tournament_matches WHERE id = $1")
        .bind(match_id)
        .fetch_one(app.pool())
        .await
        .expect("read the match status")
}

async fn bracket_type_of(app: &TestApp, match_id: Uuid) -> String {
    sqlx::query_scalar(
        "SELECT b.bracket_type FROM tournament_brackets b
         JOIN tournament_matches m ON m.bracket_id = b.id
         WHERE m.id = $1",
    )
    .bind(match_id)
    .fetch_one(app.pool())
    .await
    .expect("read the bracket type")
}

/// Drive `progression/process` for a match whose result is already recorded.
async fn process_progression(app: &TestApp, m: &FeedingMatch, winner: Uuid, loser: Uuid) {
    app.post_json(
        &format!("/v1/admin/matches/{}/progression/process", m.id),
        &json!({
            "winner_registration_id": winner,
            "loser_registration_id": loser
        }),
    )
    .await
    .assert_status(StatusCode::OK);
}

/// Play a semi-final and advance its winner: `p1_reg` beats `p2_reg` 1-0.
async fn play_and_advance(app: &TestApp, m: &FeedingMatch) {
    record_result(app, m.id, m.p1_reg, m.p2_reg, 1, 0).await;
    process_progression(app, m, m.p1_reg, m.p2_reg).await;
}

/// P-83: reverting progression on a single-elimination bracket must take the
/// advanced winner back out of the downstream match.
///
/// `revert_progression` used to handle RoundRobin | Swiss only; for elimination
/// it logged "Would revert winner progression - needs implementation" and
/// returned **200**, while the admin UI's confirm dialog promised that
/// "downstream pairings created from it are rolled back". An admin got a
/// success snackbar for work that never happened, on the most
/// destructive-sounding control in the tab.
///
/// The assertion is on the FINAL's participant slot, not on the source match:
/// clearing the source result was the part that already worked, so asserting
/// that would pass against the bug.
#[tokio::test]
async fn test_revert_withdraws_the_winner_from_the_downstream_elimination_match() {
    let app = TestApp::new().await;

    // A 4-player bracket is the minimum that produces a semi-final feeding a
    // final, which is what P-83 is about.
    let tournament_id =
        start_bracket_tournament(&app, "p83-revert-elim", "single_elimination", 4).await;
    let semi = feeding_matches(&app, &tournament_id)
        .await
        .into_iter()
        .next()
        .expect("a seeded semi-final that feeds the final");

    play_and_advance(&app, &semi).await;

    let winner_reg = semi.p1_reg.to_string();
    let final_id = semi.winner_target.to_string();
    assert!(
        occupies_slot(&app, &tournament_id, &final_id, &winner_reg).await,
        "precondition: processing progression must seat the winner in the final"
    );

    // Revert it.
    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", semi.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);

    assert!(
        !occupies_slot(&app, &tournament_id, &final_id, &winner_reg).await,
        "P-83: after reverting, the winner must no longer occupy a slot in the \
         final — the old implementation logged 'needs implementation' and left \
         them seeded while reporting success"
    );
}

// ============================================================================
// P-169 — REVERT AFTER A SCORE CORRECTION, AND THE RULE FOR ROUNDS THAT HAVE
//         ALREADY MOVED ON
// ============================================================================

/// P-169: revert must withdraw whoever this match ACTUALLY advanced, not
/// whoever it currently records as the winner.
///
/// P-83 fixed the elimination no-op by clearing the slot held by
/// `match_.winner_registration_id`. Since P-72 an admin can correct a
/// confirmed-but-wrong score, and a correction that flips the winner rewrites
/// that column while deliberately leaving the bracket alone (moving
/// participants is the progression controls' job, not a side effect of a score
/// edit). So the identity revert looked for the *corrected* winner — who was
/// never advanced — found them nowhere, and returned 200 having left the
/// *wrong* player in the next round. Exactly the silent no-op P-83 was
/// supposed to have removed, on the flow P-72 had just created.
///
/// Both semis are advanced first so the final is a full pairing: reverting one
/// of them must empty that one slot, leave the other alone, and put the final
/// back to `pending` — the state it was in before this match fed it.
#[tokio::test]
async fn test_revert_withdraws_the_advanced_participant_after_a_score_correction() {
    let app = TestApp::new().await;

    let tournament_id = start_bracket_tournament(&app, "p169-corr", "single_elimination", 4).await;
    let semis = feeding_matches(&app, &tournament_id).await;
    assert_eq!(semis.len(), 2, "a 4-player bracket has two semi-finals");
    let (first, second) = (semis[0], semis[1]);
    let final_id = first.winner_target;
    assert_eq!(
        second.winner_target, final_id,
        "both semi-finals must feed the same final"
    );

    play_and_advance(&app, &first).await;
    play_and_advance(&app, &second).await;

    let before = slots(&app, final_id).await;
    assert!(
        before.0 == Some(first.p1_reg) || before.1 == Some(first.p1_reg),
        "precondition: the first semi's winner is seated in the final"
    );
    assert!(
        before.0 == Some(second.p1_reg) || before.1 == Some(second.p1_reg),
        "precondition: the second semi's winner is seated in the final"
    );
    assert_eq!(
        match_status(&app, final_id).await,
        "ready",
        "precondition: a full pairing is ready to play"
    );

    // P-72 correction: the semi was scored the wrong way round. 1-0 becomes
    // 0-2, so the recorded winner flips to participant 2 — and the bracket is
    // deliberately untouched, which is the state revert exists to repair.
    app.post_json(
        &format!(
            "/v1/admin/tournaments/{tournament_id}/matches/{}/result-override",
            first.id
        ),
        &json!({
            "participant1_score": 0,
            "participant2_score": 2,
            "reason": "Demo review: participant 2 won this map."
        }),
    )
    .await
    .assert_status(StatusCode::OK);

    let corrected: Option<Uuid> =
        sqlx::query_scalar("SELECT winner_registration_id FROM tournament_matches WHERE id = $1")
            .bind(first.id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        corrected,
        Some(first.p2_reg),
        "precondition: the correction flips the recorded winner"
    );
    let after_correction = slots(&app, final_id).await;
    assert!(
        after_correction.0 == Some(first.p1_reg) || after_correction.1 == Some(first.p1_reg),
        "precondition: the correction leaves the WRONG player in the final — \
         that is what makes revert the only control that can fix it"
    );

    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", first.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);

    let after = slots(&app, final_id).await;
    assert!(
        after.0 != Some(first.p1_reg) && after.1 != Some(first.p1_reg),
        "P-169: revert must take the participant this match advanced back out \
         of the final, even though the recorded winner has since changed under \
         it — the identity-matching version returned 200 and left them there"
    );
    assert!(
        after.0 != Some(first.p2_reg) && after.1 != Some(first.p2_reg),
        "revert must not seat the corrected winner either — that is reapply's job"
    );
    assert!(
        after.0 == Some(second.p1_reg) || after.1 == Some(second.p1_reg),
        "the other semi-final's winner must be left exactly where they were"
    );
    assert_eq!(
        match_status(&app, final_id).await,
        "pending",
        "a final that is no longer a full pairing must go back to pending, or \
         the bracket keeps offering a half-empty match as playable"
    );

    // Idempotent: a second revert has nothing left to take out and must not
    // touch the participant who is legitimately seated.
    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", first.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);
    assert_eq!(
        slots(&app, final_id).await,
        after,
        "reverting twice must land where reverting once did"
    );
}

/// P-169: reverting into a round that has already been played must REFUSE, with
/// a reason that names the blocking match — not succeed, and not cascade.
///
/// Cascading would delete a result somebody really recorded (and whatever that
/// match progressed in turn) as a side effect of repairing an earlier round,
/// with no audit row and no undo. Refusing costs the operator one extra step —
/// revert the deeper match first — and that step is stated in the error.
///
/// The 409 is not a new contract: `revert_progression`'s OpenAPI has always
/// documented "Cannot revert - subsequent matches affected". It was never
/// implemented.
#[tokio::test]
async fn test_revert_refuses_when_the_next_round_has_already_been_played() {
    let app = TestApp::new().await;

    let tournament_id =
        start_bracket_tournament(&app, "p169-played", "single_elimination", 4).await;
    let semis = feeding_matches(&app, &tournament_id).await;
    let (first, second) = (semis[0], semis[1]);
    let final_id = first.winner_target;

    play_and_advance(&app, &first).await;
    play_and_advance(&app, &second).await;

    // The final gets played too.
    record_result(&app, final_id, first.p1_reg, second.p1_reg, 1, 0).await;

    let response = app
        .post_json(
            &format!("/v1/admin/matches/{}/progression/revert", first.id),
            &json!({}),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);

    let detail = response.json::<serde_json::Value>()["detail"]
        .as_str()
        .expect("a problem-details body")
        .to_string();
    assert!(
        detail.contains("R2M1"),
        "the refusal must NAME the match that blocks it, got: {detail}"
    );
    assert!(
        detail.contains("has a recorded result"),
        "the refusal must say WHY the round cannot be rolled into, got: {detail}"
    );
    assert!(
        detail.contains("Revert R2M1 first"),
        "the refusal must tell the operator what to do instead, got: {detail}"
    );

    // Nothing was destroyed on the way to refusing.
    let after = slots(&app, final_id).await;
    assert!(
        after.0 == Some(first.p1_reg) || after.1 == Some(first.p1_reg),
        "a refused revert must leave the played round exactly as it was"
    );
    let winner: Option<Uuid> =
        sqlx::query_scalar("SELECT winner_registration_id FROM tournament_matches WHERE id = $1")
            .bind(final_id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        winner,
        Some(first.p1_reg),
        "the played result must survive the refusal"
    );

    // Reapply is revert + re-process, so it inherits the refusal rather than
    // rewriting the final's roster under a recorded result.
    let response = app
        .post_json(
            &format!("/v1/admin/matches/{}/progression/reapply", first.id),
            &json!({ "new_winner_registration_id": first.p2_reg }),
        )
        .await;
    response.assert_status(StatusCode::CONFLICT);
    assert!(
        response.json::<serde_json::Value>()["detail"]
            .as_str()
            .unwrap()
            .contains("R2M1"),
        "reapply must refuse for the same, named reason"
    );

    // Revert the final first, and the earlier revert then goes through — the
    // refusal is a sequencing rule, not a dead end.
    sqlx::query(
        "UPDATE tournament_matches SET winner_registration_id = NULL,
             loser_registration_id = NULL, status = 'ready', completed_at = NULL
         WHERE id = $1",
    )
    .bind(final_id)
    .execute(app.pool())
    .await
    .unwrap();

    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", first.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);
    let after = slots(&app, final_id).await;
    assert!(
        after.0 != Some(first.p1_reg) && after.1 != Some(first.p1_reg),
        "once the blocking result is gone, the revert must actually run"
    );
}

/// P-169: double elimination is handled, not refused — and BOTH routings are
/// rolled back.
///
/// A winners-bracket match feeds two matches: the winner advances inside the
/// winners bracket and the loser drops into the losers bracket. A revert that
/// only followed `winner_progresses_to` would leave a player in the losers
/// bracket for a match they are no longer recorded as having lost.
#[tokio::test]
async fn test_revert_rolls_back_both_winner_and_loser_routing_in_double_elimination() {
    let app = TestApp::new().await;

    let tournament_id = start_bracket_tournament(&app, "p169-de", "double_elimination", 4).await;
    let source = feeding_matches(&app, &tournament_id)
        .await
        .into_iter()
        .find(|m| m.loser_target.is_some())
        .expect("a winners-bracket match that also drops its loser");
    let loser_target = source.loser_target.unwrap();

    assert_eq!(
        bracket_type_of(&app, source.id).await,
        "winners",
        "this test must exercise the winners bracket, or it proves nothing \
         about double elimination"
    );
    assert_eq!(bracket_type_of(&app, loser_target).await, "losers");

    play_and_advance(&app, &source).await;

    let wb_next = slots(&app, source.winner_target).await;
    assert!(
        wb_next.0 == Some(source.p1_reg) || wb_next.1 == Some(source.p1_reg),
        "precondition: the winner advanced inside the winners bracket"
    );
    let lb_next = slots(&app, loser_target).await;
    assert!(
        lb_next.0 == Some(source.p2_reg) || lb_next.1 == Some(source.p2_reg),
        "precondition: the loser dropped into the losers bracket"
    );

    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", source.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);

    let wb_next = slots(&app, source.winner_target).await;
    assert!(
        wb_next.0 != Some(source.p1_reg) && wb_next.1 != Some(source.p1_reg),
        "the winner must come back out of the winners-bracket match"
    );
    let lb_next = slots(&app, loser_target).await;
    assert!(
        lb_next.0 != Some(source.p2_reg) && lb_next.1 != Some(source.p2_reg),
        "the loser must come back out of the losers-bracket match — a revert \
         that only followed winner_progresses_to would strand them there"
    );
}

/// P-169: `reapply_progression` must leave the match row agreeing with the
/// bracket it just rewrote.
///
/// The attribution write lived inside the round-robin/swiss standings branch,
/// so on an elimination bracket reapply seated the new winner downstream and
/// left `winner_registration_id` naming the old one. The match then said one
/// thing and the bracket another — and the next revert, which reads that
/// column, would go hunting for a participant who was never advanced. The
/// mirror image of the defect this finding is about, in the function that calls
/// revert.
#[tokio::test]
async fn test_reapply_records_the_new_winner_on_an_elimination_match() {
    let app = TestApp::new().await;

    let tournament_id = start_bracket_tournament(&app, "p169-reap", "single_elimination", 4).await;
    let semi = feeding_matches(&app, &tournament_id)
        .await
        .into_iter()
        .next()
        .expect("a seeded semi-final");

    play_and_advance(&app, &semi).await;

    app.post_json(
        &format!("/v1/admin/matches/{}/progression/reapply", semi.id),
        &json!({ "new_winner_registration_id": semi.p2_reg }),
    )
    .await
    .assert_status(StatusCode::OK);

    let (winner, loser): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT winner_registration_id, loser_registration_id
         FROM tournament_matches WHERE id = $1",
    )
    .bind(semi.id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        winner,
        Some(semi.p2_reg),
        "reapply must record the winner it advanced, or the match disagrees \
         with the bracket it produced"
    );
    assert_eq!(loser, Some(semi.p1_reg));

    let final_slots = slots(&app, semi.winner_target).await;
    assert!(
        final_slots.0 == Some(semi.p2_reg) || final_slots.1 == Some(semi.p2_reg),
        "the new winner must be the one seated in the final"
    );
    assert!(
        final_slots.0 != Some(semi.p1_reg) && final_slots.1 != Some(semi.p1_reg),
        "the old winner must not still be there"
    );

    // …and because the row and the bracket now agree, a follow-up revert can
    // still find what to undo.
    app.post_json(
        &format!("/v1/admin/matches/{}/progression/revert", semi.id),
        &json!({}),
    )
    .await
    .assert_status(StatusCode::OK);
    let final_slots = slots(&app, semi.winner_target).await;
    assert!(
        final_slots.0 != Some(semi.p2_reg) && final_slots.1 != Some(semi.p2_reg),
        "revert after reapply must still empty the slot"
    );
}
