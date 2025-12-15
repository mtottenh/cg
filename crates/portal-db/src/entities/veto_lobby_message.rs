//! Veto lobby message database entities.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `veto_lobby_messages` table.
#[derive(Debug, Clone, FromRow)]
pub struct VetoLobbyMessageRow {
    pub id: Uuid,
    pub match_id: Uuid,
    pub veto_session_id: Option<Uuid>,
    pub author_user_id: Uuid,
    pub author_registration_id: Option<Uuid>,
    pub message_type: String,
    pub content: String,
    pub team_registration_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Data for creating a new veto lobby message.
#[derive(Debug, Clone)]
pub struct NewVetoLobbyMessage {
    pub match_id: Uuid,
    pub veto_session_id: Option<Uuid>,
    pub author_user_id: Uuid,
    pub author_registration_id: Option<Uuid>,
    pub message_type: String,
    pub content: String,
    pub team_registration_id: Option<Uuid>,
}
