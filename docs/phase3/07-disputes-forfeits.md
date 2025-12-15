# Disputes & Forfeits Design

> **Sub-Phases**: 3.10 (Forfeit Handling), 3.11 (Dispute System)
> **Related**: [04-result-submission.md](./04-result-submission.md), [06-bracket-progression.md](./06-bracket-progression.md)

---

## Overview

This document covers two related systems:

1. **Forfeit Handling**: When a team cannot or will not play (no-show, withdrawal, disqualification)
2. **Dispute System**: When teams disagree about match results

Both systems can trigger bracket progression changes and may require saga patterns for consistency.

---

## Part 1: Forfeit System

### Forfeit Types

| Type | Trigger | Winner | Stats Recorded |
|------|---------|--------|----------------|
| `no_show` | Failed check-in | Opponent auto-wins | Walkover win |
| `withdrawal` | Team withdraws | Opponent auto-wins | Walkover win |
| `disqualification` | Rule violation | Opponent auto-wins | Walkover win |
| `technical_default` | Technical issues | Opponent auto-wins | Walkover win |
| `double_forfeit` | Both teams forfeit | Neither | Match voided |

### Forfeit Effects

1. **Match Status**: Set to `Forfeit`
2. **Winner/Loser**: Set based on forfeit type
3. **Scores**: Set to default forfeit score (e.g., 3-0 for Bo5)
4. **Registration Status**: Forfeiting team → `Disqualified` or `Withdrawn`
5. **Bracket Progression**: Winner advances as normal

### Cascade Forfeits (Double Elimination)

When a team forfeits in Winners Bracket:
- They drop to Losers Bracket (unless also forfeit there)
- If they forfeit again in Losers, they're eliminated

When a team forfeits multiple scheduled matches:
- May trigger automatic tournament-wide disqualification
- All remaining matches become forfeits

---

## Part 2: Dispute System

### Dispute Workflow

```
                    ┌─────────────────────┐
                    │  Result Submitted   │
                    │  (Claim: Pending)   │
                    └──────────┬──────────┘
                               │ Opponent disputes
                               ▼
                    ┌─────────────────────┐
                    │    Dispute Created  │
                    │   Status: Pending   │
                    │                     │
                    │ - Match → Disputed  │
                    │ - Claim → Disputed  │
                    └──────────┬──────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │    Admin Review     │
                    │                     │
                    │ - Review evidence   │
                    │ - Contact teams     │
                    │ - Make decision     │
                    └──────────┬──────────┘
                               │
        ┌──────────────────────┼──────────────────────┐
        │                      │                      │
        ▼                      ▼                      ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│    Uphold     │     │   Overturn    │     │   Rematch     │
│               │     │               │     │               │
│ Original      │     │ Reverse       │     │ Reset match   │
│ result stands │     │ winner/loser  │     │ to Ready      │
└───────┬───────┘     └───────┬───────┘     └───────┬───────┘
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│ Match →       │     │ Revert        │     │ Match →       │
│ Completed     │     │ progression,  │     │ Ready/        │
│               │     │ re-apply with │     │ Scheduled     │
│ Normal        │     │ new winner    │     │               │
│ progression   │     │               │     │ Reschedule    │
└───────────────┘     └───────────────┘     └───────────────┘
```

### Dispute Outcomes

| Outcome | Description | Actions |
|---------|-------------|---------|
| `upheld` | Original result correct | Complete match normally |
| `overturned` | Result was wrong | Reverse winner/loser, re-progress |
| `rematch` | Cannot determine | Reset match, reschedule |
| `adjusted` | Partial correction | Adjust scores, may change winner |
| `double_dq` | Both teams violated rules | Both disqualified |

---

## Database Schema

### disputes

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

COMMENT ON TABLE disputes IS 'Match result disputes for admin resolution';
```

### dispute_messages

```sql
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

COMMENT ON TABLE dispute_messages IS 'Communication thread for dispute resolution';
```

### forfeit_records

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

COMMENT ON TABLE forfeit_records IS 'Record of match forfeits';
```

---

## Domain Entities

### Dispute

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    DisputeId, EvidenceId, ResultClaimId, TournamentMatchId,
    TournamentRegistrationId, UserId,
};

/// A dispute against a match result.
#[derive(Debug, Clone)]
pub struct Dispute {
    pub id: DisputeId,
    pub match_id: TournamentMatchId,
    pub result_claim_id: Option<ResultClaimId>,

    /// Who raised the dispute
    pub disputed_by_registration_id: TournamentRegistrationId,
    pub disputed_by_user_id: UserId,

    /// Dispute details
    pub reason: DisputeReason,
    pub description: String,
    pub evidence_ids: Vec<EvidenceId>,

    /// Original result being disputed
    pub original_winner_registration_id: Option<TournamentRegistrationId>,
    pub original_participant1_score: Option<i32>,
    pub original_participant2_score: Option<i32>,

    /// Status
    pub status: DisputeStatus,
    pub priority: DisputePriority,

    /// Resolution
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
```

### ForfeitRecord

```rust
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
    pub fn default_score(&self, match_format: MatchFormat) -> (i32, i32) {
        // Winner gets max wins needed, loser gets 0
        let winner_score = match_format.wins_required() as i32;
        (winner_score, 0)
    }
}
```

---

## Service Design

### ForfeitService

```rust
pub struct ForfeitService<TMR, TRR, FRR, MLS, PS>
where
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    FRR: ForfeitRecordRepository,
    MLS: MatchLifecycleService,
    PS: ProgressionService,
{
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    forfeit_repo: Arc<FRR>,
    lifecycle_service: Arc<MLS>,
    progression_service: Arc<PS>,
}

impl<TMR, TRR, FRR, MLS, PS> ForfeitService<TMR, TRR, FRR, MLS, PS>
where
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    FRR: ForfeitRecordRepository,
    MLS: MatchLifecycleService,
    PS: ProgressionService,
{
    /// Process a forfeit for a match.
    ///
    /// **NOTE**: Should be called within a saga.
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
}

#[derive(Debug, Clone)]
pub enum ForfeitTrigger {
    System { reason: String },
    User(UserId),
    Admin { user_id: UserId, reason: String },
}

#[derive(Debug, Clone)]
pub struct ForfeitResult {
    pub match_id: TournamentMatchId,
    pub forfeit_record: ForfeitRecord,
    pub winner_registration_id: Option<TournamentRegistrationId>,
    pub progression_result: Option<ProgressionResult>,
}
```

### DisputeService

```rust
pub struct DisputeService<DR, DMR, TMR, TRR, RCR, MLS, PS>
where
    DR: DisputeRepository,
    DMR: DisputeMessageRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    RCR: ResultClaimRepository,
    MLS: MatchLifecycleService,
    PS: ProgressionService,
{
    dispute_repo: Arc<DR>,
    message_repo: Arc<DMR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    claim_repo: Arc<RCR>,
    lifecycle_service: Arc<MLS>,
    progression_service: Arc<PS>,
}

impl<DR, DMR, TMR, TRR, RCR, MLS, PS> DisputeService<DR, DMR, TMR, TRR, RCR, MLS, PS>
where
    DR: DisputeRepository,
    DMR: DisputeMessageRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    RCR: ResultClaimRepository,
    MLS: MatchLifecycleService,
    PS: ProgressionService,
{
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
    ///
    /// **NOTE**: Uses saga for bracket progression reversal.
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

---

## API Endpoints

### Forfeit Endpoints

#### POST /v1/tournaments/{tournament_id}/registrations/{registration_id}/withdraw

Withdraw from tournament.

**Request**:
```json
{
  "reason": "Team disbanded"
}
```

**Response** (200 OK):
```json
{
  "data": {
    "registration_id": "...",
    "status": "withdrawn",
    "forfeited_matches": 3,
    "withdrawn_at": "2025-01-15T21:00:00Z"
  }
}
```

#### POST /v1/admin/tournaments/{tournament_id}/matches/{match_id}/forfeit

Admin force forfeit.

**Request**:
```json
{
  "forfeiting_registration_id": "...",
  "forfeit_type": "disqualification",
  "reason": "Rule violation - use of prohibited software"
}
```

### Dispute Endpoints

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/dispute

Raise a dispute.

**Request**:
```json
{
  "result_claim_id": "...",
  "reason": "wrong_score",
  "description": "The score for map 2 is incorrect. We won 16-14, not 16-10. See attached screenshot.",
  "evidence_ids": ["..."]
}
```

**Response** (201 Created):
```json
{
  "data": {
    "dispute_id": "...",
    "match_id": "...",
    "status": "pending",
    "created_at": "2025-01-15T21:10:00Z"
  }
}
```

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/dispute/{dispute_id}/message

Add message to dispute thread.

**Request**:
```json
{
  "message": "Here is additional evidence from our player's POV",
  "evidence_ids": ["..."]
}
```

#### GET /v1/admin/disputes

Get admin dispute queue.

**Query Parameters**:
- `tournament_id`: Filter by tournament
- `status`: Filter by status
- `priority`: Filter by priority

**Response**:
```json
{
  "data": {
    "disputes": [
      {
        "id": "...",
        "match_id": "...",
        "tournament_name": "Weekly Cup #15",
        "reason": "wrong_score",
        "status": "pending",
        "priority": "normal",
        "created_at": "2025-01-15T21:10:00Z",
        "participants": ["Team Alpha", "Team Beta"]
      }
    ],
    "total": 5
  }
}
```

#### GET /v1/admin/disputes/{dispute_id}

Get dispute details with full thread.

**Response**:
```json
{
  "data": {
    "dispute": {
      "id": "...",
      "status": "under_review",
      "reason": "wrong_score",
      "description": "...",
      "original_result": {
        "winner": "Team Alpha",
        "scores": [2, 1]
      }
    },
    "match": {
      "id": "...",
      "participant1": "Team Alpha",
      "participant2": "Team Beta",
      "status": "disputed"
    },
    "messages": [
      {
        "author": "Team Beta",
        "author_type": "participant",
        "message": "...",
        "created_at": "..."
      },
      {
        "author": "Admin Jane",
        "author_type": "admin",
        "message": "...",
        "is_internal": false,
        "created_at": "..."
      }
    ],
    "evidence": [...]
  }
}
```

#### POST /v1/admin/disputes/{dispute_id}/resolve

Resolve a dispute.

**Request** (uphold):
```json
{
  "resolution_type": "upheld",
  "notes": "After reviewing evidence, the original result appears correct."
}
```

**Request** (overturn):
```json
{
  "resolution_type": "overturned",
  "new_winner_registration_id": "...",
  "new_participant1_score": 1,
  "new_participant2_score": 2,
  "notes": "Demo analysis confirms different score for map 2."
}
```

**Request** (rematch):
```json
{
  "resolution_type": "rematch",
  "notes": "Cannot determine result due to technical issues. Match will be replayed."
}
```

---

## Error Handling

### New Error Types

```rust
pub enum ForfeitError {
    /// Match already completed
    MatchAlreadyCompleted(TournamentMatchId),

    /// Registration not in this match
    NotInMatch {
        registration_id: TournamentRegistrationId,
        match_id: TournamentMatchId,
    },

    /// Already forfeited
    AlreadyForfeited(TournamentMatchId),

    /// Cannot withdraw after tournament started
    CannotWithdrawAfterStart(TournamentId),
}

pub enum DisputeError {
    /// Match not in disputable state
    MatchNotDisputable(TournamentMatchId),

    /// Already have pending dispute
    DisputeAlreadyExists(TournamentMatchId),

    /// Dispute not found
    DisputeNotFound(DisputeId),

    /// Cannot dispute own result claim
    CannotDisputeOwnClaim,

    /// Dispute already resolved
    AlreadyResolved(DisputeId),

    /// Invalid resolution for this dispute
    InvalidResolution(String),
}
```

---

## Testing Notes

### Unit Tests

- Forfeit type default scores
- Dispute status transitions
- Resolution validation

### Integration Tests

```
test_process_no_show_forfeit
test_process_withdrawal
test_process_disqualification
test_double_forfeit
test_cascade_forfeit_double_elim
test_withdraw_from_tournament
test_raise_dispute
test_add_dispute_message
test_resolve_dispute_uphold
test_resolve_dispute_overturn
test_resolve_dispute_rematch
test_overturn_updates_bracket
test_dispute_pauses_progression
```

### Edge Case Tests

```
test_forfeit_in_grand_final
test_dispute_after_next_match_started
test_overturn_with_cascade_changes
test_double_dq_bracket_handling
```
