//! Rating system types (Glicko-2).
//!
//! The platform uses Glicko-2 for player ratings, which tracks:
//! - Rating: the player's skill estimate
//! - Rating Deviation (RD): uncertainty in the rating
//! - Volatility: expected rating fluctuation

use serde::{Deserialize, Serialize};

/// Default rating for new players.
pub const DEFAULT_RATING: f64 = 1500.0;

/// Default rating deviation for new players.
pub const DEFAULT_RATING_DEVIATION: f64 = 350.0;

/// Default volatility for new players.
pub const DEFAULT_VOLATILITY: f64 = 0.06;

/// Minimum rating deviation (players who play regularly).
pub const MIN_RATING_DEVIATION: f64 = 30.0;

/// Maximum rating deviation (inactive players or new players).
pub const MAX_RATING_DEVIATION: f64 = 350.0;

/// A player's Glicko-2 rating.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Glicko2Rating {
    /// The player's skill estimate.
    pub rating: f64,

    /// Uncertainty in the rating (lower = more certain).
    pub rating_deviation: f64,

    /// Expected rating fluctuation.
    pub volatility: f64,
}

impl Default for Glicko2Rating {
    fn default() -> Self {
        Self::new_player()
    }
}

impl Glicko2Rating {
    /// Create a new player rating with default values.
    #[must_use]
    pub fn new_player() -> Self {
        Self {
            rating: DEFAULT_RATING,
            rating_deviation: DEFAULT_RATING_DEVIATION,
            volatility: DEFAULT_VOLATILITY,
        }
    }

    /// Create a rating with specific values.
    #[must_use]
    pub fn new(rating: f64, rating_deviation: f64, volatility: f64) -> Self {
        Self {
            rating,
            rating_deviation: rating_deviation.clamp(MIN_RATING_DEVIATION, MAX_RATING_DEVIATION),
            volatility,
        }
    }

    /// Get the rating as an integer (for display).
    #[must_use]
    pub fn rating_int(&self) -> i32 {
        self.rating.round() as i32
    }

    /// Calculate the 95% confidence interval for the rating.
    ///
    /// Returns (lower_bound, upper_bound).
    #[must_use]
    pub fn confidence_interval(&self) -> (f64, f64) {
        let margin = 2.0 * self.rating_deviation;
        (self.rating - margin, self.rating + margin)
    }

    /// Check if this rating is "established" (low uncertainty).
    ///
    /// An established rating has RD below 100, indicating
    /// the player has played enough games for a reliable estimate.
    #[must_use]
    pub fn is_established(&self) -> bool {
        self.rating_deviation < 100.0
    }

    /// Get a "display rating" that accounts for uncertainty.
    ///
    /// This returns rating - RD, giving a conservative estimate
    /// that's useful for ranking players fairly.
    #[must_use]
    pub fn display_rating(&self) -> f64 {
        self.rating - self.rating_deviation
    }
}

/// A change in rating after a match.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RatingChange {
    /// Rating before the match.
    pub old_rating: f64,

    /// Rating after the match.
    pub new_rating: f64,

    /// Old rating deviation.
    pub old_rd: f64,

    /// New rating deviation.
    pub new_rd: f64,
}

impl RatingChange {
    /// Calculate the rating delta.
    #[must_use]
    pub fn rating_delta(&self) -> f64 {
        self.new_rating - self.old_rating
    }

    /// Check if the rating increased.
    #[must_use]
    pub fn is_gain(&self) -> bool {
        self.new_rating > self.old_rating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_player_rating() {
        let rating = Glicko2Rating::new_player();
        assert_eq!(rating.rating, DEFAULT_RATING);
        assert_eq!(rating.rating_deviation, DEFAULT_RATING_DEVIATION);
        assert!(!rating.is_established());
    }

    #[test]
    fn test_rating_deviation_clamping() {
        let rating = Glicko2Rating::new(1500.0, 500.0, 0.06);
        assert_eq!(rating.rating_deviation, MAX_RATING_DEVIATION);

        let rating = Glicko2Rating::new(1500.0, 10.0, 0.06);
        assert_eq!(rating.rating_deviation, MIN_RATING_DEVIATION);
    }

    #[test]
    fn test_confidence_interval() {
        let rating = Glicko2Rating::new(1500.0, 100.0, 0.06);
        let (low, high) = rating.confidence_interval();
        assert_eq!(low, 1300.0);
        assert_eq!(high, 1700.0);
    }

    #[test]
    fn test_established_rating() {
        let new_player = Glicko2Rating::new_player();
        assert!(!new_player.is_established());

        let veteran = Glicko2Rating::new(1600.0, 50.0, 0.05);
        assert!(veteran.is_established());
    }
}
