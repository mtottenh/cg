//! Bracket progression service.
//!
//! Handles winner advancement, loser routing, and bracket completion detection
//! after match results are finalized.

use std::sync::Arc;

use async_trait::async_trait;
use portal_core::types::{
    BracketType, MatchFormat, MatchFormatPlan, MatchParticipantSource, StageFormat, StageStatus,
    TournamentMatchStatus,
};
use portal_core::{
    DomainError, TournamentBracketId, TournamentId, TournamentMatchId, TournamentRegistrationId,
    TournamentStageId,
};
use tracing::{info, instrument};

use crate::entities::tournament::{
    SeededParticipant, TournamentBracket, TournamentMatch, TournamentStanding,
};
use crate::repositories::tournament::{
    CreateTournamentBracket, ParticipantSlot, TournamentBracketRepository,
    TournamentMatchRepository, TournamentRegistrationRepository, TournamentStageRepository,
    TournamentStandingsRepository,
};

use super::bracket_generator::groups;
use super::bracket_generator::{BracketGenerator, CrossLinkType};
use super::helpers;
use super::match_completion::StageAdvancer;

/// Result of processing match progression.
#[derive(Debug, Clone)]
pub struct ProgressionResult {
    /// The match that completed
    pub match_id: TournamentMatchId,
    /// Winner advancement info
    pub winner_advancement: Option<Advancement>,
    /// Loser routing info
    pub loser_result: LoserResult,
    /// Updated standings (for round robin/swiss)
    pub updated_standings: Vec<TournamentStanding>,
    /// Matches that are now ready to start
    pub newly_ready_matches: Vec<TournamentMatchId>,
    /// Whether the bracket is now complete
    pub bracket_complete: bool,
    /// Whether the tournament is now complete
    pub tournament_complete: bool,
    /// Whether a stage transition was triggered (e.g., groups → playoffs)
    pub stage_advanced: bool,
}

/// Winner advancement information.
#[derive(Debug, Clone)]
pub struct Advancement {
    /// The target match
    pub target_match_id: TournamentMatchId,
    /// Which position in the target match (1 or 2)
    pub target_position: i32,
}

/// Result of loser routing.
#[derive(Debug, Clone)]
pub enum LoserResult {
    /// Loser is eliminated
    Eliminated,
    /// Loser drops to another bracket (double elim)
    DropsTo {
        target_match_id: TournamentMatchId,
        target_position: i32,
    },
    /// No loser routing (round robin, etc.)
    NotApplicable,
}

/// What reverting a match's progression has to undo, per bracket family.
///
/// P-169. The mapping below is an exhaustive `match` **on purpose**: a new
/// `BracketType` variant must fail to compile here rather than fall through a
/// catch-all arm into a `Ok(())` that reports success and does nothing. A
/// silent success on an unhandled format is the exact defect this type exists
/// to make impossible — a test can only ever cover the variants that existed
/// when it was written, the compiler covers the ones added afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevertEffect {
    /// Standings-derived formats. `round_robin.rs` and `swiss.rs` create every
    /// match with `winner_progresses_to: None` / `loser_progresses_to: None`,
    /// so a result leaves no trace in any other match — only in the standings,
    /// which are derived from the completed rows. Clearing the recorded result
    /// and recomputing is therefore the whole of the rollback.
    ClearRecordedResult,
    /// Link-following formats. A result seats a participant in the match the
    /// progression links point at, so the rollback is to take them back out of
    /// that slot.
    WithdrawFromDownstream,
}

/// Classify a bracket type. Free function, not a method, so the exhaustiveness
/// can be unit-tested without standing up a repository stack.
const fn revert_effect(bracket_type: BracketType) -> RevertEffect {
    match bracket_type {
        BracketType::RoundRobin | BracketType::Swiss => RevertEffect::ClearRecordedResult,
        BracketType::SingleElim
        | BracketType::Winners
        | BracketType::Losers
        | BracketType::GrandFinal => RevertEffect::WithdrawFromDownstream,
    }
}

/// Which of a match's two outputs fed a downstream slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgressionRole {
    /// Followed `winner_progresses_to`; the target slot is a `WinnerOf(..)`.
    Winner,
    /// Followed `loser_progresses_to` (double elimination); the target slot is
    /// a `LoserOf(..)`.
    Loser,
}

impl ProgressionRole {
    /// Whether `source` is the wiring that says "this slot is filled by the
    /// {winner,loser} of the match at `position`".
    fn fills_from(self, source: &MatchParticipantSource, position: &str) -> bool {
        match (self, source) {
            (Self::Winner, MatchParticipantSource::WinnerOf(pos))
            | (Self::Loser, MatchParticipantSource::LoserOf(pos)) => pos == position,
            _ => false,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Winner => "winner",
            Self::Loser => "loser",
        }
    }
}

/// Statuses a downstream match may be in and still have a participant pulled
/// back out of it: nothing has happened there yet beyond the pairing itself.
///
/// Anything else — it is being played, has been played, forfeited, disputed or
/// cancelled — means the round has moved on, and a revert that reached into it
/// would be destroying real recorded play. Those refuse instead; see
/// `ensure_target_has_not_moved_on`.
const fn target_is_still_untouched(status: TournamentMatchStatus) -> bool {
    matches!(
        status,
        TournamentMatchStatus::Pending
            | TournamentMatchStatus::Ready
            | TournamentMatchStatus::Scheduled
    )
}

/// Service for handling bracket progression.
#[derive(Clone)]
pub struct ProgressionService<TMR, TBR, TSR, TRR, TSTR> {
    match_repo: Arc<TMR>,
    bracket_repo: Arc<TBR>,
    stage_repo: Arc<TSR>,
    registration_repo: Arc<TRR>,
    standing_repo: Arc<TSTR>,
}

impl<TMR, TBR, TSR, TRR, TSTR> ProgressionService<TMR, TBR, TSR, TRR, TSTR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TSR: TournamentStageRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingsRepository,
{
    /// Create a new progression service.
    pub fn new(
        match_repo: Arc<TMR>,
        bracket_repo: Arc<TBR>,
        stage_repo: Arc<TSR>,
        registration_repo: Arc<TRR>,
        standing_repo: Arc<TSTR>,
    ) -> Self {
        Self {
            match_repo,
            bracket_repo,
            stage_repo,
            registration_repo,
            standing_repo,
        }
    }

    /// Process match completion and handle all progression.
    #[instrument(skip(self))]
    pub async fn process_match_completion(
        &self,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        loser_registration_id: TournamentRegistrationId,
    ) -> Result<ProgressionResult, DomainError> {
        let match_ = self.get_match(match_id).await?;
        let bracket = self.get_bracket(match_.bracket_id).await?;

        // Record who won BEFORE anything derives from it.
        //
        // This used to live inside the round-robin/swiss standings branch, so
        // on an elimination bracket `reapply_progression` moved the bracket to
        // the new winner and left `winner_registration_id` naming the old one.
        // The match then disagreed with the pairing it had produced — and a
        // later revert, which reads that column, would go looking for a
        // participant who was never advanced (P-169). Attribution is a property
        // of the match, not of the bracket family, so it is written for every
        // format.
        self.record_outcome(&match_, winner_registration_id, loser_registration_id)
            .await?;

        // Advance winner
        let winner_advancement = self.advance_winner(&match_, winner_registration_id).await?;

        // Route loser
        let loser_result = self
            .route_loser(&match_, &bracket, loser_registration_id)
            .await?;

        // Update standings (for round robin/swiss)
        let updated_standings = if matches!(
            bracket.bracket_type,
            BracketType::RoundRobin | BracketType::Swiss
        ) {
            self.standing_repo.recompute_bracket(bracket.id).await?
        } else {
            Vec::new()
        };

        // Find newly ready matches
        let newly_ready_matches = self.find_newly_ready_matches(match_.bracket_id).await?;

        // Check completion
        let bracket_complete = self.check_bracket_completion(match_.bracket_id).await?;

        // Check for stage advancement (groups → playoffs)
        let stage_advanced = self
            .advance_stage_if_complete(match_.bracket_id)
            .await?
            .is_some();

        let tournament_complete = if bracket_complete {
            self.check_tournament_completion(match_.tournament_id)
                .await?
        } else {
            false
        };

        info!(
            match_id = %match_id,
            winner = %winner_registration_id,
            bracket_complete = bracket_complete,
            stage_advanced = stage_advanced,
            tournament_complete = tournament_complete,
            "Processed match completion"
        );

        Ok(ProgressionResult {
            match_id,
            winner_advancement,
            loser_result,
            updated_standings,
            newly_ready_matches,
            bracket_complete,
            tournament_complete,
            stage_advanced,
        })
    }

    /// Advance the winner to their next match.
    #[instrument(skip(self))]
    pub async fn advance_winner(
        &self,
        source_match: &TournamentMatch,
        winner_registration_id: TournamentRegistrationId,
    ) -> Result<Option<Advancement>, DomainError> {
        let Some(target_match_id) = source_match.winner_progresses_to else {
            // No next match - this was a final or there's no progression
            return Ok(None);
        };

        let target_match = self.get_match(target_match_id).await?;

        // Determine which position to fill
        let target_position = self.determine_target_position(source_match, &target_match, true)?;

        // Get registration info for denormalization
        let registration = self
            .registration_repo
            .find_by_id(winner_registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::Internal(format!("Registration {winner_registration_id} not found"))
            })?;

        // Use assign_participant to update target match
        let slot = if target_position == 1 {
            ParticipantSlot::One
        } else {
            ParticipantSlot::Two
        };

        self.match_repo
            .assign_participant(
                target_match_id,
                slot,
                winner_registration_id,
                registration.participant_name.clone(),
                registration.participant_logo_url.clone(),
                registration.seed,
            )
            .await?;

        info!(
            source_match = %source_match.id,
            target_match = %target_match_id,
            winner = %winner_registration_id,
            position = target_position,
            "Advanced winner"
        );

        Ok(Some(Advancement {
            target_match_id,
            target_position,
        }))
    }

    /// Route the loser to their destination.
    #[instrument(skip(self))]
    pub async fn route_loser(
        &self,
        source_match: &TournamentMatch,
        bracket: &TournamentBracket,
        loser_registration_id: TournamentRegistrationId,
    ) -> Result<LoserResult, DomainError> {
        // For round robin/swiss, losers aren't routed anywhere
        if matches!(
            bracket.bracket_type,
            BracketType::RoundRobin | BracketType::Swiss
        ) {
            return Ok(LoserResult::NotApplicable);
        }

        // Check if loser progresses somewhere (double elim)
        let Some(target_match_id) = source_match.loser_progresses_to else {
            return Ok(LoserResult::Eliminated);
        };

        let target_match = self.get_match(target_match_id).await?;

        // Determine position
        let target_position = self.determine_target_position(source_match, &target_match, false)?;

        // Get registration info
        let registration = self
            .registration_repo
            .find_by_id(loser_registration_id)
            .await?
            .ok_or_else(|| {
                DomainError::Internal(format!("Registration {loser_registration_id} not found"))
            })?;

        // Use assign_participant to update target match
        let slot = if target_position == 1 {
            ParticipantSlot::One
        } else {
            ParticipantSlot::Two
        };

        self.match_repo
            .assign_participant(
                target_match_id,
                slot,
                loser_registration_id,
                registration.participant_name.clone(),
                registration.participant_logo_url.clone(),
                registration.seed,
            )
            .await?;

        info!(
            source_match = %source_match.id,
            target_match = %target_match_id,
            loser = %loser_registration_id,
            position = target_position,
            "Routed loser"
        );

        Ok(LoserResult::DropsTo {
            target_match_id,
            target_position,
        })
    }

    /// Make the match row record this winner/loser, so everything derived from
    /// it — standings, the bracket's own revert — agrees with the progression
    /// about to be applied.
    ///
    /// Standings used to be delta-accumulated (`update_after_match` twice, then
    /// re-rank), which meant a retried or double-delivered admin
    /// `progression/process` counted the same match again. They are now
    /// *derived* from the completed match rows, which makes this write the one
    /// that matters: the row is the source of truth, and on the reapply path it
    /// carries the overturned result. Running it any number of times converges.
    ///
    /// Scores stay as recorded; only the attribution changes. That matches the
    /// old delta behaviour, which read the winner's games out of whichever
    /// participant slot they occupied. Correcting the *score* is
    /// `ResultService::override_result`'s job, and it is audited.
    async fn record_outcome(
        &self,
        match_: &TournamentMatch,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<(), DomainError> {
        if match_.winner_registration_id == Some(winner_id)
            && match_.loser_registration_id == Some(loser_id)
        {
            return Ok(());
        }

        self.match_repo
            .submit_result(
                match_.id,
                match_.participant1_score,
                match_.participant2_score,
                winner_id,
                loser_id,
            )
            .await?;

        Ok(())
    }

    /// Find matches that are now ready to start.
    #[instrument(skip(self))]
    pub async fn find_newly_ready_matches(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatchId>, DomainError> {
        let matches = self.match_repo.list_by_bracket(bracket_id).await?;

        let ready: Vec<_> = matches
            .into_iter()
            .filter(|m| {
                m.status == TournamentMatchStatus::Pending
                    && m.participant1_registration_id.is_some()
                    && m.participant2_registration_id.is_some()
            })
            .map(|m| m.id)
            .collect();

        // Atomic: every match in the set flips Pending → Ready in one
        // statement, or none do. Previously this loop updated matches
        // one at a time; a mid-loop failure left the bracket with some
        // Ready and the rest Pending, and the UI showed half-enabled
        // controls. See audit I5.
        if !ready.is_empty() {
            self.match_repo.mark_pending_as_ready_bulk(&ready).await?;
        }

        Ok(ready)
    }

    /// Check if a bracket is complete.
    #[instrument(skip(self))]
    pub async fn check_bracket_completion(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<bool, DomainError> {
        let matches = self.match_repo.list_by_bracket(bracket_id).await?;

        // Bracket is complete if all matches are completed or cancelled
        let all_done = matches.iter().all(|m| {
            matches!(
                m.status,
                TournamentMatchStatus::Completed | TournamentMatchStatus::Cancelled
            )
        });

        Ok(all_done)
    }

    /// Check if a tournament is complete.
    #[instrument(skip(self))]
    pub async fn check_tournament_completion(
        &self,
        tournament_id: TournamentId,
    ) -> Result<bool, DomainError> {
        let stages = self.stage_repo.list_by_tournament(tournament_id).await?;

        // Tournament is complete if all stages are complete
        for stage in stages {
            let brackets = self.bracket_repo.list_by_stage(stage.id).await?;
            for bracket in brackets {
                if !self.check_bracket_completion(bracket.id).await? {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Check if all brackets in a stage are complete.
    async fn check_stage_completion(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<bool, DomainError> {
        let brackets = self.bracket_repo.list_by_stage(stage_id).await?;
        for bracket in &brackets {
            if !self.check_bracket_completion(bracket.id).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Seed the next stage once the last group result is in.
    ///
    /// If `bracket_id` belongs to a group stage whose brackets have all
    /// finished, the following stage is generated from the group standings
    /// and the two stage statuses flip. Returns the stage that was opened.
    ///
    /// Safe to call after every result and to re-run: an unfinished bracket,
    /// a non-group stage, a group stage with nothing after it (a plain round
    /// robin), or a stage already marked `Completed` (a saga re-drive after
    /// the playoffs were seeded) all return `None` without touching anything.
    #[instrument(skip(self))]
    pub async fn advance_stage_if_complete(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Option<TournamentStageId>, DomainError> {
        if !self.check_bracket_completion(bracket_id).await? {
            return Ok(None);
        }
        let bracket = self.get_bracket(bracket_id).await?;
        let stage = self
            .stage_repo
            .find_by_id(bracket.stage_id)
            .await?
            .ok_or_else(|| {
                DomainError::Internal(format!("Stage {} not found", bracket.stage_id))
            })?;
        if stage.format != StageFormat::GroupStage || stage.status == StageStatus::Completed {
            return Ok(None);
        }
        if self
            .stage_repo
            .find_next_stage(bracket.tournament_id, stage.stage_order)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        if !self.check_stage_completion(stage.id).await? {
            return Ok(None);
        }
        let next_stage_id = self
            .advance_to_next_stage(bracket.tournament_id, &stage)
            .await?;
        Ok(Some(next_stage_id))
    }

    /// Advance from a completed group stage to the playoff stage.
    ///
    /// Reads standings from each group, cross-seeds for playoffs, and generates
    /// the playoff bracket (SE or DE).
    async fn advance_to_next_stage(
        &self,
        tournament_id: TournamentId,
        completed_stage: &crate::entities::tournament::TournamentStage,
    ) -> Result<TournamentStageId, DomainError> {
        // Find the next stage
        let next_stage = self
            .stage_repo
            .find_next_stage(tournament_id, completed_stage.stage_order)
            .await?
            .ok_or_else(|| {
                DomainError::InvalidState("No next stage found for advancement".to_string())
            })?;

        // Get all group brackets ordered by group_number
        let mut group_brackets = self.bracket_repo.list_by_stage(completed_stage.id).await?;
        group_brackets.sort_by_key(|b| b.group_number.unwrap_or(0));

        let advance_per_group = completed_stage.advancement_count.unwrap_or(2) as usize;

        // Get standings from each group and map back to SeededParticipant
        let mut group_standings: Vec<Vec<SeededParticipant>> = Vec::new();
        for bracket in &group_brackets {
            let standings = self.standing_repo.list_by_bracket(bracket.id).await?;

            let mut group_top: Vec<SeededParticipant> = Vec::new();
            for standing in &standings {
                let reg = self
                    .registration_repo
                    .find_by_id(standing.registration_id)
                    .await?
                    .ok_or_else(|| {
                        DomainError::Internal(format!(
                            "Registration {} not found",
                            standing.registration_id
                        ))
                    })?;
                group_top.push(SeededParticipant {
                    registration_id: reg.id,
                    seed: standing.position,
                    participant_name: reg.participant_name,
                    participant_logo_url: reg.participant_logo_url,
                });
            }

            group_standings.push(group_top);
        }

        // Cross-seed for playoffs
        let playoff_participants =
            groups::cross_seed_for_playoffs(group_standings, advance_per_group);

        let num_playoff_participants = playoff_participants.len();

        if num_playoff_participants < 2 {
            return Err(DomainError::InvalidState(
                "Not enough participants advanced to playoffs".to_string(),
            ));
        }

        // Build the playoff stage's format plan. Every stage-creation path
        // persists a concrete match_format (the groups flow resolves the
        // per-phase override against the tournament default at start time),
        // so the Bo3 fallback only covers hand-created stages that never set
        // one. Final/grand-final overrides ride in the stage's
        // format_settings.
        let format_plan = MatchFormatPlan::from_settings(
            next_stage.match_format.unwrap_or(MatchFormat::Bo3),
            Some(&next_stage.format_settings),
        )
        .map_err(|e| DomainError::InvalidState(format!("invalid stage format settings: {e}")))?;

        // Generate playoff brackets based on stage format
        match next_stage.format {
            StageFormat::SingleElimination => {
                self.generate_se_playoff(
                    tournament_id,
                    next_stage.id,
                    playoff_participants,
                    &format_plan,
                )
                .await?;
            }
            StageFormat::DoubleElimination => {
                self.generate_de_playoff(
                    tournament_id,
                    next_stage.id,
                    playoff_participants,
                    &format_plan,
                )
                .await?;
            }
            _ => {
                return Err(DomainError::InvalidState(format!(
                    "Unsupported playoff format: {}",
                    next_stage.format
                )));
            }
        }

        // Atomic status flip: completed group stage → Completed and
        // next playoff stage → Active in one transaction. Previously
        // the two UPDATEs ran sequentially; a failure between them
        // left the tournament with a finished group stage but no
        // active playoff stage — the tournament would appear stalled
        // and would need manual DB intervention to resume. See audit
        // I5. (The bracket-generation writes above run before this
        // point, so a failure there cleanly aborts without mutating
        // stage status.)
        self.stage_repo
            .transition_stages(
                completed_stage.id,
                StageStatus::Completed,
                next_stage.id,
                StageStatus::Active,
            )
            .await?;

        info!(
            tournament_id = %tournament_id,
            from_stage = %completed_stage.id,
            to_stage = %next_stage.id,
            participants = num_playoff_participants,
            "Advanced from group stage to playoffs"
        );

        Ok(next_stage.id)
    }

    /// Generate a Single Elimination playoff bracket.
    async fn generate_se_playoff(
        &self,
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        participants: Vec<SeededParticipant>,
        format_plan: &MatchFormatPlan,
    ) -> Result<(), DomainError> {
        let bracket = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id,
                tournament_id,
                name: "Playoffs".to_string(),
                bracket_type: BracketType::SingleElim,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let generated = BracketGenerator::single_elimination(
            tournament_id,
            stage_id,
            bracket.id,
            participants,
            format_plan,
        )?;

        self.bracket_repo
            .update(
                bracket.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(generated.total_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        let matches = self.match_repo.bulk_create(generated.matches).await?;
        let position_to_match = helpers::build_position_map(&matches);

        helpers::apply_initial_assignments(
            self.match_repo.as_ref(),
            &generated.initial_assignments,
            &position_to_match,
        )
        .await?;

        helpers::apply_byes(
            self.match_repo.as_ref(),
            &generated.byes,
            &position_to_match,
        )
        .await?;

        helpers::set_se_progression_links(self.match_repo.as_ref(), &matches, &position_to_match)
            .await?;

        // Mark newly ready matches
        self.find_newly_ready_matches(bracket.id).await?;

        Ok(())
    }

    /// Generate a Double Elimination playoff bracket.
    async fn generate_de_playoff(
        &self,
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        participants: Vec<SeededParticipant>,
        format_plan: &MatchFormatPlan,
    ) -> Result<(), DomainError> {
        let wb = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id,
                tournament_id,
                name: "Winners Bracket".to_string(),
                bracket_type: BracketType::Winners,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let lb = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id,
                tournament_id,
                name: "Losers Bracket".to_string(),
                bracket_type: BracketType::Losers,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let gf = self
            .bracket_repo
            .create(CreateTournamentBracket {
                stage_id,
                tournament_id,
                name: "Grand Final".to_string(),
                bracket_type: BracketType::GrandFinal,
                total_rounds: 0,
                group_number: None,
            })
            .await?;

        let generated = BracketGenerator::double_elimination(
            tournament_id,
            stage_id,
            wb.id,
            lb.id,
            gf.id,
            participants,
            format_plan,
        )?;

        // Update bracket round counts
        self.bracket_repo
            .update(
                wb.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(generated.winners_bracket.total_rounds),
                    current_round: Some(1),
                },
            )
            .await?;

        if generated.losers_bracket.total_rounds > 0 {
            self.bracket_repo
                .update(
                    lb.id,
                    crate::repositories::tournament::UpdateTournamentBracket {
                        name: None,
                        total_rounds: Some(generated.losers_bracket.total_rounds),
                        current_round: Some(1),
                    },
                )
                .await?;
        }

        self.bracket_repo
            .update(
                gf.id,
                crate::repositories::tournament::UpdateTournamentBracket {
                    name: None,
                    total_rounds: Some(1),
                    current_round: Some(1),
                },
            )
            .await?;

        // Create matches for all 3 brackets
        let wb_matches = self
            .match_repo
            .bulk_create(generated.winners_bracket.matches)
            .await?;

        let lb_matches = if generated.losers_bracket.matches.is_empty() {
            Vec::new()
        } else {
            self.match_repo
                .bulk_create(generated.losers_bracket.matches)
                .await?
        };

        let gf_matches = self
            .match_repo
            .bulk_create(generated.grand_final.matches)
            .await?;

        // Build unified position → match ID map
        let mut position_to_match = helpers::build_position_map(&wb_matches);
        position_to_match.extend(helpers::build_position_map(&lb_matches));
        position_to_match.extend(helpers::build_position_map(&gf_matches));

        // Apply initial assignments and byes
        helpers::apply_initial_assignments(
            self.match_repo.as_ref(),
            &generated.winners_bracket.initial_assignments,
            &position_to_match,
        )
        .await?;

        helpers::apply_byes(
            self.match_repo.as_ref(),
            &generated.winners_bracket.byes,
            &position_to_match,
        )
        .await?;

        // Build progression links
        let mut progression: std::collections::HashMap<
            TournamentMatchId,
            (Option<TournamentMatchId>, Option<TournamentMatchId>),
        > = std::collections::HashMap::new();

        // WB intra-bracket
        for match_ in &wb_matches {
            let (round, match_in_round) =
                helpers::parse_round_match(&match_.bracket_position, "WR");
            if round == 0 {
                continue;
            }
            let next_pos = format!("WR{}M{}", round + 1, (match_in_round + 1) / 2);
            if let Some(&next_id) = position_to_match.get(&next_pos) {
                let entry = progression.entry(match_.id).or_insert((None, None));
                entry.0 = Some(next_id);
            }
        }

        // LB intra-bracket
        for match_ in &lb_matches {
            let (lb_round, match_in_round) =
                helpers::parse_round_match(&match_.bracket_position, "LR");
            if lb_round == 0 {
                continue;
            }
            let next_pos = if lb_round % 2 == 1 && lb_round > 1 {
                format!("LR{}M{match_in_round}", lb_round + 1)
            } else if lb_round == 1 {
                format!("LR2M{match_in_round}")
            } else {
                format!("LR{}M{}", lb_round + 1, (match_in_round + 1) / 2)
            };
            if let Some(&next_id) = position_to_match.get(&next_pos) {
                let entry = progression.entry(match_.id).or_insert((None, None));
                entry.0 = Some(next_id);
            }
        }

        // Cross-bracket links
        for link in &generated.cross_bracket_links {
            let source_id = position_to_match.get(&link.source_bracket_position);
            let target_id = position_to_match.get(&link.target_bracket_position);
            if let (Some(&src), Some(&tgt)) = (source_id, target_id) {
                let entry = progression.entry(src).or_insert((None, None));
                match link.link_type {
                    CrossLinkType::LoserDropsTo => entry.1 = Some(tgt),
                    CrossLinkType::WinnerAdvancesTo => entry.0 = Some(tgt),
                }
            }
        }

        // Write all progression links
        for (match_id, (winner_to, loser_to)) in &progression {
            self.match_repo
                .set_progression_links(*match_id, *winner_to, *loser_to)
                .await?;
        }

        // Mark newly ready matches in WB
        self.find_newly_ready_matches(wb.id).await?;

        Ok(())
    }

    /// Revert progression for a match (used when a result is overturned).
    ///
    /// What "revert" means depends on the bracket family, and the two cases are
    /// selected by an exhaustive [`revert_effect`] match — no catch-all arm can
    /// return success without doing work.
    ///
    /// **Standings-derived formats (round robin, swiss).** Standings are
    /// *derived* from the completed match rows that carry a winner, so
    /// reverting means removing this match from that source — clearing
    /// `winner_registration_id`/`loser_registration_id` — and recomputing.
    /// Subtracting deltas (the original behaviour) was not idempotent: because
    /// the winner was never cleared, a second revert re-derived the same deltas
    /// and drove the standings negative. Clearing is a no-op on an
    /// already-reverted match and the recompute is a pure function of what is
    /// left, so reverting N times lands where reverting once does.
    ///
    /// **Link-following formats (single elim, winners, losers, grand final).**
    /// The participant this match seated downstream is taken back out of that
    /// slot, and the target drops back to `Pending` if it is no longer a full
    /// pairing — its pre-progression state.
    ///
    /// P-83 implemented the elimination case, which had logged "needs
    /// implementation" and returned `Ok(())` while the confirm dialog promised
    /// that "downstream pairings created from it are rolled back".
    ///
    /// P-169 fixes what P-83's identity-based lookup still could not do. It
    /// withdrew `match_.winner_registration_id` — *the currently recorded
    /// winner* — from the downstream slot. Since P-72 an admin can correct a
    /// confirmed-but-wrong score, and a correction flips that column while
    /// deliberately leaving the bracket alone. Revert then went looking for the
    /// **new** winner, who had never been advanced, found them nowhere, and
    /// returned success having left the **old** winner sitting in the next
    /// round: the precise no-op P-83 was supposed to have eliminated, on the
    /// precise flow P-72 created. Withdrawal is now resolved *structurally* —
    /// the slot a target match declares it draws from this one
    /// (`WinnerOf(bracket_position)` / `LoserOf(..)`) is by construction the
    /// slot this match filled, whoever currently sits in it.
    #[instrument(skip(self))]
    pub async fn revert_progression(&self, match_id: TournamentMatchId) -> Result<(), DomainError> {
        let match_ = self.get_match(match_id).await?;
        let bracket = self.get_bracket(match_.bracket_id).await?;

        match revert_effect(bracket.bracket_type) {
            RevertEffect::ClearRecordedResult => {
                if match_.winner_registration_id.is_some() || match_.loser_registration_id.is_some()
                {
                    self.match_repo.clear_result(match_id).await?;
                    info!(
                        match_id = %match_id,
                        "Cleared recorded result so standings no longer derive it"
                    );
                }

                // Always recompute — even when there was nothing to clear —
                // so a revert converges the bracket regardless of how it got
                // into its current state.
                self.standing_repo.recompute_bracket(bracket.id).await?;
            }
            RevertEffect::WithdrawFromDownstream => {
                let winner_withdrawn = self
                    .withdraw_from_target(
                        &match_,
                        match_.winner_progresses_to,
                        ProgressionRole::Winner,
                    )
                    .await?;
                let loser_withdrawn = self
                    .withdraw_from_target(
                        &match_,
                        match_.loser_progresses_to,
                        ProgressionRole::Loser,
                    )
                    .await?;

                info!(
                    match_id = %match_id,
                    bracket_type = %bracket.bracket_type,
                    winner_withdrawn,
                    loser_withdrawn,
                    "Withdrew this match's advancements from the downstream matches"
                );
            }
        }

        info!(match_id = %match_id, "Reverted progression");

        Ok(())
    }

    /// Take whoever this match sent into `target_match_id` back out of the slot
    /// it put them in. Returns whether a slot was actually cleared.
    ///
    /// The slot is resolved from the bracket *wiring* rather than from the
    /// occupant's identity: a slot whose `participantN_source` is
    /// `WinnerOf(<this match's position>)` can only ever have been filled by
    /// this match, so it is the right slot to empty even when the recorded
    /// winner has since changed under it (P-169 — see `revert_progression`).
    /// Identity matching is kept only as a fallback for brackets with no source
    /// wiring, so hand-built or legacy brackets still roll back.
    ///
    /// Refuses, loudly, in the two cases where clearing would corrupt rather
    /// than repair: when the occupant did not come from this match at all, and
    /// when the downstream round has already moved on (see
    /// `ensure_target_has_not_moved_on`).
    async fn withdraw_from_target(
        &self,
        source_match: &TournamentMatch,
        target_match_id: Option<TournamentMatchId>,
        role: ProgressionRole,
    ) -> Result<bool, DomainError> {
        // No link: this was a final, or the loser is eliminated. Nothing was
        // ever seated downstream, so there is nothing to take back.
        let Some(target_match_id) = target_match_id else {
            return Ok(false);
        };

        // Whoever this match advanced must be one of its own participants.
        let candidates: Vec<TournamentRegistrationId> = [
            source_match.participant1_registration_id,
            source_match.participant2_registration_id,
        ]
        .into_iter()
        .flatten()
        .collect();

        if candidates.is_empty() {
            // A bye match: the walkover was seeded straight into the next round
            // by `apply_byes`, never advanced out of here.
            info!(
                source_match = %source_match.id,
                target_match = %target_match_id,
                "Revert: source match has no participants, so it advanced nobody"
            );
            return Ok(false);
        }

        let target = self.get_match(target_match_id).await?;

        let slots = [
            (
                ParticipantSlot::One,
                target.participant1_source.as_ref(),
                target.participant1_registration_id,
            ),
            (
                ParticipantSlot::Two,
                target.participant2_source.as_ref(),
                target.participant2_registration_id,
            ),
        ];

        // 1. Structural: the slot wired to draw from this match.
        // 2. Fallback: the slot held by one of this match's participants.
        let resolved = slots
            .iter()
            .find(|(_, source, _)| {
                source.is_some_and(|s| role.fills_from(s, &source_match.bracket_position))
            })
            .or_else(|| {
                slots
                    .iter()
                    .find(|(_, _, occupant)| occupant.is_some_and(|id| candidates.contains(&id)))
            });

        let Some(&(slot, _, occupant)) = resolved else {
            info!(
                source_match = %source_match.id,
                target_match = %target_match_id,
                role = role.label(),
                "Revert: no downstream slot is fed by this match; nothing to withdraw"
            );
            return Ok(false);
        };

        // Already empty — a repeat revert, or progression never ran. Converges
        // rather than erroring, which is what makes revert re-runnable.
        let Some(occupant) = occupant else {
            info!(
                source_match = %source_match.id,
                target_match = %target_match_id,
                role = role.label(),
                "Revert: downstream slot is already empty"
            );
            return Ok(false);
        };

        // The wired slot is held by someone who cannot have come from here.
        // That is a corrupted bracket, not a revertable one: evicting them
        // would compound the damage, and returning Ok would hide it.
        if !candidates.contains(&occupant) {
            return Err(DomainError::Conflict(format!(
                "Cannot revert {}: the {} slot of {} is held by registration {}, who did not \
                 play in {}. The bracket is inconsistent — refusing to evict a participant this \
                 match did not put there.",
                source_match.bracket_position,
                role.label(),
                target.bracket_position,
                occupant,
                source_match.bracket_position,
            )));
        }

        self.ensure_target_has_not_moved_on(source_match, &target)
            .await?;

        self.match_repo
            .clear_participant(target_match_id, slot)
            .await?;

        // Restore the target's *pre-progression* state, not just its roster.
        // A match that went Pending → Ready when this participant arrived has
        // to go back to Pending, or the bracket keeps offering a half-empty
        // pairing as playable — and `find_newly_ready_matches` only ever
        // promotes Pending, so it would never be re-marked when the slot is
        // legitimately refilled.
        if target_is_still_untouched(target.status)
            && target.status != TournamentMatchStatus::Pending
        {
            self.match_repo
                .update_status(target_match_id, TournamentMatchStatus::Pending)
                .await?;
        }

        info!(
            source_match = %source_match.id,
            target_match = %target_match_id,
            registration = %occupant,
            role = role.label(),
            "Reverted advancement out of the downstream match"
        );

        Ok(true)
    }

    /// Refuse to reach into a downstream match that has already moved on.
    ///
    /// P-169. Reverting a leaf is undoing a pairing; reverting into a round
    /// that has been played is deleting a result somebody actually recorded,
    /// and whatever *that* match progressed in turn. The rule chosen here is
    /// **refuse with a reason, never cascade**: the alternative silently
    /// discards real play with no audit row and no undo, and a wrong refusal
    /// costs an admin one extra click (revert the deeper match first) while a
    /// wrong cascade costs a tournament its record. The refusal names the
    /// blocking match so the operator knows exactly where to start.
    async fn ensure_target_has_not_moved_on(
        &self,
        source_match: &TournamentMatch,
        target: &TournamentMatch,
    ) -> Result<(), DomainError> {
        let blocker = if target.winner_registration_id.is_some() {
            Some("has a recorded result".to_string())
        } else if !target_is_still_untouched(target.status) {
            Some(format!("is {}", target.status))
        } else if let Some(deeper_id) = target.winner_progresses_to {
            // Defence in depth: a target with no recorded winner whose own
            // downstream slot is already occupied by one of its participants
            // has been progressed by hand. Undoing this pairing would strand
            // that deeper advancement.
            let deeper = self.get_match(deeper_id).await?;
            let target_participants = [
                target.participant1_registration_id,
                target.participant2_registration_id,
            ];
            let advanced = [
                deeper.participant1_registration_id,
                deeper.participant2_registration_id,
            ]
            .into_iter()
            .flatten()
            .any(|id| target_participants.contains(&Some(id)));

            advanced.then(|| {
                format!(
                    "has already advanced a participant into {}",
                    deeper.bracket_position
                )
            })
        } else {
            None
        };

        if let Some(what) = blocker {
            return Err(DomainError::Conflict(format!(
                "Cannot revert {}: the next-round match {} {}. Revert {} first, then retry — \
                 rolling back into a round that has already been played would silently discard \
                 that result.",
                source_match.bracket_position,
                target.bracket_position,
                what,
                target.bracket_position
            )));
        }

        Ok(())
    }

    /// Reapply progression with a different winner.
    ///
    /// Revert, then re-process. Because `revert_progression` now empties the
    /// slot this match fed (rather than assuming the recorded winner is still
    /// sitting in it) and `process_match_completion` records the new
    /// attribution for every bracket family, a reapply whose new winner routes
    /// to a *different* target slot no longer leaves the stale entry behind,
    /// and the match row no longer disagrees with the pairing it produced.
    ///
    /// It inherits the refusal too: if the next round has already been played,
    /// this returns the same conflict rather than rewriting under it.
    #[instrument(skip(self))]
    pub async fn reapply_progression(
        &self,
        match_id: TournamentMatchId,
        new_winner_registration_id: TournamentRegistrationId,
    ) -> Result<ProgressionResult, DomainError> {
        let match_ = self.get_match(match_id).await?;

        // Determine new loser
        let new_loser_registration_id =
            if match_.participant1_registration_id == Some(new_winner_registration_id) {
                match_.participant2_registration_id
            } else {
                match_.participant1_registration_id
            }
            .ok_or_else(|| DomainError::InvalidState("Cannot determine loser".to_string()))?;

        // First revert existing progression
        self.revert_progression(match_id).await?;

        // Then process with new winner
        self.process_match_completion(
            match_id,
            new_winner_registration_id,
            new_loser_registration_id,
        )
        .await
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    async fn get_match(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        self.match_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(id))
    }

    async fn get_bracket(&self, id: TournamentBracketId) -> Result<TournamentBracket, DomainError> {
        self.bracket_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Bracket {id} not found")))
    }

    fn determine_target_position(
        &self,
        source_match: &TournamentMatch,
        target_match: &TournamentMatch,
        is_winner: bool,
    ) -> Result<i32, DomainError> {
        use portal_core::types::MatchParticipantSource;

        // Check participant sources to determine correct position
        let source_position = &source_match.bracket_position;

        // Helper to extract position from source
        let matches_source = |source: &MatchParticipantSource| -> bool {
            match source {
                MatchParticipantSource::WinnerOf(pos) if is_winner => pos == source_position,
                MatchParticipantSource::LoserOf(pos) if !is_winner => pos == source_position,
                _ => false,
            }
        };

        // Check if position 1 expects from this match
        if let Some(ref source) = target_match.participant1_source
            && matches_source(source)
        {
            return Ok(1);
        }

        // Check if position 2 expects from this match
        if let Some(ref source) = target_match.participant2_source
            && matches_source(source)
        {
            return Ok(2);
        }

        // Fallback: use first empty position
        if target_match.participant1_registration_id.is_none() {
            Ok(1)
        } else if target_match.participant2_registration_id.is_none() {
            Ok(2)
        } else {
            Err(DomainError::InvalidState(
                "Target match already has both participants".to_string(),
            ))
        }
    }
}

/// The saga's view of the service: one call per completed match that seeds
/// the playoffs exactly when the last group result lands.
#[async_trait]
impl<TMR, TBR, TSR, TRR, TSTR> StageAdvancer for ProgressionService<TMR, TBR, TSR, TRR, TSTR>
where
    TMR: TournamentMatchRepository + 'static,
    TBR: TournamentBracketRepository + 'static,
    TSR: TournamentStageRepository + 'static,
    TRR: TournamentRegistrationRepository + 'static,
    TSTR: TournamentStandingsRepository + 'static,
{
    async fn advance_if_stage_complete(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Option<TournamentStageId>, DomainError> {
        self.advance_stage_if_complete(bracket_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P-169. Every bracket format must map to a revert that *does something*.
    ///
    /// The real guard is the exhaustive `match` in [`revert_effect`] — adding a
    /// `BracketType` variant without deciding what reverting means for it is a
    /// compile error, which no test can be. This pins the decisions themselves,
    /// so a future edit cannot quietly re-route a link-following format into
    /// the standings-only branch (where it would clear a result and leave the
    /// downstream pairing standing: a success that does nothing, which is the
    /// whole finding).
    #[test]
    fn test_every_bracket_format_has_a_revert_effect() {
        let all = [
            BracketType::Winners,
            BracketType::Losers,
            BracketType::SingleElim,
            BracketType::RoundRobin,
            BracketType::Swiss,
            BracketType::GrandFinal,
        ];

        for bracket_type in all {
            let effect = revert_effect(bracket_type);
            let expected = match bracket_type {
                // No progression links exist in these formats
                // (round_robin.rs / swiss.rs build every match with
                // `winner_progresses_to: None`), so the standings are the
                // only downstream trace.
                BracketType::RoundRobin | BracketType::Swiss => RevertEffect::ClearRecordedResult,
                // Double elimination is these three: a winners-bracket match
                // feeds both a later winners match and a losers match, and
                // both links are followed back out.
                _ => RevertEffect::WithdrawFromDownstream,
            };
            assert_eq!(
                effect, expected,
                "{bracket_type} must revert by {expected:?}, not {effect:?}"
            );
        }
    }

    /// The wiring test that makes the structural slot lookup trustworthy:
    /// a winner link matches only `WinnerOf`, a loser link only `LoserOf`, and
    /// both only for *this* match's bracket position.
    #[test]
    fn test_progression_role_only_matches_its_own_wiring() {
        let winner = ProgressionRole::Winner;
        let loser = ProgressionRole::Loser;

        let winner_of_r1m1 = MatchParticipantSource::WinnerOf("R1M1".to_string());
        let loser_of_r1m1 = MatchParticipantSource::LoserOf("R1M1".to_string());
        let winner_of_r1m2 = MatchParticipantSource::WinnerOf("R1M2".to_string());

        assert!(winner.fills_from(&winner_of_r1m1, "R1M1"));
        assert!(!winner.fills_from(&loser_of_r1m1, "R1M1"));
        assert!(!winner.fills_from(&winner_of_r1m2, "R1M1"));

        assert!(loser.fills_from(&loser_of_r1m1, "R1M1"));
        assert!(!loser.fills_from(&winner_of_r1m1, "R1M1"));

        assert!(!winner.fills_from(&MatchParticipantSource::Seed(1), "R1M1"));
        assert!(!loser.fills_from(&MatchParticipantSource::Bye, "R1M1"));
    }

    /// Only a downstream match that has not been touched may be edited by a
    /// revert; everything else refuses. Pinned per status so that adding one
    /// to `TournamentMatchStatus` and forgetting it here shows up as a match
    /// arm, not as a bracket quietly losing a played result.
    #[test]
    fn test_only_untouched_downstream_matches_may_be_edited() {
        for status in [
            TournamentMatchStatus::Pending,
            TournamentMatchStatus::Ready,
            TournamentMatchStatus::Scheduled,
        ] {
            assert!(
                target_is_still_untouched(status),
                "{status} is a pairing that has not been played yet"
            );
        }

        for status in [
            TournamentMatchStatus::CheckingIn,
            TournamentMatchStatus::PickBan,
            TournamentMatchStatus::InProgress,
            TournamentMatchStatus::AwaitingResult,
            TournamentMatchStatus::Completed,
            TournamentMatchStatus::Cancelled,
            TournamentMatchStatus::Forfeit,
            TournamentMatchStatus::Disputed,
        ] {
            assert!(
                !target_is_still_untouched(status),
                "{status} means the next round has moved on; revert must refuse"
            );
        }
    }

    #[test]
    fn test_advancement_creation() {
        let adv = Advancement {
            target_match_id: TournamentMatchId::new(),
            target_position: 1,
        };

        assert_eq!(adv.target_position, 1);
    }

    #[test]
    fn test_loser_result_eliminated() {
        let result = LoserResult::Eliminated;
        assert!(matches!(result, LoserResult::Eliminated));
    }

    #[test]
    fn test_loser_result_drops_to() {
        let target_id = TournamentMatchId::new();
        let result = LoserResult::DropsTo {
            target_match_id: target_id,
            target_position: 2,
        };

        if let LoserResult::DropsTo {
            target_match_id,
            target_position,
        } = result
        {
            assert_eq!(target_match_id, target_id);
            assert_eq!(target_position, 2);
        } else {
            panic!("Expected DropsTo variant");
        }
    }

    #[test]
    fn test_loser_result_not_applicable() {
        let result = LoserResult::NotApplicable;
        assert!(matches!(result, LoserResult::NotApplicable));
    }

    #[test]
    fn test_progression_result_creation() {
        let match_id = TournamentMatchId::new();
        let target_match_id = TournamentMatchId::new();

        let result = ProgressionResult {
            match_id,
            winner_advancement: Some(Advancement {
                target_match_id,
                target_position: 1,
            }),
            loser_result: LoserResult::Eliminated,
            updated_standings: vec![],
            newly_ready_matches: vec![],
            bracket_complete: false,
            tournament_complete: false,
            stage_advanced: false,
        };

        assert_eq!(result.match_id, match_id);
        assert!(result.winner_advancement.is_some());
        assert!(matches!(result.loser_result, LoserResult::Eliminated));
        assert!(!result.bracket_complete);
        assert!(!result.tournament_complete);
    }

    #[test]
    fn test_progression_result_bracket_complete() {
        let match_id = TournamentMatchId::new();

        let result = ProgressionResult {
            match_id,
            winner_advancement: None, // Final match
            loser_result: LoserResult::Eliminated,
            updated_standings: vec![],
            newly_ready_matches: vec![],
            bracket_complete: true,
            tournament_complete: true,
            stage_advanced: false,
        };

        assert!(result.winner_advancement.is_none());
        assert!(result.bracket_complete);
        assert!(result.tournament_complete);
    }

    #[test]
    fn test_loser_result_clone() {
        let target_id = TournamentMatchId::new();
        let original = LoserResult::DropsTo {
            target_match_id: target_id,
            target_position: 1,
        };

        let cloned = original;
        if let LoserResult::DropsTo {
            target_match_id,
            target_position,
        } = cloned
        {
            assert_eq!(target_match_id, target_id);
            assert_eq!(target_position, 1);
        } else {
            panic!("Clone should preserve variant");
        }
    }
}
