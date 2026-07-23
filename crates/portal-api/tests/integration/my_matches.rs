//! Integration tests for `GET /v1/users/me/matches`.
//!
//! P-29 — the endpoint returned HTTP 500 to *every* caller. The backing query
//! (`PgTournamentMatchRepository::list_by_player`) did `SELECT DISTINCT tm.*`
//! and then ordered by a `CASE tm.status ... END` expression that was not in
//! the select list, which Postgres rejects outright with "for SELECT DISTINCT,
//! ORDER BY expressions must appear in select list". The failure was not
//! data-dependent: an empty result set 500s just as reliably as a populated one.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_test::prelude::*;
use uuid::Uuid;

/// A player who reaches their matches through both supported paths.
struct MatchesFixture {
    token: String,
    /// Match where the player occupies BOTH slots (individual registration vs
    /// the team whose roster they are on) — the row that `DISTINCT` collapses.
    both_slots_match_id: Uuid,
    /// Player's individual registration vs an unrelated opponent.
    individual_match_id: Uuid,
    /// Player's team vs an unrelated opponent.
    team_match_id: Uuid,
    /// A match between two strangers — must never be returned.
    unrelated_match_id: Uuid,
}

async fn set_match_status(
    app: &TestApp,
    match_id: Uuid,
    status: &str,
    scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
) {
    sqlx::query("UPDATE tournament_matches SET status = $2, scheduled_at = $3 WHERE id = $1")
        .bind(match_id)
        .bind(status)
        .bind(scheduled_at)
        .execute(app.pool())
        .await
        .expect("failed to set match status");
}

/// Builds a tournament in which the player under test is reachable
/// both as an individual registrant and via a league-team-season roster.
async fn setup(app: &TestApp) -> MatchesFixture {
    let suffix = Uuid::new_v4().simple().to_string();
    let short = &suffix[..8];

    let subject = UserBuilder::new()
        .username(format!("p29_subject_{short}"))
        .build_persisted(app.pool())
        .await;
    let opponent = UserBuilder::new()
        .username(format!("p29_opponent_{short}"))
        .build_persisted(app.pool())
        .await;
    let stranger_a = UserBuilder::new()
        .username(format!("p29_stranger_a_{short}"))
        .build_persisted(app.pool())
        .await;
    let stranger_b = UserBuilder::new()
        .username(format!("p29_stranger_b_{short}"))
        .build_persisted(app.pool())
        .await;

    // League team season the subject is rostered on.
    let league = LeagueBuilder::new()
        .name(format!("P29 League {short}"))
        .slug(format!("p29-league-{short}"))
        .build_persisted(app.pool())
        .await;
    let season = LeagueSeasonBuilder::new()
        .league_id(league.id)
        .name(format!("P29 Season {short}"))
        .slug(format!("p29-season-{short}"))
        .registration()
        .build_persisted(app.pool())
        .await;
    let team = LeagueTeamBuilder::new()
        .name(format!("P29 Team {short}"))
        .tag("P29")
        .league_id(league.id)
        .owner(subject.id)
        .build_persisted(app.pool())
        .await;
    let team_season = LeagueTeamSeasonBuilder::new()
        .team_id(team.id)
        .season_id(season.id)
        .build_persisted(app.pool())
        .await;
    LeagueTeamMemberBuilder::new()
        .team_season_id(team_season.id)
        .player_id(subject.id)
        .role("player")
        .build_persisted(app.pool())
        .await;

    // Tournament scaffolding.
    let game_id = get_game_id(app.pool(), "cs2").await;
    let tournament = TournamentBuilder::new()
        .game_id(game_id)
        .created_by(subject.id)
        .name(format!("P29 Tournament {short}"))
        .slug(format!("p29-tournament-{short}"))
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

    // Registrations: the subject as an individual, the subject's team, and
    // three unrelated participants.
    let individual_reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .player_id_from_uuid(subject.id)
        .participant_name("Subject (solo)")
        .registered_by_uuid(subject.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let team_reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .team_season_id_from_uuid(team_season.id)
        .participant_name("Subject's team")
        .registered_by_uuid(subject.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let opponent_reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .player_id_from_uuid(opponent.id)
        .participant_name("Opponent")
        .registered_by_uuid(opponent.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let stranger_a_reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .player_id_from_uuid(stranger_a.id)
        .participant_name("Stranger A")
        .registered_by_uuid(stranger_a.id)
        .approved()
        .build_persisted(app.pool())
        .await;
    let stranger_b_reg = TournamentRegistrationBuilder::new()
        .tournament_id_from_uuid(tournament.id)
        .player_id_from_uuid(stranger_b.id)
        .participant_name("Stranger B")
        .registered_by_uuid(stranger_b.id)
        .approved()
        .build_persisted(app.pool())
        .await;

    let new_match = |number: i32| {
        TournamentMatchBuilder::new()
            .tournament_id_from_uuid(tournament.id)
            .stage_id(stage.id)
            .bracket_id(bracket.id)
            .round(1)
            .match_number(number)
    };

    let both_slots = new_match(1)
        .participant1(individual_reg.id, "Subject (solo)")
        .participant2(team_reg.id, "Subject's team")
        .build_persisted(app.pool())
        .await;
    let individual_match = new_match(2)
        .participant1(individual_reg.id, "Subject (solo)")
        .participant2(opponent_reg.id, "Opponent")
        .build_persisted(app.pool())
        .await;
    let team_match = new_match(3)
        .participant1(team_reg.id, "Subject's team")
        .participant2(opponent_reg.id, "Opponent")
        .build_persisted(app.pool())
        .await;
    let unrelated = new_match(4)
        .participant1(stranger_a_reg.id, "Stranger A")
        .participant2(stranger_b_reg.id, "Stranger B")
        .build_persisted(app.pool())
        .await;

    // Statuses chosen so the intended ranking (`in_progress` first,
    // `completed` last) is distinguishable from insertion order.
    let scheduled_at = chrono::Utc::now() + chrono::Duration::hours(2);
    set_match_status(app, team_match.id.as_uuid(), "in_progress", None).await;
    set_match_status(
        app,
        both_slots.id.as_uuid(),
        "scheduled",
        Some(scheduled_at),
    )
    .await;
    set_match_status(app, individual_match.id.as_uuid(), "completed", None).await;
    set_match_status(app, unrelated.id.as_uuid(), "in_progress", None).await;

    MatchesFixture {
        token: create_test_token(
            subject.id,
            subject.id,
            &format!("p29_subject_{short}"),
            TEST_JWT_SECRET,
        ),
        both_slots_match_id: both_slots.id.as_uuid(),
        individual_match_id: individual_match.id.as_uuid(),
        team_match_id: team_match.id.as_uuid(),
        unrelated_match_id: unrelated.id.as_uuid(),
    }
}

fn returned_ids(body: &serde_json::Value) -> Vec<Uuid> {
    body["data"]
        .as_array()
        .expect("data must be an array")
        .iter()
        .map(|m| {
            m["id"]
                .as_str()
                .expect("match id must be a string")
                .parse()
                .expect("match id must be a UUID")
        })
        .collect()
}

/// The endpoint must answer 200 with the caller's matches — reached through an
/// individual registration or through a league-team-season roster — ranked by
/// status urgency, and must not leak matches the caller is not in.
#[tokio::test]
async fn test_my_matches_returns_player_matches_ordered_by_status() {
    let app = TestApp::new().await;
    let fixture = setup(&app).await;

    let response = app
        .get_with_token("/v1/users/me/matches", &fixture.token)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let ids = returned_ids(&body);

    assert_eq!(
        ids,
        vec![
            fixture.team_match_id,
            fixture.both_slots_match_id,
            fixture.individual_match_id,
        ],
        "matches must be ranked in_progress (1) → scheduled (4) → completed (9)"
    );
    assert!(
        !ids.contains(&fixture.unrelated_match_id),
        "a match between two other participants must not be returned"
    );
}

/// A match the player reaches through BOTH the individual-registration path and
/// the team-roster path joins twice; `DISTINCT` must collapse it to one row.
#[tokio::test]
async fn test_my_matches_does_not_duplicate_dual_path_match() {
    let app = TestApp::new().await;
    let fixture = setup(&app).await;

    let response = app
        .get_with_token("/v1/users/me/matches", &fixture.token)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    let ids = returned_ids(&body);

    let occurrences = ids
        .iter()
        .filter(|id| **id == fixture.both_slots_match_id)
        .count();
    assert_eq!(
        occurrences, 1,
        "the match the player reaches via both an individual registration and \
         a team roster must appear exactly once, got {occurrences}"
    );

    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "no match may be listed twice");
}

/// The `status` filter narrows the same query — it must not reintroduce the
/// 500, and it must return only matches in that status.
#[tokio::test]
async fn test_my_matches_status_filter() {
    let app = TestApp::new().await;
    let fixture = setup(&app).await;

    let response = app
        .get_with_token("/v1/users/me/matches?status=completed", &fixture.token)
        .await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(
        returned_ids(&body),
        vec![fixture.individual_match_id],
        "only the completed match should come back"
    );
}

/// The bug was not data-dependent: a player with no matches at all also got a
/// 500, because Postgres rejects the query at planning time.
#[tokio::test]
async fn test_my_matches_empty_for_player_without_matches() {
    let app = TestApp::new().await;

    let user = UserBuilder::new()
        .username(format!(
            "p29_nomatches_{}",
            &Uuid::new_v4().simple().to_string()[..8]
        ))
        .build_persisted(app.pool())
        .await;
    let token = create_test_token(user.id, user.id, "p29_nomatches", TEST_JWT_SECRET);

    let response = app.get_with_token("/v1/users/me/matches", &token).await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(
        body["data"]
            .as_array()
            .expect("data must be an array")
            .len(),
        0
    );
}
