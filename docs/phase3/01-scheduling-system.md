# Scheduling System Design

> **Sub-Phases**: 3.2 (Scheduling), 3.3 (Availability)
> **Related**: [03-match-lifecycle.md](./03-match-lifecycle.md)

---

## Overview

The scheduling system handles how matches get assigned times. It supports two primary modes:

1. **Self-Scheduled**: Both teams negotiate a match time through proposals
2. **Admin-Scheduled**: Tournament admins directly assign match times

Additionally, the **Availability System** allows players and teams to define when they're available, enabling intelligent match time suggestions.

---

## Database Schema

### schedule_proposals

```sql
CREATE TABLE schedule_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who proposed
    proposed_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    proposed_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Proposed time slots (array of up to 5 options)
    proposed_times TIMESTAMPTZ[] NOT NULL,

    -- Selected time (set when accepted)
    selected_time TIMESTAMPTZ,

    -- Response tracking
    responded_at TIMESTAMPTZ,
    responded_by_user_id UUID REFERENCES users(id),

    -- Counter-proposal reference
    counter_proposal_id UUID REFERENCES schedule_proposals(id),

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Expiration
    expires_at TIMESTAMPTZ NOT NULL,

    -- Admin notes
    notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
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
CREATE INDEX idx_schedule_proposals_expires ON schedule_proposals(expires_at)
    WHERE status = 'pending';

CREATE TRIGGER schedule_proposals_updated_at
    BEFORE UPDATE ON schedule_proposals
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE schedule_proposals IS 'Match scheduling proposals between teams';
```

### availability_windows

```sql
CREATE TABLE availability_windows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Owner (one of these set)
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,

    -- Time window (day of week + times in minutes from midnight)
    day_of_week SMALLINT NOT NULL,  -- 0 = Sunday, 6 = Saturday
    start_minutes SMALLINT NOT NULL,  -- Minutes from midnight (0-1439)
    end_minutes SMALLINT NOT NULL,    -- Minutes from midnight (0-1439)

    -- Timezone for interpretation
    timezone VARCHAR(64) NOT NULL DEFAULT 'UTC',

    -- Effective dates (null = always)
    effective_from DATE,
    effective_until DATE,

    -- For tournament-specific availability
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,

    -- Substitute flag (for team availability)
    is_substitute BOOLEAN NOT NULL DEFAULT false,

    -- Active flag
    is_active BOOLEAN NOT NULL DEFAULT true,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
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

CREATE INDEX idx_availability_windows_player ON availability_windows(player_id)
    WHERE player_id IS NOT NULL;
CREATE INDEX idx_availability_windows_team ON availability_windows(team_season_id)
    WHERE team_season_id IS NOT NULL;
CREATE INDEX idx_availability_windows_tournament ON availability_windows(tournament_id)
    WHERE tournament_id IS NOT NULL;

CREATE TRIGGER availability_windows_updated_at
    BEFORE UPDATE ON availability_windows
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE availability_windows IS 'Recurring weekly availability for players/teams';
```

### availability_exceptions

```sql
CREATE TABLE availability_exceptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Owner (one of these set)
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    team_season_id UUID REFERENCES league_team_seasons(id) ON DELETE CASCADE,

    -- Exception date
    exception_date DATE NOT NULL,

    -- Type: blocked (unavailable) or override (available)
    exception_type VARCHAR(16) NOT NULL DEFAULT 'blocked',

    -- For override type: specific time window
    start_minutes SMALLINT,
    end_minutes SMALLINT,

    -- Reason
    reason VARCHAR(256),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT availability_exceptions_owner_check CHECK (
        (player_id IS NOT NULL)::int + (team_season_id IS NOT NULL)::int = 1
    ),
    CONSTRAINT availability_exceptions_type_check CHECK (
        exception_type IN ('blocked', 'override')
    ),
    CONSTRAINT availability_exceptions_override_time CHECK (
        exception_type = 'blocked' OR
        (start_minutes IS NOT NULL AND end_minutes IS NOT NULL)
    )
);

CREATE INDEX idx_availability_exceptions_player ON availability_exceptions(player_id, exception_date)
    WHERE player_id IS NOT NULL;
CREATE INDEX idx_availability_exceptions_team ON availability_exceptions(team_season_id, exception_date)
    WHERE team_season_id IS NOT NULL;

COMMENT ON TABLE availability_exceptions IS 'Date-specific availability overrides';
```

---

## Domain Entities

### ScheduleProposal

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    ScheduleProposalId, TournamentMatchId, TournamentRegistrationId, UserId,
};

/// A schedule proposal for a match.
#[derive(Debug, Clone)]
pub struct ScheduleProposal {
    pub id: ScheduleProposalId,
    pub match_id: TournamentMatchId,

    /// Who proposed this schedule
    pub proposed_by_registration_id: TournamentRegistrationId,
    pub proposed_by_user_id: UserId,

    /// Proposed time slots (1-5 options)
    pub proposed_times: Vec<DateTime<Utc>>,

    /// Selected time (when accepted)
    pub selected_time: Option<DateTime<Utc>>,

    /// Response tracking
    pub responded_at: Option<DateTime<Utc>>,
    pub responded_by_user_id: Option<UserId>,

    /// Counter-proposal reference
    pub counter_proposal_id: Option<ScheduleProposalId>,

    /// Current status
    pub status: ProposalStatus,

    /// When this proposal expires
    pub expires_at: DateTime<Utc>,

    /// Admin notes
    pub notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
    CounterProposed,
    Expired,
    Cancelled,
}

impl ProposalStatus {
    /// Check if a transition is valid
    pub fn can_transition_to(&self, target: ProposalStatus) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Accepted)
            | (Self::Pending, Self::Rejected)
            | (Self::Pending, Self::CounterProposed)
            | (Self::Pending, Self::Expired)
            | (Self::Pending, Self::Cancelled)
        )
    }
}
```

### AvailabilityWindow

```rust
use chrono::{DateTime, NaiveDate, Utc};
use portal_core::ids::{
    AvailabilityWindowId, LeagueTeamSeasonId, PlayerId, TournamentId,
};

/// A recurring weekly availability window.
#[derive(Debug, Clone)]
pub struct AvailabilityWindow {
    pub id: AvailabilityWindowId,

    /// Owner (player or team)
    pub owner: AvailabilityOwner,

    /// Day of week (0 = Sunday, 6 = Saturday)
    pub day_of_week: u8,

    /// Start time (minutes from midnight, 0-1439)
    pub start_minutes: u16,

    /// End time (minutes from midnight, 1-1440)
    pub end_minutes: u16,

    /// Timezone for interpreting the times
    pub timezone: String,

    /// Effective date range
    pub effective_from: Option<NaiveDate>,
    pub effective_until: Option<NaiveDate>,

    /// Tournament-specific availability
    pub tournament_id: Option<TournamentId>,

    /// Whether this is substitute availability
    pub is_substitute: bool,

    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum AvailabilityOwner {
    Player(PlayerId),
    Team(LeagueTeamSeasonId),
}

impl AvailabilityWindow {
    /// Get start time as hours:minutes string
    pub fn start_time_str(&self) -> String {
        format!("{:02}:{:02}", self.start_minutes / 60, self.start_minutes % 60)
    }

    /// Get end time as hours:minutes string
    pub fn end_time_str(&self) -> String {
        format!("{:02}:{:02}", self.end_minutes / 60, self.end_minutes % 60)
    }

    /// Check if a given UTC datetime falls within this window
    pub fn contains(&self, dt: DateTime<Utc>, exceptions: &[AvailabilityException]) -> bool {
        // Convert to local timezone
        let tz: chrono_tz::Tz = self.timezone.parse().unwrap_or(chrono_tz::UTC);
        let local = dt.with_timezone(&tz);

        // Check day of week
        if local.weekday().num_days_from_sunday() as u8 != self.day_of_week {
            return false;
        }

        // Check time
        let minutes = local.hour() as u16 * 60 + local.minute() as u16;
        if minutes < self.start_minutes || minutes >= self.end_minutes {
            return false;
        }

        // Check exceptions
        let date = local.date_naive();
        for exc in exceptions {
            if exc.exception_date == date {
                match exc.exception_type {
                    ExceptionType::Blocked => return false,
                    ExceptionType::Override { start, end } => {
                        // Override provides specific available time
                        return minutes >= start && minutes < end;
                    }
                }
            }
        }

        true
    }
}
```

### AvailabilityException

```rust
use chrono::{DateTime, NaiveDate, Utc};
use portal_core::ids::{
    AvailabilityExceptionId, LeagueTeamSeasonId, PlayerId,
};

/// A date-specific availability exception.
#[derive(Debug, Clone)]
pub struct AvailabilityException {
    pub id: AvailabilityExceptionId,
    pub owner: AvailabilityOwner,
    pub exception_date: NaiveDate,
    pub exception_type: ExceptionType,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ExceptionType {
    /// Completely unavailable on this date
    Blocked,
    /// Available during specific time only
    Override {
        start: u16,  // minutes from midnight
        end: u16,
    },
}
```

---

## State Machine

### Proposal State Transitions

```
                          ┌──────────────┐
                          │   Pending    │
                          └──────┬───────┘
                                 │
        ┌───────────────┬────────┼────────┬───────────────┐
        ▼               ▼        ▼        ▼               ▼
┌───────────────┐ ┌───────────┐ ┌─────────────────┐ ┌───────────┐
│   Accepted    │ │ Rejected  │ │CounterProposed  │ │  Expired  │
│               │ │           │ │                 │ │           │
│ selected_time │ │           │ │ new proposal    │ │ timeout   │
│   is set      │ │           │ │   created       │ │  reached  │
└───────────────┘ └───────────┘ └─────────────────┘ └───────────┘
        │                              │
        ▼                              ▼
┌───────────────┐             ┌─────────────────┐
│ Match status  │             │ New proposal    │
│ → Scheduled   │             │ becomes pending │
└───────────────┘             └─────────────────┘
```

### Transition Rules

| From | To | Condition | Side Effects |
|------|----|-----------|--------------|
| Pending | Accepted | Opponent selects time | Match scheduled_at set, status → Scheduled |
| Pending | Rejected | Opponent declines | None |
| Pending | CounterProposed | Opponent creates counter | New proposal linked |
| Pending | Expired | expires_at reached | None |
| Pending | Cancelled | Proposer cancels | None |

---

## Service Design

### SchedulingService

```rust
pub struct SchedulingService<SPR, TMR, TRR>
where
    SPR: ScheduleProposalRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    proposal_repo: Arc<SPR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    default_proposal_ttl: Duration,  // e.g., 48 hours
}

impl<SPR, TMR, TRR> SchedulingService<SPR, TMR, TRR>
where
    SPR: ScheduleProposalRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new schedule proposal.
    ///
    /// # Errors
    /// - `MatchNotFound` if match doesn't exist
    /// - `NotAuthorized` if user is not part of the match
    /// - `InvalidState` if match cannot be scheduled
    /// - `ValidationError` if proposed times are invalid
    pub async fn propose_schedule(
        &self,
        match_id: TournamentMatchId,
        proposed_times: Vec<DateTime<Utc>>,
        proposed_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Accept a schedule proposal, selecting one of the proposed times.
    ///
    /// This also updates the match to Scheduled status.
    pub async fn accept_proposal(
        &self,
        proposal_id: ScheduleProposalId,
        selected_time: DateTime<Utc>,
        accepted_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Reject a schedule proposal.
    pub async fn reject_proposal(
        &self,
        proposal_id: ScheduleProposalId,
        rejected_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Counter-propose with new times.
    ///
    /// Creates a new proposal and links it to the original.
    pub async fn counter_propose(
        &self,
        original_proposal_id: ScheduleProposalId,
        new_times: Vec<DateTime<Utc>>,
        proposed_by: UserId,
    ) -> Result<ScheduleProposal, DomainError>;

    /// Admin directly schedules a match.
    ///
    /// Bypasses the proposal workflow entirely.
    pub async fn admin_schedule(
        &self,
        match_id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
        admin_id: UserId,
    ) -> Result<TournamentMatch, DomainError>;

    /// Expire pending proposals that have passed their deadline.
    ///
    /// Called by background job.
    pub async fn expire_proposals(&self) -> Result<Vec<ScheduleProposal>, DomainError>;

    /// Get active proposal for a match.
    pub async fn get_active_proposal(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ScheduleProposal>, DomainError>;

    /// Get proposal history for a match.
    pub async fn get_proposal_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ScheduleProposal>, DomainError>;
}
```

### AvailabilityService

```rust
pub struct AvailabilityService<AWR, AER, PR, LTSR>
where
    AWR: AvailabilityWindowRepository,
    AER: AvailabilityExceptionRepository,
    PR: PlayerRepository,
    LTSR: LeagueTeamSeasonRepository,
{
    window_repo: Arc<AWR>,
    exception_repo: Arc<AER>,
    player_repo: Arc<PR>,
    team_season_repo: Arc<LTSR>,
}

impl<AWR, AER, PR, LTSR> AvailabilityService<AWR, AER, PR, LTSR>
where
    AWR: AvailabilityWindowRepository,
    AER: AvailabilityExceptionRepository,
    PR: PlayerRepository,
    LTSR: LeagueTeamSeasonRepository,
{
    // --- Window Management ---

    /// Set availability windows for a player.
    ///
    /// Replaces existing windows with new set.
    pub async fn set_player_availability(
        &self,
        player_id: PlayerId,
        windows: Vec<CreateAvailabilityWindow>,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Set availability windows for a team.
    pub async fn set_team_availability(
        &self,
        team_season_id: LeagueTeamSeasonId,
        windows: Vec<CreateAvailabilityWindow>,
        is_substitute: bool,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Get availability windows for a player.
    pub async fn get_player_availability(
        &self,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    /// Get availability windows for a team.
    pub async fn get_team_availability(
        &self,
        team_season_id: LeagueTeamSeasonId,
        include_substitutes: bool,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    // --- Exception Management ---

    /// Add an availability exception (blocked date or override).
    pub async fn add_exception(
        &self,
        owner: AvailabilityOwner,
        exception_date: NaiveDate,
        exception_type: ExceptionType,
        reason: Option<String>,
    ) -> Result<AvailabilityException, DomainError>;

    /// Remove an availability exception.
    pub async fn remove_exception(
        &self,
        exception_id: AvailabilityExceptionId,
    ) -> Result<(), DomainError>;

    /// Get exceptions for an owner within a date range.
    pub async fn get_exceptions(
        &self,
        owner: &AvailabilityOwner,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<AvailabilityException>, DomainError>;

    // --- Suggestions ---

    /// Suggest optimal match times based on both participants' availability.
    ///
    /// Returns up to `max_suggestions` time slots where both teams are available.
    pub async fn suggest_match_times(
        &self,
        match_id: TournamentMatchId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max_suggestions: usize,
    ) -> Result<Vec<SuggestedTime>, DomainError>;

    /// Check if a specific time works for both match participants.
    pub async fn check_availability(
        &self,
        match_id: TournamentMatchId,
        proposed_time: DateTime<Utc>,
    ) -> Result<AvailabilityCheck, DomainError>;
}

#[derive(Debug, Clone)]
pub struct CreateAvailabilityWindow {
    pub day_of_week: u8,
    pub start_minutes: u16,
    pub end_minutes: u16,
    pub timezone: String,
}

#[derive(Debug, Clone)]
pub struct SuggestedTime {
    pub time: DateTime<Utc>,
    pub participant1_available: bool,
    pub participant2_available: bool,
    pub overlap_minutes: u16,  // Duration of overlapping availability
}

#[derive(Debug, Clone)]
pub struct AvailabilityCheck {
    pub time: DateTime<Utc>,
    pub participant1: ParticipantAvailability,
    pub participant2: ParticipantAvailability,
}

#[derive(Debug, Clone)]
pub struct ParticipantAvailability {
    pub is_available: bool,
    pub reason: Option<String>,  // If unavailable, why
}
```

---

## Repository Interfaces

### ScheduleProposalRepository

```rust
#[async_trait]
pub trait ScheduleProposalRepository: Send + Sync + 'static {
    async fn create(&self, proposal: &ScheduleProposal) -> Result<ScheduleProposal, DomainError>;

    async fn find_by_id(
        &self,
        id: ScheduleProposalId,
    ) -> Result<Option<ScheduleProposal>, DomainError>;

    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ScheduleProposal>, DomainError>;

    async fn find_pending_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ScheduleProposal>, DomainError>;

    async fn update(&self, proposal: &ScheduleProposal) -> Result<ScheduleProposal, DomainError>;

    async fn find_expired(&self, before: DateTime<Utc>) -> Result<Vec<ScheduleProposal>, DomainError>;
}
```

### AvailabilityWindowRepository

```rust
#[async_trait]
pub trait AvailabilityWindowRepository: Send + Sync + 'static {
    async fn create(&self, window: &AvailabilityWindow) -> Result<AvailabilityWindow, DomainError>;

    async fn find_by_player(
        &self,
        player_id: PlayerId,
        tournament_id: Option<TournamentId>,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    async fn find_by_team(
        &self,
        team_season_id: LeagueTeamSeasonId,
        include_substitutes: bool,
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;

    async fn delete_by_player(&self, player_id: PlayerId) -> Result<u64, DomainError>;

    async fn delete_by_team(&self, team_season_id: LeagueTeamSeasonId) -> Result<u64, DomainError>;

    async fn bulk_create(
        &self,
        windows: &[AvailabilityWindow],
    ) -> Result<Vec<AvailabilityWindow>, DomainError>;
}
```

---

## API Endpoints

### Schedule Proposals

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/propose

Create a schedule proposal.

**Request**:
```json
{
  "proposed_times": [
    "2025-01-15T19:00:00Z",
    "2025-01-16T20:00:00Z",
    "2025-01-17T18:00:00Z"
  ]
}
```

**Response** (201 Created):
```json
{
  "data": {
    "id": "...",
    "match_id": "...",
    "proposed_by": {
      "registration_id": "...",
      "name": "Team Alpha"
    },
    "proposed_times": ["..."],
    "status": "pending",
    "expires_at": "2025-01-14T19:00:00Z"
  }
}
```

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/accept

Accept a schedule proposal.

**Request**:
```json
{
  "proposal_id": "...",
  "selected_time": "2025-01-15T19:00:00Z"
}
```

**Response** (200 OK):
```json
{
  "data": {
    "proposal": {
      "id": "...",
      "status": "accepted",
      "selected_time": "2025-01-15T19:00:00Z"
    },
    "match": {
      "id": "...",
      "status": "scheduled",
      "scheduled_at": "2025-01-15T19:00:00Z"
    }
  }
}
```

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/reject

Reject a schedule proposal.

**Request**:
```json
{
  "proposal_id": "..."
}
```

#### POST /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/counter

Counter-propose with new times.

**Request**:
```json
{
  "original_proposal_id": "...",
  "proposed_times": [
    "2025-01-18T19:00:00Z",
    "2025-01-19T20:00:00Z"
  ]
}
```

#### POST /v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule

Admin direct scheduling.

**Request**:
```json
{
  "scheduled_at": "2025-01-15T19:00:00Z",
  "notes": "Scheduled by admin due to deadline"
}
```

#### GET /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/proposals

Get proposal history for a match.

#### GET /v1/tournaments/{tournament_id}/matches/{match_id}/schedule/suggested-times

Get suggested times based on availability.

**Query Parameters**:
- `from`: Start of search range (ISO 8601)
- `to`: End of search range (ISO 8601)
- `limit`: Maximum suggestions (default 5)

---

### Availability

#### GET /v1/players/{player_id}/availability

Get player availability windows.

**Query Parameters**:
- `tournament_id`: Optional, for tournament-specific availability

**Response**:
```json
{
  "data": {
    "windows": [
      {
        "id": "...",
        "day_of_week": 6,
        "day_name": "Saturday",
        "start_time": "14:00",
        "end_time": "18:00",
        "timezone": "America/New_York"
      }
    ],
    "exceptions": [
      {
        "id": "...",
        "date": "2025-01-20",
        "type": "blocked",
        "reason": "Holiday"
      }
    ]
  }
}
```

#### PUT /v1/players/me/availability

Set player availability (replaces existing).

**Request**:
```json
{
  "timezone": "America/New_York",
  "windows": [
    {
      "day_of_week": 6,
      "start_time": "14:00",
      "end_time": "18:00"
    },
    {
      "day_of_week": 0,
      "start_time": "12:00",
      "end_time": "22:00"
    }
  ]
}
```

#### POST /v1/players/me/availability/exceptions

Add availability exception.

**Request**:
```json
{
  "date": "2025-01-20",
  "type": "blocked",
  "reason": "Holiday travel"
}
```

#### DELETE /v1/players/me/availability/exceptions/{exception_id}

Remove availability exception.

#### GET /v1/leagues/{league_id}/teams/{team_id}/availability

Get team availability.

#### PUT /v1/leagues/{league_id}/teams/{team_id}/availability

Set team availability.

---

## Edge Cases

### Timezone Handling

1. **Storage**: All times stored as UTC in database
2. **Availability Windows**: Stored with timezone, converted to UTC for comparison
3. **API Input**: Accept ISO 8601 with timezone or assume UTC
4. **API Output**: Return UTC with timezone hint for display

### Deadline Enforcement

1. **Scheduling Deadline**: Configurable per tournament (`schedule_deadline` field on match)
2. **Proposal Expiry**: Default 48 hours, configurable per tournament
3. **Admin Escalation**: If no agreement by deadline, admin notified
4. **Auto-Forfeit**: Optional tournament setting to auto-forfeit if not scheduled

### Admin Override

Admins can:
- Directly schedule any match
- Cancel pending proposals
- Extend scheduling deadlines
- Override expired matches

### Conflict Resolution

If both teams propose simultaneously:
- First proposal (by created_at) takes precedence
- Second becomes a counter-proposal automatically

---

## Error Handling

### New Error Types

```rust
pub enum SchedulingError {
    /// Match is not in a state that allows scheduling
    MatchNotSchedulable(TournamentMatchId),

    /// User is not a participant in this match
    NotMatchParticipant(UserId, TournamentMatchId),

    /// Proposal has already expired
    ProposalExpired(ScheduleProposalId),

    /// Proposal is not pending
    ProposalNotPending(ScheduleProposalId),

    /// Selected time not in proposal
    TimeNotInProposal(DateTime<Utc>),

    /// Proposed time is in the past
    TimeInPast(DateTime<Utc>),

    /// Proposed time is past schedule deadline
    PastScheduleDeadline(TournamentMatchId),

    /// Too many proposed times (max 5)
    TooManyProposedTimes(usize),

    /// Invalid availability window
    InvalidAvailabilityWindow(String),
}
```

---

## Testing Notes

### Unit Tests

- Proposal state transitions
- Availability window time calculations
- Timezone conversions
- Overlap detection algorithm

### Integration Tests

```
test_propose_schedule_success
test_propose_schedule_not_participant
test_propose_schedule_match_not_ready
test_accept_proposal_success
test_accept_proposal_wrong_time
test_reject_proposal_success
test_counter_propose_success
test_proposal_expiry
test_admin_schedule_override
test_availability_window_crud
test_availability_exception_blocks_window
test_suggest_times_with_overlap
test_suggest_times_no_overlap
```

### Edge Case Tests

```
test_simultaneous_proposals
test_proposal_after_deadline
test_timezone_boundary_availability
test_availability_spanning_midnight
test_exception_override_window
```
