//! Demo validator adapter for the match completion saga.
//!
//! Wraps the DemoService to implement the MatchDemoValidator trait.

use async_trait::async_trait;
use portal_core::types::{LineupSource, substitutes_are_minority};
use portal_core::{DomainError, ResultClaimId, TournamentMatchId, TournamentRegistrationId};
use portal_db::{
    PgLeagueTeamMemberRepository, PgMatchLineupRepository, PgTournamentMatchRepository,
    PgTournamentRegistrationRepository, PgTournamentRepository,
};
use portal_domain::entities::demo::DemoPlayer;
use portal_domain::entities::demo_validation::{
    DemoValidationResult, TeamSide, UnrecognizedPlayer,
};
use portal_domain::repositories::league_team::LeagueTeamMemberRepository;
use portal_domain::repositories::match_lineup::MatchLineupRepository;
use portal_domain::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository, TournamentRepository,
};
use portal_domain::services::tournament::{DemoValidationOutcome, MatchDemoValidator};
use std::sync::Arc;
use tracing::debug;

use crate::state::AppDemoService;
use crate::state::AppEligibilityService;
use crate::state::AppResultService;

/// Adapter that wraps DemoService + ResultService to implement MatchDemoValidator.
#[derive(Clone)]
pub struct DemoValidatorAdapter {
    demo_service: AppDemoService,
    result_service: AppResultService,
    /// Phase D enforcement of the §0.4 lineup rules on the demo-derived lineup.
    enforcer: LineupEnforcer,
}

impl DemoValidatorAdapter {
    /// Create a new adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        demo_service: AppDemoService,
        result_service: AppResultService,
        lineup_repo: Arc<PgMatchLineupRepository>,
        match_repo: Arc<PgTournamentMatchRepository>,
        tournament_repo: Arc<PgTournamentRepository>,
        registration_repo: Arc<PgTournamentRegistrationRepository>,
        member_repo: Arc<PgLeagueTeamMemberRepository>,
        eligibility_service: AppEligibilityService,
    ) -> Self {
        Self {
            demo_service,
            result_service,
            enforcer: LineupEnforcer::new(
                lineup_repo,
                match_repo,
                tournament_repo,
                registration_repo,
                member_repo,
                eligibility_service,
            ),
        }
    }
}

/// Enforces the §0.4 lineup rules on the authoritative demo-derived lineup.
///
/// Covers the majority rule and the elo/eligibility caps. Extracted from the
/// adapter so it can be exercised without constructing the demo/result services.
#[derive(Clone)]
pub struct LineupEnforcer {
    lineup_repo: Arc<PgMatchLineupRepository>,
    match_repo: Arc<PgTournamentMatchRepository>,
    tournament_repo: Arc<PgTournamentRepository>,
    registration_repo: Arc<PgTournamentRegistrationRepository>,
    member_repo: Arc<PgLeagueTeamMemberRepository>,
    eligibility_service: AppEligibilityService,
}

impl LineupEnforcer {
    /// Create a new enforcer.
    pub fn new(
        lineup_repo: Arc<PgMatchLineupRepository>,
        match_repo: Arc<PgTournamentMatchRepository>,
        tournament_repo: Arc<PgTournamentRepository>,
        registration_repo: Arc<PgTournamentRegistrationRepository>,
        member_repo: Arc<PgLeagueTeamMemberRepository>,
        eligibility_service: AppEligibilityService,
    ) -> Self {
        Self {
            lineup_repo,
            match_repo,
            tournament_repo,
            registration_repo,
            member_repo,
            eligibility_service,
        }
    }

    /// Registered demo players whose side could not be inferred (§0b
    /// correction): resolved to an account but present in neither side's
    /// demo-derived lineup. They KEEP attribution — this only flags the
    /// ambiguous side for an admin to resolve via the review.
    ///
    /// Self-gating: with no demo-source lineup for the match (season did not
    /// opt in), nothing is flagged.
    pub async fn sideless_registered(
        &self,
        match_id: TournamentMatchId,
        demo_players: &[DemoPlayer],
    ) -> Result<Vec<UnrecognizedPlayer>, DomainError> {
        let lineup_players = self
            .lineup_repo
            .list_players_by_source(match_id, LineupSource::Demo)
            .await?;
        if lineup_players.is_empty() {
            return Ok(Vec::new());
        }
        let in_lineup: std::collections::HashSet<_> =
            lineup_players.iter().map(|p| p.player_id).collect();

        Ok(demo_players
            .iter()
            .filter(|p| {
                p.player_id
                    .is_some_and(|pid| !in_lineup.contains(&pid))
            })
            .map(|p| UnrecognizedPlayer {
                steam_id: p.steam_id.clone(),
                player_name: format!(
                    "SIDE UNASSIGNED: registered player '{}' could not be assigned to a side — admin must resolve (attribution kept)",
                    p.player_name
                ),
                // The side is precisely what is unknown; default to team 1 for
                // the review payload, the admin resolves the real side.
                team_side: TeamSide::Team1,
                registration_side: 1,
            })
            .collect())
    }

    /// Enforce the §0.4 lineup rules on the authoritative (demo-derived) lineup:
    /// the majority rule (`substitutes_are_minority`) and the elo/eligibility
    /// caps (via `EligibilityService`). A violation is surfaced as an
    /// [`UnrecognizedPlayer`] so it RAISES the existing two-captain
    /// `roster_mismatch` review — it does NOT auto-void.
    ///
    /// Self-gating: with no demo-source lineup (season did not opt in), there
    /// are no players to check and nothing is raised.
    pub async fn lineup_violations(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<UnrecognizedPlayer>, DomainError> {
        let Some(match_) = self.match_repo.find_by_id(match_id).await? else {
            return Ok(Vec::new());
        };
        let Some(tournament) = self
            .tournament_repo
            .find_by_id(match_.tournament_id)
            .await?
        else {
            return Ok(Vec::new());
        };
        let game_id = tournament.game_id;
        let settings = tournament.settings;

        // Resolve each side's team-season for the P-26 check (None for
        // individual/adhoc registrations — no roster to face).
        let mut team_seasons = [None, None];
        for (i, reg) in [
            match_.participant1_registration_id,
            match_.participant2_registration_id,
        ]
        .iter()
        .enumerate()
        {
            if let Some(reg) = reg {
                team_seasons[i] = self
                    .registration_repo
                    .find_by_id(*reg)
                    .await?
                    .and_then(|r| r.team_season_id);
            }
        }

        let mut violations = Vec::new();
        for (side, reg, opposing_ts) in [
            (1_i32, match_.participant1_registration_id, team_seasons[1]),
            (2_i32, match_.participant2_registration_id, team_seasons[0]),
        ] {
            let Some(reg) = reg else { continue };
            self.check_registration_lineup(
                match_id,
                reg,
                side,
                game_id,
                &settings,
                opposing_ts,
                &mut violations,
            )
            .await?;
        }
        Ok(violations)
    }

    #[allow(clippy::too_many_arguments)]
    async fn check_registration_lineup(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        side: i32,
        game_id: portal_core::GameId,
        settings: &serde_json::Value,
        opposing_team_season: Option<portal_core::LeagueTeamSeasonId>,
        out: &mut Vec<UnrecognizedPlayer>,
    ) -> Result<(), DomainError> {
        let Some(lineup) = self
            .lineup_repo
            .find_by_match_registration(match_id, registration_id)
            .await?
        else {
            return Ok(());
        };
        let Some(with_players) = self.lineup_repo.get_with_players(lineup.id).await? else {
            return Ok(());
        };

        // Consider the authoritative demo rows, de-duplicated per player (a
        // player appearing across maps counts once for the per-lineup rule).
        let mut seen = std::collections::HashSet::new();
        let mut player_ids = Vec::new();
        let mut sub_count = 0usize;
        for p in with_players
            .players
            .iter()
            .filter(|p| p.source == LineupSource::Demo)
        {
            if seen.insert(p.player_id) {
                player_ids.push(p.player_id);
                if p.is_substitute {
                    sub_count += 1;
                }
            }
        }
        if player_ids.is_empty() {
            return Ok(());
        }

        let team_side = if side == 2 {
            TeamSide::Team2
        } else {
            TeamSide::Team1
        };

        // Majority rule (§0.4): substitutes must be a strict minority.
        if !substitutes_are_minority(sub_count, player_ids.len()) {
            out.push(UnrecognizedPlayer {
                steam_id: String::new(),
                player_name: format!(
                    "LINEUP RULE: substitutes are not a minority ({sub_count} of {} played)",
                    player_ids.len()
                ),
                team_side,
                registration_side: side,
            });
        }

        // Elo / eligibility caps (§5a) on who actually played.
        let elo_violations = self
            .eligibility_service
            .check_players_from_settings(settings, game_id, &player_ids)
            .await?;
        for v in elo_violations {
            out.push(UnrecognizedPlayer {
                steam_id: String::new(),
                player_name: format!("LINEUP RULE ({}): {}", v.restriction, v.message),
                team_side,
                registration_side: side,
            });
        }

        // P-26 (§0.4 rule 4): a player may not play AGAINST a team they are
        // rostered on. Review-raiser only — attribution is never touched.
        if let Some(opposing_ts) = opposing_team_season {
            for pid in &player_ids {
                if self.member_repo.is_member(opposing_ts, *pid).await? {
                    out.push(UnrecognizedPlayer {
                        steam_id: String::new(),
                        player_name: format!(
                            "LINEUP RULE (P-26): player {pid} is rostered on the OPPOSING team and played against it"
                        ),
                        team_side,
                        registration_side: side,
                    });
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MatchDemoValidator for DemoValidatorAdapter {
    async fn validate_match_demos(
        &self,
        match_id: TournamentMatchId,
        claim_id: ResultClaimId,
    ) -> Result<Vec<DemoValidationOutcome>, DomainError> {
        // Get the confirmed claim for game results
        let claim = self.result_service.get_claim_by_id(claim_id).await?;

        debug!(
            claim_id = %claim_id,
            demo_link_ids = ?claim.demo_link_ids,
            game_results_count = claim.game_results.len(),
            "Validating demos for claim"
        );

        // Skip if no demos linked
        if claim.demo_link_ids.is_empty() {
            debug!("No demo_link_ids in claim, skipping validation");
            return Ok(Vec::new());
        }

        // Get demos linked to the match with full data
        let demo_links = self
            .demo_service
            .get_match_demos_with_data(match_id, true, None)
            .await?;

        debug!(
            match_id = %match_id,
            demo_links_count = demo_links.len(),
            "Found demo links for match"
        );

        if demo_links.is_empty() {
            debug!("No demo links found for match, skipping validation");
            return Ok(Vec::new());
        }

        let mut outcomes = Vec::new();

        for link_data in &demo_links {
            let link = &link_data.link;

            // Only validate links that are in the claim's demo_link_ids
            if !claim.demo_link_ids.contains(&link.id) {
                continue;
            }

            // Find the corresponding game result for this link.
            //
            // Prefer the link's own `game_number`, but fall back to the game result
            // that points at THIS link. The auto-linker creates links with
            // `game_number: None` (services/demo.rs:498), so before the fallback every
            // auto-linked demo fell through to the series-level score below and was
            // compared against the demo's per-MAP score: a correct BO1 claim of 1-0
            // (16-14 on the map) was checked against 13-7 and always "mismatched".
            // That raised a spurious review, which pauses the completion saga
            // (match_completion.rs:409) and stops the winner advancing until an admin
            // approves. `GameResult.demo_link_id` carried the mapping all along.
            let game_result = link
                .game_number
                .and_then(|gn| claim.game_results.iter().find(|gr| gr.game_number == gn))
                .or_else(|| {
                    claim
                        .game_results
                        .iter()
                        .find(|gr| gr.demo_link_id == Some(link.id))
                });

            // Build validation result
            let mut validation = DemoValidationResult::default();

            // P-23 revival (Phase D): a demo player who did NOT resolve to a
            // registered account is a "ringer" (§0.4 rule 1 — an outsider has no
            // account). Diff the demo's players against recognised accounts:
            // `player_id == None` after resolution means "no account on the
            // site". These become `unrecognized_players`, which raises the
            // existing two-captain `roster_mismatch` review rather than being
            // silently attributed.
            let demo = &link_data.demo;
            let mut unrecognized = collect_unrecognized_players(&link_data.players, demo);

            // §0b correction: a registered player whose side could not be
            // inferred (in neither side's demo lineup) keeps attribution but is
            // flagged here so an admin resolves the side via the review.
            unrecognized.extend(
                self.enforcer
                    .sideless_registered(match_id, &link_data.players)
                    .await?,
            );
            if let Some(ref metadata) = demo.metadata {
                // Extract claimed score for this game
                let claimed_score = game_result.map_or(
                    (
                        claim.claimed_participant1_score,
                        claim.claimed_participant2_score,
                    ),
                    |gr| (gr.participant1_score, gr.participant2_score),
                );
                validation.claimed_score = claimed_score;

                // Check scores
                let extracted_score = (metadata.team1_score, metadata.team2_score);
                validation.extracted_score = Some(extracted_score);

                if extracted_score == claimed_score {
                    validation.is_valid = true;
                    validation.confidence = 0.9;
                } else {
                    validation.add_error(format!(
                        "Score mismatch: demo shows {}-{} but claim says {}-{}",
                        extracted_score.0, extracted_score.1, claimed_score.0, claimed_score.1,
                    ));
                }

                // Check map name (if game result has map info)
                if let Some(gr) = game_result {
                    if metadata.map_name.to_lowercase() == gr.map_id.to_lowercase() {
                        validation.map_match = true;
                    } else {
                        validation.map_match = false;
                        validation.add_warning(
                            format!(
                                "Map mismatch: demo map '{}' vs claimed '{}'",
                                metadata.map_name, gr.map_id
                            ),
                            0.3,
                        );
                    }
                }
            } else {
                // No metadata — can't validate
                validation.add_warning("Demo has no parsed metadata".to_string(), 0.5);
                validation.claimed_score = (
                    claim.claimed_participant1_score,
                    claim.claimed_participant2_score,
                );
            }

            // Include outcomes with score/map issues OR an unrecognized (ringer)
            // player — the latter must raise the roster-mismatch review even when
            // the score matches perfectly.
            if !validation.errors.is_empty()
                || !validation.warnings.is_empty()
                || !unrecognized.is_empty()
            {
                outcomes.push(DemoValidationOutcome {
                    link_id: link.id,
                    validation,
                    unrecognized_players: unrecognized,
                });
            }
        }

        // Phase D enforcement: check the §0.4 lineup rules (majority + elo caps)
        // on the authoritative demo-derived lineup. Violations RAISE the review
        // (never auto-void). Attach to an existing outcome if any, else create a
        // synthetic one anchored to the first demo link so the review can fire.
        let lineup_violations = self.enforcer.lineup_violations(match_id).await?;
        if !lineup_violations.is_empty() {
            if let Some(first) = outcomes.first_mut() {
                first.unrecognized_players.extend(lineup_violations);
            } else if let Some(link) = demo_links.first() {
                outcomes.push(DemoValidationOutcome {
                    link_id: link.link.id,
                    validation: DemoValidationResult::default(),
                    unrecognized_players: lineup_violations,
                });
            }
        }

        Ok(outcomes)
    }
}

/// Diff a demo's players against recognised (registered) accounts.
///
/// A `DemoPlayer` whose `player_id` is `None` did not resolve to any
/// `players.steam_id_64` — i.e. it has no site account. Under §0.4 rule 1 that
/// is the ringer case: an outsider has no account. Each such player becomes an
/// [`UnrecognizedPlayer`], which the review flow turns into a `roster_mismatch`.
///
/// The team side is inferred from the demo's own team names (`team1`/`team2` in
/// the parsed metadata). When metadata is absent or the name doesn't match,
/// the player is attributed to team 1 — the review is raised regardless, and an
/// admin resolves the exact side.
fn collect_unrecognized_players(
    players: &[DemoPlayer],
    demo: &portal_domain::entities::demo::Demo,
) -> Vec<UnrecognizedPlayer> {
    let team_names = demo
        .metadata
        .as_ref()
        .map(|m| (m.team1_name.clone(), m.team2_name.clone()));
    unrecognized_from_players(players, team_names.as_ref())
}

/// Core of [`collect_unrecognized_players`], decoupled from the `Demo` shell so
/// it can be unit-tested. `team_names` are the demo's `(team1, team2)` names.
fn unrecognized_from_players(
    players: &[DemoPlayer],
    team_names: Option<&(String, String)>,
) -> Vec<UnrecognizedPlayer> {
    let (team1_name, team2_name) = team_names.cloned().unwrap_or_default();

    players
        .iter()
        .filter(|p| p.player_id.is_none())
        .map(|p| {
            // Infer the side from the demo's team name; default to team 1.
            let (team_side, registration_side) = match p.team_name.as_deref() {
                Some(name) if !team2_name.is_empty() && name == team2_name => (TeamSide::Team2, 2),
                Some(name) if !team1_name.is_empty() && name == team1_name => (TeamSide::Team1, 1),
                _ => (TeamSide::Team1, 1),
            };
            UnrecognizedPlayer {
                steam_id: p.steam_id.clone(),
                player_name: p.player_name.clone(),
                team_side,
                registration_side,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use portal_core::{DemoId, DemoPlayerId, PlayerId};
    use portal_domain::entities::demo::DemoPlayerStats;

    fn demo_player(steam_id: &str, team: &str, player_id: Option<PlayerId>) -> DemoPlayer {
        DemoPlayer {
            id: DemoPlayerId::new(),
            demo_id: DemoId::new(),
            steam_id: steam_id.to_string(),
            player_name: format!("name_{steam_id}"),
            team_name: Some(team.to_string()),
            player_id,
            stats: DemoPlayerStats::default(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn only_unregistered_players_are_flagged_as_ringers() {
        let names = ("Alpha".to_string(), "Bravo".to_string());
        let players = vec![
            // Registered (has an account) -> recognised, not a ringer.
            demo_player("111", "Alpha", Some(PlayerId::new())),
            // Unregistered on team1 -> ringer, side 1.
            demo_player("222", "Alpha", None),
            // Unregistered on team2 -> ringer, side 2.
            demo_player("333", "Bravo", None),
        ];

        let out = unrecognized_from_players(&players, Some(&names));
        assert_eq!(out.len(), 2, "only the two accountless players are ringers");

        let s2 = out.iter().find(|u| u.steam_id == "222").unwrap();
        assert_eq!(s2.registration_side, 1);
        assert_eq!(s2.team_side, TeamSide::Team1);

        let s3 = out.iter().find(|u| u.steam_id == "333").unwrap();
        assert_eq!(s3.registration_side, 2);
        assert_eq!(s3.team_side, TeamSide::Team2);
    }

    #[test]
    fn all_recognised_players_yield_no_ringers() {
        let players = vec![
            demo_player("1", "Alpha", Some(PlayerId::new())),
            demo_player("2", "Bravo", Some(PlayerId::new())),
        ];
        assert!(unrecognized_from_players(&players, None).is_empty());
    }
}
