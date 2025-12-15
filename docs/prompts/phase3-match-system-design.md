# Phase 3: Match System - Design & Planning

## Context

You are designing the **Match System** (Phase 3) for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This is one of the most complex phases, encompassing:

- **Match scheduling** with team agreement workflows
- **Availability systems** for teams and substitutes
- **Pick-ban (map veto) lobbies**
- **Result submission** with evidence support
- **Evidence systems** integrated with game plugins (e.g., CS2 demo files from S3)
- **Bracket progression** and forfeit handling
- **Dispute resolution** workflows

**Key Documents**:
- **Design Document**: `docs/tournament-system-design.md` - Review existing architecture
- **Progress Tracking**: `docs/tournament-implementation-progress.md` - Phases 1-2 are complete
- **This Prompt**: Guides the creation of detailed design documents

**Completed Prior Work**:
- Phase 1: Core Foundation (tournaments, stages, brackets, registrations, basic bracket generation)
- Phase 2: Registration & Seeding (registration workflows, seeding algorithms, check-in status)

---

## Your Task

**DO NOT write implementation code yet.** Your task is to create comprehensive design documents that extend the existing tournament system design with advanced match features. This requires careful analysis and planning.

### Deliverables

Create **multiple focused design documents** (easier to read and generate than one massive document):

1. **`docs/phase3/00-overview.md`** - Executive summary, subsystem relationships, implementation plan
2. **`docs/phase3/01-scheduling-system.md`** - Match scheduling, availability, team agreement
3. **`docs/phase3/02-pick-ban-system.md`** - Map veto lobbies, turn-based actions, plugin integration
4. **`docs/phase3/03-match-lifecycle.md`** - Match states, transitions, pre-match lobby
5. **`docs/phase3/04-result-submission.md`** - Result claims, confirmation, evidence linking
6. **`docs/phase3/05-evidence-system.md`** - Evidence types, storage, CS2 demo integration
7. **`docs/phase3/06-bracket-progression.md`** - Winner advancement, standings, tournament completion
8. **`docs/phase3/07-disputes-forfeits.md`** - Dispute workflows, forfeit handling, admin resolution
9. **`docs/phase3/08-sagas-orchestration.md`** - Saga patterns for multi-step operations

Create the `docs/phase3/` directory for these documents.

---

## Phase 3 Subsystems to Design

### 3.1 Match Scheduling System

Design a system where matches can be scheduled in multiple ways:

**Self-Scheduled Matches**:
- Both teams must agree on a match time
- Proposer submits available time slots
- Opponent accepts one slot OR proposes alternatives
- Deadline enforcement (if no agreement, admin intervenes or forfeit rules apply)
- Timezone handling (store UTC, display local)

**Availability System**:
- Players/teams can define weekly availability windows
- Recurring patterns (e.g., "Saturdays 2-6pm")
- Exception dates (holidays, travel)
- Used to suggest optimal match times
- Substitute availability for roster flexibility

**Admin-Scheduled Matches**:
- Tournament admin sets fixed times
- Override capabilities for emergencies

**Design Considerations**:
- How do scheduling deadlines interact with bracket progression?
- What happens if teams can't agree (forfeit rules, admin escalation)?
- How does availability factor into auto-suggestions?
- How do substitutes declare availability differently from starters?

### 3.2 Pick-Ban (Map Veto) System

Design a comprehensive map selection system:

**Veto Formats**:
- Bo1: Ban-Ban-Ban-Ban-Ban-Pick (alternate)
- Bo3: Ban-Ban-Pick-Pick-Ban-Ban-Pick
- Bo5: Ban-Ban-Pick-Pick-Pick-Pick-Pick
- Custom formats per tournament/game

**Veto Flow**:
- Coin flip or seeding determines first action
- Turn-based actions with time limits
- Auto-action on timeout (random selection)
- Side selection after map pick

**Technical Considerations**:
- Real-time updates (WebSocket or polling?)
- State machine for veto progress
- Map pool configuration per tournament/stage
- Game-specific map metadata from plugins

**Plugin Integration**:
- `GamePlugin::get_available_maps()` - Returns map pool with metadata
- `GamePlugin::get_veto_formats()` - Returns supported veto formats
- Map thumbnails, names, and game-specific properties

### 3.3 Match Lifecycle & State Machine

Design the complete match lifecycle:

**States**:
```
Pending → Ready → Scheduled → CheckingIn → PickBan → InProgress →
  → AwaitingResult → Completed
  → Disputed → Resolved
  → Forfeit
  → Cancelled
```

**Transitions**:
- Define all valid state transitions
- Required conditions for each transition
- Side effects (notifications, bracket updates)
- Timeout handlers (no-show → forfeit)

**Pre-Match Lobby**:
- Match check-in window (5-15 min before start)
- Ready confirmation from both teams
- Server connection details (from plugin or admin)

### 3.4 Result Submission System

Design how match results are submitted and confirmed:

**Submission Flow**:
- Either team submits result claim
- Opponent confirms OR disputes
- Auto-confirmation timeout
- Admin override capability

**Evidence System**:
- Evidence types: screenshots, demo files, external links
- Evidence sources: manual upload, game plugin (S3 integration)
- Required vs optional evidence per tournament
- Evidence retention policies

**CS2 Demo Integration** (reference implementation):
- Demos are uploaded to S3 bucket by external service
- Players select from available demos in bucket
- Demo metadata: match ID, map, timestamp, size
- Validation: demo matches claimed result?

**Design Considerations**:
- How does evidence link to specific games in a series?
- Can evidence be added after result confirmation?
- Privacy: who can view evidence?
- Storage: S3 paths, presigned URLs for access

### 3.5 Game Plugin Integration for Evidence

Extend the `GamePlugin` trait:

```rust
pub trait TournamentPlugin: Send + Sync {
    // Existing methods...

    /// Get available evidence for a match
    async fn get_available_evidence(
        &self,
        match_context: &MatchContext,
    ) -> Result<Vec<EvidenceItem>, PluginError>;

    /// Validate submitted evidence
    async fn validate_evidence(
        &self,
        evidence: &EvidenceSubmission,
    ) -> Result<EvidenceValidation, PluginError>;

    /// Get match server connection details
    async fn get_server_info(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ServerInfo>, PluginError>;
}

pub struct MatchContext {
    pub tournament_id: TournamentId,
    pub match_id: TournamentMatchId,
    pub participants: Vec<ParticipantInfo>,
    pub scheduled_at: Option<DateTime<Utc>>,
}

pub struct EvidenceItem {
    pub id: String,
    pub evidence_type: EvidenceType,
    pub source: EvidenceSource,
    pub name: String,
    pub url: String,  // Could be S3 presigned URL
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub enum EvidenceType {
    DemoFile,
    Screenshot,
    ExternalLink,
    GameServerLog,
}

pub enum EvidenceSource {
    ManualUpload,
    GameServer,
    ExternalApi,
}
```

### 3.6 Bracket Progression

Design how match results affect the bracket:

**Winner Advancement**:
- Update next match with winner
- Handle byes (auto-advance)
- Grand final reset in double elimination

**Loser Handling**:
- Single elim: elimination
- Double elim: move to losers bracket
- Round robin: update standings

**Automatic Updates**:
- Next match becomes "Ready" when both participants set
- Standings recalculation for round robin/swiss
- Tournament completion detection

### 3.7 Forfeit & Dispute Handling

**Forfeit Scenarios**:
- No-show (failed to check in)
- Withdrawal during match
- Disqualification (rule violation)

**Forfeit Effects**:
- Match marked as forfeit
- Opponent advances
- Stats handling (walkover win)

**Dispute System**:
- Either team can raise dispute before confirmation
- Dispute reasons (wrong score, cheating allegation, etc.)
- Evidence attachment
- Admin review queue
- Resolution options: uphold, overturn, rematch, DQ

### 3.8 Saga Patterns & Orchestration

**IMPORTANT**: Identify which operations require saga patterns for consistency.

A **saga** is needed when an operation involves multiple steps that must either all succeed or be compensated (rolled back). Consider sagas for:

**Likely Saga Candidates**:

1. **Match Completion Saga**
   - Submit result → Validate → Update match → Advance winner → Update standings → Notify
   - If bracket update fails, what happens to the result?
   - Compensation: revert match status, clear winner advancement

2. **Forfeit Processing Saga**
   - Mark forfeit → Advance opponent → Update standings → Handle cascading matches
   - Multiple matches may need updates in double elimination

3. **Tournament Start Saga**
   - Close registration → Process no-shows → Generate brackets → Create matches → Notify
   - Partial failure scenarios?

4. **Dispute Resolution Saga**
   - Resolve dispute → Potentially revert results → Re-advance correct winner → Update standings
   - May cascade through multiple bracket rounds

5. **Pick-Ban Completion Saga**
   - Complete veto → Create match games → Update match status → Notify server
   - External system integration (game servers)

**Saga Design Questions**:
- What are the compensating actions for each step?
- Can operations be made idempotent for retry safety?
- Should we use choreography (events) or orchestration (coordinator)?
- What's the timeout/retry strategy?
- How do we handle partial failures?

**Not Sagas** (single transaction suffices):
- Simple status updates
- Read operations
- Single-entity mutations

**Document in `08-sagas-orchestration.md`**:
- Identify each saga with steps
- Define compensating actions
- Specify idempotency requirements
- Choose orchestration approach
- Define failure modes and recovery

---

## Design Document Structure

Each document in `docs/phase3/` should be **focused and self-contained** (~300-600 lines each). Here's what each should include:

---

### `00-overview.md` - Overview & Implementation Plan

```markdown
# Phase 3: Match System - Overview

## Executive Summary
- Phase 3 scope and goals
- Key design decisions
- Integration with Phases 1-2

## Subsystem Map
- Diagram showing how subsystems relate
- Data flow between components
- External integrations (plugins, S3, etc.)

## Implementation Plan
- Sub-phase breakdown (see below)
- Dependencies between sub-phases
- Critical path
- Estimated complexity per sub-phase

## Cross-Cutting Concerns
- Authorization model
- Audit logging requirements
- Notification triggers
```

---

### `01-scheduling-system.md` - Scheduling & Availability

```markdown
# Scheduling System Design

## Overview
- Self-scheduled vs admin-scheduled matches
- Team agreement workflow

## Database Schema
- Tables: schedule_proposals, availability_windows, availability_exceptions
- Relationships and constraints

## Domain Entities
- ScheduleProposal, AvailabilityWindow, AvailabilityException

## State Machine
- Proposal states: Proposed → Accepted/Rejected/Expired/CounterProposed

## Service Design
- SchedulingService methods
- AvailabilityService methods

## API Endpoints
- POST /matches/{id}/schedule/propose
- POST /matches/{id}/schedule/accept
- GET/PUT /players/{id}/availability
- GET /matches/{id}/suggested-times

## Edge Cases
- Timezone handling
- Deadline enforcement
- Admin override
```

---

### `02-pick-ban-system.md` - Map Veto System

```markdown
# Pick-Ban (Map Veto) System Design

## Overview
- Veto formats (Bo1, Bo3, Bo5, custom)
- Turn-based action flow

## Database Schema
- Tables: veto_sessions, veto_actions, map_pools
- Constraints for turn order

## Domain Entities
- VetoSession, VetoAction, MapPool

## State Machine
- Session states: Pending → CoinFlip → InProgress → Completed
- Action validation rules

## Plugin Integration
- GamePlugin::get_available_maps()
- GamePlugin::get_veto_formats()
- Map metadata structure

## Service Design
- VetoService methods
- Timeout handling

## API Endpoints
- POST /matches/{id}/veto/start
- POST /matches/{id}/veto/action
- GET /matches/{id}/veto/status

## Real-Time Considerations
- Polling vs WebSocket for veto updates
- Timeout auto-actions
```

---

### `03-match-lifecycle.md` - Match States & Transitions

```markdown
# Match Lifecycle Design

## Overview
- Complete match state machine
- Pre-match lobby flow

## State Machine Diagram
- All states with transitions
- Transition conditions
- Side effects per transition

## Database Schema
- Modifications to tournament_matches
- New tables if needed

## Domain Entities
- Updated TournamentMatch
- MatchCheckIn entity

## Service Design
- MatchLifecycleService
- Transition methods with validation

## API Endpoints
- POST /matches/{id}/check-in
- POST /matches/{id}/start
- GET /matches/{id}/status

## Timeout Handling
- Check-in window expiry
- No-show detection
```

---

### `04-result-submission.md` - Result Submission

```markdown
# Result Submission Design

## Overview
- Submission and confirmation flow
- Auto-confirmation timeout

## Database Schema
- Tables: result_claims, result_confirmations
- Link to evidence

## Domain Entities
- ResultClaim, ResultConfirmation

## State Machine
- Claim states: Submitted → Confirmed/Disputed/TimedOut

## Service Design
- ResultService methods
- Validation logic

## API Endpoints
- POST /matches/{id}/result
- POST /matches/{id}/result/confirm
- POST /matches/{id}/result/dispute

## Game-by-Game Results
- Series result submission
- Individual game scores
```

---

### `05-evidence-system.md` - Evidence & Demo Integration

```markdown
# Evidence System Design

## Overview
- Evidence types and sources
- CS2 demo integration specifics

## Database Schema
- Tables: match_evidence, evidence_links
- S3 reference storage

## Domain Entities
- Evidence, EvidenceLink

## Plugin Integration
- TournamentPlugin::get_available_evidence()
- TournamentPlugin::validate_evidence()
- S3 presigned URL generation

## CS2 Demo Specifics
- Demo bucket structure
- Metadata extraction
- Demo selection UI flow

## Service Design
- EvidenceService methods

## API Endpoints
- GET /matches/{id}/evidence/available
- POST /matches/{id}/evidence
- GET /matches/{id}/evidence

## Storage Considerations
- Retention policies
- Access control (presigned URLs)
```

---

### `06-bracket-progression.md` - Bracket Updates

```markdown
# Bracket Progression Design

## Overview
- Winner advancement logic
- Standings calculations

## Progression Rules
- Single elimination
- Double elimination (winners/losers)
- Round robin standings
- Swiss pairings

## Database Schema
- tournament_standings table
- Match source references

## Service Design
- ProgressionService
- StandingsService

## API Endpoints
- GET /tournaments/{id}/standings
- GET /brackets/{id}/matches

## Tournament Completion
- Detection logic
- Final standings calculation
```

---

### `07-disputes-forfeits.md` - Disputes & Forfeits

```markdown
# Disputes & Forfeits Design

## Overview
- Dispute workflow
- Forfeit scenarios and effects

## Database Schema
- Tables: disputes, dispute_evidence, forfeit_records

## Domain Entities
- Dispute, DisputeResolution

## State Machine
- Dispute states: Raised → UnderReview → Resolved

## Forfeit Handling
- Automatic (no-show)
- Manual (withdrawal, DQ)
- Cascade effects

## Service Design
- DisputeService
- ForfeitService

## API Endpoints
- POST /matches/{id}/dispute
- GET /admin/disputes
- POST /admin/disputes/{id}/resolve
```

---

### `08-sagas-orchestration.md` - Saga Patterns

```markdown
# Saga Patterns & Orchestration

## Overview
- Why sagas are needed
- Orchestration vs choreography decision

## Identified Sagas

### Match Completion Saga
- Steps: [list each step]
- Compensating actions: [for each step]
- Failure scenarios
- Idempotency strategy

### Forfeit Processing Saga
- Steps and compensation
- Cascade handling

### Tournament Start Saga
- Steps and compensation

### Dispute Resolution Saga
- Steps and compensation
- Historical bracket reconstruction

### Pick-Ban Completion Saga
- Steps and compensation
- External system handling

## Implementation Approach
- Saga coordinator design
- State persistence
- Retry policies
- Monitoring and alerting

## Error Recovery
- Manual intervention points
- Admin tools needed
```

---

## Common Sections for Each Document

Each document should include where relevant:

1. **Database Schema** - Table definitions, constraints, indexes
2. **Domain Entities** - Rust struct definitions
3. **Repository Interfaces** - Trait methods needed
4. **Service Design** - Service responsibilities and key methods
5. **API Endpoints** - Full specifications with request/response
6. **Error Handling** - New error types and scenarios
7. **Testing Notes** - Key test scenarios to cover

---

## Implementation Plan Structure

The implementation plan should be included in `docs/phase3/00-overview.md` under the "Implementation Plan" section.

### Sub-Phase Summary Table

Include a table like:

| Sub-Phase | Name | Complexity | Dependencies | Key Deliverables |
|-----------|------|------------|--------------|------------------|
| 3.1 | Match Core | M | None | Match lifecycle service |
| 3.2 | Scheduling Proposals | L | 3.1 | Schedule negotiation |
| ... | ... | ... | ... | ... |

### Dependency Graph

Create an ASCII or mermaid diagram showing:
- Which sub-phases can run in parallel
- Critical path through the implementation
- External dependencies

### Sub-Phase Definitions

For each sub-phase, document:

```markdown
## Sub-Phase 3.X: [Name]

### Scope
- What's included
- What's explicitly excluded

### Dependencies
- Which sub-phases must be complete first
- External dependencies

### Deliverables
- Files to create/modify
- Database migrations
- Tests to write

### Acceptance Criteria
- Testable requirements
- Integration points to verify

### Complexity
- Estimated effort (S/M/L/XL)
- Risk factors
- Saga involvement (yes/no)
```

### Suggested Sub-Phases

Consider breaking Phase 3 into these sub-phases:

1. **3.1 Match Core** - Basic match lifecycle, status transitions
2. **3.2 Scheduling Proposals** - Team scheduling workflow
3. **3.3 Availability System** - Weekly availability windows
4. **3.4 Pick-Ban Core** - Map veto state machine, basic flow
5. **3.5 Pick-Ban Plugin Integration** - Game-specific maps, formats
6. **3.6 Result Submission** - Basic result submission & confirmation
7. **3.7 Evidence System** - Evidence types, storage, linking
8. **3.8 Plugin Evidence Integration** - CS2 demo integration
9. **3.9 Bracket Progression** - Winner advancement, standings (SAGA)
10. **3.10 Forfeit Handling** - No-shows, withdrawals (SAGA)
11. **3.11 Dispute System** - Disputes, admin resolution (SAGA)

---

## Research Steps

Before designing, investigate:

1. **Read existing design**: `docs/tournament-system-design.md` thoroughly
2. **Review match entities**: Check what's already defined for `TournamentMatch`, `TournamentMatchGame`
3. **Check plugin system**: `crates/portal-plugins/src/traits.rs` for existing plugin interface
4. **Examine CS2 plugin**: `crates/portal-plugins/src/games/cs2/mod.rs` for game-specific patterns
5. **Review existing services**: `crates/portal-domain/src/services/tournament/` for service patterns
6. **Check existing migrations**: `migrations/0030_create_tournaments.sql` for current schema

---

## Quality Requirements

Your design documents should:

1. **Be complete** - Cover all scenarios, edge cases, error states
2. **Be consistent** - Match existing codebase patterns and naming conventions
3. **Be realistic** - Consider implementation complexity
4. **Be testable** - Every feature should have clear test criteria
5. **Be extensible** - Allow for future game plugins and tournament types

---

## Output

After completing this task, you should have created the `docs/phase3/` directory with these documents:

```
docs/phase3/
├── 00-overview.md           # Executive summary, subsystem map, implementation plan
├── 01-scheduling-system.md  # Scheduling & availability design
├── 02-pick-ban-system.md    # Map veto system design
├── 03-match-lifecycle.md    # Match states & transitions
├── 04-result-submission.md  # Result submission design
├── 05-evidence-system.md    # Evidence & demo integration
├── 06-bracket-progression.md # Bracket updates design
├── 07-disputes-forfeits.md  # Disputes & forfeits design
└── 08-sagas-orchestration.md # Saga patterns design
```

**Document Size Guidelines**:
- Each document should be 300-600 lines
- Focus on one subsystem per document
- Include all schema, entities, services, and API for that subsystem
- Cross-reference other documents where needed

**Quality Checklist**:
- [ ] All saga candidates identified with compensation strategies
- [ ] State machines fully defined with all transitions
- [ ] Database schema includes constraints and indexes
- [ ] API endpoints have full request/response specifications
- [ ] Plugin integration points clearly defined
- [ ] Error scenarios documented
- [ ] Implementation sub-phases have clear dependencies

These documents will guide the actual implementation work. Do NOT proceed to implementation until the design has been reviewed and approved.
