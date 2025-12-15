# Tournament System Implementation

## Context

You are implementing a tournament system for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL).

**Key Documents**:
- **Design Document**: `docs/tournament-system-design.md` - Contains complete architecture, database schema, service definitions, and API specifications
- **Progress Tracking**: `docs/tournament-implementation-progress.md` - Track what's been completed

**Existing Patterns to Follow**:
- League teams: `crates/portal-domain/src/services/league_team/`
- Repository adapters: `crates/portal-db/src/adapters/league_team/`
- Handlers: `crates/portal-api/src/handlers/league_teams/`
- Entity definitions: `crates/portal-domain/src/entities/league_team.rs`
- Test patterns: `crates/portal-api/tests/league_teams_test.rs`

## Implementation Instructions

### Before Starting ANY Phase

1. **Read the design document**: `docs/tournament-system-design.md`
2. **Check progress**: `docs/tournament-implementation-progress.md` to see current status
3. **Update progress**: Mark the phase as "🟡 In Progress" with today's date
4. **Understand patterns**: Read the referenced existing code to match patterns exactly

### After Completing ANY Phase

1. **Run tests**: `cargo test -p portal-domain tournament && cargo test -p portal-api tournaments`
2. **Check compilation**: `cargo check --workspace`
3. **Update progress**: Mark completed items, update status to "🟢 Complete"
4. **Commit changes**: Create descriptive commit with all phase changes

---

## Phase 1: Core Foundation

### Goal
Create the foundational tournament infrastructure: database tables, domain entities, basic services, and single elimination bracket generation.

### Step 1.1: Add Strongly-Typed IDs

**File**: `crates/portal-core/src/ids.rs`

Add these ID types using the existing `define_id!` macro:
```rust
define_id!(TournamentId);
define_id!(TournamentStageId);
define_id!(TournamentBracketId);
define_id!(TournamentRegistrationId);
define_id!(TournamentMatchId);
define_id!(TournamentMatchGameId);
define_id!(TournamentMapPoolId);
```

**Test**: Ensure `cargo check -p portal-core` passes.

### Step 1.2: Add Tournament Types/Enums

**File**: Create `crates/portal-core/src/types/tournament.rs`

Define enums following existing patterns in `types/`:
- `TournamentStatus`: Draft, Published, Registration, CheckIn, Seeding, InProgress, Completed, Cancelled
- `TournamentFormat`: SingleElimination, DoubleElimination, RoundRobin, Swiss, GroupsAndPlayoffs
- `TournamentParticipantType`: Team, Individual, AdHoc
- `RegistrationType`: Open, InviteOnly, Qualification, Approval
- `SchedulingMode`: Live, SelfScheduled, Hybrid
- `MatchStatus`: Pending, Ready, Scheduled, CheckingIn, PickBan, InProgress, AwaitingResult, Completed, Cancelled, Forfeit, Disputed
- `RegistrationStatus`: Pending, Approved, CheckedIn, Active, Eliminated, Disqualified, Withdrawn, NoShow
- `WithdrawalPolicy`: Forfeit, Reseeding, WaitlistPromotion, AdminDecision

Each enum needs:
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`
- `impl Display`
- `impl FromStr`
- Helper methods (e.g., `is_terminal()`, `can_transition_to()`)

**Update**: `crates/portal-core/src/types/mod.rs` to export new module.
**Update**: `crates/portal-core/src/lib.rs` to re-export types.

**Test**: `cargo check -p portal-core`

### Step 1.3: Add Tournament Errors

**File**: `crates/portal-core/src/errors.rs`

Add to `DomainError` enum:
```rust
TournamentNotFound(String),
TournamentStageNotFound(String),
TournamentBracketNotFound(String),
TournamentMatchNotFound(String),
TournamentRegistrationNotFound(String),
TournamentNotOpen,
TournamentRegistrationClosed,
TournamentAlreadyStarted,
TournamentFull,
AlreadyRegisteredForTournament,
NotRegisteredForTournament,
TournamentRegistrationPending,
NotCheckedIn,
MatchNotReady,
MatchAlreadyStarted,
MatchAlreadyCompleted,
InvalidMatchResult,
BracketGenerationFailed(String),
InsufficientParticipants,
```

**Test**: `cargo check -p portal-core`

### Step 1.4: Create Database Migration

**File**: Create `migrations/0030_create_tournaments.sql` (use next available number)

Copy the SQL from the design document's "Database Schema" section. Include:
- `tournaments` table
- `tournament_stages` table
- `tournament_brackets` table
- `tournament_registrations` table
- `tournament_matches` table
- `tournament_match_games` table
- All indexes and constraints
- Permission inserts

**Test**: Start the server and verify migration runs: `cargo run -p portal-app`

### Step 1.5: Create Domain Entities

**File**: Create `crates/portal-domain/src/entities/tournament.rs`

Define entities following the design document and existing patterns in `entities/league_team.rs`:
- `Tournament`
- `TournamentStage`
- `TournamentBracket`
- `TournamentRegistration`
- `TournamentMatch`
- `TournamentMatchGame`

Each entity needs:
- All fields from design document
- Helper methods (e.g., `is_registration_open()`, `can_compete()`)
- Command structs (e.g., `CreateTournamentCommand`, `UpdateTournamentCommand`)

**Update**: `crates/portal-domain/src/entities/mod.rs` to export.

**Test**: `cargo check -p portal-domain`

### Step 1.6: Create Repository Traits

**File**: Create `crates/portal-domain/src/repositories/tournament.rs`

Define repository traits following `repositories/league_team.rs` patterns:
- `TournamentRepository`
- `TournamentStageRepository`
- `TournamentBracketRepository`
- `TournamentMatchRepository`
- `TournamentRegistrationRepository`

Each trait needs:
- `#[cfg_attr(test, mockall::automock)]`
- `#[async_trait]`
- Standard CRUD methods
- Query methods (find_by_*, list_by_*)
- Status update methods

**Update**: `crates/portal-domain/src/repositories/mod.rs` to export.

**Test**: `cargo check -p portal-domain`

### Step 1.7: Create Database Entities (portal-db)

**File**: Create `crates/portal-db/src/entities/tournament.rs`

Define row types for SQLx:
- `TournamentRow`
- `TournamentStageRow`
- `TournamentBracketRow`
- `TournamentMatchRow`
- `TournamentRegistrationRow`
- `TournamentMatchGameRow`

Each needs `#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]`

**Update**: `crates/portal-db/src/entities/mod.rs` to export.

**Test**: `cargo check -p portal-db`

### Step 1.8: Create Repository Adapters

**Directory**: Create `crates/portal-db/src/adapters/tournament/`

Create files following `adapters/league_team/` patterns:
- `mod.rs` - Module exports
- `conversions.rs` - `From<Row> for Entity` implementations
- `tournament.rs` - `PgTournamentRepository`
- `stage.rs` - `PgTournamentStageRepository`
- `bracket.rs` - `PgTournamentBracketRepository`
- `match_.rs` - `PgTournamentMatchRepository` (underscore to avoid keyword)
- `registration.rs` - `PgTournamentRegistrationRepository`

**Update**: `crates/portal-db/src/adapters/mod.rs` to export.
**Update**: `crates/portal-db/src/lib.rs` to re-export adapters.

**Test**: `cargo check -p portal-db`

### Step 1.9: Create Bracket Generator

**File**: Create `crates/portal-domain/src/services/tournament/mod.rs`
**File**: Create `crates/portal-domain/src/services/tournament/bracket_generator.rs`

Implement single elimination bracket generation:
```rust
pub trait BracketGenerator: Send + Sync {
    fn generate_matches(
        &self,
        participants: &[SeededParticipant],
        settings: &BracketSettings,
    ) -> Result<Vec<GeneratedMatch>, BracketError>;

    fn calculate_rounds(&self, participant_count: usize) -> usize;
    fn assign_byes(&self, participant_count: usize) -> Vec<ByeAssignment>;
}

pub struct SingleEliminationGenerator;
impl BracketGenerator for SingleEliminationGenerator { ... }
```

Include proper seeding (1v16, 8v9, etc.) and bye handling.

**Test**: Write unit tests in `crates/portal-domain/src/services/tournament/tests/bracket_generation_tests.rs`:
```rust
#[test]
fn test_4_team_single_elim() { ... }

#[test]
fn test_8_team_single_elim() { ... }

#[test]
fn test_6_team_with_byes() { ... }

#[test]
fn test_seeding_positions() { ... }
```

### Step 1.10: Create TournamentService

**File**: Create `crates/portal-domain/src/services/tournament/service.rs`

Implement the service with generic repository parameters:
```rust
pub struct TournamentService<TR, TSR, TBR, TMR, TRegR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TBR: TournamentBracketRepository,
    TMR: TournamentMatchRepository,
    TRegR: TournamentRegistrationRepository,
{
    tournament_repo: Arc<TR>,
    stage_repo: Arc<TSR>,
    bracket_repo: Arc<TBR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRegR>,
}
```

Implement methods:
- `create_tournament()`
- `get_tournament()`
- `update_tournament()`
- `list_tournaments()`
- `publish()`
- `open_registration()`
- `close_registration()`
- `start_tournament()` - generates brackets

**Update**: `crates/portal-domain/src/services/mod.rs` to export.

**Test**: `cargo check -p portal-domain`

### Step 1.11: Create API DTOs

**File**: Create `crates/portal-api/src/dto/requests/tournament.rs`
**File**: Create `crates/portal-api/src/dto/responses/tournament.rs`

Follow patterns in existing DTOs:
- Request types with `#[derive(Deserialize, Validate, ToSchema)]`
- Response types with `#[derive(Serialize, ToSchema)]`
- `From` implementations for domain ↔ DTO conversions

**Update**: `crates/portal-api/src/dto/requests/mod.rs`
**Update**: `crates/portal-api/src/dto/responses/mod.rs`

**Test**: `cargo check -p portal-api`

### Step 1.12: Create API Handlers

**Directory**: Create `crates/portal-api/src/handlers/tournaments/`

Create files:
- `mod.rs`
- `tournament.rs` - CRUD handlers

Each handler needs:
- `#[utoipa::path(...)]` annotation
- Proper extractors (State, Auth, Path, ValidatedJson)
- Permission checks where needed
- `ApiResult<...>` return type

**Update**: `crates/portal-api/src/handlers/mod.rs` to export.

### Step 1.13: Create Routes

**File**: Create `crates/portal-api/src/routes/tournaments.rs`

Define routes following existing patterns:
```rust
pub fn tournament_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/tournaments", post(create_tournament).get(list_tournaments))
        .route("/v1/tournaments/:id", get(get_tournament).patch(update_tournament).delete(delete_tournament))
        .route("/v1/tournaments/:id/publish", post(publish_tournament))
        // ... more routes
}
```

**Update**: `crates/portal-api/src/routes/mod.rs` to include tournament routes.

### Step 1.14: Update AppState

**File**: `crates/portal-api/src/state.rs`

Add tournament service and repositories:
```rust
pub type AppTournamentService = TournamentService<
    PgTournamentRepository,
    PgTournamentStageRepository,
    PgTournamentBracketRepository,
    PgTournamentMatchRepository,
    PgTournamentRegistrationRepository,
>;

// In AppState struct:
pub tournament_service: AppTournamentService,
```

Initialize in `AppState::new()` or `with_storage()`.

### Step 1.15: Update OpenAPI

**File**: `crates/portal-api/src/openapi.rs`

Add:
- All tournament handlers to `paths(...)`
- All tournament DTOs to `components(schemas(...))`
- "tournaments" tag

### Step 1.16: Write Integration Tests

**File**: Create `crates/portal-api/tests/tournaments_test.rs`

Write tests following `league_teams_test.rs` patterns:
```rust
#[tokio::test]
async fn test_create_tournament() { ... }

#[tokio::test]
async fn test_get_tournament() { ... }

#[tokio::test]
async fn test_list_tournaments() { ... }

#[tokio::test]
async fn test_update_tournament() { ... }

#[tokio::test]
async fn test_tournament_lifecycle() { ... }

#[tokio::test]
async fn test_register_for_tournament() { ... }

#[tokio::test]
async fn test_generate_bracket() { ... }
```

### Step 1.17: Create Test Builders

**File**: Create `crates/portal-test/src/builders/tournament.rs`

```rust
pub struct TournamentBuilder { ... }

impl TournamentBuilder {
    pub fn new() -> Self { ... }
    pub fn name(mut self, name: &str) -> Self { ... }
    pub fn format(mut self, format: TournamentFormat) -> Self { ... }
    pub async fn build_persisted(self, pool: &DbPool) -> Tournament { ... }
}
```

**Update**: `crates/portal-test/src/builders/mod.rs` to export.

### Phase 1 Verification

Run these commands and ensure all pass:
```bash
cargo check --workspace
cargo test -p portal-core
cargo test -p portal-domain tournament
cargo test -p portal-api tournaments
```

Update `docs/tournament-implementation-progress.md`:
- Check off all Phase 1 items
- Mark Phase 1 as "🟢 Complete"
- List all files created

---

## Phase 2: Registration & Seeding

### Prerequisites
- Phase 1 complete
- All Phase 1 tests passing

### Goal
Complete registration flow with check-in, waitlist, and seeding algorithms.

### Step 2.1: Registration Service

**File**: Create `crates/portal-domain/src/services/tournament/registration.rs`

Implement:
- `register_team()` - for team tournaments
- `register_player()` - for individual tournaments
- `withdraw()`
- `approve_registration()` - admin
- `reject_registration()` - admin
- `disqualify()` - admin
- `list_registrations()`
- Eligibility validation

### Step 2.2: Check-in Service

**File**: Create `crates/portal-domain/src/services/tournament/checkin.rs`

Implement:
- `check_in()` - participant checks in
- `admin_check_in()` - admin checks in participant
- `process_no_shows()` - mark unchecked-in as no-show
- `get_check_in_status()`

### Step 2.3: Seeding Service

**File**: Create `crates/portal-domain/src/services/tournament/seeding.rs`

Implement seeding algorithms:
```rust
pub enum SeedingAlgorithm {
    Random,
    Rating,
    SeasonRank,
    Manual,
}

pub struct SeedingService { ... }

impl SeedingService {
    pub async fn auto_seed(&self, tournament_id: TournamentId, algorithm: SeedingAlgorithm) -> Result<Vec<SeededParticipant>, DomainError>;
    pub async fn manual_seed(&self, tournament_id: TournamentId, seeds: Vec<(TournamentRegistrationId, i32)>) -> Result<(), DomainError>;
    pub async fn get_current_seeding(&self, tournament_id: TournamentId) -> Result<Vec<SeededParticipant>, DomainError>;
}
```

### Step 2.4: Registration Handlers

**File**: Create `crates/portal-api/src/handlers/tournaments/registration.rs`

Endpoints:
- `POST /v1/tournaments/{id}/registrations` - Register
- `GET /v1/tournaments/{id}/registrations` - List registrations
- `GET /v1/tournament-registrations/{id}` - Get registration
- `DELETE /v1/tournament-registrations/{id}` - Withdraw
- `POST /v1/tournament-registrations/{id}/approve` - Admin approve
- `POST /v1/tournament-registrations/{id}/reject` - Admin reject
- `POST /v1/tournament-registrations/{id}/check-in` - Check in

### Step 2.5: Seeding Handlers

**File**: Create `crates/portal-api/src/handlers/tournaments/seeding.rs`

Endpoints:
- `GET /v1/tournaments/{id}/seeding` - Get current seeding
- `POST /v1/tournaments/{id}/seeding/auto` - Auto-seed
- `POST /v1/tournaments/{id}/seeding/manual` - Manual seed

### Step 2.6: Write Tests

```rust
// Unit tests
#[test]
fn test_random_seeding() { ... }

#[test]
fn test_rating_based_seeding() { ... }

// Integration tests
#[tokio::test]
async fn test_team_registration() { ... }

#[tokio::test]
async fn test_check_in_flow() { ... }

#[tokio::test]
async fn test_no_show_processing() { ... }

#[tokio::test]
async fn test_registration_approval() { ... }
```

---

## Phase 3: Match System

### Prerequisites
- Phase 2 complete
- All Phase 2 tests passing

### Goal
Complete match lifecycle: scheduling, result submission, bracket progression.

### Step 3.1: Match Game Repository

Add `TournamentMatchGameRepository` trait and `PgTournamentMatchGameRepository` adapter.

### Step 3.2: Match Service

**File**: Create `crates/portal-domain/src/services/tournament/match_service.rs`

Implement:
- Match scheduling
- Pre-match check-in
- Match start
- Game result submission
- Match result finalization
- Result confirmation
- Forfeit handling

### Step 3.3: Progression Service

**File**: Create `crates/portal-domain/src/services/tournament/progression.rs`

Implement:
- `process_match_completion()` - advances winners
- `check_bracket_complete()`
- `check_stage_complete()`
- `check_tournament_complete()`

### Step 3.4: Dispute Handling

Add to match service:
- `raise_dispute()`
- `resolve_dispute()`
- `override_result()` - admin

### Step 3.5: Match Handlers

**File**: Create `crates/portal-api/src/handlers/tournaments/matches.rs`

All match endpoints from design document.

### Step 3.6: Write Tests

Comprehensive match lifecycle tests:
- Scheduling
- Result submission
- Winner advancement
- Double elimination loser bracket
- Forfeit scenarios
- Dispute flow

---

## Phase 4: Plugin Integration

### Prerequisites
- Phase 3 complete

### Goal
Game-specific customization: map pools, veto system, settings validation.

### Step 4.1: Extend GamePlugin Trait

**File**: `crates/portal-plugins/src/traits.rs`

Add tournament methods to `GamePlugin`:
```rust
fn validate_tournament_settings(&self, settings: &Value) -> Result<(), String>;
fn tournament_format_config(&self, format: &TournamentFormatId) -> Option<FormatConfig>;
fn calculate_tournament_seeding(&self, participants: &[ParticipantRating]) -> Option<Vec<Seed>>;
fn validate_match_result(&self, result: &MatchResult) -> Result<(), String>;
fn get_map_veto_formats(&self) -> Vec<MapVetoFormat>;
```

### Step 4.2: Map Pool System

Create migration for `tournament_map_pools` and `tournament_map_veto_logs`.

Implement:
- `TournamentMapPoolRepository`
- `TournamentMapVetoLogRepository`
- Map pool handlers

### Step 4.3: Map Veto Service

**File**: Create `crates/portal-domain/src/services/tournament/map_veto.rs`

Implement veto state machine:
- Track current state (bans, picks, remaining maps)
- Validate actions
- Determine next action
- Complete veto

### Step 4.4: CS2 Tournament Support

**File**: `crates/portal-plugins/src/games/cs2/mod.rs`

Implement all tournament-related `GamePlugin` methods for CS2.

### Step 4.5: Write Tests

- Plugin settings validation
- Map veto flow
- Game-specific result validation

---

## Phase 5: Advanced Formats

### Prerequisites
- Phase 4 complete

### Goal
All bracket formats: double elimination, round robin, swiss, groups+playoffs.

### Step 5.1: Double Elimination Generator

Implement winners bracket, losers bracket, and grand final logic.

Key test cases:
- Winner advances in winners bracket
- Loser drops to losers bracket
- Grand final reset scenario

### Step 5.2: Round Robin Generator

Implement:
- Pairing generation (Berger tables)
- Standings calculation
- Tiebreaker logic (head-to-head, game differential)

### Step 5.3: Swiss Generator

Implement:
- Dynamic pairing after each round
- Buchholz scoring
- Avoiding repeat matchups

### Step 5.4: Groups + Playoffs

Implement:
- Group stage generation
- Standings per group
- Advancement to playoffs
- Playoff bracket generation from group results

### Step 5.5: Standings System

Create migration for `tournament_standings`.

Implement:
- `TournamentStandingsRepository`
- Standings calculation service
- Standings handlers

### Step 5.6: Write Extensive Tests

Each format needs comprehensive tests for:
- Initial bracket generation
- Match completion and advancement
- Edge cases (byes, withdrawals)
- Correct final standings

---

## Phase 6: Polish & Performance

### Prerequisites
- Phase 5 complete

### Goal
Production readiness: performance, admin tools, documentation.

### Step 6.1: Materialized Views

Create `mv_tournament_bracket_display` materialized view.
Implement refresh strategy.

### Step 6.2: Query Optimization

Profile slow queries.
Add missing indexes.
Optimize bracket retrieval.

### Step 6.3: Admin Tools

Admin-only endpoints:
- Tournament duplication
- Bulk result updates
- Participant management
- Audit logging

### Step 6.4: Documentation

- Update CLAUDE.md with tournament section
- Ensure all OpenAPI docs are complete
- Add examples to API responses

---

## General Guidelines

### Code Style
- Follow existing patterns exactly
- Use `#[instrument(skip(self))]` for tracing
- Use `info!()`, `warn!()`, `error!()` for logging
- Handle all error cases explicitly

### Testing
- Every public method needs tests
- Use builders for test data
- Test both success and error cases
- Integration tests should be isolated (separate test database)

### Commits
- One commit per logical change
- Reference the phase in commit messages
- Example: `[Tournament Phase 1] Add core domain entities`

### When Stuck
1. Re-read the design document section
2. Look at how league_teams does it
3. Check if there's a similar pattern elsewhere
4. Ask for clarification rather than guessing

---

## Quick Reference

### File Locations

| Component | Location |
|-----------|----------|
| IDs | `crates/portal-core/src/ids.rs` |
| Types/Enums | `crates/portal-core/src/types/` |
| Errors | `crates/portal-core/src/errors.rs` |
| Domain Entities | `crates/portal-domain/src/entities/` |
| Repository Traits | `crates/portal-domain/src/repositories/` |
| Services | `crates/portal-domain/src/services/` |
| DB Entities | `crates/portal-db/src/entities/` |
| Adapters | `crates/portal-db/src/adapters/` |
| Handlers | `crates/portal-api/src/handlers/` |
| Routes | `crates/portal-api/src/routes/` |
| DTOs | `crates/portal-api/src/dto/` |
| OpenAPI | `crates/portal-api/src/openapi.rs` |
| State | `crates/portal-api/src/state.rs` |
| Tests | `crates/portal-api/tests/` |
| Builders | `crates/portal-test/src/builders/` |

### Test Commands

```bash
# All tests
cargo test

# Specific crate
cargo test -p portal-domain

# Pattern match
cargo test tournament

# Single test with output
cargo test test_create_tournament -- --nocapture

# Integration tests only
cargo test -p portal-api

# With database logs
RUST_LOG=sqlx=debug cargo test
```

### Build Commands

```bash
# Check all
cargo check --workspace

# Build all
cargo build --workspace

# Run server
cargo run -p portal-app

# SQLx offline mode
cargo sqlx prepare --workspace
```
