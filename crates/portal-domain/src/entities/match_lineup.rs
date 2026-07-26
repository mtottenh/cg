//! Match lineup domain entities.
//!
//! A lineup records *who actually played* a match, per registration — distinct
//! from the roster (who is *eligible*). Two-phase (see `docs/lineup-design.md`):
//!
//! - **Provisional** (`source = Declared`): a captain's promise, declared once at
//!   check-in / pick-ban, match-level (`game_number = None`).
//! - **Authoritative** (`source = Demo`): derived per-map from the demo after it is
//!   played. This is what counts for stats, awards, and eligibility enforcement.

use chrono::{DateTime, Utc};
use portal_core::types::{LineupSource, LineupStatus, ParticipationStatus};
use portal_core::{
    MatchLineupId, MatchLineupPlayerId, PlayerId, TournamentMatchId, TournamentRegistrationId,
    UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// MATCH LINEUP
// =============================================================================

/// A lineup for one registration in one match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchLineup {
    pub id: MatchLineupId,
    pub match_id: TournamentMatchId,
    pub registration_id: TournamentRegistrationId,
    pub status: LineupStatus,
    /// User who declared/last-edited the provisional lineup (None for demo-only).
    pub declared_by: Option<UserId>,
    pub declared_at: Option<DateTime<Utc>>,
    /// Stamped when the match starts (`PickBan`/`InProgress`); read-only thereafter.
    pub locked_at: Option<DateTime<Utc>>,
    /// True when the declared lineup is below the required team size.
    pub short_handed: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MatchLineup {
    /// Whether the lineup may still be edited.
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.locked_at.is_none() && self.status.is_editable()
    }

    /// Whether the lineup is visible to the opposing registration (§0 Q3).
    #[must_use]
    pub fn is_opponent_visible(&self) -> bool {
        self.status == LineupStatus::Locked
    }
}

// =============================================================================
// MATCH LINEUP PLAYER
// =============================================================================

/// A single player row within a lineup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchLineupPlayer {
    pub id: MatchLineupPlayerId,
    pub lineup_id: MatchLineupId,
    pub player_id: PlayerId,
    pub source: LineupSource,
    /// Per-map for demo rows; `None` for match-level declared rows.
    pub game_number: Option<i32>,
    /// For demo rows: auto-set when the player is not on the team roster.
    pub is_substitute: bool,
    /// Snapshot of roster membership at declaration/ingestion time.
    pub was_rostered: bool,
    pub participation_status: ParticipationStatus,
    pub created_at: DateTime<Utc>,
}

/// A lineup together with its player rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchLineupWithPlayers {
    pub lineup: MatchLineup,
    pub players: Vec<MatchLineupPlayer>,
}

// =============================================================================
// COMMAND TYPES
// =============================================================================

/// A single player to place in a lineup, with its rostered-snapshot resolved.
#[derive(Debug, Clone)]
pub struct LineupPlayerInput {
    pub player_id: PlayerId,
    /// Whether the player is currently on the team roster (snapshot).
    pub was_rostered: bool,
    /// Per-map (demo rows) or `None` (declared rows).
    pub game_number: Option<i32>,
    pub participation_status: ParticipationStatus,
}

/// Command to declare/replace a provisional (declared) lineup for a registration.
#[derive(Debug, Clone)]
pub struct DeclareLineupCommand {
    pub match_id: TournamentMatchId,
    pub registration_id: TournamentRegistrationId,
    pub players: Vec<LineupPlayerInput>,
    pub declared_by: UserId,
    /// If true, mark the lineup `submitted`; otherwise leave it `draft`.
    pub submit: bool,
    pub short_handed: bool,
    pub notes: Option<String>,
}
