# Pick-Ban (Map Veto) System Design

> **Sub-Phases**: 3.4 (Pick-Ban Core), 3.5 (Plugin Integration)
> **Related**: [03-match-lifecycle.md](./03-match-lifecycle.md)

---

## Overview

The Pick-Ban system handles map selection for matches through a turn-based veto process. Different game plugins provide different veto formats (Bo1, Bo3, Bo5), and the system enforces turn order, timeout handling, and side selection.

### Key Features

- **Format-Agnostic**: Veto sequences defined by game plugins
- **Turn-Based**: Strict turn enforcement with team validation
- **Timeout Handling**: Auto-action on timeout (random selection)
- **Side Selection**: Winner of map pick can choose side (if applicable)
- **Real-Time Ready**: Designed for WebSocket notifications

---

## Veto Formats

### Standard Formats (from plugin)

**Bo1 (7-map pool)**:
```
Team A Ban → Team B Ban → Team A Ban → Team B Ban →
Team A Ban → Team B Ban → Decider (remaining map)
```

**Bo3 (7-map pool)**:
```
Team A Ban → Team B Ban → Team A Pick → Team B Pick →
Team A Ban → Team B Ban → Decider (remaining map)
```

**Bo5 (7-map pool)**:
```
Team A Ban → Team B Ban → Team A Pick → Team B Pick →
Team A Pick → Team B Pick → Decider (remaining map)
```

### Action Types

| Type | Description |
|------|-------------|
| `ban` | Remove a map from the pool |
| `pick` | Select a map to be played |
| `decider` | Last remaining map (automatic) |

---

## Database Schema

### veto_sessions

```sql
CREATE TABLE veto_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Format
    veto_format_id VARCHAR(64) NOT NULL,
    map_pool TEXT[] NOT NULL,  -- Starting map pool

    -- Coin flip / first action
    first_action_registration_id UUID REFERENCES tournament_registrations(id),
    coin_flip_winner_registration_id UUID REFERENCES tournament_registrations(id),

    -- Current state
    current_action_number INTEGER NOT NULL DEFAULT 0,
    current_team_turn UUID REFERENCES tournament_registrations(id),

    -- Remaining maps (updated as veto progresses)
    remaining_maps TEXT[] NOT NULL,

    -- Selected maps (in play order)
    selected_maps TEXT[] NOT NULL DEFAULT '{}',

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Timing
    action_deadline TIMESTAMPTZ,
    timeout_seconds INTEGER NOT NULL DEFAULT 30,

    -- Timestamps
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT veto_sessions_unique_match UNIQUE (match_id),
    CONSTRAINT veto_sessions_check_status CHECK (status IN (
        'pending', 'coin_flip', 'in_progress', 'completed', 'cancelled'
    ))
);

CREATE INDEX idx_veto_sessions_match ON veto_sessions(match_id);
CREATE INDEX idx_veto_sessions_status ON veto_sessions(status);
CREATE INDEX idx_veto_sessions_deadline ON veto_sessions(action_deadline)
    WHERE status = 'in_progress';

CREATE TRIGGER veto_sessions_updated_at
    BEFORE UPDATE ON veto_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE veto_sessions IS 'Map veto sessions for tournament matches';
```

### veto_actions

```sql
CREATE TABLE veto_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES veto_sessions(id) ON DELETE CASCADE,

    -- Action details
    action_number INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,
    map_id VARCHAR(64) NOT NULL,

    -- Who performed
    performed_by_registration_id UUID REFERENCES tournament_registrations(id),
    performed_by_user_id UUID REFERENCES users(id),

    -- Side selection (for picks)
    side_selection VARCHAR(16),  -- e.g., 'ct', 't', 'attack', 'defense'
    side_selected_by_registration_id UUID REFERENCES tournament_registrations(id),

    -- Auto-action (timeout)
    was_auto_action BOOLEAN NOT NULL DEFAULT false,
    auto_action_reason VARCHAR(64),

    -- Timestamps
    performed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    side_selected_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT veto_actions_unique UNIQUE (session_id, action_number),
    CONSTRAINT veto_actions_check_type CHECK (action_type IN ('ban', 'pick', 'decider')),
    CONSTRAINT veto_actions_check_number CHECK (action_number >= 1)
);

CREATE INDEX idx_veto_actions_session ON veto_actions(session_id);

COMMENT ON TABLE veto_actions IS 'Individual actions in a map veto session';
```

---

## Domain Entities

### VetoSession

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    TournamentMatchId, TournamentRegistrationId, UserId, VetoSessionId,
};

/// A map veto session for a match.
#[derive(Debug, Clone)]
pub struct VetoSession {
    pub id: VetoSessionId,
    pub match_id: TournamentMatchId,

    /// Veto format identifier (from plugin)
    pub veto_format_id: String,

    /// Starting map pool
    pub map_pool: Vec<String>,

    /// Who won the coin flip (first picker)
    pub coin_flip_winner_registration_id: Option<TournamentRegistrationId>,

    /// Who has first action
    pub first_action_registration_id: Option<TournamentRegistrationId>,

    /// Current action number (0 = not started)
    pub current_action_number: u32,

    /// Whose turn it is
    pub current_team_turn: Option<TournamentRegistrationId>,

    /// Maps remaining in pool
    pub remaining_maps: Vec<String>,

    /// Maps selected (in play order)
    pub selected_maps: Vec<String>,

    /// Current status
    pub status: VetoStatus,

    /// Deadline for current action
    pub action_deadline: Option<DateTime<Utc>>,

    /// Timeout per action (seconds)
    pub timeout_seconds: u32,

    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetoStatus {
    /// Session created, waiting to start
    Pending,
    /// Coin flip in progress
    CoinFlip,
    /// Veto actions in progress
    InProgress,
    /// All maps selected, veto complete
    Completed,
    /// Veto cancelled (match cancelled, etc.)
    Cancelled,
}

impl VetoSession {
    /// Get the current action from the format sequence.
    pub fn get_current_action(&self, format: &VetoFormat) -> Option<&VetoFormatAction> {
        format.sequence.get(self.current_action_number as usize)
    }

    /// Check if veto is complete.
    pub fn is_complete(&self, format: &VetoFormat) -> bool {
        self.current_action_number as usize >= format.sequence.len()
    }

    /// Get next team to act.
    pub fn get_next_team(
        &self,
        format: &VetoFormat,
        participant1_id: TournamentRegistrationId,
        participant2_id: TournamentRegistrationId,
    ) -> Option<TournamentRegistrationId> {
        let action = self.get_current_action(format)?;

        match action.team {
            0 => None, // Decider - automatic
            1 => Some(
                if self.first_action_registration_id == Some(participant1_id) {
                    participant1_id
                } else {
                    participant2_id
                }
            ),
            2 => Some(
                if self.first_action_registration_id == Some(participant1_id) {
                    participant2_id
                } else {
                    participant1_id
                }
            ),
            _ => None,
        }
    }
}
```

### VetoAction

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    TournamentRegistrationId, UserId, VetoActionId, VetoSessionId,
};

/// A single action in a veto session.
#[derive(Debug, Clone)]
pub struct VetoAction {
    pub id: VetoActionId,
    pub session_id: VetoSessionId,

    /// Action sequence number (1-indexed)
    pub action_number: u32,

    /// Type of action
    pub action_type: VetoActionType,

    /// Map selected/banned
    pub map_id: String,

    /// Who performed the action
    pub performed_by_registration_id: Option<TournamentRegistrationId>,
    pub performed_by_user_id: Option<UserId>,

    /// Side selection (for picks)
    pub side_selection: Option<String>,
    pub side_selected_by_registration_id: Option<TournamentRegistrationId>,
    pub side_selected_at: Option<DateTime<Utc>>,

    /// Was this an auto-action due to timeout?
    pub was_auto_action: bool,
    pub auto_action_reason: Option<String>,

    pub performed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetoActionType {
    Ban,
    Pick,
    Decider,
}

impl std::fmt::Display for VetoActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ban => write!(f, "ban"),
            Self::Pick => write!(f, "pick"),
            Self::Decider => write!(f, "decider"),
        }
    }
}
```

### VetoFormat (from plugin)

```rust
/// Map veto format configuration from game plugin.
#[derive(Debug, Clone)]
pub struct VetoFormat {
    pub id: String,
    pub display_name: String,
    pub description: String,

    /// Sequence of actions
    pub sequence: Vec<VetoFormatAction>,

    /// Minimum maps required in pool
    pub min_map_pool: usize,
}

#[derive(Debug, Clone)]
pub struct VetoFormatAction {
    /// Which team performs this action
    /// 0 = automatic (decider)
    /// 1 = team with first action
    /// 2 = team with second action
    pub team: u8,

    /// Action type
    pub action_type: VetoActionType,
}

impl VetoFormat {
    /// Create standard Bo1 format.
    pub fn bo1() -> Self {
        Self {
            id: "bo1_standard".to_string(),
            display_name: "Best of 1".to_string(),
            description: "6 bans alternating, 1 decider".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }

    /// Create standard Bo3 format.
    pub fn bo3() -> Self {
        Self {
            id: "bo3_standard".to_string(),
            display_name: "Best of 3".to_string(),
            description: "Ban-Ban-Pick-Pick-Ban-Ban-Decider".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }

    /// Create standard Bo5 format.
    pub fn bo5() -> Self {
        Self {
            id: "bo5_standard".to_string(),
            display_name: "Best of 5".to_string(),
            description: "Ban-Ban-Pick-Pick-Pick-Pick-Decider".to_string(),
            sequence: vec![
                VetoFormatAction { team: 1, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 2, action_type: VetoActionType::Ban },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 1, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 2, action_type: VetoActionType::Pick },
                VetoFormatAction { team: 0, action_type: VetoActionType::Decider },
            ],
            min_map_pool: 7,
        }
    }
}
```

---

## State Machine

### Session State Transitions

```
            ┌──────────────┐
            │   Pending    │
            │  (created)   │
            └──────┬───────┘
                   │ start_veto()
                   ▼
            ┌──────────────┐
            │  Coin Flip   │
            │  (optional)  │
            └──────┬───────┘
                   │ coin_flip_complete()
                   ▼
            ┌──────────────┐
            │ In Progress  │◄───────┐
            │              │        │
            └──────┬───────┘        │
                   │                │
       ┌───────────┴───────────┐    │
       ▼                       ▼    │
┌──────────────┐        ┌──────────┴───┐
│ Perform      │        │   Timeout    │
│ Action       │        │ Auto-Action  │
└──────┬───────┘        └──────┬───────┘
       │                       │
       └───────────┬───────────┘
                   │
                   ▼
            ┌──────────────┐
            │  More        │──yes──▶ back to In Progress
            │  Actions?    │
            └──────┬───────┘
                   │ no
                   ▼
            ┌──────────────┐
            │  Completed   │
            │              │
            │ Match → In   │
            │  Progress    │
            └──────────────┘
```

### Action Validation Rules

| Rule | Description |
|------|-------------|
| Correct Team | Only the team whose turn it is can act |
| Valid Map | Map must be in remaining_maps |
| Not Banned | Cannot pick a banned map |
| Deadline | Action must be before action_deadline |
| Sequential | Actions must be in order |

---

## Service Design

### VetoService

```rust
pub struct VetoService<VSR, VAR, TMR, TRR, PM>
where
    VSR: VetoSessionRepository,
    VAR: VetoActionRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    PM: PluginManager,
{
    session_repo: Arc<VSR>,
    action_repo: Arc<VAR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    plugin_manager: Arc<PM>,
    default_timeout_seconds: u32,  // e.g., 30
}

impl<VSR, VAR, TMR, TRR, PM> VetoService<VSR, VAR, TMR, TRR, PM>
where
    VSR: VetoSessionRepository,
    VAR: VetoActionRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
    PM: PluginManager,
{
    /// Create a veto session for a match.
    ///
    /// Called when match transitions to PickBan status.
    pub async fn create_session(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<VetoSession, DomainError>;

    /// Start the veto session.
    ///
    /// If coin flip required, sets status to CoinFlip.
    /// Otherwise, starts directly with first action.
    pub async fn start_session(
        &self,
        session_id: VetoSessionId,
    ) -> Result<VetoSession, DomainError>;

    /// Record coin flip result.
    ///
    /// Sets the team that will have first action.
    pub async fn record_coin_flip(
        &self,
        session_id: VetoSessionId,
        winner_registration_id: TournamentRegistrationId,
    ) -> Result<VetoSession, DomainError>;

    /// Perform a veto action (ban or pick).
    ///
    /// # Errors
    /// - `NotYourTurn` if wrong team tries to act
    /// - `InvalidMap` if map not in remaining_maps
    /// - `ActionTimeout` if deadline passed
    pub async fn perform_action(
        &self,
        session_id: VetoSessionId,
        map_id: String,
        performed_by: UserId,
    ) -> Result<VetoActionResult, DomainError>;

    /// Select side for a picked map.
    ///
    /// Called after a pick action by the team that picked.
    pub async fn select_side(
        &self,
        session_id: VetoSessionId,
        action_number: u32,
        side: String,
        selected_by: UserId,
    ) -> Result<VetoAction, DomainError>;

    /// Process timeout for current action.
    ///
    /// Called by background job when action_deadline passes.
    /// Performs random selection from remaining maps.
    pub async fn process_timeout(
        &self,
        session_id: VetoSessionId,
    ) -> Result<VetoActionResult, DomainError>;

    /// Cancel veto session.
    pub async fn cancel_session(
        &self,
        session_id: VetoSessionId,
        reason: String,
    ) -> Result<VetoSession, DomainError>;

    /// Get current veto state for a match.
    pub async fn get_session_state(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<VetoSessionState, DomainError>;

    /// Find sessions with expired deadlines.
    ///
    /// Called by background job.
    pub async fn find_timed_out_sessions(&self) -> Result<Vec<VetoSession>, DomainError>;
}

#[derive(Debug, Clone)]
pub struct VetoActionResult {
    pub session: VetoSession,
    pub action: VetoAction,
    pub veto_complete: bool,
    pub next_team: Option<TournamentRegistrationId>,
    pub next_action_type: Option<VetoActionType>,
}

#[derive(Debug, Clone)]
pub struct VetoSessionState {
    pub session: VetoSession,
    pub actions: Vec<VetoAction>,
    pub format: VetoFormat,
    pub current_action: Option<VetoFormatAction>,
    pub maps_with_status: Vec<MapStatus>,
}

#[derive(Debug, Clone)]
pub struct MapStatus {
    pub map_id: String,
    pub map_name: String,
    pub image_url: Option<String>,
    pub status: MapVetoStatus,
    pub banned_by: Option<TournamentRegistrationId>,
    pub picked_by: Option<TournamentRegistrationId>,
    pub game_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapVetoStatus {
    Available,
    Banned,
    Picked,
    Decider,
}
```

---

## Plugin Integration

### Extended GamePlugin Trait

```rust
/// Extension to GamePlugin for tournament features.
pub trait TournamentPlugin: GamePlugin {
    /// Get available veto formats for this game.
    fn veto_formats(&self) -> Vec<VetoFormat>;

    /// Get the default veto format for a match format.
    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String>;

    /// Validate a map pool for this game.
    fn validate_map_pool_for_veto(
        &self,
        maps: &[String],
        veto_format_id: &str,
    ) -> Result<(), String>;

    /// Get map metadata for display.
    fn get_map_metadata(&self, map_id: &str) -> Option<MapMetadata>;

    /// Get available sides for a map (e.g., CT/T for CS2).
    fn get_available_sides(&self, map_id: &str) -> Vec<SideOption>;
}

#[derive(Debug, Clone)]
pub struct MapMetadata {
    pub id: String,
    pub display_name: String,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub game_modes: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SideOption {
    pub id: String,
    pub display_name: String,
    pub short_name: String,
}
```

### CS2 Plugin Implementation

```rust
impl TournamentPlugin for Cs2Plugin {
    fn veto_formats(&self) -> Vec<VetoFormat> {
        vec![
            VetoFormat::bo1(),
            VetoFormat::bo3(),
            VetoFormat::bo5(),
        ]
    }

    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String> {
        match match_format {
            MatchFormat::Bo1 => Some("bo1_standard".to_string()),
            MatchFormat::Bo3 => Some("bo3_standard".to_string()),
            MatchFormat::Bo5 => Some("bo5_standard".to_string()),
            MatchFormat::Bo7 => None, // Not supported
        }
    }

    fn validate_map_pool_for_veto(
        &self,
        maps: &[String],
        veto_format_id: &str,
    ) -> Result<(), String> {
        let format = self.veto_formats()
            .into_iter()
            .find(|f| f.id == veto_format_id)
            .ok_or_else(|| format!("Unknown veto format: {}", veto_format_id))?;

        if maps.len() < format.min_map_pool {
            return Err(format!(
                "Map pool too small: {} maps required, {} provided",
                format.min_map_pool,
                maps.len()
            ));
        }

        // Validate each map exists
        let available_maps = self.available_maps();
        for map in maps {
            if !available_maps.iter().any(|m| &m.id == map) {
                return Err(format!("Unknown map: {}", map));
            }
        }

        Ok(())
    }

    fn get_map_metadata(&self, map_id: &str) -> Option<MapMetadata> {
        self.available_maps()
            .into_iter()
            .find(|m| m.id == map_id)
            .map(|m| MapMetadata {
                id: m.id,
                display_name: m.display_name,
                image_url: m.image_url,
                thumbnail_url: None,
                game_modes: m.game_modes,
                description: None,
            })
    }

    fn get_available_sides(&self, _map_id: &str) -> Vec<SideOption> {
        vec![
            SideOption {
                id: "ct".to_string(),
                display_name: "Counter-Terrorist".to_string(),
                short_name: "CT".to_string(),
            },
            SideOption {
                id: "t".to_string(),
                display_name: "Terrorist".to_string(),
                short_name: "T".to_string(),
            },
        ]
    }
}
```

---

## API Endpoints

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/veto/start

Start the veto session for a match.

**Response** (200 OK):
```json
{
  "data": {
    "session_id": "...",
    "status": "coin_flip",
    "map_pool": ["de_mirage", "de_inferno", "de_nuke", "de_overpass", "de_ancient", "de_anubis", "de_vertigo"],
    "coin_flip_required": true
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/veto/coin-flip

Record coin flip result.

**Request**:
```json
{
  "winner_registration_id": "..."
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/veto/action

Perform a veto action.

**Request**:
```json
{
  "map_id": "de_mirage"
}
```

**Response** (200 OK):
```json
{
  "data": {
    "action": {
      "action_number": 1,
      "action_type": "ban",
      "map_id": "de_mirage",
      "performed_by": {
        "registration_id": "...",
        "name": "Team Alpha"
      }
    },
    "session": {
      "status": "in_progress",
      "current_action_number": 2,
      "current_team_turn": "...",
      "remaining_maps": ["de_inferno", "de_nuke", "de_overpass", "de_ancient", "de_anubis", "de_vertigo"],
      "action_deadline": "2025-01-15T19:00:30Z"
    },
    "next_action": {
      "action_number": 2,
      "action_type": "ban",
      "team_name": "Team Beta"
    }
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/veto/side

Select starting side after a pick.

**Request**:
```json
{
  "action_number": 3,
  "side": "ct"
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/veto/state

Get current veto state.

**Response** (200 OK):
```json
{
  "data": {
    "session": {
      "id": "...",
      "status": "in_progress",
      "current_action_number": 4,
      "action_deadline": "2025-01-15T19:01:30Z"
    },
    "format": {
      "id": "bo3_standard",
      "display_name": "Best of 3",
      "total_actions": 7
    },
    "current_action": {
      "action_number": 4,
      "action_type": "pick",
      "team_name": "Team Beta"
    },
    "maps": [
      {
        "id": "de_mirage",
        "name": "Mirage",
        "image_url": "...",
        "status": "banned",
        "banned_by": "Team Alpha"
      },
      {
        "id": "de_inferno",
        "name": "Inferno",
        "image_url": "...",
        "status": "banned",
        "banned_by": "Team Beta"
      },
      {
        "id": "de_nuke",
        "name": "Nuke",
        "image_url": "...",
        "status": "picked",
        "picked_by": "Team Alpha",
        "game_number": 1,
        "side_selected": "ct"
      },
      {
        "id": "de_overpass",
        "name": "Overpass",
        "image_url": "...",
        "status": "available"
      }
    ],
    "actions": [
      {"number": 1, "type": "ban", "map": "de_mirage", "team": "Team Alpha"},
      {"number": 2, "type": "ban", "map": "de_inferno", "team": "Team Beta"},
      {"number": 3, "type": "pick", "map": "de_nuke", "team": "Team Alpha", "side": "ct"}
    ]
  }
}
```

---

## Real-Time Considerations

### WebSocket Events (Future)

```json
// Veto action performed
{
  "type": "veto_action",
  "match_id": "...",
  "action": {
    "number": 3,
    "type": "pick",
    "map_id": "de_nuke",
    "team": "Team Alpha"
  },
  "next_team": "Team Beta",
  "next_action_type": "pick",
  "deadline": "2025-01-15T19:01:30Z"
}

// Veto complete
{
  "type": "veto_complete",
  "match_id": "...",
  "selected_maps": ["de_nuke", "de_overpass", "de_ancient"],
  "match_status": "in_progress"
}

// Timeout warning
{
  "type": "veto_timeout_warning",
  "match_id": "...",
  "seconds_remaining": 10
}
```

### Polling Fallback

For clients without WebSocket, poll `GET /veto/state` every 5 seconds during active veto.

---

## Timeout Handling

### Background Job

```rust
/// Background job to process veto timeouts.
pub async fn process_veto_timeouts(veto_service: &VetoService) {
    loop {
        let timed_out = veto_service.find_timed_out_sessions().await?;

        for session in timed_out {
            match veto_service.process_timeout(session.id).await {
                Ok(result) => {
                    log::info!(
                        "Auto-action for session {}: {} {}",
                        session.id,
                        result.action.action_type,
                        result.action.map_id
                    );

                    // Emit WebSocket event
                    notify_veto_action(&result).await;
                }
                Err(e) => {
                    log::error!("Failed to process timeout for session {}: {}", session.id, e);
                }
            }
        }

        // Check every 5 seconds
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
```

### Auto-Action Logic

When timeout occurs:
1. Get remaining maps
2. Select random map from remaining
3. Record action with `was_auto_action = true`
4. Continue to next action or complete

---

## Error Handling

### New Error Types

```rust
pub enum VetoError {
    /// Session not found
    SessionNotFound(VetoSessionId),

    /// Not the active team's turn
    NotYourTurn {
        expected: TournamentRegistrationId,
        got: TournamentRegistrationId,
    },

    /// Map not available (already banned/picked or not in pool)
    MapNotAvailable(String),

    /// Session not in correct state
    InvalidSessionState {
        current: VetoStatus,
        expected: VetoStatus,
    },

    /// Action deadline passed
    ActionTimeout,

    /// Veto format not found
    FormatNotFound(String),

    /// Invalid side selection
    InvalidSide(String),

    /// Side selection not allowed for this action
    SideSelectionNotAllowed(u32),
}
```

---

## Testing Notes

### Unit Tests

- VetoFormat sequence validation
- Turn calculation logic
- Map availability checking
- Random selection for timeout

### Integration Tests

```
test_create_veto_session
test_start_veto_with_coin_flip
test_perform_ban_action
test_perform_pick_action
test_wrong_team_cannot_act
test_invalid_map_rejected
test_timeout_auto_action
test_complete_bo1_veto
test_complete_bo3_veto
test_side_selection_after_pick
test_veto_session_cancellation
test_get_veto_state
```

### Edge Case Tests

```
test_simultaneous_action_attempt
test_action_at_exact_deadline
test_all_maps_same_name_handling
test_plugin_format_not_found
```
