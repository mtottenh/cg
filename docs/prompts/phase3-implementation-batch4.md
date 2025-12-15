# Phase 3 Implementation - Batch 4: Disputes & Forfeits

## Context

You are implementing **Phase 3 Batch 4** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This batch covers the forfeit handling and dispute resolution systems.

**Prerequisites**: Batches 1, 2, and 3 must be complete (Sub-Phases 3.1-3.9).

**Design Documents** (READ THESE FIRST):
- `docs/phase3/00-overview.md` - Overall architecture
- `docs/phase3/07-disputes-forfeits.md` - Disputes and forfeits design
- `docs/phase3/08-sagas-orchestration.md` - Saga patterns
- `docs/phase3/06-bracket-progression.md` - Progression (for integration)

**Reference Files**:
- `crates/portal-domain/src/services/tournament/progression.rs` - Progression service (from Batch 3)
- `crates/portal-domain/src/services/saga/mod.rs` - Saga coordinator (from Batch 3)
- `crates/portal-domain/src/services/tournament/match_lifecycle.rs` - Match lifecycle

---

## Your Task

Implement **Sub-Phases 3.10 and 3.11** following the design documents exactly.

### Sub-Phases in This Batch

| Sub-Phase | Name | Description |
|-----------|------|-------------|
| 3.10 | Forfeit Handling | No-show, withdrawal, disqualification processing |
| 3.11 | Dispute System | Dispute workflow, admin resolution, bracket correction |

### Implementation Order

```
3.10 Forfeit Handling (SAGA)
         │
         ▼
3.11 Dispute System (SAGA)
```

**Note**: Both sub-phases use saga patterns and integrate with the progression service from Batch 3.

---

## Sub-Phase 3.10: Forfeit Handling

### Scope

Implement forfeit processing with saga pattern as defined in `docs/phase3/07-disputes-forfeits.md`.

### Forfeit Types

| Type | Trigger | Winner | Effect |
|------|---------|--------|--------|
| `no_show` | Failed check-in | Opponent auto-wins | Walkover |
| `withdrawal` | Team withdraws | Opponent auto-wins | Walkover |
| `disqualification` | Rule violation | Opponent auto-wins | Walkover |
| `technical_default` | Technical issues | Opponent auto-wins | Walkover |

### Deliverables

#### 1. Migration: Forfeit Records

Create migration `migrations/0038_forfeits.sql`:

```sql
CREATE TABLE forfeit_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who forfeited
    forfeiting_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),

    -- Type and reason
    forfeit_type VARCHAR(32) NOT NULL,
    reason TEXT,

    -- Triggered by
    triggered_by_user_id UUID REFERENCES users(id),
    triggered_by_system BOOLEAN NOT NULL DEFAULT false,

    -- Timestamps
    forfeited_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT forfeit_records_check_type CHECK (forfeit_type IN (
        'no_show', 'withdrawal', 'disqualification', 'technical_default'
    ))
);

CREATE INDEX idx_forfeit_records_match ON forfeit_records(match_id);
CREATE INDEX idx_forfeit_records_registration ON forfeit_records(forfeiting_registration_id);
```

#### 2. Domain Entities

Create `crates/portal-domain/src/entities/forfeit.rs`:

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    ForfeitRecordId, TournamentMatchId, TournamentRegistrationId, UserId,
};

/// Record of a forfeit.
#[derive(Debug, Clone)]
pub struct ForfeitRecord {
    pub id: ForfeitRecordId,
    pub match_id: TournamentMatchId,
    pub forfeiting_registration_id: TournamentRegistrationId,
    pub forfeit_type: ForfeitType,
    pub reason: Option<String>,
    pub triggered_by_user_id: Option<UserId>,
    pub triggered_by_system: bool,
    pub forfeited_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForfeitType {
    NoShow,
    Withdrawal,
    Disqualification,
    TechnicalDefault,
}

impl ForfeitType {
    /// Get default score for walkover win.
    pub fn default_score(&self, match_format: MatchFormat) -> (i32, i32) {
        let winner_score = match_format.wins_required() as i32;
        (winner_score, 0)
    }
}

#[derive(Debug, Clone)]
pub enum ForfeitTrigger {
    System { reason: String },
    User(UserId),
    Admin { user_id: UserId, reason: String },
}
```

Add `ForfeitRecordId` to `crates/portal-core/src/ids.rs`.

#### 3. Repository

Create `ForfeitRecordRepository` trait and implementation:

```rust
#[async_trait]
pub trait ForfeitRecordRepository: Send + Sync + 'static {
    async fn create(&self, record: &ForfeitRecord) -> Result<ForfeitRecord, DomainError>;
    async fn find_by_id(&self, id: ForfeitRecordId) -> Result<Option<ForfeitRecord>, DomainError>;
    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Option<ForfeitRecord>, DomainError>;
    async fn find_by_registration(&self, registration_id: TournamentRegistrationId) -> Result<Vec<ForfeitRecord>, DomainError>;
}
```

#### 4. Service: ForfeitService

Create `crates/portal-domain/src/services/tournament/forfeit.rs`:

```rust
pub struct ForfeitService<TMR, TRR, FRR, MLS, PS> {
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    forfeit_repo: Arc<FRR>,
    lifecycle_service: Arc<MLS>,
    progression_service: Arc<PS>,
}

impl ForfeitService {
    /// Process a forfeit for a match.
    pub async fn process_forfeit(
        &self,
        match_id: TournamentMatchId,
        forfeiting_registration_id: TournamentRegistrationId,
        forfeit_type: ForfeitType,
        reason: Option<String>,
        triggered_by: ForfeitTrigger,
    ) -> Result<ForfeitResult, DomainError>;

    /// Process double forfeit (both teams forfeit).
    pub async fn process_double_forfeit(
        &self,
        match_id: TournamentMatchId,
        reason: Option<String>,
        triggered_by: ForfeitTrigger,
    ) -> Result<ForfeitResult, DomainError>;

    /// Process no-show after check-in deadline.
    pub async fn process_no_show(
        &self,
        match_id: TournamentMatchId,
        no_show_registration_id: TournamentRegistrationId,
    ) -> Result<ForfeitResult, DomainError>;

    /// Withdraw a team from the tournament.
    ///
    /// Forfeits all their remaining matches.
    pub async fn withdraw_from_tournament(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
        reason: Option<String>,
        withdrawn_by: UserId,
    ) -> Result<Vec<ForfeitResult>, DomainError>;

    /// Disqualify a team from the tournament.
    ///
    /// Forfeits all their remaining matches.
    pub async fn disqualify(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
        reason: String,
        disqualified_by: UserId,
    ) -> Result<Vec<ForfeitResult>, DomainError>;

    /// Find pending matches for a registration.
    async fn find_pending_matches(
        &self,
        tournament_id: TournamentId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<TournamentMatch>, DomainError>;
}

#[derive(Debug, Clone)]
pub struct ForfeitResult {
    pub match_id: TournamentMatchId,
    pub forfeit_record: ForfeitRecord,
    pub winner_registration_id: Option<TournamentRegistrationId>,
    pub progression_result: Option<ProgressionResult>,
}
```

#### 5. Forfeit Processing Saga

Create `crates/portal-domain/src/services/saga/forfeit_processing.rs`:

```rust
pub struct ForfeitProcessingSaga {
    // Dependencies
}

impl SagaDefinition for ForfeitProcessingSaga {
    fn saga_type(&self) -> &str { "forfeit_processing" }
    fn version(&self) -> i32 { 1 }

    fn steps(&self) -> Vec<Box<dyn SagaStep>> {
        vec![
            Box::new(ValidateForfeitStep { ... }),
            Box::new(CreateForfeitRecordStep { ... }),
            Box::new(UpdateMatchStatusStep { ... }),
            Box::new(UpdateRegistrationStatusStep { ... }),
            Box::new(AdvanceOpponentStep { ... }),
            Box::new(HandleCascadeStep { ... }),
            Box::new(UpdateStandingsStep { ... }),
        ]
    }
}
```

**Cascade Handling** (Double Elimination):

When a team forfeits in Winners Bracket and should also be eliminated from Losers Bracket, the `HandleCascadeStep` should:
1. Find all pending matches for the forfeiting registration
2. For each match, create a child forfeit or mark as walkover
3. Ensure proper ordering in double elimination

#### 6. API Handlers

- `POST /v1/tournaments/{id}/registrations/{registration_id}/withdraw` - Withdraw from tournament
- `POST /v1/admin/tournaments/{id}/matches/{match_id}/forfeit` - Admin force forfeit
- `POST /v1/admin/tournaments/{id}/registrations/{registration_id}/disqualify` - Disqualify team

#### 7. Tests

```rust
#[tokio::test]
async fn test_process_no_show_forfeit() { ... }

#[tokio::test]
async fn test_process_withdrawal() { ... }

#[tokio::test]
async fn test_process_disqualification() { ... }

#[tokio::test]
async fn test_double_forfeit() { ... }

#[tokio::test]
async fn test_withdraw_from_tournament() { ... }

#[tokio::test]
async fn test_forfeit_advances_opponent() { ... }

#[tokio::test]
async fn test_cascade_forfeit_double_elim() { ... }

#[tokio::test]
async fn test_forfeit_saga_compensation() { ... }
```

### Acceptance Criteria (3.10)

- [x] No-show forfeits process correctly
- [x] Withdrawal forfeits all remaining matches
- [x] Disqualification works with cascade
- [x] Opponent advances on forfeit
- [x] Double elimination cascade handled
- [x] Walkover scores recorded correctly
- [x] Saga compensation works
- [x] All tests pass (17 forfeit tests)

---

## Sub-Phase 3.11: Dispute System

### Scope

Implement the dispute resolution workflow as defined in `docs/phase3/07-disputes-forfeits.md`.

### Dispute Workflow

```
Result Submitted → Opponent Disputes → Admin Reviews → Resolution
                                                    │
                    ┌──────────────┬────────────────┼────────────────┬──────────────┐
                    │              │                │                │              │
                 Uphold       Overturn          Rematch          Adjusted      Double DQ
                    │              │                │                │              │
              Match stands   Reverse winner   Reset match    Adjust scores   Both DQ'd
                           & re-progress
```

### Resolution Types

| Type | Description | Actions |
|------|-------------|---------|
| `upheld` | Original result correct | Complete match normally |
| `overturned` | Result was wrong | Reverse winner/loser, re-progress |
| `rematch` | Cannot determine | Reset match, reschedule |
| `adjusted` | Partial correction | Adjust scores, may change winner |
| `double_dq` | Both teams violated rules | Both disqualified |

### Deliverables

#### 1. Migration: Disputes

Create migration `migrations/0039_disputes.sql`:

```sql
CREATE TABLE disputes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    result_claim_id UUID REFERENCES result_claims(id) ON DELETE SET NULL,

    -- Who disputed
    disputed_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    disputed_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Dispute details
    reason VARCHAR(64) NOT NULL,
    description TEXT NOT NULL,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- What was claimed vs disputed
    original_winner_registration_id UUID REFERENCES tournament_registrations(id),
    original_participant1_score INTEGER,
    original_participant2_score INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    priority VARCHAR(16) NOT NULL DEFAULT 'normal',

    -- Resolution
    resolved_at TIMESTAMPTZ,
    resolved_by_user_id UUID REFERENCES users(id),
    resolution_type VARCHAR(32),
    resolution_notes TEXT,

    -- For overturned results
    new_winner_registration_id UUID REFERENCES tournament_registrations(id),
    new_participant1_score INTEGER,
    new_participant2_score INTEGER,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT disputes_check_status CHECK (status IN (
        'pending', 'under_review', 'resolved', 'cancelled'
    )),
    CONSTRAINT disputes_check_reason CHECK (reason IN (
        'wrong_score', 'wrong_winner', 'cheating', 'rule_violation',
        'technical_issue', 'player_misconduct', 'other'
    )),
    CONSTRAINT disputes_check_resolution CHECK (
        status != 'resolved' OR resolution_type IS NOT NULL
    ),
    CONSTRAINT disputes_check_resolution_type CHECK (
        resolution_type IS NULL OR resolution_type IN (
            'upheld', 'overturned', 'rematch', 'adjusted', 'double_dq'
        )
    ),
    CONSTRAINT disputes_check_priority CHECK (priority IN (
        'low', 'normal', 'high', 'urgent'
    ))
);

CREATE INDEX idx_disputes_match ON disputes(match_id);
CREATE INDEX idx_disputes_status ON disputes(status);
CREATE INDEX idx_disputes_priority ON disputes(priority, created_at)
    WHERE status IN ('pending', 'under_review');

CREATE TRIGGER disputes_updated_at
    BEFORE UPDATE ON disputes
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE dispute_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dispute_id UUID NOT NULL REFERENCES disputes(id) ON DELETE CASCADE,

    -- Author
    author_user_id UUID NOT NULL REFERENCES users(id),
    author_type VARCHAR(16) NOT NULL,  -- 'participant', 'admin', 'system'

    -- Content
    message TEXT NOT NULL,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- Visibility
    is_internal BOOLEAN NOT NULL DEFAULT false,  -- Admin-only notes

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT dispute_messages_check_author_type CHECK (
        author_type IN ('participant', 'admin', 'system')
    )
);

CREATE INDEX idx_dispute_messages_dispute ON dispute_messages(dispute_id);
```

#### 2. Domain Entities

Create `crates/portal-domain/src/entities/dispute.rs`:

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    DisputeId, DisputeMessageId, EvidenceId, ResultClaimId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};

/// A dispute against a match result.
#[derive(Debug, Clone)]
pub struct Dispute {
    pub id: DisputeId,
    pub match_id: TournamentMatchId,
    pub result_claim_id: Option<ResultClaimId>,
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub original_winner_registration_id: Option<TournamentRegistrationId>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,
    pub status: DisputeStatus,
    pub priority: DisputePriority,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by_user_id: Option<UserId>,
    pub resolution: Option<DisputeResolution>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeReason {
    WrongScore,
    WrongWinner,
    Cheating,
    RuleViolation,
    TechnicalIssue,
    PlayerMisconduct,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeStatus {
    Pending,
    UnderReview,
    Resolved,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputePriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone)]
pub struct DisputeResolution {
    pub resolution_type: ResolutionType,
    pub notes: String,
    pub new_winner_registration_id: Option<TournamentRegistrationId>,
    pub new_participant1_score: Option<i32>,
    pub new_participant2_score: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionType {
    Upheld,
    Overturned,
    Rematch,
    Adjusted,
    DoubleDq,
}

/// A message in a dispute thread.
#[derive(Debug, Clone)]
pub struct DisputeMessage {
    pub id: DisputeMessageId,
    pub dispute_id: DisputeId,
    pub author_user_id: UserId,
    pub author_type: AuthorType,
    pub message: String,
    pub evidence_ids: Vec<EvidenceId>,
    pub is_internal: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorType {
    Participant,
    Admin,
    System,
}
```

Add `DisputeId` and `DisputeMessageId` to `crates/portal-core/src/ids.rs`.

#### 3. Repository

Create `DisputeRepository` and `DisputeMessageRepository` traits and implementations:

```rust
#[async_trait]
pub trait DisputeRepository: Send + Sync + 'static {
    async fn create(&self, dispute: &Dispute) -> Result<Dispute, DomainError>;
    async fn find_by_id(&self, id: DisputeId) -> Result<Option<Dispute>, DomainError>;
    async fn find_by_match(&self, match_id: TournamentMatchId) -> Result<Vec<Dispute>, DomainError>;
    async fn find_pending(&self, tournament_id: Option<TournamentId>, priority: Option<DisputePriority>) -> Result<Vec<Dispute>, DomainError>;
    async fn update(&self, dispute: &Dispute) -> Result<Dispute, DomainError>;
    async fn exists_pending_for_match(&self, match_id: TournamentMatchId) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait DisputeMessageRepository: Send + Sync + 'static {
    async fn create(&self, message: &DisputeMessage) -> Result<DisputeMessage, DomainError>;
    async fn find_by_dispute(&self, dispute_id: DisputeId, include_internal: bool) -> Result<Vec<DisputeMessage>, DomainError>;
}
```

#### 4. Service: DisputeService

Create `crates/portal-domain/src/services/tournament/dispute.rs`:

```rust
pub struct DisputeService<DR, DMR, TMR, TRR, RCR, MLS, PS> {
    dispute_repo: Arc<DR>,
    message_repo: Arc<DMR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    claim_repo: Arc<RCR>,
    lifecycle_service: Arc<MLS>,
    progression_service: Arc<PS>,
}

impl DisputeService {
    /// Raise a dispute against a match result.
    pub async fn raise_dispute(
        &self,
        match_id: TournamentMatchId,
        result_claim_id: Option<ResultClaimId>,
        reason: DisputeReason,
        description: String,
        evidence_ids: Vec<EvidenceId>,
        disputed_by: UserId,
    ) -> Result<Dispute, DomainError>;

    /// Add a message to a dispute thread.
    pub async fn add_message(
        &self,
        dispute_id: DisputeId,
        message: String,
        evidence_ids: Vec<EvidenceId>,
        author: UserId,
        is_internal: bool,
    ) -> Result<DisputeMessage, DomainError>;

    /// Assign dispute for review.
    pub async fn assign_for_review(
        &self,
        dispute_id: DisputeId,
        assigned_by: UserId,
    ) -> Result<Dispute, DomainError>;

    /// Resolve dispute with uphold (original result stands).
    pub async fn resolve_uphold(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError>;

    /// Resolve dispute by overturning result.
    pub async fn resolve_overturn(
        &self,
        dispute_id: DisputeId,
        new_winner_registration_id: TournamentRegistrationId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError>;

    /// Resolve dispute by ordering a rematch.
    pub async fn resolve_rematch(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError>;

    /// Resolve dispute with adjusted scores.
    pub async fn resolve_adjusted(
        &self,
        dispute_id: DisputeId,
        new_participant1_score: i32,
        new_participant2_score: i32,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError>;

    /// Resolve dispute by disqualifying both teams.
    pub async fn resolve_double_dq(
        &self,
        dispute_id: DisputeId,
        notes: String,
        resolved_by: UserId,
    ) -> Result<DisputeResolutionResult, DomainError>;

    /// Get pending disputes (admin queue).
    pub async fn get_pending_disputes(
        &self,
        tournament_id: Option<TournamentId>,
        priority: Option<DisputePriority>,
    ) -> Result<Vec<Dispute>, DomainError>;

    /// Get dispute with full thread.
    pub async fn get_dispute_with_thread(
        &self,
        dispute_id: DisputeId,
        include_internal: bool,
    ) -> Result<DisputeWithThread, DomainError>;
}

#[derive(Debug, Clone)]
pub struct DisputeResolutionResult {
    pub dispute: Dispute,
    pub match_: TournamentMatch,
    pub progression_changes: Option<ProgressionChanges>,
}

#[derive(Debug, Clone)]
pub struct ProgressionChanges {
    pub reverted_matches: Vec<TournamentMatchId>,
    pub updated_matches: Vec<TournamentMatchId>,
    pub new_winner_path: Vec<TournamentMatchId>,
}

#[derive(Debug, Clone)]
pub struct DisputeWithThread {
    pub dispute: Dispute,
    pub messages: Vec<DisputeMessage>,
    pub match_: TournamentMatch,
    pub evidence: Vec<Evidence>,
}
```

#### 5. Dispute Resolution Saga (Overturn)

Create `crates/portal-domain/src/services/saga/dispute_resolution.rs`:

```rust
pub struct DisputeResolutionSaga {
    // Dependencies
}

impl SagaDefinition for DisputeResolutionSaga {
    fn saga_type(&self) -> &str { "dispute_resolution" }
    fn version(&self) -> i32 { 1 }

    fn steps(&self) -> Vec<Box<dyn SagaStep>> {
        vec![
            Box::new(ValidateResolutionStep { ... }),
            Box::new(MarkDisputeResolvedStep { ... }),
            Box::new(RevertOriginalProgressionStep { ... }),
            Box::new(UpdateMatchResultStep { ... }),
            Box::new(ApplyNewProgressionStep { ... }),
            Box::new(HandleDownstreamMatchesStep { ... }),
            Box::new(RecalculateStandingsStep { ... }),
        ]
    }
}
```

**Handling Downstream Matches**:

When overturning a result where the original winner has already played additional matches:

1. **Option A (Default)**: Allow downstream matches to stand
   - Log that original progression was overturned
   - New winner path noted but not enforced

2. **Option B (Admin Choice)**: Cascade changes
   - Void affected downstream matches
   - Re-run progression with new winner
   - Most disruptive, rarely used

3. **Option C**: Order rematches for affected matches
   - Reset downstream matches to Ready
   - Schedule for replay

#### 6. Update Match Lifecycle

Ensure `MatchLifecycleService` supports:
- Transitioning match to `Disputed` status
- Transitioning back to `Ready` for rematch
- Storing dispute reference on match

#### 7. API Handlers

**Participant Endpoints**:
- `POST /v1/tournaments/{id}/matches/{match_id}/dispute` - Raise a dispute
- `POST /v1/tournaments/{id}/matches/{match_id}/dispute/{dispute_id}/message` - Add message

**Admin Endpoints**:
- `GET /v1/admin/disputes` - Get dispute queue
- `GET /v1/admin/disputes/{dispute_id}` - Get dispute details with thread
- `POST /v1/admin/disputes/{dispute_id}/assign` - Assign for review
- `POST /v1/admin/disputes/{dispute_id}/resolve` - Resolve dispute
- `POST /v1/admin/disputes/{dispute_id}/message` - Add admin message (can be internal)

#### 8. Request/Response DTOs

```rust
// Raise dispute request
pub struct RaiseDisputeRequest {
    pub result_claim_id: Option<ResultClaimId>,
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,
}

// Resolve dispute request
pub struct ResolveDisputeRequest {
    pub resolution_type: ResolutionType,
    pub notes: String,
    // Only for overturn/adjusted
    pub new_winner_registration_id: Option<TournamentRegistrationId>,
    pub new_participant1_score: Option<i32>,
    pub new_participant2_score: Option<i32>,
}

// Dispute queue response
pub struct DisputeQueueResponse {
    pub disputes: Vec<DisputeSummary>,
    pub total: i64,
}

pub struct DisputeSummary {
    pub id: DisputeId,
    pub match_id: TournamentMatchId,
    pub tournament_name: String,
    pub reason: DisputeReason,
    pub status: DisputeStatus,
    pub priority: DisputePriority,
    pub created_at: DateTime<Utc>,
    pub participants: Vec<String>,
}
```

#### 9. Tests

```rust
#[tokio::test]
async fn test_raise_dispute() { ... }

#[tokio::test]
async fn test_add_dispute_message() { ... }

#[tokio::test]
async fn test_dispute_sets_match_disputed() { ... }

#[tokio::test]
async fn test_resolve_dispute_uphold() { ... }

#[tokio::test]
async fn test_resolve_dispute_overturn() { ... }

#[tokio::test]
async fn test_resolve_dispute_rematch() { ... }

#[tokio::test]
async fn test_overturn_updates_bracket() { ... }

#[tokio::test]
async fn test_overturn_with_downstream_matches() { ... }

#[tokio::test]
async fn test_cannot_dispute_own_claim() { ... }

#[tokio::test]
async fn test_dispute_already_exists() { ... }

#[tokio::test]
async fn test_dispute_resolution_saga_success() { ... }

#[tokio::test]
async fn test_dispute_resolution_saga_compensation() { ... }
```

### Acceptance Criteria (3.11)

- [x] Disputes can be raised with evidence
- [x] Match transitions to Disputed status
- [x] Message thread works (public and internal)
- [x] Uphold resolution completes match normally
- [x] Overturn reverses and re-applies progression
- [x] Rematch resets match to Ready
- [x] Adjusted updates scores correctly
- [x] Double DQ disqualifies both teams
- [x] Saga compensation works
- [x] All tests pass (32 dispute tests)

---

## Error Handling

### New Error Types

Add to `crates/portal-core/src/errors.rs`:

```rust
pub enum ForfeitError {
    MatchAlreadyCompleted(TournamentMatchId),
    NotInMatch {
        registration_id: TournamentRegistrationId,
        match_id: TournamentMatchId,
    },
    AlreadyForfeited(TournamentMatchId),
    CannotWithdrawAfterStart(TournamentId),
}

pub enum DisputeError {
    MatchNotDisputable(TournamentMatchId),
    DisputeAlreadyExists(TournamentMatchId),
    DisputeNotFound(DisputeId),
    CannotDisputeOwnClaim,
    AlreadyResolved(DisputeId),
    InvalidResolution(String),
}
```

---

## Verification Checklist

Before considering this batch complete:

### Sub-Phase 3.10
- [x] No-show forfeit works
- [x] Withdrawal works
- [x] Disqualification works
- [x] Cascade forfeit works (double elim)
- [x] Opponent advances correctly
- [x] Saga compensation works
- [x] Integration tests pass (17 tests)

### Sub-Phase 3.11
- [x] Dispute creation works
- [x] Message thread works
- [x] All resolution types work
- [x] Overturn reverses progression
- [x] Rematch resets match
- [x] Saga compensation works
- [x] Integration tests pass (32 tests)

### Overall
- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace` passes
- [x] OpenAPI docs complete

---

## Phase 3 Completion

After completing Batch 4, Phase 3 is complete. Verify all acceptance criteria from the design documents:

1. **Scheduling**: Teams can negotiate match times ✓
2. **Pick-Ban**: Map veto completes with game-specific formats ✓
3. **Results**: Results submitted, confirmed, and disputed ✓
4. **Evidence**: Demo files linkable from external storage ✓
5. **Progression**: Winners advance automatically ✓
6. **Forfeits**: No-shows and withdrawals handled ✓
7. **Disputes**: Results can be disputed and corrected ✓
8. **Sagas**: Multi-step operations are atomic and recoverable ✓

Update `docs/tournament-implementation-progress.md` to mark Phase 3 as complete.

---

## Output

After completing this batch:
1. Run full test suite: `cargo test --workspace`
2. Run clippy: `cargo clippy --workspace -- -D warnings`
3. Update progress documentation
4. Note any deviations from design

**Phase 3 is now complete. Proceed to Phase 4 (Admin & Operations) when ready.**

---

## Status: ✅ COMPLETE

**Completed**: 2025-12-01

### Implementation Summary

**Sub-Phase 3.10 - Forfeit Handling (COMPLETE)**
- ✅ Migration: Forfeit records table with type tracking
- ✅ Domain entities: `ForfeitRecord`, `ForfeitType`, `ForfeitTrigger`
- ✅ Service: `ForfeitService` with no-show, withdrawal, disqualification, double forfeit
- ✅ Handlers: Player withdrawal and admin forfeit endpoints
- ✅ Tests: 17 forfeit tests passing

**Sub-Phase 3.11 - Dispute System (COMPLETE)**
- ✅ Migration: Disputes and dispute messages tables
- ✅ Domain entities: `Dispute`, `DisputeMessage`, resolution types
- ✅ Service: `DisputeService` with all resolution types (upheld, overturned, rematch, adjusted, double_dq)
- ✅ Handlers: Participant dispute raising, admin resolution endpoints
- ✅ Tests: 32 dispute tests passing

**Files Created:**
- `crates/portal-domain/src/entities/dispute.rs`
- `crates/portal-domain/src/services/tournament/forfeit.rs`
- `crates/portal-domain/src/services/tournament/dispute.rs`
- `crates/portal-db/src/adapters/tournament/forfeit.rs`
- `crates/portal-db/src/adapters/tournament/dispute.rs`
- `crates/portal-api/src/handlers/forfeit.rs`
- `crates/portal-api/src/handlers/dispute.rs`
- `crates/portal-api/src/dto/requests/forfeit.rs`
- `crates/portal-api/src/dto/responses/forfeit.rs`
- `crates/portal-api/src/dto/requests/dispute.rs`
- `crates/portal-api/src/dto/responses/dispute.rs`
- `crates/portal-api/tests/forfeit_test.rs`
- `crates/portal-api/tests/dispute_test.rs`

**Test Results:**
- Forfeit tests: 17 passing
- Dispute tests: 32 passing
- Total batch tests: 49 passing
