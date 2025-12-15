# Phase 3 Implementation - Batch 1: Match Lifecycle & Scheduling

## Context

You are implementing **Phase 3 Batch 1** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This batch covers the foundational match lifecycle system and scheduling capabilities.

**Design Documents** (READ THESE FIRST):
- `docs/phase3/00-overview.md` - Overall architecture and implementation plan
- `docs/phase3/01-scheduling-system.md` - Scheduling design
- `docs/phase3/03-match-lifecycle.md` - Match state machine design

**Completed Prior Work**:
- Phase 1: Core Foundation (tournaments, stages, brackets, registrations)
- Phase 2: Registration & Seeding (registration workflows, seeding algorithms)
- Phase 3 Design: All design documents in `docs/phase3/`

**Reference Files**:
- `docs/tournament-system-design.md` - Existing architecture
- `docs/tournament-implementation-progress.md` - Progress tracking
- `crates/portal-domain/src/services/tournament/mod.rs` - Existing tournament services
- `migrations/0030_create_tournaments.sql` - Existing schema

---

## Your Task

Implement **Sub-Phases 3.1, 3.2, and 3.3** following the design documents exactly. This batch establishes the match lifecycle state machine and scheduling system.

### Sub-Phases in This Batch

| Sub-Phase | Name | Description |
|-----------|------|-------------|
| 3.1 | Match Lifecycle Core | Match status enum, state transitions, lifecycle service |
| 3.2 | Match Scheduling | Schedule proposals, acceptance workflow |
| 3.3 | Availability System | Weekly availability windows, time suggestions |

### Implementation Order

```
3.1 Match Lifecycle Core
         │
         ▼
3.2 Match Scheduling
         │
         ▼
3.3 Availability System
```

**Complete each sub-phase fully before moving to the next.**

---

## Sub-Phase 3.1: Match Lifecycle Core

### Scope

Implement the complete match state machine as defined in `docs/phase3/03-match-lifecycle.md`.

### Deliverables

#### 1. Migration: Match Lifecycle Fields

Create migration `migrations/0031_match_lifecycle.sql`:

```sql
-- Add check-in and lifecycle fields to tournament_matches
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_opens_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_deadline TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant1_checked_in_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant2_checked_in_at TIMESTAMPTZ;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant1_checked_in_by UUID REFERENCES users(id);
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS participant2_checked_in_by UUID REFERENCES users(id);
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS veto_required BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE tournament_matches ADD COLUMN IF NOT EXISTS check_in_required BOOLEAN NOT NULL DEFAULT false;

-- Match status log table
CREATE TABLE match_status_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    from_status VARCHAR(32) NOT NULL,
    to_status VARCHAR(32) NOT NULL,
    transition_reason VARCHAR(64),
    triggered_by_user_id UUID REFERENCES users(id),
    triggered_by_system BOOLEAN NOT NULL DEFAULT false,
    metadata JSONB NOT NULL DEFAULT '{}',
    transitioned_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_match_status_log_match ON match_status_log(match_id);
```

#### 2. Domain: MatchStatus Enum Enhancement

Update `crates/portal-core/src/types/tournament.rs`:

- Add `can_transition_to(target: MatchStatus) -> bool` method
- Add `allowed_transitions() -> Vec<MatchStatus>` method
- Add `is_terminal() -> bool` method
- Add `is_active() -> bool` method

Refer to `docs/phase3/03-match-lifecycle.md` for the complete transition rules.

#### 3. Domain Entity: MatchStatusLog

Create `crates/portal-domain/src/entities/match_status_log.rs`:

```rust
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

#### 4. Repository: MatchStatusLogRepository

Create trait in `crates/portal-domain/src/repositories/match_status_log.rs`:

```rust
#[async_trait]
pub trait MatchStatusLogRepository: Send + Sync + 'static {
    async fn create(&self, log: &MatchStatusLog) -> Result<MatchStatusLog, DomainError>;
    async fn find_by_match_id(&self, match_id: TournamentMatchId) -> Result<Vec<MatchStatusLog>, DomainError>;
}
```

Implement in `crates/portal-db/src/adapters/tournament/match_status_log.rs`.

#### 5. Service: MatchLifecycleService

Create `crates/portal-domain/src/services/tournament/match_lifecycle.rs`:

```rust
pub struct MatchLifecycleService<TMR, MSLR>
where
    TMR: TournamentMatchRepository,
    MSLR: MatchStatusLogRepository,
{
    match_repo: Arc<TMR>,
    log_repo: Arc<MSLR>,
}

impl<TMR, MSLR> MatchLifecycleService<TMR, MSLR> {
    /// Transition match to new status with validation
    pub async fn transition(
        &self,
        match_id: TournamentMatchId,
        to_status: MatchStatus,
        triggered_by: TransitionTrigger,
        reason: Option<String>,
    ) -> Result<TournamentMatch, DomainError>;

    /// Mark match as ready when both participants set
    pub async fn mark_ready(&self, match_id: TournamentMatchId) -> Result<TournamentMatch, DomainError>;

    /// Schedule the match
    pub async fn schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        scheduled_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Record participant check-in
    pub async fn check_in(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Get status history
    pub async fn get_status_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchStatusLog>, DomainError>;
}

pub enum TransitionTrigger {
    User(UserId),
    System { job_name: String },
    Admin { user_id: UserId, override_reason: String },
}
```

#### 6. API Handlers

Add to `crates/portal-api/src/handlers/tournaments.rs` (or create `matches.rs`):

- `GET /v1/tournaments/{id}/matches/{match_id}/status` - Get match status with details
- `POST /v1/tournaments/{id}/matches/{match_id}/check-in` - Check in for match
- `GET /v1/tournaments/{id}/matches/{match_id}/status-history` - Get transition history
- `POST /v1/admin/tournaments/{id}/matches/{match_id}/transition` - Admin force transition

#### 7. Tests

Create `crates/portal-api/tests/match_lifecycle_test.rs`:

```rust
#[tokio::test]
async fn test_match_status_pending_to_ready() { ... }

#[tokio::test]
async fn test_match_status_ready_to_scheduled() { ... }

#[tokio::test]
async fn test_match_check_in() { ... }

#[tokio::test]
async fn test_invalid_transition_rejected() { ... }

#[tokio::test]
async fn test_status_history_recorded() { ... }

#[tokio::test]
async fn test_admin_force_transition() { ... }
```

### Acceptance Criteria (3.1)

- [x] All valid state transitions work correctly
- [x] Invalid transitions return appropriate errors
- [x] Status log records all transitions with actor
- [x] Check-in updates participant check-in timestamps
- [x] Admin can force transitions with reason
- [x] All tests pass
- [x] OpenAPI docs updated

---

## Sub-Phase 3.2: Match Scheduling

### Scope

Implement the schedule proposal system as defined in `docs/phase3/01-scheduling-system.md`.

### Deliverables

#### 1. Migration: Schedule Proposals

Create migration `migrations/0032_schedule_proposals.sql`:

```sql
CREATE TABLE schedule_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    proposed_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    proposed_by_user_id UUID NOT NULL REFERENCES users(id),
    proposed_times TIMESTAMPTZ[] NOT NULL,
    selected_time TIMESTAMPTZ,
    responded_at TIMESTAMPTZ,
    responded_by_user_id UUID REFERENCES users(id),
    counter_proposal_id UUID REFERENCES schedule_proposals(id),
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT schedule_proposals_check_status CHECK (status IN (
        'pending', 'accepted', 'rejected', 'counter_proposed', 'expired', 'cancelled'
    )),
    CONSTRAINT schedule_proposals_check_times CHECK (
        array_length(proposed_times, 1) >= 1 AND
        array_length(proposed_times, 1) <= 5
    )
);

CREATE INDEX idx_schedule_proposals_match ON schedule_proposals(match_id);
CREATE INDEX idx_schedule_proposals_status ON schedule_proposals(status);
CREATE INDEX idx_schedule_proposals_expires ON schedule_proposals(expires_at) WHERE status = 'pending';

CREATE TRIGGER schedule_proposals_updated_at
    BEFORE UPDATE ON schedule_proposals
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

#### 2. Domain Entities

Create `crates/portal-domain/src/entities/schedule_proposal.rs`:

```rust
pub struct ScheduleProposal {
    pub id: ScheduleProposalId,
    pub match_id: TournamentMatchId,
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub proposed_by_user_id: UserId,
    pub proposed_times: Vec<DateTime<Utc>>,
    pub selected_time: Option<DateTime<Utc>>,
    pub responded_at: Option<DateTime<Utc>>,
    pub responded_by_user_id: Option<UserId>,
    pub counter_proposal_id: Option<ScheduleProposalId>,
    pub status: ProposalStatus,
    pub expires_at: DateTime<Utc>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
    CounterProposed,
    Expired,
    Cancelled,
}
```

#### 3. Repository

Create trait and implementation for `ScheduleProposalRepository`.

#### 4. Service: SchedulingService

Create `crates/portal-domain/src/services/tournament/scheduling.rs`:

```rust
pub struct SchedulingService<SPR, TMR, TRR, MLS>
where
    SPR: ScheduleProposalRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    MLS: MatchLifecycleService,
{
    // ...
}

impl SchedulingService {
    pub async fn propose_schedule(
        &self,
        match_id: TournamentMatchId,
        proposed_times: Vec<DateTime<Utc>>,
        proposed_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    pub async fn accept_proposal(
        &self,
        proposal_id: ScheduleProposalId,
        selected_time: DateTime<Utc>,
        accepted_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    pub async fn reject_proposal(
        &self,
        proposal_id: ScheduleProposalId,
        rejected_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    pub async fn counter_propose(
        &self,
        original_proposal_id: ScheduleProposalId,
        new_times: Vec<DateTime<Utc>>,
        proposed_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    pub async fn admin_schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        admin_id: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn expire_proposals(&self) -> Result<Vec<ScheduleProposal>, DomainError>;
}
```

#### 5. API Handlers

- `POST /v1/tournaments/{id}/matches/{match_id}/schedule/propose`
- `POST /v1/tournaments/{id}/matches/{match_id}/schedule/accept`
- `POST /v1/tournaments/{id}/matches/{match_id}/schedule/reject`
- `POST /v1/tournaments/{id}/matches/{match_id}/schedule/counter`
- `GET /v1/tournaments/{id}/matches/{match_id}/schedule/proposals`
- `POST /v1/admin/tournaments/{id}/matches/{match_id}/schedule`

#### 6. Tests

```rust
#[tokio::test]
async fn test_propose_schedule() { ... }

#[tokio::test]
async fn test_accept_proposal() { ... }

#[tokio::test]
async fn test_reject_proposal() { ... }

#[tokio::test]
async fn test_counter_propose() { ... }

#[tokio::test]
async fn test_proposal_expiry() { ... }

#[tokio::test]
async fn test_admin_schedule() { ... }

#[tokio::test]
async fn test_non_participant_cannot_propose() { ... }
```

### Acceptance Criteria (3.2)

- [x] Teams can propose match times (1-5 options)
- [x] Opponent can accept, reject, or counter-propose
- [x] Accepting a proposal schedules the match
- [x] Proposals expire after deadline
- [x] Admins can directly schedule matches
- [x] All tests pass
- [x] OpenAPI docs updated

---

## Sub-Phase 3.3: Availability System

### Scope

Implement availability windows as defined in `docs/phase3/01-scheduling-system.md`.

### Deliverables

#### 1. Migration: Availability Tables

Create migration `migrations/0033_availability.sql`:

```sql
CREATE TABLE availability_windows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    day_of_week SMALLINT NOT NULL,
    start_minutes SMALLINT NOT NULL,
    end_minutes SMALLINT NOT NULL,
    timezone VARCHAR(64) NOT NULL DEFAULT 'UTC',
    effective_from DATE,
    effective_until DATE,
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,
    is_substitute BOOLEAN NOT NULL DEFAULT false,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT availability_windows_owner_check CHECK (
        (player_id IS NOT NULL)::int + (team_season_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT availability_windows_day_check CHECK (day_of_week BETWEEN 0 AND 6),
    CONSTRAINT availability_windows_time_check CHECK (
        start_minutes >= 0 AND start_minutes < 1440 AND
        end_minutes > 0 AND end_minutes <= 1440 AND
        start_minutes < end_minutes
    )
);

CREATE TABLE availability_exceptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    exception_date DATE NOT NULL,
    exception_type VARCHAR(16) NOT NULL DEFAULT 'blocked',
    start_minutes SMALLINT,
    end_minutes SMALLINT,
    reason VARCHAR(256),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT availability_exceptions_owner_check CHECK (
        (player_id IS NOT NULL)::int + (team_season_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT availability_exceptions_type_check CHECK (
        exception_type IN ('blocked', 'override')
    )
);

CREATE INDEX idx_availability_windows_player ON availability_windows(player_id) WHERE player_id IS NOT NULL;
CREATE INDEX idx_availability_windows_team ON availability_windows(team_season_id) WHERE team_season_id IS NOT NULL;
CREATE INDEX idx_availability_exceptions_player ON availability_exceptions(player_id, exception_date) WHERE player_id IS NOT NULL;
```

#### 2. Domain Entities

Create entities for `AvailabilityWindow` and `AvailabilityException`.

#### 3. Service: AvailabilityService

Create `crates/portal-domain/src/services/tournament/availability.rs`:

```rust
pub struct AvailabilityService<AWR, AER> {
    // ...
}

impl AvailabilityService {
    pub async fn set_player_availability(
        &self,
        player_id: PlayerId,
        windows: Vec<CreateAvailabilityWindow>,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    pub async fn get_player_availability(
        &self,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    pub async fn add_exception(
        &self,
        owner: AvailabilityOwner,
        exception_date: NaiveDate,
        exception_type: ExceptionType,
        reason: Option<String>,
    ) -> Result<AvailabilityException, DomainError>;

    pub async fn suggest_match_times(
        &self,
        match_id: TournamentMatchId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max_suggestions: usize,
    ) -> Result<Vec<SuggestedTime>, DomainError>;
}
```

#### 4. API Handlers

- `GET /v1/players/{player_id}/availability`
- `PUT /v1/players/me/availability`
- `POST /v1/players/me/availability/exceptions`
- `DELETE /v1/players/me/availability/exceptions/{id}`
- `GET /v1/tournaments/{id}/matches/{match_id}/schedule/suggested-times`

#### 5. Tests

```rust
#[tokio::test]
async fn test_set_player_availability() { ... }

#[tokio::test]
async fn test_availability_exception_blocks_window() { ... }

#[tokio::test]
async fn test_suggest_times_with_overlap() { ... }

#[tokio::test]
async fn test_timezone_conversion() { ... }
```

### Acceptance Criteria (3.3)

- [x] Players can set weekly recurring availability
- [x] Exception dates override recurring patterns
- [x] System suggests optimal match times based on overlap
- [x] Timezone handling works correctly
- [x] All tests pass
- [x] OpenAPI docs updated

---

## Implementation Guidelines

### Code Patterns

Follow existing codebase patterns:

1. **Repository Pattern**: Traits in `portal-domain/src/repositories/`, implementations in `portal-db/src/adapters/`
2. **Service Pattern**: Generic services with Arc dependencies
3. **Three-Layer Types**: DB entity → Domain entity → API DTO
4. **Error Handling**: Domain errors convert to API errors
5. **OpenAPI**: Every endpoint documented with `#[utoipa::path]`

### File Organization

```
crates/
├── portal-core/src/
│   ├── ids.rs                    # Add ScheduleProposalId, AvailabilityWindowId, etc.
│   └── types/tournament.rs       # Enhance MatchStatus
├── portal-domain/src/
│   ├── entities/
│   │   ├── schedule_proposal.rs  # NEW
│   │   ├── availability.rs       # NEW
│   │   └── match_status_log.rs   # NEW
│   ├── repositories/
│   │   ├── schedule_proposal.rs  # NEW
│   │   ├── availability.rs       # NEW
│   │   └── match_status_log.rs   # NEW
│   └── services/tournament/
│       ├── mod.rs                # Update exports
│       ├── match_lifecycle.rs    # NEW
│       ├── scheduling.rs         # NEW
│       └── availability.rs       # NEW
├── portal-db/src/
│   ├── entities/
│   │   └── tournament.rs         # Add new DB entities
│   └── adapters/tournament/
│       ├── schedule_proposal.rs  # NEW
│       ├── availability.rs       # NEW
│       └── match_status_log.rs   # NEW
└── portal-api/src/
    ├── dto/
    │   ├── requests/tournament.rs  # Add request DTOs
    │   └── responses/tournament.rs # Add response DTOs
    ├── handlers/
    │   └── tournaments.rs          # Add handlers (or create matches.rs)
    ├── routes/tournaments.rs       # Add routes
    ├── openapi.rs                  # Register new paths/schemas
    └── state.rs                    # Add new services to AppState
```

### Testing Strategy

1. Run `cargo check` after each major change
2. Write tests as you implement each component
3. Run `cargo test -p portal-api` for integration tests
4. Verify OpenAPI at `/swagger-ui` after adding handlers

### Commit Strategy

Make logical commits after completing each sub-component:
- "Add migration for match lifecycle fields"
- "Add MatchStatus transition methods"
- "Add MatchLifecycleService"
- "Add match lifecycle API endpoints"
- etc.

---

## Verification Checklist

Before considering this batch complete:

### Sub-Phase 3.1
- [x] Migration applied successfully
- [x] MatchStatus has transition validation methods
- [x] MatchLifecycleService handles all transitions
- [x] Status log records all transitions
- [x] API endpoints work (test with curl/Swagger)
- [x] Integration tests pass

### Sub-Phase 3.2
- [x] Schedule proposals table created
- [x] Proposal workflow (propose → accept/reject/counter) works
- [x] Match transitions to Scheduled on accept
- [x] Admin override scheduling works
- [x] Proposal expiry logic works
- [x] Integration tests pass

### Sub-Phase 3.3
- [x] Availability tables created
- [x] Player availability CRUD works
- [x] Exception handling works
- [x] Time suggestion algorithm works
- [x] Integration tests pass

### Overall
- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace` passes
- [x] OpenAPI docs complete
- [x] No security vulnerabilities introduced

---

## Output

After completing this batch:

1. Update `docs/tournament-implementation-progress.md` with completed items
2. List any design document deviations with rationale
3. Note any blockers or issues discovered
4. Confirm all acceptance criteria are met

**Do not proceed to Sub-Phase 3.4 (Pick-Ban) until this batch is complete and verified.**

---

## Status: ✅ COMPLETE

**Completed**: 2025-12-01

### Implementation Summary

**Files Created/Modified:**
- `migrations/0031_match_lifecycle.sql` - Check-in fields, status log table
- `migrations/0032_schedule_proposals.sql` - Schedule proposal tables
- `migrations/0033_availability.sql` - Availability windows, overrides, suggestions
- `crates/portal-domain/src/entities/schedule_proposal.rs` - Schedule proposal entity
- `crates/portal-domain/src/entities/availability.rs` - Availability entities
- `crates/portal-domain/src/entities/match_lifecycle.rs` - Match lifecycle types
- `crates/portal-domain/src/services/tournament/match_lifecycle.rs` - Match state machine
- `crates/portal-domain/src/services/tournament/scheduling.rs` - Scheduling workflow
- `crates/portal-domain/src/services/tournament/availability.rs` - Availability service
- `crates/portal-db/src/adapters/tournament/match_status_log.rs` - Status log adapter
- `crates/portal-db/src/adapters/tournament/schedule_proposal.rs` - Proposal adapter
- `crates/portal-api/src/handlers/availability.rs` - Availability handlers
- `crates/portal-api/src/routes/availability.rs` - Availability routes
- `crates/portal-api/src/dto/requests/availability.rs` - Request DTOs
- `crates/portal-api/src/dto/responses/availability.rs` - Response DTOs

**Test Results:**
- All endpoint routing tests pass
- Validation tests pass
- Integration tests pass
