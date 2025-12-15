# Phase 3: Match System - Overview

> **Status**: Design Phase
> **Dependencies**: Phase 1 (Core Foundation), Phase 2 (Registration & Seeding)
> **Related Documents**: [tournament-system-design.md](../tournament-system-design.md)

---

## Executive Summary

Phase 3 implements the **Match System** - the most complex phase of the tournament system. This encompasses all functionality required to run competitive matches from scheduling through result confirmation and bracket progression.

### Key Capabilities

1. **Match Scheduling** - Self-scheduled matches with team agreement, admin overrides, availability tracking
2. **Pick-Ban System** - Game-specific map veto with turn-based actions and timeout handling
3. **Match Lifecycle** - Complete state machine from pending through completion
4. **Result Submission** - Claim/confirm workflow with auto-confirmation timeout
5. **Evidence System** - Demo files, screenshots, and plugin-integrated evidence
6. **Bracket Progression** - Automatic winner advancement and standings updates
7. **Disputes & Forfeits** - Dispute resolution workflow and forfeit handling

### Design Principles

- **Plugin-First**: Map veto formats, evidence types, and validation rules come from game plugins
- **Saga-Aware**: Multi-step operations use saga patterns for consistency
- **Real-Time Ready**: State machines designed for WebSocket notification integration
- **Audit Complete**: All state transitions logged for dispute resolution

---

## Subsystem Map

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              MATCH SYSTEM (Phase 3)                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────────────┐   │
│  │   SCHEDULING    │────▶│  MATCH LIFECYCLE │────▶│    RESULT SUBMISSION    │   │
│  │    SYSTEM       │     │   STATE MACHINE  │     │       WORKFLOW          │   │
│  │                 │     │                  │     │                         │   │
│  │ - Proposals     │     │ - State checks   │     │ - Result claims         │   │
│  │ - Availability  │     │ - Transitions    │     │ - Confirmations         │   │
│  │ - Admin assign  │     │ - Pre-match      │     │ - Auto-timeout          │   │
│  └────────┬────────┘     └────────┬─────────┘     └───────────┬─────────────┘   │
│           │                       │                           │                  │
│           │              ┌────────▼─────────┐                 │                  │
│           │              │   PICK-BAN       │                 │                  │
│           └─────────────▶│    SYSTEM        │◀────────────────┘                  │
│                          │                  │                                    │
│                          │ - Map veto flow  │                                    │
│                          │ - Turn actions   │                                    │
│                          │ - Plugin maps    │                                    │
│                          └────────┬─────────┘                                    │
│                                   │                                              │
│  ┌─────────────────┐     ┌────────▼─────────┐     ┌─────────────────────────┐   │
│  │    EVIDENCE     │◀────│    DISPUTES &    │────▶│   BRACKET PROGRESSION   │   │
│  │     SYSTEM      │     │    FORFEITS      │     │                         │   │
│  │                 │     │                  │     │ - Winner advancement    │   │
│  │ - Demo files    │     │ - Dispute flow   │     │ - Loser routing         │   │
│  │ - Screenshots   │     │ - Forfeit types  │     │ - Standings update      │   │
│  │ - Plugin links  │     │ - Admin resolve  │     │ - Tournament complete   │   │
│  └─────────────────┘     └──────────────────┘     └─────────────────────────┘   │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Data Flow

```
Registration Complete (Phase 2)
         │
         ▼
┌─────────────────┐
│ Bracket Generated│
│ Matches: Pending │
└────────┬────────┘
         │
         ▼
┌─────────────────┐    ┌─────────────────┐
│ Both Participants│───▶│  Match: Ready    │
│     Set         │    │  (can schedule)  │
└─────────────────┘    └────────┬────────┘
                                │
              ┌─────────────────┴─────────────────┐
              ▼                                   ▼
     ┌─────────────────┐                ┌─────────────────┐
     │ Self-Scheduled  │                │ Admin-Scheduled │
     │ (Proposals)     │                │ (Direct assign) │
     └────────┬────────┘                └────────┬────────┘
              │                                   │
              └─────────────────┬─────────────────┘
                                ▼
                      ┌─────────────────┐
                      │ Match: Scheduled │
                      └────────┬────────┘
                               │
                      ┌────────▼────────┐
                      │ Check-In Window │
                      │ (Optional)      │
                      └────────┬────────┘
                               │
                      ┌────────▼────────┐
                      │ Match: PickBan  │
                      │ (If required)   │
                      └────────┬────────┘
                               │
                      ┌────────▼────────┐
                      │ Match: InProgress│
                      └────────┬────────┘
                               │
                      ┌────────▼─────────┐
                      │ Match: Awaiting  │
                      │     Result       │
                      └────────┬─────────┘
                               │
              ┌────────────────┴────────────────┐
              ▼                                 ▼
     ┌─────────────────┐              ┌─────────────────┐
     │ Result Confirmed │              │ Result Disputed │
     └────────┬────────┘              └────────┬────────┘
              │                                 │
              │                        ┌────────▼────────┐
              │                        │ Admin Resolution│
              │                        └────────┬────────┘
              │                                 │
              └─────────────────┬───────────────┘
                                ▼
                      ┌─────────────────┐
                      │ Match: Completed │
                      └────────┬────────┘
                               │
                      ┌────────▼─────────────┐
                      │ PROGRESSION SAGA     │
                      │ - Advance winner     │
                      │ - Route loser        │
                      │ - Update standings   │
                      │ - Check completion   │
                      └──────────────────────┘
```

---

## Implementation Plan

### Sub-Phase Summary

| Sub-Phase | Name | Complexity | Dependencies | Saga | Key Deliverables |
|-----------|------|------------|--------------|------|------------------|
| 3.1 | Match Lifecycle Core | M | None | No | Match state machine, transitions |
| 3.2 | Match Scheduling | M | 3.1 | No | Proposal workflow, admin scheduling |
| 3.3 | Availability System | S | 3.2 | No | Availability windows, suggestions |
| 3.4 | Pick-Ban Core | L | 3.1 | No | Veto state machine, turn actions |
| 3.5 | Pick-Ban Plugin Integration | M | 3.4 | No | Game maps, veto formats |
| 3.6 | Result Submission | M | 3.1 | No | Claims, confirmations, timeout |
| 3.7 | Evidence System | M | 3.6 | No | Evidence types, storage |
| 3.8 | Plugin Evidence Integration | M | 3.7 | No | CS2 demos, S3 integration |
| 3.9 | Bracket Progression | L | 3.6 | **Yes** | Winner advancement, standings |
| 3.10 | Forfeit Handling | M | 3.9 | **Yes** | Forfeit types, cascade effects |
| 3.11 | Dispute System | M | 3.9 | **Yes** | Dispute workflow, resolution |

**Complexity**: S = Small, M = Medium, L = Large

### Dependency Graph

```
                    ┌──────────────┐
                    │ 3.1 Match    │
                    │ Lifecycle    │
                    │ Core         │
                    └──────┬───────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│3.2 Scheduling│   │3.4 Pick-Ban  │   │3.6 Result    │
│              │   │    Core      │   │  Submission  │
└──────┬───────┘   └──────┬───────┘   └──────┬───────┘
       │                  │                  │
       ▼                  ▼                  ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│3.3 Availability│ │3.5 Plugin    │   │3.7 Evidence  │
│    System    │   │  Integration │   │    System    │
└──────────────┘   └──────────────┘   └──────┬───────┘
                                             │
                                             ▼
                                      ┌──────────────┐
                                      │3.8 Plugin    │
                                      │  Evidence    │
                                      └──────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│3.9 Bracket   │◀──│3.10 Forfeit  │──▶│3.11 Dispute  │
│ Progression  │   │  Handling    │   │    System    │
│  (SAGA)      │   │   (SAGA)     │   │   (SAGA)     │
└──────────────┘   └──────────────┘   └──────────────┘
```

### Critical Path

```
3.1 → 3.6 → 3.9 (Bracket Progression Saga)
```

This path is critical because:
1. Match lifecycle (3.1) is the foundation
2. Result submission (3.6) triggers progression
3. Bracket progression (3.9) is the core value delivery

### Parallel Development Opportunities

**Track A** (Scheduling):
```
3.1 → 3.2 → 3.3
```

**Track B** (Pick-Ban):
```
3.1 → 3.4 → 3.5
```

**Track C** (Results & Evidence):
```
3.1 → 3.6 → 3.7 → 3.8
```

**Track D** (Progression - requires Track C):
```
3.6 → 3.9 → 3.10/3.11
```

---

## Sub-Phase Definitions

### 3.1 Match Lifecycle Core

**Scope**:
- Match status enum with all states
- State transition validation
- Transition side effects (timestamps, notifications)
- MatchLifecycleService with transition methods

**Deliverables**:
- `portal-core/src/types/match_status.rs` - Status enum with transition rules
- `portal-domain/src/services/tournament/match_lifecycle.rs` - Lifecycle service
- `portal-domain/src/repositories/tournament_match.rs` - Extended with status updates
- Unit tests for state machine transitions

**Acceptance Criteria**:
- All valid transitions work correctly
- Invalid transitions return appropriate errors
- Timestamps (started_at, completed_at) set on relevant transitions
- Audit events emitted for each transition

---

### 3.2 Match Scheduling

**Scope**:
- Schedule proposal entity and workflow
- Proposal states: Proposed → Accepted/Rejected/Expired/CounterProposed
- Admin direct scheduling capability
- Schedule deadline enforcement

**Deliverables**:
- Migration: `schedule_proposals` table
- `ScheduleProposal` entity
- `SchedulingService` with proposal methods
- API endpoints for propose/accept/reject/counter
- Unit and integration tests

**Acceptance Criteria**:
- Teams can propose match times
- Opponents can accept, reject, or counter-propose
- Deadlines are enforced with expiration
- Admins can directly set schedules

---

### 3.3 Availability System

**Scope**:
- Weekly availability windows for players/teams
- Recurring patterns with exception dates
- Availability-based time suggestions
- Substitute availability tracking

**Deliverables**:
- Migration: `availability_windows`, `availability_exceptions` tables
- `AvailabilityWindow`, `AvailabilityException` entities
- `AvailabilityService` with CRUD and suggestion methods
- API endpoints for availability management

**Acceptance Criteria**:
- Players can set weekly recurring availability
- Exception dates override recurring patterns
- System suggests optimal match times based on overlap
- Substitutes can have separate availability

---

### 3.4 Pick-Ban Core

**Scope**:
- Veto session state machine
- Turn-based action validation
- Timeout handling with auto-action
- Side selection after map pick

**Deliverables**:
- `VetoSession` entity with state machine
- `VetoAction` entity for action log
- `VetoService` with action methods
- Timeout job scheduling interface

**Acceptance Criteria**:
- Veto sessions track all actions
- Only correct team can act on their turn
- Timeout triggers random selection
- Session completes when all maps determined

---

### 3.5 Pick-Ban Plugin Integration

**Scope**:
- Plugin method for available maps
- Plugin method for veto formats
- Map metadata (thumbnails, names)
- Tournament-specific map pool configuration

**Deliverables**:
- Extended `GamePlugin` trait with veto methods
- CS2 plugin implementation of veto methods
- `TournamentMapPool` service integration
- API endpoints for map pool configuration

**Acceptance Criteria**:
- Plugins provide map pools with metadata
- Plugins define veto format sequences
- Tournaments can customize map pools
- Veto uses plugin-provided formats

---

### 3.6 Result Submission

**Scope**:
- Result claim entity and workflow
- Claim states: Submitted → Confirmed/Disputed/TimedOut
- Auto-confirmation after timeout
- Game-by-game result submission for series

**Deliverables**:
- Migration: `result_claims` table
- `ResultClaim` entity
- `ResultService` with submit/confirm/dispute methods
- API endpoints for result workflow

**Acceptance Criteria**:
- Either team can submit result claim
- Opponent can confirm or dispute
- Auto-confirmation after configurable timeout
- Series results tracked per-game

---

### 3.7 Evidence System

**Scope**:
- Evidence types (demo, screenshot, link)
- Evidence storage abstraction
- Evidence linking to matches/games
- Access control for evidence viewing

**Deliverables**:
- Migration: `match_evidence` table
- `Evidence` entity
- `EvidenceService` with upload/link methods
- Storage integration (local + S3)

**Acceptance Criteria**:
- Multiple evidence types supported
- Evidence links to specific matches or games
- Presigned URLs for secure access
- Evidence can be required per tournament

---

### 3.8 Plugin Evidence Integration

**Scope**:
- Plugin method for available evidence
- Plugin evidence validation
- CS2 demo file integration
- S3 bucket scanning for demos

**Deliverables**:
- Extended `TournamentPlugin` trait
- CS2 plugin evidence methods
- S3 scanning service
- Demo selection API

**Acceptance Criteria**:
- Plugins can provide evidence from external sources
- CS2 demos discovered from S3 bucket
- Demo metadata extracted (map, duration, etc.)
- Players can link discovered demos to matches

---

### 3.9 Bracket Progression (SAGA)

**Scope**:
- Winner advancement to next match
- Loser routing (double elimination)
- Standings calculation (round robin/swiss)
- Tournament completion detection

**Saga Steps**:
1. Validate result finalized
2. Update match with winner/loser
3. Advance winner to next match
4. Route loser (if double elim)
5. Update standings (if applicable)
6. Check bracket completion
7. Check tournament completion

**Compensating Actions**:
- Revert match result
- Clear winner from next match
- Recalculate standings

**Deliverables**:
- `ProgressionSaga` coordinator
- `ProgressionService` methods
- `StandingsService` for round robin/swiss
- Saga state persistence

**Acceptance Criteria**:
- Winners automatically advance
- Losers route correctly in double elimination
- Standings update for round robin
- Tournament completion detected
- Partial failures recoverable

---

### 3.10 Forfeit Handling (SAGA)

**Scope**:
- Forfeit types: no-show, withdrawal, disqualification
- Forfeit processing with opponent advancement
- Cascade effects in double elimination

**Saga Steps**:
1. Mark match as forfeit
2. Set winner (opponent)
3. Trigger progression saga
4. Handle cascading forfeits (if applicable)

**Deliverables**:
- `ForfeitService` methods
- Integration with progression saga
- Cascade detection for double elim
- Admin forfeit override

**Acceptance Criteria**:
- No-show auto-detected after check-in window
- Opponent advances on forfeit
- Double elim cascades handled
- Stats reflect walkover wins

---

### 3.11 Dispute System (SAGA)

**Scope**:
- Dispute raising with evidence
- Admin review queue
- Resolution options: uphold, overturn, rematch, DQ
- Result correction with bracket updates

**Saga Steps** (for overturn):
1. Mark dispute resolved
2. Revert original result
3. Apply corrected result
4. Re-run progression saga with correct winner
5. Update downstream matches if needed

**Deliverables**:
- Migration: `disputes` table
- `Dispute` entity
- `DisputeService` with resolution methods
- Admin API for dispute queue

**Acceptance Criteria**:
- Disputes pause bracket progression
- Evidence attachable to disputes
- Resolution triggers appropriate updates
- Overturns cascade correctly

---

## Cross-Cutting Concerns

### Authorization Model

| Action | Permission Required | Context |
|--------|---------------------|---------|
| Propose schedule | Match participant | Own matches only |
| Submit result | Match participant | Own matches only |
| Confirm result | Match opponent | Own matches only |
| Raise dispute | Match participant | Own matches only |
| Resolve dispute | `tournament.disputes.resolve` | Any tournament |
| Admin schedule | `tournament.brackets.manage` | Assigned tournaments |
| Forfeit match | `tournament.brackets.manage` | Assigned tournaments |
| Override result | `tournament.brackets.manage` | Assigned tournaments |

### Audit Logging Requirements

All match state transitions must be logged to `entity_changes`:
- `entity_type`: `tournament_match`
- `change_type`: State transition name
- `changed_by`: Actor user ID
- `old_data`: Previous state
- `new_data`: New state
- `metadata`: Transition context (reason, evidence IDs, etc.)

### Notification Triggers

| Event | Recipients | Priority |
|-------|------------|----------|
| Match scheduled | Both participants | Normal |
| Check-in opens | Both participants | High |
| Veto turn | Active team | High |
| Result submitted | Opponent | High |
| Result confirmed | Both participants | Normal |
| Dispute raised | Both participants + admins | High |
| Dispute resolved | Both participants | High |
| Match forfeited | Both participants | High |

---

## Database Schema Summary

### New Tables

| Table | Purpose | Document |
|-------|---------|----------|
| `schedule_proposals` | Match scheduling proposals | 01-scheduling-system.md |
| `availability_windows` | Player/team availability | 01-scheduling-system.md |
| `availability_exceptions` | Availability overrides | 01-scheduling-system.md |
| `veto_sessions` | Map veto session state | 02-pick-ban-system.md |
| `result_claims` | Result submission claims | 04-result-submission.md |
| `match_evidence` | Evidence attachments | 05-evidence-system.md |
| `disputes` | Match disputes | 07-disputes-forfeits.md |
| `saga_states` | Saga execution state | 08-sagas-orchestration.md |

### Modified Tables

| Table | Changes | Document |
|-------|---------|----------|
| `tournament_matches` | Add check-in fields, scheduling metadata | 03-match-lifecycle.md |
| `tournament_match_games` | Add evidence links | 05-evidence-system.md |

---

## New Service Summary

| Service | Responsibility | Document |
|---------|---------------|----------|
| `MatchLifecycleService` | State transitions | 03-match-lifecycle.md |
| `SchedulingService` | Schedule proposals | 01-scheduling-system.md |
| `AvailabilityService` | Availability management | 01-scheduling-system.md |
| `VetoService` | Map veto sessions | 02-pick-ban-system.md |
| `ResultService` | Result submission | 04-result-submission.md |
| `EvidenceService` | Evidence management | 05-evidence-system.md |
| `ProgressionService` | Bracket advancement | 06-bracket-progression.md |
| `StandingsService` | Round robin/swiss standings | 06-bracket-progression.md |
| `ForfeitService` | Forfeit handling | 07-disputes-forfeits.md |
| `DisputeService` | Dispute workflow | 07-disputes-forfeits.md |
| `SagaCoordinator` | Saga orchestration | 08-sagas-orchestration.md |

---

## API Summary

See individual documents for full specifications. Key endpoint groups:

- `POST /matches/{id}/schedule/*` - Scheduling operations
- `GET /players/{id}/availability` - Availability management
- `POST /matches/{id}/veto/*` - Pick-ban operations
- `POST /matches/{id}/result/*` - Result submission
- `POST /matches/{id}/evidence/*` - Evidence management
- `GET /brackets/{id}/matches` - Bracket queries
- `GET /tournaments/{id}/standings` - Standings queries
- `POST /admin/disputes/*` - Dispute resolution

---

## Success Criteria

Phase 3 is complete when:

1. **Scheduling**: Teams can successfully negotiate match times
2. **Pick-Ban**: Map veto completes with all game-specific formats
3. **Results**: Results can be submitted, confirmed, and disputed
4. **Evidence**: Demo files linkable from external storage
5. **Progression**: Winners advance automatically through brackets
6. **Forfeits**: No-shows and withdrawals handled correctly
7. **Disputes**: Disputed results can be resolved with bracket correction
8. **Sagas**: Multi-step operations are atomic and recoverable
9. **Tests**: All integration tests pass (target: 50+ new tests)
10. **Docs**: OpenAPI documentation complete for all endpoints
