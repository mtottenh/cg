//! Database row types for PUGs and ad-hoc teams.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Row from the `pugs` table.
#[derive(Debug, Clone, FromRow)]
pub struct PugRow {
    pub id: Uuid,
    pub game_id: Uuid,
    pub created_by_user_id: Uuid,
    pub join_code: String,
    pub status: String,
    pub match_format: String,
    pub map_selection_mode: String,
    pub side_selection_mode: String,
    pub team_size: i32,
    pub region: Option<String>,
    pub map_pool: Option<Vec<String>>,
    pub listed: bool,
    pub tournament_id: Option<Uuid>,
    pub match_id: Option<Uuid>,
    pub winner_team: Option<i16>,
    pub team1_score: Option<i32>,
    pub team2_score: Option<i32>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Row from `pug_players` joined with `players` for display info.
#[derive(Debug, Clone, FromRow)]
pub struct PugPlayerRow {
    pub pug_id: Uuid,
    pub player_id: Uuid,
    pub user_id: Option<Uuid>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub has_steam_id: bool,
    pub team: Option<i16>,
    pub is_captain: bool,
    pub joined_at: DateTime<Utc>,
}

/// Row from `pug_wheel_entries` joined with `players` for the nominator name.
#[derive(Debug, Clone, FromRow)]
pub struct PugWheelEntryRow {
    pub pug_id: Uuid,
    pub player_id: Uuid,
    pub player_name: String,
    pub map_id: String,
    pub created_at: DateTime<Utc>,
}

/// Row from the `pug_wheel_spins` table.
#[derive(Debug, Clone, FromRow)]
pub struct PugWheelSpinRow {
    pub id: Uuid,
    pub pug_id: Uuid,
    pub game_number: i32,
    pub entries: serde_json::Value,
    pub winner_map_id: String,
    pub spin_seed: i64,
    pub spun_by_player_id: Option<Uuid>,
    pub spun_at: DateTime<Utc>,
}

/// Row from the `tournament_adhoc_teams` table.
#[derive(Debug, Clone, FromRow)]
pub struct AdhocTeamRow {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Row from the `tournament_adhoc_team_members` table.
#[derive(Debug, Clone, FromRow)]
pub struct AdhocTeamMemberRow {
    pub adhoc_team_id: Uuid,
    pub player_id: Uuid,
    pub is_captain: bool,
}
