//! Tournament response DTOs.

use chrono::{DateTime, Utc};
use portal_core::types::{
    BracketStatus, ProposalStatus, StageFormat, StageStatus, TournamentInvitationStatus,
    TournamentMatchStatus, TournamentRegistrationStatus, TournamentStatus,
};
use portal_domain::entities::tournament::{
    Tournament, TournamentBracket, TournamentInvitation, TournamentMatch, TournamentMatchGame,
    TournamentRegistration, TournamentStage, TournamentStanding,
};
use portal_domain::entities::{MatchStatusLog, ScheduleProposal};
use portal_domain::services::tournament::{MatchStatusDetails, RegistrationCounts};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// TOURNAMENT RESPONSES
// =============================================================================

/// Response DTO for a tournament.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentResponse {
    pub id: String,
    pub game_id: String,

    // League linkage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub league_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,

    // Identity
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,

    // Format
    pub format: String,
    pub format_settings: serde_json::Value,
    pub participant_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_size: Option<i32>,

    // Capacity
    pub min_participants: i32,
    pub max_participants: i32,

    // Registration
    pub registration_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_start: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_in_start: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_in_end: Option<DateTime<Utc>>,

    // Scheduling
    pub scheduling_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone_hint: Option<String>,

    // Match settings
    pub default_match_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_map_veto_format: Option<String>,

    // Prize pool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prize_pool: Option<serde_json::Value>,

    // Rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_url: Option<String>,

    // Additional settings (JSONB). Eligibility restrictions live under the
    // `"eligibility"` key (also projected into `eligibility_restrictions`);
    // veto side-selection mode under `"side_selection_mode"`.
    pub settings: serde_json::Value,

    // Policies
    pub withdrawal_policy: String,

    // Status
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: TournamentStatus,

    /// When this was archived, or absent while it is live. Archived rows are
    /// hidden from player-facing listings; nothing is deleted and the status
    /// above is untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,

    // Ownership
    pub created_by: String,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    // Eligibility restrictions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility_restrictions: Option<EligibilityRestrictionsResponse>,

    // Computed fields
    pub is_registration_open: bool,
    pub is_check_in_open: bool,
}

/// Eligibility restrictions configured for a tournament.
#[derive(Debug, Serialize, ToSchema)]
pub struct EligibilityRestrictionsResponse {
    /// Max current rating for any player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rating_per_player: Option<i32>,
    /// Min current rating for any player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rating_per_player: Option<i32>,
    /// Max peak (all-time high) rating for any player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_peak_rating_per_player: Option<i32>,
    /// Max average rating for any player (from history).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_avg_rating_per_player: Option<i32>,
    /// Max sum of all team members' current ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_total_rating: Option<i32>,
    /// Min sum of all team members' current ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_team_total_rating: Option<i32>,
    /// Max average of team members' current ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_team_average_rating: Option<i32>,
    /// Min average of team members' current ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_team_average_rating: Option<i32>,
    /// Only allow players in certain rank tiers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_rank_tiers: Vec<String>,
    /// Min matches played to be eligible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_matches_played: Option<i32>,
}

impl From<portal_domain::entities::eligibility::EligibilityRestrictions>
    for EligibilityRestrictionsResponse
{
    fn from(r: portal_domain::entities::eligibility::EligibilityRestrictions) -> Self {
        Self {
            max_rating_per_player: r.max_rating_per_player,
            min_rating_per_player: r.min_rating_per_player,
            max_peak_rating_per_player: r.max_peak_rating_per_player,
            max_avg_rating_per_player: r.max_avg_rating_per_player,
            max_team_total_rating: r.max_team_total_rating,
            min_team_total_rating: r.min_team_total_rating,
            max_team_average_rating: r.max_team_average_rating,
            min_team_average_rating: r.min_team_average_rating,
            allowed_rank_tiers: r.allowed_rank_tiers,
            min_matches_played: r.min_matches_played,
        }
    }
}

impl From<Tournament> for TournamentResponse {
    fn from(t: Tournament) -> Self {
        let is_registration_open = t.is_registration_open();
        let is_check_in_open = t.is_check_in_open();
        let restrictions = t.eligibility_restrictions();
        let eligibility_restrictions = restrictions
            .has_restrictions()
            .then(|| EligibilityRestrictionsResponse::from(restrictions));

        Self {
            id: t.id.to_string(),
            game_id: t.game_id.to_string(),
            league_id: t.league_id.map(|id| id.to_string()),
            season_id: t.season_id.map(|id| id.to_string()),
            name: t.name,
            slug: t.slug,
            description: t.description,
            logo_url: t.logo_url,
            banner_url: t.banner_url,
            format: t.format.to_string(),
            format_settings: t.format_settings,
            participant_type: t.participant_type.to_string(),
            team_size: t.team_size,
            min_participants: t.min_participants,
            max_participants: t.max_participants,
            registration_type: t.registration_type.to_string(),
            registration_start: t.registration_start,
            registration_end: t.registration_end,
            check_in_required: t.check_in_required,
            check_in_start: t.check_in_start,
            check_in_end: t.check_in_end,
            scheduling_mode: t.scheduling_mode.to_string(),
            starts_at: t.starts_at,
            ends_at: t.ends_at,
            timezone_hint: t.timezone_hint,
            default_match_format: t.default_match_format.to_string(),
            default_map_veto_format: t.default_map_veto_format,
            prize_pool: t.prize_pool,
            rules_url: t.rules_url,
            settings: t.settings,
            withdrawal_policy: t.withdrawal_policy.to_string(),
            archived_at: t.archived_at,
            status: t.status,
            created_by: t.created_by.to_string(),
            created_at: t.created_at,
            updated_at: t.updated_at,
            published_at: t.published_at,
            started_at: t.started_at,
            completed_at: t.completed_at,
            eligibility_restrictions,
            is_registration_open,
            is_check_in_open,
        }
    }
}

/// Summary response for listing tournaments.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentSummaryResponse {
    pub id: String,
    pub game_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub league_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    pub format: String,
    pub participant_type: String,
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: TournamentStatus,
    /// When this was archived, or absent while it is live. Archived rows are
    /// hidden from player-facing listings; nothing is deleted and the status
    /// above is untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    pub max_participants: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<DateTime<Utc>>,
    pub is_registration_open: bool,
}

impl From<Tournament> for TournamentSummaryResponse {
    fn from(t: Tournament) -> Self {
        let is_registration_open = t.is_registration_open();

        Self {
            id: t.id.to_string(),
            game_id: t.game_id.to_string(),
            league_id: t.league_id.map(|id| id.to_string()),
            season_id: t.season_id.map(|id| id.to_string()),
            name: t.name,
            slug: t.slug,
            logo_url: t.logo_url,
            format: t.format.to_string(),
            participant_type: t.participant_type.to_string(),
            status: t.status,
            archived_at: t.archived_at,
            max_participants: t.max_participants,
            starts_at: t.starts_at,
            is_registration_open,
        }
    }
}

// =============================================================================
// TOURNAMENT STAGE RESPONSES
// =============================================================================

/// Response DTO for a tournament stage.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentStageResponse {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub stage_order: i32,
    // P-112: typed as the enum so the schema publishes its permitted values and
    // clients get a union, not `string`. Wire-compatible per `wire_compat_tests`.
    pub format: StageFormat,
    pub format_settings: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advancement_count: Option<i32>,
    pub advancement_rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_veto_format: Option<String>,
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: StageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<TournamentStage> for TournamentStageResponse {
    fn from(s: TournamentStage) -> Self {
        Self {
            id: s.id.to_string(),
            tournament_id: s.tournament_id.to_string(),
            name: s.name,
            stage_order: s.stage_order,
            format: s.format,
            format_settings: s.format_settings,
            advancement_count: s.advancement_count,
            advancement_rule: s.advancement_rule.to_string(),
            match_format: s.match_format.map(|f| f.to_string()),
            map_veto_format: s.map_veto_format,
            status: s.status,
            starts_at: s.starts_at,
            ends_at: s.ends_at,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

// =============================================================================
// TOURNAMENT BRACKET RESPONSES
// =============================================================================

/// Response DTO for a tournament bracket.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentBracketResponse {
    pub id: String,
    pub stage_id: String,
    pub tournament_id: String,
    pub name: String,
    pub bracket_type: String,
    pub total_rounds: i32,
    pub current_round: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_number: Option<i32>,
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: BracketStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<TournamentBracket> for TournamentBracketResponse {
    fn from(b: TournamentBracket) -> Self {
        Self {
            id: b.id.to_string(),
            stage_id: b.stage_id.to_string(),
            tournament_id: b.tournament_id.to_string(),
            name: b.name,
            bracket_type: b.bracket_type.to_string(),
            total_rounds: b.total_rounds,
            current_round: b.current_round,
            group_number: b.group_number,
            status: b.status,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

// =============================================================================
// TOURNAMENT REGISTRATION RESPONSES
// =============================================================================

/// Response DTO for a tournament registration.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentRegistrationResponse {
    pub id: String,
    pub tournament_id: String,

    // Participant identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_season_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,

    // Display info
    pub participant_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant_logo_url: Option<String>,

    // Registration
    pub registered_by: String,
    pub registered_at: DateTime<Utc>,

    // Check-in
    pub checked_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_in_at: Option<DateTime<Utc>>,

    // Seeding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_rating: Option<i32>,

    // Status
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: TournamentRegistrationStatus,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn_at: Option<DateTime<Utc>>,
}

impl From<TournamentRegistration> for TournamentRegistrationResponse {
    fn from(r: TournamentRegistration) -> Self {
        Self {
            id: r.id.to_string(),
            tournament_id: r.tournament_id.to_string(),
            team_season_id: r.team_season_id.map(|id| id.to_string()),
            player_id: r.player_id.map(|id| id.to_string()),
            participant_name: r.participant_name,
            participant_logo_url: r.participant_logo_url,
            registered_by: r.registered_by.to_string(),
            registered_at: r.registered_at,
            checked_in: r.checked_in,
            checked_in_at: r.checked_in_at,
            seed: r.seed,
            seed_rating: r.seed_rating,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
            withdrawn_at: r.withdrawn_at,
        }
    }
}

// =============================================================================
// TOURNAMENT INVITATION RESPONSES
// =============================================================================

/// Response DTO for a tournament invitation.
///
/// The invite list behind `registration_type = "invite_only"` (audit P-27).
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentInvitationResponse {
    pub id: String,
    pub tournament_id: String,

    /// Invited user — set for individual tournaments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Invited team-season — set for team tournaments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_season_id: Option<String>,

    pub status: TournamentInvitationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub invited_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<TournamentInvitation> for TournamentInvitationResponse {
    fn from(i: TournamentInvitation) -> Self {
        Self {
            id: i.id.to_string(),
            tournament_id: i.tournament_id.to_string(),
            user_id: i.user_id.map(|id| id.to_string()),
            team_season_id: i.team_season_id.map(|id| id.to_string()),
            status: i.status,
            message: i.message,
            invited_by: i.invited_by.to_string(),
            accepted_at: i.accepted_at,
            revoked_at: i.revoked_at,
            created_at: i.created_at,
        }
    }
}

// =============================================================================
// TOURNAMENT MATCH RESPONSES
// =============================================================================

/// Response DTO for a tournament match.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentMatchResponse {
    pub id: String,
    pub bracket_id: String,
    pub stage_id: String,
    pub tournament_id: String,

    // Position
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,

    // Participants
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_registration_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_registration_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_seed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_seed: Option<i32>,

    // Match format
    pub match_format: String,
    pub maps_required: i32,

    // Scheduling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_deadline: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    // Results
    pub participant1_score: i32,
    pub participant2_score: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner_registration_id: Option<String>,

    // Status
    //
    // Typed as the enum rather than `String` so the OpenAPI schema carries the
    // permitted values and `openapi-typescript` emits a union. A drifted status
    // in a client is then a compile error, not a silent `default:` branch (P-31).
    // Wire-compatible: `Display` and `Serialize` agree for every variant, which
    // is asserted by `schema_wire_compat_tests` in portal-core.
    pub status: TournamentMatchStatus,
    pub disputed: bool,

    // VOD/Stream
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vod_url: Option<String>,

    // Check-in
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_checked_in_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_checked_in_at: Option<DateTime<Utc>>,
    pub check_in_required: bool,
    pub veto_required: bool,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<TournamentMatch> for TournamentMatchResponse {
    fn from(m: TournamentMatch) -> Self {
        Self {
            id: m.id.to_string(),
            bracket_id: m.bracket_id.to_string(),
            stage_id: m.stage_id.to_string(),
            tournament_id: m.tournament_id.to_string(),
            round: m.round,
            match_number: m.match_number,
            bracket_position: m.bracket_position,
            participant1_registration_id: m.participant1_registration_id.map(|id| id.to_string()),
            participant2_registration_id: m.participant2_registration_id.map(|id| id.to_string()),
            participant1_name: m.participant1_name,
            participant1_logo_url: m.participant1_logo_url,
            participant1_seed: m.participant1_seed,
            participant2_name: m.participant2_name,
            participant2_logo_url: m.participant2_logo_url,
            participant2_seed: m.participant2_seed,
            match_format: m.match_format.to_string(),
            maps_required: m.maps_required,
            scheduled_at: m.scheduled_at,
            schedule_deadline: m.schedule_deadline,
            started_at: m.started_at,
            completed_at: m.completed_at,
            participant1_score: m.participant1_score,
            participant2_score: m.participant2_score,
            winner_registration_id: m.winner_registration_id.map(|id| id.to_string()),
            status: m.status,
            disputed: m.disputed,
            stream_url: m.stream_url,
            vod_url: m.vod_url,
            participant1_checked_in_at: m.participant1_checked_in_at,
            participant2_checked_in_at: m.participant2_checked_in_at,
            check_in_required: m.check_in_required,
            veto_required: m.veto_required,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

// =============================================================================
// TOURNAMENT MATCH GAME RESPONSES
// =============================================================================

/// Response DTO for a tournament match game.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentMatchGameResponse {
    pub id: String,
    pub match_id: String,
    pub game_number: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1_score: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2_score: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner_registration_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<TournamentMatchGame> for TournamentMatchGameResponse {
    fn from(g: TournamentMatchGame) -> Self {
        Self {
            id: g.id.to_string(),
            match_id: g.match_id.to_string(),
            game_number: g.game_number,
            map_id: g.map_id,
            participant1_score: g.participant1_score,
            participant2_score: g.participant2_score,
            winner_registration_id: g.winner_registration_id.map(|id| id.to_string()),
            started_at: g.started_at,
            completed_at: g.completed_at,
            duration_seconds: g.duration_seconds,
            status: g.status.to_string(),
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

// =============================================================================
// TOURNAMENT STANDINGS RESPONSES
// =============================================================================

/// Response DTO for tournament standings.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentStandingResponse {
    pub id: String,
    pub bracket_id: String,
    pub registration_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant_name: Option<String>,
    pub position: i32,
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,
    pub game_wins: i32,
    pub game_losses: i32,
    pub game_differential: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buchholz_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opponent_match_wins: Option<f64>,
    pub points: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

impl From<TournamentStanding> for TournamentStandingResponse {
    fn from(s: TournamentStanding) -> Self {
        let win_rate = s.win_rate();

        Self {
            id: s.id.to_string(),
            bracket_id: s.bracket_id.to_string(),
            registration_id: s.registration_id.to_string(),
            participant_name: s.participant_name,
            position: s.position,
            matches_played: s.matches_played,
            matches_won: s.matches_won,
            matches_lost: s.matches_lost,
            matches_drawn: s.matches_drawn,
            game_wins: s.game_wins,
            game_losses: s.game_losses,
            game_differential: s.game_differential,
            buchholz_score: s.buchholz_score,
            opponent_match_wins: s.opponent_match_wins,
            points: s.points,
            win_rate,
            updated_at: s.updated_at,
        }
    }
}

// =============================================================================
// SEEDING RESPONSES
// =============================================================================

/// Response DTO for a seeded participant.
#[derive(Debug, Serialize, ToSchema)]
pub struct SeededParticipantResponse {
    /// Registration ID.
    pub registration_id: String,
    /// Participant display name.
    pub participant_name: String,
    /// Assigned seed number (1 = highest seed).
    pub seed: i32,
    /// Rating used for seeding (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_rating: Option<i32>,
}

/// Response DTO for check-in status.
#[derive(Debug, Serialize, ToSchema)]
pub struct CheckInStatusResponse {
    /// Tournament ID.
    pub tournament_id: String,
    /// Whether check-in is required for this tournament.
    pub check_in_required: bool,
    /// Whether the check-in window is currently open.
    pub check_in_open: bool,
    /// Number of participants who have checked in.
    pub checked_in_count: i64,
    /// Total eligible participants (approved + checked_in).
    pub total_eligible: i64,
}

// =============================================================================
// MATCH LIFECYCLE RESPONSES
// =============================================================================

/// Response DTO for match status details.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchStatusDetailsResponse {
    /// Match ID.
    pub match_id: String,
    /// Current status.
    pub current_status: String,
    /// Allowed transitions from current status.
    pub allowed_transitions: Vec<String>,
    /// Whether match is in terminal state.
    pub is_terminal: bool,
    /// Whether match is actively in progress.
    pub is_active: bool,
    /// Scheduled time (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<DateTime<Utc>>,
    /// When match started (if started).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// When match completed (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of status transitions.
    pub transition_count: usize,
    /// Latest transition log entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_transition: Option<MatchStatusLogResponse>,
}

/// Response DTO for a match status log entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchStatusLogResponse {
    /// Log entry ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Status before the transition.
    pub from_status: String,
    /// Status after the transition.
    pub to_status: String,
    /// Human-readable reason for the transition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_reason: Option<String>,
    /// User who triggered the transition (if not system).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggered_by_user_id: Option<String>,
    /// Whether the transition was triggered by a background job.
    pub triggered_by_system: bool,
    /// Whether this was an admin override.
    pub is_admin_override: bool,
    /// When the transition occurred.
    pub transitioned_at: DateTime<Utc>,
}

impl From<MatchStatusLog> for MatchStatusLogResponse {
    fn from(log: MatchStatusLog) -> Self {
        let is_admin_override = log.is_admin_override();
        Self {
            id: log.id.to_string(),
            match_id: log.match_id.to_string(),
            from_status: log.from_status.to_string(),
            to_status: log.to_status.to_string(),
            transition_reason: log.transition_reason,
            triggered_by_user_id: log.triggered_by_user_id.map(|id| id.to_string()),
            triggered_by_system: log.triggered_by_system,
            is_admin_override,
            transitioned_at: log.transitioned_at,
        }
    }
}

impl From<MatchStatusDetails> for MatchStatusDetailsResponse {
    fn from(details: MatchStatusDetails) -> Self {
        Self {
            match_id: details.match_id.to_string(),
            current_status: details.current_status.to_string(),
            allowed_transitions: details
                .allowed_transitions
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            is_terminal: details.is_terminal,
            is_active: details.is_active,
            scheduled_at: details.scheduled_at,
            started_at: details.started_at,
            completed_at: details.completed_at,
            transition_count: details.transition_count,
            latest_transition: details.latest_transition.map(Into::into),
        }
    }
}

// =============================================================================
// SCHEDULE PROPOSAL RESPONSES
// =============================================================================

/// Response DTO for a schedule proposal.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScheduleProposalResponse {
    /// Proposal ID.
    pub id: String,
    /// Match ID this proposal is for.
    pub match_id: String,
    /// Registration ID of the proposer.
    pub proposed_by_registration_id: String,
    /// User ID of the proposer.
    pub proposed_by_user_id: String,
    /// Proposed time slots (1-5 options).
    pub proposed_times: Vec<DateTime<Utc>>,
    /// Selected time (when accepted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_time: Option<DateTime<Utc>>,
    /// When the proposal was responded to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_at: Option<DateTime<Utc>>,
    /// User who responded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responded_by_user_id: Option<String>,
    /// Counter-proposal ID if this was counter-proposed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter_proposal_id: Option<String>,
    /// Current status.
    // Typed as the enum so the OpenAPI schema carries the permitted values and
    // clients get a union rather than `string` (P-31). Wire-compatible: asserted
    // by `wire_compat_tests` in portal-core.
    pub status: ProposalStatus,
    /// When this proposal expires.
    pub expires_at: DateTime<Utc>,
    /// Notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Reason provided by the responder when the proposal was rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

// =============================================================================
// TOURNAMENT MAP POOL RESPONSES
// =============================================================================

/// Response DTO for a tournament's effective map pool.
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentMapPoolResponse {
    /// Map IDs in the pool.
    pub maps: Vec<String>,
    /// Source of the pool: "tournament" (custom override) or "game" (default from game config).
    #[schema(example = "game")]
    pub source: String,
}

impl From<ScheduleProposal> for ScheduleProposalResponse {
    fn from(p: ScheduleProposal) -> Self {
        Self {
            id: p.id.to_string(),
            match_id: p.match_id.to_string(),
            proposed_by_registration_id: p.proposed_by_registration_id.to_string(),
            proposed_by_user_id: p.proposed_by_user_id.to_string(),
            proposed_times: p.proposed_times,
            selected_time: p.selected_time,
            responded_at: p.responded_at,
            responded_by_user_id: p.responded_by_user_id.map(|id| id.to_string()),
            counter_proposal_id: p.counter_proposal_id.map(|id| id.to_string()),
            status: p.status,
            expires_at: p.expires_at,
            notes: p.notes,
            rejection_reason: p.rejection_reason,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

// =============================================================================
// MATCH PARTICIPANT RESOLUTION (P-53 / P-56)
// =============================================================================

/// The two registrations that face each other in one match, plus which of
/// them (if either) belongs to the caller.
///
/// # Why this exists
///
/// Resolving "which registration am I in this match?" used to be done in the
/// browser by paging `GET /v1/tournaments/{id}/registrations` and scanning the
/// rows. That scan is bounded by `PaginationParams::limit()`, which clamps
/// `per_page` to 100 — so in a tournament with more than 100 participants
/// every participant whose row sorts past #100 resolved to `null`, and the
/// result-submission affordance simply never appeared for them. 128-player
/// events are routine, and the failure is silent.
///
/// This endpoint answers the question directly from the match row, in O(1),
/// so participant count cannot affect it.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchParticipantsResponse {
    /// The match these registrations belong to.
    pub match_id: String,
    /// Registration seated in slot 1, if the slot is filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant1: Option<TournamentRegistrationResponse>,
    /// Registration seated in slot 2, if the slot is filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant2: Option<TournamentRegistrationResponse>,
    /// The caller's own registration id, when the caller is one of the two —
    /// directly as the registered player, or as a member of the registered
    /// team-season. `null` for spectators and staff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_registration_id: Option<String>,
    /// Whether the caller may CHECK IN for their registration (P-193).
    ///
    /// Check-in is gated narrower than `speaks_for` — captain, team owner,
    /// active delegate, or the registered player — because a check-in can
    /// auto-advance the match. A plain roster member speaks for the
    /// registration (may submit and confirm results) yet cannot check it
    /// in, so the check-in panel must key off THIS field rather than
    /// `my_registration_id`, or it offers an action the backend refuses.
    pub my_registration_can_check_in: bool,
}

// =============================================================================
// SELF-SCOPED REGISTRATION LOOKUP + REAL COUNTS (P-167)
// =============================================================================

/// Every registration in one tournament that the caller speaks for.
///
/// # Why this exists
///
/// The tournament page decided "am I registered?" by fetching
/// `GET /v1/tournaments/{id}/registrations` **at the default `per_page` of
/// 20** and scanning the page for the viewer. Past row 20 every participant
/// was shown the "Join This Tournament" call to action instead of their own
/// registration: no Registered chip, no withdraw control, no check-in — the
/// product told them, on the page they land on first, that they were not in a
/// tournament they were in.
///
/// Widening the page only moves the ceiling (the same defect was already
/// fixed once at 100), so the question is answered directly: at most one
/// lookup by player plus one per team-season the caller belongs to,
/// regardless of how large the tournament is.
///
/// Usually zero or one row. It is a list because a player can legitimately
/// hold both an individual row and a team row (different tournaments allow
/// different things), and because silently picking one of several would be
/// the same class of lie this replaces.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyTournamentRegistrationsResponse {
    /// Tournament the rows belong to.
    pub tournament_id: String,
    /// The caller's registrations, including terminal ones (`withdrawn`,
    /// `disqualified`) so the client can tell "withdrew" from "never entered".
    pub registrations: Vec<TournamentRegistrationResponse>,
}

/// Real per-status registration counts for a tournament.
///
/// The participant count and the pending-approvals badge used to be the
/// `.length` of a **page** of the registrations list, so a 64-slot event with
/// 40 entrants displayed "20 / 64" — telling every viewer there were 44 free
/// slots — and an organiser with 40 people waiting saw "20 pending approvals"
/// (P-167).
#[derive(Debug, Serialize, ToSchema)]
pub struct TournamentRegistrationCountsResponse {
    /// Tournament these counts describe.
    pub tournament_id: String,
    /// Every registration row, whatever its status.
    pub total: i64,
    /// Rows that still represent someone taking part — everything except
    /// `withdrawn` and `disqualified`. This is the number to show against
    /// `max_participants`.
    pub participating: i64,
    /// Awaiting organiser approval.
    pub pending: i64,
    /// Approved, awaiting check-in.
    pub approved: i64,
    /// Checked in.
    pub checked_in: i64,
    /// Currently competing.
    pub active: i64,
    /// Eliminated.
    pub eliminated: i64,
    /// Removed for a rule violation.
    pub disqualified: i64,
    /// Voluntarily withdrawn.
    pub withdrawn: i64,
    /// Failed to check in.
    pub no_show: i64,
}

impl TournamentRegistrationCountsResponse {
    /// Build from the domain counts.
    #[must_use]
    pub fn new(tournament_id: String, counts: RegistrationCounts) -> Self {
        Self {
            tournament_id,
            total: counts.total,
            participating: counts.participating,
            pending: counts.pending,
            approved: counts.approved,
            checked_in: counts.checked_in,
            active: counts.active,
            eliminated: counts.eliminated,
            disqualified: counts.disqualified,
            withdrawn: counts.withdrawn,
            no_show: counts.no_show,
        }
    }
}

// =============================================================================
// ADMIN RESULT OVERRIDE (P-72)
// =============================================================================

/// One recorded admin correction of a match's score.
///
/// Read back from the `entity_changes` audit trail, which is where
/// `override_result_audited` writes it in the same transaction as the score
/// itself — so a correction that is not on this list did not happen.
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchResultOverrideResponse {
    /// Audit row id.
    pub id: String,
    /// Match whose score was corrected.
    pub match_id: String,
    /// Score recorded before the correction (absent if the match had none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_participant1_score: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_participant2_score: Option<i32>,
    /// Winner recorded before the correction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_winner_registration_id: Option<String>,
    /// Score recorded by the correction.
    pub new_participant1_score: i32,
    pub new_participant2_score: i32,
    /// Winner recorded by the correction.
    pub new_winner_registration_id: String,
    /// Operator-supplied justification.
    pub reason: String,
    /// Player id of the admin who made the correction.
    pub changed_by_player_id: String,
    /// That admin's display name — never make an operator read a truncated
    /// UUID to find out who changed a score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by_name: Option<String>,
    /// When the correction was made.
    pub created_at: DateTime<Utc>,
}
