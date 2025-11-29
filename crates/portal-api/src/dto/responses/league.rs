//! League response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::league::{
    League, LeagueAccessType, LeagueInvitation, LeagueInvitationStatus, LeagueInvitationType,
    LeagueMemberWithUser, LeagueMembershipType, LeagueStatus, UserLeagueMembership,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Response DTO for a league.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueResponse {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    pub access_type: String,
    pub status: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<League> for LeagueResponse {
    fn from(league: League) -> Self {
        Self {
            id: league.id.to_string(),
            game_id: league.game_id.to_string(),
            name: league.name,
            slug: league.slug,
            description: league.description,
            logo_url: league.logo_url,
            access_type: league.access_type.as_str().to_string(),
            status: league.status.as_str().to_string(),
            created_by: league.created_by.to_string(),
            created_at: league.created_at,
            updated_at: league.updated_at,
        }
    }
}

/// Response DTO for a league member (with user info, for listings).
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueMemberResponse {
    pub id: String,
    pub league_id: String,
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,
}

impl From<LeagueMemberWithUser> for LeagueMemberResponse {
    fn from(member: LeagueMemberWithUser) -> Self {
        Self {
            id: member.id.to_string(),
            league_id: member.league_id.to_string(),
            user_id: member.user_id.to_string(),
            username: member.username,
            email: member.email,
            membership_type: member.membership_type.as_str().to_string(),
            joined_at: member.joined_at,
        }
    }
}

/// Simpler response DTO for member operations (join, role update, etc.).
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueMemberBasicResponse {
    pub id: String,
    pub league_id: String,
    pub user_id: String,
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,
}

impl From<portal_domain::entities::league::LeagueMember> for LeagueMemberBasicResponse {
    fn from(member: portal_domain::entities::league::LeagueMember) -> Self {
        Self {
            id: member.id.to_string(),
            league_id: member.league_id.to_string(),
            user_id: member.user_id.to_string(),
            membership_type: member.membership_type.as_str().to_string(),
            joined_at: member.joined_at,
        }
    }
}

/// Response DTO for a user's league membership.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserLeagueMembershipResponse {
    pub league_id: String,
    pub league_name: String,
    pub league_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub league_logo_url: Option<String>,
    pub game_id: String,
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,
}

impl From<UserLeagueMembership> for UserLeagueMembershipResponse {
    fn from(membership: UserLeagueMembership) -> Self {
        Self {
            league_id: membership.league_id.to_string(),
            league_name: membership.league_name,
            league_slug: membership.league_slug,
            league_logo_url: membership.league_logo_url,
            game_id: membership.game_id.to_string(),
            membership_type: membership.membership_type.as_str().to_string(),
            joined_at: membership.joined_at,
        }
    }
}

/// Response DTO for a league invitation.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeagueInvitationResponse {
    pub id: String,
    pub league_id: String,
    pub user_id: String,
    pub invitation_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<LeagueInvitation> for LeagueInvitationResponse {
    fn from(inv: LeagueInvitation) -> Self {
        Self {
            id: inv.id.to_string(),
            league_id: inv.league_id.to_string(),
            user_id: inv.user_id.to_string(),
            invitation_type: inv.invitation_type.as_str().to_string(),
            status: inv.status.as_str().to_string(),
            message: inv.message,
            invited_by: inv.invited_by.map(|u| u.to_string()),
            responded_by: inv.responded_by.map(|u| u.to_string()),
            responded_at: inv.responded_at,
            expires_at: inv.expires_at,
            created_at: inv.created_at,
        }
    }
}
