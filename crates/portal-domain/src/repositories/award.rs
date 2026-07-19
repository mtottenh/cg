//! Award repository trait (templates, instances, finalized results).

use async_trait::async_trait;
use portal_core::{AwardId, DomainError, GameId, PlayerId, UserId};
use uuid::Uuid;

use crate::entities::award::{
    Award, AwardResult, AwardScopeType, AwardTemplate, MinQualifier, StatAggregation, StatDirection,
};

/// Data for creating an award instance.
#[derive(Debug, Clone)]
pub struct CreateAward {
    pub scope_type: AwardScopeType,
    /// Tournament id or league-season id depending on `scope_type`.
    pub scope_id: Uuid,
    pub game_id: GameId,
    /// Source template, when instantiated from one.
    pub template_id: Option<portal_core::AwardTemplateId>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: StatAggregation,
    pub direction: StatDirection,
    pub min_qualifier: Option<MinQualifier>,
    pub created_by: UserId,
}

/// Presentation-only fields an organizer may edit on an active award.
/// `None` leaves the field unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateAwardPresentation {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

impl UpdateAwardPresentation {
    /// Whether the update changes anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.icon.is_none()
            && self.color.is_none()
    }
}

/// One podium row to write at finalization.
#[derive(Debug, Clone)]
pub struct CreateAwardResult {
    /// Competition rank (1-based; ties share a rank).
    pub rank: i32,
    pub player_id: PlayerId,
    pub value: f64,
    pub demos_counted: i32,
}

/// A finalized award result joined with its award and the scope's display
/// name — one entry in a player's trophy case.
#[derive(Debug, Clone)]
pub struct PlayerTrophy {
    pub result: AwardResult,
    pub award: Award,
    /// Tournament name or league-season name, when the scope still exists.
    pub scope_name: Option<String>,
}

/// Repository for award templates, instances, and finalized results.
#[async_trait]
pub trait AwardRepository: Send + Sync + 'static {
    // =========================================================================
    // Templates
    // =========================================================================

    /// List a game's award templates.
    async fn list_templates_by_game(
        &self,
        game_id: GameId,
    ) -> Result<Vec<AwardTemplate>, DomainError>;

    /// Find a template by its stable key within a game.
    async fn find_template_by_key(
        &self,
        game_id: GameId,
        key: &str,
    ) -> Result<Option<AwardTemplate>, DomainError>;

    // =========================================================================
    // Award instances
    // =========================================================================

    /// Create an award. A duplicate name within the same scope yields
    /// [`DomainError::Conflict`].
    async fn create(&self, award: CreateAward) -> Result<Award, DomainError>;

    /// Find an award by id.
    async fn find_by_id(&self, id: AwardId) -> Result<Option<Award>, DomainError>;

    /// List every award in a scope (all statuses), stable order.
    async fn list_by_scope(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<Vec<Award>, DomainError>;

    /// Update presentation fields. A name collision within the scope yields
    /// [`DomainError::Conflict`].
    async fn update_presentation(
        &self,
        id: AwardId,
        update: UpdateAwardPresentation,
    ) -> Result<Award, DomainError>;

    /// Set the lifecycle status.
    async fn set_status(
        &self,
        id: AwardId,
        status: crate::entities::award::AwardStatus,
    ) -> Result<Award, DomainError>;

    // =========================================================================
    // Finalized results
    // =========================================================================

    /// Atomically replace an award's results and flip its status to
    /// `finalized` (idempotent re-finalization replaces the podium).
    async fn replace_results_and_finalize(
        &self,
        award_id: AwardId,
        results: Vec<CreateAwardResult>,
    ) -> Result<Vec<AwardResult>, DomainError>;

    /// List an award's podium rows ordered by rank.
    async fn list_results_by_award(
        &self,
        award_id: AwardId,
    ) -> Result<Vec<AwardResult>, DomainError>;

    /// A player's trophy case: results on finalized awards, newest first,
    /// with award branding and scope display names.
    async fn list_trophies_by_player(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTrophy>, DomainError>;
}
