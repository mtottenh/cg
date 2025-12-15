# Phase 3 Implementation - Batch 2: Pick-Ban & Result Submission

## Context

You are implementing **Phase 3 Batch 2** for a multi-game competitive gaming portal backend built in Rust (Axum, SQLx, PostgreSQL). This batch covers the map veto system and result submission workflow.

**Prerequisites**: Batch 1 must be complete (Sub-Phases 3.1, 3.2, 3.3).

**Design Documents** (READ THESE FIRST):
- `docs/phase3/00-overview.md` - Overall architecture
- `docs/phase3/02-pick-ban-system.md` - Map veto design
- `docs/phase3/04-result-submission.md` - Result submission design

**Reference Files**:
- `crates/portal-plugins/src/traits.rs` - Existing plugin traits
- `crates/portal-plugins/src/types.rs` - Plugin types including `MapPickBanFormat`
- `crates/portal-plugins/src/games/cs2/mod.rs` - CS2 plugin reference

---

## Your Task

Implement **Sub-Phases 3.4, 3.5, and 3.6** following the design documents exactly.

### Sub-Phases in This Batch

| Sub-Phase | Name | Description |
|-----------|------|-------------|
| 3.4 | Pick-Ban Core | Veto session state machine, turn-based actions |
| 3.5 | Pick-Ban Plugin Integration | Game-specific maps, veto formats |
| 3.6 | Result Submission | Claim/confirm workflow, game-by-game results |

### Implementation Order

```
3.4 Pick-Ban Core
         │
         ▼
3.5 Pick-Ban Plugin Integration
         │
         ▼
3.6 Result Submission
```

---

## Sub-Phase 3.4: Pick-Ban Core

### Scope

Implement the veto session state machine as defined in `docs/phase3/02-pick-ban-system.md`.

### Deliverables

#### 1. Migration: Veto Tables

Create migration `migrations/0034_veto_sessions.sql`:

```sql
CREATE TABLE veto_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    veto_format_id VARCHAR(64) NOT NULL,
    map_pool TEXT[] NOT NULL,
    first_action_registration_id UUID REFERENCES tournament_registrations(id),
    coin_flip_winner_registration_id UUID REFERENCES tournament_registrations(id),
    current_action_number INTEGER NOT NULL DEFAULT 0,
    current_team_turn UUID REFERENCES tournament_registrations(id),
    remaining_maps TEXT[] NOT NULL,
    selected_maps TEXT[] NOT NULL DEFAULT '{}',
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    action_deadline TIMESTAMPTZ,
    timeout_seconds INTEGER NOT NULL DEFAULT 30,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT veto_sessions_unique_match UNIQUE (match_id),
    CONSTRAINT veto_sessions_check_status CHECK (status IN (
        'pending', 'coin_flip', 'in_progress', 'completed', 'cancelled'
    ))
);

CREATE TABLE veto_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES veto_sessions(id) ON DELETE CASCADE,
    action_number INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,
    map_id VARCHAR(64) NOT NULL,
    performed_by_registration_id UUID REFERENCES tournament_registrations(id),
    performed_by_user_id UUID REFERENCES users(id),
    side_selection VARCHAR(16),
    side_selected_by_registration_id UUID REFERENCES tournament_registrations(id),
    was_auto_action BOOLEAN NOT NULL DEFAULT false,
    auto_action_reason VARCHAR(64),
    performed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    side_selected_at TIMESTAMPTZ,

    CONSTRAINT veto_actions_unique UNIQUE (session_id, action_number),
    CONSTRAINT veto_actions_check_type CHECK (action_type IN ('ban', 'pick', 'decider')),
    CONSTRAINT veto_actions_check_number CHECK (action_number >= 1)
);

CREATE INDEX idx_veto_sessions_match ON veto_sessions(match_id);
CREATE INDEX idx_veto_sessions_status ON veto_sessions(status);
CREATE INDEX idx_veto_sessions_deadline ON veto_sessions(action_deadline) WHERE status = 'in_progress';
CREATE INDEX idx_veto_actions_session ON veto_actions(session_id);
```

#### 2. Domain Entities

Create in `crates/portal-domain/src/entities/`:

- `veto_session.rs` - VetoSession, VetoStatus enum
- `veto_action.rs` - VetoAction, VetoActionType enum

#### 3. Repository

Create `VetoSessionRepository` and `VetoActionRepository` traits and implementations.

#### 4. Service: VetoService

Create `crates/portal-domain/src/services/tournament/veto.rs`:

```rust
pub struct VetoService<VSR, VAR, TMR, TRR> {
    // ...
}

impl VetoService {
    pub async fn create_session(&self, match_id: TournamentMatchId) -> Result<VetoSession, DomainError>;
    pub async fn start_session(&self, session_id: VetoSessionId) -> Result<VetoSession, DomainError>;
    pub async fn record_coin_flip(&self, session_id: VetoSessionId, winner: TournamentRegistrationId) -> Result<VetoSession, DomainError>;
    pub async fn perform_action(&self, session_id: VetoSessionId, map_id: String, performed_by: UserId) -> Result<VetoActionResult, DomainError>;
    pub async fn select_side(&self, session_id: VetoSessionId, action_number: u32, side: String, selected_by: UserId) -> Result<VetoAction, DomainError>;
    pub async fn process_timeout(&self, session_id: VetoSessionId) -> Result<VetoActionResult, DomainError>;
    pub async fn get_session_state(&self, match_id: TournamentMatchId) -> Result<VetoSessionState, DomainError>;
    pub async fn find_timed_out_sessions(&self) -> Result<Vec<VetoSession>, DomainError>;
}
```

#### 5. API Handlers

- `POST /v1/tournaments/{id}/matches/{match_id}/veto/start`
- `POST /v1/tournaments/{id}/matches/{match_id}/veto/coin-flip`
- `POST /v1/tournaments/{id}/matches/{match_id}/veto/action`
- `POST /v1/tournaments/{id}/matches/{match_id}/veto/side`
- `GET /v1/tournaments/{id}/matches/{match_id}/veto/state`

#### 6. Tests

```rust
#[tokio::test]
async fn test_create_veto_session() { ... }

#[tokio::test]
async fn test_perform_ban_action() { ... }

#[tokio::test]
async fn test_perform_pick_action() { ... }

#[tokio::test]
async fn test_wrong_team_cannot_act() { ... }

#[tokio::test]
async fn test_timeout_auto_action() { ... }

#[tokio::test]
async fn test_complete_bo3_veto() { ... }
```

### Acceptance Criteria (3.4)

- [x] Veto sessions track state correctly
- [x] Turn validation enforces correct team
- [x] Actions update remaining/selected maps
- [x] Timeout triggers random selection
- [x] Session completes when all maps determined
- [x] All tests pass (18 veto tests)

---

## Sub-Phase 3.5: Pick-Ban Plugin Integration

### Scope

Extend the plugin system for veto formats as defined in `docs/phase3/02-pick-ban-system.md`.

### Deliverables

#### 1. Extend Plugin Traits

Update `crates/portal-plugins/src/traits.rs`:

```rust
/// Extension trait for tournament features
pub trait TournamentPlugin: GamePlugin {
    fn veto_formats(&self) -> Vec<VetoFormat>;
    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String>;
    fn validate_map_pool_for_veto(&self, maps: &[String], veto_format_id: &str) -> Result<(), String>;
    fn get_map_metadata(&self, map_id: &str) -> Option<MapMetadata>;
    fn get_available_sides(&self, map_id: &str) -> Vec<SideOption>;
}
```

#### 2. Add Types

Update `crates/portal-plugins/src/types.rs`:

```rust
pub struct VetoFormat {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub sequence: Vec<VetoFormatAction>,
    pub min_map_pool: usize,
}

pub struct VetoFormatAction {
    pub team: u8,  // 0 = auto, 1 = first, 2 = second
    pub action_type: VetoActionType,
}

pub struct MapMetadata {
    pub id: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub game_modes: Vec<String>,
}

pub struct SideOption {
    pub id: String,
    pub display_name: String,
    pub short_name: String,
}
```

#### 3. Implement for CS2

Update `crates/portal-plugins/src/games/cs2/mod.rs`:

```rust
impl TournamentPlugin for Cs2Plugin {
    fn veto_formats(&self) -> Vec<VetoFormat> {
        vec![
            VetoFormat::bo1(),
            VetoFormat::bo3(),
            VetoFormat::bo5(),
        ]
    }

    fn get_available_sides(&self, _map_id: &str) -> Vec<SideOption> {
        vec![
            SideOption { id: "ct".into(), display_name: "Counter-Terrorist".into(), short_name: "CT".into() },
            SideOption { id: "t".into(), display_name: "Terrorist".into(), short_name: "T".into() },
        ]
    }
    // ...
}
```

#### 4. Integrate with VetoService

Update VetoService to use plugin for format validation and map metadata.

#### 5. Tests

```rust
#[tokio::test]
async fn test_cs2_veto_formats() { ... }

#[tokio::test]
async fn test_cs2_map_metadata() { ... }

#[tokio::test]
async fn test_cs2_side_options() { ... }
```

### Acceptance Criteria (3.5)

- [x] Plugins provide veto formats
- [x] VetoService uses plugin formats
- [x] Map metadata available from plugin
- [x] Side selection options from plugin
- [x] All tests pass

---

## Sub-Phase 3.6: Result Submission

### Scope

Implement the result claim/confirm workflow as defined in `docs/phase3/04-result-submission.md`.

### Deliverables

#### 1. Migration: Result Claims

Create migration `migrations/0035_result_claims.sql`:

```sql
CREATE TABLE result_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    submitted_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    submitted_by_user_id UUID NOT NULL REFERENCES users(id),
    claimed_winner_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    claimed_participant1_score INTEGER NOT NULL,
    claimed_participant2_score INTEGER NOT NULL,
    game_results JSONB NOT NULL DEFAULT '[]',
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    confirmed_at TIMESTAMPTZ,
    confirmed_by_registration_id UUID REFERENCES tournament_registrations(id),
    confirmed_by_user_id UUID REFERENCES users(id),
    auto_confirm_at TIMESTAMPTZ,
    was_auto_confirmed BOOLEAN NOT NULL DEFAULT false,
    evidence_ids UUID[] NOT NULL DEFAULT '{}',
    submitter_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT result_claims_check_status CHECK (status IN (
        'pending', 'confirmed', 'disputed', 'superseded', 'cancelled'
    )),
    CONSTRAINT result_claims_scores_non_negative CHECK (
        claimed_participant1_score >= 0 AND claimed_participant2_score >= 0
    )
);

CREATE INDEX idx_result_claims_match ON result_claims(match_id);
CREATE INDEX idx_result_claims_status ON result_claims(status);
CREATE INDEX idx_result_claims_auto_confirm ON result_claims(auto_confirm_at) WHERE status = 'pending';
```

#### 2. Domain Entities

Create `crates/portal-domain/src/entities/result_claim.rs`:

```rust
pub struct ResultClaim {
    pub id: ResultClaimId,
    pub match_id: TournamentMatchId,
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub claimed_participant1_score: i32,
    pub claimed_participant2_score: i32,
    pub game_results: Vec<GameResult>,
    pub status: ClaimStatus,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirmed_by_registration_id: Option<TournamentRegistrationId>,
    pub confirmed_by_user_id: Option<UserId>,
    pub auto_confirm_at: Option<DateTime<Utc>>,
    pub was_auto_confirmed: bool,
    pub evidence_ids: Vec<EvidenceId>,
    pub submitter_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct GameResult {
    pub game_number: i32,
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: TournamentRegistrationId,
    // ...
}

pub enum ClaimStatus {
    Pending,
    Confirmed,
    Disputed,
    Superseded,
    Cancelled,
}
```

#### 3. Repository

Create `ResultClaimRepository` trait and implementation.

#### 4. Service: ResultService

Create `crates/portal-domain/src/services/tournament/result.rs`:

```rust
pub struct ResultService<RCR, TMR, TRR, MLS> {
    // ...
    auto_confirm_timeout: Duration,  // e.g., 15 minutes
}

impl ResultService {
    pub async fn submit_claim(&self, match_id: TournamentMatchId, claim: SubmitResultClaim, submitted_by: UserId) -> Result<ResultClaim, DomainError>;
    pub async fn confirm_claim(&self, claim_id: ResultClaimId, confirmed_by: UserId) -> Result<ResultClaim, DomainError>;
    pub async fn dispute_claim(&self, claim_id: ResultClaimId, disputed_by: UserId, reason: String) -> Result<ResultClaim, DomainError>;
    pub async fn cancel_claim(&self, claim_id: ResultClaimId, cancelled_by: UserId) -> Result<ResultClaim, DomainError>;
    pub async fn process_auto_confirmations(&self) -> Result<Vec<ResultClaim>, DomainError>;
    pub async fn get_pending_claim(&self, match_id: TournamentMatchId) -> Result<Option<ResultClaim>, DomainError>;
}
```

#### 5. Validation Logic

Implement score validation as described in design doc:
- Winner must be a participant
- Scores must match winner
- Game count must match format
- Game scores must sum to series score

#### 6. API Handlers

- `POST /v1/tournaments/{id}/matches/{match_id}/result`
- `POST /v1/tournaments/{id}/matches/{match_id}/result/confirm`
- `POST /v1/tournaments/{id}/matches/{match_id}/result/dispute`
- `GET /v1/tournaments/{id}/matches/{match_id}/result`
- `GET /v1/tournaments/{id}/matches/{match_id}/result/history`

#### 7. Tests

```rust
#[tokio::test]
async fn test_submit_result_claim() { ... }

#[tokio::test]
async fn test_confirm_result_claim() { ... }

#[tokio::test]
async fn test_cannot_confirm_own_claim() { ... }

#[tokio::test]
async fn test_dispute_result_claim() { ... }

#[tokio::test]
async fn test_auto_confirmation() { ... }

#[tokio::test]
async fn test_bo3_result_validation() { ... }
```

### Acceptance Criteria (3.6)

- [x] Result claims can be submitted
- [x] Opponent can confirm or dispute
- [x] Cannot confirm own claim
- [x] Auto-confirmation works after timeout
- [x] Score validation catches invalid results
- [x] Game-by-game results stored for series
- [x] All tests pass (10 results tests)

---

## Verification Checklist

Before considering this batch complete:

### Sub-Phase 3.4
- [x] Veto session CRUD works
- [x] Turn-based actions validated
- [x] Timeout handling works
- [x] Integration tests pass (18 tests)

### Sub-Phase 3.5
- [x] TournamentPlugin trait extended
- [x] CS2 plugin implements veto methods
- [x] VetoService uses plugin
- [x] Integration tests pass

### Sub-Phase 3.6
- [x] Result claims CRUD works
- [x] Confirm/dispute workflow works
- [x] Auto-confirmation works
- [x] Validation catches bad input
- [x] Integration tests pass (10 tests)

### Overall
- [x] `cargo check --workspace` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace` passes
- [x] OpenAPI docs complete

---

## Status: ✅ COMPLETE

**Completed**: 2025-12-01

### Implementation Summary

**Files Created/Modified:**
- `migrations/0034_veto_sessions.sql` - Veto tables
- `migrations/0035_result_claims.sql` - Result claims table
- `crates/portal-domain/src/entities/veto.rs` - VetoSession, VetoAction, VetoFormat
- `crates/portal-domain/src/entities/result_claim.rs` - ResultClaim, GameResult
- `crates/portal-domain/src/services/tournament/veto.rs` - VetoService
- `crates/portal-domain/src/services/tournament/result.rs` - ResultService
- `crates/portal-db/src/adapters/tournament/veto.rs` - Veto repositories
- `crates/portal-db/src/adapters/tournament/result_claim.rs` - Result claim repository
- `crates/portal-api/src/handlers/veto.rs` - Veto API handlers
- `crates/portal-api/src/handlers/results.rs` - Result API handlers
- `crates/portal-api/tests/veto_test.rs` - 18 veto tests
- `crates/portal-api/tests/results_test.rs` - 10 result tests

**Test Results:**
- Veto tests: 18 passing
- Result tests: 10 passing
- Total batch tests: 28 passing
