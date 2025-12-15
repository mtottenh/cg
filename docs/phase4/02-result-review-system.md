# Phase 4.3-4.4: Result Review System

> **Status**: Design Phase
> **Dependencies**: Phase 4.1-4.2 (Demo Integration)
> **Related**: [00-overview.md](./00-overview.md), [01-demo-integration.md](./01-demo-integration.md)

---

## Overview

This document covers the second half of Phase 4:
- **4.3**: Result Review entity and service
- **4.4**: Review workflow integration with match completion saga

The Result Review System handles validation discrepancies between claimed results and demo evidence, routing issues through appropriate approval workflows.

---

## Review Triggers

When a result is submitted with demo evidence, validation may detect issues requiring human review:

| Trigger | Condition | Required Action |
|---------|-----------|-----------------|
| **Roster Mismatch** | Demo contains players not on either team's registered roster | Both captains must acknowledge |
| **Score Mismatch** | Demo final score differs from claimed score | League admin approval required |
| **Winner Mismatch** | Demo winner differs from claimed winner | League admin approval required |

### Trigger Priority

If multiple triggers apply:
1. Score/Winner mismatch always requires admin approval
2. Roster mismatch alone can be resolved by captains
3. If roster mismatch + score/winner mismatch: captains acknowledge first, then escalate to admin

---

## Review States

```
ResultReviewStatus:
  - pending_acknowledgment    # Roster mismatch only, waiting for both captains
  - pending_admin_review      # Score/winner mismatch, waiting for admin
  - acknowledged              # Both captains acknowledged roster mismatch
  - approved                  # Admin approved despite mismatch
  - rejected                  # Admin rejected the result
```

### State Transitions

```
                                ┌──────────────────────┐
                                │     VALIDATION       │
                                │     TRIGGERED        │
                                └──────────┬───────────┘
                                           │
                    ┌──────────────────────┴──────────────────────┐
                    │                                              │
                    ▼                                              ▼
    ┌───────────────────────────┐              ┌───────────────────────────┐
    │  Roster Mismatch Only     │              │  Score/Winner Mismatch    │
    │                           │              │  (with or without roster) │
    └─────────────┬─────────────┘              └─────────────┬─────────────┘
                  │                                          │
                  ▼                                          ▼
    ┌───────────────────────────┐              ┌───────────────────────────┐
    │  pending_acknowledgment   │              │  pending_admin_review     │
    └─────────────┬─────────────┘              └─────────────┬─────────────┘
                  │                                          │
                  │ Both captains                            │ Admin decision
                  │ acknowledge                              │
                  ▼                                          │
    ┌───────────────────────────┐                           │
    │  acknowledged             │                           │
    └─────────────┬─────────────┘                           │
                  │                                          │
                  │ Check for                               │
                  │ score/winner                            │
                  │ mismatch                                │
                  ▼                                          │
        ┌─────────────────┐                                 │
        │ Score/Winner    │  Yes                            │
        │ mismatch too?   │────────────────────────────────▶│
        └────────┬────────┘                                 │
                 │ No                                       │
                 ▼                                          ▼
    ┌───────────────────────────┐    ┌───────────────────────────┐
    │  CONTINUE TO COMPLETION   │    │  Admin approves/rejects   │
    └───────────────────────────┘    └─────────────┬─────────────┘
                                                   │
                                    ┌──────────────┴──────────────┐
                                    ▼                              ▼
                    ┌───────────────────────────┐  ┌───────────────────────────┐
                    │  approved                 │  │  rejected                 │
                    │  → Continue completion    │  │  → Match returns to       │
                    │                           │  │    in_progress            │
                    └───────────────────────────┘  └───────────────────────────┘
```

---

## ResultReview Entity

```rust
// portal-domain/src/entities/result_review.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{
    DemoMatchLinkId, ResultClaimId, ResultReviewId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use super::demo_validation::{DemoValidationResult, UnrecognizedPlayer};

/// Status of a result review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "result_review_status", rename_all = "snake_case")]
pub enum ResultReviewStatus {
    /// Roster mismatch only, waiting for both captains to acknowledge.
    PendingAcknowledgment,

    /// Score or winner mismatch, waiting for admin review.
    PendingAdminReview,

    /// Both captains have acknowledged the roster mismatch.
    Acknowledged,

    /// Admin has approved the result despite mismatches.
    Approved,

    /// Admin has rejected the result.
    Rejected,
}

impl ResultReviewStatus {
    /// Returns true if review is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    /// Returns true if review is pending any action.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingAcknowledgment | Self::PendingAdminReview)
    }
}

/// A review record for a result claim with validation issues.
#[derive(Debug, Clone)]
pub struct ResultReview {
    pub id: ResultReviewId,
    pub result_claim_id: ResultClaimId,
    pub match_id: TournamentMatchId,

    // Review triggers
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,

    // Demo validation details
    pub demo_link_id: Option<DemoMatchLinkId>,
    pub validation_result: Option<DemoValidationResult>,
    pub unrecognized_players: Vec<UnrecognizedPlayer>,

    // Status tracking
    pub status: ResultReviewStatus,

    // Captain acknowledgments
    pub captain1_registration_id: TournamentRegistrationId,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    pub captain1_acknowledged_by_user_id: Option<UserId>,

    pub captain2_registration_id: TournamentRegistrationId,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,
    pub captain2_acknowledged_by_user_id: Option<UserId>,

    // Admin resolution
    pub reviewed_by_user_id: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResultReview {
    /// Create a new review for roster mismatch.
    pub fn for_roster_mismatch(
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation_result: DemoValidationResult,
        unrecognized_players: Vec<UnrecognizedPlayer>,
        captain1_registration_id: TournamentRegistrationId,
        captain2_registration_id: TournamentRegistrationId,
    ) -> Self {
        Self {
            id: ResultReviewId::new(),
            result_claim_id,
            match_id,
            roster_mismatch: true,
            score_mismatch: false,
            winner_mismatch: false,
            demo_link_id,
            validation_result: Some(validation_result),
            unrecognized_players,
            status: ResultReviewStatus::PendingAcknowledgment,
            captain1_registration_id,
            captain1_acknowledged: false,
            captain1_acknowledged_at: None,
            captain1_acknowledged_by_user_id: None,
            captain2_registration_id,
            captain2_acknowledged: false,
            captain2_acknowledged_at: None,
            captain2_acknowledged_by_user_id: None,
            reviewed_by_user_id: None,
            reviewed_at: None,
            admin_notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Create a new review for score/winner mismatch.
    pub fn for_score_mismatch(
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation_result: DemoValidationResult,
        score_mismatch: bool,
        winner_mismatch: bool,
        unrecognized_players: Vec<UnrecognizedPlayer>,
        captain1_registration_id: TournamentRegistrationId,
        captain2_registration_id: TournamentRegistrationId,
    ) -> Self {
        Self {
            id: ResultReviewId::new(),
            result_claim_id,
            match_id,
            roster_mismatch: !unrecognized_players.is_empty(),
            score_mismatch,
            winner_mismatch,
            demo_link_id,
            validation_result: Some(validation_result),
            unrecognized_players,
            status: ResultReviewStatus::PendingAdminReview,
            captain1_registration_id,
            captain1_acknowledged: false,
            captain1_acknowledged_at: None,
            captain1_acknowledged_by_user_id: None,
            captain2_registration_id,
            captain2_acknowledged: false,
            captain2_acknowledged_at: None,
            captain2_acknowledged_by_user_id: None,
            reviewed_by_user_id: None,
            reviewed_at: None,
            admin_notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Check if both captains have acknowledged.
    pub fn both_captains_acknowledged(&self) -> bool {
        self.captain1_acknowledged && self.captain2_acknowledged
    }

    /// Check if this review requires admin action.
    pub fn requires_admin(&self) -> bool {
        self.score_mismatch || self.winner_mismatch
    }
}
```

---

## Database Migration

**Migration: `0042_result_reviews.sql`**

```sql
-- Result Review System
-- Tracks validation issues requiring human review before match completion

CREATE TYPE result_review_status AS ENUM (
    'pending_acknowledgment',
    'pending_admin_review',
    'acknowledged',
    'approved',
    'rejected'
);

CREATE TABLE result_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    result_claim_id UUID NOT NULL REFERENCES result_claims(id) ON DELETE CASCADE,
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Review triggers
    roster_mismatch BOOLEAN NOT NULL DEFAULT false,
    score_mismatch BOOLEAN NOT NULL DEFAULT false,
    winner_mismatch BOOLEAN NOT NULL DEFAULT false,

    -- Demo validation details
    demo_link_id UUID REFERENCES demo_match_links(id),
    validation_result JSONB,
    unrecognized_players JSONB NOT NULL DEFAULT '[]',

    -- Status
    status result_review_status NOT NULL,

    -- Captain 1 acknowledgment
    captain1_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    captain1_acknowledged BOOLEAN NOT NULL DEFAULT false,
    captain1_acknowledged_at TIMESTAMPTZ,
    captain1_acknowledged_by_user_id UUID REFERENCES users(id),

    -- Captain 2 acknowledgment
    captain2_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    captain2_acknowledged BOOLEAN NOT NULL DEFAULT false,
    captain2_acknowledged_at TIMESTAMPTZ,
    captain2_acknowledged_by_user_id UUID REFERENCES users(id),

    -- Admin resolution
    reviewed_by_user_id UUID REFERENCES users(id),
    reviewed_at TIMESTAMPTZ,
    admin_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_captain_acknowledgment CHECK (
        (NOT captain1_acknowledged OR captain1_acknowledged_at IS NOT NULL)
        AND (NOT captain2_acknowledged OR captain2_acknowledged_at IS NOT NULL)
    ),
    CONSTRAINT valid_admin_review CHECK (
        (status NOT IN ('approved', 'rejected')) OR reviewed_by_user_id IS NOT NULL
    )
);

-- Indexes
CREATE INDEX idx_result_reviews_match ON result_reviews(match_id);
CREATE INDEX idx_result_reviews_claim ON result_reviews(result_claim_id);
CREATE INDEX idx_result_reviews_status ON result_reviews(status);

-- Partial index for pending reviews (admin queue)
CREATE INDEX idx_result_reviews_pending_admin ON result_reviews(created_at)
    WHERE status = 'pending_admin_review';

-- Partial index for pending acknowledgments
CREATE INDEX idx_result_reviews_pending_ack ON result_reviews(created_at)
    WHERE status = 'pending_acknowledgment';

-- Trigger for updated_at
CREATE TRIGGER set_result_reviews_updated_at
    BEFORE UPDATE ON result_reviews
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE result_reviews IS
    'Tracks validation issues requiring human review before match completion';
COMMENT ON COLUMN result_reviews.roster_mismatch IS
    'Demo contains players not on either team registered roster';
COMMENT ON COLUMN result_reviews.score_mismatch IS
    'Demo final score differs from claimed score';
COMMENT ON COLUMN result_reviews.winner_mismatch IS
    'Demo winner differs from claimed winner';
```

---

## Repository

```rust
// portal-domain/src/repositories/result_review.rs

use async_trait::async_trait;
use crate::entities::ResultReview;
use crate::entities::result_review::ResultReviewStatus;
use crate::ids::{ResultClaimId, ResultReviewId, TournamentMatchId, UserId};
use crate::errors::DomainError;

#[async_trait]
pub trait ResultReviewRepository: Send + Sync + 'static {
    /// Insert a new review.
    async fn insert(&self, review: &ResultReview) -> Result<(), DomainError>;

    /// Find review by ID.
    async fn find_by_id(&self, id: ResultReviewId) -> Result<Option<ResultReview>, DomainError>;

    /// Find review by result claim ID.
    async fn find_by_claim_id(
        &self,
        claim_id: ResultClaimId,
    ) -> Result<Option<ResultReview>, DomainError>;

    /// Find review by match ID.
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError>;

    /// Find all pending reviews for admin queue.
    async fn find_pending_admin_reviews(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResultReview>, DomainError>;

    /// Count pending admin reviews.
    async fn count_pending_admin_reviews(&self) -> Result<i64, DomainError>;

    /// Update captain acknowledgment.
    async fn update_captain_acknowledgment(
        &self,
        id: ResultReviewId,
        captain_side: i32, // 1 or 2
        acknowledged_by_user_id: UserId,
    ) -> Result<(), DomainError>;

    /// Update status after both captains acknowledge.
    async fn update_status(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
    ) -> Result<(), DomainError>;

    /// Record admin resolution.
    async fn resolve(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
        reviewed_by_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<(), DomainError>;
}
```

---

## ResultReviewService

```rust
// portal-domain/src/services/tournament/result_review.rs

use std::sync::Arc;
use chrono::Utc;

use crate::entities::{ResultReview, ResultReviewStatus};
use crate::entities::demo_validation::{DemoValidationResult, UnrecognizedPlayer};
use crate::errors::DomainError;
use crate::ids::{
    DemoMatchLinkId, ResultClaimId, ResultReviewId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};
use crate::repositories::{ResultReviewRepository, TournamentMatchRepository};

pub struct ResultReviewService<RRR, MR>
where
    RRR: ResultReviewRepository,
    MR: TournamentMatchRepository,
{
    review_repo: Arc<RRR>,
    match_repo: Arc<MR>,
}

impl<RRR, MR> ResultReviewService<RRR, MR>
where
    RRR: ResultReviewRepository,
    MR: TournamentMatchRepository,
{
    pub fn new(review_repo: Arc<RRR>, match_repo: Arc<MR>) -> Self {
        Self { review_repo, match_repo }
    }

    /// Create a review from validation result.
    pub async fn create_from_validation(
        &self,
        result_claim_id: ResultClaimId,
        match_id: TournamentMatchId,
        demo_link_id: Option<DemoMatchLinkId>,
        validation: &DemoValidationResult,
        captain1_reg_id: TournamentRegistrationId,
        captain2_reg_id: TournamentRegistrationId,
    ) -> Result<Option<ResultReview>, DomainError> {
        // Extract unrecognized players from warnings
        let unrecognized = Self::extract_unrecognized_players(validation);

        let score_mismatch = validation.has_score_mismatch();
        let winner_mismatch = validation.has_winner_mismatch();
        let roster_mismatch = validation.has_roster_mismatch() || !unrecognized.is_empty();

        // No issues = no review needed
        if !score_mismatch && !winner_mismatch && !roster_mismatch {
            return Ok(None);
        }

        let review = if score_mismatch || winner_mismatch {
            // Score/winner mismatch goes directly to admin
            ResultReview::for_score_mismatch(
                result_claim_id,
                match_id,
                demo_link_id,
                validation.clone(),
                score_mismatch,
                winner_mismatch,
                unrecognized,
                captain1_reg_id,
                captain2_reg_id,
            )
        } else {
            // Roster mismatch only - captains can acknowledge
            ResultReview::for_roster_mismatch(
                result_claim_id,
                match_id,
                demo_link_id,
                validation.clone(),
                unrecognized,
                captain1_reg_id,
                captain2_reg_id,
            )
        };

        self.review_repo.insert(&review).await?;

        Ok(Some(review))
    }

    /// Captain acknowledges the roster mismatch.
    pub async fn acknowledge(
        &self,
        review_id: ResultReviewId,
        user_id: UserId,
        registration_id: TournamentRegistrationId,
    ) -> Result<ResultReview, DomainError> {
        let mut review = self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))?;

        // Verify status allows acknowledgment
        if review.status != ResultReviewStatus::PendingAcknowledgment {
            return Err(DomainError::InvalidReviewState(
                review.status,
                "Cannot acknowledge review not pending acknowledgment".to_string(),
            ));
        }

        // Determine which captain is acknowledging
        let captain_side = if registration_id == review.captain1_registration_id {
            1
        } else if registration_id == review.captain2_registration_id {
            2
        } else {
            return Err(DomainError::NotAuthorized(
                "User is not a captain for this match".to_string(),
            ));
        };

        // Check not already acknowledged
        let already_acked = match captain_side {
            1 => review.captain1_acknowledged,
            2 => review.captain2_acknowledged,
            _ => unreachable!(),
        };
        if already_acked {
            return Err(DomainError::ReviewAlreadyAcknowledged(captain_side));
        }

        // Record acknowledgment
        self.review_repo
            .update_captain_acknowledgment(review_id, captain_side, user_id)
            .await?;

        // Refresh review
        review = self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))?;

        // Check if both captains have now acknowledged
        if review.both_captains_acknowledged() {
            if review.requires_admin() {
                // Escalate to admin review
                self.review_repo
                    .update_status(review_id, ResultReviewStatus::PendingAdminReview)
                    .await?;
            } else {
                // No admin needed, mark as acknowledged (complete)
                self.review_repo
                    .update_status(review_id, ResultReviewStatus::Acknowledged)
                    .await?;
            }

            // Refresh again
            review = self.review_repo
                .find_by_id(review_id)
                .await?
                .ok_or(DomainError::ResultReviewNotFound(review_id))?;
        }

        Ok(review)
    }

    /// Admin approves the result despite mismatches.
    pub async fn approve(
        &self,
        review_id: ResultReviewId,
        admin_user_id: UserId,
        notes: Option<String>,
    ) -> Result<ResultReview, DomainError> {
        let review = self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))?;

        // Verify status allows approval
        if review.status != ResultReviewStatus::PendingAdminReview {
            return Err(DomainError::InvalidReviewState(
                review.status,
                "Cannot approve review not pending admin review".to_string(),
            ));
        }

        self.review_repo
            .resolve(review_id, ResultReviewStatus::Approved, admin_user_id, notes)
            .await?;

        self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))
    }

    /// Admin rejects the result.
    pub async fn reject(
        &self,
        review_id: ResultReviewId,
        admin_user_id: UserId,
        notes: Option<String>,
    ) -> Result<ResultReview, DomainError> {
        let review = self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))?;

        // Verify status allows rejection
        if review.status != ResultReviewStatus::PendingAdminReview {
            return Err(DomainError::InvalidReviewState(
                review.status,
                "Cannot reject review not pending admin review".to_string(),
            ));
        }

        self.review_repo
            .resolve(review_id, ResultReviewStatus::Rejected, admin_user_id, notes)
            .await?;

        self.review_repo
            .find_by_id(review_id)
            .await?
            .ok_or(DomainError::ResultReviewNotFound(review_id))
    }

    /// Get review by match ID.
    pub async fn get_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        self.review_repo.find_by_match_id(match_id).await
    }

    /// List pending admin reviews.
    pub async fn list_pending_reviews(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ResultReview>, i64), DomainError> {
        let reviews = self.review_repo
            .find_pending_admin_reviews(limit, offset)
            .await?;
        let total = self.review_repo.count_pending_admin_reviews().await?;
        Ok((reviews, total))
    }

    // Helper to extract unrecognized players from validation warnings
    fn extract_unrecognized_players(validation: &DemoValidationResult) -> Vec<UnrecognizedPlayer> {
        // In practice, we'd parse the warnings or have the validation return structured data
        // For now, return empty - the actual implementation would use proper structured data
        Vec::new()
    }
}
```

---

## API Endpoints

### GET `/v1/matches/{match_id}/result-review`

Get the review status for a match.

```rust
/// Get result review status for a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/result-review",
    params(
        ("match_id" = String, Path, description = "Match ID"),
    ),
    responses(
        (status = 200, description = "Review found", body = DataResponse<ResultReviewResponse>),
        (status = 404, description = "No review for this match"),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn get_result_review(
    State(state): State<AppState>,
    Path(match_id): Path<TournamentMatchId>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let review = state
        .result_review_service
        .get_for_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No review exists for this match"))?;

    Ok(Json(DataResponse::new(review.into())))
}
```

### POST `/v1/matches/{match_id}/result-review/acknowledge`

Captain acknowledges the roster mismatch.

```rust
/// Acknowledge a result review (captain).
#[utoipa::path(
    post,
    path = "/v1/matches/{match_id}/result-review/acknowledge",
    responses(
        (status = 200, description = "Acknowledged", body = DataResponse<ResultReviewResponse>),
        (status = 400, description = "Cannot acknowledge", body = ApiError),
        (status = 403, description = "Not a captain", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn acknowledge_result_review(
    State(state): State<AppState>,
    Path(match_id): Path<TournamentMatchId>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    // Get review for match
    let review = state
        .result_review_service
        .get_for_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No review exists for this match"))?;

    // Get user's registration for this match
    let registration_id = get_user_registration_for_match(&state, &user, match_id).await?;

    let updated = state
        .result_review_service
        .acknowledge(review.id, user.user_id, registration_id)
        .await?;

    Ok(Json(DataResponse::new(updated.into())))
}
```

### GET `/v1/admin/result-reviews`

List pending reviews for admin queue.

```rust
/// List pending result reviews (admin).
#[utoipa::path(
    get,
    path = "/v1/admin/result-reviews",
    params(
        ("limit" = Option<i64>, Query, description = "Max results"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination"),
    ),
    responses(
        (status = 200, description = "Pending reviews", body = PaginatedResponse<ResultReviewResponse>),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn list_pending_reviews(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    perm_checker: PermissionChecker,
    Query(params): Query<PaginationParams>,
) -> ApiResult<Json<PaginatedResponse<ResultReviewResponse>>> {
    perm_checker
        .require_permission(&user, permissions::tournament::DISPUTES_RESOLVE)
        .await?;

    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let (reviews, total) = state
        .result_review_service
        .list_pending_reviews(limit, offset)
        .await?;

    Ok(Json(PaginatedResponse::new(
        reviews.into_iter().map(Into::into).collect(),
        total,
        limit,
        offset,
    )))
}
```

### POST `/v1/admin/result-reviews/{id}/approve`

Admin approves the result.

```rust
/// Approve a result review (admin).
#[utoipa::path(
    post,
    path = "/v1/admin/result-reviews/{id}/approve",
    params(
        ("id" = String, Path, description = "Review ID"),
    ),
    request_body = AdminReviewDecisionRequest,
    responses(
        (status = 200, description = "Approved", body = DataResponse<ResultReviewResponse>),
        (status = 400, description = "Cannot approve", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn approve_result_review(
    State(state): State<AppState>,
    Path(id): Path<ResultReviewId>,
    AuthenticatedUser(user): AuthenticatedUser,
    perm_checker: PermissionChecker,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    perm_checker
        .require_permission(&user, permissions::tournament::DISPUTES_RESOLVE)
        .await?;

    let review = state
        .result_review_service
        .approve(id, user.user_id, req.notes)
        .await?;

    // Trigger match completion continuation
    state
        .match_completion_saga
        .continue_after_review(review.match_id)
        .await?;

    Ok(Json(DataResponse::new(review.into())))
}
```

### POST `/v1/admin/result-reviews/{id}/reject`

Admin rejects the result.

```rust
/// Reject a result review (admin).
#[utoipa::path(
    post,
    path = "/v1/admin/result-reviews/{id}/reject",
    params(
        ("id" = String, Path, description = "Review ID"),
    ),
    request_body = AdminReviewDecisionRequest,
    responses(
        (status = 200, description = "Rejected", body = DataResponse<ResultReviewResponse>),
        (status = 400, description = "Cannot reject", body = ApiError),
        (status = 404, description = "Review not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "result_reviews"
)]
pub async fn reject_result_review(
    State(state): State<AppState>,
    Path(id): Path<ResultReviewId>,
    AuthenticatedUser(user): AuthenticatedUser,
    perm_checker: PermissionChecker,
    ValidatedJson(req): ValidatedJson<AdminReviewDecisionRequest>,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    perm_checker
        .require_permission(&user, permissions::tournament::DISPUTES_RESOLVE)
        .await?;

    let review = state
        .result_review_service
        .reject(id, user.user_id, req.notes)
        .await?;

    // Return match to in_progress state
    state
        .match_lifecycle_service
        .transition_to(review.match_id, MatchStatus::InProgress)
        .await?;

    Ok(Json(DataResponse::new(review.into())))
}
```

---

## DTOs

```rust
// portal-api/src/dto/responses/result_review.rs

#[derive(Debug, Serialize, ToSchema)]
pub struct ResultReviewResponse {
    pub id: String,
    pub result_claim_id: String,
    pub match_id: String,

    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,

    pub demo_link_id: Option<String>,
    pub validation_result: Option<DemoValidationResultResponse>,
    pub unrecognized_players: Vec<UnrecognizedPlayerResponse>,

    pub status: String,

    pub captain1_registration_id: String,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,

    pub captain2_registration_id: String,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,

    pub reviewed_by_user_id: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnrecognizedPlayerResponse {
    pub steam_id: String,
    pub player_name: String,
    pub team_side: String,
    pub registration_side: i32,
}

// portal-api/src/dto/requests/result_review.rs

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminReviewDecisionRequest {
    #[validate(length(max = 1000))]
    pub notes: Option<String>,
}
```

---

## Match Completion Saga Integration

The match completion saga is extended with a demo validation step:

```rust
// portal-domain/src/services/tournament/match_completion.rs (extended)

impl MatchCompletionSaga {
    /// Execute the match completion saga.
    pub async fn execute(&self, match_id: TournamentMatchId) -> Result<(), DomainError> {
        // Step 1: Validate result is confirmed
        self.step_validate_result_confirmed(match_id).await?;

        // Step 2: Validate demos (NEW)
        let review = self.step_validate_demos(match_id).await?;

        // If review was created, pause saga until review is resolved
        if let Some(review) = review {
            if review.status.is_pending() {
                // Store saga state for continuation
                self.store_pending_review(match_id, review.id).await?;
                return Ok(()); // Saga paused
            }

            // Review is already resolved (acknowledged or approved)
            if review.status == ResultReviewStatus::Rejected {
                return Err(DomainError::ResultRejectedByAdmin(review.id));
            }
        }

        // Step 3: Update match with winner/loser
        self.step_finalize_match(match_id).await?;

        // Step 4: Advance winner
        self.step_advance_winner(match_id).await?;

        // Step 5: Route loser (if double elim)
        self.step_route_loser(match_id).await?;

        // Step 6: Update standings
        self.step_update_standings(match_id).await?;

        // Step 7: Check completion
        self.step_check_completion(match_id).await?;

        Ok(())
    }

    /// Validate demos against the confirmed result.
    async fn step_validate_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        // Get the confirmed result claim
        let claim = self.result_repo
            .find_confirmed_by_match_id(match_id)
            .await?
            .ok_or(DomainError::NoConfirmedResult(match_id))?;

        // If no demo links, skip validation
        if claim.demo_link_ids.is_empty() {
            return Ok(None);
        }

        // Get match details for participant steam IDs
        let (team1_steam_ids, team2_steam_ids) =
            self.get_participant_steam_ids(match_id).await?;

        // Validate each linked demo
        for demo_link_id in &claim.demo_link_ids {
            let link = self.demo_link_repo
                .find_by_id(*demo_link_id)
                .await?
                .ok_or(DomainError::DemoMatchLinkNotFound(*demo_link_id))?;

            // Find matching game result
            let game_result = claim.game_results
                .iter()
                .find(|g| link.game_number == Some(g.game_number))
                .or_else(|| claim.game_results.first())
                .ok_or(DomainError::NoGameResultForDemo(*demo_link_id))?;

            // Validate
            let validation = self.demo_service
                .validate_against_result(
                    link.demo_id,
                    game_result,
                    &team1_steam_ids,
                    &team2_steam_ids,
                )
                .await?;

            // Store validation result
            self.demo_link_repo
                .update_validation(*demo_link_id, validation.clone())
                .await?;

            // Check if review needed
            if !validation.is_valid || validation.has_roster_mismatch() {
                let review = self.result_review_service
                    .create_from_validation(
                        claim.id,
                        match_id,
                        Some(*demo_link_id),
                        &validation,
                        claim.submitted_by_registration_id,
                        self.get_opponent_registration_id(match_id, claim.submitted_by_registration_id).await?,
                    )
                    .await?;

                if review.is_some() {
                    return Ok(review);
                }
            }
        }

        Ok(None)
    }

    /// Continue saga after review is resolved.
    pub async fn continue_after_review(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        // Check review status
        let review = self.result_review_service
            .get_for_match(match_id)
            .await?
            .ok_or(DomainError::NoReviewForMatch(match_id))?;

        if review.status == ResultReviewStatus::Rejected {
            return Err(DomainError::ResultRejectedByAdmin(review.id));
        }

        if !review.status.is_terminal() && review.status != ResultReviewStatus::Acknowledged {
            return Err(DomainError::ReviewStillPending(review.id));
        }

        // Continue saga from step 3
        self.step_finalize_match(match_id).await?;
        self.step_advance_winner(match_id).await?;
        self.step_route_loser(match_id).await?;
        self.step_update_standings(match_id).await?;
        self.step_check_completion(match_id).await?;

        Ok(())
    }
}
```

---

## Routes

```rust
// portal-api/src/routes/matches.rs

.route(
    "/v1/matches/:match_id/result-review",
    get(handlers::result_reviews::get_result_review),
)
.route(
    "/v1/matches/:match_id/result-review/acknowledge",
    post(handlers::result_reviews::acknowledge_result_review),
)

// portal-api/src/routes/admin.rs

.route(
    "/v1/admin/result-reviews",
    get(handlers::result_reviews::list_pending_reviews),
)
.route(
    "/v1/admin/result-reviews/:id",
    get(handlers::result_reviews::get_result_review_by_id),
)
.route(
    "/v1/admin/result-reviews/:id/approve",
    post(handlers::result_reviews::approve_result_review),
)
.route(
    "/v1/admin/result-reviews/:id/reject",
    post(handlers::result_reviews::reject_result_review),
)
```

---

## Integration Tests

### Category F: Review System (5 tests)

```rust
#[tokio::test]
async fn test_result_with_roster_mismatch_requires_acknowledgment() {
    // Submit result with demo showing unrecognized players
    // Verify review created with status pending_acknowledgment
}

#[tokio::test]
async fn test_result_with_score_mismatch_requires_admin() {
    // Submit result with demo showing different score
    // Verify review created with status pending_admin_review
}

#[tokio::test]
async fn test_captain_acknowledgment_flow() {
    // Create roster mismatch review
    // Have both captains acknowledge
    // Verify status transitions to acknowledged
}

#[tokio::test]
async fn test_admin_approval_flow() {
    // Create score mismatch review
    // Admin approves
    // Verify match completion continues
}

#[tokio::test]
async fn test_admin_rejection_returns_match_to_progress() {
    // Create score mismatch review
    // Admin rejects
    // Verify match returns to in_progress status
}
```

---

## Acceptance Criteria

### 4.3 Result Review System

- [ ] `ResultReview` entity captures all review triggers and state
- [ ] Migration creates `result_reviews` table with proper constraints
- [ ] Repository supports all CRUD operations and queries
- [ ] Service handles captain acknowledgment with proper validation
- [ ] Service handles admin approval/rejection
- [ ] API endpoints implemented with OpenAPI docs

### 4.4 Review Workflow Integration

- [ ] Match completion saga includes demo validation step
- [ ] Saga pauses when review is needed
- [ ] Saga continues after review is resolved
- [ ] Rejected reviews return match to in_progress
- [ ] All integration tests pass
