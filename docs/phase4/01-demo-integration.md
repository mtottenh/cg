# Phase 4.1-4.2: Demo Integration

> **Status**: Design Phase
> **Dependencies**: Phase 3.8 (Demo Evidence Integration)
> **Related**: [00-overview.md](./00-overview.md), [phase3/05-evidence-system.md](../phase3/05-evidence-system.md)

---

## Overview

This document covers the first two sub-phases of Phase 4:
- **4.1**: New demo handlers and validation methods
- **4.2**: Result claim demo bridge (demo_link_ids)

---

## 4.1 Demo Handlers & Validation

### New API Endpoints

#### GET `/v1/matches/{match_id}/demos`

Returns all demos linked to a specific match.

**Request**:
```
GET /v1/matches/{match_id}/demos?include_stats=true&game_number=1
```

**Query Parameters**:
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| include_stats | bool | No | Include full demo stats in response |
| game_number | i32 | No | Filter to specific game number |

**Response** (200):
```json
{
  "data": [
    {
      "link": {
        "id": "uuid",
        "demo_id": "uuid",
        "match_id": "uuid",
        "game_number": 1,
        "linked_at": "2024-01-15T10:00:00Z",
        "linked_by_user_id": "uuid",
        "validation_result": {
          "is_valid": true,
          "confidence": 0.95,
          "extracted_score": [16, 10],
          "claimed_score": [16, 10],
          "map_match": true,
          "warnings": [],
          "errors": []
        }
      },
      "demo": {
        "id": "uuid",
        "file_name": "match_12345.dem",
        "map_name": "de_dust2",
        "category": "league",
        "status": "ready",
        "team1_name": "Team Alpha",
        "team2_name": "Team Beta",
        "team1_score": 16,
        "team2_score": 10,
        "created_at": "2024-01-15T09:00:00Z"
      },
      "players": [
        {
          "steam_id": "76561198000000001",
          "player_name": "Player1",
          "team_name": "Team Alpha",
          "kills": 20,
          "deaths": 12,
          "assists": 5,
          "adr": 92.3,
          "hs_percentage": 40.0
        }
      ]
    }
  ]
}
```

**Authorization**: Match participant or tournament admin

---

#### DELETE `/v1/admin/demos/{demo_id}/link/{match_id}`

Unlink a demo from a match (admin only).

**Request**:
```
DELETE /v1/admin/demos/{demo_id}/link/{match_id}
```

**Response** (204): No content

**Authorization**: `tournament.brackets.manage`

---

### New Domain Types

#### DemoValidationResult

```rust
// portal-domain/src/entities/demo_validation.rs

use serde::{Deserialize, Serialize};

/// Result of validating a demo against a claimed match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoValidationResult {
    /// Whether the demo validates the claimed result.
    pub is_valid: bool,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,

    /// Score extracted from the demo [team1, team2].
    pub extracted_score: Option<(i32, i32)>,

    /// Score claimed in the result submission.
    pub claimed_score: (i32, i32),

    /// Whether the map in the demo matches the claimed map.
    pub map_match: bool,

    /// Non-fatal warnings (e.g., missing players).
    pub warnings: Vec<String>,

    /// Fatal errors that invalidate the result.
    pub errors: Vec<String>,
}

impl DemoValidationResult {
    pub fn valid(confidence: f32, extracted_score: (i32, i32), claimed_score: (i32, i32)) -> Self {
        Self {
            is_valid: true,
            confidence,
            extracted_score: Some(extracted_score),
            claimed_score,
            map_match: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn invalid(errors: Vec<String>, claimed_score: (i32, i32)) -> Self {
        Self {
            is_valid: false,
            confidence: 0.0,
            extracted_score: None,
            claimed_score,
            map_match: false,
            warnings: Vec::new(),
            errors,
        }
    }

    pub fn has_roster_mismatch(&self) -> bool {
        self.warnings.iter().any(|w| w.contains("player") || w.contains("roster"))
    }

    pub fn has_score_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Score mismatch"))
    }

    pub fn has_winner_mismatch(&self) -> bool {
        self.errors.iter().any(|e| e.contains("Winner mismatch"))
    }
}
```

#### GameDemoValidation

```rust
/// Validation result for a single game in a series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDemoValidation {
    pub game_number: i32,
    pub demo_link_id: DemoMatchLinkId,
    pub validation: DemoValidationResult,
}

/// Aggregated validation for all games in a match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDemoValidation {
    /// Overall validation status.
    pub overall_valid: bool,

    /// Lowest confidence across all games.
    pub overall_confidence: f32,

    /// Per-game validation results.
    pub game_validations: Vec<GameDemoValidation>,

    /// Game numbers without linked demos.
    pub unvalidated_games: Vec<i32>,

    /// Combined list of unrecognized players across all demos.
    pub unrecognized_players: Vec<UnrecognizedPlayer>,
}

/// A player found in a demo but not on either team's roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrecognizedPlayer {
    /// Steam ID of the unrecognized player.
    pub steam_id: String,

    /// Player's name as shown in the demo.
    pub player_name: String,

    /// Which team they played for in the demo.
    pub team_side: TeamSide,

    /// Which registered team this maps to (1 or 2).
    pub registration_side: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TeamSide {
    Team1,
    Team2,
}
```

---

### Extended DemoMatchLinkRepository

```rust
// portal-domain/src/repositories/demo.rs (extended)

#[async_trait]
pub trait DemoMatchLinkRepository: Send + Sync + 'static {
    // ... existing methods ...

    /// Find links by multiple IDs.
    async fn find_by_ids(
        &self,
        ids: &[DemoMatchLinkId],
    ) -> Result<Vec<DemoMatchLink>, DomainError>;

    /// Find all links for a match.
    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLink>, DomainError>;

    /// Find all links for a match with full demo data.
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
}
```

---

### DemoService Extensions

```rust
// portal-domain/src/services/demo.rs (extended)

impl<DR, DMLR, DPR> DemoService<DR, DMLR, DPR>
where
    DR: DemoRepository,
    DMLR: DemoMatchLinkRepository,
    DPR: DemoPlayerRepository,
{
    /// Get all demos linked to a match with optional stats.
    pub async fn get_match_demos(
        &self,
        match_id: TournamentMatchId,
        include_stats: bool,
        game_number: Option<i32>,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError> {
        let links = self.link_repo.find_by_match_id_with_demos(match_id).await?;

        let mut result = Vec::new();
        for (link, demo, players) in links {
            if let Some(gn) = game_number {
                if link.game_number != Some(gn) {
                    continue;
                }
            }

            result.push(DemoMatchLinkWithData {
                link,
                demo,
                players: if include_stats { Some(players) } else { None },
            });
        }

        Ok(result)
    }

    /// Validate a demo against a claimed result.
    pub async fn validate_against_result(
        &self,
        demo_id: DemoId,
        claimed_result: &GameResult,
        team1_steam_ids: &[String],
        team2_steam_ids: &[String],
    ) -> Result<DemoValidationResult, DomainError> {
        let demo = self.demo_repo
            .find_by_id(demo_id)
            .await?
            .ok_or(DomainError::DemoNotFound(demo_id))?;

        let players = self.player_repo.find_by_demo_id(demo_id).await?;

        // Build validation result
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut confidence = 1.0f32;

        // 1. Check map match
        let map_match = Self::maps_match(&demo.map_name, &claimed_result.map_id);
        if !map_match {
            errors.push(format!(
                "Map mismatch: demo has '{}', claimed '{}'",
                demo.map_name.as_deref().unwrap_or("unknown"),
                claimed_result.map_id
            ));
            confidence = 0.0;
        }

        // 2. Check player presence and build unrecognized list
        let demo_steam_ids: Vec<String> = players.iter()
            .map(|p| p.steam_id.clone())
            .collect();

        let (t1_missing, t2_missing, unrecognized) = Self::check_players(
            &demo_steam_ids,
            &players,
            team1_steam_ids,
            team2_steam_ids,
        );

        if !t1_missing.is_empty() {
            warnings.push(format!(
                "Team 1 players not in demo: {}",
                t1_missing.join(", ")
            ));
            confidence *= 0.7;
        }
        if !t2_missing.is_empty() {
            warnings.push(format!(
                "Team 2 players not in demo: {}",
                t2_missing.join(", ")
            ));
            confidence *= 0.7;
        }
        if !unrecognized.is_empty() {
            warnings.push(format!(
                "Unrecognized players in demo: {}",
                unrecognized.iter().map(|p| &p.player_name).collect::<Vec<_>>().join(", ")
            ));
        }

        // 3. Check scores
        let demo_t1_score = demo.team1_score.unwrap_or(0);
        let demo_t2_score = demo.team2_score.unwrap_or(0);

        if demo_t1_score != claimed_result.participant1_score
            || demo_t2_score != claimed_result.participant2_score
        {
            errors.push(format!(
                "Score mismatch: demo shows {}-{}, claimed {}-{}",
                demo_t1_score, demo_t2_score,
                claimed_result.participant1_score, claimed_result.participant2_score
            ));
            confidence = 0.0;
        }

        // 4. Check winner
        let demo_winner_is_t1 = demo_t1_score > demo_t2_score;
        let claimed_winner_is_t1 = claimed_result.participant1_score > claimed_result.participant2_score;

        if demo_winner_is_t1 != claimed_winner_is_t1 {
            errors.push("Winner mismatch between demo and claimed result".to_string());
            confidence = 0.0;
        }

        Ok(DemoValidationResult {
            is_valid: errors.is_empty() && confidence > 0.5,
            confidence,
            extracted_score: Some((demo_t1_score, demo_t2_score)),
            claimed_score: (claimed_result.participant1_score, claimed_result.participant2_score),
            map_match,
            warnings,
            errors,
        })
    }

    /// Unlink a demo from a match.
    pub async fn unlink_from_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        let links = self.link_repo.find_by_match_id(match_id).await?;

        let link = links.into_iter()
            .find(|l| l.demo_id == demo_id)
            .ok_or(DomainError::DemoNotLinkedToMatch(demo_id, match_id))?;

        self.link_repo.delete(link.id).await
    }

    // Helper: normalize map names for comparison
    fn maps_match(demo_map: &Option<String>, claimed_map: &str) -> bool {
        let Some(dm) = demo_map else { return false };
        let normalize = |s: &str| s.to_lowercase().replace("de_", "").replace('_', "");
        normalize(dm) == normalize(claimed_map)
    }

    // Helper: check which players are present/missing/unrecognized
    fn check_players(
        demo_steam_ids: &[String],
        players: &[DemoPlayer],
        team1_ids: &[String],
        team2_ids: &[String],
    ) -> (Vec<String>, Vec<String>, Vec<UnrecognizedPlayer>) {
        let t1_missing: Vec<String> = team1_ids.iter()
            .filter(|id| !demo_steam_ids.contains(id))
            .cloned()
            .collect();

        let t2_missing: Vec<String> = team2_ids.iter()
            .filter(|id| !demo_steam_ids.contains(id))
            .cloned()
            .collect();

        let all_registered: Vec<&String> = team1_ids.iter()
            .chain(team2_ids.iter())
            .collect();

        let unrecognized: Vec<UnrecognizedPlayer> = players.iter()
            .filter(|p| !all_registered.contains(&&p.steam_id))
            .map(|p| {
                // Determine which team they were on in the demo
                let team_side = if p.team_name.as_ref().map(|t| t.contains("1")).unwrap_or(false) {
                    TeamSide::Team1
                } else {
                    TeamSide::Team2
                };
                let registration_side = if team_side == TeamSide::Team1 { 1 } else { 2 };

                UnrecognizedPlayer {
                    steam_id: p.steam_id.clone(),
                    player_name: p.player_name.clone(),
                    team_side,
                    registration_side,
                }
            })
            .collect();

        (t1_missing, t2_missing, unrecognized)
    }
}
```

---

## 4.2 Result Claim Demo Bridge

### Database Migration

**Migration: `0041_result_claims_demo_links.sql`**

```sql
-- Add demo_link_ids to result_claims for demo catalog integration
-- This bridges result claims to the demo catalog without duplicating data

ALTER TABLE result_claims
ADD COLUMN demo_link_ids UUID[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN result_claims.demo_link_ids IS
    'Array of demo match link IDs from demo_match_links table. Separate from evidence_ids to maintain clean domain boundaries.';

-- Index for efficient lookup of claims by demo link
CREATE INDEX idx_result_claims_demo_links ON result_claims USING gin(demo_link_ids);

-- Add validation_result column to demo_match_links
ALTER TABLE demo_match_links
ADD COLUMN validation_result JSONB;

COMMENT ON COLUMN demo_match_links.validation_result IS
    'Cached validation result comparing this demo against the claimed match result';
```

---

### Extended ResultClaim Entity

```rust
// portal-domain/src/entities/result_claim.rs (extended)

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

---

### Extended SubmitResultClaimRequest

```rust
// portal-api/src/dto/requests/result.rs (extended)

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
    /// If provided, this demo is used for validation of this specific game.
    pub demo_link_id: Option<String>,
}
```

---

### Extended ResultService

```rust
// portal-domain/src/services/tournament/result.rs (extended)

impl<RCR, MR, DMLR> ResultService<RCR, MR, DMLR>
where
    RCR: ResultClaimRepository,
    MR: TournamentMatchRepository,
    DMLR: DemoMatchLinkRepository,
{
    /// Submit a result claim with optional demo references.
    pub async fn submit_claim(
        &self,
        match_id: TournamentMatchId,
        submitted_by_registration_id: TournamentRegistrationId,
        submitted_by_user_id: UserId,
        winner_registration_id: TournamentRegistrationId,
        game_results: Vec<GameResult>,
        evidence_ids: Vec<EvidenceId>,
        demo_link_ids: Vec<DemoMatchLinkId>,
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
                    return Err(DomainError::DemoNotLinkedToMatch(
                        link.demo_id,
                        match_id,
                    ));
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

        // Create the claim
        let claim = ResultClaim {
            id: ResultClaimId::new(),
            match_id,
            submitted_by_registration_id,
            submitted_by_user_id,
            winner_registration_id,
            game_results,
            evidence_ids,
            demo_link_ids,
            status: ResultClaimStatus::Submitted,
            confirmed_by_registration_id: None,
            confirmed_by_user_id: None,
            confirmed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.claim_repo.insert(&claim).await?;

        Ok(claim)
    }
}
```

---

## DTOs

### DemoMatchLinkWithDemoResponse

```rust
// portal-api/src/dto/responses/demo.rs

/// Demo link with full demo data.
#[derive(Debug, Serialize, ToSchema)]
pub struct DemoMatchLinkWithDemoResponse {
    /// The link record.
    pub link: DemoMatchLinkResponse,

    /// The demo details.
    pub demo: DemoResponse,

    /// Player stats (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<Vec<DemoPlayerResponse>>,
}

/// Demo match link details.
#[derive(Debug, Serialize, ToSchema)]
pub struct DemoMatchLinkResponse {
    pub id: String,
    pub demo_id: String,
    pub match_id: String,
    pub game_number: Option<i32>,
    pub linked_at: DateTime<Utc>,
    pub linked_by_user_id: Option<String>,
    pub validation_result: Option<DemoValidationResultResponse>,
}

/// Validation result response.
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

---

## API Handlers

### get_demos_for_match

```rust
// portal-api/src/handlers/demos.rs

/// Get demos linked to a match.
#[utoipa::path(
    get,
    path = "/v1/matches/{match_id}/demos",
    params(
        ("match_id" = String, Path, description = "Match ID"),
        ("include_stats" = Option<bool>, Query, description = "Include player stats"),
        ("game_number" = Option<i32>, Query, description = "Filter by game number"),
    ),
    responses(
        (status = 200, description = "Demos for match", body = DataResponse<Vec<DemoMatchLinkWithDemoResponse>>),
        (status = 404, description = "Match not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn get_demos_for_match(
    State(state): State<AppState>,
    Path(match_id): Path<TournamentMatchId>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(params): Query<GetDemosForMatchQuery>,
) -> ApiResult<Json<DataResponse<Vec<DemoMatchLinkWithDemoResponse>>>> {
    // Authorization: must be participant or admin
    // (implementation details...)

    let demos = state
        .demo_service
        .get_match_demos(
            match_id,
            params.include_stats.unwrap_or(false),
            params.game_number,
        )
        .await?;

    let response: Vec<DemoMatchLinkWithDemoResponse> = demos
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(DataResponse::new(response)))
}
```

### unlink_demo_from_match

```rust
/// Unlink a demo from a match (admin only).
#[utoipa::path(
    delete,
    path = "/v1/admin/demos/{demo_id}/link/{match_id}",
    params(
        ("demo_id" = String, Path, description = "Demo ID"),
        ("match_id" = String, Path, description = "Match ID"),
    ),
    responses(
        (status = 204, description = "Demo unlinked"),
        (status = 404, description = "Link not found", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "demos"
)]
pub async fn unlink_demo_from_match(
    State(state): State<AppState>,
    Path((demo_id, match_id)): Path<(DemoId, TournamentMatchId)>,
    AuthenticatedUser(user): AuthenticatedUser,
    perm_checker: PermissionChecker,
) -> ApiResult<StatusCode> {
    // Require admin permission
    perm_checker
        .require_permission(&user, permissions::tournament::BRACKETS_MANAGE)
        .await?;

    state
        .demo_service
        .unlink_from_match(demo_id, match_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
```

---

## Routes

```rust
// portal-api/src/routes/matches.rs (extended)

.route(
    "/v1/matches/:match_id/demos",
    get(handlers::demos::get_demos_for_match),
)

// portal-api/src/routes/admin.rs (extended)

.route(
    "/v1/admin/demos/:demo_id/link/:match_id",
    delete(handlers::demos::unlink_demo_from_match),
)
```

---

## OpenAPI Registration

```rust
// portal-api/src/openapi.rs (extended paths)

handlers::demos::get_demos_for_match,
handlers::demos::unlink_demo_from_match,

// Extended schemas
DemoMatchLinkWithDemoResponse,
DemoMatchLinkResponse,
DemoValidationResultResponse,
GetDemosForMatchQuery,
```

---

## Integration Tests

### Category A: Demo Catalog (5 tests)

```rust
#[tokio::test]
async fn test_list_demos_empty() {
    // No demos exist, returns empty list
}

#[tokio::test]
async fn test_list_demos_with_filters() {
    // Filter by game, category, status, map
}

#[tokio::test]
async fn test_get_demo_not_found() {
    // Returns 404 for non-existent demo
}

#[tokio::test]
async fn test_get_demo_players() {
    // Returns player stats for a demo
}

#[tokio::test]
async fn test_get_demo_links() {
    // Returns all matches linked to a demo
}
```

### Category B: Demo-Match Linking (5 tests)

```rust
#[tokio::test]
async fn test_get_demos_for_match_empty() {
    // Match with no linked demos
}

#[tokio::test]
async fn test_get_demos_for_match_with_demos() {
    // Match with linked demos, basic response
}

#[tokio::test]
async fn test_get_demos_for_match_with_stats() {
    // include_stats=true returns player data
}

#[tokio::test]
async fn test_link_demo_to_match_success() {
    // Admin can link demo to match
}

#[tokio::test]
async fn test_unlink_demo_from_match_success() {
    // Admin can unlink demo from match
}
```

### Category C: Result Submission with Demos (5 tests)

```rust
#[tokio::test]
async fn test_submit_result_with_demo_ids() {
    // Result submission includes demo_link_ids
}

#[tokio::test]
async fn test_submit_result_auto_links_demos() {
    // Demos linked to match are auto-associated
}

#[tokio::test]
async fn test_submit_result_with_per_game_demos() {
    // Each game can have its own demo_link_id
}

#[tokio::test]
async fn test_submit_result_invalid_demo_id() {
    // Rejects demo_link_id not linked to this match
}

#[tokio::test]
async fn test_submit_result_nonexistent_demo() {
    // Rejects non-existent demo_link_id
}
```

---

## Acceptance Criteria

### 4.1 Demo Handlers & Validation

- [ ] `GET /v1/matches/{id}/demos` returns linked demos with optional stats
- [ ] `DELETE /v1/admin/demos/{id}/link/{match_id}` unlinks demo (admin only)
- [ ] `DemoValidationResult` captures all validation outcomes
- [ ] Validation detects map, score, winner, and roster mismatches
- [ ] Map name normalization handles `de_` prefix variations

### 4.2 Result Claim Demo Bridge

- [ ] Migration adds `demo_link_ids` to `result_claims`
- [ ] Migration adds `validation_result` to `demo_match_links`
- [ ] `ResultClaim` entity includes `demo_link_ids` field
- [ ] `SubmitResultClaimRequest` accepts `demo_link_ids` and per-game `demo_link_id`
- [ ] `ResultService.submit_claim` validates demo links belong to the match
- [ ] All integration tests pass
