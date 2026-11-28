//! Match lineup response DTOs.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use portal_core::PlayerId;
use portal_core::types::{LineupSource, LineupStatus, ParticipationStatus};
use portal_domain::entities::match_lineup::{
    MatchLineup, MatchLineupPlayer, MatchLineupWithPlayers,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Shown when a lineup row's player has no profile name to show.
pub const UNKNOWN_PLAYER_NAME: &str = "Unknown player";

/// How a lineup names a player: their display name, and avatar if any.
#[derive(Debug, Clone)]
pub struct LineupPlayerIdentity {
    /// Never empty.
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Player identities for lineup responses, keyed by player.
pub type LineupPlayerNames = HashMap<PlayerId, LineupPlayerIdentity>;

/// A single player row in a lineup.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MatchLineupPlayerResponse {
    /// Lineup-player row ID.
    pub id: String,
    /// Player ID.
    pub player_id: String,
    /// The player's display name ("Unknown player" when they have none).
    pub display_name: String,
    /// The player's avatar, when they have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
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

impl MatchLineupPlayerResponse {
    /// Build a row, naming the player from `names` (falls back to
    /// [`UNKNOWN_PLAYER_NAME`] when the player is missing or unnamed).
    #[must_use]
    pub fn named(p: MatchLineupPlayer, names: &LineupPlayerNames) -> Self {
        let identity = names.get(&p.player_id);
        let display_name = identity
            .map(|i| i.display_name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or(UNKNOWN_PLAYER_NAME)
            .to_string();
        Self {
            id: p.id.to_string(),
            player_id: p.player_id.to_string(),
            display_name,
            avatar_url: identity.and_then(|i| i.avatar_url.clone()),
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
        names: &LineupPlayerNames,
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
                players
                    .into_iter()
                    .map(|p| MatchLineupPlayerResponse::named(p, names))
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    /// Build a fully-visible response (caller owns the lineup or it is locked).
    #[must_use]
    pub fn from_visible(wp: MatchLineupWithPlayers, names: &LineupPlayerNames) -> Self {
        Self::from_with_visibility(wp.lineup, wp.players, true, names)
    }
}
