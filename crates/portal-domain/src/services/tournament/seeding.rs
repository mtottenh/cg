//! Tournament seeding service.
//!
//! Handles participant seeding using various algorithms:
//! - Random: Random seeding for casual tournaments
//! - Rating: Based on player/team rating
//! - SeasonRank: Based on seasonal standings
//! - Manual: Admin-specified seeds

use std::sync::Arc;

use portal_core::types::{SeedingAlgorithm, TournamentRegistrationStatus};
use portal_core::{DomainError, TournamentId, TournamentRegistrationId};
use rand::seq::SliceRandom;
use tracing::{info, instrument};

use crate::entities::tournament::TournamentRegistration;
use crate::repositories::tournament::{
    TournamentRegistrationRepository, TournamentRepository, UpdateTournamentRegistration,
};

/// A participant with their seed assignment.
#[derive(Debug, Clone)]
pub struct SeededParticipant {
    /// Registration ID.
    pub registration_id: TournamentRegistrationId,
    /// Participant display name.
    pub participant_name: String,
    /// Assigned seed number (1 = highest seed).
    pub seed: i32,
    /// Rating used for seeding (if applicable).
    pub seed_rating: Option<i32>,
}

/// Service for tournament seeding operations.
pub struct SeedingService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    tournament_repo: Arc<TR>,
    registration_repo: Arc<TRR>,
}

impl<TR, TRR> SeedingService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new seeding service.
    pub const fn new(tournament_repo: Arc<TR>, registration_repo: Arc<TRR>) -> Self {
        Self {
            tournament_repo,
            registration_repo,
        }
    }

    /// Auto-seed participants using the specified algorithm.
    ///
    /// This retrieves all eligible participants (approved or checked-in),
    /// calculates seeds based on the algorithm, and updates the registrations.
    #[instrument(skip(self))]
    pub async fn auto_seed(
        &self,
        tournament_id: TournamentId,
        algorithm: SeedingAlgorithm,
    ) -> Result<Vec<SeededParticipant>, DomainError> {
        // Verify tournament exists and is in a valid state for seeding
        let tournament = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id))?;

        // Tournament should be in a state where seeding makes sense
        // (registration closed or about to start)
        if tournament.is_registration_open() {
            tracing::warn!(
                tournament_id = %tournament_id,
                "Seeding tournament while registration is still open"
            );
        }

        // Get all eligible participants (checked_in takes priority, then approved)
        let eligible_registrations = self.get_eligible_participants(tournament_id).await?;

        if eligible_registrations.is_empty() {
            return Err(DomainError::InsufficientParticipants);
        }

        info!(
            tournament_id = %tournament_id,
            algorithm = %algorithm,
            participant_count = eligible_registrations.len(),
            "Auto-seeding tournament"
        );

        // Calculate seeds based on algorithm
        let seeded = match algorithm {
            SeedingAlgorithm::Random => self.seed_random(eligible_registrations),
            SeedingAlgorithm::Rating => self.seed_by_rating(eligible_registrations),
            SeedingAlgorithm::SeasonRank => self.seed_by_season_rank(eligible_registrations),
            SeedingAlgorithm::Manual => {
                return Err(DomainError::InvalidState(
                    "Use manual_seed for manual seeding".to_string(),
                ));
            }
        };

        // Persist seeds to database
        for participant in &seeded {
            self.registration_repo
                .update(
                    participant.registration_id,
                    UpdateTournamentRegistration {
                        seed: Some(participant.seed),
                        seed_rating: participant.seed_rating,
                        ..Default::default()
                    },
                )
                .await?;
        }

        info!(
            tournament_id = %tournament_id,
            "Seeding complete"
        );

        Ok(seeded)
    }

    /// Manually set seeds for participants.
    ///
    /// Takes a list of (registration_id, seed) pairs and applies them.
    /// Seeds must be unique and start from 1.
    #[instrument(skip(self))]
    pub async fn manual_seed(
        &self,
        tournament_id: TournamentId,
        seeds: Vec<(TournamentRegistrationId, i32)>,
    ) -> Result<Vec<SeededParticipant>, DomainError> {
        // Verify tournament exists
        let _ = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id))?;

        // Validate seeds
        let mut seed_numbers: Vec<i32> = seeds.iter().map(|(_, s)| *s).collect();
        seed_numbers.sort_unstable();

        // Check for duplicates
        for window in seed_numbers.windows(2) {
            if window[0] == window[1] {
                return Err(DomainError::InvalidState(format!(
                    "Duplicate seed number: {}",
                    window[0]
                )));
            }
        }

        // Check seeds are positive
        if seed_numbers.first().is_some_and(|s| *s < 1) {
            return Err(DomainError::InvalidState(
                "Seeds must be positive".to_string(),
            ));
        }

        info!(
            tournament_id = %tournament_id,
            participant_count = seeds.len(),
            "Manually seeding tournament"
        );

        let mut seeded = Vec::new();

        for (registration_id, seed) in seeds {
            // Verify registration belongs to this tournament
            let registration = self
                .registration_repo
                .find_by_id(registration_id)
                .await?
                .ok_or_else(|| DomainError::TournamentRegistrationNotFound(registration_id))?;

            if registration.tournament_id != tournament_id {
                return Err(DomainError::InvalidState(format!(
                    "Registration {registration_id} does not belong to tournament {tournament_id}"
                )));
            }

            // Update the seed
            self.registration_repo
                .update(
                    registration_id,
                    UpdateTournamentRegistration {
                        seed: Some(seed),
                        ..Default::default()
                    },
                )
                .await?;

            seeded.push(SeededParticipant {
                registration_id,
                participant_name: registration.participant_name,
                seed,
                seed_rating: registration.seed_rating,
            });
        }

        // Sort by seed for output
        seeded.sort_by_key(|p| p.seed);

        Ok(seeded)
    }

    /// Get the current seeding for a tournament.
    #[instrument(skip(self))]
    pub async fn get_current_seeding(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<SeededParticipant>, DomainError> {
        // Verify tournament exists
        let _ = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id))?;

        // Get seeded registrations ordered by seed
        let registrations = self.registration_repo.list_seeded(tournament_id).await?;

        let seeded: Vec<SeededParticipant> = registrations
            .into_iter()
            .filter_map(|r| {
                r.seed.map(|seed| SeededParticipant {
                    registration_id: r.id,
                    participant_name: r.participant_name,
                    seed,
                    seed_rating: r.seed_rating,
                })
            })
            .collect();

        Ok(seeded)
    }

    /// Clear all seeds for a tournament.
    #[instrument(skip(self))]
    pub async fn clear_seeding(&self, tournament_id: TournamentId) -> Result<(), DomainError> {
        // Verify tournament exists
        let _ = self
            .tournament_repo
            .find_by_id(tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(tournament_id))?;

        self.registration_repo.clear_seeds(tournament_id).await?;

        info!(tournament_id = %tournament_id, "Cleared all seeds");

        Ok(())
    }

    // =========================================================================
    // Private helper methods
    // =========================================================================

    /// Get all eligible participants for seeding.
    ///
    /// Returns participants with CheckedIn or Approved status.
    async fn get_eligible_participants(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError> {
        // First get checked-in participants
        let (checked_in, _) = self
            .registration_repo
            .list_by_tournament(
                tournament_id,
                Some(TournamentRegistrationStatus::CheckedIn),
                1000,
                0,
            )
            .await?;

        if !checked_in.is_empty() {
            // If we have checked-in participants, only seed those
            return Ok(checked_in);
        }

        // Otherwise get approved participants
        let (approved, _) = self
            .registration_repo
            .list_by_tournament(
                tournament_id,
                Some(TournamentRegistrationStatus::Approved),
                1000,
                0,
            )
            .await?;

        Ok(approved)
    }

    /// Seed participants randomly.
    fn seed_random(&self, registrations: Vec<TournamentRegistration>) -> Vec<SeededParticipant> {
        let mut rng = rand::rng();
        let mut indices: Vec<usize> = (0..registrations.len()).collect();
        indices.shuffle(&mut rng);

        indices
            .into_iter()
            .enumerate()
            .map(|(seed_idx, reg_idx)| {
                let reg = &registrations[reg_idx];
                SeededParticipant {
                    registration_id: reg.id,
                    participant_name: reg.participant_name.clone(),
                    seed: (seed_idx + 1) as i32,
                    seed_rating: None,
                }
            })
            .collect()
    }

    /// Seed participants by rating (highest rating = seed 1).
    fn seed_by_rating(&self, registrations: Vec<TournamentRegistration>) -> Vec<SeededParticipant> {
        let mut with_ratings: Vec<(TournamentRegistration, i32)> = registrations
            .into_iter()
            .map(|r| {
                // Use seed_rating if available, otherwise default to 1000
                let rating = r.seed_rating.unwrap_or(1000);
                (r, rating)
            })
            .collect();

        // Sort by rating descending (highest first)
        with_ratings.sort_by(|a, b| b.1.cmp(&a.1));

        with_ratings
            .into_iter()
            .enumerate()
            .map(|(idx, (reg, rating))| SeededParticipant {
                registration_id: reg.id,
                participant_name: reg.participant_name,
                seed: (idx + 1) as i32,
                seed_rating: Some(rating),
            })
            .collect()
    }

    /// Seed participants by season rank.
    ///
    /// This is a placeholder - in practice, this would need to look up
    /// the participant's standing in the current or previous season.
    fn seed_by_season_rank(
        &self,
        registrations: Vec<TournamentRegistration>,
    ) -> Vec<SeededParticipant> {
        // For now, fall back to rating-based seeding
        // In a full implementation, this would:
        // 1. Look up each team's/player's season standings
        // 2. Sort by their position in the standings
        tracing::info!("Season rank seeding - falling back to rating-based");
        self.seed_by_rating(registrations)
    }
}

// Manual Clone implementation
impl<TR, TRR> Clone for SeedingService<TR, TRR>
where
    TR: TournamentRepository,
    TRR: TournamentRegistrationRepository,
{
    fn clone(&self) -> Self {
        Self {
            tournament_repo: Arc::clone(&self.tournament_repo),
            registration_repo: Arc::clone(&self.registration_repo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seeded_participant() {
        let participant = SeededParticipant {
            registration_id: TournamentRegistrationId::new(),
            participant_name: "Test Team".to_string(),
            seed: 1,
            seed_rating: Some(2000),
        };

        assert_eq!(participant.seed, 1);
        assert_eq!(participant.seed_rating, Some(2000));
    }
}
