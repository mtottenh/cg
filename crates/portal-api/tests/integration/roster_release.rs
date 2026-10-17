//! A team leaving a season releases its roster; dead rosters seat nobody.
//!
//! The production report: a player who had founded a team, then disbanded and
//! archived it, could not join or be invited to any other team in the same
//! season — "player is already a primary member of team <id> in this season",
//! naming a team nobody could see. Disband and archive changed the team and
//! left its roster seats `active`, and `idx_one_primary_team_per_season`
//! counts exactly those.
//!
//! Also pinned here, from the same audit: a dead team takes no season slot,
//! accepts no invites, and enters no cups; cup entry needs a captain of the
//! team in the cup's own season; check-in needs an approved entry; and an
//! archived league takes no new members.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

/// A league (the dev user is its admin and a member), returning its id.
async fn create_league(app: &TestApp, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({ "game_id": game_id, "name": slug, "slug": slug, "access_type": "open" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

async fn first_season(app: &TestApp, league_id: &str) -> String {
    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    body["data"][0]["id"].as_str().unwrap().to_string()
}

/// Create a team as the dev user, returning (team id, team-season id).
async fn create_team(app: &TestApp, season_id: &str, name: &str, tag: &str) -> (String, String) {
    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": name, "tag": tag }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    (
        created["data"]["team"]["id"].as_str().unwrap().to_string(),
        created["data"]["team_season"]["id"]
            .as_str()
            .unwrap()
            .to_string(),
    )
}

/// A second player, a member of every league, with a token.
async fn other_player(app: &TestApp, name: &str) -> (Uuid, String) {
    let user = UserBuilder::new()
        .username(name)
        .email(format!("{name}@example.com"))
        .build_persisted(app.pool())
        .await;
    sqlx::query(
        "INSERT INTO league_members (league_id, user_id, membership_type)
         SELECT id, $1, 'member' FROM leagues ON CONFLICT DO NOTHING",
    )
    .bind(user.id)
    .execute(app.pool())
    .await
    .unwrap();
    let token = create_test_token(user.id, user.id, name, TEST_JWT_SECRET);
    (user.id, token)
}

/// The active seats on a roster.
async fn active_seats(app: &TestApp, team_season_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM league_team_members
         WHERE team_season_id = $1::uuid AND status = 'active' AND left_at IS NULL",
    )
    .bind(team_season_id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

/// A team cup in `registration` inside the given league season.
async fn create_team_cup(
    app: &TestApp,
    league_id: &str,
    season_id: &str,
    slug: &str,
    team_size: i32,
) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "league_id": league_id,
                "season_id": season_id,
                "name": slug,
                "slug": slug,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "team_size": team_size,
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo1"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let id = created["data"]["id"].as_str().unwrap().to_string();
    app.post_auth(&format!("/v1/tournaments/{id}/publish"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/tournaments/{id}/open-registration"))
        .await
        .assert_status(StatusCode::OK);
    id
}

async fn registration_status(app: &TestApp, cup: &str, team_season_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT status FROM tournament_registrations
         WHERE tournament_id = $1::uuid AND team_season_id = $2::uuid",
    )
    .bind(cup)
    .bind(team_season_id)
    .fetch_one(app.pool())
    .await
    .unwrap()
}

// =============================================================================
// Disband / archive release the roster
// =============================================================================

/// The reported bug, via disband: the founder is free to start again.
#[tokio::test]
async fn test_disbanding_a_team_frees_its_players_for_the_season() {
    let app = TestApp::new().await;
    let league = create_league(&app, "disband-frees").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Gone Soon", "GONE").await;

    app.delete_auth(&format!("/v1/league-teams/{team}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    assert_eq!(active_seats(&app, &team_season).await, 0);
    let status: String =
        sqlx::query_scalar("SELECT status FROM league_team_seasons WHERE id = $1::uuid")
            .bind(&team_season)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(status, "withdrawn", "a disbanded team leaves its seasons");

    // Before the fix: 409 "already a primary member of team <gone>".
    create_team(&app, &season, "Second Wind", "WIND").await;
}

/// Disbanding withdraws an entry in a cup that has not drawn a bracket.
#[tokio::test]
async fn test_disbanding_withdraws_open_cup_entries() {
    let app = TestApp::new().await;
    let league = create_league(&app, "disband-cup").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Cup Quitters", "QUIT").await;
    let cup = create_team_cup(&app, &league, &season, "disband-open-cup", 1).await;
    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season, "participant_name": "Cup Quitters" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    app.delete_auth(&format!("/v1/league-teams/{team}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    assert_eq!(
        registration_status(&app, &cup, &team_season).await,
        "withdrawn"
    );
}

/// Once a cup has a bracket its matches reference the entry: disbanding is
/// refused rather than leaving the bracket a team with no players.
#[tokio::test]
async fn test_disbanding_a_team_in_a_bracketed_cup_is_refused() {
    let app = TestApp::new().await;
    let league = create_league(&app, "disband-bracket").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Committed", "COMM").await;
    let cup = create_team_cup(&app, &league, &season, "disband-bracket-cup", 1).await;
    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season, "participant_name": "Committed" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    app.post_auth(&format!("/v1/tournaments/{cup}/close-registration"))
        .await
        .assert_status(StatusCode::OK);

    app.delete_auth(&format!("/v1/league-teams/{team}"))
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    assert_eq!(active_seats(&app, &team_season).await, 1, "nothing moved");
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/league-teams/{team}"))
        .await
        .json();
    assert_eq!(body["data"]["status"], "active");
}

/// The reported bug, via archive: the founder can found again, and restoring
/// the archived team then does not seat them twice.
#[tokio::test]
async fn test_archiving_frees_the_roster_and_restore_respects_the_one_team_rule() {
    let app = TestApp::new().await;
    let league = create_league(&app, "archive-frees").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Shelved", "SHLF").await;

    app.post_auth(&format!("/v1/league-teams/{team}/archive"))
        .await
        .assert_status(StatusCode::OK);
    assert_eq!(active_seats(&app, &team_season).await, 0);

    let (_, new_team_season) = create_team(&app, &season, "Fresh Start", "FRSH").await;

    app.post_auth(&format!("/v1/league-teams/{team}/restore"))
        .await
        .assert_status(StatusCode::OK);
    assert_eq!(
        active_seats(&app, &team_season).await,
        0,
        "the founder is on another team now; the restore must not reseat them"
    );
    assert_eq!(active_seats(&app, &new_team_season).await, 1);
}

/// Archive then restore with nothing in between is exact: the seats return.
#[tokio::test]
async fn test_restoring_an_archived_team_brings_its_roster_back() {
    let app = TestApp::new().await;
    let league = create_league(&app, "archive-roundtrip").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Round Trip", "TRIP").await;

    app.post_auth(&format!("/v1/league-teams/{team}/archive"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/league-teams/{team}/restore"))
        .await
        .assert_status(StatusCode::OK);

    assert_eq!(active_seats(&app, &team_season).await, 1);
}

/// An archived team holds no season slot — and so restoring it into a season
/// that has filled meanwhile is refused.
#[tokio::test]
async fn test_archived_team_frees_its_slot_and_restore_rechecks_capacity() {
    let app = TestApp::new().await;
    let league = create_league(&app, "archive-slots").await;
    let season = first_season(&app, &league).await;
    sqlx::query("UPDATE league_seasons SET max_teams = 1 WHERE id = $1::uuid")
        .bind(&season)
        .execute(app.pool())
        .await
        .unwrap();
    let (team, _) = create_team(&app, &season, "Slot Holder", "SLOT").await;

    app.post_auth(&format!("/v1/league-teams/{team}/archive"))
        .await
        .assert_status(StatusCode::OK);

    // Before the fix: 409 "maximum number of teams reached".
    create_team(&app, &season, "Slot Taker", "TAKE").await;

    app.post_auth(&format!("/v1/league-teams/{team}/restore"))
        .await
        .assert_status(StatusCode::CONFLICT);
}

// =============================================================================
// Dead rosters seat nobody
// =============================================================================

/// An invitation issued while the team was live cannot seat a player once
/// the team is disbanded.
#[tokio::test]
async fn test_invitation_to_a_disbanded_team_cannot_be_accepted() {
    let app = TestApp::new().await;
    let league = create_league(&app, "dead-invite").await;
    let season = first_season(&app, &league).await;
    let (team, team_season) = create_team(&app, &season, "Fading", "FADE").await;
    let (player, token) = other_player(&app, "late_invitee").await;

    let response = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season}/invitations"),
            &json!({ "player_id": player, "role": "player" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let invite: serde_json::Value = response.json();
    let invite_id = invite["data"]["id"].as_str().unwrap().to_string();

    app.delete_auth(&format!("/v1/league-teams/{team}"))
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // Nor does it linger in the player's list as something to act on.
    let mine: serde_json::Value = app
        .get_with_token("/v1/league-team-invitations/me", &token)
        .await
        .json();
    assert_eq!(mine["data"].as_array().map_or(0, Vec::len), 0);

    app.post_with_token(
        &format!("/v1/league-team-invitations/{invite_id}/accept"),
        &token,
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(active_seats(&app, &team_season).await, 0);
}

/// A player who left a roster can come back to it. The (roster, player)
/// unique constraint used to turn the second seat into a 500.
#[tokio::test]
async fn test_a_player_can_rejoin_a_roster_they_left() {
    let app = TestApp::new().await;
    let league = create_league(&app, "rejoin").await;
    let season = first_season(&app, &league).await;
    let (_, team_season) = create_team(&app, &season, "Revolving Door", "DOOR").await;
    let (player, token) = other_player(&app, "boomerang").await;

    for _ in 0..2 {
        let response = app
            .post_json(
                &format!("/v1/league-team-seasons/{team_season}/invitations"),
                &json!({ "player_id": player, "role": "player" }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
        let invite: serde_json::Value = response.json();
        let invite_id = invite["data"]["id"].as_str().unwrap();
        app.post_with_token(
            &format!("/v1/league-team-invitations/{invite_id}/accept"),
            &token,
        )
        .await
        .assert_status(StatusCode::OK);
        assert_eq!(active_seats(&app, &team_season).await, 2);

        app.post_with_token(
            &format!("/v1/league-team-seasons/{team_season}/leave"),
            &token,
        )
        .await
        .assert_status(StatusCode::NO_CONTENT);
        assert_eq!(active_seats(&app, &team_season).await, 1);
    }
}

// =============================================================================
// Cup entry
// =============================================================================

/// Only a captain of the team (or the cup's staff) can enter it.
#[tokio::test]
async fn test_registering_someone_elses_team_is_forbidden() {
    let app = TestApp::new().await;
    let league = create_league(&app, "not-my-team").await;
    let season = first_season(&app, &league).await;
    let (_, team_season) = create_team(&app, &season, "Not Yours", "NOPE").await;
    let cup = create_team_cup(&app, &league, &season, "not-my-team-cup", 1).await;
    let (_, token) = other_player(&app, "stranger").await;

    app.post_json_with_token(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season, "participant_name": "Not Yours" }),
        &token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
}

/// A league cup is for that league season's teams.
#[tokio::test]
async fn test_registering_a_team_from_another_season_is_refused() {
    let app = TestApp::new().await;
    let home = create_league(&app, "home-league").await;
    let away = create_league(&app, "away-league").await;
    let home_season = first_season(&app, &home).await;
    let away_season = first_season(&app, &away).await;
    let (_, away_team_season) = create_team(&app, &away_season, "Tourists", "TOUR").await;
    let cup = create_team_cup(&app, &home, &home_season, "home-cup", 1).await;

    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": away_team_season, "participant_name": "Tourists" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

/// The second report: the roster a cup needs is its lineup size (or the
/// season's minimum, if larger) — never the season's roster cap.
#[tokio::test]
async fn test_cup_roster_requirement_is_the_lineup_not_the_cap() {
    let app = TestApp::new().await;
    let league = create_league(&app, "lineup-not-cap").await;
    let season = first_season(&app, &league).await;
    sqlx::query(
        "UPDATE league_seasons SET team_size_min = 2, team_size_max = 15 WHERE id = $1::uuid",
    )
    .bind(&season)
    .execute(app.pool())
    .await
    .unwrap();
    let (_, team_season) = create_team(&app, &season, "Duo", "DUO").await;
    let cup = create_team_cup(&app, &league, &season, "solo-cup", 1).await;

    // One player: the cup's lineup is 1, but the season's floor is 2.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{cup}/registrations/team"),
            &json!({ "team_season_id": team_season, "participant_name": "Duo" }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert!(
        body["detail"].as_str().unwrap_or("").contains("1 of the 2"),
        "the larger of lineup and season floor binds: {body}"
    );

    let (player, token) = other_player(&app, "second_of_two").await;
    let response = app
        .post_json(
            &format!("/v1/league-team-seasons/{team_season}/invitations"),
            &json!({ "player_id": player, "role": "player" }),
        )
        .await;
    let invite: serde_json::Value = response.json();
    let invite_id = invite["data"]["id"].as_str().unwrap();
    app.post_with_token(
        &format!("/v1/league-team-invitations/{invite_id}/accept"),
        &token,
    )
    .await
    .assert_status(StatusCode::OK);

    // Two of fifteen is enough.
    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season, "participant_name": "Duo" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
}

/// Check-in is for approved entries; a pending one has not been accepted.
#[tokio::test]
async fn test_a_pending_entry_cannot_check_in() {
    let app = TestApp::new().await;
    let league = create_league(&app, "pending-checkin").await;
    let season = first_season(&app, &league).await;
    let (_, team_season) = create_team(&app, &season, "Eager", "EGR").await;
    let cup = create_team_cup(&app, &league, &season, "pending-checkin-cup", 1).await;
    let response = app
        .post_json(
            &format!("/v1/tournaments/{cup}/registrations/team"),
            &json!({ "team_season_id": team_season, "participant_name": "Eager" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let reg: serde_json::Value = response.json();
    let reg_id = reg["data"]["id"].as_str().unwrap().to_string();

    // Open registration auto-approves; put it back to pending, and open the
    // check-in window so only the status can refuse.
    sqlx::query("UPDATE tournament_registrations SET status = 'pending' WHERE id = $1::uuid")
        .bind(&reg_id)
        .execute(app.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE tournaments SET check_in_required = true,
                check_in_start = NOW() - INTERVAL '1 hour',
                check_in_end = NOW() + INTERVAL '1 hour'
         WHERE id = $1::uuid",
    )
    .bind(&cup)
    .execute(app.pool())
    .await
    .unwrap();

    app.post_auth(&format!(
        "/v1/tournaments/{cup}/registrations/{reg_id}/check-in"
    ))
    .await
    .assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(
        registration_status(&app, &cup, &team_season).await,
        "pending"
    );
}

// =============================================================================
// Archived parents
// =============================================================================

/// An archived league is hidden, and takes no new members by direct link.
#[tokio::test]
async fn test_an_archived_league_cannot_be_joined() {
    let app = TestApp::new().await;
    let league = create_league(&app, "archived-join").await;
    app.post_auth(&format!("/v1/leagues/{league}/archive"))
        .await
        .assert_status(StatusCode::OK);
    let user = UserBuilder::new()
        .username("latecomer")
        .email("latecomer@example.com")
        .build_persisted(app.pool())
        .await;
    let token = create_test_token(user.id, user.id, "latecomer", TEST_JWT_SECRET);

    app.post_with_token(&format!("/v1/leagues/{league}/join"), &token)
        .await
        .assert_status(StatusCode::BAD_REQUEST);
}

/// A season whose league is archived takes no new teams, even though its own
/// status still says `registration`.
#[tokio::test]
async fn test_no_team_can_be_founded_in_an_archived_leagues_season() {
    let app = TestApp::new().await;
    let league = create_league(&app, "archived-found").await;
    let season = first_season(&app, &league).await;
    app.post_auth(&format!("/v1/leagues/{league}/archive"))
        .await
        .assert_status(StatusCode::OK);

    app.post_json(
        &format!("/v1/league-seasons/{season}/teams"),
        &json!({ "name": "Ghost", "tag": "GHST" }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}
