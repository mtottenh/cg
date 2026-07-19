//! Tournament match builder for tests.

use portal_core::types::{MatchFormat, MatchParticipantSource};
use portal_core::{
    TournamentBracketId, TournamentId, TournamentMatchId, TournamentRegistrationId,
    TournamentStageId,
};
use portal_db::DbPool;
use portal_db::adapters::PgTournamentMatchRepository;
use portal_domain::entities::tournament::TournamentMatch;
use portal_domain::repositories::tournament::{CreateTournamentMatch, TournamentMatchRepository};

/// Builder for creating test tournament matches.
#[derive(Debug, Clone)]
pub struct TournamentMatchBuilder {
    bracket_id: Option<TournamentBracketId>,
    stage_id: Option<TournamentStageId>,
    tournament_id: Option<TournamentId>,
    round: i32,
    match_number: i32,
    bracket_position: Option<String>,
    participant1_registration_id: Option<TournamentRegistrationId>,
    participant2_registration_id: Option<TournamentRegistrationId>,
    participant1_name: Option<String>,
    participant1_logo_url: Option<String>,
    participant1_seed: Option<i32>,
    participant2_name: Option<String>,
    participant2_logo_url: Option<String>,
    participant2_seed: Option<i32>,
    participant1_source: Option<MatchParticipantSource>,
    participant2_source: Option<MatchParticipantSource>,
    match_format: MatchFormat,
    maps_required: i32,
    winner_progresses_to: Option<TournamentMatchId>,
    loser_progresses_to: Option<TournamentMatchId>,
}

impl Default for TournamentMatchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentMatchBuilder {
    /// Create a new tournament match builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bracket_id: None,
            stage_id: None,
            tournament_id: None,
            round: 1,
            match_number: 1,
            bracket_position: None,
            participant1_registration_id: None,
            participant2_registration_id: None,
            participant1_name: None,
            participant1_logo_url: None,
            participant1_seed: None,
            participant2_name: None,
            participant2_logo_url: None,
            participant2_seed: None,
            participant1_source: None,
            participant2_source: None,
            match_format: MatchFormat::Bo3,
            maps_required: 2,
            winner_progresses_to: None,
            loser_progresses_to: None,
        }
    }

    /// Set the bracket ID (required).
    #[must_use]
    pub fn bracket_id(mut self, id: TournamentBracketId) -> Self {
        self.bracket_id = Some(id);
        self
    }

    /// Set the bracket ID from a raw UUID.
    #[must_use]
    pub fn bracket_id_from_uuid(mut self, id: uuid::Uuid) -> Self {
        self.bracket_id = Some(TournamentBracketId::from(id));
        self
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

    /// Set the round number.
    #[must_use]
    pub const fn round(mut self, round: i32) -> Self {
        self.round = round;
        self
    }

    /// Set the match number.
    #[must_use]
    pub const fn match_number(mut self, number: i32) -> Self {
        self.match_number = number;
        self
    }

    /// Set the bracket position string (e.g., "W-R1-M1", "L-R2-M3").
    #[must_use]
    pub fn bracket_position(mut self, position: impl Into<String>) -> Self {
        self.bracket_position = Some(position.into());
        self
    }

    /// Set participant 1 (first team/player).
    #[must_use]
    pub fn participant1(
        mut self,
        registration_id: TournamentRegistrationId,
        name: impl Into<String>,
    ) -> Self {
        self.participant1_registration_id = Some(registration_id);
        self.participant1_name = Some(name.into());
        self
    }

    /// Set participant 1 from a raw UUID.
    #[must_use]
    pub fn participant1_from_uuid(
        mut self,
        registration_id: uuid::Uuid,
        name: impl Into<String>,
    ) -> Self {
        self.participant1_registration_id = Some(TournamentRegistrationId::from(registration_id));
        self.participant1_name = Some(name.into());
        self
    }

    /// Set participant 1 seed.
    #[must_use]
    pub const fn participant1_seed(mut self, seed: i32) -> Self {
        self.participant1_seed = Some(seed);
        self
    }

    /// Set participant 2 (second team/player).
    #[must_use]
    pub fn participant2(
        mut self,
        registration_id: TournamentRegistrationId,
        name: impl Into<String>,
    ) -> Self {
        self.participant2_registration_id = Some(registration_id);
        self.participant2_name = Some(name.into());
        self
    }

    /// Set participant 2 from a raw UUID.
    #[must_use]
    pub fn participant2_from_uuid(
        mut self,
        registration_id: uuid::Uuid,
        name: impl Into<String>,
    ) -> Self {
        self.participant2_registration_id = Some(TournamentRegistrationId::from(registration_id));
        self.participant2_name = Some(name.into());
        self
    }

    /// Set participant 2 seed.
    #[must_use]
    pub const fn participant2_seed(mut self, seed: i32) -> Self {
        self.participant2_seed = Some(seed);
        self
    }

    /// Set the match format.
    #[must_use]
    pub const fn match_format(mut self, format: MatchFormat) -> Self {
        self.match_format = format;
        self
    }

    /// Set match format to Bo1.
    #[must_use]
    pub fn bo1(mut self) -> Self {
        self.match_format = MatchFormat::Bo1;
        self.maps_required = 1;
        self
    }

    /// Set match format to Bo3.
    #[must_use]
    pub fn bo3(mut self) -> Self {
        self.match_format = MatchFormat::Bo3;
        self.maps_required = 2;
        self
    }

    /// Set match format to Bo5.
    #[must_use]
    pub fn bo5(mut self) -> Self {
        self.match_format = MatchFormat::Bo5;
        self.maps_required = 3;
        self
    }

    /// Set the number of maps required to win.
    #[must_use]
    pub const fn maps_required(mut self, maps: i32) -> Self {
        self.maps_required = maps;
        self
    }

    /// Set where the winner progresses to.
    #[must_use]
    pub fn winner_progresses_to(mut self, match_id: TournamentMatchId) -> Self {
        self.winner_progresses_to = Some(match_id);
        self
    }

    /// Set where the loser progresses to (for double elimination).
    #[must_use]
    pub fn loser_progresses_to(mut self, match_id: TournamentMatchId) -> Self {
        self.loser_progresses_to = Some(match_id);
        self
    }

    /// Build and persist the match to the database using repository.
    ///
    /// # Panics
    ///
    /// Panics if `bracket_id`, `stage_id`, or `tournament_id` is not set.
    pub async fn build_persisted(self, pool: &DbPool) -> TournamentMatch {
        let bracket_id = self
            .bracket_id
            .expect("bracket_id must be set before building");
        let stage_id = self.stage_id.expect("stage_id must be set before building");
        let tournament_id = self
            .tournament_id
            .expect("tournament_id must be set before building");

        let bracket_position = self
            .bracket_position
            .unwrap_or_else(|| format!("R{}-M{}", self.round, self.match_number));

        let repo = PgTournamentMatchRepository::new(pool.clone());

        let create = CreateTournamentMatch {
            bracket_id,
            stage_id,
            tournament_id,
            round: self.round,
            match_number: self.match_number,
            bracket_position,
            participant1_registration_id: self.participant1_registration_id,
            participant2_registration_id: self.participant2_registration_id,
            participant1_name: self.participant1_name,
            participant1_logo_url: self.participant1_logo_url,
            participant1_seed: self.participant1_seed,
            participant2_name: self.participant2_name,
            participant2_logo_url: self.participant2_logo_url,
            participant2_seed: self.participant2_seed,
            participant1_source: self.participant1_source,
            participant2_source: self.participant2_source,
            match_format: self.match_format,
            maps_required: self.maps_required,
            winner_progresses_to: self.winner_progresses_to,
            loser_progresses_to: self.loser_progresses_to,
        };

        repo.create(create)
            .await
            .expect("Failed to create test tournament match")
    }
}
