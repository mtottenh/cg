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
///
/// P-67 deleted the participant-facing direct-set duplicate; the admin route is
/// the only direct-set path now, and the dev identity holds the permission.
async fn schedule(app: &TestApp, tournament_id: &str, match_id: &str) {
    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    let response = app
        .post_json(
            &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
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

/// Attach a `lineup_required = true` season to a tournament (opt-in gate §9).
async fn attach_lineup_required_season(app: &TestApp, tournament_id: &str) {
    use portal_test::prelude::LeagueSeasonBuilder;
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let season = LeagueSeasonBuilder::new()
        .active()
        .slug(format!("lineup-req-{}", &unique[..8]))
        .name(format!("Lineup Req {}", &unique[..8]))
        .build_persisted(app.pool())
        .await;
    sqlx::query("UPDATE league_seasons SET lineup_required = true WHERE id = $1")
        .bind(season.id)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE tournaments SET season_id = $2 WHERE id = $1")
        .bind(uuid::Uuid::parse_str(tournament_id).unwrap())
        .bind(season.id)
        .execute(app.pool())
        .await
        .unwrap();
}

/// ⚠️ SPEC CHANGED (design correction 2026-07-24, §0b): this test previously
/// asserted the OPPOSITE — that a registered player not in the lineup was
/// de-attributed (`player_id` NULLed). That assertion encoded the bug the
/// correction fixes. **Attribution follows registration**: a registered player
/// who appears in a map's demo is attributed for that map, full stop — even
/// when their side cannot be inferred. Only players with no site account stay
/// unattributed (the base Steam-ID join already leaves them NULL). The lineup
/// and the §0.4 rules are review-raisers, never stat-strippers.
///
/// Drives the REAL ingestion path (`DemoService::link_to_match` with the
/// lineup materializer, on a `lineup_required` season).
#[tokio::test]
async fn test_registered_players_keep_attribution() {
    use portal_test::prelude::DemoBuilder;
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, _reg2) =
        create_tournament_with_matches(&app, "lineup-attr-keep").await;
    attach_lineup_required_season(&app, &tournament_id).await;

    let (_ub, player_b) = create_test_player(&app, "attr_keep_b").await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let dev_player = uuid::Uuid::parse_str(DEV_PLAYER_ID).unwrap();

    // Demo metadata names the two sides; player A (dev, reg1's registrant) is
    // on "TeamA" (sideable); player B is registered but has a team name that
    // matches neither side (un-sideable); player C has NO account (ringer).
    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("attr_keep.dem")
        .cs2_metadata("de_dust2", "TeamA", "TeamB", 13, 7)
        .build_persisted(app.pool())
        .await;
    for (steam, team, pid) in [
        ("111", "TeamA", Some(dev_player)),
        ("222", "Mixup", Some(player_b)),
        ("333", "TeamB", None),
    ] {
        sqlx::query(
            "INSERT INTO demo_players (demo_id, steam_id, player_name, team_name, player_id) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(demo.id)
        .bind(steam)
        .bind(format!("p{steam}"))
        .bind(team)
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

    // Link through the real DemoService (materializer wired, as in AppState).
    let pool = app.pool().clone();
    let lineup_service = portal_domain::services::tournament::LineupService::new(
        Arc::new(portal_db::PgMatchLineupRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool.clone())),
    );
    let demo_service = portal_domain::services::DemoService::new(
        Arc::new(portal_db::PgDemoRepository::new(pool.clone())),
        Arc::new(portal_db::PgDemoMatchLinkRepository::new(pool.clone())),
        Arc::new(portal_db::PgDemoPlayerRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentMatchRepository::new(pool.clone())),
    )
    .with_lineup_materializer(Arc::new(lineup_service));

    let match_tid: portal_core::TournamentMatchId = match_id.parse().unwrap();
    demo_service
        .link_to_match(
            portal_core::DemoId::from(demo.id),
            match_tid,
            Some(1),
            portal_core::DemoLinkType::Manual,
            None,
        )
        .await
        .expect("link demo to match");

    let attributed = |steam: &'static str| {
        let pool = app.pool().clone();
        let demo_id = demo.id;
        async move {
            sqlx::query_scalar::<_, Option<uuid::Uuid>>(
                "SELECT player_id FROM demo_player_stats WHERE demo_id=$1 AND steam_id=$2",
            )
            .bind(demo_id)
            .bind(steam)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    // A: sided, in the lineup, attributed.
    assert_eq!(
        attributed("111").await,
        Some(dev_player),
        "a registered lineup player keeps attribution"
    );
    // B: registered but un-sideable — STAYS ATTRIBUTED (the correction). The
    // old behaviour stripped this to NULL; that was the bug.
    assert_eq!(
        attributed("222").await,
        Some(player_b),
        "a registered player whose side cannot be inferred must keep attribution"
    );
    // C: no account — never attributed (the base Steam-ID join leaves it NULL).
    assert_eq!(
        attributed("333").await,
        None,
        "an unregistered demo player is not attributed"
    );

    // The lineup itself: A is on reg1's side; un-sideable B has no lineup row
    // (their side is admin-resolved via the review, not guessed).
    let lineup_players: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT mlp.player_id FROM match_lineup_players mlp
         JOIN match_lineups ml ON ml.id = mlp.lineup_id
         WHERE ml.match_id = $1 AND mlp.source = 'demo'",
    )
    .bind(match_tid.as_uuid())
    .fetch_all(app.pool())
    .await
    .unwrap();
    assert!(lineup_players.contains(&dev_player), "A is in the lineup");
    assert!(
        !lineup_players.contains(&player_b),
        "un-sideable B gets no lineup row (side is admin-resolved)"
    );
}

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
        Arc::new(portal_db::PgTournamentRepository::new(pool.clone())),
        Arc::new(portal_db::PgTournamentRegistrationRepository::new(
            pool.clone(),
        )),
        Arc::new(portal_db::PgLeagueTeamMemberRepository::new(pool)),
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

/// §0b correction: a registered demo player whose side could not be inferred
/// (present in the demo, absent from both sides' demo lineups) is flagged for
/// admin side-resolution via the review — while KEEPING attribution (asserted
/// in `test_registered_players_keep_attribution`).
#[tokio::test]
async fn test_sideless_registered_player_flagged_for_review() {
    use portal_domain::repositories::demo::DemoPlayerRepository;
    use portal_test::prelude::DemoBuilder;

    let app = TestApp::new().await;
    let (_t, match_id, reg1, _reg2) = create_tournament_with_matches(&app, "lineup-sideless").await;
    let (_ub, player_b) = create_test_player(&app, "sideless_b").await;
    let game_id = get_game_id(app.pool(), "cs2").await;
    let dev_player = uuid::Uuid::parse_str(DEV_PLAYER_ID).unwrap();

    let demo = DemoBuilder::new()
        .game_id(game_id)
        .file_name("sideless.dem")
        .cs2_metadata("de_dust2", "TeamA", "TeamB", 13, 7)
        .build_persisted(app.pool())
        .await;
    for (steam, team, pid) in [
        ("111", "TeamA", Some(dev_player)),
        ("222", "Mixup", Some(player_b)), // registered, un-sideable
    ] {
        sqlx::query(
            "INSERT INTO demo_players (demo_id, steam_id, player_name, team_name, player_id) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(demo.id)
        .bind(steam)
        .bind(format!("p{steam}"))
        .bind(team)
        .bind(pid)
        .execute(app.pool())
        .await
        .unwrap();
    }

    // The demo lineup for reg1 contains only the sided player A.
    let dev_pid: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();
    materialize(&app, &match_id, &reg1, vec![dev_pid]).await;

    let pool = app.pool().clone();
    let demo_players = portal_db::PgDemoPlayerRepository::new(pool)
        .find_by_demo(portal_core::DemoId::from(demo.id))
        .await
        .unwrap();

    let flags = make_enforcer(&app)
        .sideless_registered(match_id.parse().unwrap(), &demo_players)
        .await
        .unwrap();
    assert_eq!(flags.len(), 1, "exactly the un-sideable player is flagged");
    assert!(
        flags[0].player_name.contains("SIDE UNASSIGNED"),
        "the flag names the ambiguity: {flags:?}"
    );
    assert_eq!(flags[0].steam_id, "222");
}

/// P-26 (§0.4 rule 4): a player who is rostered on the OPPOSING team and
/// appears in this side's demo lineup RAISES a review — attribution untouched.
#[tokio::test]
async fn test_p26_opposing_roster_player_raises_review() {
    use portal_test::prelude::LeagueTeamSeasonBuilder;

    let app = TestApp::new().await;
    let (_t, match_id, reg1, reg2) = create_tournament_with_matches(&app, "lineup-p26").await;
    let (_ux, player_x) = create_test_player(&app, "p26_x").await;
    let dev: portal_core::PlayerId = DEV_PLAYER_ID.parse().unwrap();

    // Make reg2 a TEAM registration whose roster contains player X.
    // Build the season + team with unique slugs (the builders' auto-created
    // defaults collide on (league_id, slug)).
    let unique = uuid::Uuid::new_v4().simple().to_string();
    let season = portal_test::prelude::LeagueSeasonBuilder::new()
        .active()
        .slug(format!("p26-{}", &unique[..8]))
        .name(format!("P26 {}", &unique[..8]))
        .build_persisted(app.pool())
        .await;
    let team = portal_test::prelude::LeagueTeamBuilder::new()
        .league_id(season.league_id)
        .owner(player_x)
        .name(format!("P26 Team {}", &unique[..8]))
        .tag(format!("P{}", &unique[..3]))
        .active()
        .build_persisted(app.pool())
        .await;
    let ts = LeagueTeamSeasonBuilder::new()
        .team_id(team.id)
        .season_id(season.id)
        .active()
        .build_persisted(app.pool())
        .await;
    portal_test::prelude::LeagueTeamMemberBuilder::new()
        .team_season_id(ts.id)
        .player_id(player_x)
        .player()
        .active()
        .build_persisted(app.pool())
        .await;
    sqlx::query(
        "UPDATE tournament_registrations SET team_season_id = $2, player_id = NULL WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&reg2).unwrap())
    .bind(ts.id)
    .execute(app.pool())
    .await
    .unwrap();

    // X plays FOR reg1 — against their own team (reg2).
    materialize(
        &app,
        &match_id,
        &reg1,
        vec![dev, player_x.to_string().parse().unwrap()],
    )
    .await;

    let violations = make_enforcer(&app)
        .lineup_violations(match_id.parse().unwrap())
        .await
        .unwrap();
    // The violation names X. (Which registration_side it reports depends on
    // random seeding — reg1 may be participant 1 or 2 — so don't pin the side.)
    assert!(
        violations.iter().any(
            |v| v.player_name.contains("P-26") && v.player_name.contains(&player_x.to_string())
        ),
        "an opposing-roster player must raise a P-26 review, got {violations:?}"
    );

    // Enforcement never touches lineup rows or attribution: X is still in the
    // lineup after the check.
    let still_in_lineup: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM match_lineup_players mlp
         JOIN match_lineups ml ON ml.id = mlp.lineup_id
         WHERE ml.match_id = $1 AND mlp.player_id = $2",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .bind(player_x)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(still_in_lineup, 1, "enforcement must not remove the player");
}

/// P-58 (residual): a team that played WITHOUT a parsed demo credited nobody.
///
/// The original P-58 fix queried `source = 'demo'` alone. A team registration's
/// `player_id` is `None`, so when no demo lineup existed the caller's fallback
/// had nothing to fall back to and the whole team's participation was dropped —
/// announced by a `warn!` line and nothing else. Every match a league played
/// without demo coverage silently produced no stats for either side.
///
/// What settles it is the inconsistency: for an INDIVIDUAL registration the
/// updater credits the registrant with no proof they played at all. Teams were
/// held to a stricter evidentiary standard for the same statistic, and the
/// penalty for failing it was silence.
///
/// This drives the real `StatsUpdaterAdapter`, exactly as the completion saga
/// does, against a match whose only lineup is `declared`.
#[tokio::test]
async fn test_participation_credited_from_declared_lineup_when_no_demo() {
    use portal_domain::services::tournament::MatchStatsUpdater;
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2) =
        create_tournament_with_matches(&app, "lineup-p58-declared").await;

    let (user_d, player_d) = create_test_player(&app, "p58_declared_d").await;
    let (_ue, player_e) = create_test_player(&app, "p58_declared_e").await;

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
    let reg2_tid: portal_core::TournamentRegistrationId = reg2.parse().unwrap();
    let d_pid: portal_core::PlayerId = player_d.to_string().parse().unwrap();
    let e_pid: portal_core::PlayerId = player_e.to_string().parse().unwrap();
    let declarer: portal_core::UserId = user_d.to_string().parse().unwrap();

    // A DECLARED lineup only — no demo was ever parsed for this match.
    lineup_service
        .declare_lineup(
            match_tid,
            reg1_tid,
            vec![d_pid, e_pid],
            declarer,
            true,
            None,
        )
        .await
        .expect("declare lineup");

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

    assert_eq!(
        credited(player_d).await,
        1,
        "a declared player must be credited when no demo lineup exists — \
         crediting nobody is what P-58 was"
    );
    assert_eq!(
        credited(player_e).await,
        1,
        "both declared players are credited, not just the declarer"
    );
}

/// The authority order must HOLD: when a demo lineup exists, a stale declared
/// lineup naming different players must not dilute it.
///
/// This is the assertion that stops the residual fix from becoming a regression
/// of the original one. Falling back to `declared` is only safe because exactly
/// one source is ever used — a union would credit players the demo proves did
/// not play, which is strictly worse than the bug being fixed.
#[tokio::test]
async fn test_demo_lineup_outranks_a_stale_declared_lineup() {
    use portal_domain::services::tournament::MatchStatsUpdater;
    use std::sync::Arc;

    let app = TestApp::new().await;
    let (_tournament_id, match_id, reg1, reg2) =
        create_tournament_with_matches(&app, "lineup-p58-order").await;

    let (user_f, player_f) = create_test_player(&app, "p58_order_f").await;
    let (_ug, player_g) = create_test_player(&app, "p58_order_g").await;

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
    let reg2_tid: portal_core::TournamentRegistrationId = reg2.parse().unwrap();
    let f_pid: portal_core::PlayerId = player_f.to_string().parse().unwrap();
    let g_pid: portal_core::PlayerId = player_g.to_string().parse().unwrap();
    let declarer: portal_core::UserId = user_f.to_string().parse().unwrap();

    // F was PROMISED before the match...
    lineup_service
        .declare_lineup(match_tid, reg1_tid, vec![f_pid], declarer, true, None)
        .await
        .expect("declare lineup");
    // ...but G is who the demo says actually played.
    lineup_service
        .materialize_demo_lineup(match_tid, reg1_tid, Some(1), vec![g_pid])
        .await
        .expect("materialize demo lineup");

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

    assert_eq!(
        credited(player_g).await,
        1,
        "the demo lineup is authoritative — G actually played"
    );
    assert_eq!(
        credited(player_f).await,
        0,
        "F was only promised. Crediting them would mean the declared source \
         diluted the demo source, which is worse than the bug this fixes"
    );
}
