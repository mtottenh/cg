//! Integration tests for `GET /v1/tournaments/{id}/matches/{id}/participants`.
//!
//! P-53 / P-56 — a participant past registration #100 could not submit a
//! result at all.
//!
//! `useMatchDetail` resolved "which registration am I in this match?" by
//! fetching the tournament's registrations list and scanning it in the
//! browser. That list is paginated and `PaginationParams::limit()` clamps
//! `per_page` at 100, so the scan could only ever see the first 100 rows.
//! Every gate on the match page — `canSubmitResult`, `showConfirmationPanel`,
//! `showSchedulingPanel`, `showCheckInPanel` — keys off that resolution, so a
//! participant sorted past row 100 silently lost the ability to submit a
//! result, confirm one, or schedule. 128-player CS2 events are routine.
//!
//! The test below is deliberately run at TRUE scale (101 registrations)
//! because that is the only way to demonstrate the ceiling rather than assert
//! around it: it first proves the subject's registration is genuinely absent
//! from the maximum-size first page (i.e. the old scan could not have found
//! it), then proves the new endpoint resolves them anyway.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use uuid::Uuid;

/// One more than `PaginationParams::limit()`'s hard cap, so exactly one
/// registration falls off the end of the largest page a client can ask for.
const REGISTRATIONS: usize = 101;

struct CeilingFixture {
    tournament_id: Uuid,
    match_id: Uuid,
    /// Registration of the participant who sorts LAST — past the page-1 ceiling.
    late_registration_id: Uuid,
    /// Their opponent in the match, who sorts first.
    opponent_registration_id: Uuid,
    /// Bearer token for the late participant.
    late_token: String,
    /// Bearer token for somebody in the tournament who is not in this match.
    bystander_token: String,
}

/// A tournament with `REGISTRATIONS` approved participants, where the LAST
/// registrant is seated in a match against the FIRST.
///
/// Seeds are deliberately left unset: `list_by_tournament` orders by
/// `seed ASC NULLS LAST, registered_at ASC`, so with no seeds the ordering is
/// purely registration order and "the last registrant is row 101" is a fact
/// about the fixture rather than a hope about a seeding algorithm.
async fn setup(app: &TestApp) -> CeilingFixture {
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();

    let game_id = get_game_id(app.pool(), "cs2").await;
    let organiser = UserBuilder::new()
        .username(format!("p53_org_{short}"))
        .build_persisted(app.pool())
        .await;

    let tournament = TournamentBuilder::new()
        .game_id(game_id)
        .created_by(organiser.id)
        .name(format!("P53 Ceiling {short}"))
        .slug(format!("p53-ceiling-{short}"))
        .single_elimination()
        .in_progress()
        .build_persisted(app.pool())
        .await;
    let stage = TournamentStageBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .single_elimination()
        .build_persisted(app.pool())
        .await;
    let bracket = TournamentBracketBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .stage_id(stage.id)
        .single_elimination()
        .build_persisted(app.pool())
        .await;

    // Registrations are created in order, so index 0 is row 1 and index 100
    // is row 101 — the one the 100-row page cannot reach.
    let mut users = Vec::with_capacity(REGISTRATIONS);
    let mut registrations = Vec::with_capacity(REGISTRATIONS);
    for i in 0..REGISTRATIONS {
        let user = UserBuilder::new()
            .username(format!("p53_{short}_{i:03}"))
            .build_persisted(app.pool())
            .await;
        let registration = TournamentRegistrationBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .player_id_from_uuid(user.id)
            .participant_name(format!("Player {i:03}"))
            .registered_by_uuid(user.id)
            .approved()
            .build_persisted(app.pool())
            .await;
        users.push(user);
        registrations.push(registration);
    }

    let first = &registrations[0];
    let late = &registrations[REGISTRATIONS - 1];

    let match_ = TournamentMatchBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .stage_id(stage.id)
        .bracket_id(bracket.id)
        .round(1)
        .match_number(1)
        .participant1(first.id, "Player 000")
        .participant2(late.id, format!("Player {:03}", REGISTRATIONS - 1))
        .build_persisted(app.pool())
        .await;

    let late_user = &users[REGISTRATIONS - 1];
    let bystander = &users[1];

    CeilingFixture {
        tournament_id: tournament.id,
        match_id: match_.id.as_uuid(),
        late_registration_id: late.id.as_uuid(),
        opponent_registration_id: first.id.as_uuid(),
        late_token: create_test_token(
            late_user.id,
            late_user.id,
            &late_user.username,
            TEST_JWT_SECRET,
        ),
        bystander_token: create_test_token(
            bystander.id,
            bystander.id,
            &bystander.username,
            TEST_JWT_SECRET,
        ),
    }
}

/// The premise: the subject really is unreachable through the paginated
/// registrations list at its maximum page size. If this ever stops holding —
/// because the cap moved, or the ordering changed — the ceiling test below
/// would start passing for the wrong reason, so it is asserted explicitly
/// rather than assumed.
#[tokio::test]
async fn test_registration_past_100_is_absent_from_the_largest_page() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get_with_token(
            &format!(
                "/v1/tournaments/{}/registrations?per_page=100&page=1",
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
        100,
        "per_page is capped at 100 by PaginationParams::limit(); a client cannot ask for more"
    );

    let ids: Vec<&str> = rows.iter().filter_map(|r| r["id"].as_str()).collect();
    assert!(
        !ids.contains(&f.late_registration_id.to_string().as_str()),
        "the subject's registration must NOT be on page 1 — otherwise this fixture \
         does not reproduce the ceiling and the endpoint test proves nothing"
    );
}

#[tokio::test]
async fn test_match_participants_resolves_a_registration_past_the_page_ceiling() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .get_with_token(
            &format!(
                "/v1/tournaments/{}/matches/{}/participants",
                f.tournament_id, f.match_id
            ),
            &f.late_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    assert_eq!(
        body["data"]["my_registration_id"].as_str(),
        Some(f.late_registration_id.to_string().as_str()),
        "the caller is participant 2 of this match; resolving them must not depend \
         on where their row falls in the paginated registrations list"
    );

    // Both participants come back, because the composable also needs the
    // opponent (it feeds `opponentPlayerId`, which gates schedule suggestions).
    assert_eq!(
        body["data"]["participant1"]["id"].as_str(),
        Some(f.opponent_registration_id.to_string().as_str())
    );
    assert_eq!(
        body["data"]["participant2"]["id"].as_str(),
        Some(f.late_registration_id.to_string().as_str())
    );
    assert!(
        body["data"]["participant1"]["player_id"].is_string(),
        "the opponent's player_id must be present — it is what opponentPlayerId reads"
    );
}

#[tokio::test]
async fn test_match_participants_reports_no_registration_for_a_non_participant() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    // Registered in the tournament, but not in THIS match. The participant-only
    // panels key off `my_registration_id`, so a bystander resolving to a
    // registration would hand them a submit affordance for someone else's match.
    let response = app
        .get_with_token(
            &format!(
                "/v1/tournaments/{}/matches/{}/participants",
                f.tournament_id, f.match_id
            ),
            &f.bystander_token,
        )
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();

    assert!(
        body["data"]["my_registration_id"].is_null(),
        "a tournament participant who is not in this match must resolve to null, got {:?}",
        body["data"]["my_registration_id"]
    );
    // …but they can still see who is playing: the match page renders for
    // spectators too.
    assert!(body["data"]["participant1"]["id"].is_string());
    assert!(body["data"]["participant2"]["id"].is_string());
}
