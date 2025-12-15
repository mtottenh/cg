//! Veto delegate database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `veto_delegates` table.
#[derive(Debug, Clone, FromRow)]
pub struct VetoDelegateRow {
    pub id: Uuid,
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub delegated_by_user_id: Uuid,
    pub delegated_by_role: String,
    pub tournament_id: Option<Uuid>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Data for creating a new veto delegate.
#[derive(Debug, Clone)]
pub struct NewVetoDelegate {
    pub team_season_id: Uuid,
    pub player_id: Uuid,
    pub delegated_by_user_id: Uuid,
    pub delegated_by_role: String,
    pub tournament_id: Option<Uuid>,
}
