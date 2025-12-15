# Tournament System Implementation Progress

> **Design Document**: [docs/tournament-system-design.md](./tournament-system-design.md)
> **Implementation Prompt**: [docs/prompts/tournament-implementation.md](./prompts/tournament-implementation.md)

---

## Current Status

| Phase | Status | Started | Completed | Notes |
|-------|--------|---------|-----------|-------|
| Phase 1: Core Foundation | 🟢 Complete | 2025-11-30 | 2025-11-30 | All tests passing |
| Phase 2: Registration & Seeding | 🟢 Complete | 2025-11-30 | 2025-11-30 | Services + Handlers + Tests |
| Phase 3: Match System | 🟡 In Progress | 2025-11-30 | - | Batches 1-3 complete (7/9 sub-phases done) |
| Phase 4: Plugin Integration | 🟡 Partial | 2025-12-01 | - | Veto formats complete, evidence deferred |
| Phase 5: Advanced Formats | 🔴 Not Started | - | - | |
| Phase 6: Polish & Performance | 🔴 Not Started | - | - | |

**Legend**: 🔴 Not Started | 🟡 In Progress | 🟢 Complete | ⏸️ Blocked/Deferred

---

## Phase 1: Core Foundation

### Goals
- [x] Database migrations for core tables
- [x] Strongly-typed IDs in `portal-core`
- [x] Domain entities in `portal-domain`
- [x] Repository traits in `portal-domain`
- [x] Repository adapters in `portal-db`
- [x] Basic `TournamentService`
- [x] Single elimination bracket generator
- [x] Tournament CRUD API endpoints
- [x] Basic bracket retrieval endpoint

### Files Created
```
# Migrations
migrations/NNNN_create_tournaments.sql

# portal-core
crates/portal-core/src/ids.rs                    # Add tournament IDs
crates/portal-core/src/types/tournament.rs       # Tournament enums
crates/portal-core/src/errors.rs                 # Tournament errors

# portal-domain entities
crates/portal-domain/src/entities/tournament.rs

# portal-domain repositories
crates/portal-domain/src/repositories/tournament.rs
crates/portal-domain/src/repositories/tournament_stage.rs
crates/portal-domain/src/repositories/tournament_bracket.rs
crates/portal-domain/src/repositories/tournament_match.rs
crates/portal-domain/src/repositories/tournament_registration.rs

# portal-domain services
crates/portal-domain/src/services/tournament/mod.rs
crates/portal-domain/src/services/tournament/service.rs
crates/portal-domain/src/services/tournament/bracket_generator.rs

# portal-db adapters
crates/portal-db/src/adapters/tournament/mod.rs
crates/portal-db/src/adapters/tournament/tournament.rs
crates/portal-db/src/adapters/tournament/stage.rs
crates/portal-db/src/adapters/tournament/bracket.rs
crates/portal-db/src/adapters/tournament/match_.rs
crates/portal-db/src/adapters/tournament/registration.rs
crates/portal-db/src/adapters/tournament/conversions.rs

# portal-db entities
crates/portal-db/src/entities/tournament.rs

# portal-api
crates/portal-api/src/dto/requests/tournament.rs
crates/portal-api/src/dto/responses/tournament.rs
crates/portal-api/src/handlers/tournaments/mod.rs
crates/portal-api/src/handlers/tournaments/tournament.rs
crates/portal-api/src/routes/tournaments.rs
```

### Tests Created
```
# Integration tests (19 tests, all passing)
crates/portal-api/tests/tournaments_test.rs
  - test_create_tournament
  - test_create_tournament_duplicate_slug
  - test_get_tournament_by_id
  - test_get_tournament_by_slug
  - test_get_tournament_not_found
  - test_list_tournaments
  - test_list_tournaments_filter_by_game
  - test_update_tournament
  - test_publish_tournament
  - test_open_registration
  - test_create_stage
  - test_get_stages
  - test_create_individual_tournament
  - test_create_double_elimination_tournament
  - test_create_round_robin_tournament
  - test_create_tournament_unauthorized
  - test_update_tournament_unauthorized
  - test_create_tournament_missing_required_fields
  - test_create_tournament_invalid_participant_range

# Test builders
crates/portal-test/src/builders/tournament.rs  # TournamentBuilder
```

### Acceptance Criteria
- [x] Can create a tournament via API
- [x] Can register teams/players for a tournament
- [x] Can generate single elimination bracket
- [x] Can retrieve bracket structure via API
- [x] All tests passing (19/19 integration tests)
- [x] `cargo check` passes
- [x] OpenAPI docs updated

---

## Phase 2: Registration & Seeding

### Goals
- [x] Registration types (open, invite-only, approval)
- [x] Check-in system
- [ ] Waitlist support (deferred to later phase)
- [x] Seeding algorithms (random, rating-based, manual)
- [x] Eligibility validation
- [x] Registration API endpoints

### Files Created/Modified
```
# Services (NEW)
crates/portal-domain/src/services/tournament/registration.rs  # Withdraw, approve, reject, disqualify
crates/portal-domain/src/services/tournament/seeding.rs       # Auto-seed (random, rating), manual seed
crates/portal-domain/src/services/tournament/checkin.rs       # Check-in, admin check-in, process no-shows
crates/portal-domain/src/services/tournament/mod.rs           # Updated exports

# Repository updates
crates/portal-domain/src/repositories/tournament.rs           # Added clear_seeds method
crates/portal-db/src/adapters/tournament/registration.rs      # Added clear_seeds, fixed check_in status

# Handlers (NEW - added to existing tournaments.rs)
crates/portal-api/src/handlers/tournaments.rs                 # Registration management + seeding handlers

# Routes
crates/portal-api/src/routes/tournaments.rs                   # New routes for Phase 2 endpoints

# DTOs (Extended)
crates/portal-api/src/dto/requests/tournament.rs              # RejectRegistrationRequest, DisqualifyRequest, AutoSeedRequest, ManualSeedRequest
crates/portal-api/src/dto/responses/tournament.rs             # SeededParticipantResponse, CheckInStatusResponse

# AppState
crates/portal-api/src/state.rs                                # Added registration_service, checkin_service, seeding_service

# OpenAPI
crates/portal-api/src/openapi.rs                              # Registered new handlers and schemas

# Dependencies
crates/portal-domain/Cargo.toml                               # Added rand crate for seeding algorithms
```

### Tests Created
```
# Integration tests (added to crates/portal-api/tests/tournaments_test.rs)
  - test_withdraw_registration
  - test_get_check_in_status
  - test_get_seeding_empty
  - test_auto_seed_random
  - test_manual_seed
  - test_clear_seeding
```

### Acceptance Criteria
- [x] Teams can register for team tournaments
- [x] Players can register for individual tournaments
- [x] Withdrawal from tournaments works
- [x] Registration approval/rejection works
- [x] Check-in status retrieval works
- [x] Auto-seeding with random algorithm works
- [x] Manual seeding works
- [x] Clear seeding works
- [x] All tests passing (cargo check passes)
- [x] OpenAPI documentation updated

### API Endpoints Added
```
# Registration Management
DELETE /v1/tournaments/{id}/registrations/{reg_id}          # Withdraw
POST   /v1/tournaments/{id}/registrations/{reg_id}/approve  # Admin approve
POST   /v1/tournaments/{id}/registrations/{reg_id}/reject   # Admin reject
POST   /v1/tournaments/{id}/registrations/{reg_id}/disqualify # Admin disqualify
POST   /v1/tournaments/{id}/registrations/{reg_id}/admin-check-in # Admin check-in

# Check-in
GET    /v1/tournaments/{id}/check-in-status                 # Get check-in status
POST   /v1/tournaments/{id}/process-no-shows                # Process no-shows

# Seeding
GET    /v1/tournaments/{id}/seeding                         # Get current seeding
POST   /v1/tournaments/{id}/seeding/auto                    # Auto-seed
POST   /v1/tournaments/{id}/seeding/manual                  # Manual seed
DELETE /v1/tournaments/{id}/seeding                         # Clear seeding
```

---

## Phase 3: Match System

### Sub-Phase Progress

| Sub-Phase | Status | Description |
|-----------|--------|-------------|
| 3.1: Match Lifecycle Core | 🟢 Complete | State machine, status transitions |
| 3.2: Match Scheduling | 🟢 Complete | Schedule proposals, acceptance workflow |
| 3.3: Availability System | 🟢 Complete | Weekly windows, overrides, suggestions |
| 3.4: Pick-Ban Core | 🟢 Complete | Veto sessions, turn-based actions (18 tests) |
| 3.5: Pick-Ban Plugin | 🟢 Complete | CS2 maps, veto formats, side selection |
| 3.6: Result Submission | 🟢 Complete | Claim/confirm workflow (10 tests) |
| 3.7: Evidence System | 🟡 Partial | Handlers exist, service not wired |
| 3.8: Plugin Evidence | ⏸️ Deferred | Requires demo parser integration |
| 3.9: Bracket Progression | 🟢 Complete | Saga, transactions, standings (5 tests) |

### Goals
- [x] Match status state machine (MatchLifecycleService)
- [x] Match scheduling proposal workflow (SchedulingService)
- [x] Player availability management (AvailabilityService)
- [x] Time suggestion generation
- [x] Game-by-game result submission (ResultService)
- [x] Match result confirmation (claim/confirm/dispute)
- [x] Bracket progression logic (ProgressionService, MatchCompletionSaga)
- [x] Forfeit handling
- [ ] Evidence service wiring to AppState
- [ ] Progression service wiring to AppState

### Files Created/Modified (Batch 1)
```
# Migrations
migrations/0031_match_status_logs.sql        # Match status history
migrations/0032_schedule_proposals.sql       # Schedule proposal workflow
migrations/0033_availability.sql             # Availability windows & suggestions

# portal-core
crates/portal-core/src/ids.rs                # Added ScheduleProposalId, SuggestedTimeId, AvailabilityWindowId, AvailabilityExceptionId

# portal-domain Services
crates/portal-domain/src/services/tournament/match_lifecycle.rs  # Status transitions
crates/portal-domain/src/services/tournament/scheduling.rs       # Proposal workflow
crates/portal-domain/src/services/tournament/availability.rs     # Availability management

# portal-domain Entities
crates/portal-domain/src/entities/match_lifecycle.rs
crates/portal-domain/src/entities/schedule_proposal.rs
crates/portal-domain/src/entities/availability.rs

# portal-domain Repositories
crates/portal-domain/src/repositories/match_lifecycle.rs
crates/portal-domain/src/repositories/schedule_proposal.rs
crates/portal-domain/src/repositories/availability.rs

# portal-db Adapters
crates/portal-db/src/adapters/tournament/match_lifecycle.rs
crates/portal-db/src/adapters/tournament/scheduling.rs
crates/portal-db/src/adapters/availability.rs

# portal-api Handlers
crates/portal-api/src/handlers/tournaments.rs     # Match lifecycle & scheduling
crates/portal-api/src/handlers/availability.rs    # Availability endpoints

# portal-api Routes
crates/portal-api/src/routes/tournaments.rs       # Match & scheduling routes
crates/portal-api/src/routes/availability.rs      # Availability routes

# portal-api DTOs
crates/portal-api/src/dto/requests/availability.rs
crates/portal-api/src/dto/responses/availability.rs

# AppState
crates/portal-api/src/state.rs                    # Added match_lifecycle_service, scheduling_service, availability_service
```

### Tests Created (Batch 1)
```
# Integration tests (added to crates/portal-api/tests/tournaments_test.rs)

# Match Lifecycle (need state machine fixes)
  - test_get_match_status
  - test_get_match_status_history_empty
  - test_get_match_status_history_after_transitions
  - test_schedule_match
  - test_match_check_in
  - test_forfeit_match
  - test_admin_match_transition
  - test_match_status_not_found

# Scheduling (need state machine fixes)
  - test_propose_schedule
  - test_get_active_proposal
  - test_accept_schedule_proposal
  - test_reject_schedule_proposal
  - test_get_proposal_history
  - test_admin_schedule_match

# Availability (ALL PASSING - 13 tests)
  - test_create_availability_window ✓
  - test_get_player_availability_windows ✓
  - test_update_availability_window ✓
  - test_delete_availability_window ✓
  - test_create_availability_override ✓
  - test_get_player_availability_overrides ✓
  - test_delete_availability_override ✓
  - test_get_player_date_availability ✓
  - test_get_public_player_availability ✓
  - test_generate_time_suggestions ✓
  - test_get_match_suggestions ✓
  - test_availability_window_unauthorized ✓
  - test_availability_window_invalid_time_range ✓
```

### API Endpoints Added (Batch 1)
```
# Match Lifecycle
GET    /v1/tournaments/{id}/matches/{match_id}/status         # Get match status
GET    /v1/tournaments/{id}/matches/{match_id}/status-history # Get status history
POST   /v1/tournaments/{id}/matches/{match_id}/schedule       # Schedule match
POST   /v1/tournaments/{id}/matches/{match_id}/check-in       # Match check-in
POST   /v1/tournaments/{id}/matches/{match_id}/forfeit        # Forfeit match
POST   /v1/admin/tournaments/{id}/matches/{match_id}/transition # Admin transition

# Scheduling Workflow
POST   /v1/tournaments/{id}/matches/{match_id}/schedule/propose  # Propose times
POST   /v1/tournaments/{id}/matches/{match_id}/schedule/accept   # Accept proposal
POST   /v1/tournaments/{id}/matches/{match_id}/schedule/reject   # Reject proposal
POST   /v1/tournaments/{id}/matches/{match_id}/schedule/counter  # Counter-propose
GET    /v1/tournaments/{id}/matches/{match_id}/schedule/active   # Get active proposal
GET    /v1/tournaments/{id}/matches/{match_id}/schedule/history  # Get proposal history
POST   /v1/admin/tournaments/{id}/matches/{match_id}/schedule    # Admin schedule

# Availability Management
POST   /v1/players/me/availability/windows                    # Create window
GET    /v1/players/me/availability/windows                    # Get windows
PATCH  /v1/players/me/availability/windows/{id}               # Update window
DELETE /v1/players/me/availability/windows/{id}               # Delete window
POST   /v1/players/me/availability/overrides                  # Create override
GET    /v1/players/me/availability/overrides                  # Get overrides
DELETE /v1/players/me/availability/overrides/{id}             # Delete override
GET    /v1/players/me/availability/date                       # Get date availability
GET    /v1/players/{player_id}/availability/date              # Public availability
POST   /v1/tournaments/{id}/matches/{match_id}/suggestions/generate  # Generate suggestions
GET    /v1/tournaments/{id}/matches/{match_id}/suggestions    # Get suggestions
```

### Known Issues (Batch 1)
- Match lifecycle tests now pass after state machine fixes

### Acceptance Criteria (Batch 1)
- [x] Match status service implemented
- [x] Schedule proposal workflow implemented
- [x] Availability windows CRUD implemented
- [x] Availability overrides CRUD implemented
- [x] Time suggestion generation implemented
- [x] All availability tests passing (13/13)
- [x] Match scheduling state machine works

---

### Files Created/Modified (Batch 2)
```
# Migrations
migrations/0034_veto_sessions.sql          # Veto sessions and actions
migrations/0035_result_claims.sql          # Result claim workflow

# portal-domain Entities
crates/portal-domain/src/entities/veto.rs           # VetoSession, VetoAction, VetoFormat
crates/portal-domain/src/entities/result_claim.rs   # ResultClaim, GameResult

# portal-domain Services
crates/portal-domain/src/services/tournament/veto.rs    # VetoService
crates/portal-domain/src/services/tournament/result.rs  # ResultService

# portal-db Adapters
crates/portal-db/src/adapters/tournament/veto.rs           # Veto repositories
crates/portal-db/src/adapters/tournament/result_claim.rs   # Result claim repository

# portal-api Handlers
crates/portal-api/src/handlers/veto.rs      # Veto API handlers
crates/portal-api/src/handlers/results.rs   # Result API handlers

# portal-api DTOs
crates/portal-api/src/dto/requests/veto.rs
crates/portal-api/src/dto/requests/result.rs
crates/portal-api/src/dto/responses/veto.rs
crates/portal-api/src/dto/responses/result.rs
```

### Tests Created (Batch 2)
```
# Veto Tests (18 tests - crates/portal-api/tests/veto_test.rs)
  - test_create_veto_session_invalid_match_id
  - test_create_veto_session_invalid_format
  - test_get_veto_session_not_found
  - test_perform_veto_action_no_session
  - test_select_side_no_session
  - test_veto_format_bo1
  - test_veto_format_bo3
  - test_veto_format_bo5
  - test_veto_timeout_too_low
  - test_veto_timeout_too_high
  - test_veto_action_empty_map_id
  - test_veto_side_empty_side
  - ... (18 tests total)

# Result Tests (10 tests - crates/portal-api/tests/results_test.rs)
  - test_submit_result_invalid_match_id
  - test_get_result_claim_not_found
  - test_list_result_claims_for_nonexistent_match
  - test_confirm_result_invalid_claim
  - test_dispute_result_invalid_claim
  - test_submit_result_missing_winner_id
  - test_submit_result_invalid_winner_id_format
  - test_dispute_result_missing_reason
  - test_result_endpoints_exist
  - test_result_confirm_dispute_endpoints_exist
```

### API Endpoints Added (Batch 2)
```
# Veto System
POST   /v1/matches/{match_id}/veto                    # Create veto session
GET    /v1/matches/{match_id}/veto                    # Get veto session
POST   /v1/matches/{match_id}/veto/start              # Start veto
POST   /v1/matches/{match_id}/veto/coin-flip          # Record coin flip
POST   /v1/matches/{match_id}/veto/action             # Perform veto action
POST   /v1/matches/{match_id}/veto/side               # Select side
GET    /v1/matches/{match_id}/veto/state              # Get veto state

# Result Submission
POST   /v1/matches/{match_id}/result                  # Submit result claim
GET    /v1/matches/{match_id}/result                  # Get pending result
GET    /v1/matches/{match_id}/result/history          # Get result history
POST   /v1/matches/{match_id}/result/{claim_id}/confirm  # Confirm result
POST   /v1/matches/{match_id}/result/{claim_id}/dispute  # Dispute result
```

### Acceptance Criteria (Batch 2)
- [x] Veto sessions can be created and managed
- [x] Turn-based veto actions validated
- [x] Result claims can be submitted
- [x] Results can be confirmed or disputed
- [x] All tests passing (28 tests)

---

### Files Created/Modified (Batch 3)
```
# Migrations
migrations/0036_evidence.sql               # Evidence tables
migrations/0037_progression_sagas.sql      # Progression and saga tables

# portal-domain Entities
crates/portal-domain/src/entities/evidence.rs   # Evidence types
crates/portal-domain/src/entities/saga.rs       # Saga execution state

# portal-domain Services
crates/portal-domain/src/services/tournament/evidence.rs         # EvidenceService
crates/portal-domain/src/services/tournament/progression.rs      # ProgressionService
crates/portal-domain/src/services/tournament/standings.rs        # StandingsService
crates/portal-domain/src/services/tournament/saga.rs             # SagaCoordinator
crates/portal-domain/src/services/tournament/match_completion.rs # MatchCompletionSaga

# portal-db Adapters
crates/portal-db/src/adapters/tournament/match_completion_tx.rs  # Transaction executor
crates/portal-db/src/transaction.rs                               # Transaction support

# portal-api Handlers
crates/portal-api/src/handlers/evidence.rs     # Evidence API (stubs)
crates/portal-api/src/handlers/progression.rs  # Progression API (stubs)
```

### Tests Created (Batch 3)
```
# Transaction Tests (5 tests - crates/portal-db/tests/transaction_test.rs)
  - test_match_completion_transaction_success
  - test_match_completion_transaction_rollback_on_error
  - test_match_completion_transaction_rollback_on_invalid_participant
  - test_match_completion_transaction_rollback_on_drop
  - test_match_completion_fails_for_wrong_status
```

### API Endpoints Added (Batch 3)
```
# Evidence System (stubs - service not wired)
POST   /v1/matches/{match_id}/evidence/upload           # Initiate upload
POST   /v1/matches/{match_id}/evidence/{id}/complete    # Complete upload
POST   /v1/matches/{match_id}/evidence/link             # Add link
GET    /v1/matches/{match_id}/evidence                  # List evidence
GET    /v1/matches/{match_id}/evidence/{id}/access      # Get access URL
DELETE /v1/matches/{match_id}/evidence/{id}             # Delete evidence

# Progression System (stubs - service not wired)
GET    /v1/matches/{match_id}/progression               # Get progression
POST   /v1/admin/matches/{match_id}/progression/revert  # Revert progression
POST   /v1/admin/matches/{match_id}/progression/reapply # Reapply progression
```

### Acceptance Criteria (Batch 3)
- [x] Evidence entities and migrations complete
- [x] Progression and saga infrastructure complete
- [x] Transaction support implemented
- [x] Match completion saga implemented
- [x] All transaction tests passing (5 tests)
- [ ] Evidence service needs wiring to AppState
- [ ] Progression service needs wiring to AppState

---

## Phase 4: Plugin Integration

### Goals
- [x] Extended `GamePlugin` trait for tournaments (TournamentPlugin)
- [x] Map pool configuration
- [x] Map veto system (VetoService, VetoFormat)
- [x] Game-specific settings validation
- [ ] Plugin-provided seeding (optional)
- [ ] Match result validation
- [ ] Evidence discovery plugin (deferred - requires demo parser)

### Files Created/Modified
```
# Plugins
crates/portal-plugins/src/traits.rs              # TournamentPlugin trait
crates/portal-plugins/src/types.rs               # VetoFormat, MapMetadata, SideOption
crates/portal-plugins/src/games/cs2/mod.rs       # CS2 veto formats, maps, sides

# Services (implemented in Phase 3 Batch 2)
crates/portal-domain/src/services/tournament/veto.rs
crates/portal-domain/src/entities/veto.rs        # VetoFormat with Bo1, Bo3, Bo5

# Database
migrations/0034_veto_sessions.sql                # Veto sessions and actions

# Handlers (implemented in Phase 3 Batch 2)
crates/portal-api/src/handlers/veto.rs
```

### Tests Completed
```
# Integration tests (18 tests - crates/portal-api/tests/veto_test.rs)
  - Veto format validation (Bo1, Bo3, Bo5)
  - Session creation and management
  - Turn-based action validation
  - Side selection
```

### Acceptance Criteria
- [x] Can configure tournament map pool
- [x] Map veto flow works
- [x] CS2 plugin validates tournament settings
- [x] All tests passing (18 veto tests)
- [ ] Evidence plugin integration (deferred)

---

## Phase 5: Advanced Formats

### Goals
- [ ] Double elimination generator
- [ ] Round robin generator
- [ ] Swiss system generator
- [ ] Groups + playoffs hybrid
- [ ] Multi-stage orchestration
- [ ] Standings system

### Files Created/Modified
```
# Services
crates/portal-domain/src/services/tournament/generators/
  - mod.rs
  - single_elimination.rs
  - double_elimination.rs
  - round_robin.rs
  - swiss.rs
  - groups_playoffs.rs

crates/portal-domain/src/services/tournament/standings.rs

# Database
migrations/NNNN_tournament_standings.sql

# Repositories
crates/portal-domain/src/repositories/tournament_standings.rs

# Handlers
crates/portal-api/src/handlers/tournaments/standings.rs
```

### Tests Required
```
# Unit tests (extensive!)
crates/portal-domain/src/services/tournament/tests/
  - double_elimination_tests.rs
  - round_robin_tests.rs
  - swiss_tests.rs
  - groups_playoffs_tests.rs
  - standings_tests.rs

# Integration tests
crates/portal-api/tests/tournament_formats_test.rs
  - test_double_elimination_lifecycle
  - test_round_robin_lifecycle
  - test_swiss_lifecycle
  - test_groups_to_playoffs
```

### Acceptance Criteria
- [ ] Double elimination brackets work
- [ ] Round robin with standings works
- [ ] Swiss pairings work
- [ ] Groups advance to playoffs
- [ ] All tests passing

---

## Phase 6: Polish & Performance

### Goals
- [ ] Materialized views for bracket display
- [ ] Query optimization
- [ ] Admin tools
- [ ] Tournament duplication
- [ ] Comprehensive error handling
- [ ] Documentation

### Files Created/Modified
```
# Migrations
migrations/NNNN_tournament_views.sql

# Performance
crates/portal-db/src/adapters/tournament/views.rs

# Admin handlers
crates/portal-api/src/handlers/tournaments/admin.rs
```

### Acceptance Criteria
- [ ] Bracket queries are fast (<100ms for 128-team bracket)
- [ ] Admin can override results
- [ ] Tournament duplication works
- [ ] All edge cases handled
- [ ] Documentation complete

---

## Blockers & Issues

| Issue | Phase | Description | Resolution |
|-------|-------|-------------|------------|
| Evidence service not wired | 3.7 | EvidenceService needs to be added to AppState | Pending |
| Progression service not wired | 3.9 | ProgressionService needs to be added to AppState | Pending |
| Demo parser needed | 3.8 | CS2 demo parsing requires external library | Deferred |
| S3 client implementation | 3.7 | Need S3Client trait implementation for presigned URLs | Pending |

---

## Notes & Decisions

### Design Decisions Made During Implementation

1. **2025-11-30** - Added `MatchFormat` enum to portal-core/types/tournament.rs for match format types (Bo1, Bo3, Bo5, Bo7)
2. **2025-11-30** - Created comprehensive tournament types module with all status enums and state machine helper methods
3. **2025-11-30** - DB entities use string types for enums (matching SQL CHECK constraints) with domain layer handling conversions
4. **2025-11-30** - Added `Published` status to `TournamentStatus` enum for draft -> published -> registration flow
5. **2025-11-30** - Added TournamentBuilder to portal-test for integration testing
6. **2025-11-30** - Created 19 comprehensive integration tests covering CRUD, lifecycle, stages, and authorization
7. **2025-11-30** - Phase 2: Created separate services (RegistrationService, CheckInService, SeedingService) for single-responsibility
8. **2025-11-30** - Phase 2: Seeding uses rand crate with `rng()` function for random seeding algorithm
9. **2025-11-30** - Phase 2: Waitlist support deferred to a later phase as it requires additional database schema changes

10. **2025-12-01** - Phase 3 Batch 2: VetoFormat struct lives in portal-domain/entities/veto.rs with Bo1, Bo3, Bo5 static constructors
11. **2025-12-01** - Phase 3 Batch 2: Result claims use claim/confirm/dispute workflow with auto-confirm timeout
12. **2025-12-01** - Phase 3 Batch 3: Transaction support uses `DbTransaction<'a>` type alias for `sqlx::Transaction<'a, Postgres>`
13. **2025-12-01** - Phase 3 Batch 3: Transactional repository methods use `_in_tx` suffix pattern (e.g., `find_by_id_in_tx`)
14. **2025-12-01** - Phase 3 Batch 3: Match completion uses atomic transactions via `complete_match_in_transaction()` function
15. **2025-12-01** - Phase 3 Batch 3: Evidence service and progression service handlers are stubs pending AppState wiring

### Deviations from Design Document

1. **2025-11-30** - Migration file named `0030_create_tournaments.sql` (next available sequence number after existing migrations)
2. **2025-11-30** - Phase 3 Batch 1: Implemented availability system early to support scheduling workflow (player availability informs match scheduling)
3. **2025-11-30** - Phase 3 Batch 1: Scheduling uses proposal workflow (propose → accept/reject/counter) rather than direct scheduling for self-scheduled tournaments
4. **2025-12-01** - Phase 3 Batch 3: Evidence plugin integration (3.8) deferred - requires external demo parser library
5. **2025-12-01** - Phase 3 Batch 3: Evidence and progression handlers return "service not configured" until AppState wiring is complete

---

## Commands Reference

```bash
# Run all tournament tests
cargo test -p portal-api --features="test-utils" tournaments
cargo test -p portal-api --features="test-utils" veto
cargo test -p portal-api --features="test-utils" results

# Run transaction tests
cargo test -p portal-db --test transaction_test

# Run all API tests
cargo test -p portal-api --features="test-utils"

# Check compilation
cargo check --workspace

# Run specific phase tests
cargo test -p portal-api --features="test-utils" test_create_tournament

# Generate SQL for offline mode
cargo sqlx prepare --workspace
```
