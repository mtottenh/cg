# Saga Patterns & Orchestration

> **Applies to**: Sub-phases 3.9, 3.10, 3.11
> **Related**: All Phase 3 documents

---

## Overview

A **saga** is a sequence of operations that must either all succeed or be compensated (rolled back). In Phase 3, several operations involve multiple coordinated updates across matches, registrations, standings, and brackets.

This document identifies all saga candidates, defines their steps and compensation strategies, and specifies the implementation approach.

### Why Sagas?

Single database transactions can handle simple updates, but these scenarios need sagas:

1. **Match Completion**: Result confirmation triggers bracket advancement affecting multiple matches
2. **Forfeit Processing**: May cascade through multiple matches in double elimination
3. **Dispute Resolution**: Overturning a result requires reverting and re-applying progression
4. **Tournament Start**: Closing registration, processing no-shows, generating brackets

### Saga Design Principles

- **Orchestration over Choreography**: Central coordinator manages saga flow (easier to reason about)
- **Idempotent Steps**: Each step can be safely retried
- **Persistent State**: Saga state stored in database for recovery
- **Compensating Actions**: Every step has a defined undo operation

---

## Database Schema

### saga_executions

```sql
CREATE TABLE saga_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Saga identity
    saga_type VARCHAR(64) NOT NULL,
    saga_version INTEGER NOT NULL DEFAULT 1,

    -- Context
    tournament_id UUID REFERENCES tournaments(id) ON DELETE SET NULL,
    match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    correlation_id VARCHAR(128),  -- For external reference

    -- Input data
    input_data JSONB NOT NULL,

    -- Current state
    current_step INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Step history
    step_history JSONB NOT NULL DEFAULT '[]',

    -- Error tracking
    last_error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,

    -- Timing
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT saga_executions_check_status CHECK (status IN (
        'pending', 'running', 'completed', 'failed', 'compensating', 'compensated'
    ))
);

CREATE INDEX idx_saga_executions_status ON saga_executions(status);
CREATE INDEX idx_saga_executions_type ON saga_executions(saga_type, status);
CREATE INDEX idx_saga_executions_tournament ON saga_executions(tournament_id)
    WHERE tournament_id IS NOT NULL;
CREATE INDEX idx_saga_executions_match ON saga_executions(match_id)
    WHERE match_id IS NOT NULL;

CREATE TRIGGER saga_executions_updated_at
    BEFORE UPDATE ON saga_executions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE saga_executions IS 'Saga execution state for multi-step operations';
```

### step_history JSON structure

```json
{
  "step_history": [
    {
      "step": 0,
      "name": "validate_result",
      "status": "completed",
      "started_at": "2025-01-15T21:00:00Z",
      "completed_at": "2025-01-15T21:00:01Z",
      "output": {"validated": true}
    },
    {
      "step": 1,
      "name": "update_match_result",
      "status": "completed",
      "started_at": "2025-01-15T21:00:01Z",
      "completed_at": "2025-01-15T21:00:02Z",
      "output": {"match_id": "...", "winner_id": "..."}
    },
    {
      "step": 2,
      "name": "advance_winner",
      "status": "failed",
      "started_at": "2025-01-15T21:00:02Z",
      "error": "Next match position already filled",
      "retry_count": 3
    }
  ]
}
```

---

## Saga Coordinator

### Core Implementation

```rust
use std::sync::Arc;
use async_trait::async_trait;

/// Saga coordinator manages saga execution.
pub struct SagaCoordinator<SR>
where
    SR: SagaRepository,
{
    saga_repo: Arc<SR>,
    registrations: HashMap<String, Box<dyn SagaDefinition>>,
}

impl<SR> SagaCoordinator<SR>
where
    SR: SagaRepository,
{
    /// Register a saga definition.
    pub fn register<S: SagaDefinition + 'static>(&mut self, saga: S) {
        self.registrations.insert(saga.saga_type().to_string(), Box::new(saga));
    }

    /// Start a new saga execution.
    pub async fn start(
        &self,
        saga_type: &str,
        input: serde_json::Value,
        context: SagaContext,
    ) -> Result<SagaExecution, SagaError> {
        let definition = self.registrations.get(saga_type)
            .ok_or(SagaError::UnknownSagaType(saga_type.to_string()))?;

        let execution = SagaExecution {
            id: SagaExecutionId::new(),
            saga_type: saga_type.to_string(),
            saga_version: definition.version(),
            tournament_id: context.tournament_id,
            match_id: context.match_id,
            correlation_id: context.correlation_id,
            input_data: input,
            current_step: 0,
            status: SagaStatus::Pending,
            step_history: vec![],
            last_error: None,
            retry_count: 0,
            max_retries: definition.max_retries(),
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.saga_repo.create(&execution).await?;
        self.execute(execution).await
    }

    /// Continue executing a saga.
    async fn execute(&self, mut execution: SagaExecution) -> Result<SagaExecution, SagaError> {
        let definition = self.registrations.get(&execution.saga_type)
            .ok_or(SagaError::UnknownSagaType(execution.saga_type.clone()))?;

        execution.status = SagaStatus::Running;
        execution.started_at = Some(Utc::now());
        self.saga_repo.update(&execution).await?;

        let steps = definition.steps();

        while execution.current_step < steps.len() as i32 {
            let step = &steps[execution.current_step as usize];
            let step_result = self.execute_step(&execution, step.as_ref()).await;

            match step_result {
                Ok(output) => {
                    // Record success
                    execution.step_history.push(StepRecord {
                        step: execution.current_step,
                        name: step.name().to_string(),
                        status: StepStatus::Completed,
                        started_at: Utc::now(),
                        completed_at: Some(Utc::now()),
                        output: Some(output),
                        error: None,
                        retry_count: 0,
                    });
                    execution.current_step += 1;
                    execution.retry_count = 0;
                    self.saga_repo.update(&execution).await?;
                }
                Err(e) => {
                    execution.retry_count += 1;
                    execution.last_error = Some(e.to_string());

                    if execution.retry_count < execution.max_retries {
                        // Retry after delay
                        tokio::time::sleep(self.retry_delay(execution.retry_count)).await;
                        continue;
                    }

                    // Max retries exceeded - compensate
                    execution.step_history.push(StepRecord {
                        step: execution.current_step,
                        name: step.name().to_string(),
                        status: StepStatus::Failed,
                        started_at: Utc::now(),
                        completed_at: None,
                        output: None,
                        error: Some(e.to_string()),
                        retry_count: execution.retry_count,
                    });

                    return self.compensate(execution, &steps).await;
                }
            }
        }

        // All steps completed
        execution.status = SagaStatus::Completed;
        execution.completed_at = Some(Utc::now());
        self.saga_repo.update(&execution).await?;

        Ok(execution)
    }

    /// Run compensation for failed saga.
    async fn compensate(
        &self,
        mut execution: SagaExecution,
        steps: &[Box<dyn SagaStep>],
    ) -> Result<SagaExecution, SagaError> {
        execution.status = SagaStatus::Compensating;
        self.saga_repo.update(&execution).await?;

        // Compensate in reverse order
        for step_idx in (0..execution.current_step).rev() {
            let step = &steps[step_idx as usize];

            if let Some(compensation) = step.compensation() {
                let step_record = &execution.step_history[step_idx as usize];

                match compensation.execute(&execution.input_data, step_record.output.as_ref()).await {
                    Ok(_) => {
                        // Record compensation success
                    }
                    Err(e) => {
                        // Compensation failed - manual intervention needed
                        execution.status = SagaStatus::Failed;
                        execution.last_error = Some(format!(
                            "Compensation failed at step {}: {}",
                            step_idx, e
                        ));
                        self.saga_repo.update(&execution).await?;
                        return Err(SagaError::CompensationFailed(execution.id));
                    }
                }
            }
        }

        execution.status = SagaStatus::Compensated;
        execution.completed_at = Some(Utc::now());
        self.saga_repo.update(&execution).await?;

        Err(SagaError::SagaCompensated(execution.id))
    }

    fn retry_delay(&self, retry_count: i32) -> Duration {
        // Exponential backoff: 1s, 2s, 4s, 8s...
        Duration::from_secs(2u64.pow(retry_count as u32 - 1))
    }
}
```

### Saga Definition Trait

```rust
#[async_trait]
pub trait SagaDefinition: Send + Sync {
    fn saga_type(&self) -> &str;
    fn version(&self) -> i32;
    fn max_retries(&self) -> i32 { 3 }
    fn steps(&self) -> Vec<Box<dyn SagaStep>>;
}

#[async_trait]
pub trait SagaStep: Send + Sync {
    fn name(&self) -> &str;

    async fn execute(
        &self,
        input: &serde_json::Value,
        context: &SagaStepContext,
    ) -> Result<serde_json::Value, SagaError>;

    fn compensation(&self) -> Option<Box<dyn SagaCompensation>> {
        None
    }

    fn is_idempotent(&self) -> bool {
        true
    }
}

#[async_trait]
pub trait SagaCompensation: Send + Sync {
    async fn execute(
        &self,
        input: &serde_json::Value,
        step_output: Option<&serde_json::Value>,
    ) -> Result<(), SagaError>;
}
```

---

## Identified Sagas

### 1. Match Completion Saga

**Trigger**: Result claim confirmed (manual or auto)

**Steps**:

| # | Step | Action | Compensation |
|---|------|--------|--------------|
| 0 | `validate_result` | Validate result against rules | N/A |
| 1 | `finalize_result_claim` | Set claim status to confirmed | Revert to pending |
| 2 | `update_match_status` | Set match to Completed, record scores | Revert to AwaitingResult |
| 3 | `advance_winner` | Place winner in next match | Remove from next match |
| 4 | `route_loser` | Handle loser (eliminate or drop) | Revert loser status |
| 5 | `update_standings` | Update RR/Swiss standings | Recalculate standings |
| 6 | `check_bracket_completion` | Check if bracket complete | N/A |
| 7 | `check_tournament_completion` | Check if tournament complete | N/A |
| 8 | `update_ready_matches` | Mark newly-ready matches | N/A |
| 9 | `send_notifications` | Notify participants | N/A |

```rust
pub struct MatchCompletionSaga {
    result_service: Arc<ResultService>,
    match_service: Arc<MatchLifecycleService>,
    progression_service: Arc<ProgressionService>,
    standings_service: Arc<StandingsService>,
    notification_service: Arc<NotificationService>,
}

impl SagaDefinition for MatchCompletionSaga {
    fn saga_type(&self) -> &str { "match_completion" }
    fn version(&self) -> i32 { 1 }

    fn steps(&self) -> Vec<Box<dyn SagaStep>> {
        vec![
            Box::new(ValidateResultStep { ... }),
            Box::new(FinalizeClaimStep { ... }),
            Box::new(UpdateMatchStatusStep { ... }),
            Box::new(AdvanceWinnerStep { ... }),
            Box::new(RouteLoserStep { ... }),
            Box::new(UpdateStandingsStep { ... }),
            Box::new(CheckBracketCompletionStep { ... }),
            Box::new(CheckTournamentCompletionStep { ... }),
            Box::new(UpdateReadyMatchesStep { ... }),
            Box::new(SendNotificationsStep { ... }),
        ]
    }
}
```

**Input**:
```json
{
  "match_id": "...",
  "result_claim_id": "...",
  "winner_registration_id": "...",
  "loser_registration_id": "...",
  "participant1_score": 2,
  "participant2_score": 1,
  "confirmed_by": "user_id or auto"
}
```

---

### 2. Forfeit Processing Saga

**Trigger**: No-show, withdrawal, or disqualification

**Steps**:

| # | Step | Action | Compensation |
|---|------|--------|--------------|
| 0 | `validate_forfeit` | Ensure match can be forfeited | N/A |
| 1 | `create_forfeit_record` | Record the forfeit | Delete record |
| 2 | `update_match_status` | Set match to Forfeit | Revert status |
| 3 | `update_registration_status` | Mark team as eliminated/withdrawn | Revert status |
| 4 | `advance_opponent` | Place opponent in next match | Remove from next match |
| 5 | `handle_cascade` | Process cascade (double elim) | Revert cascade |
| 6 | `update_standings` | Update standings if applicable | Recalculate |
| 7 | `send_notifications` | Notify participants | N/A |

```rust
pub struct ForfeitProcessingSaga { ... }

impl SagaDefinition for ForfeitProcessingSaga {
    fn saga_type(&self) -> &str { "forfeit_processing" }
    fn version(&self) -> i32 { 1 }
    // ...
}
```

**Input**:
```json
{
  "match_id": "...",
  "forfeiting_registration_id": "...",
  "forfeit_type": "no_show",
  "reason": "Failed to check in",
  "triggered_by": {"system": "check_in_expiry"}
}
```

**Cascade Handling** (Double Elimination):

When a team forfeits in Winners Bracket and has no remaining matches in Losers:
```
Step 5 (handle_cascade) finds all matches where:
- forfeiting_registration_id is a participant
- Match status is Pending or Ready

For each such match, creates a child ForfeitProcessingSaga.
```

---

### 3. Dispute Resolution Saga (Overturn)

**Trigger**: Admin resolves dispute with "overturn" outcome

**Steps**:

| # | Step | Action | Compensation |
|---|------|--------|--------------|
| 0 | `validate_resolution` | Ensure dispute can be resolved | N/A |
| 1 | `mark_dispute_resolved` | Update dispute status | Revert to under_review |
| 2 | `revert_original_progression` | Remove original winner from bracket | Re-apply original |
| 3 | `update_match_result` | Set new winner/scores | Revert to original |
| 4 | `apply_new_progression` | Advance new winner | Revert progression |
| 5 | `handle_downstream_matches` | Update affected downstream matches | Revert downstream |
| 6 | `recalculate_standings` | Recalculate if RR/Swiss | Revert standings |
| 7 | `send_notifications` | Notify all affected | N/A |

**Input**:
```json
{
  "dispute_id": "...",
  "match_id": "...",
  "resolution_type": "overturned",
  "new_winner_registration_id": "...",
  "new_participant1_score": 1,
  "new_participant2_score": 2,
  "resolved_by": "admin_user_id",
  "notes": "Demo review confirms different result"
}
```

**Downstream Match Handling**:

If original winner W1 was advanced and played more matches:
1. Find all matches where W1 participated after the disputed match
2. Determine if those results are affected
3. Options:
   - Void downstream matches (extreme, rarely used)
   - Allow them to stand (most common)
   - Schedule rematches

---

### 4. Tournament Start Saga

**Trigger**: Tournament transitions from Registration/CheckIn to InProgress

**Steps**:

| # | Step | Action | Compensation |
|---|------|--------|--------------|
| 0 | `validate_can_start` | Check minimum participants, etc. | N/A |
| 1 | `close_registration` | Update tournament status | Revert to registration |
| 2 | `process_no_shows` | Handle non-checked-in participants | Revert to checked_in |
| 3 | `finalize_seeding` | Lock in final seed order | N/A |
| 4 | `generate_brackets` | Create bracket structure | Delete brackets |
| 5 | `create_initial_matches` | Create first-round matches | Delete matches |
| 6 | `populate_seeded_matches` | Place seeded participants | Clear participants |
| 7 | `process_byes` | Handle bye advancement | Revert byes |
| 8 | `update_tournament_status` | Set to InProgress | Revert status |
| 9 | `send_notifications` | Notify all participants | N/A |

**Input**:
```json
{
  "tournament_id": "...",
  "started_by": "admin_user_id"
}
```

---

### 5. Pick-Ban Completion Saga

**Trigger**: All veto actions complete

**Steps**:

| # | Step | Action | Compensation |
|---|------|--------|--------------|
| 0 | `validate_veto_complete` | Ensure all maps selected | N/A |
| 1 | `finalize_veto_session` | Set session to Completed | Revert to in_progress |
| 2 | `create_match_games` | Create game entries for maps | Delete games |
| 3 | `update_match_status` | Set match to InProgress | Revert to PickBan |
| 4 | `request_server` | Request game server (if applicable) | Cancel request |
| 5 | `send_notifications` | Notify match ready | N/A |

**Input**:
```json
{
  "match_id": "...",
  "veto_session_id": "...",
  "selected_maps": ["de_mirage", "de_inferno", "de_ancient"]
}
```

---

## Not Sagas

These operations use single database transactions:

| Operation | Why Not a Saga |
|-----------|---------------|
| Schedule match | Single match update |
| Submit result claim | Creates claim, doesn't trigger progression |
| Add dispute message | Single insert |
| Update availability | Single entity |
| Check in for match | Single match update |
| Veto action | Single action, session update |

---

## Implementation Approach

### Orchestration Pattern

We use **orchestration** (central coordinator) rather than **choreography** (event-driven):

**Advantages**:
- Explicit flow control
- Easier debugging and monitoring
- Clear compensation path
- Saga state is observable

**Trade-offs**:
- Central coordinator can be a bottleneck
- Single point of failure (mitigated by persistence)

### Idempotency Strategy

Each step must be idempotent for safe retries:

```rust
impl SagaStep for AdvanceWinnerStep {
    async fn execute(
        &self,
        input: &serde_json::Value,
        context: &SagaStepContext,
    ) -> Result<serde_json::Value, SagaError> {
        let winner_id: TournamentRegistrationId = input.get("winner_registration_id")...;
        let next_match_id: TournamentMatchId = input.get("next_match_id")...;

        // Check if already advanced (idempotency)
        let next_match = self.match_repo.find_by_id(next_match_id).await?;

        if next_match.participant1_registration_id == Some(winner_id)
            || next_match.participant2_registration_id == Some(winner_id)
        {
            // Already advanced, skip
            return Ok(json!({
                "status": "already_completed",
                "next_match_id": next_match_id
            }));
        }

        // Perform advancement...
    }
}
```

### Failure Modes

| Failure | Recovery |
|---------|----------|
| Step fails, retries succeed | Continue saga |
| Step fails, max retries exceeded | Run compensation, mark saga failed |
| Compensation fails | Mark saga failed, alert admin |
| Server crash during saga | Background job resumes pending sagas |
| Database unavailable | Retry with exponential backoff |

### Background Recovery Job

```rust
/// Job to resume interrupted sagas.
pub async fn saga_recovery_job(coordinator: &SagaCoordinator) {
    loop {
        // Find sagas stuck in running/compensating
        let stuck_sagas = coordinator.find_stuck_sagas(Duration::from_secs(300)).await;

        for saga in stuck_sagas {
            match saga.status {
                SagaStatus::Running => {
                    // Resume from current step
                    coordinator.resume(saga.id).await;
                }
                SagaStatus::Compensating => {
                    // Continue compensation
                    coordinator.continue_compensation(saga.id).await;
                }
                _ => {}
            }
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
```

---

## Monitoring & Alerting

### Metrics

- `saga_started_total{saga_type}` - Counter of started sagas
- `saga_completed_total{saga_type, status}` - Counter by outcome
- `saga_duration_seconds{saga_type}` - Histogram of duration
- `saga_step_duration_seconds{saga_type, step}` - Per-step timing
- `saga_retry_total{saga_type, step}` - Retry counter

### Alerts

- Saga stuck in Running/Compensating > 5 minutes
- Compensation failure rate > 0
- Saga failure rate > 5% in last hour

### Admin Dashboard

Expose endpoint for saga monitoring:

```
GET /admin/sagas
GET /admin/sagas/{id}
POST /admin/sagas/{id}/retry
POST /admin/sagas/{id}/cancel
```

---

## API Integration

### Sync vs Async Sagas

**Synchronous** (wait for completion):
- Match completion (needs result for client)
- Forfeit processing (needs confirmation)

**Asynchronous** (return saga ID):
- Tournament start (can take time)
- Dispute resolution (admin can wait)

### API Response Pattern

```json
// Sync saga response
{
  "data": {
    "match": { ... },
    "progression": { ... }
  },
  "saga": {
    "id": "...",
    "status": "completed"
  }
}

// Async saga response
{
  "data": {
    "saga_id": "...",
    "status": "pending",
    "poll_url": "/admin/sagas/..."
  }
}
```

---

## Testing Notes

### Unit Tests

- Saga coordinator step execution
- Compensation flow
- Idempotency checks
- Retry logic

### Integration Tests

```
test_match_completion_saga_success
test_match_completion_saga_step_failure
test_match_completion_saga_compensation
test_forfeit_saga_with_cascade
test_dispute_overturn_saga
test_tournament_start_saga
test_saga_recovery_after_crash
test_saga_idempotent_retry
```

### Chaos Tests

```
test_saga_survives_db_disconnect
test_saga_survives_coordinator_restart
test_concurrent_saga_executions
```
