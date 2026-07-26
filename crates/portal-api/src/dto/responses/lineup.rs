//! Match lineup response DTOs.

use chrono::{DateTime, Utc};
use portal_core::types::{LineupSource, LineupStatus, ParticipationStatus};
use portal_domain::entities::match_lineup::{
    MatchLineup, MatchLineupPlayer, MatchLineupWithPlayers,
};
use serde::Serialize;
use utoipa::ToSchema;

/// A single player row in a lineup.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchLineupPlayerResponse {
    /// Lineup-player row ID.
    pub id: String,
    /// Player ID.
    pub player_id: String,
    /// Where this row came from (declared/demo/evidence/admin).
    pub source: LineupSource,
    /// Per-map game number (demo rows); null for match-level declared rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_number: Option<i32>,
    /// Whether this player is a substitute (not on the team roster).
    pub is_substitute: bool,
    /// Snapshot of roster membership at declaration/ingestion.
    pub was_rostered: bool,
    /// How the player participated.
    pub participation_status: ParticipationStatus,
}

impl From<MatchLineupPlayer> for MatchLineupPlayerResponse {
    fn from(p: MatchLineupPlayer) -> Self {
        Self {
            id: p.id.to_string(),
            player_id: p.player_id.to_string(),
            source: p.source,
            game_number: p.game_number,
            is_substitute: p.is_substitute,
            was_rostered: p.was_rostered,
            participation_status: p.participation_status,
        }
    }
}

/// A lineup for one registration, with its player rows.
///
/// `players` is omitted (empty) when the viewer is not permitted to see it —
/// a provisional lineup is opponent-visible only once `locked` (§0 Q3).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchLineupResponse {
    /// Lineup ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Registration ID this lineup belongs to.
    pub registration_id: String,
    /// Lifecycle status (draft/submitted/locked).
    pub status: LineupStatus,
    /// User who declared the lineup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_by: Option<String>,
    /// When the provisional lineup was declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_at: Option<DateTime<Utc>>,
    /// When the lineup locked (match start).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_at: Option<DateTime<Utc>>,
    /// Whether the lineup is below the required team size.
    pub short_handed: bool,
    /// Optional captain note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Whether the caller is permitted to see the player list.
    pub players_visible: bool,
    /// The player rows (empty when `players_visible` is false).
    pub players: Vec<MatchLineupPlayerResponse>,
}

impl MatchLineupResponse {
    /// Build a response, honouring visibility. When `visible` is false the
    /// player list is withheld (opponent cannot see an unlocked lineup).
    #[must_use]
    pub fn from_with_visibility(
        lineup: MatchLineup,
        players: Vec<MatchLineupPlayer>,
        visible: bool,
    ) -> Self {
        Self {
            id: lineup.id.to_string(),
            match_id: lineup.match_id.to_string(),
            registration_id: lineup.registration_id.to_string(),
            status: lineup.status,
            declared_by: lineup.declared_by.map(|id| id.to_string()),
            declared_at: lineup.declared_at,
            locked_at: lineup.locked_at,
            short_handed: lineup.short_handed,
            notes: lineup.notes,
            players_visible: visible,
            players: if visible {
                players.into_iter().map(Into::into).collect()
            } else {
                Vec::new()
            },
        }
    }

    /// Build a fully-visible response (caller owns the lineup or it is locked).
    #[must_use]
    pub fn from_visible(wp: MatchLineupWithPlayers) -> Self {
        Self::from_with_visibility(wp.lineup, wp.players, true)
    }
}
