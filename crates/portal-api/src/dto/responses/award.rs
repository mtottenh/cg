//! Award, standings, leaderboard, and stat-catalog response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::award::{Award, AwardResult, AwardTemplate};
use portal_domain::repositories::{LeaderboardEntry, PlayerTrophy};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// A per-game award template (organizer picker entry).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AwardTemplateResponse {
    pub id: Uuid,
    pub game_id: Uuid,
    /// Stable key used to instantiate the template (e.g. `swag7`).
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    /// mdi icon name.
    pub icon: Option<String>,
    /// `#rrggbb` accent color.
    pub color: Option<String>,
    pub stat_key: String,
    /// `sum` | `max_single_demo` | `avg_per_demo`.
    pub aggregation: String,
    /// `desc` | `asc`.
    pub direction: String,
    /// `matches` | `rounds`, when a qualifier is set.
    pub min_qualifier_type: Option<String>,
    pub min_qualifier_value: Option<i32>,
}

impl From<AwardTemplate> for AwardTemplateResponse {
    fn from(t: AwardTemplate) -> Self {
        Self {
            id: t.id.as_uuid(),
            game_id: t.game_id.as_uuid(),
            key: t.key,
            name: t.name,
            description: t.description,
            icon: t.icon,
            color: t.color,
            stat_key: t.stat_key,
            aggregation: t.aggregation.to_string(),
            direction: t.direction.to_string(),
            min_qualifier_type: t.min_qualifier.map(|q| q.qualifier_type.to_string()),
            min_qualifier_value: t.min_qualifier.map(|q| q.value),
        }
    }
}

/// An award instance scoped to a tournament or league season.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AwardResponse {
    pub id: Uuid,
    /// `tournament` | `league_season`.
    pub scope_type: String,
    /// Tournament id or league-season id depending on `scope_type`.
    pub scope_id: Uuid,
    pub game_id: Uuid,
    /// Source template id, when created from one.
    pub template_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub stat_key: String,
    /// `sum` | `max_single_demo` | `avg_per_demo`.
    pub aggregation: String,
    /// `desc` | `asc`.
    pub direction: String,
    /// `matches` | `rounds`, when a qualifier is set.
    pub min_qualifier_type: Option<String>,
    pub min_qualifier_value: Option<i32>,
    /// `player` (v1) | `team` (reserved).
    pub subject_type: String,
    /// `active` | `finalized` | `void`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Award> for AwardResponse {
    fn from(a: Award) -> Self {
        Self {
            id: a.id.as_uuid(),
            scope_type: a.scope_type.to_string(),
            scope_id: a.scope_id,
            game_id: a.game_id.as_uuid(),
            template_id: a.template_id.map(|id| id.as_uuid()),
            name: a.name,
            description: a.description,
            icon: a.icon,
            color: a.color,
            stat_key: a.stat_key,
            aggregation: a.aggregation.to_string(),
            direction: a.direction.to_string(),
            min_qualifier_type: a.min_qualifier.map(|q| q.qualifier_type.to_string()),
            min_qualifier_value: a.min_qualifier.map(|q| q.value),
            subject_type: a.subject_type.to_string(),
            status: a.status.to_string(),
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

/// One ranked row in award standings or a plain leaderboard.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LeaderboardEntryResponse {
    /// Competition rank (1-based; ties share a rank).
    pub rank: i32,
    pub player_id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    /// Aggregated stat value.
    pub value: f64,
    /// Distinct demos that contributed to `value`.
    pub demos_counted: i64,
}

impl LeaderboardEntryResponse {
    /// Build ranked rows from ordered leaderboard entries.
    #[must_use]
    pub fn from_entries(entries: Vec<LeaderboardEntry>) -> Vec<Self> {
        let ranks = portal_domain::services::competition_ranks(&entries);
        ranks
            .into_iter()
            .zip(entries)
            .map(|(rank, e)| Self {
                rank,
                player_id: e.player_id.as_uuid(),
                display_name: e.display_name,
                avatar_url: e.avatar_url,
                value: e.value,
                demos_counted: e.demos_counted,
            })
            .collect()
    }
}

/// Award standings: the award plus its current ranked rows.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AwardStandingsResponse {
    pub award: AwardResponse,
    pub entries: Vec<LeaderboardEntryResponse>,
}

/// A finalized podium row.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AwardResultResponse {
    pub id: Uuid,
    pub award_id: Uuid,
    /// Competition rank (1-based; ties share a rank).
    pub rank: i32,
    pub player_id: Uuid,
    pub value: f64,
    pub demos_counted: i32,
    pub finalized_at: DateTime<Utc>,
}

impl From<AwardResult> for AwardResultResponse {
    fn from(r: AwardResult) -> Self {
        Self {
            id: r.id.as_uuid(),
            award_id: r.award_id.as_uuid(),
            rank: r.rank,
            player_id: r.player_id.as_uuid(),
            value: r.value,
            demos_counted: r.demos_counted,
            finalized_at: r.finalized_at,
        }
    }
}

/// Finalization outcome: the finalized award and its podium.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FinalizedAwardResponse {
    pub award: AwardResponse,
    pub results: Vec<AwardResultResponse>,
}

/// One trophy-case entry: a finalized result with its award and the scope's
/// display name.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlayerTrophyResponse {
    pub award: AwardResponse,
    pub result: AwardResultResponse,
    /// Tournament or league-season name, when the scope still exists.
    pub scope_name: Option<String>,
}

impl From<PlayerTrophy> for PlayerTrophyResponse {
    fn from(t: PlayerTrophy) -> Self {
        Self {
            award: AwardResponse::from(t.award),
            result: AwardResultResponse::from(t.result),
            scope_name: t.scope_name,
        }
    }
}

/// One stat the game plugin can extract, for award-builder UIs.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StatCatalogEntryResponse {
    /// Stable stat key (e.g. `headshot_kills`, `kills.weapon.mag7`).
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// UI grouping (`Combat`, `Utility`, `Objective`, `Weapons`).
    pub category: String,
    /// `count` (additive) or `ratio` (per-demo; never summed).
    pub value_type: String,
    /// Longer description for tooltips.
    pub description: String,
}

impl From<portal_plugins::stats::StatDefinition> for StatCatalogEntryResponse {
    fn from(d: portal_plugins::stats::StatDefinition) -> Self {
        Self {
            key: d.key,
            label: d.label,
            category: d.category,
            value_type: match d.value_type {
                portal_plugins::stats::StatValueType::Count => "count".to_string(),
                portal_plugins::stats::StatValueType::Ratio => "ratio".to_string(),
            },
            description: d.description,
        }
    }
}
