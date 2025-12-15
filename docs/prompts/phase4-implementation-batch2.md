# Phase 4.2 Implementation - Result Claim Demo Bridge

## Context

You are implementing **Phase 4.2** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This phase creates the bridge between result claims and the demo catalog by adding `demo_link_ids` to result claims.

**Prerequisites**: Phase 4.1 (Demo Handlers & Validation) is complete.

**Design Documents**:
- `docs/phase4/00-overview.md` - Phase 4 overview
- `docs/phase4/01-demo-integration.md` - Detailed design for this batch

**Reference Files**:
- `crates/portal-domain/src/entities/result_claim.rs` - ResultClaim entity
- `crates/portal-domain/src/services/tournament/result.rs` - ResultService
- `crates/portal-api/src/dto/requests/result.rs` - Result request DTOs
- `crates/portal-db/src/adapters/tournament/result_claim.rs` - ResultClaim repository

---

## Your Task

Implement the result claim demo bridge defined in Phase 4.2.

### Goals

1. **Database Migration**: Add `demo_link_ids UUID[]` to `result_claims` table
2. **Extended ResultClaim Entity**: Add `demo_link_ids: Vec<DemoMatchLinkId>` field
3. **Extended DTOs**: Add `demo_link_ids` to request/response types
4. **Modified ResultService**: Validate demo links belong to match on submission
5. **Integration Tests**: 5 tests covering result submission with demos

---

## Implementation

### 1. Database Migration

Create `migrations/0041_result_claims_demo_links.sql`:

```sql
-- Add demo_link_ids to result_claims for demo catalog integration
-- This bridges result claims to the demo catalog without duplicating data

ALTER TABLE result_claims
ADD COLUMN demo_link_ids UUID[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN result_claims.demo_link_ids IS
    'Array of demo match link IDs from demo_match_links table. Separate from evidence_ids to maintain clean domain boundaries.';

-- Index for efficient lookup of claims by demo link
CREATE INDEX idx_result_claims_demo_links ON result_claims USING gin(demo_link_ids);

-- Add validation_result column to demo_match_links for caching validation outcomes
ALTER TABLE demo_match_links
ADD COLUMN IF NOT EXISTS validation_result JSONB;

COMMENT ON COLUMN demo_match_links.validation_result IS
    'Cached validation result comparing this demo against the claimed match result';
```

### 2. Extended ResultClaim Entity

Update `crates/portal-domain/src/entities/result_claim.rs`:

```rust
use crate::ids::{DemoMatchLinkId, EvidenceId, ResultClaimId, TournamentMatchId, TournamentRegistrationId, UserId};

pub struct ResultClaim {
    pub id: ResultClaimId,
    pub match_id: TournamentMatchId,
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,

    // Result data
    pub winner_registration_id: TournamentRegistrationId,
    pub game_results: Vec<GameResult>,

    // Evidence references (existing)
    pub evidence_ids: Vec<EvidenceId>,

    // Demo catalog references (NEW)
    pub demo_link_ids: Vec<DemoMatchLinkId>,

    // Status
    pub status: ResultClaimStatus,
    pub confirmed_by_registration_id: Option<TournamentRegistrationId>,
    pub confirmed_by_user_id: Option<UserId>,
    pub confirmed_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 3. Extended DB Entity

Update `crates/portal-db/src/entities/result_claim.rs`:

```rust
#[derive(Debug, FromRow)]
pub struct DbResultClaim {
    // ... existing fields ...
    pub demo_link_ids: Vec<Uuid>,
}
```

Update the `From<DbResultClaim>` conversion to handle `demo_link_ids`.

### 4. Extended Repository

Update `crates/portal-db/src/adapters/tournament/result_claim.rs`:

- Insert query should include `demo_link_ids`
- Select queries should include `demo_link_ids`

```sql
INSERT INTO result_claims (
    id, match_id, submitted_by_registration_id, submitted_by_user_id,
    winner_registration_id, game_results, evidence_ids, demo_link_ids,
    status, created_at, updated_at
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
```

### 5. Extended Request DTOs

Update `crates/portal-api/src/dto/requests/result.rs`:

```rust
/// Request to submit a result claim.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SubmitResultClaimRequest {
    /// The registration ID of the winning participant.
    pub winner_registration_id: String,

    /// Per-game results for series matches.
    #[validate(length(min = 1, max = 7))]
    pub game_results: Vec<GameResultInput>,

    /// Evidence IDs to attach (from evidence system).
    #[serde(default)]
    pub evidence_ids: Vec<String>,

    /// Demo match link IDs to attach (from demo catalog).
    /// These reference demos already linked to this match via demo_match_links.
    #[serde(default)]
    pub demo_link_ids: Vec<String>,
}

/// Per-game result input.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct GameResultInput {
    /// Game number in the series (1-indexed).
    pub game_number: i32,

    /// Map played.
    #[validate(length(min = 1, max = 64))]
    pub map_id: String,

    /// Score for participant 1.
    #[validate(range(min = 0, max = 100))]
    pub participant1_score: i32,

    /// Score for participant 2.
    #[validate(range(min = 0, max = 100))]
    pub participant2_score: i32,

    /// Optional: specific demo link ID for this game.
    pub demo_link_id: Option<String>,
}
```

### 6. Extended Response DTOs

Update `crates/portal-api/src/dto/responses/result.rs`:

```rust
#[derive(Debug, Serialize, ToSchema)]
pub struct ResultClaimResponse {
    // ... existing fields ...

    /// Demo link IDs attached to this claim.
    pub demo_link_ids: Vec<String>,
}
```

### 7. Extended ResultService

Update `crates/portal-domain/src/services/tournament/result.rs`:

```rust
impl<RCR, MR, DMLR> ResultService<RCR, MR, DMLR>
where
    RCR: ResultClaimRepository,
    MR: TournamentMatchRepository,
    DMLR: DemoMatchLinkRepository,
{
    pub async fn submit_claim(
        &self,
        match_id: TournamentMatchId,
        submitted_by_registration_id: TournamentRegistrationId,
        submitted_by_user_id: UserId,
        winner_registration_id: TournamentRegistrationId,
        game_results: Vec<GameResult>,
        evidence_ids: Vec<EvidenceId>,
        demo_link_ids: Vec<DemoMatchLinkId>,  // NEW parameter
    ) -> Result<ResultClaim, DomainError> {
        // Validate the match exists and is in correct state
        let match_ = self.match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::MatchNotFound(match_id))?;

        if match_.status != MatchStatus::AwaitingResult {
            return Err(DomainError::InvalidMatchState(
                match_.status,
                "Cannot submit result for match not awaiting result".to_string(),
            ));
        }

        // Validate demo_link_ids belong to this match
        if !demo_link_ids.is_empty() {
            let links = self.demo_link_repo.find_by_ids(&demo_link_ids).await?;

            for link in &links {
                if link.match_id != match_id {
                    return Err(DomainError::DemoNotLinkedToMatch(link.demo_id, match_id));
                }
            }

            // Verify all requested IDs were found
            let found_ids: Vec<_> = links.iter().map(|l| l.id).collect();
            for id in &demo_link_ids {
                if !found_ids.contains(id) {
                    return Err(DomainError::DemoMatchLinkNotFound(*id));
                }
            }
        }

        // Create the claim with demo_link_ids
        let claim = ResultClaim {
            id: ResultClaimId::new(),
            match_id,
            submitted_by_registration_id,
            submitted_by_user_id,
            winner_registration_id,
            game_results,
            evidence_ids,
            demo_link_ids,  // NEW field
            status: ResultClaimStatus::Submitted,
            // ... rest of fields
        };

        self.claim_repo.insert(&claim).await?;
        Ok(claim)
    }
}
```

### 8. Extended Handler

Update `crates/portal-api/src/handlers/results.rs`:

```rust
pub async fn submit_result_claim(
    State(state): State<AppState>,
    Path((tournament_id, match_id)): Path<(TournamentId, TournamentMatchId)>,
    AuthenticatedUser(user): AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<SubmitResultClaimRequest>,
) -> ApiResult<Json<DataResponse<ResultClaimResponse>>> {
    // ... existing validation ...

    // Parse demo_link_ids
    let demo_link_ids: Vec<DemoMatchLinkId> = req.demo_link_ids
        .iter()
        .map(|s| s.parse())
        .collect::<Result<_, _>>()
        .map_err(|_| ApiError::bad_request("Invalid demo link ID format"))?;

    let claim = state
        .result_service
        .submit_claim(
            match_id,
            registration_id,
            user.user_id,
            winner_registration_id,
            game_results,
            evidence_ids,
            demo_link_ids,  // NEW argument
        )
        .await?;

    Ok(Json(DataResponse::new(claim.into())))
}
```

### 9. DomainError Extensions

Add to `crates/portal-core/src/errors.rs` or `crates/portal-domain/src/errors.rs`:

```rust
#[error("Demo {0} is not linked to match {1}")]
DemoNotLinkedToMatch(DemoId, TournamentMatchId),

#[error("Demo match link {0} not found")]
DemoMatchLinkNotFound(DemoMatchLinkId),
```

---

## Tests

Add to `crates/portal-api/tests/results_test.rs` or create new file:

### Category C: Result Submission with Demos (5 tests)

```rust
#[tokio::test]
async fn test_submit_result_with_demo_ids() {
    let app = TestApp::new().await;

    // Setup: Create tournament, match, link demo to match
    let (tournament_id, match_id) = create_tournament_with_match(&app).await;
    let demo_id = create_test_demo(&app).await;
    let link_id = link_demo_to_match(&app, demo_id, match_id, Some(1)).await;

    // Submit result with demo_link_ids
    let response = app.post_json(
        &format!("/v1/tournaments/{}/matches/{}/result/submit", tournament_id, match_id),
        &json!({
            "winner_registration_id": "...",
            "game_results": [...],
            "demo_link_ids": [link_id.to_string()]
        })
    ).await;

    response.assert_status(StatusCode::CREATED);
    let claim: ResultClaimResponse = response.json();
    assert!(claim.demo_link_ids.contains(&link_id.to_string()));
}

#[tokio::test]
async fn test_submit_result_auto_validates_demos() {
    // Submit result with demo_link_ids
    // Verify validation_result is stored on the link
}

#[tokio::test]
async fn test_submit_result_with_per_game_demos() {
    // Each GameResultInput has its own demo_link_id
    // Verify they're associated correctly
}

#[tokio::test]
async fn test_submit_result_invalid_demo_id() {
    // Submit with demo_link_id that doesn't belong to this match
    // Should return 400 Bad Request
}

#[tokio::test]
async fn test_submit_result_nonexistent_demo() {
    // Submit with demo_link_id that doesn't exist
    // Should return 404 Not Found
}
```

---

## Test Helpers

Add to test utilities:

```rust
async fn create_test_demo(app: &TestApp, game_id: GameId) -> DemoId {
    // Insert demo directly into database for testing
}

async fn link_demo_to_match(
    app: &TestApp,
    demo_id: DemoId,
    match_id: TournamentMatchId,
    game_number: Option<i32>,
) -> DemoMatchLinkId {
    // Create demo_match_links record
}

async fn create_tournament_with_match(app: &TestApp) -> (TournamentId, TournamentMatchId) {
    // Create tournament with bracket and get first match
}
```

---

## Acceptance Criteria

- [ ] Migration `0041_result_claims_demo_links.sql` created and runs successfully
- [ ] `result_claims.demo_link_ids` column exists with GIN index
- [ ] `demo_match_links.validation_result` column exists
- [ ] `ResultClaim` domain entity includes `demo_link_ids`
- [ ] `DbResultClaim` DB entity includes `demo_link_ids`
- [ ] Repository insert/select includes `demo_link_ids`
- [ ] `SubmitResultClaimRequest` accepts `demo_link_ids` and per-game `demo_link_id`
- [ ] `ResultClaimResponse` includes `demo_link_ids`
- [ ] `ResultService.submit_claim` validates demo links belong to match
- [ ] Appropriate errors for invalid/missing demo links
- [ ] All 5 integration tests pass
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

---

## Verification

```bash
# Run migration
sqlx migrate run

# Verify column exists
psql -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'result_claims' AND column_name = 'demo_link_ids';"

# Run tests
cargo test -p portal-api --test results_test -- result_with_demo

# Run all tests
cargo test
```

---

## Notes

- The `demo_link_ids` array contains references to `demo_match_links` records, NOT to `demos` directly
- This maintains a clean separation: demos are linked to matches first, then claims reference those links
- The existing `evidence_ids` field remains unchanged for traditional evidence uploads
- A result claim can have both `evidence_ids` and `demo_link_ids` or either one
