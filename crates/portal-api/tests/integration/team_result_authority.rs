//! Who may report a team's result (P-168).
//!
//! `ResultService::find_user_registration` matched
//! `registration.registered_by == user_id`, so for a TEAM registration exactly
//! one human — whoever clicked "register" — could submit or confirm a result.
//! A co-captain, or a captain who replaced the original registrant, got
//! `NotAuthorized`, while the frontend (which gates the submission panel on
//! roster membership) offered them the form. The backend also disagreed with
//! itself: `raise_dispute` and the dispute thread used the broader
//! team-membership rule, so the same person could dispute a result they were
//! refused permission to submit.
//!
//! All of it now resolves through `speaks_for_registration`: an active member
//! of the registered team-season, or (for individual registrations) the
//! registered player. These tests pin the three cases that matter — a
//! non-registrant member CAN submit, a non-registrant member on the other side
//! CAN confirm, and someone on neither roster CANNOT.
//!
//! Authorization is asserted with real, non-dev tokens throughout:
//! `PermissionChecker` short-circuits for `dev-token` in `test-utils` builds,
//! so a test that used the dev account would prove nothing about
//! authorization.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use serde_json::json;
use uuid::Uuid;

struct TeamMatchFixture {
    tournament_id: Uuid,
    match_id: Uuid,
    team_a_registration_id: Uuid,
    team_b_registration_id: Uuid,
    /// Team A's registrant — the ONLY person the pre-fix rule allowed to act.
    /// Used where a test needs a claim on the table without depending on the
    /// fix, so the negative tests stay green under a red-proof probe.
    a_registrant_token: String,
    /// Team A member who did NOT register the team (a second captain).
    a_cocaptain_token: String,
    /// Team B member who did NOT register the team (an ordinary roster player).
    b_member_token: String,
    /// A registered user on neither roster.
    outsider_token: String,
}

/// Two rostered teams facing each other in an `in_progress` match, where the
/// person who created each registration is NOT the person who acts below.
async fn setup(app: &TestApp) -> TeamMatchFixture {
    let short = Uuid::new_v4().simple().to_string()[..8].to_string();

    let user = async |name: &str| {
        UserBuilder::new()
            .username(format!("p168_{name}_{short}"))
            .build_persisted(app.pool())
            .await
    };

    let a_registrant = user("a_reg").await;
    let a_cocaptain = user("a_co").await;
    let b_registrant = user("b_reg").await;
    let b_member = user("b_mem").await;
    let outsider = user("out").await;

    let league = LeagueBuilder::new()
        .name(format!("P168 League {short}"))
        .slug(format!("p168-league-{short}"))
        .build_persisted(app.pool())
        .await;
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(format!("P168 Season {short}"))
        .slug(format!("p168-season-{short}"))
        .registration()
        .build_persisted(app.pool())
        .await;

    let team_season = async |label: &str, owner: Uuid, members: Vec<(Uuid, &str)>| {
        let team = LeagueTeamBuilder::new()
            .name(format!("P168 {label} {short}"))
            .tag(label)
            .league_id(league.id)
            .owner(owner)
            .build_persisted(app.pool())
            .await;
        let team_season = LeagueTeamSeasonBuilder::new()
            .team_id(team.id)
            .season_id(season.id)
            .build_persisted(app.pool())
            .await;
        for (player_id, role) in members {
            LeagueTeamMemberBuilder::new()
                .team_season_id(team_season.id)
                .player_id(player_id)
                .role(role)
                .build_persisted(app.pool())
                .await;
        }
        team_season.id
    };

    let team_a_season = team_season(
        "AAA",
        a_registrant.id,
        vec![(a_registrant.id, "captain"), (a_cocaptain.id, "captain")],
    )
    .await;
    let team_b_season = team_season(
        "BBB",
        b_registrant.id,
        vec![(b_registrant.id, "captain"), (b_member.id, "player")],
    )
    .await;

    let game_id = get_game_id(app.pool(), "cs2").await;
    let tournament = TournamentBuilder::new()
        .game_id(game_id)
        .created_by(a_registrant.id)
        .name(format!("P168 Tournament {short}"))
        .slug(format!("p168-tournament-{short}"))
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

    // Registered BY the captains who then take no further part — that is the
    // whole point of the fixture.
    let team_a_registration = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_a_season)
        .participant_name("Team AAA")
        .registered_by_uuid(a_registrant.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let team_b_registration = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_b_season)
        .participant_name("Team BBB")
        .registered_by_uuid(b_registrant.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    let match_ = TournamentMatchBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .stage_id(stage.id)
        .bracket_id(bracket.id)
        .round(1)
        .match_number(1)
        .bo1()
        .participant1(team_a_registration.id, "Team AAA")
        .participant2(team_b_registration.id, "Team BBB")
        .build_persisted(app.pool())
        .await;

    // Result submission requires `in_progress` (or `awaiting_result`). The
    // builder has no status setter; the admin transition path is exercised by
    // its own tests and is not what this file is about.
    sqlx::query("UPDATE tournament_matches SET status = 'in_progress' WHERE id = $1")
        .bind(match_.id.as_uuid())
        .execute(app.pool())
        .await
        .expect("failed to set match in_progress");

    let token = |u: &portal_db::entities::UserRow| {
        create_test_token(u.id, u.id, &u.username, TEST_JWT_SECRET)
    };

    TeamMatchFixture {
        tournament_id: tournament.id,
        match_id: match_.id.as_uuid(),
        team_a_registration_id: team_a_registration.id.as_uuid(),
        team_b_registration_id: team_b_registration.id.as_uuid(),
        a_registrant_token: token(&a_registrant),
        a_cocaptain_token: token(&a_cocaptain),
        b_member_token: token(&b_member),
        outsider_token: token(&outsider),
    }
}

fn claim_body(winner: Uuid) -> serde_json::Value {
    json!({
        "claimed_winner_registration_id": winner.to_string(),
        "participant1_score": 1,
        "participant2_score": 0,
        "game_results": [{
            "game_number": 1,
            "map_id": "de_dust2",
            "participant1_score": 16,
            "participant2_score": 10
        }],
        "evidence_ids": [],
        "demo_link_ids": []
    })
}

/// THE defect: the co-captain is on the roster but did not create the
/// registration, so `registered_by == user_id` refused them — and the match
/// page had already rendered them the submission form.
#[tokio::test]
async fn test_team_member_who_did_not_register_can_submit_a_result() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result", f.match_id),
            &claim_body(f.team_a_registration_id),
            &f.a_cocaptain_token,
        )
        .await;

    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]["claim"]["submitted_by_registration_id"].as_str(),
        Some(f.team_a_registration_id.to_string().as_str()),
        "the claim must be attributed to the team the submitter plays for, not to whoever \
         happened to register it"
    );
}

/// The other half of the flow: confirmation was gated on the same rule, so a
/// team whose registrant was unavailable could not accept a correct score
/// either — it sat until the 24h auto-confirm.
#[tokio::test]
async fn test_opposing_team_member_who_did_not_register_can_confirm() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let submitted = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result", f.match_id),
            &claim_body(f.team_a_registration_id),
            &f.a_cocaptain_token,
        )
        .await;
    submitted.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = submitted.json();
    let claim_id = body["data"]["claim"]["id"].as_str().unwrap().to_string();

    // An ordinary roster player, not a captain and not the registrant. The
    // rule is roster membership — the same one the dispute thread has always
    // used and the one the frontend gates its panels on.
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result/{}/confirm", f.match_id, claim_id),
            &json!({}),
            &f.b_member_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    let match_row: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, winner_registration_id FROM tournament_matches WHERE id = $1",
    )
    .bind(f.match_id)
    .fetch_one(app.pool())
    .await
    .expect("match row");
    assert_eq!(match_row.0, "completed");
    assert_eq!(
        match_row.1,
        Some(f.team_a_registration_id),
        "confirmation must actually write the result, not merely return 200"
    );
}

/// The rule still refuses everyone else — widening authority to the roster
/// must not widen it to the tournament.
#[tokio::test]
async fn test_non_member_cannot_submit_or_confirm() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let refused = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result", f.match_id),
            &claim_body(f.team_a_registration_id),
            &f.outsider_token,
        )
        .await;
    refused.assert_status(StatusCode::FORBIDDEN);

    // …and with a claim on the table, they cannot confirm it either. The claim
    // is submitted by the REGISTRANT, who could act under the old rule too, so
    // this test does not depend on the fix — it stays green when the fix is
    // reverted, which is what proves a red-proof probe hit the rule and not the
    // plumbing.
    let submitted = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result", f.match_id),
            &claim_body(f.team_a_registration_id),
            &f.a_registrant_token,
        )
        .await;
    submitted.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = submitted.json();
    let claim_id = body["data"]["claim"]["id"].as_str().unwrap().to_string();

    let refused = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result/{}/confirm", f.match_id, claim_id),
            &json!({}),
            &f.outsider_token,
        )
        .await;
    refused.assert_status(StatusCode::FORBIDDEN);
}

/// Submission and disputing are now authorized by ONE rule, so they cannot
/// disagree about the same person. Before, `raise_dispute` used team
/// membership while submission used `registered_by`, so this caller could
/// dispute a result they were forbidden to submit.
#[tokio::test]
async fn test_dispute_and_submission_agree_on_who_speaks_for_a_team() {
    let app = TestApp::new().await;
    let f = setup(&app).await;

    let submitted = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result", f.match_id),
            &claim_body(f.team_a_registration_id),
            &f.a_cocaptain_token,
        )
        .await;
    submitted.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = submitted.json();
    let claim_id = body["data"]["claim"]["id"].as_str().unwrap().to_string();

    // The opposing roster member may dispute — and, per the test above, may
    // also confirm. Same rule, both directions.
    let disputed = app
        .post_json_with_token(
            &format!("/v1/matches/{}/result/{}/dispute", f.match_id, claim_id),
            &json!({ "reason": "Scoreline is wrong", "evidence_ids": [] }),
            &f.b_member_token,
        )
        .await;
    disputed.assert_status(StatusCode::OK);

    // The outsider is refused by the dispute path exactly as by submission.
    let refused = app
        .post_json_with_token(
            &format!(
                "/v1/tournaments/{}/matches/{}/dispute",
                f.tournament_id, f.match_id
            ),
            &json!({
                "registration_id": f.team_b_registration_id.to_string(),
                "reason": "other",
                "description": "This caller is on neither roster and must be refused.",
                "evidence_ids": []
            }),
            &f.outsider_token,
        )
        .await;
    refused.assert_status(StatusCode::FORBIDDEN);
}
