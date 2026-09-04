//! League response DTOs.

use super::tournament::EligibilityRestrictionsResponse;
use chrono::{DateTime, Utc};
use portal_domain::entities::eligibility::EligibilityRestrictions;
use portal_domain::entities::league::{
    League, LeagueInvitation, LeagueMemberWithUser, LeagueStatus, UserLeagueMembership,
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
    // Typed as the enum so the schema publishes its permitted values and
    // clients get a union, not `string` (P-112/P-178). Wire-compatible: serde
    // snake_case matches the old `as_str()` strings.
    pub status: LeagueStatus,
    /// When the league was archived, or absent while it is live. Archived
    /// leagues are hidden from player-facing listings, along with their
    /// seasons, teams and tournaments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    /// League configuration including entry requirements.
    /// Entry requirements are stored under the `"eligibility"` key.
    pub settings: serde_json::Value,
    /// Entry requirements, projected as the same typed shape tournaments
    /// expose — clients no longer need to spelunk `settings` JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility_restrictions: Option<EligibilityRestrictionsResponse>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<League> for LeagueResponse {
    fn from(league: League) -> Self {
        let restrictions = EligibilityRestrictions::from_settings(&league.settings);
        let eligibility_restrictions = restrictions
            .has_restrictions()
            .then(|| EligibilityRestrictionsResponse::from(restrictions));
        Self {
            id: league.id.to_string(),
            game_id: league.game_id.to_string(),
            name: league.name,
            slug: league.slug,
            description: league.description,
            logo_url: league.logo_url,
            access_type: league.access_type.as_str().to_string(),
            status: league.status,
            archived_at: league.archived_at,
            settings: league.settings,
            eligibility_restrictions,
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
    /// Member email. **Only populated for callers holding
    /// `league.members.manage` on this league.** It was previously always
    /// present on an endpoint that required no authentication at all, so any
    /// anonymous caller could enumerate member email addresses (P-37).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub membership_type: String,
    pub joined_at: DateTime<Utc>,
}

impl LeagueMemberResponse {
    /// Build a member row, including the email only when the caller is
    /// authorised to see it. See the `email` field docs (P-37).
    #[must_use]
    pub fn from_member(member: LeagueMemberWithUser, include_email: bool) -> Self {
        let email = member.email.clone();
        let mut this = Self::from(member);
        this.email = include_email.then_some(email);
        this
    }
}

impl From<LeagueMemberWithUser> for LeagueMemberResponse {
    fn from(member: LeagueMemberWithUser) -> Self {
        Self {
            id: member.id.to_string(),
            league_id: member.league_id.to_string(),
            user_id: member.user_id.to_string(),
            username: member.username,
            // Omitted by default -- opt in via `from_member` (P-37).
            email: None,
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
    /// Status of the league itself. Memberships are returned for every
    /// status — a league admin has to be able to see (and restore) a league
    /// that has been archived. Typed as the enum so clients get a union
    /// rather than `string` (as `LeagueResponse::status` already is).
    pub league_status: LeagueStatus,
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
            league_status: membership.league_status,
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
    /// Name of the league the invitation is for. Without it, two pending
    /// invitations are indistinguishable on the invitations page and
    /// accept/decline is a blind choice (P-38) — team invitations already
    /// carry `team_name`/`league_name`, so the asymmetry was unintended.
    pub league_name: String,
    pub user_id: String,
    /// Username of the invited/applying user. Always present.
    ///
    /// P-115: the admin invitations and applications tables had only `user_id`
    /// to show and truncated it to 8 characters — and UUID v7 prefixes are
    /// timestamps, so rows created seconds apart were indistinguishable rather
    /// than merely cryptic. `LeagueMemberResponse` has carried `username`
    /// since it existed; this closes the asymmetry.
    pub username: String,
    /// The user's display name, when they have a player profile. This is the
    /// name the invite search shows the organiser, so it is what the resulting
    /// row should lead with; `username` is the fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
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

impl LeagueInvitationResponse {
    /// Build the response from a domain invitation plus the league's name.
    ///
    /// Deliberately not a `From<LeagueInvitation>` impl: the domain entity
    /// does not carry the league name, and forcing every call site to supply
    /// it keeps the P-38 fix compile-checked (a new endpoint cannot silently
    /// ship a nameless invitation).
    #[must_use]
    pub fn from_invitation(inv: LeagueInvitation, league_name: String) -> Self {
        Self {
            id: inv.id.to_string(),
            league_id: inv.league_id.to_string(),
            league_name,
            user_id: inv.user_id.to_string(),
            username: inv.username,
            display_name: inv.display_name,
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
