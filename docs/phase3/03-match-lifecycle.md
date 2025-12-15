# Match Lifecycle Design

> **Sub-Phase**: 3.1 (Match Lifecycle Core)
> **Related**: [01-scheduling-system.md](./01-scheduling-system.md), [02-pick-ban-system.md](./02-pick-ban-system.md)

---

## Overview

The Match Lifecycle defines all possible states a tournament match can be in and the valid transitions between them. This is the foundation for the entire match system - scheduling, veto, results, and progression all depend on the match being in the correct state.

### Design Goals

- **Complete State Coverage**: Every match scenario has a defined state
- **Clear Transitions**: Explicit rules for what triggers each transition
- **Audit Trail**: All transitions logged with actor and timestamp
- **Side Effects**: Defined actions triggered by each transition
- **Timeout Handling**: Background jobs for time-based transitions

---

## State Machine

### Complete State Diagram

```
                                    ┌─────────────────────┐
                                    │      Pending        │
                                    │  (waiting for both  │
                                    │   participants)     │
                                    └──────────┬──────────┘
                                               │
                                               │ both participants set
                                               ▼
                                    ┌─────────────────────┐
                                    │       Ready         │
                                    │  (can be scheduled) │
                                    └──────────┬──────────┘
                                               │
              ┌────────────────────────────────┼────────────────────────────────┐
              │                                │                                │
              │ self-scheduled                 │ admin-scheduled                │ auto-scheduled
              │ proposal accepted              │                                │ (live mode)
              ▼                                ▼                                ▼
┌─────────────────────┐             ┌─────────────────────┐             ┌──────────────────┐
│     Scheduled       │◄────────────│     Scheduled       │◄────────────│   Scheduled      │
│                     │             │                     │             │                  │
└──────────┬──────────┘             └──────────┬──────────┘             └────────┬─────────┘
           │                                   │                                 │
           │ check-in window opens             │                                 │
           │ (if required)                     │                                 │
           ▼                                   │                                 │
┌─────────────────────┐                        │                                 │
│    CheckingIn       │                        │                                 │
│  (pre-match check)  │                        │                                 │
└──────────┬──────────┘                        │                                 │
           │                                   │                                 │
           │ both checked in                   │ no check-in required            │
           │ OR check-in window ends           │                                 │
           ▼                                   ▼                                 │
┌─────────────────────┐             ┌─────────────────────┐                      │
│      PickBan        │◄────────────│      PickBan        │◀─────────────────────┘
│  (map veto active)  │             │  (if veto required) │
└──────────┬──────────┘             └──────────┬──────────┘
           │                                   │
           │ veto complete                     │ no veto required
           ▼                                   ▼
┌─────────────────────┐             ┌─────────────────────┐
│    InProgress       │◄────────────│    InProgress       │
│   (match playing)   │             │                     │
└──────────┬──────────┘             └──────────┬──────────┘
           │                                   │
           │ match time elapsed                │
           │ OR manual trigger                 │
           ▼                                   ▼
┌─────────────────────┐
│  AwaitingResult     │
│ (waiting for score) │
└──────────┬──────────┘
           │
           ├──────────────────────────────────────────────────────────────┐
           │ result confirmed                                             │
           ▼                                                              │
┌─────────────────────┐                                                   │
│     Completed       │◄──────────────────────────────────────┐           │
│   (match finished)  │                                       │           │
└─────────────────────┘                                       │           │
                                                              │           │
                                               dispute resolved│           │ result disputed
                                               (uphold/overturn)          │
                                                              │           ▼
                                                   ┌─────────────────────────────┐
                                                   │        Disputed             │
                                                   │  (under admin review)       │
                                                   └─────────────────────────────┘

┌─────────────────────┐
│      Forfeit        │◄──── from any active state (no-show, withdrawal, DQ)
│   (walkover win)    │
└─────────────────────┘

┌─────────────────────┐
│     Cancelled       │◄──── from Pending, Ready, or Scheduled (admin action)
│  (match voided)     │
└─────────────────────┘
```

### State Definitions

| Status | Description | Entry Conditions |
|--------|-------------|------------------|
| `Pending` | Waiting for participants | Match created in bracket generation |
| `Ready` | Both participants set, can schedule | Winner/loser from previous match set |
| `Scheduled` | Time assigned | Schedule proposal accepted or admin set |
| `CheckingIn` | Pre-match check-in window | Check-in window opens (configured time before start) |
| `PickBan` | Map veto in progress | Check-in complete or scheduled time reached |
| `InProgress` | Match being played | Veto complete or match started |
| `AwaitingResult` | Waiting for result submission | Match time elapsed or triggered |
| `Completed` | Match finished normally | Result confirmed |
| `Disputed` | Result under admin review | Result claim disputed |
| `Forfeit` | One team forfeited | No-show, withdrawal, or DQ |
| `Cancelled` | Match voided | Admin cancellation |

---

## Transition Rules

### Transition Matrix

| From | To | Trigger | Conditions | Side Effects |
|------|----|---------|-----------:|--------------|
| Pending | Ready | System | Both participants set | None |
| Ready | Scheduled | Proposal accepted | Valid schedule proposal | Set `scheduled_at` |
| Ready | Scheduled | Admin action | Admin permission | Set `scheduled_at` |
| Scheduled | CheckingIn | Time trigger | Check-in window opens | Send notifications |
| Scheduled | PickBan | Time trigger | Scheduled time reached, no check-in | Create veto session |
| Scheduled | InProgress | Time trigger | No veto required | Set `started_at` |
| Scheduled | Forfeit | No-show | Check-in deadline passed | Advance opponent |
| CheckingIn | PickBan | Both checked in | Both participants confirmed | Create veto session |
| CheckingIn | Forfeit | Timeout | Check-in deadline, one/both missing | Advance opponent |
| PickBan | InProgress | Veto complete | All maps selected | Set `started_at` |
| InProgress | AwaitingResult | Time/Manual | Match time elapsed | None |
| AwaitingResult | Completed | Result confirmed | Both teams agree or timeout | Set `completed_at`, trigger progression |
| AwaitingResult | Disputed | Dispute raised | One team disputes | Create dispute |
| Disputed | Completed | Dispute resolved | Admin resolution | Apply resolution, trigger progression |
| * | Forfeit | Various | No-show, withdrawal, DQ | Set winner, trigger progression |
| Pending/Ready/Scheduled | Cancelled | Admin action | Admin permission | Clear references |

### Transition Validation

```rust
impl MatchStatus {
    /// Check if transition to target status is valid.
    pub fn can_transition_to(&self, target: MatchStatus) -> bool {
        matches!(
            (self, target),
            // Normal flow
            (Self::Pending, Self::Ready)
            | (Self::Ready, Self::Scheduled)
            | (Self::Scheduled, Self::CheckingIn)
            | (Self::Scheduled, Self::PickBan)
            | (Self::Scheduled, Self::InProgress)
            | (Self::CheckingIn, Self::PickBan)
            | (Self::CheckingIn, Self::InProgress)
            | (Self::PickBan, Self::InProgress)
            | (Self::InProgress, Self::AwaitingResult)
            | (Self::AwaitingResult, Self::Completed)
            | (Self::AwaitingResult, Self::Disputed)
            | (Self::Disputed, Self::Completed)

            // Forfeit from any active state
            | (Self::Scheduled, Self::Forfeit)
            | (Self::CheckingIn, Self::Forfeit)
            | (Self::PickBan, Self::Forfeit)
            | (Self::InProgress, Self::Forfeit)
            | (Self::AwaitingResult, Self::Forfeit)

            // Cancel from early states
            | (Self::Pending, Self::Cancelled)
            | (Self::Ready, Self::Cancelled)
            | (Self::Scheduled, Self::Cancelled)
        )
    }

    /// Get allowed transitions from this state.
    pub fn allowed_transitions(&self) -> Vec<MatchStatus> {
        match self {
            Self::Pending => vec![Self::Ready, Self::Cancelled],
            Self::Ready => vec![Self::Scheduled, Self::Cancelled],
            Self::Scheduled => vec![
                Self::CheckingIn,
                Self::PickBan,
                Self::InProgress,
                Self::Forfeit,
                Self::Cancelled,
            ],
            Self::CheckingIn => vec![Self::PickBan, Self::InProgress, Self::Forfeit],
            Self::PickBan => vec![Self::InProgress, Self::Forfeit],
            Self::InProgress => vec![Self::AwaitingResult, Self::Forfeit],
            Self::AwaitingResult => vec![Self::Completed, Self::Disputed, Self::Forfeit],
            Self::Disputed => vec![Self::Completed],
            Self::Completed | Self::Forfeit | Self::Cancelled => vec![],
        }
    }

    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Forfeit | Self::Cancelled)
    }

    /// Check if match is actively in progress.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::CheckingIn | Self::PickBan | Self::InProgress | Self::AwaitingResult
        )
    }
}
```

---

## Database Schema Modifications

### tournament_matches (additions)

```sql
-- Add new columns to tournament_matches
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    check_in_opens_at TIMESTAMPTZ;

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    check_in_deadline TIMESTAMPTZ;

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    participant1_checked_in_at TIMESTAMPTZ;

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    participant2_checked_in_at TIMESTAMPTZ;

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    participant1_checked_in_by UUID REFERENCES users(id);

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    participant2_checked_in_by UUID REFERENCES users(id);

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    veto_required BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS
    check_in_required BOOLEAN NOT NULL DEFAULT false;

-- Index for finding matches needing status updates
CREATE INDEX idx_tournament_matches_check_in_opens
    ON tournament_matches(check_in_opens_at)
    WHERE status = 'scheduled' AND check_in_opens_at IS NOT NULL;

CREATE INDEX idx_tournament_matches_check_in_deadline
    ON tournament_matches(check_in_deadline)
    WHERE status = 'checking_in' AND check_in_deadline IS NOT NULL;
```

### match_status_log (new table)

```sql
CREATE TABLE match_status_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Transition details
    from_status VARCHAR(32) NOT NULL,
    to_status VARCHAR(32) NOT NULL,
    transition_reason VARCHAR(64),

    -- Who triggered
    triggered_by_user_id UUID REFERENCES users(id),
    triggered_by_system BOOLEAN NOT NULL DEFAULT false,

    -- Additional context
    metadata JSONB NOT NULL DEFAULT '{}',

    -- Timestamp
    transitioned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_match_status_log_match ON match_status_log(match_id);
CREATE INDEX idx_match_status_log_time ON match_status_log(transitioned_at);

COMMENT ON TABLE match_status_log IS 'Audit log of match status transitions';
```

---

## Domain Entities

### MatchStatus Enum (update)

```rust
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Pending,
    Ready,
    Scheduled,
    CheckingIn,
    PickBan,
    InProgress,
    AwaitingResult,
    Completed,
    Disputed,
    Forfeit,
    Cancelled,
}

impl fmt::Display for MatchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Scheduled => "scheduled",
            Self::CheckingIn => "checking_in",
            Self::PickBan => "pick_ban",
            Self::InProgress => "in_progress",
            Self::AwaitingResult => "awaiting_result",
            Self::Completed => "completed",
            Self::Disputed => "disputed",
            Self::Forfeit => "forfeit",
            Self::Cancelled => "cancelled",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for MatchStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "scheduled" => Ok(Self::Scheduled),
            "checking_in" => Ok(Self::CheckingIn),
            "pick_ban" => Ok(Self::PickBan),
            "in_progress" => Ok(Self::InProgress),
            "awaiting_result" => Ok(Self::AwaitingResult),
            "completed" => Ok(Self::Completed),
            "disputed" => Ok(Self::Disputed),
            "forfeit" => Ok(Self::Forfeit),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Unknown match status: {}", s)),
        }
    }
}
```

### MatchStatusLog

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{MatchStatusLogId, TournamentMatchId, UserId};

/// Log entry for a match status transition.
#[derive(Debug, Clone)]
pub struct MatchStatusLog {
    pub id: MatchStatusLogId,
    pub match_id: TournamentMatchId,
    pub from_status: MatchStatus,
    pub to_status: MatchStatus,
    pub transition_reason: Option<String>,
    pub triggered_by_user_id: Option<UserId>,
    pub triggered_by_system: bool,
    pub metadata: serde_json::Value,
    pub transitioned_at: DateTime<Utc>,
}
```

### TournamentMatch (extended)

```rust
/// A match in a tournament bracket (extended with check-in fields).
#[derive(Debug, Clone)]
pub struct TournamentMatch {
    // ... existing fields ...

    /// When check-in window opens
    pub check_in_opens_at: Option<DateTime<Utc>>,

    /// Check-in deadline
    pub check_in_deadline: Option<DateTime<Utc>>,

    /// Participant 1 check-in info
    pub participant1_checked_in_at: Option<DateTime<Utc>>,
    pub participant1_checked_in_by: Option<UserId>,

    /// Participant 2 check-in info
    pub participant2_checked_in_at: Option<DateTime<Utc>>,
    pub participant2_checked_in_by: Option<UserId>,

    /// Whether map veto is required
    pub veto_required: bool,

    /// Whether pre-match check-in is required
    pub check_in_required: bool,
}

impl TournamentMatch {
    /// Check if both participants have checked in.
    pub fn both_checked_in(&self) -> bool {
        self.participant1_checked_in_at.is_some() && self.participant2_checked_in_at.is_some()
    }

    /// Check if check-in window is open.
    pub fn is_check_in_open(&self, now: DateTime<Utc>) -> bool {
        if let (Some(opens), Some(deadline)) = (self.check_in_opens_at, self.check_in_deadline) {
            now >= opens && now < deadline
        } else {
            false
        }
    }

    /// Check if check-in deadline has passed.
    pub fn is_check_in_expired(&self, now: DateTime<Utc>) -> bool {
        self.check_in_deadline.is_some_and(|d| now >= d)
    }
}
```

---

## Service Design

### MatchLifecycleService

```rust
pub struct MatchLifecycleService<TMR, MSLR, VSR, TRR>
where
    TMR: TournamentMatchRepository,
    MSLR: MatchStatusLogRepository,
    VSR: VetoSessionRepository,
    TRR: TournamentRegistrationRepository,
{
    match_repo: Arc<TMR>,
    log_repo: Arc<MSLR>,
    veto_repo: Arc<VSR>,
    registration_repo: Arc<TRR>,
}

impl<TMR, MSLR, VSR, TRR> MatchLifecycleService<TMR, MSLR, VSR, TRR>
where
    TMR: TournamentMatchRepository,
    MSLR: MatchStatusLogRepository,
    VSR: VetoSessionRepository,
    TRR: TournamentRegistrationRepository,
{
    // === Status Transitions ===

    /// Transition match to a new status.
    ///
    /// Validates the transition and applies side effects.
    pub async fn transition(
        &self,
        match_id: TournamentMatchId,
        to_status: MatchStatus,
        triggered_by: TransitionTrigger,
        reason: Option<String>,
    ) -> Result<TournamentMatch, DomainError>;

    /// Internal transition logic with side effects.
    async fn apply_transition(
        &self,
        match_: &mut TournamentMatch,
        to_status: MatchStatus,
        triggered_by: &TransitionTrigger,
        reason: Option<String>,
    ) -> Result<(), DomainError>;

    // === Specific Transitions ===

    /// Mark match as ready when both participants are set.
    pub async fn mark_ready(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Schedule the match.
    pub async fn schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        scheduled_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Open check-in window.
    pub async fn open_check_in(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Record participant check-in.
    pub async fn check_in(
        &self,
        match_id: TournamentMatchId,
        participant_registration_id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Start map veto phase.
    pub async fn start_pick_ban(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Start the match (veto complete or no veto).
    pub async fn start_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Move to awaiting result.
    pub async fn await_result(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Complete match with result.
    pub async fn complete(
        &self,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        participant1_score: i32,
        participant2_score: i32,
    ) -> Result<TournamentMatch, DomainError>;

    /// Mark match as disputed.
    pub async fn mark_disputed(
        &self,
        match_id: TournamentMatchId,
        disputed_by: UserId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    /// Process forfeit.
    pub async fn forfeit(
        &self,
        match_id: TournamentMatchId,
        forfeiting_registration_id: TournamentRegistrationId,
        forfeit_type: ForfeitType,
        triggered_by: TransitionTrigger,
    ) -> Result<TournamentMatch, DomainError>;

    /// Cancel match.
    pub async fn cancel(
        &self,
        match_id: TournamentMatchId,
        cancelled_by: UserId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    // === Background Jobs ===

    /// Find matches that need check-in window opened.
    pub async fn find_pending_check_ins(&self) -> Result<Vec<TournamentMatch>, DomainError>;

    /// Find matches with expired check-in deadlines.
    pub async fn find_expired_check_ins(&self) -> Result<Vec<TournamentMatch>, DomainError>;

    /// Find matches that should start (scheduled time reached).
    pub async fn find_ready_to_start(&self) -> Result<Vec<TournamentMatch>, DomainError>;

    // === Queries ===

    /// Get match status history.
    pub async fn get_status_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchStatusLog>, DomainError>;
}

#[derive(Debug, Clone)]
pub enum TransitionTrigger {
    User(UserId),
    System { job_name: String },
    Admin { user_id: UserId, override_reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForfeitType {
    NoShow,
    Withdrawal,
    Disqualification,
    TechnicalDefault,
}
```

---

## Transition Side Effects

### Pending → Ready

- No side effects (automatic when participants set)

### Ready → Scheduled

- Set `scheduled_at`
- Calculate `check_in_opens_at` (e.g., 15 min before)
- Calculate `check_in_deadline` (e.g., 5 min before)
- Send scheduling notification to both teams

### Scheduled → CheckingIn

- Send check-in reminder notifications
- Start check-in timeout job

### CheckingIn → PickBan

- Create veto session
- Send veto start notification

### CheckingIn → Forfeit

- Determine which team(s) didn't check in
- Set winner as opponent (or void if both no-show)
- Trigger progression saga

### PickBan → InProgress

- Set `started_at`
- Create match games based on veto results
- Send match start notification

### InProgress → AwaitingResult

- Send result submission reminder

### AwaitingResult → Completed

- Set `completed_at`
- Set `winner_registration_id` and `loser_registration_id`
- Update scores
- **Trigger progression saga**

### AwaitingResult → Disputed

- Create dispute record
- Pause bracket progression for this branch
- Notify admins

### Disputed → Completed

- Apply dispute resolution
- **Trigger progression saga** (possibly with corrected winner)

### Any → Forfeit

- Set winner as opponent
- Set forfeit reason
- **Trigger progression saga**

---

## Background Jobs

### Match Lifecycle Job

```rust
/// Background job for match lifecycle transitions.
pub struct MatchLifecycleJob {
    lifecycle_service: Arc<MatchLifecycleService>,
    check_interval: Duration,
}

impl MatchLifecycleJob {
    pub async fn run(&self) {
        loop {
            // 1. Open check-in windows
            let pending_check_ins = self.lifecycle_service.find_pending_check_ins().await?;
            for match_ in pending_check_ins {
                self.lifecycle_service.open_check_in(match_.id).await?;
            }

            // 2. Process expired check-ins (no-shows)
            let expired = self.lifecycle_service.find_expired_check_ins().await?;
            for match_ in expired {
                self.process_check_in_expiry(&match_).await?;
            }

            // 3. Start matches at scheduled time
            let ready_to_start = self.lifecycle_service.find_ready_to_start().await?;
            for match_ in ready_to_start {
                self.start_scheduled_match(&match_).await?;
            }

            tokio::time::sleep(self.check_interval).await;
        }
    }

    async fn process_check_in_expiry(&self, match_: &TournamentMatch) -> Result<(), DomainError> {
        let p1_checked = match_.participant1_checked_in_at.is_some();
        let p2_checked = match_.participant2_checked_in_at.is_some();

        match (p1_checked, p2_checked) {
            (true, true) => {
                // Both checked in - should not happen, but proceed
                self.lifecycle_service.start_pick_ban(match_.id).await?;
            }
            (true, false) => {
                // P1 checked in, P2 no-show
                self.lifecycle_service.forfeit(
                    match_.id,
                    match_.participant2_registration_id.unwrap(),
                    ForfeitType::NoShow,
                    TransitionTrigger::System { job_name: "check_in_expiry".to_string() },
                ).await?;
            }
            (false, true) => {
                // P2 checked in, P1 no-show
                self.lifecycle_service.forfeit(
                    match_.id,
                    match_.participant1_registration_id.unwrap(),
                    ForfeitType::NoShow,
                    TransitionTrigger::System { job_name: "check_in_expiry".to_string() },
                ).await?;
            }
            (false, false) => {
                // Both no-show - double forfeit
                // Winner determined by seeding or both DQ'd
                self.handle_double_no_show(match_).await?;
            }
        }

        Ok(())
    }

    async fn start_scheduled_match(&self, match_: &TournamentMatch) -> Result<(), DomainError> {
        if match_.veto_required {
            self.lifecycle_service.start_pick_ban(match_.id).await?;
        } else {
            self.lifecycle_service.start_match(match_.id).await?;
        }
        Ok(())
    }
}
```

---

## API Endpoints

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/status

Get current match status with details.

**Response**:
```json
{
  "data": {
    "match_id": "...",
    "status": "checking_in",
    "status_details": {
      "scheduled_at": "2025-01-15T19:00:00Z",
      "check_in_opens_at": "2025-01-15T18:45:00Z",
      "check_in_deadline": "2025-01-15T18:55:00Z",
      "participant1_checked_in": true,
      "participant2_checked_in": false
    },
    "allowed_actions": ["check_in"],
    "next_state": {
      "expected_status": "pick_ban",
      "expected_at": "2025-01-15T18:55:00Z"
    }
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/check-in

Check in for a match.

**Response** (200 OK):
```json
{
  "data": {
    "match_id": "...",
    "checked_in_at": "2025-01-15T18:50:00Z",
    "status": "checking_in",
    "both_checked_in": false
  }
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/status-history

Get match status transition history.

**Response**:
```json
{
  "data": [
    {
      "from_status": "pending",
      "to_status": "ready",
      "reason": "Both participants set",
      "transitioned_at": "2025-01-14T10:00:00Z",
      "triggered_by": "system"
    },
    {
      "from_status": "ready",
      "to_status": "scheduled",
      "reason": "Schedule proposal accepted",
      "transitioned_at": "2025-01-14T12:00:00Z",
      "triggered_by": {"user": "..."}
    }
  ]
}
```

### POST /v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition

Admin force status transition.

**Request**:
```json
{
  "to_status": "in_progress",
  "reason": "Technical issues resolved, starting match manually"
}
```

---

## Error Handling

### New Error Types

```rust
pub enum MatchLifecycleError {
    /// Invalid state transition
    InvalidTransition {
        from: MatchStatus,
        to: MatchStatus,
    },

    /// Match not in expected state
    UnexpectedState {
        match_id: TournamentMatchId,
        expected: MatchStatus,
        actual: MatchStatus,
    },

    /// Check-in window not open
    CheckInNotOpen(TournamentMatchId),

    /// Check-in already completed
    AlreadyCheckedIn(TournamentMatchId),

    /// User is not a participant in match
    NotParticipant {
        user_id: UserId,
        match_id: TournamentMatchId,
    },

    /// Cannot modify completed match
    MatchFinalized(TournamentMatchId),
}
```

---

## Testing Notes

### Unit Tests

- All valid transitions work
- Invalid transitions rejected
- Side effect timestamps set correctly
- Check-in window calculations

### Integration Tests

```
test_match_lifecycle_pending_to_ready
test_match_lifecycle_schedule
test_match_lifecycle_check_in_single
test_match_lifecycle_check_in_both
test_match_lifecycle_check_in_expiry_no_show
test_match_lifecycle_double_no_show
test_match_lifecycle_skip_check_in
test_match_lifecycle_to_pick_ban
test_match_lifecycle_to_in_progress
test_match_lifecycle_complete
test_match_lifecycle_forfeit_from_any_state
test_match_lifecycle_cancel
test_match_lifecycle_status_log_created
test_admin_force_transition
```

### Edge Case Tests

```
test_concurrent_check_in_attempts
test_check_in_at_exact_deadline
test_transition_during_maintenance
test_status_history_ordering
```
