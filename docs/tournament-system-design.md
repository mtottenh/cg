# Tournament System Architecture Design

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Key Design Decisions](#key-design-decisions)
3. [Domain Model](#domain-model)
4. [Database Schema](#database-schema)
5. [Service Layer Architecture](#service-layer-architecture)
6. [Plugin System Integration](#plugin-system-integration)
7. [API Design](#api-design)
8. [State Machines](#state-machines)
9. [Testing Strategy](#testing-strategy)
10. [Implementation Phases](#implementation-phases)

---

## Executive Summary

This document describes the architecture for implementing a comprehensive tournament system in the gaming portal backend. The design supports:

- **Multiple tournament formats**: Single/Double Elimination, Round Robin, Swiss, Group Stage + Playoffs
- **Both team and individual (PuG) tournaments**
- **Game-specific customization** via the plugin system
- **Multi-stage tournaments** (qualifiers → main event)
- **Flexible registration models** (open, invite-only, qualification-based)
- **Map veto/pick systems** per game
- **Real-time bracket progression**

The design follows existing codebase patterns:
- Strongly-typed IDs (`TournamentId`, `TournamentMatchId`, etc.)
- Repository traits in `portal-domain`, implementations in `portal-db`
- Generic services with dependency injection
- OpenAPI-documented handlers

---

## Key Design Decisions

### 1. Tournament Relationship to Leagues and Seasons

**Decision**: Tournaments are **optionally** linked to leagues and seasons.

```
Tournament
├── league_id: Option<LeagueId>      // NULL for standalone tournaments
├── season_id: Option<LeagueSeasonId> // NULL if not season-specific
└── game_id: GameId                   // Always required
```

**Rationale**:
- **Standalone tournaments**: One-off community events, LAN parties, weekly cups
- **League-integrated tournaments**: Season playoffs, qualification brackets
- **Season-specific tournaments**: End-of-season championships using seasonal rosters

### 2. Team vs. Individual (PuG) Tournaments

**Decision**: Use a **participant abstraction** that handles both models.

```rust
pub enum TournamentParticipantType {
    Team,       // Uses existing LeagueTeamSeason rosters
    Individual, // Individual player registration
    AdHoc,      // Teams formed at registration (pickup games)
}
```

**Participant Resolution**:
- `Team`: Links to `LeagueTeamSeasonId` - uses seasonal roster
- `Individual`: Links to `PlayerId` directly
- `AdHoc`: Creates temporary team structure for the tournament

### 3. Plugin Interface for Custom Bracket Types

**Decision**: Core bracket algorithms are **built-in**, but plugins can **extend** behavior.

```rust
pub trait TournamentPlugin: Send + Sync {
    /// Validate tournament settings for this game
    fn validate_tournament_settings(&self, settings: &Value) -> Result<(), String>;

    /// Custom seeding algorithm (optional)
    fn calculate_seeding(&self, participants: &[ParticipantRating]) -> Vec<Seed>;

    /// Match result validation
    fn validate_match_result(&self, result: &MatchResult) -> Result<(), String>;

    /// Map veto configuration
    fn get_map_veto_formats(&self) -> Vec<MapVetoFormat>;
}
```

**Built-in formats**: Single Elimination, Double Elimination, Round Robin, Swiss
**Plugin extension**: Custom seeding, game-specific validation, hybrid formats

### 4. Partial Match Results in Series

**Decision**: Track **individual games** within a series, supporting partial progress.

```
TournamentMatch (Bo3)
├── Game 1: Team A wins on de_mirage
├── Game 2: Team B wins on de_inferno
└── Game 3: Pending (Team A picks de_ancient)
```

**Schema**: `tournament_match_games` table for per-game results

### 5. Mid-Tournament Withdrawals

**Decision**: Support multiple withdrawal handling strategies per tournament.

```rust
pub enum WithdrawalPolicy {
    Forfeit,           // Opponent advances with walkover
    Reseeding,         // Remaining participants reseeded
    WaitlistPromotion, // Next on waitlist takes slot
    AdminDecision,     // Manual intervention required
}
```

### 6. Sync vs. Async Tournaments

**Decision**: Support both via **scheduling mode**.

```rust
pub enum SchedulingMode {
    Live,          // All matches at set times, real-time progression
    SelfScheduled, // Participants schedule within deadlines
    Hybrid,        // Core brackets live, early rounds self-scheduled
}
```

### 7. Data Denormalization for Bracket Display

**Decision**: Use **materialized bracket views** for efficient client rendering.

Denormalized data:
- Participant names/logos cached at match level
- Running scores/results in bracket node
- Current round/position in tournament

### 8. Timezone Handling for Scheduled Matches

**Decision**: Store all times in **UTC**, display conversion on client.

```rust
pub struct TournamentMatch {
    pub scheduled_at: Option<DateTime<Utc>>,  // UTC timestamp
    pub timezone_hint: Option<String>,         // e.g., "America/New_York" for display
}
```

---

## Domain Model

### Entity Relationship Diagram

```
                                    ┌─────────────────┐
                                    │      Game       │
                                    └────────┬────────┘
                                             │
         ┌───────────────────────────────────┼───────────────────────────────────┐
         │                                   │                                   │
         ▼                                   ▼                                   ▼
┌─────────────────┐              ┌─────────────────────┐              ┌─────────────────┐
│     League      │              │     Tournament      │◄─────────────│     Season      │
└────────┬────────┘              └─────────┬───────────┘              └─────────────────┘
         │                                 │
         │                    ┌────────────┼────────────┐
         │                    │            │            │
         ▼                    ▼            ▼            ▼
┌─────────────────┐  ┌────────────────┐ ┌──────────────────────┐ ┌─────────────────────┐
│   LeagueTeam    │  │TournamentStage │ │TournamentRegistration│ │  TournamentMapPool  │
└────────┬────────┘  └────────┬───────┘ └──────────────────────┘ └─────────────────────┘
         │                    │
         │                    ▼
         │           ┌────────────────────┐
         │           │ TournamentBracket  │
         │           └────────┬───────────┘
         │                    │
         │                    ▼
         │           ┌────────────────────┐
         └──────────►│  TournamentMatch   │
                     └────────┬───────────┘
                              │
                              ▼
                     ┌────────────────────┐
                     │TournamentMatchGame │
                     └────────────────────┘
```

### Core Entities

#### Tournament

The root entity for a competitive event.

```rust
/// A tournament event - can be standalone or part of a league season.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tournament {
    pub id: TournamentId,
    pub game_id: GameId,

    // Optional league/season linkage
    pub league_id: Option<LeagueId>,
    pub season_id: Option<LeagueSeasonId>,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,

    // Format
    pub format: TournamentFormat,
    pub participant_type: TournamentParticipantType,
    pub team_size: Option<i32>,  // For team/adhoc tournaments

    // Capacity
    pub min_participants: i32,
    pub max_participants: i32,

    // Registration
    pub registration_type: RegistrationType,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,

    // Scheduling
    pub scheduling_mode: SchedulingMode,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone_hint: Option<String>,

    // Match settings
    pub default_match_format: MatchFormat,
    pub default_map_veto_format: Option<String>,

    // Prize pool
    pub prize_pool: Option<PrizePool>,

    // Rules & settings
    pub rules_url: Option<String>,
    pub settings: serde_json::Value,  // Game-specific settings

    // Withdrawal handling
    pub withdrawal_policy: WithdrawalPolicy,

    // Status
    pub status: TournamentStatus,

    // Ownership
    pub created_by: UserId,
    pub organization_id: Option<OrganizationId>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

#### TournamentStage

For multi-stage tournaments (e.g., Group Stage → Playoffs).

```rust
/// A stage within a tournament (e.g., Groups, Quarterfinals, Grand Final).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentStage {
    pub id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Identity
    pub name: String,
    pub order: i32,  // 1 = first stage, 2 = second stage, etc.

    // Format
    pub format: StageFormat,
    pub bracket_type: Option<BracketType>,  // For elimination stages

    // Advancement
    pub advancement_count: Option<i32>,  // How many advance to next stage
    pub advancement_rule: AdvancementRule,

    // Match settings (can override tournament defaults)
    pub match_format: Option<MatchFormat>,
    pub map_veto_format: Option<String>,

    // Status
    pub status: StageStatus,

    // Timestamps
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum StageFormat {
    SingleElimination,
    DoubleElimination,
    RoundRobin,
    Swiss,
    GroupStage { groups: i32, teams_per_group: i32 },
}
```

#### TournamentBracket

Represents the bracket structure within a stage.

```rust
/// A bracket within a tournament stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentBracket {
    pub id: TournamentBracketId,
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Identity
    pub name: String,  // e.g., "Winners Bracket", "Losers Bracket", "Group A"
    pub bracket_type: BracketType,

    // Structure
    pub total_rounds: i32,
    pub current_round: i32,

    // For groups
    pub group_number: Option<i32>,

    // Status
    pub status: BracketStatus,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum BracketType {
    Winners,      // Upper bracket in double elimination
    Losers,       // Lower bracket in double elimination
    SingleElim,   // Standard single elimination
    RoundRobin,   // Round robin within group
    Swiss,        // Swiss pairing
    GrandFinal,   // Grand final (double elim)
}
```

#### TournamentRegistration

Participant registration for a tournament.

```rust
/// A registration for tournament participation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentRegistration {
    pub id: TournamentRegistrationId,
    pub tournament_id: TournamentId,

    // Participant identity (one of these will be set)
    pub team_season_id: Option<LeagueTeamSeasonId>,  // For team tournaments
    pub player_id: Option<PlayerId>,                  // For individual tournaments
    pub adhoc_team_id: Option<TournamentAdhocTeamId>, // For pickup tournaments

    // Display info (denormalized for efficiency)
    pub participant_name: String,
    pub participant_logo_url: Option<String>,

    // Registration
    pub registered_by: UserId,
    pub registered_at: DateTime<Utc>,

    // Check-in
    pub checked_in: bool,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_in_by: Option<UserId>,

    // Seeding
    pub seed: Option<i32>,
    pub seed_rating: Option<i32>,  // Used for auto-seeding

    // Status
    pub status: RegistrationStatus,

    // Notes
    pub admin_notes: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

pub enum RegistrationStatus {
    Pending,       // Awaiting approval
    Approved,      // Approved, awaiting check-in
    CheckedIn,     // Checked in, ready to play
    Active,        // Currently competing
    Eliminated,    // Eliminated from tournament
    Disqualified,  // Removed for rule violation
    Withdrawn,     // Voluntarily withdrew
    NoShow,        // Failed to check in
}
```

#### TournamentMatch

A match within a bracket.

```rust
/// A match in a tournament bracket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMatch {
    pub id: TournamentMatchId,
    pub bracket_id: TournamentBracketId,
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Position in bracket
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,  // e.g., "W1-1", "L2-3", "GF"

    // Participants (nullable for future matches)
    pub participant1_registration_id: Option<TournamentRegistrationId>,
    pub participant2_registration_id: Option<TournamentRegistrationId>,

    // Denormalized participant info
    pub participant1_name: Option<String>,
    pub participant1_logo_url: Option<String>,
    pub participant1_seed: Option<i32>,
    pub participant2_name: Option<String>,
    pub participant2_logo_url: Option<String>,
    pub participant2_seed: Option<i32>,

    // Source (where do participants come from?)
    pub participant1_source: Option<MatchParticipantSource>,
    pub participant2_source: Option<MatchParticipantSource>,

    // Match format
    pub match_format: MatchFormat,
    pub maps_required: i32,  // Best of N

    // Scheduling
    pub scheduled_at: Option<DateTime<Utc>>,
    pub schedule_deadline: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    // Results
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: Option<TournamentRegistrationId>,
    pub loser_registration_id: Option<TournamentRegistrationId>,

    // Progression
    pub winner_progresses_to: Option<TournamentMatchId>,
    pub loser_progresses_to: Option<TournamentMatchId>,  // For double elim

    // Status
    pub status: MatchStatus,

    // Disputes
    pub disputed: bool,
    pub dispute_reason: Option<String>,
    pub dispute_resolved_by: Option<UserId>,
    pub dispute_resolution: Option<String>,

    // VOD/Stream
    pub stream_url: Option<String>,
    pub vod_url: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum MatchStatus {
    Pending,       // Waiting for participants
    Ready,         // Both participants set, awaiting scheduling/start
    Scheduled,     // Scheduled, not yet started
    CheckingIn,    // Pre-match check-in phase
    PickBan,       // Map veto in progress
    InProgress,    // Match is being played
    AwaitingResult,// Waiting for result submission
    Completed,     // Match finished normally
    Cancelled,     // Match cancelled (bye, etc.)
    Forfeit,       // One participant forfeited
    Disputed,      // Result under dispute
}

pub struct MatchParticipantSource {
    pub source_type: SourceType,
    pub source_match_id: Option<TournamentMatchId>,
    pub source_bracket_id: Option<TournamentBracketId>,
    pub position: SourcePosition,
}

pub enum SourceType {
    Seed,          // Direct seed from registration
    MatchWinner,   // Winner of another match
    MatchLoser,    // Loser of another match (double elim)
    GroupAdvance,  // Advanced from group stage
}

pub enum SourcePosition {
    Winner,
    Loser,
    Position(i32),  // For group stage (1st, 2nd, etc.)
}
```

#### TournamentMatchGame

Individual games within a series.

```rust
/// A single game within a tournament match (for Bo3, Bo5, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMatchGame {
    pub id: TournamentMatchGameId,
    pub match_id: TournamentMatchId,

    // Game number in series
    pub game_number: i32,

    // Map selection
    pub map_id: Option<String>,
    pub map_picked_by: Option<TournamentRegistrationId>,
    pub side_selection_by: Option<TournamentRegistrationId>,

    // Results
    pub participant1_score: Option<i32>,
    pub participant2_score: Option<i32>,
    pub winner_registration_id: Option<TournamentRegistrationId>,

    // Timing
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,

    // Status
    pub status: GameStatus,

    // Game-specific data
    pub game_data: serde_json::Value,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum GameStatus {
    Pending,
    MapVeto,
    InProgress,
    Completed,
    Cancelled,
}
```

#### TournamentMapPool

Tournament-specific map pool configuration.

```rust
/// Map pool configuration for a tournament.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMapPool {
    pub id: TournamentMapPoolId,
    pub tournament_id: TournamentId,

    // Can be stage-specific
    pub stage_id: Option<TournamentStageId>,

    // Maps
    pub maps: Vec<String>,

    // Veto format
    pub veto_format_id: Option<String>,

    // Created/Updated
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Enumerations

```rust
pub enum TournamentFormat {
    SingleElimination,
    DoubleElimination,
    RoundRobin,
    Swiss,
    GroupsAndPlayoffs {
        groups: i32,
        teams_per_group: i32,
        advance_per_group: i32,
        playoff_format: Box<TournamentFormat>,
    },
    Custom(String),
}

pub enum TournamentParticipantType {
    Team,        // Existing league teams
    Individual,  // Solo players
    AdHoc,       // Teams formed at registration
}

pub enum RegistrationType {
    Open,           // Anyone can register
    InviteOnly,     // Only invited participants
    Qualification,  // Must qualify through another tournament
    Approval,       // Admin approval required
}

pub enum SchedulingMode {
    Live,          // Fixed schedule, real-time
    SelfScheduled, // Participants arrange matches
    Hybrid,        // Mix of both
}

pub enum TournamentStatus {
    Draft,           // Being configured
    Published,       // Open for viewing
    Registration,    // Accepting registrations
    CheckIn,         // Pre-tournament check-in
    Seeding,         // Generating seeding
    InProgress,      // Tournament running
    Completed,       // Tournament finished
    Cancelled,       // Tournament cancelled
}

pub enum AdvancementRule {
    TopN(i32),           // Top N advance
    AboveThreshold(i32), // Score above threshold
    WinPercent(f32),     // Win percentage above threshold
    Custom,              // Plugin-defined
}

pub enum WithdrawalPolicy {
    Forfeit,
    Reseeding,
    WaitlistPromotion,
    AdminDecision,
}
```

---

## Database Schema

### Migration: `NNNN_create_tournaments.sql`

```sql
-- Migration: Create Tournament System
-- Description: Core tournament infrastructure

-- =============================================================================
-- 1. TOURNAMENTS
-- =============================================================================

CREATE TABLE tournaments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id),

    -- Optional league/season linkage
    league_id UUID REFERENCES leagues(id) ON DELETE SET NULL,
    season_id UUID REFERENCES league_seasons(id) ON DELETE SET NULL,

    -- Identity
    name VARCHAR(128) NOT NULL,
    slug VARCHAR(128) NOT NULL,
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),

    -- Format
    format VARCHAR(64) NOT NULL,                    -- 'single_elimination', 'double_elimination', etc.
    format_settings JSONB NOT NULL DEFAULT '{}',   -- Format-specific config
    participant_type VARCHAR(32) NOT NULL DEFAULT 'team',
    team_size INTEGER,

    -- Capacity
    min_participants INTEGER NOT NULL DEFAULT 2,
    max_participants INTEGER NOT NULL DEFAULT 64,

    -- Registration
    registration_type VARCHAR(32) NOT NULL DEFAULT 'open',
    registration_start TIMESTAMPTZ,
    registration_end TIMESTAMPTZ,
    check_in_start TIMESTAMPTZ,
    check_in_end TIMESTAMPTZ,
    check_in_required BOOLEAN NOT NULL DEFAULT false,

    -- Scheduling
    scheduling_mode VARCHAR(32) NOT NULL DEFAULT 'live',
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    timezone_hint VARCHAR(64),

    -- Match settings
    default_match_format VARCHAR(16) NOT NULL DEFAULT 'bo1',
    default_map_veto_format VARCHAR(64),

    -- Prize pool (JSONB for flexibility)
    prize_pool JSONB,

    -- Rules
    rules_url VARCHAR(512),
    settings JSONB NOT NULL DEFAULT '{}',

    -- Policies
    withdrawal_policy VARCHAR(32) NOT NULL DEFAULT 'forfeit',

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'draft',

    -- Ownership
    created_by UUID NOT NULL REFERENCES users(id),
    organization_id UUID,  -- Future: REFERENCES organizations(id)

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT tournaments_slug_unique UNIQUE (slug),
    CONSTRAINT tournaments_check_status CHECK (status IN (
        'draft', 'published', 'registration', 'check_in',
        'seeding', 'in_progress', 'completed', 'cancelled'
    )),
    CONSTRAINT tournaments_check_format CHECK (format IN (
        'single_elimination', 'double_elimination', 'round_robin',
        'swiss', 'groups_and_playoffs', 'custom'
    )),
    CONSTRAINT tournaments_check_participant_type CHECK (participant_type IN (
        'team', 'individual', 'adhoc'
    )),
    CONSTRAINT tournaments_check_registration_type CHECK (registration_type IN (
        'open', 'invite_only', 'qualification', 'approval'
    )),
    CONSTRAINT tournaments_check_scheduling_mode CHECK (scheduling_mode IN (
        'live', 'self_scheduled', 'hybrid'
    )),
    CONSTRAINT tournaments_check_withdrawal_policy CHECK (withdrawal_policy IN (
        'forfeit', 'reseeding', 'waitlist_promotion', 'admin_decision'
    )),
    CONSTRAINT tournaments_check_participants CHECK (
        min_participants >= 2 AND
        max_participants >= min_participants
    ),
    CONSTRAINT tournaments_check_dates CHECK (
        (registration_start IS NULL OR registration_end IS NULL OR registration_start < registration_end) AND
        (registration_end IS NULL OR starts_at IS NULL OR registration_end <= starts_at)
    )
);

CREATE INDEX idx_tournaments_game ON tournaments(game_id);
CREATE INDEX idx_tournaments_league ON tournaments(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX idx_tournaments_season ON tournaments(season_id) WHERE season_id IS NOT NULL;
CREATE INDEX idx_tournaments_status ON tournaments(status);
CREATE INDEX idx_tournaments_starts_at ON tournaments(starts_at) WHERE starts_at IS NOT NULL;
CREATE INDEX idx_tournaments_slug ON tournaments(slug);

CREATE TRIGGER tournaments_updated_at
    BEFORE UPDATE ON tournaments
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournaments IS 'Tournament events with configurable formats and registration';

-- =============================================================================
-- 2. TOURNAMENT STAGES
-- =============================================================================

CREATE TABLE tournament_stages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Identity
    name VARCHAR(64) NOT NULL,
    stage_order INTEGER NOT NULL,

    -- Format
    format VARCHAR(64) NOT NULL,
    format_settings JSONB NOT NULL DEFAULT '{}',

    -- Advancement
    advancement_count INTEGER,
    advancement_rule VARCHAR(32) NOT NULL DEFAULT 'top_n',

    -- Match settings (override tournament defaults)
    match_format VARCHAR(16),
    map_veto_format VARCHAR(64),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timing
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_stages_unique_order UNIQUE (tournament_id, stage_order),
    CONSTRAINT tournament_stages_check_status CHECK (status IN (
        'pending', 'in_progress', 'completed', 'cancelled'
    )),
    CONSTRAINT tournament_stages_check_format CHECK (format IN (
        'single_elimination', 'double_elimination', 'round_robin',
        'swiss', 'group_stage'
    )),
    CONSTRAINT tournament_stages_check_advancement CHECK (advancement_rule IN (
        'top_n', 'above_threshold', 'win_percent', 'custom'
    ))
);

CREATE INDEX idx_tournament_stages_tournament ON tournament_stages(tournament_id);
CREATE INDEX idx_tournament_stages_order ON tournament_stages(tournament_id, stage_order);

CREATE TRIGGER tournament_stages_updated_at
    BEFORE UPDATE ON tournament_stages
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_stages IS 'Stages within a multi-stage tournament';

-- =============================================================================
-- 3. TOURNAMENT BRACKETS
-- =============================================================================

CREATE TABLE tournament_brackets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stage_id UUID NOT NULL REFERENCES tournament_stages(id) ON DELETE CASCADE,
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Identity
    name VARCHAR(64) NOT NULL,
    bracket_type VARCHAR(32) NOT NULL,

    -- Structure
    total_rounds INTEGER NOT NULL DEFAULT 1,
    current_round INTEGER NOT NULL DEFAULT 0,

    -- For groups
    group_number INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_brackets_check_type CHECK (bracket_type IN (
        'winners', 'losers', 'single_elim', 'round_robin', 'swiss', 'grand_final'
    )),
    CONSTRAINT tournament_brackets_check_status CHECK (status IN (
        'pending', 'in_progress', 'completed', 'cancelled'
    ))
);

CREATE INDEX idx_tournament_brackets_stage ON tournament_brackets(stage_id);
CREATE INDEX idx_tournament_brackets_tournament ON tournament_brackets(tournament_id);

CREATE TRIGGER tournament_brackets_updated_at
    BEFORE UPDATE ON tournament_brackets
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_brackets IS 'Brackets within tournament stages';

-- =============================================================================
-- 4. TOURNAMENT REGISTRATIONS
-- =============================================================================

CREATE TABLE tournament_registrations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Participant identity (exactly one should be set based on tournament type)
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    adhoc_team_id UUID,  -- Future: REFERENCES tournament_adhoc_teams(id)

    -- Denormalized display info
    participant_name VARCHAR(128) NOT NULL,
    participant_logo_url VARCHAR(512),

    -- Registration
    registered_by UUID NOT NULL REFERENCES users(id),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Check-in
    checked_in BOOLEAN NOT NULL DEFAULT false,
    checked_in_at TIMESTAMPTZ,
    checked_in_by UUID REFERENCES users(id),

    -- Seeding
    seed INTEGER,
    seed_rating INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Admin
    admin_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    withdrawn_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT tournament_registrations_one_participant CHECK (
        (team_season_id IS NOT NULL)::int +
        (player_id IS NOT NULL)::int +
        (adhoc_team_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT tournament_registrations_unique_team UNIQUE (tournament_id, team_season_id),
    CONSTRAINT tournament_registrations_unique_player UNIQUE (tournament_id, player_id),
    CONSTRAINT tournament_registrations_unique_adhoc UNIQUE (tournament_id, adhoc_team_id),
    CONSTRAINT tournament_registrations_check_status CHECK (status IN (
        'pending', 'approved', 'checked_in', 'active',
        'eliminated', 'disqualified', 'withdrawn', 'no_show'
    ))
);

CREATE INDEX idx_tournament_registrations_tournament ON tournament_registrations(tournament_id);
CREATE INDEX idx_tournament_registrations_team_season ON tournament_registrations(team_season_id)
    WHERE team_season_id IS NOT NULL;
CREATE INDEX idx_tournament_registrations_player ON tournament_registrations(player_id)
    WHERE player_id IS NOT NULL;
CREATE INDEX idx_tournament_registrations_status ON tournament_registrations(tournament_id, status);
CREATE INDEX idx_tournament_registrations_seed ON tournament_registrations(tournament_id, seed)
    WHERE seed IS NOT NULL;

CREATE TRIGGER tournament_registrations_updated_at
    BEFORE UPDATE ON tournament_registrations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_registrations IS 'Participant registrations for tournaments';

-- =============================================================================
-- 5. TOURNAMENT MATCHES
-- =============================================================================

CREATE TABLE tournament_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL REFERENCES tournament_brackets(id) ON DELETE CASCADE,
    stage_id UUID NOT NULL REFERENCES tournament_stages(id) ON DELETE CASCADE,
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Position in bracket
    round INTEGER NOT NULL,
    match_number INTEGER NOT NULL,
    bracket_position VARCHAR(16) NOT NULL,  -- e.g., "W1-1", "L2-3", "GF"

    -- Participants
    participant1_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,
    participant2_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,

    -- Denormalized participant info (cached for bracket display)
    participant1_name VARCHAR(128),
    participant1_logo_url VARCHAR(512),
    participant1_seed INTEGER,
    participant2_name VARCHAR(128),
    participant2_logo_url VARCHAR(512),
    participant2_seed INTEGER,

    -- Source tracking (JSONB for flexibility)
    participant1_source JSONB,
    participant2_source JSONB,

    -- Match format
    match_format VARCHAR(16) NOT NULL DEFAULT 'bo1',
    maps_required INTEGER NOT NULL DEFAULT 1,

    -- Scheduling
    scheduled_at TIMESTAMPTZ,
    schedule_deadline TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Results
    participant1_score INTEGER NOT NULL DEFAULT 0,
    participant2_score INTEGER NOT NULL DEFAULT 0,
    winner_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,
    loser_registration_id UUID REFERENCES tournament_registrations(id) ON DELETE SET NULL,

    -- Progression
    winner_progresses_to UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    loser_progresses_to UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Disputes
    disputed BOOLEAN NOT NULL DEFAULT false,
    dispute_reason TEXT,
    dispute_resolved_by UUID REFERENCES users(id),
    dispute_resolution TEXT,
    dispute_resolved_at TIMESTAMPTZ,

    -- VOD/Stream
    stream_url VARCHAR(512),
    vod_url VARCHAR(512),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_matches_unique_position UNIQUE (bracket_id, bracket_position),
    CONSTRAINT tournament_matches_check_status CHECK (status IN (
        'pending', 'ready', 'scheduled', 'checking_in', 'pick_ban',
        'in_progress', 'awaiting_result', 'completed', 'cancelled', 'forfeit', 'disputed'
    )),
    CONSTRAINT tournament_matches_check_format CHECK (match_format IN (
        'bo1', 'bo3', 'bo5', 'bo7'
    ))
);

CREATE INDEX idx_tournament_matches_bracket ON tournament_matches(bracket_id);
CREATE INDEX idx_tournament_matches_stage ON tournament_matches(stage_id);
CREATE INDEX idx_tournament_matches_tournament ON tournament_matches(tournament_id);
CREATE INDEX idx_tournament_matches_status ON tournament_matches(status);
CREATE INDEX idx_tournament_matches_scheduled ON tournament_matches(scheduled_at)
    WHERE scheduled_at IS NOT NULL;
CREATE INDEX idx_tournament_matches_participant1 ON tournament_matches(participant1_registration_id)
    WHERE participant1_registration_id IS NOT NULL;
CREATE INDEX idx_tournament_matches_participant2 ON tournament_matches(participant2_registration_id)
    WHERE participant2_registration_id IS NOT NULL;

CREATE TRIGGER tournament_matches_updated_at
    BEFORE UPDATE ON tournament_matches
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_matches IS 'Individual matches within tournament brackets';

-- =============================================================================
-- 6. TOURNAMENT MATCH GAMES
-- =============================================================================

CREATE TABLE tournament_match_games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Game number in series
    game_number INTEGER NOT NULL,

    -- Map selection
    map_id VARCHAR(64),
    map_picked_by UUID REFERENCES tournament_registrations(id),
    side_selection_by UUID REFERENCES tournament_registrations(id),

    -- Results
    participant1_score INTEGER,
    participant2_score INTEGER,
    winner_registration_id UUID REFERENCES tournament_registrations(id),

    -- Timing
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    duration_seconds INTEGER,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Game-specific data
    game_data JSONB NOT NULL DEFAULT '{}',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_match_games_unique UNIQUE (match_id, game_number),
    CONSTRAINT tournament_match_games_check_status CHECK (status IN (
        'pending', 'map_veto', 'in_progress', 'completed', 'cancelled'
    )),
    CONSTRAINT tournament_match_games_check_number CHECK (game_number >= 1)
);

CREATE INDEX idx_tournament_match_games_match ON tournament_match_games(match_id);

CREATE TRIGGER tournament_match_games_updated_at
    BEFORE UPDATE ON tournament_match_games
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_match_games IS 'Individual games within a tournament match series';

-- =============================================================================
-- 7. TOURNAMENT MAP POOLS
-- =============================================================================

CREATE TABLE tournament_map_pools (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    stage_id UUID REFERENCES tournament_stages(id) ON DELETE CASCADE,

    -- Maps (array of map IDs from game plugin)
    maps TEXT[] NOT NULL,

    -- Veto format
    veto_format_id VARCHAR(64),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_map_pools_unique UNIQUE (tournament_id, stage_id)
);

CREATE INDEX idx_tournament_map_pools_tournament ON tournament_map_pools(tournament_id);

CREATE TRIGGER tournament_map_pools_updated_at
    BEFORE UPDATE ON tournament_map_pools
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_map_pools IS 'Map pool configurations for tournaments';

-- =============================================================================
-- 8. TOURNAMENT MAP VETO LOG
-- =============================================================================

CREATE TABLE tournament_map_veto_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    game_number INTEGER,  -- NULL for whole-match veto

    -- Action
    action_number INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,  -- 'ban', 'pick', 'decider'
    map_id VARCHAR(64) NOT NULL,
    performed_by UUID REFERENCES tournament_registrations(id),

    -- Timestamps
    performed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_map_veto_logs_unique UNIQUE (match_id, game_number, action_number),
    CONSTRAINT tournament_map_veto_logs_check_action CHECK (action_type IN (
        'ban', 'pick', 'decider'
    ))
);

CREATE INDEX idx_tournament_map_veto_logs_match ON tournament_map_veto_logs(match_id);

COMMENT ON TABLE tournament_map_veto_logs IS 'Log of map veto actions in matches';

-- =============================================================================
-- 9. TOURNAMENT STANDINGS (For round robin/swiss)
-- =============================================================================

CREATE TABLE tournament_standings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL REFERENCES tournament_brackets(id) ON DELETE CASCADE,
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- Position
    position INTEGER NOT NULL,

    -- Stats
    matches_played INTEGER NOT NULL DEFAULT 0,
    matches_won INTEGER NOT NULL DEFAULT 0,
    matches_lost INTEGER NOT NULL DEFAULT 0,
    matches_drawn INTEGER NOT NULL DEFAULT 0,

    -- Tiebreakers
    game_wins INTEGER NOT NULL DEFAULT 0,
    game_losses INTEGER NOT NULL DEFAULT 0,
    game_differential INTEGER NOT NULL DEFAULT 0,
    buchholz_score DECIMAL(10,2),  -- For Swiss
    opponent_match_wins DECIMAL(10,2),  -- OMW%

    -- Points (for round robin)
    points INTEGER NOT NULL DEFAULT 0,

    -- Timestamps
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT tournament_standings_unique UNIQUE (bracket_id, registration_id)
);

CREATE INDEX idx_tournament_standings_bracket ON tournament_standings(bracket_id);
CREATE INDEX idx_tournament_standings_position ON tournament_standings(bracket_id, position);

CREATE TRIGGER tournament_standings_updated_at
    BEFORE UPDATE ON tournament_standings
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE tournament_standings IS 'Current standings for round robin/swiss brackets';

-- =============================================================================
-- 10. PERMISSIONS
-- =============================================================================

INSERT INTO permissions (id, name, display_name, description, category)
VALUES
    (gen_random_uuid(), 'tournament.create', 'Create Tournaments', 'Create new tournaments', 'tournament'),
    (gen_random_uuid(), 'tournament.manage', 'Manage Tournaments', 'Full tournament administration', 'tournament'),
    (gen_random_uuid(), 'tournament.brackets.manage', 'Manage Brackets', 'Edit brackets and matches', 'tournament'),
    (gen_random_uuid(), 'tournament.registrations.manage', 'Manage Registrations', 'Approve/deny registrations', 'tournament'),
    (gen_random_uuid(), 'tournament.results.submit', 'Submit Results', 'Submit match results', 'tournament'),
    (gen_random_uuid(), 'tournament.disputes.resolve', 'Resolve Disputes', 'Handle match disputes', 'tournament')
ON CONFLICT (name) DO NOTHING;

COMMENT ON TABLE tournament_standings IS 'RBAC permissions for tournament management';

-- =============================================================================
-- 11. VIEWS FOR BRACKET DISPLAY
-- =============================================================================

-- Materialized view for efficient bracket rendering
CREATE MATERIALIZED VIEW mv_tournament_bracket_display AS
SELECT
    tm.id AS match_id,
    tm.bracket_id,
    tm.stage_id,
    tm.tournament_id,
    tm.round,
    tm.match_number,
    tm.bracket_position,
    tm.status,
    tm.scheduled_at,
    tm.participant1_name,
    tm.participant1_logo_url,
    tm.participant1_seed,
    tm.participant1_score,
    tm.participant2_name,
    tm.participant2_logo_url,
    tm.participant2_seed,
    tm.participant2_score,
    tm.winner_registration_id,
    tm.match_format,
    tb.bracket_type,
    tb.name AS bracket_name,
    ts.name AS stage_name,
    t.name AS tournament_name
FROM tournament_matches tm
JOIN tournament_brackets tb ON tb.id = tm.bracket_id
JOIN tournament_stages ts ON ts.id = tm.stage_id
JOIN tournaments t ON t.id = tm.tournament_id;

CREATE UNIQUE INDEX idx_mv_bracket_display_match ON mv_tournament_bracket_display(match_id);
CREATE INDEX idx_mv_bracket_display_bracket ON mv_tournament_bracket_display(bracket_id);
CREATE INDEX idx_mv_bracket_display_tournament ON mv_tournament_bracket_display(tournament_id);

COMMENT ON MATERIALIZED VIEW mv_tournament_bracket_display IS
    'Pre-computed bracket display data for efficient client rendering';

-- Function to refresh the materialized view
CREATE OR REPLACE FUNCTION refresh_bracket_display()
RETURNS TRIGGER AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_tournament_bracket_display;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Note: In production, refresh should be done periodically or via application logic
-- to avoid performance issues with triggers on every change
```

---

## Service Layer Architecture

### Service Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           PORTAL-API LAYER                                   │
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │ Handlers: TournamentHandler, BracketHandler, MatchHandler, etc.         ││
│  └───────────────────────────────────┬─────────────────────────────────────┘│
└──────────────────────────────────────┼──────────────────────────────────────┘
                                       │
┌──────────────────────────────────────┼──────────────────────────────────────┐
│                         PORTAL-DOMAIN LAYER                                  │
│  ┌───────────────────────────────────▼─────────────────────────────────────┐│
│  │                        SERVICE LAYER                                     ││
│  │  ┌─────────────────┐  ┌───────────────────┐  ┌────────────────────────┐ ││
│  │  │TournamentService│  │ BracketService    │  │ TournamentMatchService │ ││
│  │  └────────┬────────┘  └─────────┬─────────┘  └───────────┬────────────┘ ││
│  │           │                     │                        │              ││
│  │  ┌────────▼─────────────────────▼────────────────────────▼────────────┐ ││
│  │  │                    BRACKET ALGORITHMS                               │ ││
│  │  │  SingleEliminationGenerator, DoubleEliminationGenerator,           │ ││
│  │  │  RoundRobinGenerator, SwissGenerator, etc.                         │ ││
│  │  └────────────────────────────────────────────────────────────────────┘ ││
│  └─────────────────────────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────────────────────────┐│
│  │                      REPOSITORY TRAITS                                   ││
│  │  TournamentRepository, TournamentStageRepository,                        ││
│  │  TournamentBracketRepository, TournamentMatchRepository, etc.           ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
┌──────────────────────────────────────┼──────────────────────────────────────┐
│                          PORTAL-DB LAYER                                     │
│  ┌───────────────────────────────────▼─────────────────────────────────────┐│
│  │                      REPOSITORY ADAPTERS                                 ││
│  │  PgTournamentRepository, PgTournamentStageRepository,                    ││
│  │  PgTournamentBracketRepository, PgTournamentMatchRepository, etc.       ││
│  └─────────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────────┘
```

### Core Services

#### TournamentService

```rust
pub struct TournamentService<TR, TSR, TRegR, GR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TRegR: TournamentRegistrationRepository,
    GR: GameRepository,
{
    tournament_repo: Arc<TR>,
    stage_repo: Arc<TSR>,
    registration_repo: Arc<TRegR>,
    game_repo: Arc<GR>,
}

impl<TR, TSR, TRegR, GR> TournamentService<TR, TSR, TRegR, GR>
where
    TR: TournamentRepository,
    TSR: TournamentStageRepository,
    TRegR: TournamentRegistrationRepository,
    GR: GameRepository,
{
    // CRUD
    pub async fn create_tournament(
        &self,
        cmd: CreateTournamentCommand,
        created_by: UserId,
    ) -> Result<Tournament, DomainError>;

    pub async fn get_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn update_tournament(
        &self,
        id: TournamentId,
        cmd: UpdateTournamentCommand,
    ) -> Result<Tournament, DomainError>;

    pub async fn list_tournaments(
        &self,
        filters: TournamentFilters,
        pagination: Pagination,
    ) -> Result<(Vec<Tournament>, i64), DomainError>;

    // Lifecycle
    pub async fn publish(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn open_registration(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn close_registration(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn start_check_in(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn finalize_check_in(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn start_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn complete_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    pub async fn cancel_tournament(&self, id: TournamentId) -> Result<Tournament, DomainError>;

    // Validation
    fn validate_tournament_settings(
        &self,
        tournament: &Tournament,
        plugin: &dyn GamePlugin,
    ) -> Result<(), DomainError>;
}
```

#### TournamentRegistrationService

```rust
pub struct TournamentRegistrationService<TRegR, TR, LTSR, PR>
where
    TRegR: TournamentRegistrationRepository,
    TR: TournamentRepository,
    LTSR: LeagueTeamSeasonRepository,
    PR: PlayerRepository,
{
    registration_repo: Arc<TRegR>,
    tournament_repo: Arc<TR>,
    team_season_repo: Arc<LTSR>,
    player_repo: Arc<PR>,
}

impl<TRegR, TR, LTSR, PR> TournamentRegistrationService<TRegR, TR, LTSR, PR>
where
    TRegR: TournamentRegistrationRepository,
    TR: TournamentRepository,
    LTSR: LeagueTeamSeasonRepository,
    PR: PlayerRepository,
{
    // Registration
    pub async fn register_team(
        &self,
        tournament_id: TournamentId,
        team_season_id: LeagueTeamSeasonId,
        registered_by: UserId,
    ) -> Result<TournamentRegistration, DomainError>;

    pub async fn register_player(
        &self,
        tournament_id: TournamentId,
        player_id: PlayerId,
        registered_by: UserId,
    ) -> Result<TournamentRegistration, DomainError>;

    pub async fn withdraw(
        &self,
        registration_id: TournamentRegistrationId,
        withdrawn_by: UserId,
    ) -> Result<TournamentRegistration, DomainError>;

    // Admin
    pub async fn approve_registration(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<TournamentRegistration, DomainError>;

    pub async fn reject_registration(
        &self,
        registration_id: TournamentRegistrationId,
        reason: String,
    ) -> Result<TournamentRegistration, DomainError>;

    pub async fn disqualify(
        &self,
        registration_id: TournamentRegistrationId,
        reason: String,
    ) -> Result<TournamentRegistration, DomainError>;

    // Check-in
    pub async fn check_in(
        &self,
        registration_id: TournamentRegistrationId,
        checked_in_by: UserId,
    ) -> Result<TournamentRegistration, DomainError>;

    pub async fn process_no_shows(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentRegistration>, DomainError>;

    // Queries
    pub async fn list_registrations(
        &self,
        tournament_id: TournamentId,
        filters: RegistrationFilters,
    ) -> Result<Vec<TournamentRegistration>, DomainError>;

    pub async fn get_checked_in_count(&self, tournament_id: TournamentId) -> Result<i64, DomainError>;
}
```

#### TournamentBracketService

```rust
pub struct TournamentBracketService<TBR, TSR, TMR, TRegR>
where
    TBR: TournamentBracketRepository,
    TSR: TournamentStageRepository,
    TMR: TournamentMatchRepository,
    TRegR: TournamentRegistrationRepository,
{
    bracket_repo: Arc<TBR>,
    stage_repo: Arc<TSR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRegR>,
}

impl<TBR, TSR, TMR, TRegR> TournamentBracketService<TBR, TSR, TMR, TRegR>
where
    TBR: TournamentBracketRepository,
    TSR: TournamentStageRepository,
    TMR: TournamentMatchRepository,
    TRegR: TournamentRegistrationRepository,
{
    // Generation
    pub async fn generate_bracket(
        &self,
        stage_id: TournamentStageId,
        seeded_participants: Vec<SeededParticipant>,
    ) -> Result<TournamentBracket, DomainError>;

    pub async fn regenerate_bracket(
        &self,
        bracket_id: TournamentBracketId,
        seeded_participants: Vec<SeededParticipant>,
    ) -> Result<TournamentBracket, DomainError>;

    // Seeding
    pub async fn auto_seed(
        &self,
        tournament_id: TournamentId,
        algorithm: SeedingAlgorithm,
    ) -> Result<Vec<SeededParticipant>, DomainError>;

    pub async fn manual_seed(
        &self,
        tournament_id: TournamentId,
        seeds: Vec<(TournamentRegistrationId, i32)>,
    ) -> Result<(), DomainError>;

    // Queries
    pub async fn get_bracket_with_matches(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<BracketWithMatches, DomainError>;

    pub async fn get_bracket_display_data(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<BracketDisplayData>, DomainError>;
}

pub enum SeedingAlgorithm {
    Random,
    Rating,
    SeasonRank,
    Manual,
    PluginProvided(String),
}
```

#### TournamentMatchService

```rust
pub struct TournamentMatchService<TMR, TMGR, TBR, TRegR, MVLR>
where
    TMR: TournamentMatchRepository,
    TMGR: TournamentMatchGameRepository,
    TBR: TournamentBracketRepository,
    TRegR: TournamentRegistrationRepository,
    MVLR: TournamentMapVetoLogRepository,
{
    match_repo: Arc<TMR>,
    game_repo: Arc<TMGR>,
    bracket_repo: Arc<TBR>,
    registration_repo: Arc<TRegR>,
    veto_log_repo: Arc<MVLR>,
}

impl<TMR, TMGR, TBR, TRegR, MVLR> TournamentMatchService<TMR, TMGR, TBR, TRegR, MVLR>
where
    TMR: TournamentMatchRepository,
    TMGR: TournamentMatchGameRepository,
    TBR: TournamentBracketRepository,
    TRegR: TournamentRegistrationRepository,
    MVLR: TournamentMapVetoLogRepository,
{
    // Scheduling
    pub async fn schedule_match(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn reschedule_match(
        &self,
        match_id: TournamentMatchId,
        new_time: DateTime<Utc>,
    ) -> Result<TournamentMatch, DomainError>;

    // Match flow
    pub async fn start_check_in(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn player_check_in(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn start_map_veto(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn perform_veto_action(
        &self,
        match_id: TournamentMatchId,
        action: VetoAction,
        performed_by: TournamentRegistrationId,
    ) -> Result<VetoState, DomainError>;

    pub async fn start_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<TournamentMatch, DomainError>;

    // Results
    pub async fn submit_game_result(
        &self,
        match_id: TournamentMatchId,
        game_number: i32,
        result: GameResult,
        submitted_by: UserId,
    ) -> Result<TournamentMatchGame, DomainError>;

    pub async fn submit_match_result(
        &self,
        match_id: TournamentMatchId,
        result: MatchResult,
        submitted_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn confirm_result(
        &self,
        match_id: TournamentMatchId,
        confirmed_by: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError>;

    // Admin
    pub async fn override_result(
        &self,
        match_id: TournamentMatchId,
        result: MatchResult,
        override_by: UserId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn report_forfeit(
        &self,
        match_id: TournamentMatchId,
        forfeiting_registration_id: TournamentRegistrationId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    // Disputes
    pub async fn raise_dispute(
        &self,
        match_id: TournamentMatchId,
        raised_by: TournamentRegistrationId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError>;

    pub async fn resolve_dispute(
        &self,
        match_id: TournamentMatchId,
        resolution: DisputeResolution,
        resolved_by: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    // Progression
    pub async fn process_match_completion(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<ProgressionResult, DomainError>;
}

pub struct VetoAction {
    pub action_type: VetoActionType,
    pub map_id: String,
}

pub struct VetoState {
    pub remaining_maps: Vec<String>,
    pub banned_maps: Vec<(String, TournamentRegistrationId)>,
    pub picked_maps: Vec<(String, TournamentRegistrationId)>,
    pub next_action: Option<VetoActionExpected>,
    pub is_complete: bool,
}

pub struct ProgressionResult {
    pub winner_advanced_to: Option<TournamentMatchId>,
    pub loser_advanced_to: Option<TournamentMatchId>,
    pub bracket_complete: bool,
    pub stage_complete: bool,
    pub tournament_complete: bool,
}
```

### Bracket Generation Algorithms

```rust
/// Trait for bracket generation algorithms.
pub trait BracketGenerator: Send + Sync {
    /// Generate matches for the bracket.
    fn generate_matches(
        &self,
        participants: &[SeededParticipant],
        settings: &BracketSettings,
    ) -> Result<Vec<GeneratedMatch>, BracketError>;

    /// Get the number of rounds for N participants.
    fn calculate_rounds(&self, participant_count: usize) -> usize;

    /// Handle bye assignment.
    fn assign_byes(&self, participant_count: usize) -> Vec<ByeAssignment>;
}

pub struct SingleEliminationGenerator;

impl BracketGenerator for SingleEliminationGenerator {
    fn generate_matches(
        &self,
        participants: &[SeededParticipant],
        settings: &BracketSettings,
    ) -> Result<Vec<GeneratedMatch>, BracketError> {
        let bracket_size = participants.len().next_power_of_two();
        let rounds = (bracket_size as f64).log2() as usize;
        let byes = bracket_size - participants.len();

        let mut matches = Vec::new();
        let mut match_number = 1;

        // Generate first round with byes
        for round in 1..=rounds {
            let matches_in_round = bracket_size >> round;
            for i in 0..matches_in_round {
                matches.push(GeneratedMatch {
                    round,
                    match_number,
                    bracket_position: format!("R{}-{}", round, i + 1),
                    participant1_seed: None,  // Filled during seeding
                    participant2_seed: None,
                    winner_progresses_to: if round < rounds {
                        Some(format!("R{}-{}", round + 1, (i / 2) + 1))
                    } else {
                        None
                    },
                    loser_progresses_to: None,
                });
                match_number += 1;
            }
        }

        // Apply seeding with standard bracket positioning
        self.apply_seeding(&mut matches, participants, byes)?;

        Ok(matches)
    }

    fn calculate_rounds(&self, participant_count: usize) -> usize {
        (participant_count.next_power_of_two() as f64).log2() as usize
    }

    fn assign_byes(&self, participant_count: usize) -> Vec<ByeAssignment> {
        let bracket_size = participant_count.next_power_of_two();
        let byes_needed = bracket_size - participant_count;

        // Top seeds get byes
        (1..=byes_needed)
            .map(|seed| ByeAssignment { seed: seed as i32 })
            .collect()
    }
}

pub struct DoubleEliminationGenerator;
pub struct RoundRobinGenerator;
pub struct SwissGenerator;

impl BracketGenerator for SwissGenerator {
    fn generate_matches(
        &self,
        participants: &[SeededParticipant],
        settings: &BracketSettings,
    ) -> Result<Vec<GeneratedMatch>, BracketError> {
        // Swiss generates round-by-round based on standings
        // Initial round pairs by seed
        let mut matches = Vec::new();
        let participant_count = participants.len();

        // Calculate rounds (typically ceil(log2(n)) + 1)
        let rounds = ((participant_count as f64).log2().ceil() as usize) + 1;

        // Generate first round pairings
        for i in 0..(participant_count / 2) {
            matches.push(GeneratedMatch {
                round: 1,
                match_number: (i + 1) as i32,
                bracket_position: format!("SW1-{}", i + 1),
                participant1_seed: Some((i * 2 + 1) as i32),
                participant2_seed: Some((i * 2 + 2) as i32),
                winner_progresses_to: None,  // Swiss pairs dynamically
                loser_progresses_to: None,
            });
        }

        // Subsequent rounds generated after previous round completes
        // using Buchholz scoring or similar tiebreakers

        Ok(matches)
    }

    fn calculate_rounds(&self, participant_count: usize) -> usize {
        ((participant_count as f64).log2().ceil() as usize) + 1
    }

    fn assign_byes(&self, participant_count: usize) -> Vec<ByeAssignment> {
        if participant_count % 2 == 1 {
            // Odd number - lowest ranked player gets bye each round
            vec![ByeAssignment { seed: participant_count as i32 }]
        } else {
            vec![]
        }
    }
}
```

---

## Plugin System Integration

### Extended GamePlugin Trait

Add tournament-specific methods to the existing `GamePlugin` trait:

```rust
// In portal-plugins/src/traits.rs

pub trait GamePlugin: Send + Sync {
    // ... existing methods ...

    // ========================================================================
    // Tournament Support (Extended)
    // ========================================================================

    /// Validate tournament-specific settings for this game.
    fn validate_tournament_settings(&self, settings: &Value) -> Result<(), String> {
        Ok(())  // Default: accept all settings
    }

    /// Get tournament format-specific configuration.
    fn tournament_format_config(&self, format: &TournamentFormatId) -> Option<FormatConfig> {
        None  // Default: use standard config
    }

    /// Custom seeding algorithm for this game.
    fn calculate_tournament_seeding(
        &self,
        participants: &[ParticipantRating],
    ) -> Option<Vec<Seed>> {
        None  // Default: use standard seeding
    }

    /// Validate match result for this game.
    fn validate_match_result(&self, result: &MatchResult) -> Result<(), String> {
        Ok(())  // Default: accept all results
    }

    /// Get win conditions for this game.
    fn win_conditions(&self) -> WinConditions {
        WinConditions::default()
    }

    /// Process game-specific match data.
    fn process_match_data(&self, data: &Value) -> Result<ProcessedMatchData, String> {
        Ok(ProcessedMatchData::default())
    }
}

pub struct WinConditions {
    pub round_based: bool,
    pub score_based: bool,
    pub time_based: bool,
    pub rounds_to_win: Option<i32>,
    pub max_rounds: Option<i32>,
    pub overtime_enabled: bool,
}

pub struct FormatConfig {
    pub min_participants: i32,
    pub max_participants: i32,
    pub recommended_match_format: MatchFormat,
    pub supports_consolation: bool,
}
```

### CS2 Plugin Tournament Example

```rust
// In portal-plugins/src/games/cs2/mod.rs

impl GamePlugin for Cs2Plugin {
    // ... existing methods ...

    fn validate_tournament_settings(&self, settings: &Value) -> Result<(), String> {
        // Validate CS2-specific settings
        if let Some(mr) = settings.get("match_max_rounds") {
            let mr = mr.as_i64().ok_or("match_max_rounds must be a number")?;
            if mr < 12 || mr > 30 || mr % 2 != 0 {
                return Err("match_max_rounds must be even, between 12 and 30".to_string());
            }
        }

        if let Some(knife_round) = settings.get("knife_round_enabled") {
            if !knife_round.is_boolean() {
                return Err("knife_round_enabled must be boolean".to_string());
            }
        }

        Ok(())
    }

    fn tournament_format_config(&self, format: &TournamentFormatId) -> Option<FormatConfig> {
        match format {
            TournamentFormatId::SingleElimination => Some(FormatConfig {
                min_participants: 4,
                max_participants: 128,
                recommended_match_format: MatchFormat::Bo1,
                supports_consolation: true,
            }),
            TournamentFormatId::DoubleElimination => Some(FormatConfig {
                min_participants: 8,
                max_participants: 64,
                recommended_match_format: MatchFormat::Bo3,
                supports_consolation: false,
            }),
            _ => None,
        }
    }

    fn win_conditions(&self) -> WinConditions {
        WinConditions {
            round_based: true,
            score_based: false,
            time_based: false,
            rounds_to_win: Some(13),  // MR12
            max_rounds: Some(24),
            overtime_enabled: true,
        }
    }

    fn process_match_data(&self, data: &Value) -> Result<ProcessedMatchData, String> {
        // Extract CS2-specific stats from match data
        let rounds = data.get("rounds").and_then(|r| r.as_array());
        let player_stats = data.get("player_stats").and_then(|s| s.as_object());

        Ok(ProcessedMatchData {
            game_specific: serde_json::json!({
                "total_rounds": rounds.map(|r| r.len()).unwrap_or(0),
                "ct_rounds_won": data.get("ct_rounds").and_then(|r| r.as_i64()).unwrap_or(0),
                "t_rounds_won": data.get("t_rounds").and_then(|r| r.as_i64()).unwrap_or(0),
                "overtime_rounds": data.get("overtime_rounds").and_then(|r| r.as_i64()).unwrap_or(0),
            }),
            ..Default::default()
        })
    }
}
```

---

## API Design

### Tournament Endpoints

```yaml
# Tournament CRUD & Lifecycle
POST   /v1/tournaments                           # Create tournament
GET    /v1/tournaments                           # List tournaments (with filters)
GET    /v1/tournaments/{id}                      # Get tournament details
PATCH  /v1/tournaments/{id}                      # Update tournament
DELETE /v1/tournaments/{id}                      # Delete/cancel tournament

# Lifecycle transitions
POST   /v1/tournaments/{id}/publish              # Publish for viewing
POST   /v1/tournaments/{id}/open-registration    # Open registration
POST   /v1/tournaments/{id}/close-registration   # Close registration
POST   /v1/tournaments/{id}/start-check-in       # Start check-in period
POST   /v1/tournaments/{id}/finalize-check-in    # End check-in, process no-shows
POST   /v1/tournaments/{id}/start                # Generate brackets, start tournament
POST   /v1/tournaments/{id}/complete             # Mark as completed
POST   /v1/tournaments/{id}/cancel               # Cancel tournament

# Stages
POST   /v1/tournaments/{id}/stages               # Create stage
GET    /v1/tournaments/{id}/stages               # List stages
GET    /v1/tournament-stages/{stage_id}          # Get stage details
PATCH  /v1/tournament-stages/{stage_id}          # Update stage
DELETE /v1/tournament-stages/{stage_id}          # Delete stage

# Map Pools
POST   /v1/tournaments/{id}/map-pool             # Set map pool
GET    /v1/tournaments/{id}/map-pool             # Get map pool
PATCH  /v1/tournaments/{id}/map-pool             # Update map pool
POST   /v1/tournament-stages/{id}/map-pool       # Stage-specific map pool

# Registration
POST   /v1/tournaments/{id}/registrations        # Register for tournament
GET    /v1/tournaments/{id}/registrations        # List registrations
GET    /v1/tournament-registrations/{reg_id}     # Get registration details
DELETE /v1/tournament-registrations/{reg_id}     # Withdraw
POST   /v1/tournament-registrations/{reg_id}/approve    # Approve (admin)
POST   /v1/tournament-registrations/{reg_id}/reject     # Reject (admin)
POST   /v1/tournament-registrations/{reg_id}/check-in   # Check in

# Seeding
GET    /v1/tournaments/{id}/seeding              # Get current seeding
POST   /v1/tournaments/{id}/seeding/auto         # Auto-seed by algorithm
POST   /v1/tournaments/{id}/seeding/manual       # Manual seeding

# Brackets
GET    /v1/tournaments/{id}/brackets             # Get all brackets
GET    /v1/tournament-brackets/{bracket_id}      # Get bracket with matches
POST   /v1/tournament-brackets/{bracket_id}/regenerate  # Regenerate bracket

# Matches
GET    /v1/tournaments/{id}/matches              # List all matches
GET    /v1/tournament-matches/{match_id}         # Get match details
PATCH  /v1/tournament-matches/{match_id}         # Update match (schedule, etc.)

# Match Flow
POST   /v1/tournament-matches/{match_id}/schedule       # Schedule match
POST   /v1/tournament-matches/{match_id}/start-checkin  # Start pre-match check-in
POST   /v1/tournament-matches/{match_id}/checkin        # Player/team check-in
POST   /v1/tournament-matches/{match_id}/start-veto     # Start map veto
POST   /v1/tournament-matches/{match_id}/veto           # Perform veto action
POST   /v1/tournament-matches/{match_id}/start          # Start match
POST   /v1/tournament-matches/{match_id}/games/{n}/result  # Submit game result
POST   /v1/tournament-matches/{match_id}/result         # Submit final result
POST   /v1/tournament-matches/{match_id}/confirm        # Confirm result (opponent)

# Match Admin
POST   /v1/tournament-matches/{match_id}/forfeit        # Report forfeit
POST   /v1/tournament-matches/{match_id}/override       # Admin override result
POST   /v1/tournament-matches/{match_id}/dispute        # Raise dispute
POST   /v1/tournament-matches/{match_id}/resolve        # Resolve dispute

# Standings (for round robin/swiss)
GET    /v1/tournament-brackets/{bracket_id}/standings   # Get standings

# Player/Team views
GET    /v1/players/me/tournaments                # My tournament registrations
GET    /v1/players/me/tournament-matches         # My upcoming/active matches
GET    /v1/league-teams/{id}/tournaments         # Team's tournament history
```

### Request/Response Examples

#### Create Tournament

```http
POST /v1/tournaments
Content-Type: application/json
Authorization: Bearer <token>

{
  "game_id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "CS2 Weekly Cup #42",
  "slug": "cs2-weekly-42",
  "description": "Weekly community tournament",
  "format": "single_elimination",
  "participant_type": "team",
  "team_size": 5,
  "min_participants": 8,
  "max_participants": 32,
  "registration_type": "open",
  "registration_start": "2024-02-01T00:00:00Z",
  "registration_end": "2024-02-07T18:00:00Z",
  "check_in_required": true,
  "check_in_start": "2024-02-07T18:00:00Z",
  "check_in_end": "2024-02-07T19:00:00Z",
  "scheduling_mode": "live",
  "starts_at": "2024-02-07T19:00:00Z",
  "default_match_format": "bo1",
  "default_map_veto_format": "bo1_ban_ban_random",
  "settings": {
    "match_max_rounds": 24,
    "knife_round_enabled": true
  }
}
```

```http
HTTP/1.1 201 Created
Content-Type: application/json

{
  "data": {
    "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    "game_id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "CS2 Weekly Cup #42",
    "slug": "cs2-weekly-42",
    "status": "draft",
    "format": "single_elimination",
    "participant_type": "team",
    "registration_type": "open",
    "min_participants": 8,
    "max_participants": 32,
    "registered_count": 0,
    "checked_in_count": 0,
    "registration_start": "2024-02-01T00:00:00Z",
    "registration_end": "2024-02-07T18:00:00Z",
    "starts_at": "2024-02-07T19:00:00Z",
    "created_at": "2024-01-25T10:00:00Z"
  },
  "request_id": "req_abc123"
}
```

#### Get Bracket

```http
GET /v1/tournament-brackets/aaa-bbb-ccc
Authorization: Bearer <token>
```

```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "data": {
    "bracket": {
      "id": "aaa-bbb-ccc",
      "name": "Main Bracket",
      "bracket_type": "single_elim",
      "total_rounds": 4,
      "current_round": 2,
      "status": "in_progress"
    },
    "matches": [
      {
        "id": "match-1",
        "round": 1,
        "match_number": 1,
        "bracket_position": "R1-1",
        "status": "completed",
        "participant1": {
          "registration_id": "reg-1",
          "name": "Team Alpha",
          "logo_url": "https://...",
          "seed": 1,
          "score": 2
        },
        "participant2": {
          "registration_id": "reg-16",
          "name": "Team Omega",
          "logo_url": "https://...",
          "seed": 16,
          "score": 0
        },
        "winner_registration_id": "reg-1",
        "match_format": "bo3"
      },
      // ... more matches
    ]
  },
  "request_id": "req_xyz789"
}
```

#### Submit Game Result

```http
POST /v1/tournament-matches/match-123/games/1/result
Content-Type: application/json
Authorization: Bearer <token>

{
  "map_id": "de_mirage",
  "participant1_score": 13,
  "participant2_score": 8,
  "duration_seconds": 2340,
  "game_data": {
    "ct_rounds_won": 7,
    "t_rounds_won": 6,
    "overtime_rounds": 0
  }
}
```

---

## State Machines

### Tournament Status State Machine

```
                    ┌──────────────────────────────────────────────────────────┐
                    │                                                          │
                    ▼                                                          │
┌─────────┐    ┌──────────┐    ┌──────────────┐    ┌──────────┐    ┌────────────┐
│  DRAFT  │───►│PUBLISHED │───►│REGISTRATION  │───►│ CHECK_IN │───►│  SEEDING   │
└─────────┘    └──────────┘    └──────────────┘    └──────────┘    └────────────┘
     │              │                  │                 │               │
     │              │                  │                 │               │
     │              ▼                  ▼                 ▼               ▼
     │         ┌──────────┐    ┌──────────────┐    ┌──────────┐    ┌────────────┐
     │         │CANCELLED │    │  CANCELLED   │    │CANCELLED │    │ IN_PROGRESS│
     │         └──────────┘    └──────────────┘    └──────────┘    └────────────┘
     │                                                                    │
     ▼                                                                    ▼
┌──────────┐                                                      ┌────────────┐
│CANCELLED │                                                      │ COMPLETED  │
└──────────┘                                                      └────────────┘

Transitions:
  DRAFT → PUBLISHED:          publish()
  PUBLISHED → REGISTRATION:   open_registration()
  REGISTRATION → CHECK_IN:    close_registration() + check_in enabled
  REGISTRATION → SEEDING:     close_registration() + no check_in
  CHECK_IN → SEEDING:         finalize_check_in()
  SEEDING → IN_PROGRESS:      start_tournament()
  IN_PROGRESS → COMPLETED:    last match completed
  ANY → CANCELLED:            cancel_tournament()
```

### Match Status State Machine

```
                                         ┌────────────────────────────────┐
                                         │                                │
                                         ▼                                │
┌─────────┐    ┌───────┐    ┌───────────┐    ┌──────────┐    ┌──────────┐│
│ PENDING │───►│ READY │───►│SCHEDULED  │───►│CHECK_IN  │───►│ PICK_BAN ││
└─────────┘    └───────┘    └───────────┘    └──────────┘    └──────────┘│
     │              │              │               │               │      │
     │              │              │               │               │      │
     │              │              ▼               │               ▼      │
     │              │        ┌───────────┐         │        ┌──────────┐  │
     │              └───────►│IN_PROGRESS│◄────────┴───────►│CANCELLED │  │
     │                       └───────────┘                  └──────────┘  │
     │                             │                                      │
     │                             ▼                                      │
     │                       ┌───────────────┐                            │
     │                       │AWAITING_RESULT│                            │
     │                       └───────────────┘                            │
     │                             │                                      │
     │              ┌──────────────┼──────────────┐                       │
     │              ▼              ▼              ▼                       │
     │        ┌──────────┐   ┌──────────┐   ┌──────────┐                  │
     │        │COMPLETED │   │ FORFEIT  │   │ DISPUTED │                  │
     │        └──────────┘   └──────────┘   └──────────┘                  │
     │                             │              │                       │
     │                             ▼              ▼                       │
     │                       ┌──────────┐   ┌──────────┐                  │
     └──────────────────────►│CANCELLED │   │COMPLETED │◄─────────────────┘
                             └──────────┘   └──────────┘

Key Transitions:
  PENDING → READY:              Both participants assigned
  READY → SCHEDULED:            schedule_match()
  READY → IN_PROGRESS:          start_match() (for live tournaments)
  SCHEDULED → CHECK_IN:         start_check_in()
  CHECK_IN → PICK_BAN:          both_checked_in() && veto_enabled
  CHECK_IN → IN_PROGRESS:       both_checked_in() && !veto_enabled
  PICK_BAN → IN_PROGRESS:       veto_complete()
  IN_PROGRESS → AWAITING_RESULT: match_ended()
  AWAITING_RESULT → COMPLETED:   result_confirmed()
  AWAITING_RESULT → DISPUTED:    dispute_raised()
  DISPUTED → COMPLETED:          dispute_resolved()
  ANY → FORFEIT:                 report_forfeit()
  ANY → CANCELLED:               match_cancelled()
```

---

## Testing Strategy

### Unit Tests

#### Bracket Generation Tests

```rust
#[cfg(test)]
mod single_elimination_tests {
    use super::*;

    #[test]
    fn test_4_team_bracket() {
        let generator = SingleEliminationGenerator;
        let participants: Vec<SeededParticipant> = (1..=4)
            .map(|seed| SeededParticipant {
                registration_id: TournamentRegistrationId::new(),
                seed,
                name: format!("Team {}", seed),
            })
            .collect();

        let matches = generator.generate_matches(&participants, &BracketSettings::default())
            .unwrap();

        assert_eq!(matches.len(), 3);  // Semi 1, Semi 2, Final
        assert_eq!(generator.calculate_rounds(4), 2);
    }

    #[test]
    fn test_6_team_bracket_with_byes() {
        let generator = SingleEliminationGenerator;
        let byes = generator.assign_byes(6);

        assert_eq!(byes.len(), 2);  // 8 - 6 = 2 byes
        assert_eq!(byes[0].seed, 1);  // Top 2 seeds get byes
        assert_eq!(byes[1].seed, 2);
    }

    #[test]
    fn test_bracket_seeding_1v16_2v15() {
        let generator = SingleEliminationGenerator;
        let participants = create_16_participants();
        let matches = generator.generate_matches(&participants, &BracketSettings::default())
            .unwrap();

        let round1_match1 = &matches[0];
        assert_eq!(round1_match1.participant1_seed, Some(1));
        assert_eq!(round1_match1.participant2_seed, Some(16));

        let round1_match2 = &matches[1];
        assert_eq!(round1_match2.participant1_seed, Some(8));
        assert_eq!(round1_match2.participant2_seed, Some(9));
    }
}

#[cfg(test)]
mod swiss_tests {
    use super::*;

    #[test]
    fn test_swiss_round_calculation() {
        let generator = SwissGenerator;

        assert_eq!(generator.calculate_rounds(8), 4);   // log2(8) + 1
        assert_eq!(generator.calculate_rounds(16), 5);  // log2(16) + 1
    }

    #[test]
    fn test_swiss_first_round_pairing() {
        let generator = SwissGenerator;
        let participants = create_8_participants();
        let matches = generator.generate_matches(&participants, &BracketSettings::default())
            .unwrap();

        // First round: 1v2, 3v4, 5v6, 7v8
        assert_eq!(matches.len(), 4);
        assert_eq!(matches[0].participant1_seed, Some(1));
        assert_eq!(matches[0].participant2_seed, Some(2));
    }
}
```

#### Match Progression Tests

```rust
#[cfg(test)]
mod match_progression_tests {
    use super::*;

    #[tokio::test]
    async fn test_winner_advances_in_single_elim() {
        let service = create_test_match_service().await;
        let tournament = create_single_elim_tournament(8).await;

        // Complete first match
        let match1 = get_match(&tournament, "R1-1").await;
        let result = service.submit_match_result(
            match1.id,
            MatchResult {
                participant1_score: 2,
                participant2_score: 0,
            },
            UserId::new(),
        ).await.unwrap();

        let progression = service.process_match_completion(match1.id).await.unwrap();

        assert!(progression.winner_advanced_to.is_some());
        let semifinal = get_match_by_id(progression.winner_advanced_to.unwrap()).await;
        assert!(semifinal.participant1_registration_id.is_some() ||
                semifinal.participant2_registration_id.is_some());
    }

    #[tokio::test]
    async fn test_loser_advances_in_double_elim() {
        let service = create_test_match_service().await;
        let tournament = create_double_elim_tournament(8).await;

        let match1 = get_match(&tournament, "W1-1").await;
        let result = service.submit_match_result(
            match1.id,
            MatchResult {
                participant1_score: 2,
                participant2_score: 1,
            },
            UserId::new(),
        ).await.unwrap();

        let progression = service.process_match_completion(match1.id).await.unwrap();

        assert!(progression.winner_advanced_to.is_some());
        assert!(progression.loser_advanced_to.is_some());

        let losers_match = get_match_by_id(progression.loser_advanced_to.unwrap()).await;
        assert!(losers_match.bracket_position.starts_with("L"));
    }
}
```

### Integration Tests

```rust
// In crates/portal-api/tests/tournaments_test.rs

#[tokio::test]
async fn test_tournament_lifecycle() {
    let app = TestApp::new().await;
    let game_id = get_game_uuid(&app, "cs2").await;
    grant_tournament_permission(&app).await;

    // Create tournament
    let response = app.post_json("/v1/tournaments", &json!({
        "game_id": game_id,
        "name": "Test Tournament",
        "slug": "test-tournament",
        "format": "single_elimination",
        "participant_type": "team",
        "min_participants": 4,
        "max_participants": 8,
        "registration_type": "open"
    })).await;
    response.assert_status(StatusCode::CREATED);
    let tournament: Value = response.json();
    let tournament_id = tournament["data"]["id"].as_str().unwrap();

    // Publish
    app.post_auth(&format!("/v1/tournaments/{}/publish", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Open registration
    app.post_auth(&format!("/v1/tournaments/{}/open-registration", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Register teams
    for i in 0..4 {
        let team = create_test_team(&app, &format!("Team {}", i), &format!("T{}", i)).await;
        app.post_json(
            &format!("/v1/tournaments/{}/registrations", tournament_id),
            &json!({ "team_season_id": team.team_season_id })
        ).await.assert_status(StatusCode::CREATED);
    }

    // Close registration and start
    app.post_auth(&format!("/v1/tournaments/{}/close-registration", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    app.post_auth(&format!("/v1/tournaments/{}/start", tournament_id))
        .await
        .assert_status(StatusCode::OK);

    // Verify bracket created
    let brackets = app.get(&format!("/v1/tournaments/{}/brackets", tournament_id)).await;
    brackets.assert_status(StatusCode::OK);
    let brackets_data: Value = brackets.json();
    assert!(!brackets_data["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_match_result_submission() {
    let app = TestApp::new().await;
    let (tournament, bracket, matches) = setup_tournament_with_bracket(&app).await;

    let match1 = &matches[0];

    // Submit game results for Bo3
    for game_num in 1..=2 {
        app.post_json(
            &format!("/v1/tournament-matches/{}/games/{}/result", match1["id"], game_num),
            &json!({
                "map_id": "de_mirage",
                "participant1_score": 13,
                "participant2_score": 8
            })
        ).await.assert_status(StatusCode::OK);
    }

    // Verify match completed
    let match_response = app.get(&format!("/v1/tournament-matches/{}", match1["id"])).await;
    let match_data: Value = match_response.json();
    assert_eq!(match_data["data"]["status"], "completed");
    assert_eq!(match_data["data"]["participant1_score"], 2);
    assert_eq!(match_data["data"]["participant2_score"], 0);
}
```

### Test Builders

```rust
// In crates/portal-test/src/builders/tournament.rs

pub struct TournamentBuilder {
    game_id: Option<GameId>,
    name: String,
    format: TournamentFormat,
    participant_type: TournamentParticipantType,
    min_participants: i32,
    max_participants: i32,
}

impl TournamentBuilder {
    pub fn new() -> Self {
        Self {
            game_id: None,
            name: "Test Tournament".to_string(),
            format: TournamentFormat::SingleElimination,
            participant_type: TournamentParticipantType::Team,
            min_participants: 4,
            max_participants: 16,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn format(mut self, format: TournamentFormat) -> Self {
        self.format = format;
        self
    }

    pub fn double_elimination(mut self) -> Self {
        self.format = TournamentFormat::DoubleElimination;
        self
    }

    pub fn individual(mut self) -> Self {
        self.participant_type = TournamentParticipantType::Individual;
        self
    }

    pub async fn build_persisted(self, pool: &DbPool) -> Tournament {
        // Insert into database
    }
}

pub struct TournamentRegistrationBuilder { /* ... */ }
pub struct TournamentMatchBuilder { /* ... */ }
```

---

## Implementation Phases

### Phase 1: Core Foundation (Weeks 1-3)

**Goal**: Basic tournament CRUD and single elimination brackets.

**Tasks**:
1. Database migrations for core tables
   - `tournaments`
   - `tournament_stages`
   - `tournament_brackets`
   - `tournament_matches`
   - `tournament_registrations`

2. Core domain entities
   - `Tournament`, `TournamentStage`, `TournamentBracket`
   - `TournamentMatch`, `TournamentRegistration`
   - Enums and status types

3. Repository traits and adapters
   - `TournamentRepository`, `PgTournamentRepository`
   - `TournamentStageRepository`, `PgTournamentStageRepository`
   - `TournamentBracketRepository`, `PgTournamentBracketRepository`
   - `TournamentMatchRepository`, `PgTournamentMatchRepository`
   - `TournamentRegistrationRepository`, `PgTournamentRegistrationRepository`

4. Basic services
   - `TournamentService` (CRUD, lifecycle)
   - `TournamentRegistrationService` (register, withdraw)
   - `SingleEliminationGenerator`

5. API handlers
   - Tournament CRUD endpoints
   - Registration endpoints
   - Basic bracket retrieval

**Deliverables**:
- Create tournament
- Register teams/players
- Generate single elimination bracket
- View bracket

### Phase 2: Registration & Seeding (Weeks 4-5)

**Goal**: Complete registration flow and seeding system.

**Tasks**:
1. Registration types
   - Open, invite-only, approval-based
   - Waitlist support

2. Check-in system
   - Check-in period configuration
   - Player/team check-in
   - No-show processing

3. Seeding algorithms
   - Random seeding
   - Rating-based seeding
   - Manual seeding UI support

4. Eligibility validation
   - Game profile requirements
   - Rating restrictions

**Deliverables**:
- Complete registration flow
- Check-in system
- Auto-seeding

### Phase 3: Match System (Weeks 6-8)

**Goal**: Match lifecycle and result reporting.

**Tasks**:
1. Match scheduling
   - Fixed scheduling (live)
   - Self-scheduling with deadlines

2. Match flow
   - Pre-match check-in
   - Map veto system
   - Match start

3. Result reporting
   - Game-by-game results
   - Final match result
   - Result confirmation

4. Bracket progression
   - Winner advancement
   - Loser advancement (for double elim)
   - Bracket completion detection

5. Forfeits and disputes
   - Forfeit handling
   - Dispute raising
   - Admin resolution

**Deliverables**:
- Complete match lifecycle
- Result submission
- Automatic bracket progression

### Phase 4: Plugin Integration (Weeks 9-10)

**Goal**: Game-specific customization.

**Tasks**:
1. Extended `GamePlugin` trait
   - Tournament settings validation
   - Custom seeding algorithms
   - Match result validation

2. Map pool and veto
   - Tournament map pools
   - Stage-specific pools
   - Veto format execution

3. Game-specific data
   - Match statistics
   - Player performance tracking

**Deliverables**:
- Game-specific tournament settings
- Map veto system
- Game statistics integration

### Phase 5: Advanced Features (Weeks 11-14)

**Goal**: Multi-stage tournaments and advanced formats.

**Tasks**:
1. Double elimination
   - Winners bracket
   - Losers bracket
   - Grand final logic

2. Round robin
   - Pairings generation
   - Standings calculation
   - Tiebreakers

3. Swiss system
   - Dynamic pairings
   - Buchholz scoring
   - Cut-off determination

4. Groups + Playoffs
   - Group stage setup
   - Advancement rules
   - Playoff generation

5. Multi-stage orchestration
   - Stage transitions
   - Participant advancement
   - Cross-stage results

**Deliverables**:
- All bracket formats
- Multi-stage tournaments
- Complete standings system

### Phase 6: Polish & Performance (Weeks 15-16)

**Goal**: Production readiness.

**Tasks**:
1. Performance optimization
   - Materialized views
   - Query optimization
   - Caching strategy

2. Real-time considerations
   - WebSocket event hooks
   - Bracket update notifications

3. Admin tools
   - Match override UI
   - Participant management
   - Tournament duplication

4. Documentation
   - API documentation
   - Integration guides

**Deliverables**:
- Production-ready system
- Comprehensive documentation
- Admin tooling

---

## Appendix

### Strongly-Typed IDs to Add

Add to `portal-core/src/ids.rs`:

```rust
define_id!(
    /// Unique identifier for a tournament stage.
    TournamentStageId
);

define_id!(
    /// Unique identifier for a tournament bracket.
    TournamentBracketId
);

define_id!(
    /// Unique identifier for a tournament registration.
    TournamentRegistrationId
);

define_id!(
    /// Unique identifier for a tournament match.
    TournamentMatchId
);

define_id!(
    /// Unique identifier for a tournament match game.
    TournamentMatchGameId
);

define_id!(
    /// Unique identifier for a tournament map pool.
    TournamentMapPoolId
);

define_id!(
    /// Unique identifier for an ad-hoc tournament team.
    TournamentAdhocTeamId
);
```

### Permissions to Add

```rust
// In portal-core/src/lib.rs (permissions module)

pub mod tournament {
    pub const CREATE: &str = "tournament.create";
    pub const MANAGE: &str = "tournament.manage";
    pub const BRACKETS_MANAGE: &str = "tournament.brackets.manage";
    pub const REGISTRATIONS_MANAGE: &str = "tournament.registrations.manage";
    pub const RESULTS_SUBMIT: &str = "tournament.results.submit";
    pub const DISPUTES_RESOLVE: &str = "tournament.disputes.resolve";
}
```

### Error Types to Add

```rust
// In portal-core/src/errors.rs

pub enum DomainError {
    // ... existing errors ...

    // Tournament errors
    TournamentNotFound(String),
    TournamentStageNotFound(String),
    TournamentBracketNotFound(String),
    TournamentMatchNotFound(String),
    TournamentRegistrationNotFound(String),

    TournamentNotOpen,
    TournamentRegistrationClosed,
    TournamentAlreadyStarted,
    TournamentFull,

    AlreadyRegistered,
    NotRegistered,
    RegistrationPending,
    NotCheckedIn,

    MatchNotReady,
    MatchAlreadyStarted,
    MatchAlreadyCompleted,
    InvalidMatchResult,

    VetoNotStarted,
    VetoAlreadyComplete,
    InvalidVetoAction,
    NotYourTurn,

    BracketGenerationFailed(String),
    InsufficientParticipants,
}
```

---

*Document Version: 1.0*
*Last Updated: 2024-01-25*
*Author: Claude (AI Assistant)*
