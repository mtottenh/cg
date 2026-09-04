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
        .get(&format!(
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

    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    // The league trigger creates "Season 1".
    let season_id = body["data"][0]["id"].as_str().unwrap().to_string();

    let response = app
        .post_json(
            &format!("/v1/league-seasons/{season_id}/teams"),
            &json!({ "name": "Archivable Team", "tag": "ARCH" }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let created: serde_json::Value = response.json();
    let team_id = created["data"]["id"].as_str().unwrap().to_string();
    let status_before = created["data"]["status"].as_str().unwrap().to_string();

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

    let body: serde_json::Value = app
        .get(&format!("/v1/league-seasons?league_id={league_id}"))
        .await
        .json();
    let season_id = body["data"][0]["id"].as_str().unwrap().to_string();

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
