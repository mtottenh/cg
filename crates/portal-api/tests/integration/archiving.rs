//! Archiving: leagues, seasons, tournaments and league teams.
//!
//! The operator need is "make this stop being visible to players, without
//! destroying it or its history". Nothing supported it: a mis-created league
//! or a season that never ran stayed in every listing forever, and the only
//! alternative was a DELETE that would cascade through matches, rosters and
//! results.
//!
//! Two properties these tests exist to hold:
//!
//! 1. **Archiving hides, it does not change.** The row's own status is
//!    untouched, so a completed tournament comes back completed and a
//!    disbanded team comes back disbanded.
//! 2. **The cascade is inherited, not written.** A child of an archived
//!    league is hidden *because its parent is*, so restoring the league
//!    restores exactly what archiving it hid — and a child archived in its
//!    own right stays archived.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;

/// A league (dev user is its admin), returning its id.
async fn create_league(app: &TestApp, name: &str, slug: &str) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/leagues",
            &json!({ "game_id": game_id, "name": name, "slug": slug }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// The league ids in the current player's own membership list. Note the
/// endpoint returns a BARE array, not a `{data: [...]}` envelope.
async fn my_league_ids(app: &TestApp) -> Vec<String> {
    let body: serde_json::Value = app.get_auth("/v1/users/me/leagues").await.json();
    body.as_array()
        .expect("/v1/users/me/leagues returns a bare array")
        .iter()
        .map(|m| m["league_id"].as_str().unwrap().to_string())
        .collect()
}

/// The team ids in the current player's own team list
/// (`/v1/players/me/league-teams`).
async fn my_team_ids(app: &TestApp) -> Vec<String> {
    let body: serde_json::Value = app.get_auth("/v1/players/me/league-teams").await.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["team_id"].as_str().unwrap().to_string())
        .collect()
}

/// The names in the public league listing.
async fn public_league_names(app: &TestApp) -> Vec<String> {
    let body: serde_json::Value = app.get("/v1/leagues?per_page=100").await.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap().to_string())
        .collect()
}

// =============================================================================
// Leagues
// =============================================================================

#[tokio::test]
async fn test_archived_league_leaves_the_public_listing_and_comes_back() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Archivable League", "archivable-league").await;

    assert!(
        public_league_names(&app)
            .await
            .contains(&"Archivable League".to_string())
    );

    let response = app
        .post_auth(&format!("/v1/leagues/{league_id}/archive"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["archived_at"].is_string());
    assert_eq!(
        body["data"]["status"], "active",
        "archiving must not rewrite the league's own status"
    );

    assert!(
        !public_league_names(&app)
            .await
            .contains(&"Archivable League".to_string())
    );

    let response = app
        .post_auth(&format!("/v1/leagues/{league_id}/restore"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["archived_at"].is_null() || body["data"].get("archived_at").is_none());

    assert!(
        public_league_names(&app)
            .await
            .contains(&"Archivable League".to_string())
    );
}

/// The admin listing is where an archived league has to remain visible —
/// otherwise there is no way to reach the restore.
#[tokio::test]
async fn test_admin_listing_still_shows_an_archived_league() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Hidden From Players", "hidden-from-players").await;

    app.post_auth(&format!("/v1/leagues/{league_id}/archive"))
        .await
        .assert_status(StatusCode::OK);

    let body: serde_json::Value = app.get_auth("/v1/admin/leagues?per_page=100").await.json();
    let found = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == league_id.as_str())
        .expect("the archived league is still in the admin listing");
    assert!(found["archived_at"].is_string());
}

#[tokio::test]
async fn test_archiving_a_league_requires_permission() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Guarded League", "guarded-league").await;

    let outsider = UserBuilder::new()
        .username("archiveoutsider")
        .email("archiveoutsider@example.com")
        .build_persisted(app.pool())
        .await;
    let token = portal_domain::generate_access_token(
        outsider.id,
        outsider.id,
        "archiveoutsider",
        "test-jwt-secret",
    )
    .expect("token");

    app.post_with_token(&format!("/v1/leagues/{league_id}/archive"), &token)
        .await
        .assert_status(StatusCode::FORBIDDEN);
}

/// Whether the public tournament listing carries a tournament by name.
async fn tournament_listed(app: &TestApp, name: &str) -> bool {
    let body: serde_json::Value = app.get("/v1/tournaments?per_page=100").await.json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == name)
}

// =============================================================================
// Cascade
// =============================================================================

/// A tournament in an archived league is hidden with it — without the
/// tournament itself having been archived, so restoring the league brings it
/// back untouched.
#[tokio::test]
async fn test_archiving_a_league_hides_its_tournaments_without_archiving_them() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let league_id = create_league(&app, "Cascade League", "cascade-league").await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "league_id": league_id,
                "name": "Cascade Cup",
                "slug": "cascade-cup",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    assert!(
        tournament_listed(&app, "Cascade Cup").await,
        "precondition: the tournament is listed"
    );

    app.post_auth(&format!("/v1/leagues/{league_id}/archive"))
        .await
        .assert_status(StatusCode::OK);

    assert!(
        !tournament_listed(&app, "Cascade Cup").await,
        "a tournament in an archived league is hidden with it"
    );

    // The tournament itself was never touched.
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/tournaments/{tournament_id}"))
        .await
        .json();
    assert!(
        body["data"]["archived_at"].is_null() || body["data"].get("archived_at").is_none(),
        "the cascade must not write to the child"
    );

    app.post_auth(&format!("/v1/leagues/{league_id}/restore"))
        .await
        .assert_status(StatusCode::OK);

    assert!(
        tournament_listed(&app, "Cascade Cup").await,
        "restoring the league restores exactly what archiving it hid"
    );
}

/// The other half of inheritance: a child archived in its own right stays
/// archived when its parent comes back.
#[tokio::test]
async fn test_restoring_a_league_leaves_an_individually_archived_tournament_archived() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let league_id = create_league(&app, "Two Cups League", "two-cups-league").await;

    let mut ids = Vec::new();
    for (name, slug) in [("Kept Cup", "kept-cup"), ("Put Away Cup", "put-away-cup")] {
        let response = app
            .post_json(
                "/v1/tournaments",
                &json!({
                    "game_id": game_id,
                    "league_id": league_id,
                    "name": name,
                    "slug": slug,
                    "format": "single_elimination",
                    "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                    "participant_type": "individual",
                    "min_participants": 2,
                    "max_participants": 16,
                    "registration_type": "open",
                    "scheduling_mode": "live",
                    "default_match_format": "bo3"
                }),
            )
            .await;
        response.assert_status(StatusCode::CREATED);
        let created: serde_json::Value = response.json();
        ids.push(created["data"]["id"].as_str().unwrap().to_string());
    }

    app.post_auth(&format!("/v1/tournaments/{}/archive", ids[1]))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/leagues/{league_id}/archive"))
        .await
        .assert_status(StatusCode::OK);
    app.post_auth(&format!("/v1/leagues/{league_id}/restore"))
        .await
        .assert_status(StatusCode::OK);

    let body: serde_json::Value = app.get("/v1/tournaments?per_page=100").await.json();
    let names: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Kept Cup"));
    assert!(
        !names.contains(&"Put Away Cup"),
        "a tournament archived in its own right stays archived"
    );
}

// =============================================================================
// Tournaments
// =============================================================================

#[tokio::test]
async fn test_archiving_a_tournament_hides_it_but_keeps_its_status() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "name": "Solo Cup",
                "slug": "solo-cup",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();
    let status_before = created["data"]["status"].as_str().unwrap().to_string();

    let response = app
        .post_auth(&format!("/v1/tournaments/{tournament_id}/archive"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["archived_at"].is_string());
    assert_eq!(body["data"]["status"], status_before.as_str());

    let body: serde_json::Value = app.get("/v1/tournaments?per_page=100").await.json();
    assert!(
        !body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "Solo Cup")
    );

    app.post_auth(&format!("/v1/tournaments/{tournament_id}/restore"))
        .await
        .assert_status(StatusCode::OK);

    let body: serde_json::Value = app.get("/v1/tournaments?per_page=100").await.json();
    assert!(
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "Solo Cup")
    );
}

// =============================================================================
// Seasons
// =============================================================================

#[tokio::test]
async fn test_archived_season_is_hidden_unless_asked_for() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Season Archive League", "season-archive-league").await;

    let response = app
        .post_json(
            "/v1/league-seasons",
            &json!({
                "league_id": league_id,
                "name": "Season Two",
                "slug": "season-two",
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let season_id = created["data"]["id"].as_str().unwrap().to_string();

    app.post_auth(&format!("/v1/league-seasons/{season_id}/archive"))
        .await
        .assert_status(StatusCode::OK);

    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    assert!(
        !body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == season_id.as_str()),
        "the default listing is the player-facing one"
    );

    let body: serde_json::Value = app
        .get_auth(&format!(
            "/v1/league-seasons?league_id={league_id}&include_archived=true"
        ))
        .await
        .json();
    let found = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == season_id.as_str())
        .expect("an operator can ask for archived seasons");
    assert!(found["archived_at"].is_string());

    app.post_auth(&format!("/v1/league-seasons/{season_id}/restore"))
        .await
        .assert_status(StatusCode::OK);

    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    assert!(
        body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == season_id.as_str())
    );
}

/// The league's first season — the one its creation trigger makes.
async fn first_season(app: &TestApp, league_id: &str) -> String {
    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    body["data"][0]["id"].as_str().unwrap().to_string()
}

/// Whether a season's roster listing carries a team.
async fn roster_has(app: &TestApp, season_id: &str, team_id: &str) -> bool {
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/league-seasons/{season_id}/teams"))
        .await
        .json();
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["team_id"] == team_id)
}

// =============================================================================
// Teams
// =============================================================================

/// Archiving a team is not disbanding it: the team's own status is untouched,
/// which is what makes "restore" exact rather than a guess.
#[tokio::test]
async fn test_archived_team_leaves_the_season_roster_listing_and_comes_back() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Team Archive League", "team-archive-league").await;

    let season_id = first_season(&app, &league_id).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "Archivable Team", "tag": "ARCH" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    // Creating a team also registers it for the season, so the response is
    // {team, team_season} rather than the team alone.
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();
    let status_before = created["data"]["team"]["status"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(roster_has(&app, &season_id, &team_id).await);

    let response = app
        .post_auth(&format!("/v1/league-teams/{team_id}/archive"))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["archived_at"].is_string());
    assert_eq!(
        body["data"]["status"],
        status_before.as_str(),
        "archiving is not disbanding"
    );

    assert!(!roster_has(&app, &season_id, &team_id).await);

    app.post_auth(&format!("/v1/league-teams/{team_id}/restore"))
        .await
        .assert_status(StatusCode::OK);

    assert!(roster_has(&app, &season_id, &team_id).await);
}

/// A player's own league list feeds the sidebar's "your league" switcher and
/// the Find a team page, so an archived league must drop out of it (and come
/// back on restore) even though the membership row is untouched.
#[tokio::test]
async fn test_archived_league_leaves_the_players_own_league_list() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Mine To Archive", "mine-to-archive").await;

    assert!(
        my_league_ids(&app).await.contains(&league_id),
        "the creator is a member, so the league is in their own list"
    );

    app.post_auth(&format!("/v1/leagues/{league_id}/archive"))
        .await
        .assert_status(StatusCode::OK);
    assert!(
        !my_league_ids(&app).await.contains(&league_id),
        "an archived league is gone from the player's own list"
    );

    app.post_auth(&format!("/v1/leagues/{league_id}/restore"))
        .await
        .assert_status(StatusCode::OK);
    assert!(my_league_ids(&app).await.contains(&league_id));
}

/// A player's own team list must drop a team whose SEASON was archived — not
/// only its team or its league — so the My Teams page never offers a team in
/// a season that has been put away.
#[tokio::test]
async fn test_archived_season_hides_the_players_team_in_it() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Season Hides Team", "season-hides-team").await;
    let season_id = first_season(&app, &league_id).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "In A Season", "tag": "SEAS" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();

    assert!(
        my_team_ids(&app).await.contains(&team_id),
        "creating a team registers the creator, so it is in their team list"
    );

    app.post_auth(&format!("/v1/league-seasons/{season_id}/archive"))
        .await
        .assert_status(StatusCode::OK);
    assert!(
        !my_team_ids(&app).await.contains(&team_id),
        "a team in an archived season is gone from the player's team list"
    );

    app.post_auth(&format!("/v1/league-seasons/{season_id}/restore"))
        .await
        .assert_status(StatusCode::OK);
    assert!(my_team_ids(&app).await.contains(&team_id));
}

// =============================================================================
// The hidden rows stay hidden
// =============================================================================

/// `include_archived` is what makes the operator views possible, so it has to
/// be gated — otherwise "hidden from players" is one query parameter away
/// from being no rule at all.
#[tokio::test]
async fn test_include_archived_is_permission_gated() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Gated Filters League", "gated-filters-league").await;

    let season_id = first_season(&app, &league_id).await;

    // Anonymous
    app.get(&format!(
        "/v1/league-seasons?league_id={league_id}&include_archived=true"
    ))
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.get(&format!(
        "/v1/league-seasons/{season_id}/teams?include_archived=true"
    ))
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.get("/v1/tournaments?include_archived=true")
        .await
        .assert_status(StatusCode::FORBIDDEN);

    // Signed in, but nobody in particular
    let outsider = UserBuilder::new()
        .username("filteroutsider")
        .email("filteroutsider@example.com")
        .build_persisted(app.pool())
        .await;
    let token = portal_domain::generate_access_token(
        outsider.id,
        outsider.id,
        "filteroutsider",
        "test-jwt-secret",
    )
    .expect("token");

    app.get_with_token(
        &format!("/v1/league-seasons?league_id={league_id}&include_archived=true"),
        &token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
    app.get_with_token("/v1/tournaments?include_archived=true", &token)
        .await
        .assert_status(StatusCode::FORBIDDEN);

    // The league's admin can ask.
    app.get_auth(&format!(
        "/v1/league-seasons?league_id={league_id}&include_archived=true"
    ))
    .await
    .assert_status(StatusCode::OK);
}

// =============================================================================
// Moving between leagues
// =============================================================================

/// A team filed under the wrong league can be moved — identity, roster and
/// season registration together.
#[tokio::test]
async fn test_moving_a_team_carries_its_roster_into_the_target_season() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Wrong League", "wrong-league").await;
    let to_league = create_league(&app, "Right League", "right-league").await;

    let from_season = first_season(&app, &from_league).await;
    let to_season = first_season(&app, &to_league).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{from_season}/teams"),
            &json!({ "name": "Misfiled Team", "tag": "MIS" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_json(
            &format!("/v1/league-teams/{team_id}/move"),
            &json!({ "league_id": to_league, "season_id": to_season }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["league_id"], to_league.as_str());

    // It is in the new league's season...
    assert!(roster_has(&app, &to_season, &team_id).await);
    // ...and gone from the old one.
    assert!(!roster_has(&app, &from_season, &team_id).await);

    // The founding captain came with it, on the new season's roster.
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/league-seasons/{to_season}/teams"))
        .await
        .json();
    let moved = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["team_id"] == team_id.as_str())
        .expect("team present in the target season");
    assert_eq!(
        moved["active_member_count"], 1,
        "the roster must travel with the team, not be left behind"
    );
}

/// A cup that has not started holds nothing but the entry row, so the move
/// withdraws the team and says which cups it left.
#[tokio::test]
async fn test_moving_a_team_withdraws_it_from_cups_that_have_not_started() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Busy League", "busy-league").await;
    let to_league = create_league(&app, "Quiet League", "quiet-league").await;
    let from_season = first_season(&app, &from_league).await;
    let to_season = first_season(&app, &to_league).await;
    let (team_id, team_season_id) = create_team(&app, &from_season, "Early Birds", "EARLY").await;
    let cup = create_open_team_cup(
        &app,
        &from_league,
        &from_season,
        "Autumn Open",
        "autumn-open",
    )
    .await;
    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season_id, "participant_name": "Early Birds" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let response = app
        .post_json(
            &format!("/v1/league-teams/{team_id}/move"),
            &json!({ "league_id": to_league, "season_id": to_season }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["league_id"], to_league.as_str());
    let withdrawn = body["data"]["withdrawn_from"].as_array().unwrap();
    assert_eq!(
        withdrawn.len(),
        1,
        "the move must name the cup it withdrew the team from"
    );
    assert_eq!(withdrawn[0]["tournament_id"], cup.as_str());
    assert_eq!(withdrawn[0]["tournament_name"], "Autumn Open");

    // The entry is gone from the cup, not lingering on a season the team left.
    let regs: serde_json::Value = app
        .get_auth(&format!("/v1/tournaments/{cup}/registrations"))
        .await
        .json();
    assert_eq!(regs["data"].as_array().map_or(0, Vec::len), 0);
    assert!(roster_has(&app, &to_season, &team_id).await);
}

/// Once a cup has a bracket its matches reference the entry; the move is
/// refused, and the entry stays.
#[tokio::test]
async fn test_moving_a_team_entered_in_a_scheduled_cup_is_refused() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Bracket League", "bracket-league").await;
    let to_league = create_league(&app, "Other League", "other-league").await;
    let from_season = first_season(&app, &from_league).await;
    let to_season = first_season(&app, &to_league).await;
    let (team_id, team_season_id) = create_team(&app, &from_season, "Locked In", "LOCK").await;
    let cup = create_open_team_cup(
        &app,
        &from_league,
        &from_season,
        "Winter Open",
        "winter-open",
    )
    .await;
    app.post_json(
        &format!("/v1/tournaments/{cup}/registrations/team"),
        &json!({ "team_season_id": team_season_id, "participant_name": "Locked In" }),
    )
    .await
    .assert_status(StatusCode::CREATED);
    app.post_auth(&format!("/v1/tournaments/{cup}/close-registration"))
        .await
        .assert_status(StatusCode::OK);

    let response = app
        .post_json(
            &format!("/v1/league-teams/{team_id}/move"),
            &json!({ "league_id": to_league, "season_id": to_season }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or("")
            .contains("Winter Open"),
        "the refusal must name the cup: {body}"
    );
    let regs: serde_json::Value = app
        .get_auth(&format!("/v1/tournaments/{cup}/registrations"))
        .await
        .json();
    assert_eq!(regs["data"].as_array().map_or(0, Vec::len), 1);
    assert!(roster_has(&app, &from_season, &team_id).await);
}

/// A one-a-side team cup in `registration`, inside the given league season.
async fn create_open_team_cup(
    app: &TestApp,
    league_id: &str,
    season_id: &str,
    name: &str,
    slug: &str,
) -> String {
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "league_id": league_id,
                "season_id": season_id,
                "name": name,
                "slug": slug,
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "team",
                "team_size": 1,
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

/// A team in the season, returning (team id, team-season id).
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

/// The move is narrow on purpose: a team with results would either drag
/// another league's seasons along or lose them.
#[tokio::test]
async fn test_moving_a_team_that_has_played_is_refused() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Played League", "played-league").await;
    let to_league = create_league(&app, "Target League", "target-league").await;

    let from_season = first_season(&app, &from_league).await;
    let to_season = first_season(&app, &to_league).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{from_season}/teams"),
            &json!({ "name": "Veteran Team", "tag": "VET" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();

    sqlx::query("UPDATE league_team_seasons SET matches_played = 3 WHERE team_id = $1::uuid")
        .bind(&team_id)
        .execute(app.pool())
        .await
        .expect("record some history");

    let response = app
        .post_json(
            &format!("/v1/league-teams/{team_id}/move"),
            &json!({ "league_id": to_league, "season_id": to_season }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Still where it was.
    let body: serde_json::Value = app
        .get_auth(&format!("/v1/league-teams/{team_id}"))
        .await
        .json();
    assert_eq!(body["data"]["league_id"], from_league.as_str());
}

/// A season in one league is not a destination in another.
#[tokio::test]
async fn test_moving_a_team_into_a_mismatched_season_is_refused() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Origin League", "origin-league").await;
    let other_league = create_league(&app, "Other League", "other-league").await;
    let third_league = create_league(&app, "Third League", "third-league").await;

    let from_season = first_season(&app, &from_league).await;
    let third_season = first_season(&app, &third_league).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{from_season}/teams"),
            &json!({ "name": "Mismatch Team", "tag": "MSM" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();

    // Target league says one thing, target season belongs to another.
    app.post_json(
        &format!("/v1/league-teams/{team_id}/move"),
        &json!({ "league_id": other_league, "season_id": third_season }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

/// A tournament created against the wrong league can be re-filed while it is
/// still empty.
#[tokio::test]
async fn test_moving_a_tournament_between_leagues() {
    let app = TestApp::new().await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();
    let from_league = create_league(&app, "From Cup League", "from-cup-league").await;
    let to_league = create_league(&app, "To Cup League", "to-cup-league").await;

    let to_season = first_season(&app, &to_league).await;

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "league_id": from_league,
                "name": "Misfiled Cup",
                "slug": "misfiled-cup",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
                "registration_type": "open",
                "scheduling_mode": "live",
                "default_match_format": "bo3"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let tournament_id = created["data"]["id"].as_str().unwrap().to_string();

    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/move"),
            &json!({ "league_id": to_league, "season_id": to_season }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["league_id"], to_league.as_str());
    assert_eq!(body["data"]["season_id"], to_season.as_str());

    // And it can be detached entirely.
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/move"),
            &json!({ "league_id": null, "season_id": null }),
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["data"]["league_id"].is_null() || body["data"].get("league_id").is_none());
}

/// Once somebody has entered, the tournament stays where it is: a
/// registration is a team *in that league's season*.
#[tokio::test]
async fn test_moving_a_tournament_with_registrations_is_refused() {
    let app = TestApp::new().await;
    let from_league = create_league(&app, "Entered League", "entered-league").await;
    let to_league = create_league(&app, "Empty League", "empty-league").await;
    let game_id = get_game_id(app.pool(), "cs2").await.to_string();

    let response = app
        .post_json(
            "/v1/tournaments",
            &json!({
                "game_id": game_id,
                "league_id": from_league,
                "name": "Entered Cup",
                "slug": "entered-cup",
                "format": "single_elimination",
                "map_pool": portal_test::builders::DEFAULT_CS2_MAP_POOL,
                "participant_type": "individual",
                "min_participants": 2,
                "max_participants": 16,
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
    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/registrations/player"),
        &json!({ "participant_name": "Someone" }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    app.post_json(
        &format!("/v1/tournaments/{tournament_id}/move"),
        &json!({ "league_id": to_league, "season_id": null }),
    )
    .await
    .assert_status(StatusCode::BAD_REQUEST);
}

/// Both moves are platform-admin actions: they cross a league boundary, so
/// neither league's own admins can make them alone.
#[tokio::test]
async fn test_moves_require_platform_admin() {
    let app = TestApp::new().await;
    let league_id = create_league(&app, "Move Guard League", "move-guard-league").await;

    let season_id = first_season(&app, &league_id).await;

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "Guarded Team", "tag": "GRD" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["team"]["id"].as_str().unwrap().to_string();

    let outsider = UserBuilder::new()
        .username("moveoutsider")
        .email("moveoutsider@example.com")
        .build_persisted(app.pool())
        .await;
    let token = portal_domain::generate_access_token(
        outsider.id,
        outsider.id,
        "moveoutsider",
        "test-jwt-secret",
    )
    .expect("token");

    app.post_json_with_token(
        &format!("/v1/league-teams/{team_id}/move"),
        &json!({ "league_id": league_id, "season_id": season_id }),
        &token,
    )
    .await
    .assert_status(StatusCode::FORBIDDEN);
}
