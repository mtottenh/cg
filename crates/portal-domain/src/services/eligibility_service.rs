//! Eligibility checking service.
//!
//! Centralises the pattern of fetching player profiles + rating stats
//! for a specific game and running them through eligibility restrictions.
//! Used by both league join/apply handlers and tournament registration handlers.

use crate::entities::eligibility::{EligibilityRestrictions, EligibilityViolation};
use crate::repositories::{PlayerGameProfileRepository, PlayerRatingHistoryRepository};
use crate::services::PlayerGameProfileService;
use portal_core::{DomainError, GameId, PlayerId};
use std::sync::Arc;
use tracing::instrument;

/// Service for checking player eligibility against restrictions.
///
/// Wraps the pure `check_eligibility()` function with data-fetching logic,
/// keeping handlers free of profile/rating plumbing.
#[derive(Clone)]
pub struct EligibilityService<PGPR, PRHR>
where
    PGPR: PlayerGameProfileRepository,
    PRHR: PlayerRatingHistoryRepository,
{
    profile_service: PlayerGameProfileService<PGPR>,
    rating_repo: Arc<PRHR>,
}

impl<PGPR, PRHR> EligibilityService<PGPR, PRHR>
where
    PGPR: PlayerGameProfileRepository,
    PRHR: PlayerRatingHistoryRepository,
{
    pub fn new(profile_service: PlayerGameProfileService<PGPR>, rating_repo: Arc<PRHR>) -> Self {
        Self {
            profile_service,
            rating_repo,
        }
    }

    /// Check a set of players against eligibility restrictions for a specific game.
    ///
    /// Fetches each player's game profile and rating stats, then runs the
    /// standard eligibility check. Returns an empty vec if all players pass.
    ///
    /// The `game_id` parameter ensures we fetch profiles for the correct game —
    /// a player's CS2 rating is irrelevant when checking eligibility for an AoE4 league.
    #[instrument(skip(self, restrictions))]
    pub async fn check_players(
        &self,
        restrictions: &EligibilityRestrictions,
        game_id: GameId,
        player_ids: &[PlayerId],
    ) -> Result<Vec<EligibilityViolation>, DomainError> {
        if !restrictions.has_restrictions() {
            return Ok(vec![]);
        }

        let mut player_data = Vec::with_capacity(player_ids.len());
        for &pid in player_ids {
            let profile = self.profile_service.get_profile(pid, game_id).await?;
            let stats = self.rating_repo.get_rating_stats(pid, game_id).await?;
            player_data.push((pid, profile, stats));
        }

        Ok(super::eligibility::check_eligibility(restrictions, &player_data))
    }

    /// Check players against restrictions parsed from a settings JSONB value.
    ///
    /// Convenience method that parses `EligibilityRestrictions` from the
    /// `"eligibility"` key in the settings object, then delegates to `check_players`.
    /// Returns Ok(empty vec) if no restrictions are configured.
    #[instrument(skip(self, settings))]
    pub async fn check_players_from_settings(
        &self,
        settings: &serde_json::Value,
        game_id: GameId,
        player_ids: &[PlayerId],
    ) -> Result<Vec<EligibilityViolation>, DomainError> {
        let restrictions = EligibilityRestrictions::from_settings(settings);
        self.check_players(&restrictions, game_id, player_ids).await
    }
}
