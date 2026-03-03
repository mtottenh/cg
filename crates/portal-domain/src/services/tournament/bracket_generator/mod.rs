//! Bracket generation for tournaments.
//!
//! This module handles generating match structures for various tournament formats:
//! - Single Elimination (Phase 1)
//! - Double Elimination (Phase 5.1)
//! - Round Robin (Phase 5.2)
//! - Swiss System (Phase 5.3)
//!
//! Future formats (planned):
//! - Groups + Playoffs

mod double_elimination;
mod round_robin;
mod single_elimination;
mod swiss;

use crate::entities::tournament::{SeededParticipant, TournamentRegistration};
use crate::repositories::tournament::CreateTournamentMatch;

// Re-export from submodules
pub use double_elimination::{CrossBracketLink, CrossLinkType, GeneratedDoubleElimination};
pub use swiss::SwissParticipantStanding;

/// Generated bracket structure ready for database insertion.
#[derive(Debug, Clone)]
pub struct GeneratedBracket {
    /// Total number of rounds in the bracket.
    pub total_rounds: i32,
    /// Match data to create.
    pub matches: Vec<CreateTournamentMatch>,
    /// Matches that need participant assignment after creation.
    /// Maps `bracket_position` to (participant, slot).
    pub initial_assignments: Vec<InitialAssignment>,
    /// Bye information (participants who automatically advance).
    pub byes: Vec<ByeInfo>,
}

/// Initial participant assignment for a match.
#[derive(Debug, Clone)]
pub struct InitialAssignment {
    /// The bracket position (e.g., "R1M1").
    pub bracket_position: String,
    /// The participant to assign.
    pub participant: SeededParticipant,
    /// Which slot (1 or 2) to assign to.
    pub slot: u8,
}

/// Information about a bye (auto-advance).
#[derive(Debug, Clone)]
pub struct ByeInfo {
    /// The participant who receives the bye.
    pub participant: SeededParticipant,
    /// The bracket position they advance to.
    pub advances_to_position: String,
    /// Which slot (1 or 2) they advance to.
    pub slot: u8,
}

/// Bracket generator for tournaments.
pub struct BracketGenerator;

impl BracketGenerator {
    /// Prepare participants for bracket generation by sorting and assigning seeds.
    ///
    /// # Arguments
    /// * `registrations` - Confirmed registrations to seed
    ///
    /// # Returns
    /// A list of seeded participants sorted by seed.
    pub fn prepare_participants(
        registrations: Vec<TournamentRegistration>,
    ) -> Vec<SeededParticipant> {
        let mut participants: Vec<SeededParticipant> = registrations
            .into_iter()
            .enumerate()
            .map(|(idx, reg)| SeededParticipant {
                registration_id: reg.id,
                seed: reg.seed.unwrap_or((idx + 1) as i32),
                participant_name: reg.participant_name,
                participant_logo_url: reg.participant_logo_url,
            })
            .collect();

        // Sort by seed
        participants.sort_by_key(|p| p.seed);

        // Re-assign sequential seeds if there are gaps
        for (idx, participant) in participants.iter_mut().enumerate() {
            participant.seed = (idx + 1) as i32;
        }

        participants
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use portal_core::{TournamentId, TournamentRegistrationId};

    pub(crate) fn create_test_participants(count: usize) -> Vec<SeededParticipant> {
        (1..=count)
            .map(|seed| SeededParticipant {
                registration_id: TournamentRegistrationId::new(),
                seed: seed as i32,
                participant_name: format!("Team {seed}"),
                participant_logo_url: None,
            })
            .collect()
    }

    #[test]
    fn test_prepare_participants() {
        let registrations = (1..=4)
            .map(|i| TournamentRegistration {
                id: TournamentRegistrationId::new(),
                tournament_id: TournamentId::new(),
                team_season_id: None,
                player_id: None,
                adhoc_team_id: None,
                participant_name: format!("Team {i}"),
                participant_logo_url: None,
                registered_by: portal_core::UserId::new(),
                registered_at: chrono::Utc::now(),
                checked_in: true,
                checked_in_at: Some(chrono::Utc::now()),
                checked_in_by: None,
                seed: Some(i),
                seed_rating: None,
                status: portal_core::types::TournamentRegistrationStatus::Approved,
                admin_notes: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                withdrawn_at: None,
            })
            .collect();

        let prepared = BracketGenerator::prepare_participants(registrations);

        assert_eq!(prepared.len(), 4);
        assert_eq!(prepared[0].seed, 1);
        assert_eq!(prepared[1].seed, 2);
        assert_eq!(prepared[2].seed, 3);
        assert_eq!(prepared[3].seed, 4);
    }
}
