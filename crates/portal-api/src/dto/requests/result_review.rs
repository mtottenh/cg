//! Result review request DTOs.

use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

/// Request for admin review decision.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminReviewDecisionRequest {
    /// Optional notes explaining the decision.
    #[validate(length(max = 1000))]
    pub notes: Option<String>,
}
