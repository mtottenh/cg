//! Tournament stage builder for tests.

use chrono::{DateTime, Utc};
use portal_db::adapters::PgTournamentStageRepository;
use portal_db::DbPool;
use portal_domain::entities::tournament::TournamentStage;
use portal_domain::repositories::tournament::{CreateTournamentStage, TournamentStageRepository};
use portal_core::types::{AdvancementRule, MatchFormat, StageFormat};
use portal_core::TournamentId;

/// Builder for creating test tournament stages.
#[derive(Debug, Clone)]
pub struct TournamentStageBuilder {
    tournament_id: Option<TournamentId>,
    name: String,
    stage_order: i32,
    format: StageFormat,
    format_settings: serde_json::Value,
    advancement_count: Option<i32>,
    advancement_rule: AdvancementRule,
    match_format: Option<MatchFormat>,
    map_veto_format: Option<String>,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
}

impl Default for TournamentStageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentStageBuilder {
    /// Create a new tournament stage builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tournament_id: None,
            name: "Main Stage".to_string(),
            stage_order: 1,
            format: StageFormat::SingleElimination,
            format_settings: serde_json::json!({}),
            advancement_count: None,
            advancement_rule: AdvancementRule::TopN,
            match_format: None,
            map_veto_format: None,
            starts_at: None,
            ends_at: None,
        }
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

    /// Set the stage name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the stage order.
    #[must_use]
    pub const fn stage_order(mut self, order: i32) -> Self {
        self.stage_order = order;
        self
    }

    /// Set format to single elimination (default).
    #[must_use]
    pub fn single_elimination(mut self) -> Self {
        self.format = StageFormat::SingleElimination;
        self
    }

    /// Set format to double elimination.
    #[must_use]
    pub fn double_elimination(mut self) -> Self {
        self.format = StageFormat::DoubleElimination;
        self
    }

    /// Set format to round robin.
    #[must_use]
    pub fn round_robin(mut self) -> Self {
        self.format = StageFormat::RoundRobin;
        self
    }

    /// Set format to swiss.
    #[must_use]
    pub fn swiss(mut self) -> Self {
        self.format = StageFormat::Swiss;
        self
    }

    /// Set format settings.
    #[must_use]
    pub fn format_settings(mut self, settings: serde_json::Value) -> Self {
        self.format_settings = settings;
        self
    }

    /// Set advancement count (how many participants advance to next stage).
    #[must_use]
    pub const fn advancement_count(mut self, count: i32) -> Self {
        self.advancement_count = Some(count);
        self
    }

    /// Set advancement rule.
    #[must_use]
    pub const fn advancement_rule(mut self, rule: AdvancementRule) -> Self {
        self.advancement_rule = rule;
        self
    }

    /// Set the match format for this stage.
    #[must_use]
    pub fn match_format(mut self, format: MatchFormat) -> Self {
        self.match_format = Some(format);
        self
    }

    /// Set the map veto format for this stage.
    #[must_use]
    pub fn map_veto_format(mut self, format: impl Into<String>) -> Self {
        self.map_veto_format = Some(format.into());
        self
    }

    /// Set the stage start time.
    #[must_use]
    pub const fn starts_at(mut self, starts_at: DateTime<Utc>) -> Self {
        self.starts_at = Some(starts_at);
        self
    }

    /// Set the stage end time.
    #[must_use]
    pub const fn ends_at(mut self, ends_at: DateTime<Utc>) -> Self {
        self.ends_at = Some(ends_at);
        self
    }

    /// Build and persist the stage to the database using repository.
    ///
    /// # Panics
    ///
    /// Panics if `tournament_id` is not set.
    pub async fn build_persisted(self, pool: &DbPool) -> TournamentStage {
        let tournament_id = self
            .tournament_id
            .expect("tournament_id must be set before building");

        let repo = PgTournamentStageRepository::new(pool.clone());

        let create = CreateTournamentStage {
            tournament_id,
            name: self.name,
            stage_order: self.stage_order,
            format: self.format,
            format_settings: self.format_settings,
            advancement_count: self.advancement_count,
            advancement_rule: self.advancement_rule,
            match_format: self.match_format,
            map_veto_format: self.map_veto_format,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
        };

        repo.create(create)
            .await
            .expect("Failed to create test tournament stage")
    }
}
