# Phase 4.3 Implementation - Result Review System

## Context

You are implementing **Phase 4.3** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This phase creates the Result Review System for handling validation discrepancies between claimed results and demo evidence.

**Prerequisites**: Phase 4.2 (Result Claim Demo Bridge) is complete.

**Design Documents**:
- `docs/phase4/00-overview.md` - Phase 4 overview
- `docs/phase4/02-result-review-system.md` - Detailed design for this batch

**Reference Files**:
- `crates/portal-domain/src/entities/result_claim.rs` - ResultClaim entity
- `crates/portal-domain/src/entities/demo_validation.rs` - DemoValidationResult (from 4.1)
- `crates/portal-db/src/adapters/tournament/` - Tournament adapter patterns

---

## Your Task

Implement the Result Review System entity, repository, service, and API endpoints.

### Goals

1. **Database Migration**: Create `result_reviews` table with `result_review_status` enum
2. **Domain Entity**: Create `ResultReview` and `ResultReviewStatus`
3. **Repository**: Create `ResultReviewRepository` trait and PostgreSQL implementation
4. **Service**: Create `ResultReviewService` with acknowledgment and resolution methods
5. **API Endpoints**: Captain acknowledgment and admin review endpoints
6. **Integration Tests**: 3 tests covering review creation and acknowledgment

---

## Implementation

### 1. Database Migration

Create `migrations/0042_result_reviews.sql`:

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
```

### 2. Domain Entity

Create `crates/portal-domain/src/entities/result_review.rs`:

```rust
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
    PendingAcknowledgment,
    PendingAdminReview,
    Acknowledged,
    Approved,
    Rejected,
}

impl ResultReviewStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingAcknowledgment | Self::PendingAdminReview)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingAcknowledgment => "pending_acknowledgment",
            Self::PendingAdminReview => "pending_admin_review",
            Self::Acknowledged => "acknowledged",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
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

    // Status
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

    pub fn both_captains_acknowledged(&self) -> bool {
        self.captain1_acknowledged && self.captain2_acknowledged
    }

    pub fn requires_admin(&self) -> bool {
        self.score_mismatch || self.winner_mismatch
    }
}
```

Update `crates/portal-domain/src/entities/mod.rs` to export the new module.

### 3. Add ResultReviewId

Add to `crates/portal-core/src/ids.rs`:

```rust
impl_id!(ResultReviewId);
```

### 4. Repository Trait

Create `crates/portal-domain/src/repositories/result_review.rs`:

```rust
use async_trait::async_trait;
use crate::entities::{ResultReview, ResultReviewStatus};
use crate::ids::{ResultClaimId, ResultReviewId, TournamentMatchId, UserId};
use crate::errors::DomainError;

#[async_trait]
pub trait ResultReviewRepository: Send + Sync + 'static {
    async fn insert(&self, review: &ResultReview) -> Result<(), DomainError>;

    async fn find_by_id(&self, id: ResultReviewId) -> Result<Option<ResultReview>, DomainError>;

    async fn find_by_claim_id(&self, claim_id: ResultClaimId) -> Result<Option<ResultReview>, DomainError>;

    async fn find_by_match_id(&self, match_id: TournamentMatchId) -> Result<Option<ResultReview>, DomainError>;

    async fn find_pending_admin_reviews(&self, limit: i64, offset: i64) -> Result<Vec<ResultReview>, DomainError>;

    async fn count_pending_admin_reviews(&self) -> Result<i64, DomainError>;

    async fn update_captain_acknowledgment(
        &self,
        id: ResultReviewId,
        captain_side: i32,
        acknowledged_by_user_id: UserId,
    ) -> Result<(), DomainError>;

    async fn update_status(&self, id: ResultReviewId, status: ResultReviewStatus) -> Result<(), DomainError>;

    async fn resolve(
        &self,
        id: ResultReviewId,
        status: ResultReviewStatus,
        reviewed_by_user_id: UserId,
        admin_notes: Option<String>,
    ) -> Result<(), DomainError>;
}
```

### 5. DB Entity

Create `crates/portal-db/src/entities/result_review.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct DbResultReview {
    pub id: Uuid,
    pub result_claim_id: Uuid,
    pub match_id: Uuid,
    pub roster_mismatch: bool,
    pub score_mismatch: bool,
    pub winner_mismatch: bool,
    pub demo_link_id: Option<Uuid>,
    pub validation_result: Option<sqlx::types::Json<serde_json::Value>>,
    pub unrecognized_players: sqlx::types::Json<Vec<serde_json::Value>>,
    pub status: String,
    pub captain1_registration_id: Uuid,
    pub captain1_acknowledged: bool,
    pub captain1_acknowledged_at: Option<DateTime<Utc>>,
    pub captain1_acknowledged_by_user_id: Option<Uuid>,
    pub captain2_registration_id: Uuid,
    pub captain2_acknowledged: bool,
    pub captain2_acknowledged_at: Option<DateTime<Utc>>,
    pub captain2_acknowledged_by_user_id: Option<Uuid>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 6. Repository Implementation

Create `crates/portal-db/src/adapters/result_review.rs`:

Implement all repository methods with proper SQL queries.

### 7. ResultReviewService

Create `crates/portal-domain/src/services/tournament/result_review.rs`:

```rust
pub struct ResultReviewService<RRR, MR> {
    review_repo: Arc<RRR>,
    match_repo: Arc<MR>,
}

impl<RRR, MR> ResultReviewService<RRR, MR>
where
    RRR: ResultReviewRepository,
    MR: TournamentMatchRepository,
{
    pub fn new(review_repo: Arc<RRR>, match_repo: Arc<MR>) -> Self;

    /// Create a review from validation result.
    pub async fn create_from_validation(...) -> Result<Option<ResultReview>, DomainError>;

    /// Captain acknowledges the roster mismatch.
    pub async fn acknowledge(...) -> Result<ResultReview, DomainError>;

    /// Admin approves the result.
    pub async fn approve(...) -> Result<ResultReview, DomainError>;

    /// Admin rejects the result.
    pub async fn reject(...) -> Result<ResultReview, DomainError>;

    /// Get review by match ID.
    pub async fn get_for_match(...) -> Result<Option<ResultReview>, DomainError>;

    /// List pending admin reviews.
    pub async fn list_pending_reviews(...) -> Result<(Vec<ResultReview>, i64), DomainError>;
}
```

### 8. API Endpoints

Create `crates/portal-api/src/handlers/result_reviews.rs`:

```rust
/// Get result review status for a match.
#[utoipa::path(get, path = "/v1/matches/{match_id}/result-review", ...)]
pub async fn get_result_review(...) -> ApiResult<...>;

/// Acknowledge a result review (captain).
#[utoipa::path(post, path = "/v1/matches/{match_id}/result-review/acknowledge", ...)]
pub async fn acknowledge_result_review(...) -> ApiResult<...>;

/// List pending result reviews (admin).
#[utoipa::path(get, path = "/v1/admin/result-reviews", ...)]
pub async fn list_pending_reviews(...) -> ApiResult<...>;

/// Get result review by ID (admin).
#[utoipa::path(get, path = "/v1/admin/result-reviews/{id}", ...)]
pub async fn get_result_review_by_id(...) -> ApiResult<...>;

/// Approve a result review (admin).
#[utoipa::path(post, path = "/v1/admin/result-reviews/{id}/approve", ...)]
pub async fn approve_result_review(...) -> ApiResult<...>;

/// Reject a result review (admin).
#[utoipa::path(post, path = "/v1/admin/result-reviews/{id}/reject", ...)]
pub async fn reject_result_review(...) -> ApiResult<...>;
```

### 9. DTOs

Create `crates/portal-api/src/dto/requests/result_review.rs`:

```rust
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminReviewDecisionRequest {
    #[validate(length(max = 1000))]
    pub notes: Option<String>,
}
```

Create `crates/portal-api/src/dto/responses/result_review.rs`:

```rust
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
```

### 10. Routes

```rust
// matches.rs
.route("/v1/matches/:match_id/result-review", get(handlers::result_reviews::get_result_review))
.route("/v1/matches/:match_id/result-review/acknowledge", post(handlers::result_reviews::acknowledge_result_review))

// admin.rs
.route("/v1/admin/result-reviews", get(handlers::result_reviews::list_pending_reviews))
.route("/v1/admin/result-reviews/:id", get(handlers::result_reviews::get_result_review_by_id))
.route("/v1/admin/result-reviews/:id/approve", post(handlers::result_reviews::approve_result_review))
.route("/v1/admin/result-reviews/:id/reject", post(handlers::result_reviews::reject_result_review))
```

### 11. DomainError Extensions

Add to domain errors:

```rust
#[error("Result review {0} not found")]
ResultReviewNotFound(ResultReviewId),

#[error("Invalid review state {0:?}: {1}")]
InvalidReviewState(ResultReviewStatus, String),

#[error("Review already acknowledged by captain {0}")]
ReviewAlreadyAcknowledged(i32),
```

---

## Tests

### Category F: Review System (3 tests for this batch)

```rust
#[tokio::test]
async fn test_result_with_roster_mismatch_requires_acknowledgment() {
    // Create match with demo showing unrecognized players
    // Submit result, trigger validation
    // Verify review created with status pending_acknowledgment
}

#[tokio::test]
async fn test_captain_acknowledgment_flow() {
    // Create roster mismatch review
    // Have captain 1 acknowledge
    // Verify still pending
    // Have captain 2 acknowledge
    // Verify status transitions to acknowledged
}

#[tokio::test]
async fn test_get_result_review_not_found() {
    // GET review for match with no review
    // Should return 404
}
```

---

## Acceptance Criteria

- [ ] Migration `0042_result_reviews.sql` created and runs successfully
- [ ] `result_review_status` enum type created
- [ ] `result_reviews` table created with all constraints and indexes
- [ ] `ResultReview` and `ResultReviewStatus` domain types created
- [ ] `ResultReviewId` added to ID types
- [ ] `ResultReviewRepository` trait defined
- [ ] PostgreSQL repository implementation complete
- [ ] `ResultReviewService` with acknowledgment/approval/rejection methods
- [ ] All 6 API endpoints implemented with OpenAPI docs
- [ ] DTOs and conversions implemented
- [ ] Routes registered
- [ ] Domain errors added
- [ ] All 3 integration tests pass
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

---

## Verification

```bash
# Run migration
sqlx migrate run

# Verify table exists
psql -c "\d result_reviews"

# Run tests
cargo test -p portal-api --test result_review_test

# Check OpenAPI
curl http://localhost:3000/api-docs/openapi.json | jq '.paths | keys | map(select(contains("result-review")))'
```
