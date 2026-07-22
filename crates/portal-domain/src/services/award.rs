//! Award service: authoring, live standings, and finalization.
//!
//! Game-agnostic — the award engine only speaks stat keys. Validating a
//! stat key against a game plugin's catalog happens at the API layer (the
//! domain must not depend on `portal-plugins`); this service validates
//! shape (name/color/qualifier) and drives the lifecycle:
//!
//! `active` → (`finalize`) → `finalized`, or `active` → (`void`) → `void`.
//!
//! Finalization computes standings via the leaderboard aggregation, takes
//! the podium (rank ≤ 3, competition ranking — ties share a rank), and
//! atomically writes `award_results` + status. Re-finalization is allowed
//! while the surrounding scope is not locked (late demo parses recompute
//! the podium); once the scope is locked, history is stable.

use std::sync::Arc;

use portal_core::{AwardId, DomainError, FieldError, GameId, PlayerId, UserId, ValidationError};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::entities::award::{
    Award, AwardResult, AwardScopeType, AwardStatus, AwardTemplate, MinQualifier, StatAggregation,
    StatDirection,
};
use crate::repositories::award::{
    AwardRepository, CreateAward, CreateAwardResult, PlayerTrophy, UpdateAwardPresentation,
};
use crate::repositories::demo_stats::{
    DemoPlayerStatsRepository, LeaderboardEntry, LeaderboardQuery, LeaderboardScope,
    PlayerStatsEntry, PlayerStatsQuery,
};

/// How many leaderboard rows finalization examines when computing the
/// podium. Ties can extend a rank arbitrarily, so this is a safety bound,
/// not a podium size.
const FINALIZE_CANDIDATE_LIMIT: i64 = 100;

/// Highest rank stored in `award_results` (top-3 podium).
const PODIUM_MAX_RANK: i32 = 3;

/// Custom-award creation input (template-free path).
#[derive(Debug, Clone)]
pub struct CreateCustomAwardCommand {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    pub aggregation: StatAggregation,
    pub direction: StatDirection,
    pub min_qualifier: Option<MinQualifier>,
}

/// Service for award authoring, standings, and finalization.
#[derive(Clone)]
pub struct AwardService<AR, SR>
where
    AR: AwardRepository,
    SR: DemoPlayerStatsRepository,
{
    award_repo: Arc<AR>,
    stats_repo: Arc<SR>,
}

impl<AR, SR> AwardService<AR, SR>
where
    AR: AwardRepository,
    SR: DemoPlayerStatsRepository,
{
    /// Create a new award service.
    pub const fn new(award_repo: Arc<AR>, stats_repo: Arc<SR>) -> Self {
        Self {
            award_repo,
            stats_repo,
        }
    }

    // =========================================================================
    // Templates
    // =========================================================================

    /// List a game's award templates.
    #[instrument(skip(self))]
    pub async fn list_templates(&self, game_id: GameId) -> Result<Vec<AwardTemplate>, DomainError> {
        self.award_repo.list_templates_by_game(game_id).await
    }

    // =========================================================================
    // Award instances
    // =========================================================================

    /// List every award in a scope.
    #[instrument(skip(self))]
    pub async fn list_awards(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<Vec<Award>, DomainError> {
        self.award_repo.list_by_scope(scope_type, scope_id).await
    }

    /// Get an award, verifying it belongs to the given scope (an award
    /// addressed through the wrong scope path is treated as not found).
    #[instrument(skip(self))]
    pub async fn get_award_in_scope(
        &self,
        id: AwardId,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<Award, DomainError> {
        let award = self
            .award_repo
            .find_by_id(id)
            .await?
            .filter(|a| a.scope_type == scope_type && a.scope_id == scope_id);
        award.ok_or_else(|| DomainError::LookupFailed {
            resource: "award",
            query: id.to_string(),
        })
    }

    /// Create an award from a per-game template. The template's branding
    /// and metric tuple seed the instance; `name_override` lets organizers
    /// rename at creation.
    #[instrument(skip(self))]
    pub async fn create_from_template(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
        game_id: GameId,
        template_key: &str,
        name_override: Option<String>,
        created_by: UserId,
    ) -> Result<Award, DomainError> {
        let template = self
            .award_repo
            .find_template_by_key(game_id, template_key)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "award template",
                query: template_key.to_string(),
            })?;

        let name = name_override.unwrap_or_else(|| template.name.clone());
        validate_award_shape(&name, &template.stat_key, template.color.as_deref(), None)?;

        let award = self
            .award_repo
            .create(CreateAward {
                scope_type,
                scope_id,
                game_id,
                template_id: Some(template.id),
                name,
                description: template.description.clone(),
                icon: template.icon.clone(),
                color: template.color.clone(),
                stat_key: template.stat_key.clone(),
                aggregation: template.aggregation,
                direction: template.direction,
                min_qualifier: template.min_qualifier,
                created_by,
            })
            .await?;

        info!(award_id = %award.id, template = %template_key, "Created award from template");
        Ok(award)
    }

    /// Create a custom award from the stat catalog. The caller (API layer)
    /// is responsible for validating `stat_key` against the game plugin's
    /// catalog; this validates shape only.
    #[instrument(skip(self, cmd))]
    pub async fn create_custom(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
        game_id: GameId,
        cmd: CreateCustomAwardCommand,
        created_by: UserId,
    ) -> Result<Award, DomainError> {
        validate_award_shape(
            &cmd.name,
            &cmd.stat_key,
            cmd.color.as_deref(),
            cmd.min_qualifier.as_ref(),
        )?;

        let award = self
            .award_repo
            .create(CreateAward {
                scope_type,
                scope_id,
                game_id,
                template_id: None,
                name: cmd.name,
                description: cmd.description,
                icon: cmd.icon,
                color: cmd.color,
                stat_key: cmd.stat_key,
                aggregation: cmd.aggregation,
                direction: cmd.direction,
                min_qualifier: cmd.min_qualifier,
                created_by,
            })
            .await?;

        info!(award_id = %award.id, stat_key = %award.stat_key, "Created custom award");
        Ok(award)
    }

    /// Update presentation fields (name/description/icon/color) — allowed
    /// only while the award is active.
    #[instrument(skip(self, update))]
    pub async fn update_award(
        &self,
        id: AwardId,
        scope_type: AwardScopeType,
        scope_id: Uuid,
        update: UpdateAwardPresentation,
    ) -> Result<Award, DomainError> {
        let award = self.get_award_in_scope(id, scope_type, scope_id).await?;
        if !award.is_active() {
            return Err(DomainError::conflict(format!(
                "Award is {} and can no longer be edited",
                award.status
            )));
        }
        if update.is_empty() {
            return Ok(award);
        }
        if let Some(name) = &update.name {
            validate_award_shape(name, &award.stat_key, update.color.as_deref(), None)?;
        }
        self.award_repo.update_presentation(id, update).await
    }

    /// Void an active award. Finalized awards are permanent history and
    /// cannot be voided.
    #[instrument(skip(self))]
    pub async fn void_award(
        &self,
        id: AwardId,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<Award, DomainError> {
        let award = self.get_award_in_scope(id, scope_type, scope_id).await?;
        match award.status {
            AwardStatus::Active => self.award_repo.set_status(id, AwardStatus::Void).await,
            AwardStatus::Void => Ok(award),
            AwardStatus::Finalized => Err(DomainError::conflict(
                "A finalized award cannot be voided".to_string(),
            )),
        }
    }

    // =========================================================================
    // Standings + leaderboards
    // =========================================================================

    /// Live standings for an award: the leaderboard for its metric tuple.
    #[instrument(skip(self, award))]
    pub async fn standings(
        &self,
        award: &Award,
        limit: i64,
    ) -> Result<Vec<LeaderboardEntry>, DomainError> {
        self.stats_repo
            .leaderboard(&award_leaderboard_query(award, limit))
            .await
    }

    /// A plain (unnamed) leaderboard over the same aggregation engine.
    #[instrument(skip(self, query))]
    pub async fn leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<Vec<LeaderboardEntry>, DomainError> {
        self.stats_repo.leaderboard(query).await
    }

    /// A combined per-player stat leaderboard (one row per player, separate
    /// kill/death/assist/damage columns plus a rounds-weighted ADR).
    #[instrument(skip(self, query))]
    pub async fn player_stats_leaderboard(
        &self,
        query: &PlayerStatsQuery,
    ) -> Result<Vec<PlayerStatsEntry>, DomainError> {
        self.stats_repo.player_stats_leaderboard(query).await
    }

    // =========================================================================
    // Finalization
    // =========================================================================

    /// Finalize an award: snapshot the podium (rank ≤ 3, ties share ranks)
    /// into `award_results` and flip status to `finalized`.
    ///
    /// Idempotent while the scope is not locked: re-finalizing recomputes
    /// and replaces the podium. `scope_locked` reflects the surrounding
    /// tournament/season lifecycle (e.g. tournament `finalized`); once
    /// locked, results are permanent.
    #[instrument(skip(self))]
    pub async fn finalize(
        &self,
        id: AwardId,
        scope_type: AwardScopeType,
        scope_id: Uuid,
        scope_locked: bool,
    ) -> Result<(Award, Vec<AwardResult>), DomainError> {
        let award = self.get_award_in_scope(id, scope_type, scope_id).await?;
        match award.status {
            AwardStatus::Void => {
                return Err(DomainError::conflict(
                    "A voided award cannot be finalized".to_string(),
                ));
            }
            AwardStatus::Finalized if scope_locked => {
                return Err(DomainError::conflict(
                    "Award results are locked; the scope is finalized".to_string(),
                ));
            }
            AwardStatus::Active | AwardStatus::Finalized => {}
        }

        let standings = self
            .stats_repo
            .leaderboard(&award_leaderboard_query(&award, FINALIZE_CANDIDATE_LIMIT))
            .await?;
        let podium = podium_from_standings(&standings);

        let results = self
            .award_repo
            .replace_results_and_finalize(id, podium)
            .await?;
        let award = self.get_award_in_scope(id, scope_type, scope_id).await?;

        info!(
            award_id = %id,
            winners = results.len(),
            "Finalized award"
        );
        Ok((award, results))
    }

    /// Finalize every active award in a scope (the tournament-completion
    /// hook). Per-award failures are logged and skipped — a bad award must
    /// never fail the lifecycle transition. Returns how many finalized.
    #[instrument(skip(self))]
    pub async fn finalize_scope_awards(
        &self,
        scope_type: AwardScopeType,
        scope_id: Uuid,
    ) -> Result<usize, DomainError> {
        let awards = self.award_repo.list_by_scope(scope_type, scope_id).await?;
        let mut finalized = 0;
        for award in awards.iter().filter(|a| a.is_active()) {
            match self.finalize(award.id, scope_type, scope_id, false).await {
                Ok(_) => finalized += 1,
                Err(e) => {
                    warn!(award_id = %award.id, error = %e, "Failed to auto-finalize award");
                }
            }
        }
        Ok(finalized)
    }

    // =========================================================================
    // Results + trophy case
    // =========================================================================

    /// An award's podium rows.
    #[instrument(skip(self))]
    pub async fn results(&self, award_id: AwardId) -> Result<Vec<AwardResult>, DomainError> {
        self.award_repo.list_results_by_award(award_id).await
    }

    /// A player's trophy case.
    #[instrument(skip(self))]
    pub async fn player_trophies(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PlayerTrophy>, DomainError> {
        self.award_repo.list_trophies_by_player(player_id).await
    }
}

/// Build the leaderboard query for an award's metric tuple.
fn award_leaderboard_query(award: &Award, limit: i64) -> LeaderboardQuery {
    let scope = match award.scope_type {
        AwardScopeType::Tournament => {
            LeaderboardScope::Tournament(portal_core::TournamentId::from_uuid(award.scope_id))
        }
        AwardScopeType::LeagueSeason => {
            LeaderboardScope::Season(portal_core::LeagueSeasonId::from_uuid(award.scope_id))
        }
    };
    LeaderboardQuery {
        scope,
        stat_key: award.stat_key.clone(),
        aggregation: award.aggregation,
        direction: award.direction,
        min_qualifier: award.min_qualifier,
        limit,
    }
}

/// Assign competition ranks ("1224": ties share a rank, the next rank
/// skips) over standings already ordered by the ranking direction.
#[must_use]
pub fn competition_ranks(standings: &[LeaderboardEntry]) -> Vec<i32> {
    let mut ranks = Vec::with_capacity(standings.len());
    let mut rank = 0_i32;
    let mut last_value: Option<f64> = None;
    for (position, entry) in standings.iter().enumerate() {
        // Exact equality is intentional: tied values come from the same SQL
        // aggregate over the same facts.
        #[allow(clippy::float_cmp)]
        if last_value != Some(entry.value) {
            rank = i32::try_from(position)
                .unwrap_or(i32::MAX)
                .saturating_add(1);
            last_value = Some(entry.value);
        }
        ranks.push(rank);
    }
    ranks
}

/// The podium (rows with rank ≤ [`PODIUM_MAX_RANK`], ties kept) from
/// standings already ordered by the ranking direction.
fn podium_from_standings(standings: &[LeaderboardEntry]) -> Vec<CreateAwardResult> {
    competition_ranks(standings)
        .into_iter()
        .zip(standings)
        .take_while(|(rank, _)| *rank <= PODIUM_MAX_RANK)
        .map(|(rank, entry)| CreateAwardResult {
            rank,
            player_id: entry.player_id,
            value: entry.value,
            demos_counted: i32::try_from(entry.demos_counted).unwrap_or(i32::MAX),
        })
        .collect()
}

/// Shape validation shared by the creation and rename paths.
fn validate_award_shape(
    name: &str,
    stat_key: &str,
    color: Option<&str>,
    min_qualifier: Option<&MinQualifier>,
) -> Result<(), DomainError> {
    let mut errors = ValidationError::new();

    let trimmed = name.trim();
    if trimmed.is_empty() {
        errors.add(FieldError::required("name"));
    } else if trimmed.len() > 64 {
        errors.add(FieldError::length("name", 1, 64));
    }

    if stat_key.trim().is_empty() {
        errors.add(FieldError::required("stat_key"));
    } else if stat_key.len() > 128 {
        errors.add(FieldError::length("stat_key", 1, 128));
    }

    if let Some(color) = color
        && !is_hex_color(color)
    {
        errors.add(FieldError::format("color", "#rrggbb"));
    }

    if let Some(q) = min_qualifier
        && q.value < 1
    {
        errors.add(FieldError::new(
            "min_qualifier_value",
            "min_qualifier_value must be at least 1",
            "range",
        ));
    }

    errors.into_result(()).map_err(DomainError::Validation)
}

/// Whether a string is a `#rrggbb` hex color.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(value: f64) -> LeaderboardEntry {
        LeaderboardEntry {
            player_id: PlayerId::new(),
            display_name: "p".to_string(),
            avatar_url: None,
            value,
            demos_counted: 1,
        }
    }

    #[test]
    fn podium_shares_ranks_on_ties() {
        // 10, 10, 8, 8, 7, 5 → ranks 1,1,3,3,5(cut)
        let standings = [
            entry(10.0),
            entry(10.0),
            entry(8.0),
            entry(8.0),
            entry(7.0),
            entry(5.0),
        ];
        let podium = podium_from_standings(&standings);
        let ranks: Vec<i32> = podium.iter().map(|r| r.rank).collect();
        assert_eq!(ranks, vec![1, 1, 3, 3]);
    }

    #[test]
    fn podium_keeps_tied_third_place_rows() {
        let standings = [entry(9.0), entry(8.0), entry(7.0), entry(7.0), entry(6.0)];
        let podium = podium_from_standings(&standings);
        let ranks: Vec<i32> = podium.iter().map(|r| r.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3, 3]);
    }

    #[test]
    fn podium_of_empty_standings_is_empty() {
        assert!(podium_from_standings(&[]).is_empty());
    }

    #[test]
    fn shape_validation_rules() {
        assert!(validate_award_shape("Swag 7", "kills.weapon.mag7", None, None).is_ok());
        assert!(validate_award_shape("", "kills", None, None).is_err());
        assert!(validate_award_shape("x".repeat(65).as_str(), "kills", None, None).is_err());
        assert!(validate_award_shape("ok", "", None, None).is_err());
        assert!(validate_award_shape("ok", "kills", Some("#12AB34"), None).is_ok());
        assert!(validate_award_shape("ok", "kills", Some("red"), None).is_err());
        assert!(validate_award_shape("ok", "kills", Some("#12AB3"), None).is_err());
        let bad_q = MinQualifier {
            qualifier_type: crate::entities::award::MinQualifierType::Rounds,
            value: 0,
        };
        assert!(validate_award_shape("ok", "kills", None, Some(&bad_q)).is_err());
    }
}
