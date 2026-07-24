//! Integration tests for the provisional lineup declaration write path (Phase B).
//!
//! Covers: declaring a lineup for a registration, roster-snapshot resolution
//! (`was_rostered` / `is_substitute` for a declared non-rostered player),
//! opponent visibility gating (a lineup is opponent-visible only once locked),
//! and locking on match start.

use super::*;

/// The dev token identity (see `extractors/auth.rs`). The dev user registers
/// `reg1` in `create_tournament_with_matches`, so this is `reg1`'s player.
const DEV_PLAYER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Schedule the match so declaration/check-in is possible.
async fn schedule(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/schedule"),
            &json!({ "scheduled_at": scheduled_time.to_rfc3339() }),
        )
        .await;
    response.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_declare_lineup_rostered_player() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-declare").await;
    schedule(&app, &tournament_id, &match_id).await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({
                "registration_id": reg1,
                "player_ids": [DEV_PLAYER_ID],
                "submit": true
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "submitted");
    assert_eq!(body["data"]["players_visible"], true);
    let players = body["data"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 1);
    assert_eq!(players[0]["player_id"], DEV_PLAYER_ID);
    assert_eq!(players[0]["source"], "declared");
    // The registered player is on the (individual) roster.
    assert_eq!(players[0]["was_rostered"], true);
    assert_eq!(players[0]["is_substitute"], false);
}

#[tokio::test]
async fn test_declared_non_rostered_player_is_substitute() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-sub").await;
    schedule(&app, &tournament_id, &match_id).await;

    // A real player who is NOT the registered participant -> a substitute.
    let (_sub_user, sub_player) = create_test_player(&app, "lineup_sub_player").await;

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({
                "registration_id": reg1,
                "player_ids": [DEV_PLAYER_ID, sub_player.to_string()],
                "submit": false
            }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["status"], "draft");
    let players = body["data"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);

    let sub = players
        .iter()
        .find(|p| p["player_id"] == sub_player.to_string())
        .expect("substitute row present");
    assert_eq!(sub["was_rostered"], false);
    assert_eq!(
        sub["is_substitute"], true,
        "a declared non-rostered player is provisionally a substitute"
    );
}

#[tokio::test]
async fn test_redeclaring_replaces_players() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-replace").await;
    schedule(&app, &tournament_id, &match_id).await;

    let (_u, extra) = create_test_player(&app, "lineup_replace_extra").await;
    let url = format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup");

    // First declaration: two players.
    let r = app
        .post_json(
            &url,
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID, extra.to_string()] }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["players"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Re-declare with one player: the old rows are replaced, not appended.
    let r = app
        .post_json(
            &url,
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID] }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["players"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_opponent_cannot_see_unlocked_lineup() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-visibility").await;
    schedule(&app, &tournament_id, &match_id).await;

    // reg1 (dev) declares a lineup while the match has not started.
    let r = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID], "submit": true }),
        )
        .await;
    r.assert_status(StatusCode::OK);

    // The opponent (player2) lists lineups: reg1's is unlocked, so its players
    // must be withheld.
    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineups"),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let lineups = body["data"].as_array().unwrap();
    let reg1_lineup = lineups
        .iter()
        .find(|l| l["registration_id"] == reg1)
        .expect("reg1 lineup listed");
    assert_eq!(
        reg1_lineup["players_visible"], false,
        "opponent must not see an unlocked lineup's players"
    );
    assert!(reg1_lineup["players"].as_array().unwrap().is_empty());

    // Sanity: reg2 is not implicated by reg1's declaration.
    let _ = reg2;
}

#[tokio::test]
async fn test_lineup_locks_on_match_start() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-lock").await;
    schedule(&app, &tournament_id, &match_id).await;

    // Declare for reg1.
    let r = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID], "submit": true }),
        )
        .await;
    r.assert_status(StatusCode::OK);
    assert_eq!(
        r.json::<serde_json::Value>()["data"]["locked_at"],
        serde_json::Value::Null
    );

    // Both participants check in -> the match auto-advances (PickBan/InProgress),
    // which locks the lineups.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg2 }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // reg1's lineup should now be locked (and thus opponent-visible).
    let body: serde_json::Value = app
        .get_auth(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/lineups"
        ))
        .await
        .json();
    let reg1_lineup = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["registration_id"] == reg1)
        .expect("reg1 lineup listed");
    assert_eq!(reg1_lineup["status"], "locked");
    assert!(
        reg1_lineup["locked_at"].is_string(),
        "locked_at must be stamped on match start"
    );
}

#[tokio::test]
async fn test_cannot_declare_after_lock() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        create_tournament_with_matches_and_opponent(&app, "lineup-after-lock").await;
    schedule(&app, &tournament_id, &match_id).await;

    // Start the match via check-in of both sides.
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg1 }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json_with_token(
        &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/check-in"),
        &json!({ "registration_id": reg2 }),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::OK);

    // Declaration after the match has started is refused.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/lineup"),
            &json!({ "registration_id": reg1, "player_ids": [DEV_PLAYER_ID] }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::OK,
        "declaration must be refused once the match has started"
    );
}

// =============================================================================
// Phase C — authoritative demo-derived lineup materialization
// =============================================================================

/// Materialize a demo-derived lineup through the real `LineupService` and assert
/// the authoritative rows: `source = demo`, per-map `game_number`, and
/// `is_substitute`/`was_rostered` auto-resolved from the roster. A registered
/// player who is not on the (individual) registration's roster is auto-tagged a
/// substitute (§0b).
#[tokio::test]
async fn test_materialize_demo_lineup_tags_substitute() {
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-demo-mat").await;

    // A registered player who did NOT register for this match -> a substitute.
    let (_u, sub_player) = create_test_player(&app, "demo_sub_player").await;

    let pool = app.pool().clone();
    let service = portal_domain::services::tournament::LineupService::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool.clone())),
    );

    let match_uuid: portal_core::TournamentMatchId = match_id.parse().unwrap();
    let reg_uuid: portal_core::TournamentRegistrationId = reg1.parse().unwrap();
    let dev_player: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    let sub: portal_core::PlayerId = sub_player.to_string().parse().unwrap();

    service
        .materialize_demo_lineup(match_uuid, reg_uuid, Some(1), vec![dev_player, sub])
        .await
        .expect("materialize demo lineup");

    // Read it back via the API — a demo lineup is created locked (the match was
    // played), so its players are visible.
    let body: serde_json::Value = app
        .get_auth(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}/lineups"
        ))
        .await
        .json();
    let lineup = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["registration_id"] == reg1)
        .expect("reg1 demo lineup listed");

    assert_eq!(lineup["status"], "locked");
    assert_eq!(lineup["players_visible"], true);
    let players = lineup["players"].as_array().unwrap();
    assert_eq!(players.len(), 2);

    let dev_row = players
        .iter()
        .find(|p| p["player_id"] == DEV_PLAYER_ID)
        .expect("rostered demo row");
    assert_eq!(dev_row["source"], "demo");
    assert_eq!(dev_row["game_number"], 1);
    assert_eq!(dev_row["was_rostered"], true);
    assert_eq!(dev_row["is_substitute"], false);

    let sub_row = players
        .iter()
        .find(|p| p["player_id"] == sub_player.to_string())
        .expect("substitute demo row");
    assert_eq!(sub_row["source"], "demo");
    assert_eq!(
        sub_row["is_substitute"], true,
        "a registered player not on the roster is auto-tagged a substitute"
    );
    assert_eq!(sub_row["was_rostered"], false);
}

/// P-25 attribution gate (Phase C): a demo player's stats attribute to a match
/// only if that player is in the match's authoritative (demo-source) lineup. A
/// registered player present in the demo but NOT in the lineup is de-attributed;
/// a player in the lineup keeps their attribution.
#[tokio::test]
async fn test_attribution_gated_to_lineup() {
    use portal_domain::repositories::demo::DemoPlayerRepository;
    use portal_test::prelude::{DemoBuilder, DemoMatchLinkBuilder};
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-attr-gate").await;
    let (_ub, player_b) = create_test_player(&app, "attr_gate_b").await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let match_uuid: uuid::Uuid = match_id.parse().unwrap();
    let dev_player = uuid::Uuid::parse_str(DEV_PLAYER_ID).unwrap();

    // A demo linked to the match, with two players both globally attributed
    // (resolved) — player A (dev, rostered on reg1) and player B (registered,
    // not in the lineup).
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("attr_gate.dem")
        .build_persisted(app.pool())
        .await;
    DemoMatchLinkBuilder::new()
        .demo_id(demo.id)
        .match_id(match_uuid)
        .game_number(1)
        .manual()
        .build_persisted(app.pool())
        .await;
    for (steam, pid) in [("111", dev_player), ("222", player_b)] {
        sqlx::query(
            "INSERT INTO demo_players (demo_id, steam_id, player_name, player_id) VALUES ($1,$2,$3,$4)",
        )
        .bind(demo.id)
        .bind(steam)
        .bind(format!("p{steam}"))
        .bind(pid)
        .execute(app.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO demo_player_stats (demo_id, steam_id, stat_key, value, player_id) VALUES ($1,$2,'kills',10,$3)",
        )
        .bind(demo.id)
        .bind(steam)
        .bind(pid)
        .execute(app.pool())
        .await
        .unwrap();
    }

    // Materialize a demo lineup for reg1 containing ONLY player A.
    let pool = app.pool().clone();
    let service = portal_domain::services::tournament::LineupService::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool.clone())),
    );
    let match_tid: portal_core::TournamentMatchId = match_id.parse().unwrap();
    let reg_tid: portal_core::TournamentRegistrationId = reg1.parse().unwrap();
    let dev_pid: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    service
        .materialize_demo_lineup(match_tid, reg_tid, Some(1), vec![dev_pid])
        .await
        .expect("materialize demo lineup");

    // Gate attribution to the lineup.
    let demo_repo = portal_db::PgDemoPlayerRepository::new(pool.clone());
    demo_repo
        .restrict_attribution_to_lineup(portal_core::DemoId::from(demo.id), match_tid)
        .await
        .expect("restrict attribution");

    let attributed = |pid: uuid::Uuid| {
        let pool = app.pool().clone();
        let demo_id = demo.id;
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM demo_player_stats WHERE demo_id=$1 AND player_id=$2",
            )
            .bind(demo_id)
            .bind(pid)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        attributed(dev_player).await,
        1,
        "the player in the lineup keeps attribution"
    );
    assert_eq!(
        attributed(player_b).await,
        0,
        "a registered player NOT in the lineup is de-attributed"
    );
    // B's stat row still exists, just de-attributed (player_id NULL).
    let b_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM demo_player_stats WHERE demo_id=$1 AND steam_id='222' AND player_id IS NULL",
    )
    .bind(demo.id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(
        b_rows, 1,
        "the stat row remains, only its attribution is stripped"
    );
}

/// P-58: team-match participation is credited from the authoritative
/// (demo-derived) lineup — who actually played — not the whole roster. A player
/// in the lineup is credited; a registered player NOT in any map's lineup is
/// not; and the lineup overrides the registration's own player.
#[tokio::test]
async fn test_participation_credited_from_lineup() {
    use portal_domain::services::tournament::MatchStatsUpdater;
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2) =
        create_tournament_with_matches(&app, "lineup-p58").await;

    // Players who played (A, B) and one who did not (C).
    let (_ua, player_a) = create_test_player(&app, "p58_a").await;
    let (_ub, player_b) = create_test_player(&app, "p58_b").await;
    let (_uc, player_c) = create_test_player(&app, "p58_c").await;

    let pool = app.pool().clone();
    let lineup_service = portal_domain::services::tournament::LineupService::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool.clone())),
    );
    let match_tid: portal_core::TournamentMatchId = match_id.parse().unwrap();
    let reg1_tid: portal_core::TournamentRegistrationId = reg1.parse().unwrap();
    let a_pid: portal_core::PlayerId = player_a.to_string().parse().unwrap();
    let b_pid: portal_core::PlayerId = player_b.to_string().parse().unwrap();

    // Winner (reg1) lineup: A and B played on map 1. (dev is reg1's registrant
    // but did NOT play — the lineup must override the registration.)
    lineup_service
        .materialize_demo_lineup(match_tid, reg1_tid, Some(1), vec![a_pid, b_pid])
        .await
        .expect("materialize demo lineup");

    // Drive the real stats updater as the completion saga does.
    let adapter = portal_api::adapters::StatsUpdaterAdapter::new(
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgDemoMatchLinkRepository::new(pool.clone())),
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        portal_db::GameRepository::new(pool.clone()),
        portal_domain::services::PlayerGameProfileService::new(Arc::new(
            portal_db::PgPlayerGameProfileRepository::new(pool.clone()),
        )),
        Arc::new(portal_plugins::PluginManager::new()),
    );
    let reg2_tid: portal_core::TournamentRegistrationId = reg2.parse().unwrap();
    adapter
        .update_player_stats(match_tid, reg1_tid, reg2_tid, false)
        .await
        .expect("update player stats");

    let credited = |pid: uuid::Uuid| {
        let pool = app.pool().clone();
        let mid = match_tid.as_uuid();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM player_match_stats_applied WHERE player_id=$1 AND match_id=$2",
            )
            .bind(pid)
            .bind(mid)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let dev_player = uuid::Uuid::parse_str(DEV_PLAYER_ID).unwrap();
    assert_eq!(credited(player_a).await, 1, "lineup player A is credited");
    assert_eq!(credited(player_b).await, 1, "lineup player B is credited");
    assert_eq!(
        credited(player_c).await,
        0,
        "a registered player not in any lineup is not credited"
    );
    assert_eq!(
        credited(dev_player).await,
        0,
        "the lineup overrides the registration's own player"
    );
}

// =============================================================================
// Phase D — §0.4 enforcement on the authoritative demo lineup (raises review)
// =============================================================================

fn make_enforcer(app: &TestApp) -> portal_api::adapters::LineupEnforcer {
    use std::sync::Arc;
    let pool = app.pool().clone();
    let eligibility = portal_domain::services::EligibilityService::new(
        portal_domain::services::PlayerGameProfileService::new(Arc::new(
            portal_db::PgPlayerGameProfileRepository::new(pool.clone()),
        )),
        Arc::new(portal_db::PgPlayerRatingHistoryRepository::new(
            pool.clone(),
        )),
    );
    portal_api::adapters::LineupEnforcer::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRepository::new(pool)),
        eligibility,
    )
}

async fn materialize(
    app: &TestApp,
    match_id: &str,
    reg: &str,
    players: Vec<portal_core::PlayerId>,
) {
    use std::sync::Arc;
    let pool = app.pool().clone();
    let svc = portal_domain::services::tournament::LineupService::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool.clone())),
    );
    svc.materialize_demo_lineup(
        match_id.parse().unwrap(),
        reg.parse().unwrap(),
        Some(1),
        players,
    )
    .await
    .expect("materialize");
}

#[tokio::test]
async fn test_majority_rule_violation_raises_review() {
    let app = TestApp::new().await;
    let (_t, match_id, reg1, _reg2) = create_tournament_with_matches(&app, "lineup-majority").await;
    let (_a, sub_a) = create_test_player(&app, "maj_sub_a").await;
    let (_b, sub_b) = create_test_player(&app, "maj_sub_b").await;

    let dev: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    // 1 rostered (dev) + 2 subs of 3 -> subs are NOT a strict minority.
    materialize(
        &app,
        &match_id,
        &reg1,
        vec![
            dev,
            sub_a.to_string().parse().unwrap(),
            sub_b.to_string().parse().unwrap(),
        ],
    )
    .await;

    let violations = make_enforcer(&app)
        .lineup_violations(match_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        violations
            .iter()
            .any(|v| v.player_name.contains("not a minority")),
        "a sub-majority lineup must raise a review, got {violations:?}"
    );
}

#[tokio::test]
async fn test_elo_cap_violation_raises_review() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-elo").await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let dev_uuid = uuid::Uuid::parse_str(DEV_PLAYER_ID).unwrap();

    // Give the rostered player a rating well over the cap.
    sqlx::query(
        "INSERT INTO player_game_profiles
            (player_id, game_id, rating, rating_deviation, volatility, peak_rating,
             matches_played, wins, losses, draws, win_streak, best_win_streak,
             total_playtime_minutes)
         VALUES ($1,$2,3000,50,0.06,3000,0,0,0,0,0,0,0)
         ON CONFLICT (player_id, game_id) DO UPDATE SET rating = 3000, peak_rating = 3000",
    )
    .bind(dev_uuid)
    .bind(game_id)
    .execute(app.pool())
    .await
    .unwrap();

    // Tournament caps sub rating at 1000.
    sqlx::query("UPDATE tournaments SET settings = $2 WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&tournament_id).unwrap())
        .bind(serde_json::json!({ "eligibility": { "max_rating_per_player": 1000 } }))
        .execute(app.pool())
        .await
        .unwrap();

    // A single rostered player (0 subs -> majority rule fine) who is over the cap.
    let dev: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    materialize(&app, &match_id, &reg1, vec![dev]).await;

    let violations = make_enforcer(&app)
        .lineup_violations(match_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        violations
            .iter()
            .any(|v| v.player_name.contains("max_rating_per_player")),
        "an over-elo-cap player must raise a review, got {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|v| v.player_name.contains("not a minority")),
        "a full-rostered lineup must not trip the majority rule"
    );
}

#[tokio::test]
async fn test_legal_lineup_no_violation() {
    let app = TestApp::new().await;
    let (_t, match_id, reg1, _reg2) = create_tournament_with_matches(&app, "lineup-legal").await;
    // Only the rostered player, no elo cap set -> no violation.
    let dev: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    materialize(&app, &match_id, &reg1, vec![dev]).await;

    let violations = make_enforcer(&app)
        .lineup_violations(match_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        violations.is_empty(),
        "a legal lineup must not raise a review, got {violations:?}"
    );
}
