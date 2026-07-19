//! Award and leaderboard request DTOs.

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

/// Request to create an award in a tournament or league-season scope.
///
/// Two modes:
/// - **From template**: set `template_key` (optionally `name` to rename);
///   the template supplies branding and the metric tuple.
/// - **Custom**: leave `template_key` unset and provide `name` + `stat_key`
///   (and optionally the rest of the metric tuple).
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct CreateAwardRequest {
    /// Template key to instantiate (e.g. `swag7`). Custom awards omit this.
    #[validate(length(min = 1, max = 64))]
    pub template_key: Option<String>,
    /// Award name (required for custom awards; overrides the template name).
    #[validate(length(min = 1, max = 64))]
    pub name: Option<String>,
    /// Longer description for cards/tooltips.
    #[validate(length(max = 2000))]
    pub description: Option<String>,
    /// mdi icon name.
    #[validate(length(max = 64))]
    pub icon: Option<String>,
    /// `#rrggbb` accent color.
    #[validate(length(max = 7))]
    pub color: Option<String>,
    /// Stat key from the game's stat catalog (custom awards only).
    #[validate(length(min = 1, max = 128))]
    pub stat_key: Option<String>,
    /// Aggregation: `sum` (default), `max_single_demo`, or `avg_per_demo`.
    pub aggregation: Option<String>,
    /// Direction: `desc` (default) or `asc`.
    pub direction: Option<String>,
    /// Qualifier type: `matches` or `rounds`.
    pub min_qualifier_type: Option<String>,
    /// Qualifier threshold (requires `min_qualifier_type`).
    pub min_qualifier_value: Option<i32>,
}

/// Request to update an active award's presentation. Only provided fields
/// change; the metric tuple is immutable after creation.
#[derive(Debug, Clone, Deserialize, Validate, ToSchema)]
pub struct UpdateAwardRequest {
    /// New award name.
    #[validate(length(min = 1, max = 64))]
    pub name: Option<String>,
    /// New description.
    #[validate(length(max = 2000))]
    pub description: Option<String>,
    /// New mdi icon name.
    #[validate(length(max = 64))]
    pub icon: Option<String>,
    /// New `#rrggbb` accent color.
    #[validate(length(max = 7))]
    pub color: Option<String>,
}

/// Query parameters for the plain leaderboard endpoints.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct LeaderboardQueryParams {
    /// Stat key to rank on (e.g. `headshot_kills`, `kills.weapon.mag7`).
    pub stat_key: String,
    /// Aggregation: `sum` (default), `max_single_demo`, or `avg_per_demo`.
    pub aggregation: Option<String>,
    /// Direction: `desc` (default) or `asc`.
    pub direction: Option<String>,
    /// Only rank players with at least this many counted demos.
    pub min_matches: Option<i32>,
    /// Only rank players with at least this many rounds played in scope.
    pub min_rounds: Option<i32>,
    /// Maximum rows (default 10, max 100).
    pub limit: Option<i64>,
}

/// Query parameters for award standings.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct StandingsQueryParams {
    /// Maximum rows (default 10, max 100).
    pub limit: Option<i64>,
}
