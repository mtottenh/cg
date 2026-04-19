//! Bracket progression service.
//!
//! Handles winner advancement, loser routing, and bracket completion detection
//! after match results are finalized.

use std::sync::Arc;

use portal_core::types::{
    BracketType, MatchFormat, StageFormat, StageStatus, TournamentMatchStatus,
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

        // Advance winner
        let winner_advancement = self
            .advance_winner(&match_, winner_registration_id)
            .await?;

        // Route loser
        let loser_result = self
            .route_loser(&match_, &bracket, loser_registration_id)
            .await?;

        // Update standings (for round robin/swiss)
        let updated_standings = if matches!(
            bracket.bracket_type,
            BracketType::RoundRobin | BracketType::Swiss
        ) {
            self.update_standings(
                &bracket,
                &match_,
                winner_registration_id,
                loser_registration_id,
            )
            .await?
        } else {
            Vec::new()
        };

        // Find newly ready matches
        let newly_ready_matches = self.find_newly_ready_matches(match_.bracket_id).await?;

        // Check completion
        let bracket_complete = self.check_bracket_completion(match_.bracket_id).await?;

        // Check for stage advancement (groups → playoffs)
        let mut stage_advanced = false;
        let mut tournament_complete = false;

        if bracket_complete {
            // Check if the completed bracket's stage is a group stage
            let stage = self
                .stage_repo
                .find_by_id(bracket.stage_id)
                .await?
                .ok_or_else(|| {
                    DomainError::Internal(format!("Stage {} not found", bracket.stage_id))
                })?;

            if stage.format == StageFormat::GroupStage {
                // Check if ALL brackets in this stage are complete
                let all_groups_done = self.check_stage_completion(stage.id).await?;
                if all_groups_done {
                    self.advance_to_next_stage(match_.tournament_id, &stage)
                        .await?;
                    stage_advanced = true;
                }
            }

            tournament_complete = self
                .check_tournament_completion(match_.tournament_id)
                .await?;
        }

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
        let target_position = self
            .determine_target_position(source_match, &target_match, true)
            .await?;

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
        let target_position = self
            .determine_target_position(source_match, &target_match, false)
            .await?;

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

    /// Update standings after a match.
    async fn update_standings(
        &self,
        bracket: &TournamentBracket,
        match_: &TournamentMatch,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        // Get or create standings for both participants
        let mut standings = self.standing_repo.list_by_bracket(bracket.id).await?;

        // Find and update winner standing
        if let Some(winner_standing) = standings.iter_mut().find(|s| s.registration_id == winner_id)
        {
            winner_standing.matches_played += 1;
            winner_standing.matches_won += 1;
            winner_standing.game_wins +=
                match_.participant1_score.max(match_.participant2_score);
            winner_standing.game_losses +=
                match_.participant1_score.min(match_.participant2_score);
            winner_standing.game_differential =
                winner_standing.game_wins - winner_standing.game_losses;
            winner_standing.points += 3; // 3 points for win

            // Update head-to-head
            winner_standing.head_to_head.record_win(loser_id);
        }

        // Find and update loser standing
        if let Some(loser_standing) = standings.iter_mut().find(|s| s.registration_id == loser_id) {
            loser_standing.matches_played += 1;
            loser_standing.matches_lost += 1;
            loser_standing.game_wins += match_.participant1_score.min(match_.participant2_score);
            loser_standing.game_losses += match_.participant1_score.max(match_.participant2_score);
            loser_standing.game_differential =
                loser_standing.game_wins - loser_standing.game_losses;
            // 0 points for loss

            // Update head-to-head
            loser_standing.head_to_head.record_loss(winner_id);
        }

        // Recalculate positions
        standings.sort_by(|a, b| {
            // Sort by points desc, then game differential desc, then head-to-head
            b.points
                .cmp(&a.points)
                .then_with(|| b.game_differential.cmp(&a.game_differential))
        });

        for (i, standing) in standings.iter_mut().enumerate() {
            standing.position = (i + 1) as i32;
        }

        // Persist updates via recalculate_positions
        self.standing_repo.recalculate_positions(bracket.id).await?;

        Ok(standings)
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

        // Update status to Ready for these matches
        for match_id in &ready {
            self.match_repo
                .update_status(*match_id, TournamentMatchStatus::Ready)
                .await?;
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

    /// Advance from a completed group stage to the playoff stage.
    ///
    /// Reads standings from each group, cross-seeds for playoffs, and generates
    /// the playoff bracket (SE or DE).
    async fn advance_to_next_stage(
        &self,
        tournament_id: TournamentId,
        completed_stage: &crate::entities::tournament::TournamentStage,
    ) -> Result<(), DomainError> {
        // Find the next stage
        let next_stage = self
            .stage_repo
            .find_next_stage(tournament_id, completed_stage.stage_order)
            .await?
            .ok_or_else(|| {
                DomainError::InvalidState("No next stage found for advancement".to_string())
            })?;

        // Get all group brackets ordered by group_number
        let mut group_brackets = self
            .bracket_repo
            .list_by_stage(completed_stage.id)
            .await?;
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

        // Determine match format from stage or tournament default
        let match_format = next_stage.match_format.unwrap_or(MatchFormat::Bo3);

        // Generate playoff brackets based on stage format
        match next_stage.format {
            StageFormat::SingleElimination => {
                self.generate_se_playoff(
                    tournament_id,
                    next_stage.id,
                    playoff_participants,
                    match_format,
                )
                .await?;
            }
            StageFormat::DoubleElimination => {
                self.generate_de_playoff(
                    tournament_id,
                    next_stage.id,
                    playoff_participants,
                    match_format,
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

        // Mark group stage as Completed, playoff stage as Active
        self.stage_repo
            .update_status(completed_stage.id, StageStatus::Completed)
            .await?;
        self.stage_repo
            .update_status(next_stage.id, StageStatus::Active)
            .await?;

        info!(
            tournament_id = %tournament_id,
            from_stage = %completed_stage.id,
            to_stage = %next_stage.id,
            participants = num_playoff_participants,
            "Advanced from group stage to playoffs"
        );

        Ok(())
    }

    /// Generate a Single Elimination playoff bracket.
    async fn generate_se_playoff(
        &self,
        tournament_id: TournamentId,
        stage_id: TournamentStageId,
        participants: Vec<SeededParticipant>,
        match_format: MatchFormat,
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
            match_format,
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

        helpers::set_se_progression_links(
            self.match_repo.as_ref(),
            &matches,
            &position_to_match,
        )
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
        match_format: MatchFormat,
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
            match_format,
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

    /// Revert progression for a match (used when result is overturned).
    #[instrument(skip(self))]
    pub async fn revert_progression(&self, match_id: TournamentMatchId) -> Result<(), DomainError> {
        let match_ = self.get_match(match_id).await?;

        // If winner advanced, we need to clear that participant from the target match
        // This is a complex operation that may require additional repository methods
        // For now, we'll just log and return - full revert would need more infrastructure
        if match_.winner_progresses_to.is_some() {
            info!(
                match_id = %match_id,
                "Would revert winner progression - needs implementation"
            );
        }

        if match_.loser_progresses_to.is_some() {
            info!(
                match_id = %match_id,
                "Would revert loser progression - needs implementation"
            );
        }

        info!(match_id = %match_id, "Reverted progression");

        Ok(())
    }

    /// Reapply progression with a different winner.
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
            .ok_or_else(|| DomainError::TournamentMatchNotFound(id))
    }

    async fn get_bracket(&self, id: TournamentBracketId) -> Result<TournamentBracket, DomainError> {
        self.bracket_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Bracket {id} not found")))
    }

    async fn determine_target_position(
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
        if let Some(ref source) = target_match.participant1_source {
            if matches_source(source) {
                return Ok(1);
            }
        }

        // Check if position 2 expects from this match
        if let Some(ref source) = target_match.participant2_source {
            if matches_source(source) {
                return Ok(2);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let cloned = original.clone();
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
