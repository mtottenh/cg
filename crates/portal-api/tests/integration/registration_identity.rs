//! Integration tests for `GET /v1/tournaments/{id}/registrations/me` and
//! `.../registrations/counts` (P-167).
//!
//! `TournamentDetailPage` decided whether the viewer was registered by fetching
//! `GET /v1/tournaments/{id}/registrations` — with no `per_page`, so the API
//! default of 20 — and scanning the page for them. Everyone past row 20 was
//! shown the "Join This Tournament" call to action: no Registered chip, no
//! withdraw control, no check-in. The organiser's numbers came from the same
//! 20-row sample, so a 64-slot tournament with 40 entrants rendered "20 / 64"
//! and "20 pending approvals".
//!
//! Both tests below run at a scale past the LARGEST page a client can request
//! (`PaginationParams::limit()` caps `per_page` at 100), not merely past the
//! default of 20: a fix that raised the page size would still pass a 25-row
//! test, and this codebase has already hit this exact defect at 20 and at 100.
//! The premise — the subject is genuinely unreachable by paging — is asserted
//! rather than assumed.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_core::types::TournamentRegistrationStatus;
use portal_test::prelude::*;
use uuid::Uuid;

/// One past the hard `per_page` cap.
const REGISTRATIONS: usize = 101;
/// How many of the rows above are left `pending`.
const PENDING: usize = 30;
/// How many are `withdrawn` (they must not count as participants).
const WITHDRAWN: usize = 4;

struct IdentityFixture {
    tournament_id: Uuid,
    /// Registration of the participant who sorts LAST.
    late_registration_id: Uuid,
    late_token: String,
    /// A registered user with no registration in this tournament.
    stranger_token: String,
    /// A team registration nobody but its registrant created…
    team_registration_id: Uuid,
    /// …and a roster member of that team who did not create it.
    team_member_token: String,
}

/// `REGISTRATIONS` rows, the subject registered last.
///
/// Seeds are left unset: `list_by_tournament` orders by
/// `seed ASC NULLS LAST, registered_at ASC`, so registration order IS list
/// order and "the subject is row 101" is a property of the fixture.
async fn setup(app: &TestApp) -> IdentityFixture {
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();

    let game_id = get_game_id(app.pool(), "cs2").await;
    let organiser = UserBuilder::new()
        .username(format!("p167_org_{short}"))
        .build_persisted(app.pool())
        .await;

    let tournament = TournamentBuilder::new()
        .game_id(game_id)
        .created_by(organiser.id)
        .name(format!("P167 {short}"))
        .slug(format!("p167-{short}"))
        .single_elimination()
        .registration_open()
        .build_persisted(app.pool())
        .await;

    let mut late_registration_id = None;
    let mut late_token = None;
    for i in 0..REGISTRATIONS {
        let user = UserBuilder::new()
            .username(format!("p167_{short}_{i:03}"))
            .build_persisted(app.pool())
            .await;
        let builder = TournamentRegistrationBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .player_id_from_uuid(user.id)
            .participant_name(format!("Player {i:03}"))
            .registered_by_uuid(user.id);
        // A realistic mix: some awaiting approval, a few gone.
        let builder = if i < PENDING {
            builder.status(TournamentRegistrationStatus::Pending)
        } else if i < PENDING + WITHDRAWN {
            builder.status(TournamentRegistrationStatus::Withdrawn)
        } else {
            builder.approved()
        };
        let registration = builder.build_persisted(app.pool()).await;

        if i == REGISTRATIONS - 1 {
            late_registration_id = Some(registration.id.as_uuid());
            late_token = Some(create_test_token(
                user.id,
                user.id,
                &user.username,
                TEST_JWT_SECRET,
            ));
        }
    }

    // A team registration created by the captain, plus an ordinary roster
    // member who did not create it: "mine" must follow the roster (P-168), not
    // the `registered_by` column.
    let captain = UserBuilder::new()
        .username(format!("p167_cap_{short}"))
        .build_persisted(app.pool())
        .await;
    let member = UserBuilder::new()
        .username(format!("p167_mem_{short}"))
        .build_persisted(app.pool())
        .await;
    let league = LeagueBuilder::new()
        .name(format!("P167 League {short}"))
        .slug(format!("p167-league-{short}"))
        .build_persisted(app.pool())
        .await;
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(format!("P167 Season {short}"))
        .slug(format!("p167-season-{short}"))
        .registration()
        .build_persisted(app.pool())
        .await;
    let team = LeagueTeamBuilder::new()
        .name(format!("P167 Team {short}"))
        .tag("P167")
        .league_id(league.id)
        .owner(captain.id)
        .build_persisted(app.pool())
        .await;
    let team_season = LeagueTeamSeasonBuilder::new()
        .team_id(team.id)
        .season_id(season.id)
        .build_persisted(app.pool())
        .await;
    for (player, role) in [(captain.id, "captain"), (member.id, "player")] {
        LeagueTeamMemberBuilder::new()
            .team_season_id(team_season.id)
            .player_id(player)
            .role(role)
            .build_persisted(app.pool())
            .await;
    }
    let team_registration = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_season.id)
        .participant_name("P167 Team")
        .registered_by_uuid(captain.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    let stranger = UserBuilder::new()
        .username(format!("p167_stranger_{short}"))
        .build_persisted(app.pool())
        .await;

    IdentityFixture {
        tournament_id: tournament.id,
        late_registration_id: late_registration_id.expect("late registration"),
        late_token: late_token.expect("late token"),
        stranger_token: create_test_token(
            stranger.id,
            stranger.id,
            &stranger.username,
            TEST_JWT_SECRET,
        ),
        team_registration_id: team_registration.id.as_uuid(),
        team_member_token: create_test_token(
            member.id,
            member.id,
            &member.username,
            TEST_JWT_SECRET,
        ),
    }
}

/// The premise. If the subject were reachable by paging, the test below would
/// pass for the wrong reason — including against the very scan it replaces.
#[tokio::test]
async fn test_the_subject_is_unreachable_through_the_paginated_list() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    for per_page in [20, 100] {
        let response = app
            .get_with_token(
                &format!(
                    "/v1/tournaments/{}/registrations?per_page={per_page}&page=1",
                    f.tournament_id
                ),
                &f.late_token,
            )
            .await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        let rows = body["data"].as_array().expect("registrations page");
        assert_eq!(
            rows.len(),
            per_page,
            "per_page={per_page} must return a full page — PaginationParams::limit() caps it at 100"
        );
        let subject = f.late_registration_id.to_string();
        assert!(
            !rows
                .iter()
                .any(|r| r["id"].as_str() == Some(subject.as_str())),
            "the subject must NOT be on page 1 at per_page={per_page} — otherwise this fixture \
             does not reproduce the ceiling"
        );
    }
}

#[tokio::test]
async fn test_my_registrations_resolves_a_row_past_the_page_ceiling() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{}/registrations/me", f.tournament_id),
            &f.late_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    let rows = body["data"]["registrations"].as_array().expect("rows");
    assert_eq!(
        rows.len(),
        1,
        "the subject holds exactly one registration here; got {rows:?}"
    );
    assert_eq!(
        rows[0]["id"].as_str(),
        Some(f.late_registration_id.to_string().as_str()),
        "resolving the caller's own registration must not depend on where their row sorts"
    );
    assert_eq!(rows[0]["status"].as_str(), Some("approved"));
}

/// A roster member who did not create the registration still gets it — the
/// same rule that authorizes result submission (P-168), so the page cannot
/// offer an affordance the API will refuse.
#[tokio::test]
async fn test_my_registrations_follows_the_roster_not_the_registrant() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{}/registrations/me", f.tournament_id),
            &f.team_member_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    let ids: Vec<&str> = body["data"]["registrations"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec![f.team_registration_id.to_string().as_str()],
        "an active member of the registered team-season speaks for it"
    );
}

#[tokio::test]
async fn test_my_registrations_is_empty_for_a_non_participant() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get_with_token(
            &format!("/v1/tournaments/{}/registrations/me", f.tournament_id),
            &f.stranger_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    assert_eq!(
        body["data"]["registrations"].as_array().map(Vec::len),
        Some(0),
        "a non-participant must resolve to nothing — this is what makes the join \
         call-to-action correct when it IS correct"
    );
}

/// The counts are real counts, not the size of a page.
#[tokio::test]
async fn test_registration_counts_are_totals_not_page_lengths() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get(&format!(
            "/v1/tournaments/{}/registrations/counts",
            f.tournament_id
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let counts = &response.json::<serde_json::Value>()["data"];

    // +1 for the team registration.
    let total = REGISTRATIONS as i64 + 1;
    assert_eq!(counts["total"].as_i64(), Some(total));
    assert_eq!(
        counts["pending"].as_i64(),
        Some(PENDING as i64),
        "the pending badge must count every waiting row, not the pending rows of page 1 \
         (which would have read 20)"
    );
    assert_eq!(counts["withdrawn"].as_i64(), Some(WITHDRAWN as i64));
    assert_eq!(
        counts["participating"].as_i64(),
        Some(total - WITHDRAWN as i64),
        "withdrawn rows are not participants; disqualified ones are not either"
    );
    assert!(
        counts["total"].as_i64().unwrap() > 100,
        "the fixture must exceed the largest page a client can request, or this test \
         would pass against the page-length arithmetic it replaces"
    );
}
