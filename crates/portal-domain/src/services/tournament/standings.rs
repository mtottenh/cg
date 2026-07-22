//! Standings calculation service.
//!
//! Handles standings initialization, updates, recalculation, and tiebreaker computation
//! for round robin and swiss tournament formats.

use std::sync::Arc;

use portal_core::{DomainError, TournamentBracketId, TournamentRegistrationId};
use tracing::{info, instrument};

use crate::entities::tournament::{HeadToHead, TournamentRegistration, TournamentStanding};
use crate::repositories::tournament::{
    CreateTournamentStanding, TournamentMatchRepository, TournamentStandingsRepository,
};

/// Service for managing tournament standings.
#[derive(Clone)]
pub struct StandingsService<TSTR, TMR> {
    standing_repo: Arc<TSTR>,
    match_repo: Arc<TMR>,
}

impl<TSTR, TMR> StandingsService<TSTR, TMR>
where
    TSTR: TournamentStandingsRepository,
    TMR: TournamentMatchRepository,
{
    /// Create a new standings service.
    pub fn new(standing_repo: Arc<TSTR>, match_repo: Arc<TMR>) -> Self {
        Self {
            standing_repo,
            match_repo,
        }
    }

    /// Initialize standings for all participants in a bracket.
    #[instrument(skip(self, registrations))]
    pub async fn initialize_standings(
        &self,
        bracket_id: TournamentBracketId,
        registrations: &[TournamentRegistration],
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let mut standings = Vec::new();

        for (i, reg) in registrations.iter().enumerate() {
            let standing = self
                .standing_repo
                .create(CreateTournamentStanding {
                    bracket_id,
                    registration_id: reg.id,
                    position: (i + 1) as i32,
                })
                .await?;
            standings.push(standing);
        }

        info!(
            bracket_id = %bracket_id,
            count = standings.len(),
            "Initialized standings"
        );

        Ok(standings)
    }

    /// Recompute a bracket's standings from its completed match rows and
    /// recorded byes.
    ///
    /// Standings are derived, not accumulated: this delegates to the
    /// repository's idempotent [`recompute_bracket`], so it can be run any
    /// number of times without double-counting. The winner/loser arguments
    /// are read off the persisted match rows, so the previous per-result
    /// delta parameters are no longer needed.
    ///
    /// [`recompute_bracket`]: TournamentStandingsRepository::recompute_bracket
    #[instrument(skip(self))]
    pub async fn recompute_for_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        self.standing_repo.recompute_bracket(bracket_id).await
    }

    /// Recalculate all standings from scratch.
    #[instrument(skip(self))]
    pub async fn recalculate_standings(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let matches = self.match_repo.list_by_bracket(bracket_id).await?;
        let standings = self.standing_repo.list_by_bracket(bracket_id).await?;

        // Create a map to accumulate stats
        let mut stats: std::collections::HashMap<TournamentRegistrationId, StandingStats> =
            standings
                .iter()
                .map(|s| (s.registration_id, StandingStats::default()))
                .collect();

        // Replay all completed matches
        for match_ in &matches {
            if !match_.is_complete() {
                continue;
            }

            let Some(winner_id) = match_.winner_registration_id else {
                continue;
            };
            let Some(loser_id) = match_.loser_registration_id else {
                continue;
            };

            let winner_score = match_.participant1_score.max(match_.participant2_score);
            let loser_score = match_.participant1_score.min(match_.participant2_score);

            // Update winner
            if let Some(winner_stats) = stats.get_mut(&winner_id) {
                winner_stats.matches_played += 1;
                winner_stats.matches_won += 1;
                winner_stats.game_wins += winner_score;
                winner_stats.game_losses += loser_score;
                winner_stats.points += 3;
                winner_stats.head_to_head.record_win(loser_id);
            }

            // Update loser
            if let Some(loser_stats) = stats.get_mut(&loser_id) {
                loser_stats.matches_played += 1;
                loser_stats.matches_lost += 1;
                loser_stats.game_wins += loser_score;
                loser_stats.game_losses += winner_score;
                loser_stats.head_to_head.record_loss(winner_id);
            }
        }

        // Calculate Buchholz scores - need to collect first due to borrow checker
        let buchholz_updates: Vec<_> = stats
            .iter()
            .map(|(reg_id, stat)| {
                let buchholz: f64 = stat
                    .head_to_head
                    .records
                    .keys()
                    .filter_map(|opp_id| stats.get(opp_id))
                    .map(|opp_stats| f64::from(opp_stats.points))
                    .sum();
                (*reg_id, buchholz)
            })
            .collect();

        for (reg_id, buchholz) in buchholz_updates {
            if let Some(stat) = stats.get_mut(&reg_id) {
                stat.buchholz_score = Some(buchholz);
            }
        }

        // Calculate opponent match wins
        let omw_updates: Vec<_> = stats
            .iter()
            .map(|(reg_id, stat)| {
                let (total_opp_wins, total_opp_matches): (i32, i32) = stat
                    .head_to_head
                    .records
                    .keys()
                    .filter_map(|opp_id| stats.get(opp_id))
                    .fold((0, 0), |(wins, matches), opp_stats| {
                        (
                            wins + opp_stats.matches_won,
                            matches + opp_stats.matches_played,
                        )
                    });

                let omw = if total_opp_matches > 0 {
                    f64::from(total_opp_wins) / f64::from(total_opp_matches)
                } else {
                    0.0
                };
                (*reg_id, omw)
            })
            .collect();

        for (reg_id, omw) in omw_updates {
            if let Some(stat) = stats.get_mut(&reg_id) {
                stat.opponent_match_wins = Some(omw);
            }
        }

        // Recalculate positions via repository
        let updated = self.standing_repo.recalculate_positions(bracket_id).await?;

        info!(
            bracket_id = %bracket_id,
            "Recalculated standings from {} matches",
            matches.len()
        );

        Ok(updated)
    }

    /// Get current standings for a bracket.
    #[instrument(skip(self))]
    pub async fn get_standings(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentStanding>, DomainError> {
        let mut standings = self.standing_repo.list_by_bracket(bracket_id).await?;
        standings.sort_by(|a, b| a.position.cmp(&b.position));
        Ok(standings)
    }

    /// Calculate Buchholz score for a participant.
    ///
    /// Buchholz score is the sum of all opponents' points.
    #[instrument(skip(self))]
    pub async fn calculate_buchholz(
        &self,
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
    ) -> Result<f64, DomainError> {
        let matches = self.match_repo.list_by_bracket(bracket_id).await?;
        let standings = self.standing_repo.list_by_bracket(bracket_id).await?;

        let mut buchholz = 0.0;

        for match_ in &matches {
            if !match_.is_complete() {
                continue;
            }

            // Find if this registration was a participant
            let opponent_id = if match_.participant1_registration_id == Some(registration_id) {
                match_.participant2_registration_id
            } else if match_.participant2_registration_id == Some(registration_id) {
                match_.participant1_registration_id
            } else {
                continue;
            };

            // Add opponent's points
            if let Some(opp_id) = opponent_id
                && let Some(opp_standing) = standings.iter().find(|s| s.registration_id == opp_id)
            {
                buchholz += f64::from(opp_standing.points);
            }
        }

        Ok(buchholz)
    }
}

/// Internal stats accumulator for recalculation.
#[derive(Debug, Default)]
struct StandingStats {
    matches_played: i32,
    matches_won: i32,
    matches_lost: i32,
    #[allow(dead_code)]
    matches_drawn: i32,
    game_wins: i32,
    game_losses: i32,
    points: i32,
    head_to_head: HeadToHead,
    buchholz_score: Option<f64>,
    opponent_match_wins: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standing_stats_default() {
        let stats = StandingStats::default();
        assert_eq!(stats.matches_played, 0);
        assert_eq!(stats.matches_won, 0);
        assert_eq!(stats.matches_lost, 0);
        assert_eq!(stats.game_wins, 0);
        assert_eq!(stats.points, 0);
    }

    #[test]
    fn test_head_to_head_record_win() {
        let mut h2h = HeadToHead::default();
        let opponent_id = TournamentRegistrationId::new();

        h2h.record_win(opponent_id);

        assert!(h2h.records.contains_key(&opponent_id));
        let record = h2h.records.get(&opponent_id).unwrap();
        assert_eq!(record.wins, 1);
        assert_eq!(record.losses, 0);
    }

    #[test]
    fn test_head_to_head_record_loss() {
        let mut h2h = HeadToHead::default();
        let opponent_id = TournamentRegistrationId::new();

        h2h.record_loss(opponent_id);

        assert!(h2h.records.contains_key(&opponent_id));
        let record = h2h.records.get(&opponent_id).unwrap();
        assert_eq!(record.wins, 0);
        assert_eq!(record.losses, 1);
    }

    #[test]
    fn test_head_to_head_multiple_games() {
        let mut h2h = HeadToHead::default();
        let opponent_id = TournamentRegistrationId::new();

        // Record 2 wins and 1 loss against same opponent
        h2h.record_win(opponent_id);
        h2h.record_win(opponent_id);
        h2h.record_loss(opponent_id);

        let record = h2h.records.get(&opponent_id).unwrap();
        assert_eq!(record.wins, 2);
        assert_eq!(record.losses, 1);
    }

    #[test]
    fn test_standing_stats_accumulation() {
        let mut stats = StandingStats::default();
        let opponent1 = TournamentRegistrationId::new();
        let opponent2 = TournamentRegistrationId::new();

        // Simulate winning a match 2-1 against opponent1
        stats.matches_played += 1;
        stats.matches_won += 1;
        stats.game_wins += 2;
        stats.game_losses += 1;
        stats.points += 3;
        stats.head_to_head.record_win(opponent1);

        // Simulate losing a match 0-2 against opponent2
        stats.matches_played += 1;
        stats.matches_lost += 1;
        stats.game_wins += 0;
        stats.game_losses += 2;
        stats.points += 0;
        stats.head_to_head.record_loss(opponent2);

        assert_eq!(stats.matches_played, 2);
        assert_eq!(stats.matches_won, 1);
        assert_eq!(stats.matches_lost, 1);
        assert_eq!(stats.game_wins, 2);
        assert_eq!(stats.game_losses, 3);
        assert_eq!(stats.points, 3);
        assert_eq!(stats.head_to_head.records.len(), 2);
    }
}
