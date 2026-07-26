//! Pick-Up Game (PUG) domain entities.
//!
//! A PUG owns the social/gathering phase of a one-off match. At lock-in it
//! materializes as a hidden single-match container tournament (kind = pug)
//! with two ad-hoc teams, after which the standard match pipeline (veto,
//! server reservation, results, demos) takes over. See
//! `migrations/0091_pugs.sql` and the design doc for the full model.

use chrono::{DateTime, Utc};
use portal_core::types::{PugMapSelectionMode, PugStatus};
use portal_core::{
    AdhocTeamId, GameId, MatchFormat, PlayerId, PugId, PugWheelSpinId, SideSelectionMode,
    TournamentId, TournamentMatchId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// PUG
// =============================================================================

/// A pick-up game lobby.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pug {
    pub id: PugId,
    pub game_id: GameId,
    pub created_by_user_id: UserId,
    /// Ephemeral invite code; rotatable, dead once the pug leaves gathering.
    pub join_code: String,
    pub status: PugStatus,
    pub match_format: MatchFormat,
    pub map_selection_mode: PugMapSelectionMode,
    pub side_selection_mode: SideSelectionMode,
    /// Players per team (defaults to the game's team size).
    pub team_size: i32,
    pub region: Option<String>,
    /// Veto mode: custom map pool (None = game default pool).
    pub map_pool: Option<Vec<String>>,
    /// Opt-in: show in the public open-PUGs browser.
    pub listed: bool,
    /// Set at materialization (lock).
    pub tournament_id: Option<TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    /// Denormalized at series end.
    pub winner_team: Option<i16>,
    pub team1_score: Option<i32>,
    pub team2_score: Option<i32>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Gathering TTL; the sweeper expires unlocked lobbies past this.
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Pug {
    /// Whether the lobby is still gathering (join/leave/team changes allowed).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }

    /// Whether the pug has been materialized into a match.
    #[must_use]
    pub const fn is_materialized(&self) -> bool {
        self.match_id.is_some()
    }
}

/// A player in a PUG lobby, with display info joined from players/users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PugPlayer {
    pub pug_id: PugId,
    pub player_id: PlayerId,
    pub user_id: Option<UserId>,
    /// Display name from the player profile.
    pub display_name: String,
    pub avatar_url: Option<String>,
    /// Whether the player has a linked Steam ID (required to enter the server).
    pub has_steam_id: bool,
    /// 1 or 2; None = unassigned bench.
    pub team: Option<i16>,
    pub is_captain: bool,
    pub joined_at: DateTime<Utc>,
}

/// A player's wheel nomination (wheel mode only; one per player, upserted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PugWheelEntry {
    pub pug_id: PugId,
    pub player_id: PlayerId,
    /// Display name of the nominating player (for wheel segment labels).
    pub player_name: String,
    pub map_id: String,
    pub created_at: DateTime<Utc>,
}

/// A recorded wheel spin: the audit row that also drives the deterministic
/// client animation (same seed + entries => same spin everywhere).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PugWheelSpin {
    pub id: PugWheelSpinId,
    pub pug_id: PugId,
    /// Which game of the series this spin selected (1-based).
    pub game_number: i32,
    /// Snapshot of the segments at spin time:
    /// `[{map_id, weight, nominated_by: [names]}]`.
    pub entries: serde_json::Value,
    pub winner_map_id: String,
    /// Seed broadcast to clients so every wheel animates identically.
    pub spin_seed: i64,
    pub spun_by_player_id: Option<PlayerId>,
    pub spun_at: DateTime<Utc>,
}

/// Career PUG aggregates for one player — demo-derived, entirely separate
/// from `player_game_profiles` (stats separation).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PugPlayerAggregates {
    /// Completed PUGs the player was rostered in.
    pub matches_played: i64,
    pub wins: i64,
    pub losses: i64,
    /// PUG-categorized demos with stats for this player.
    pub demos_counted: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub avg_adr: f64,
    pub avg_hs_percentage: f64,
}

// =============================================================================
// AD-HOC TEAMS
// =============================================================================

/// Ephemeral roster for an adhoc-participant tournament (PUG container).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocTeam {
    pub id: AdhocTeamId,
    pub tournament_id: TournamentId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// A member of an ad-hoc team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocTeamMember {
    pub adhoc_team_id: AdhocTeamId,
    pub player_id: PlayerId,
    pub is_captain: bool,
}
