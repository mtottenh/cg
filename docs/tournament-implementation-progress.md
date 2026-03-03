# Tournament System Implementation Progress

> **Design Document**: [docs/tournament-system-design.md](./tournament-system-design.md)
> **Implementation Prompt**: [docs/prompts/tournament-implementation.md](./prompts/tournament-implementation.md)

---

## Current Status

| Phase | Status | Started | Completed | Notes |
|-------|--------|---------|-----------|-------|
| Phase 1: Core Foundation | 🟢 Complete | 2025-11-30 | 2025-11-30 | All tests passing |
| Phase 2: Registration & Seeding | 🟢 Complete | 2025-11-30 | 2025-11-30 | Services + Handlers + Tests |
| Phase 3: Match System | 🟢 Complete | 2025-11-30 | 2026-03-01 | All 9 sub-phases done (Batches 1-4) |
| Phase 4: Plugin & Demo Integration | 🟢 Complete | 2025-12-01 | 2026-03-02 | 4.0-4.3 complete, 4.4 saga integration wired end-to-end |
| Phase 5: Advanced Formats | 🟡 In Progress | 2026-03-02 | - | 5.1 Double Elimination complete |
| Phase 6: Polish & Performance | 🔴 Not Started | - | - | |

**Legend**: 🔴 Not Started | 🟡 In Progress | 🟢 Complete | ⏸️ Blocked/Deferred

**Overall Scale**: 194 API handlers, 228 OpenAPI paths, 44 database migrations, 27 domain services, 834+ integration tests across 18 test files.

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
| 3.7: Evidence System | 🟢 Complete | Services wired to AppState, catalog-based discovery (46 tests) |
| 3.8: Plugin Evidence | 🟢 Complete | CS2 plugin evidence, scanner CLI, demo ingestion pipeline |
| 3.9: Bracket Progression | 🟢 Complete | Saga, transactions, standings, wired to AppState (5 tests) |

### Goals
- [x] Match status state machine (MatchLifecycleService)
- [x] Match scheduling proposal workflow (SchedulingService)
- [x] Player availability management (AvailabilityService)
- [x] Time suggestion generation
- [x] Game-by-game result submission (ResultService)
- [x] Match result confirmation (claim/confirm/dispute)
- [x] Bracket progression logic (ProgressionService, MatchCompletionSaga)
- [x] Forfeit handling
- [x] Evidence service wiring to AppState
- [x] Progression service wiring to AppState
- [x] Demo catalog integration into evidence discovery
- [x] Scanner CLI for automated demo ingestion

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
- [x] Evidence service wired to AppState
- [x] Progression service wired to AppState

---

### Files Created/Modified (Batch 4 — Evidence Wiring, Demo Catalog & Scanner)
```
# portal-api — Evidence & Demo handlers (fully wired)
crates/portal-api/src/handlers/evidence.rs         # Full evidence service (upload, link, discover, validate)
crates/portal-api/src/handlers/demos.rs            # +batch_catalog, submit_stats, mark_failed handlers
crates/portal-api/src/handlers/games.rs            # +game management (update, enable/disable, map pool)
crates/portal-api/src/adapters/evidence_plugin.rs  # Evidence plugin adapter layer
crates/portal-api/src/adapters/mod.rs

# portal-api — Routes & OpenAPI
crates/portal-api/src/routes/admin.rs              # +batch, stats, stats-failed routes
crates/portal-api/src/routes/games.rs              # +game management routes
crates/portal-api/src/openapi.rs                   # Registered new handlers and schemas
crates/portal-api/src/state.rs                     # Wired evidence, demo, progression, result_review services

# portal-api — DTOs
crates/portal-api/src/dto/requests/demo.rs         # +SubmitDemoStatsRequest, BatchCatalogDemosRequest, MarkDemoFailedRequest
crates/portal-api/src/dto/requests/game.rs         # +UpdateGameRequest, SetMapPoolRequest
crates/portal-api/src/dto/responses/demo.rs        # +BatchCatalogResultResponse

# portal-domain — Services
crates/portal-domain/src/services/demo.rs          # +discover_for_match, CatalogResult enum, relevance scoring
crates/portal-domain/src/services/tournament/evidence.rs  # Full evidence service with S3 + plugin traits
crates/portal-domain/src/repositories/demo.rs      # +find_matching_for_context trait method

# portal-db — Adapters
crates/portal-db/src/adapters/demo.rs              # +find_matching_for_context SQL implementation

# portal-plugins
crates/portal-plugins/src/traits.rs                # +EvidencePlugin trait with discover/validate
crates/portal-plugins/src/games/cs2/mod.rs         # CS2 evidence discovery & validation implementation
crates/portal-plugins/src/manager.rs               # Plugin manager evidence methods
crates/portal-plugins/src/lib.rs                   # Export EvidencePlugin

# portal-cli — Scanner
crates/portal-cli/src/commands/scan.rs             # NEW: Scanner CLI (S3 list + API client)
crates/portal-cli/src/commands/mod.rs              # +scan module
crates/portal-cli/src/main.rs                      # +Scan command variant
crates/portal-cli/Cargo.toml                       # +feature-gated scanner deps (reqwest, aws-sdk-s3)
```

### Tests Created (Batch 4)
```
# Evidence Tests (46 tests - crates/portal-api/tests/evidence_test.rs)
  - Full evidence lifecycle tests
  - Plugin-based discovery tests
  - Catalog-based discovery tests
  - Evidence linking tests

# Demo Scanner Tests (7 new tests in crates/portal-api/tests/demos_test.rs)
  - test_batch_catalog_demos_creates_new
  - test_batch_catalog_demos_idempotent
  - test_submit_demo_stats
  - test_submit_demo_stats_idempotent
  - test_mark_demo_stats_failed
  - test_discover_evidence_finds_catalog_demos
  - test_link_catalog_discovered_evidence

# Game Tests (84 tests - crates/portal-api/tests/games_test.rs)
  - Game CRUD, map pool management, enable/disable

# Player Tests (24 tests - crates/portal-api/tests/players_test.rs)
# Role Tests (32 tests - crates/portal-api/tests/roles_test.rs)
```

### API Endpoints Added (Batch 4)
```
# Evidence System (fully wired — no longer stubs)
POST   /v1/matches/{match_id}/evidence/upload           # Initiate upload
POST   /v1/matches/{match_id}/evidence/{id}/complete    # Complete upload
POST   /v1/matches/{match_id}/evidence/link             # Add link evidence
GET    /v1/matches/{match_id}/evidence                  # List evidence
GET    /v1/matches/{match_id}/evidence/{id}/access      # Get access URL
DELETE /v1/matches/{match_id}/evidence/{id}             # Delete evidence
POST   /v1/matches/{match_id}/evidence/discover         # Discover available evidence
POST   /v1/matches/{match_id}/evidence/link-discovered  # Link discovered evidence
POST   /v1/matches/{match_id}/evidence/{id}/validate    # Validate evidence against result
GET    /v1/matches/{match_id}/evidence/{id}             # Get single evidence
GET    /v1/matches/{match_id}/games/{game_number}/evidence  # Get game evidence
POST   /v1/matches/{match_id}/evidence/process-expired  # Process expired evidence
GET    /v1/matches/{match_id}/demos                     # Get demos linked to match

# Demo Ingestion (scanner support)
POST   /v1/admin/demos/batch                            # Batch catalog demos
POST   /v1/admin/demos/{id}/stats                       # Submit parsed demo stats
POST   /v1/admin/demos/{id}/stats-failed                # Mark stats fetch as failed

# Progression System (fully wired — no longer stubs)
GET    /v1/matches/{match_id}/progression               # Get progression
POST   /v1/admin/matches/{match_id}/progression/revert  # Revert progression
POST   /v1/admin/matches/{match_id}/progression/reapply # Reapply progression
POST   /v1/admin/matches/{match_id}/progression/process # Process progression

# Game Management
PATCH  /v1/games/{game_id}                              # Update game
PUT    /v1/games/{game_id}/maps                         # Set map pool
POST   /v1/games/{game_id}/enable                       # Enable game
POST   /v1/games/{game_id}/disable                      # Disable game
```

### Acceptance Criteria (Batch 4)
- [x] Evidence service fully wired with upload, link, discover, validate
- [x] Progression service fully wired
- [x] Demo catalog integration in evidence discovery (hybrid plugin + catalog)
- [x] Batch demo cataloging API for scanner
- [x] Stats submission and failure marking APIs
- [x] Scanner CLI command with S3 listing and API client
- [x] CS2 plugin evidence discovery and validation
- [x] Game management endpoints (update, enable/disable, map pool)
- [x] All tests passing (evidence: 46, demos: 34, games: 84)

---

## Phase 4: Plugin & Demo Integration

> **Design Document**: [docs/phase4/00-overview.md](./phase4/00-overview.md)

### Sub-Phase Progress

| Sub-Phase | Status | Description |
|-----------|--------|-------------|
| 4.0: Veto Plugin | 🟢 Complete | TournamentPlugin trait, CS2 veto formats, map pool |
| 4.1: Demo Handlers & Validation | 🟢 Complete | Match demos endpoint, unlink, DemoValidationResult |
| 4.2: Result Claim Demo Bridge | 🟢 Complete | demo_link_ids on result_claims, submit with demo refs |
| 4.3: Result Review System | 🟢 Complete | ResultReviewService, captain acknowledgment, admin approve/reject (28 tests) |
| 4.4: Review Workflow Integration | 🟢 Complete | Saga demo validation step, pause/resume on review |

### Goals
- [x] Extended `GamePlugin` trait for tournaments (TournamentPlugin)
- [x] Map pool configuration
- [x] Map veto system (VetoService, VetoFormat)
- [x] Game-specific settings validation
- [x] Evidence discovery plugin (CS2 + catalog-based)
- [x] Demo-to-match linking (GET /matches/{id}/demos, DELETE unlink)
- [x] DemoValidationResult entity and validation methods
- [x] Result claims with demo_link_ids (migration 0041)
- [x] Result review system (migration 0042, service, handlers, 28 tests)
- [ ] Plugin-provided seeding (optional, low priority)
- [x] Saga demo validation step (Phase 4.4)

### Files Created/Modified
```
# Plugins
crates/portal-plugins/src/traits.rs              # TournamentPlugin + EvidencePlugin traits
crates/portal-plugins/src/types.rs               # VetoFormat, MapMetadata, SideOption
crates/portal-plugins/src/games/cs2/mod.rs       # CS2 veto formats, maps, evidence discovery/validation

# Veto (implemented in Phase 3 Batch 2)
crates/portal-domain/src/services/tournament/veto.rs
crates/portal-domain/src/entities/veto.rs
migrations/0034_veto_sessions.sql
crates/portal-api/src/handlers/veto.rs

# Demo Integration (4.1)
crates/portal-api/src/handlers/demos.rs          # get_demos_for_match, unlink_demo_from_match
crates/portal-api/src/routes/matches.rs          # GET /{match_id}/demos route
crates/portal-api/src/routes/admin.rs            # DELETE /demos/{id}/link/{match_id}
crates/portal-domain/src/entities/demo_validation.rs  # DemoValidationResult entity

# Result Claim Bridge (4.2)
migrations/0041_result_claims_demo_links.sql     # Add demo_link_ids UUID[] to result_claims
crates/portal-domain/src/entities/result_claim.rs    # demo_link_ids field
crates/portal-api/src/dto/requests/result.rs         # demo_link_ids in SubmitResultClaimRequest
crates/portal-db/src/adapters/tournament/result_claim.rs  # Handle demo_link_ids

# Result Review System (4.3)
migrations/0042_result_reviews.sql               # result_reviews table
crates/portal-domain/src/entities/result_review.rs   # ResultReview entity
crates/portal-domain/src/services/tournament/result_review.rs  # ResultReviewService
crates/portal-api/src/handlers/result_reviews.rs     # Review handlers (341 lines)
crates/portal-api/src/routes/matches.rs              # Review routes
crates/portal-api/src/routes/admin.rs                # Admin review routes
```

### Tests Completed
```
# Veto Tests (50 tests - crates/portal-api/tests/veto_test.rs)
# Veto Delegate Tests (20 tests - crates/portal-api/tests/veto_delegates_test.rs)
# Veto WebSocket Tests (24 tests - crates/portal-api/tests/veto_ws_test.rs)
# Result Review Tests (28 tests - crates/portal-api/tests/result_review_test.rs)
```

### API Endpoints Added
```
# Veto System
POST   /v1/matches/{match_id}/veto                    # Create veto session
GET    /v1/matches/{match_id}/veto                    # Get veto session
POST   /v1/matches/{match_id}/veto/start              # Start veto
POST   /v1/matches/{match_id}/veto/coin-flip          # Record coin flip
POST   /v1/matches/{match_id}/veto/action             # Perform veto action
POST   /v1/matches/{match_id}/veto/side               # Select side

# Demo-Match Integration
GET    /v1/matches/{match_id}/demos                   # Get demos for match
DELETE /v1/admin/demos/{id}/link/{match_id}           # Unlink demo from match

# Result Reviews
GET    /v1/matches/{match_id}/result-review           # Get review status
POST   /v1/matches/{match_id}/result-review/acknowledge  # Captain acknowledgment
GET    /v1/admin/result-reviews                       # List pending reviews
GET    /v1/admin/result-reviews/{id}                  # Get review details
POST   /v1/admin/result-reviews/{id}/approve          # Admin approve
POST   /v1/admin/result-reviews/{id}/reject           # Admin reject
```

### Acceptance Criteria
- [x] Can configure tournament map pool
- [x] Map veto flow works
- [x] CS2 plugin validates tournament settings
- [x] All veto tests passing (50 + 20 + 24 = 94 tests)
- [x] Evidence plugin integration (CS2 + catalog discovery)
- [x] Demo-to-match linking endpoints work
- [x] Result claims support demo_link_ids
- [x] Result review system with captain acknowledgment and admin resolution
- [x] All result review tests passing (28 tests)
- [x] Saga demo validation step (Phase 4.4)

---

## Phase 5: Advanced Formats

### Sub-Phase Progress

| Sub-Phase | Status | Description |
|-----------|--------|-------------|
| 5.1: Double Elimination | 🟢 Complete | Generator, service dispatch, cross-bracket links, 12 unit + 3 integration tests |
| 5.2: Round Robin | 🔴 Not Started | Round robin generator, standings |
| 5.3: Swiss System | 🔴 Not Started | Swiss pairing generator |
| 5.4: Groups + Playoffs | 🔴 Not Started | Multi-stage hybrid |

### Goals
- [x] Double elimination generator
- [ ] Round robin generator
- [ ] Swiss system generator
- [ ] Groups + playoffs hybrid
- [ ] Multi-stage orchestration
- [ ] Standings system

### Phase 5.1: Double Elimination (Complete)

#### Files Created/Modified
```
# Generator
crates/portal-domain/src/services/tournament/bracket_generator.rs
  - GeneratedDoubleElimination struct
  - CrossBracketLink, CrossLinkType types
  - BracketGenerator::double_elimination() method
  - lb_matches_in_round(), lb_participant_sources(), cross_seed_wr1_to_lr1() helpers
  - 12 unit tests

# Service
crates/portal-domain/src/services/tournament/service.rs
  - start_tournament() dispatches on TournamentFormat
  - start_single_elimination() (extracted from old start_tournament)
  - start_double_elimination() (new: creates 3 brackets, links cross-bracket progression)
  - apply_initial_assignments(), apply_byes(), build_position_map(), parse_round_match() helpers

# Exports
crates/portal-domain/src/services/tournament/mod.rs
  - Export GeneratedDoubleElimination, CrossBracketLink, CrossLinkType

# Integration tests
crates/portal-api/tests/tournaments_test.rs
  - test_start_double_elimination_tournament (8 teams, 3 brackets, 14 matches)
  - test_start_double_elimination_4_teams (4 teams, 6 matches)
  - test_double_elimination_with_byes (6 teams in 8-bracket)
```

#### Key Design Decisions
- No grand final reset (single GF match) for simplicity
- Cross-seeding: WR1 losers paired from opposite bracket halves to avoid rematches
- LB alternates between survivor rounds (LB players only) and dropper rounds (WB losers enter)
- Progression links collected into HashMap before writing to avoid partial overwrites
- No new migrations needed — existing columns (bracket_type, loser_progresses_to, participant_source) suffice

#### Acceptance Criteria
- [x] Double elimination brackets generated correctly (4/8/16 teams)
- [x] Byes handled in WB round 1
- [x] Cross-bracket loser progression links set
- [x] WB/LB final winners advance to grand final
- [x] Cross-seeding avoids immediate rematches
- [x] All 12 unit tests passing
- [x] All 3 integration tests passing
- [x] `cargo check --workspace` clean

### Future Phases

#### Files Planned
```
# Round Robin / Swiss / Groups
crates/portal-domain/src/services/tournament/bracket_generator.rs  # Additional methods
crates/portal-domain/src/services/tournament/standings.rs          # Already exists (stub)

# Database (if needed)
migrations/NNNN_tournament_standings.sql
```

### Remaining Acceptance Criteria
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
| ~~Evidence service not wired~~ | 3.7 | ~~EvidenceService needs to be added to AppState~~ | ✅ Resolved (Batch 4) |
| ~~Progression service not wired~~ | 3.9 | ~~ProgressionService needs to be added to AppState~~ | ✅ Resolved (Batch 4) |
| ~~Demo parser needed~~ | 3.8 | ~~CS2 demo parsing requires external library~~ | ✅ Resolved — external stats service + scanner CLI |
| ~~S3 client implementation~~ | 3.7 | ~~Need S3Client trait implementation for presigned URLs~~ | ✅ Resolved (Batch 4) |
| ~~Saga demo validation~~ | 4.4 | ~~MatchCompletionSaga needs step_validate_demos()~~ | ✅ Resolved |

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
15. ~~**2025-12-01** - Phase 3 Batch 3: Evidence and progression handlers were stubs~~ → Resolved in Batch 4

16. **2026-03-01** - Phase 3 Batch 4: Evidence service uses `EvidencePluginClient` and `EvidenceS3Client` traits for testability (adapter pattern in `portal-api/src/adapters/`)
17. **2026-03-01** - Phase 3 Batch 4: Demo evidence discovery uses relevance scoring based on Steam ID overlap (0.50), time proximity (0.30), both-teams-present (0.15), and base score (0.05)
18. **2026-03-01** - Phase 3 Batch 4: Scanner CLI is feature-gated (`#[cfg(feature = "scanner")]`) to avoid pulling in AWS SDK for normal builds
19. **2026-03-01** - Phase 3 Batch 4: Demo ingestion API is game-agnostic (JSON blobs for stats) while typed columns are a CS2 optimization
20. **2026-03-01** - Phase 4: Result claims bridge to demo catalog via `demo_link_ids UUID[]` column (migration 0041), keeping demos and evidence as separate first-class entities
21. **2026-03-01** - Phase 4: Result review system uses two-tier model: roster mismatches need captain acknowledgment, score/winner mismatches need admin approval

### Deviations from Design Document

1. **2025-11-30** - Migration file named `0030_create_tournaments.sql` (next available sequence number after existing migrations)
2. **2025-11-30** - Phase 3 Batch 1: Implemented availability system early to support scheduling workflow (player availability informs match scheduling)
3. **2025-11-30** - Phase 3 Batch 1: Scheduling uses proposal workflow (propose → accept/reject/counter) rather than direct scheduling for self-scheduled tournaments
4. ~~**2025-12-01** - Phase 3 Batch 3: Evidence plugin integration (3.8) deferred~~ → Resolved in Batch 4 via external stats service + scanner CLI
5. ~~**2025-12-01** - Phase 3 Batch 3: Evidence and progression handlers are stubs~~ → Fully wired in Batch 4
6. **2026-03-01** - Phase 3.8: Instead of embedding a demo parser library, uses external CS2 demo stats service (https://demos.cs210mans.uk) with a scanner CLI that fetches stats and submits via API
7. **2026-03-01** - Evidence discovery uses hybrid approach: plugin-based + catalog-based discovery merged in the handler, with relevance scoring for catalog matches

---

## Commands Reference

```bash
# Run all API tests (834+ tests)
cargo test -p portal-api --features="test-utils"

# Run specific test suites
cargo test -p portal-api --features="test-utils" tournaments    # 104 tests
cargo test -p portal-api --features="test-utils" veto           # 50 + 20 + 24 tests
cargo test -p portal-api --features="test-utils" results        # 46 tests
cargo test -p portal-api --features="test-utils" evidence       # 46 tests
cargo test -p portal-api --features="test-utils" demos          # 34 tests
cargo test -p portal-api --features="test-utils" games          # 84 tests
cargo test -p portal-api --features="test-utils" result_review  # 28 tests
cargo test -p portal-api --features="test-utils" dispute        # 64 tests
cargo test -p portal-api --features="test-utils" forfeit        # 34 tests
cargo test -p portal-api --features="test-utils" players        # 24 tests
cargo test -p portal-api --features="test-utils" roles          # 32 tests

# Run transaction tests
cargo test -p portal-db --test transaction_test

# Check compilation
cargo check --workspace

# Lint
cargo clippy -p portal-api -p portal-domain -p portal-db -- -D warnings

# Build scanner CLI
cargo build -p portal-cli --features scanner

# Generate SQL for offline mode
cargo sqlx prepare --workspace
```
