//! Tournament bracket builder for tests.

use portal_db::adapters::PgTournamentBracketRepository;
use portal_db::DbPool;
use portal_domain::entities::tournament::TournamentBracket;
use portal_domain::repositories::tournament::{CreateTournamentBracket, TournamentBracketRepository};
use portal_core::types::BracketType;
use portal_core::{TournamentId, TournamentStageId};

/// Builder for creating test tournament brackets.
#[derive(Debug, Clone)]
pub struct TournamentBracketBuilder {
    stage_id: Option<TournamentStageId>,
    tournament_id: Option<TournamentId>,
    name: String,
    bracket_type: BracketType,
    total_rounds: i32,
    group_number: Option<i32>,
}

impl Default for TournamentBracketBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentBracketBuilder {
    /// Create a new tournament bracket builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stage_id: None,
            tournament_id: None,
            name: "Main Bracket".to_string(),
            bracket_type: BracketType::SingleElim,
            total_rounds: 3,
            group_number: None,
        }
    }

    /// Set the stage ID (required).
    #[must_use]
    pub fn stage_id(mut self, id: TournamentStageId) -> Self {
        self.stage_id = Some(id);
        self
    }

    /// Set the stage ID from a raw UUID.
    #[must_use]
    pub fn stage_id_from_uuid(mut self, id: uuid::Uuid) -> Self {
        self.stage_id = Some(TournamentStageId::from(id));
        self
    }

    /// Set the tournament ID (required).
    #[must_use]
    pub fn tournament_id(mut self, id: TournamentId) -> Self {
        self.tournament_id = Some(id);
        self
    }

    /// Set the tournament ID from a raw UUID.
    #[must_use]
    pub fn tournament_id_from_uuid(mut self, id: uuid::Uuid) -> Self {
        self.tournament_id = Some(TournamentId::from(id));
        self
    }

    /// Set the bracket name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set bracket type to single elimination (default).
    #[must_use]
    pub fn single_elimination(mut self) -> Self {
        self.bracket_type = BracketType::SingleElim;
        self
    }

    /// Set bracket type to double elimination (winners bracket).
    #[must_use]
    pub fn double_elimination_winners(mut self) -> Self {
        self.bracket_type = BracketType::Winners;
        self
    }

    /// Set bracket type to double elimination (losers bracket).
    #[must_use]
    pub fn double_elimination_losers(mut self) -> Self {
        self.bracket_type = BracketType::Losers;
        self
    }

    /// Set bracket type to round robin.
    #[must_use]
    pub fn round_robin(mut self) -> Self {
        self.bracket_type = BracketType::RoundRobin;
        self
    }

    /// Set bracket type to swiss.
    #[must_use]
    pub fn swiss(mut self) -> Self {
        self.bracket_type = BracketType::Swiss;
        self
    }

    /// Set bracket type to grand final.
    #[must_use]
    pub fn grand_final(mut self) -> Self {
        self.bracket_type = BracketType::GrandFinal;
        self
    }

    /// Set the total number of rounds.
    #[must_use]
    pub const fn total_rounds(mut self, rounds: i32) -> Self {
        self.total_rounds = rounds;
        self
    }

    /// Set the group number (for group stage brackets).
    #[must_use]
    pub const fn group_number(mut self, number: i32) -> Self {
        self.group_number = Some(number);
        self
    }

    /// Build and persist the bracket to the database using repository.
    ///
    /// # Panics
    ///
    /// Panics if `stage_id` or `tournament_id` is not set.
    pub async fn build_persisted(self, pool: &DbPool) -> TournamentBracket {
        let stage_id = self.stage_id.expect("stage_id must be set before building");
        let tournament_id = self
            .tournament_id
            .expect("tournament_id must be set before building");

        let repo = PgTournamentBracketRepository::new(pool.clone());

        let create = CreateTournamentBracket {
            stage_id,
            tournament_id,
            name: self.name,
            bracket_type: self.bracket_type,
            total_rounds: self.total_rounds,
            group_number: self.group_number,
        };

        repo.create(create)
            .await
            .expect("Failed to create test tournament bracket")
    }
}
