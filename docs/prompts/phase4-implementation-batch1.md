# Phase 4.1 Implementation - Demo Handlers & Validation Methods

## Context

You are implementing **Phase 4.1** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This phase adds new demo API handlers and validation methods to bridge the demo catalog with the tournament system.

**Prerequisites**: Phase 3.8 (Demo Evidence Integration) is complete. The demo catalog system exists with `demos`, `demo_players`, and `demo_match_links` tables.

**Design Documents**:
- `docs/phase4/00-overview.md` - Phase 4 overview
- `docs/phase4/01-demo-integration.md` - Detailed design for this batch

**Reference Files**:
- `crates/portal-db/src/adapters/demo.rs` - Existing demo repository
- `crates/portal-api/src/handlers/demos.rs` - Existing demo handlers
- `crates/portal-plugins/src/games/cs2/evidence_validator.rs` - Existing validation logic
- `crates/portal-domain/src/services/demo.rs` - Demo service (if exists)

---

## Your Task

Implement the demo handlers and validation methods defined in Phase 4.1.

### Goals

1. **New Domain Types**: Create `DemoValidationResult`, `MatchDemoValidation`, `UnrecognizedPlayer` types
2. **Extended Repository**: Add `find_by_ids`, `find_by_match_id_with_demos`, `update_validation`, `delete` to `DemoMatchLinkRepository`
3. **DemoService Methods**: Add `get_match_demos()`, `validate_against_result()`, `unlink_from_match()`
4. **New API Endpoints**: `GET /v1/matches/{match_id}/demos`, `DELETE /v1/admin/demos/{id}/link/{match_id}`
5. **Integration Tests**: 10 tests covering demo catalog and demo-match linking

---

## Implementation

### 1. Domain Types

Create `crates/portal-domain/src/entities/demo_validation.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Result of validating a demo against a claimed match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoValidationResult {
    pub is_valid: bool,
    pub confidence: f32,
    pub extracted_score: Option<(i32, i32)>,
    pub claimed_score: (i32, i32),
    pub map_match: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl DemoValidationResult {
    pub fn has_roster_mismatch(&self) -> bool {
        self.warnings.iter().any(|w|
            w.to_lowercase().contains("player") ||
            w.to_lowercase().contains("roster") ||
            w.to_lowercase().contains("unrecognized")
        )
    }

    pub fn has_score_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Score mismatch"))
    }

    pub fn has_winner_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Winner mismatch"))
    }
}

/// A player found in a demo but not on either team's roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrecognizedPlayer {
    pub steam_id: String,
    pub player_name: String,
    pub team_side: TeamSide,
    pub registration_side: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TeamSide {
    Team1,
    Team2,
}
```

Update `crates/portal-domain/src/entities/mod.rs` to export the new module.

### 2. Extended Repository Trait

Add to `crates/portal-domain/src/repositories/demo.rs`:

```rust
/// Find links by multiple IDs.
async fn find_by_ids(&self, ids: &[DemoMatchLinkId]) -> Result<Vec<DemoMatchLink>, DomainError>;

/// Find all links for a match with full demo and player data.
async fn find_by_match_id_with_demos(
    &self,
    match_id: TournamentMatchId,
) -> Result<Vec<(DemoMatchLink, Demo, Vec<DemoPlayer>)>, DomainError>;

/// Update validation result for a link.
async fn update_validation(
    &self,
    id: DemoMatchLinkId,
    validation: DemoValidationResult,
) -> Result<(), DomainError>;

/// Delete a link (unlink demo from match).
async fn delete(&self, id: DemoMatchLinkId) -> Result<(), DomainError>;
```

### 3. Repository Implementation

Implement in `crates/portal-db/src/adapters/demo.rs`:

- `find_by_ids`: `SELECT * FROM demo_match_links WHERE id = ANY($1)`
- `find_by_match_id_with_demos`: JOIN query across demo_match_links, demos, demo_players
- `update_validation`: `UPDATE demo_match_links SET validation_result = $1 WHERE id = $2`
- `delete`: `DELETE FROM demo_match_links WHERE id = $1`

### 4. DemoService

Create or extend `crates/portal-domain/src/services/demo.rs`:

```rust
pub struct DemoService<DR, DMLR, DPR> { ... }

impl DemoService {
    /// Get all demos linked to a match with optional stats.
    pub async fn get_match_demos(
        &self,
        match_id: TournamentMatchId,
        include_stats: bool,
        game_number: Option<i32>,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError>;

    /// Validate a demo against a claimed result.
    pub async fn validate_against_result(
        &self,
        demo_id: DemoId,
        claimed_result: &GameResult,
        team1_steam_ids: &[String],
        team2_steam_ids: &[String],
    ) -> Result<DemoValidationResult, DomainError>;

    /// Unlink a demo from a match.
    pub async fn unlink_from_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError>;
}
```

### 5. API Handlers

Add to `crates/portal-api/src/handlers/demos.rs`:

```rust
/// Get demos linked to a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/demos",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("include_stats" = Option<bool>, Query),
        ("game_number" = Option<i32>, Query),
    ),
    responses(
        (status = 200, body = DataResponse<Vec<DemoMatchLinkWithDemoResponse>>),
        (status = 404, body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demos_for_match(...) -> ApiResult<...>;

/// Unlink a demo from a match (admin only).
#[utoipa::path(
    delete,
    path = "/v1/admin/demos/{demo_id}/link/{match_id}",
    responses(
        (status = 204, description = "Unlinked"),
        (status = 404, body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn unlink_demo_from_match(...) -> ApiResult<StatusCode>;
```

### 6. DTOs

Add to `crates/portal-api/src/dto/`:

**requests/demo.rs**:
```rust
#[derive(Debug, Deserialize, ToSchema)]
pub struct GetDemosForMatchQuery {
    pub include_stats: Option<bool>,
    pub game_number: Option<i32>,
}
```

**responses/demo.rs**:
```rust
#[derive(Debug, Serialize, ToSchema)]
pub struct DemoMatchLinkWithDemoResponse {
    pub link: DemoMatchLinkResponse,
    pub demo: DemoResponse,
    pub players: Option<Vec<DemoPlayerResponse>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DemoMatchLinkResponse {
    pub id: String,
    pub demo_id: String,
    pub match_id: String,
    pub game_number: Option<i32>,
    pub linked_at: DateTime<Utc>,
    pub validation_result: Option<DemoValidationResultResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DemoValidationResultResponse {
    pub is_valid: bool,
    pub confidence: f32,
    pub extracted_score: Option<[i32; 2]>,
    pub claimed_score: [i32; 2],
    pub map_match: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}
```

### 7. Routes

Add routes in `crates/portal-api/src/routes/`:

```rust
// matches.rs
.route("/v1/matches/:match_id/demos", get(handlers::demos::get_demos_for_match))

// admin.rs
.route("/v1/admin/demos/:demo_id/link/:match_id", delete(handlers::demos::unlink_demo_from_match))
```

### 8. OpenAPI Registration

Update `crates/portal-api/src/openapi.rs`:

```rust
// paths
handlers::demos::get_demos_for_match,
handlers::demos::unlink_demo_from_match,

// schemas
GetDemosForMatchQuery,
DemoMatchLinkWithDemoResponse,
DemoMatchLinkResponse,
DemoValidationResultResponse,
```

---

## Tests

Create `crates/portal-api/tests/demos_test.rs`:

### Category A: Demo Catalog (5 tests)

```rust
#[tokio::test]
async fn test_list_demos_empty() {
    let app = TestApp::new().await;
    // GET /v1/demos returns empty list
}

#[tokio::test]
async fn test_list_demos_with_filters() {
    let app = TestApp::new().await;
    // Create demos, filter by game/category/status
}

#[tokio::test]
async fn test_get_demo_not_found() {
    let app = TestApp::new().await;
    // GET /v1/demos/{nonexistent} returns 404
}

#[tokio::test]
async fn test_get_demo_players() {
    let app = TestApp::new().await;
    // Create demo with players, verify player data returned
}

#[tokio::test]
async fn test_get_demo_links() {
    let app = TestApp::new().await;
    // Create demo linked to match, verify link data
}
```

### Category B: Demo-Match Linking (5 tests)

```rust
#[tokio::test]
async fn test_get_demos_for_match_empty() {
    let app = TestApp::new().await;
    // Match with no linked demos returns empty list
}

#[tokio::test]
async fn test_get_demos_for_match_with_demos() {
    let app = TestApp::new().await;
    // Create match, link demo, verify response
}

#[tokio::test]
async fn test_get_demos_for_match_with_stats() {
    let app = TestApp::new().await;
    // include_stats=true includes player data
}

#[tokio::test]
async fn test_link_demo_to_match_success() {
    let app = TestApp::new().await;
    // Link demo to match via existing endpoint
}

#[tokio::test]
async fn test_unlink_demo_from_match_success() {
    let app = TestApp::new().await;
    // Admin can unlink demo, verify 204 and demo no longer linked
}
```

---

## Acceptance Criteria

- [ ] `DemoValidationResult` type created with helper methods
- [ ] `UnrecognizedPlayer` and `TeamSide` types created
- [ ] Repository trait extended with new methods
- [ ] Repository implementation complete for PostgreSQL
- [ ] `DemoService` created with `get_match_demos`, `validate_against_result`, `unlink_from_match`
- [ ] `GET /v1/matches/{match_id}/demos` endpoint working
- [ ] `DELETE /v1/admin/demos/{demo_id}/link/{match_id}` endpoint working (admin only)
- [ ] DTOs and conversions implemented
- [ ] Routes registered
- [ ] OpenAPI documentation updated
- [ ] All 10 integration tests pass
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

---

## Verification

```bash
# Run tests for this batch
cargo test -p portal-api --test demos_test

# Run clippy
cargo clippy -p portal-domain -p portal-db -p portal-api

# Check OpenAPI
cargo run -p portal-app &
curl http://localhost:3000/api-docs/openapi.json | jq '.paths["/v1/matches/{match_id}/demos"]'
```

---

## Notes

- The validation logic should reuse patterns from `portal-plugins/src/games/cs2/evidence_validator.rs`
- Map name normalization should handle `de_` prefix variations (e.g., `de_dust2` == `dust2`)
- Authorization for `get_demos_for_match`: match participant or tournament admin
- Authorization for `unlink_demo_from_match`: requires `tournament.brackets.manage` permission
