# Phase 4.4 Implementation - Review Workflow Integration

## Context

You are implementing **Phase 4.4** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This phase integrates the Result Review System with the Match Completion Saga to create a complete validation workflow.

**Prerequisites**: Phase 4.3 (Result Review System) is complete.

**Design Documents**:
- `docs/phase4/00-overview.md` - Phase 4 overview
- `docs/phase4/02-result-review-system.md` - Detailed design for this batch

**Reference Files**:
- `crates/portal-domain/src/services/tournament/match_completion.rs` - Match completion saga
- `crates/portal-domain/src/services/tournament/result_review.rs` - ResultReviewService (from 4.3)
- `crates/portal-domain/src/services/demo.rs` - DemoService with validation (from 4.1)
- `crates/portal-domain/src/services/tournament/match_lifecycle.rs` - Match state transitions

---

## Your Task

Integrate the Result Review System with the Match Completion Saga and implement end-to-end validation workflows.

### Goals

1. **Saga Extension**: Add `step_validate_demos()` to Match Completion Saga
2. **Review Triggers**: Automatically create reviews when validation fails
3. **Saga Pausing**: Pause saga execution when review is pending
4. **Saga Continuation**: Resume saga after review is resolved
5. **Rejection Handling**: Return match to in_progress when admin rejects
6. **Integration Tests**: 5 tests covering full workflows

---

## Implementation

### 1. Extended Match Completion Saga

Update `crates/portal-domain/src/services/tournament/match_completion.rs`:

```rust
use crate::entities::{ResultReview, ResultReviewStatus};
use crate::services::tournament::{ResultReviewService, DemoService};

pub struct MatchCompletionSaga<...> {
    // ... existing repos and services ...
    demo_service: Arc<DemoService<...>>,
    result_review_service: Arc<ResultReviewService<...>>,
    saga_state_repo: Arc<SagaStateRepository>,
}

impl MatchCompletionSaga {
    /// Execute the match completion saga.
    pub async fn execute(&self, match_id: TournamentMatchId) -> Result<SagaOutcome, DomainError> {
        // Step 1: Validate result is confirmed
        self.step_validate_result_confirmed(match_id).await?;

        // Step 2: Validate demos (NEW)
        let review = self.step_validate_demos(match_id).await?;

        // If review was created and is pending, pause saga
        if let Some(review) = review {
            if review.status.is_pending() {
                self.store_pending_state(match_id, "awaiting_review", review.id).await?;
                return Ok(SagaOutcome::Paused {
                    reason: "Awaiting result review".to_string(),
                    review_id: Some(review.id),
                });
            }

            // Review is already resolved (acknowledged or approved)
            if review.status == ResultReviewStatus::Rejected {
                return Err(DomainError::ResultRejectedByAdmin(review.id));
            }
        }

        // Continue with remaining steps
        self.complete_remaining_steps(match_id).await
    }

    /// Validate demos against the confirmed result.
    async fn step_validate_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultReview>, DomainError> {
        // Get the confirmed result claim
        let claim = self.result_claim_repo
            .find_confirmed_by_match_id(match_id)
            .await?
            .ok_or(DomainError::NoConfirmedResult(match_id))?;

        // If no demo links, skip validation (demos are optional evidence)
        if claim.demo_link_ids.is_empty() {
            return Ok(None);
        }

        // Get match to find participant registrations
        let match_ = self.match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::MatchNotFound(match_id))?;

        // Get participant steam IDs from registrations
        let (team1_steam_ids, team2_steam_ids) =
            self.get_participant_steam_ids(&match_).await?;

        // Validate each linked demo
        for demo_link_id in &claim.demo_link_ids {
            let link = self.demo_link_repo
                .find_by_id(*demo_link_id)
                .await?
                .ok_or(DomainError::DemoMatchLinkNotFound(*demo_link_id))?;

            // Find matching game result (by game_number or first result)
            let game_result = claim.game_results
                .iter()
                .find(|g| link.game_number == Some(g.game_number))
                .or_else(|| claim.game_results.first())
                .ok_or(DomainError::NoGameResultForDemo(*demo_link_id))?;

            // Perform validation
            let validation = self.demo_service
                .validate_against_result(
                    link.demo_id,
                    game_result,
                    &team1_steam_ids,
                    &team2_steam_ids,
                )
                .await?;

            // Store validation result on the link
            self.demo_link_repo
                .update_validation(*demo_link_id, validation.clone())
                .await?;

            // Check if review is needed
            if !validation.is_valid || validation.has_roster_mismatch() {
                // Determine captain registration IDs
                let captain1_reg_id = match_.participant1_registration_id
                    .ok_or(DomainError::NoParticipantForMatch(match_id, 1))?;
                let captain2_reg_id = match_.participant2_registration_id
                    .ok_or(DomainError::NoParticipantForMatch(match_id, 2))?;

                let review = self.result_review_service
                    .create_from_validation(
                        claim.id,
                        match_id,
                        Some(*demo_link_id),
                        &validation,
                        captain1_reg_id,
                        captain2_reg_id,
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
    ) -> Result<SagaOutcome, DomainError> {
        // Check review status
        let review = self.result_review_service
            .get_for_match(match_id)
            .await?
            .ok_or(DomainError::NoReviewForMatch(match_id))?;

        if review.status == ResultReviewStatus::Rejected {
            // Return match to in_progress state
            self.match_lifecycle_service
                .transition_to(match_id, MatchStatus::InProgress)
                .await?;

            return Ok(SagaOutcome::Cancelled {
                reason: "Result rejected by admin".to_string(),
            });
        }

        if !review.status.is_terminal() && review.status != ResultReviewStatus::Acknowledged {
            return Err(DomainError::ReviewStillPending(review.id));
        }

        // Clear pending state and continue saga
        self.clear_pending_state(match_id).await?;
        self.complete_remaining_steps(match_id).await
    }

    /// Complete the remaining saga steps after validation passes.
    async fn complete_remaining_steps(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<SagaOutcome, DomainError> {
        // Step 3: Finalize match with winner/loser
        self.step_finalize_match(match_id).await?;

        // Step 4: Advance winner to next match
        self.step_advance_winner(match_id).await?;

        // Step 5: Route loser (if double elimination)
        self.step_route_loser(match_id).await?;

        // Step 6: Update standings
        self.step_update_standings(match_id).await?;

        // Step 7: Check tournament completion
        self.step_check_completion(match_id).await?;

        Ok(SagaOutcome::Completed)
    }

    /// Get Steam IDs for both participants in a match.
    async fn get_participant_steam_ids(
        &self,
        match_: &TournamentMatch,
    ) -> Result<(Vec<String>, Vec<String>), DomainError> {
        // Get team members for registration 1
        let team1_ids = if let Some(reg_id) = match_.participant1_registration_id {
            self.get_steam_ids_for_registration(reg_id).await?
        } else {
            Vec::new()
        };

        // Get team members for registration 2
        let team2_ids = if let Some(reg_id) = match_.participant2_registration_id {
            self.get_steam_ids_for_registration(reg_id).await?
        } else {
            Vec::new()
        };

        Ok((team1_ids, team2_ids))
    }

    /// Get Steam IDs for a tournament registration's roster.
    async fn get_steam_ids_for_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<String>, DomainError> {
        // Get registration to find team
        let registration = self.registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::RegistrationNotFound(registration_id))?;

        // If team registration, get team members
        if let Some(team_id) = registration.team_id {
            let members = self.team_member_repo
                .find_by_team_id(team_id)
                .await?;

            let mut steam_ids = Vec::new();
            for member in members {
                if let Some(steam_id) = self.player_repo
                    .get_steam_id(member.player_id)
                    .await?
                {
                    steam_ids.push(steam_id);
                }
            }
            return Ok(steam_ids);
        }

        // If solo registration, get player's Steam ID
        if let Some(player_id) = registration.player_id {
            if let Some(steam_id) = self.player_repo
                .get_steam_id(player_id)
                .await?
            {
                return Ok(vec![steam_id]);
            }
        }

        Ok(Vec::new())
    }
}

/// Outcome of saga execution.
#[derive(Debug)]
pub enum SagaOutcome {
    /// Saga completed successfully.
    Completed,

    /// Saga paused awaiting external action.
    Paused {
        reason: String,
        review_id: Option<ResultReviewId>,
    },

    /// Saga cancelled due to rejection or error.
    Cancelled {
        reason: String,
    },
}
```

### 2. Update Approval Handler

Update the approval handler to trigger saga continuation:

```rust
// In portal-api/src/handlers/result_reviews.rs

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
    match state
        .match_completion_saga
        .continue_after_review(review.match_id)
        .await
    {
        Ok(SagaOutcome::Completed) => {
            tracing::info!(match_id = %review.match_id, "Match completion saga completed after approval");
        }
        Ok(SagaOutcome::Cancelled { reason }) => {
            tracing::warn!(match_id = %review.match_id, reason, "Saga cancelled after approval");
        }
        Err(e) => {
            tracing::error!(match_id = %review.match_id, error = %e, "Saga continuation failed");
            // Don't fail the request - approval succeeded, saga can be retried
        }
        _ => {}
    }

    Ok(Json(DataResponse::new(review.into())))
}
```

### 3. Update Rejection Handler

```rust
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

    // Clear the saga's pending state
    state
        .match_completion_saga
        .clear_pending_state(review.match_id)
        .await?;

    Ok(Json(DataResponse::new(review.into())))
}
```

### 4. Update Acknowledgment Handler

Update to trigger saga continuation when both captains acknowledge:

```rust
pub async fn acknowledge_result_review(
    State(state): State<AppState>,
    Path(match_id): Path<TournamentMatchId>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> ApiResult<Json<DataResponse<ResultReviewResponse>>> {
    let review = state
        .result_review_service
        .get_for_match(match_id)
        .await?
        .ok_or_else(|| ApiError::not_found("No review exists for this match"))?;

    let registration_id = get_user_registration_for_match(&state, &user, match_id).await?;

    let updated = state
        .result_review_service
        .acknowledge(review.id, user.user_id, registration_id)
        .await?;

    // If status transitioned to Acknowledged (roster-only mismatch resolved)
    // and doesn't require admin, continue saga
    if updated.status == ResultReviewStatus::Acknowledged && !updated.requires_admin() {
        match state
            .match_completion_saga
            .continue_after_review(match_id)
            .await
        {
            Ok(SagaOutcome::Completed) => {
                tracing::info!(match_id = %match_id, "Match completion saga completed after acknowledgment");
            }
            Err(e) => {
                tracing::error!(match_id = %match_id, error = %e, "Saga continuation failed");
            }
            _ => {}
        }
    }

    Ok(Json(DataResponse::new(updated.into())))
}
```

### 5. DomainError Extensions

Add to domain errors:

```rust
#[error("No confirmed result for match {0}")]
NoConfirmedResult(TournamentMatchId),

#[error("No game result found for demo link {0}")]
NoGameResultForDemo(DemoMatchLinkId),

#[error("No review exists for match {0}")]
NoReviewForMatch(TournamentMatchId),

#[error("Review {0} is still pending")]
ReviewStillPending(ResultReviewId),

#[error("Result rejected by admin for review {0}")]
ResultRejectedByAdmin(ResultReviewId),

#[error("No participant {1} set for match {0}")]
NoParticipantForMatch(TournamentMatchId, i32),
```

### 6. AppState Updates

Ensure `AppState` includes all required services:

```rust
pub struct AppState {
    // ... existing fields ...
    pub demo_service: Arc<DemoService<...>>,
    pub result_review_service: Arc<ResultReviewService<...>>,
    pub match_completion_saga: Arc<MatchCompletionSaga<...>>,
}
```

---

## Tests

### Category D: End-to-End Workflow (3 tests)

```rust
#[tokio::test]
async fn test_full_demo_to_result_workflow() {
    let app = TestApp::new().await;

    // Setup: Create tournament, match, register teams with known Steam IDs
    let (tournament_id, match_id) = create_tournament_with_match(&app).await;

    // Link demo to match (demo with matching players/scores)
    let demo_id = create_demo_with_matching_stats(&app, &app.team1_steam_ids, &app.team2_steam_ids).await;
    let link_id = link_demo_to_match(&app, demo_id, match_id, Some(1)).await;

    // Submit result with demo_link_ids
    submit_result_with_demo(&app, tournament_id, match_id, link_id).await;

    // Confirm result (opponent)
    confirm_result(&app, tournament_id, match_id).await;

    // Verify: Match should complete without review
    let match_ = get_match(&app, match_id).await;
    assert_eq!(match_.status, "completed");
}

#[tokio::test]
async fn test_result_with_demo_and_evidence() {
    let app = TestApp::new().await;

    // Create match with both demo links AND traditional evidence
    // Submit result with both types
    // Verify both are stored correctly
}

#[tokio::test]
async fn test_dispute_with_demo_evidence() {
    let app = TestApp::new().await;

    // Submit result with demo showing mismatched score
    // Verify review is created
    // Admin can use demo evidence to inform dispute resolution
}
```

### Category F: Review System (2 more tests)

```rust
#[tokio::test]
async fn test_admin_approval_flow() {
    let app = TestApp::new().await;

    // Create score mismatch review
    let (match_id, review_id) = create_score_mismatch_review(&app).await;

    // Admin approves
    let response = app.post_json_as_admin(
        &format!("/v1/admin/result-reviews/{}/approve", review_id),
        &json!({ "notes": "Verified with team captains" })
    ).await;
    response.assert_status(StatusCode::OK);

    // Verify match completed
    let match_ = get_match(&app, match_id).await;
    assert_eq!(match_.status, "completed");
}

#[tokio::test]
async fn test_admin_rejection_returns_match_to_progress() {
    let app = TestApp::new().await;

    // Create score mismatch review
    let (match_id, review_id) = create_score_mismatch_review(&app).await;

    // Admin rejects
    let response = app.post_json_as_admin(
        &format!("/v1/admin/result-reviews/{}/reject", review_id),
        &json!({ "notes": "Demo evidence shows different winner" })
    ).await;
    response.assert_status(StatusCode::OK);

    // Verify match returned to in_progress
    let match_ = get_match(&app, match_id).await;
    assert_eq!(match_.status, "in_progress");
}
```

---

## Test Helpers

```rust
async fn create_demo_with_matching_stats(
    app: &TestApp,
    team1_steam_ids: &[String],
    team2_steam_ids: &[String],
) -> DemoId {
    // Create demo with players matching the provided Steam IDs
    // Set scores to match expected result
}

async fn create_demo_with_roster_mismatch(app: &TestApp) -> DemoId {
    // Create demo with some players not on either roster
}

async fn create_demo_with_score_mismatch(app: &TestApp) -> DemoId {
    // Create demo with different score than will be claimed
}

async fn submit_result_with_demo(
    app: &TestApp,
    tournament_id: TournamentId,
    match_id: TournamentMatchId,
    demo_link_id: DemoMatchLinkId,
) {
    app.post_json(
        &format!("/v1/tournaments/{}/matches/{}/result/submit", tournament_id, match_id),
        &json!({
            "winner_registration_id": "...",
            "game_results": [{
                "game_number": 1,
                "map_id": "de_dust2",
                "participant1_score": 16,
                "participant2_score": 10,
            }],
            "demo_link_ids": [demo_link_id.to_string()]
        })
    ).await;
}

async fn create_score_mismatch_review(app: &TestApp) -> (TournamentMatchId, ResultReviewId) {
    // Create match
    // Link demo with mismatched score
    // Submit and confirm result
    // Return match_id and review_id
}
```

---

## Acceptance Criteria

- [ ] Match completion saga includes `step_validate_demos()`
- [ ] Saga pauses when review is created with pending status
- [ ] Saga state stored for recovery
- [ ] `continue_after_review()` resumes saga correctly
- [ ] Approval triggers saga continuation
- [ ] Rejection returns match to in_progress status
- [ ] Roster-only acknowledgment by both captains continues saga
- [ ] All DomainError variants added
- [ ] AppState includes required services
- [ ] All 5 integration tests pass
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

---

## Verification

```bash
# Run all Phase 4 tests
cargo test -p portal-api --test demos_test
cargo test -p portal-api --test results_test -- result_with_demo
cargo test -p portal-api --test result_review_test

# Test full workflow manually
# 1. Create tournament with match
# 2. Register teams with known Steam IDs
# 3. Link demo to match
# 4. Submit result with demo_link_ids
# 5. Confirm result
# 6. Verify validation runs
# 7. If review created, test acknowledgment/approval flows
```

---

## Integration Notes

- The saga should be idempotent - re-running after crash should continue correctly
- Validation runs once per demo link, results cached in `demo_match_links.validation_result`
- Multiple demos on the same match all get validated; first failure creates review
- Reviews are match-scoped - one review per match at a time
- Clear logging at each saga step for debugging
