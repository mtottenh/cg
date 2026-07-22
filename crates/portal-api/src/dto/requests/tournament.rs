//! Tournament request DTOs.

use chrono::{DateTime, Utc};
use portal_core::types::{
    MatchFormat, RegistrationType, SchedulingMode, StageFormat, TournamentFormat,
    TournamentParticipantType, WithdrawalPolicy,
};
use portal_core::{GameId, LeagueId, LeagueSeasonId, LeagueTeamSeasonId, PlayerId, TournamentId};
use portal_domain::entities::tournament::{
    CreateTournamentCommand, CreateTournamentStageCommand, UpdateTournamentCommand,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

/// Validate URL-friendly slug format.
fn validate_slug(slug: &str) -> Result<(), validator::ValidationError> {
    let bytes = slug.as_bytes();
    if bytes.is_empty() {
        return Err(validator::ValidationError::new("slug_empty"));
    }

    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(validator::ValidationError::new("slug_invalid_start"));
    }
    if !(last.is_ascii_lowercase() || last.is_ascii_digit()) {
        return Err(validator::ValidationError::new("slug_invalid_end"));
    }

    for &b in bytes {
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
            return Err(validator::ValidationError::new("slug_invalid_chars"));
        }
    }

    Ok(())
}

/// Validate that a URL uses the http or https scheme.
///
/// Used together with `#[validate(url)]` (which only checks the value parses
/// as a URL) to reject schemes like `javascript:` or `ftp:`.
fn validate_http_url(url: &str) -> Result<(), validator::ValidationError> {
    let lower = url.trim_start().to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        let mut err = validator::ValidationError::new("url_scheme");
        err.message = Some("URL must use http or https".into());
        return Err(err);
    }
    Ok(())
}

// =============================================================================
// TOURNAMENT REQUESTS
// =============================================================================

/// Request to create a new tournament.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateTournamentRequest {
    /// Game ID for this tournament.
    pub game_id: String,

    /// Optional league ID to link this tournament.
    #[serde(default)]
    pub league_id: Option<String>,

    /// Optional season ID to link this tournament.
    #[serde(default)]
    pub season_id: Option<String>,

    /// Tournament name.
    #[validate(length(min = 2, max = 100))]
    pub name: String,

    /// URL-friendly slug.
    #[validate(length(min = 2, max = 100), custom(function = "validate_slug"))]
    pub slug: String,

    /// Optional description.
    #[validate(length(max = 5000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Tournament format: `single_elimination`, `double_elimination`, `round_robin`, swiss, `groups_and_playoffs`.
    pub format: String,

    /// Optional format-specific settings.
    #[serde(default)]
    pub format_settings: Option<serde_json::Value>,

    /// Participant type: team, individual, adhoc.
    pub participant_type: String,

    /// Team size (required for team tournaments).
    #[validate(range(min = 1, max = 50))]
    #[serde(default)]
    pub team_size: Option<i32>,

    /// Minimum participants required (at least 2 for any tournament).
    #[validate(range(min = 2, max = 1024))]
    pub min_participants: i32,

    /// Maximum participants allowed.
    #[validate(range(min = 2, max = 1024))]
    pub max_participants: i32,

    /// Registration type: open, `invite_only`, qualification, approval.
    #[serde(default = "default_registration_type")]
    pub registration_type: String,

    /// Registration start time.
    #[serde(default)]
    pub registration_start: Option<DateTime<Utc>>,

    /// Registration end time.
    #[serde(default)]
    pub registration_end: Option<DateTime<Utc>>,

    /// Whether check-in is required.
    #[serde(default)]
    pub check_in_required: bool,

    /// Check-in start time.
    #[serde(default)]
    pub check_in_start: Option<DateTime<Utc>>,

    /// Check-in end time.
    #[serde(default)]
    pub check_in_end: Option<DateTime<Utc>>,

    /// Scheduling mode: live, `self_scheduled`, hybrid.
    #[serde(default = "default_scheduling_mode")]
    pub scheduling_mode: String,

    /// Tournament start time.
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,

    /// Default match format: bo1, bo3, bo5, bo7.
    #[serde(default = "default_match_format")]
    pub default_match_format: String,

    /// Default map veto format.
    #[serde(default)]
    pub default_map_veto_format: Option<String>,

    /// Withdrawal policy: forfeit, reseeding, `waitlist_promotion`, `admin_decision`.
    #[serde(default = "default_withdrawal_policy")]
    pub withdrawal_policy: String,

    /// URL to tournament rules.
    #[validate(url, custom(function = "validate_http_url"))]
    #[serde(default)]
    pub rules_url: Option<String>,

    /// Additional settings.
    #[serde(default)]
    pub settings: Option<serde_json::Value>,

    /// Map pool for this tournament — **required**, at least one map.
    ///
    /// Every map ID must exist in the game's map catalog. Tournaments own an
    /// explicit pool so map validation on result submission can fail closed.
    /// Pass the game's default pool if you don't want to customise it.
    #[validate(length(min = 1, max = 64, message = "map_pool must contain at least one map"))]
    pub map_pool: Vec<String>,

    /// Eligibility restrictions for tournament registration.
    ///
    /// Controls which players/teams are allowed to register based on
    /// their in-game rating, peak rating, rank tier, etc.
    #[serde(default)]
    pub eligibility_restrictions: Option<EligibilityRestrictionsInput>,
}

/// Typed input for eligibility restrictions.
///
/// All fields are optional — only specified fields are enforced.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct EligibilityRestrictionsInput {
    /// Max current rating for any individual player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rating_per_player: Option<i32>,

    /// Min current rating for any individual player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_rating_per_player: Option<i32>,

    /// Max peak (all-time high) rating for any player (anti-smurf).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_peak_rating_per_player: Option<i32>,

    /// Max average rating for any player (computed from history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_avg_rating_per_player: Option<i32>,

    /// Max sum of all team members' current ratings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_team_total_rating: Option<i32>,

    /// Max average of team members' current ratings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_team_average_rating: Option<i32>,

    /// Only allow players in certain rank tiers (e.g., `["silver", "gold"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_rank_tiers: Vec<String>,

    /// Min matches played to be eligible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_matches_played: Option<i32>,
}

/// Merge an optional typed eligibility input into the settings JSON.
fn merge_eligibility_into_settings(
    settings: Option<serde_json::Value>,
    eligibility: Option<EligibilityRestrictionsInput>,
) -> Option<serde_json::Value> {
    let Some(eligibility) = eligibility else {
        return settings;
    };

    let eligibility_json = serde_json::to_value(eligibility).unwrap_or_default();

    let mut settings = settings.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = settings.as_object_mut() {
        obj.insert("eligibility".to_string(), eligibility_json);
    }
    Some(settings)
}

fn default_registration_type() -> String {
    "open".to_string()
}

fn default_scheduling_mode() -> String {
    "live".to_string()
}

fn default_match_format() -> String {
    "bo1".to_string()
}

fn default_withdrawal_policy() -> String {
    "forfeit".to_string()
}

impl CreateTournamentRequest {
    /// Convert to command.
    pub fn into_command(self) -> Result<CreateTournamentCommand, crate::error::ApiError> {
        let game_id: GameId = self
            .game_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid game ID format"))?;

        let league_id: Option<LeagueId> = self
            .league_id
            .map(|id| {
                id.parse()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid league ID format"))
            })
            .transpose()?;

        let season_id: Option<LeagueSeasonId> = self
            .season_id
            .map(|id| {
                id.parse()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid season ID format"))
            })
            .transpose()?;

        let format: TournamentFormat = self
            .format
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid tournament format"))?;

        let participant_type: TournamentParticipantType = self
            .participant_type
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid participant type"))?;

        let registration_type: RegistrationType = self
            .registration_type
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid registration type"))?;

        let scheduling_mode: SchedulingMode = self
            .scheduling_mode
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid scheduling mode"))?;

        let default_match_format: MatchFormat = self
            .default_match_format
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid match format"))?;

        let withdrawal_policy: WithdrawalPolicy = self
            .withdrawal_policy
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid withdrawal policy"))?;

        Ok(CreateTournamentCommand {
            game_id,
            league_id,
            season_id,
            name: self.name,
            slug: self.slug,
            description: self.description,
            format,
            format_settings: self.format_settings,
            participant_type,
            team_size: self.team_size,
            min_participants: self.min_participants,
            max_participants: self.max_participants,
            registration_type,
            registration_start: self.registration_start,
            registration_end: self.registration_end,
            check_in_required: self.check_in_required,
            check_in_start: self.check_in_start,
            check_in_end: self.check_in_end,
            scheduling_mode,
            starts_at: self.starts_at,
            default_match_format,
            default_map_veto_format: self.default_map_veto_format,
            withdrawal_policy,
            rules_url: self.rules_url,
            settings: merge_eligibility_into_settings(self.settings, self.eligibility_restrictions),
            map_pool: self.map_pool,
        })
    }
}

/// Request to update a tournament.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateTournamentRequest {
    /// Updated tournament name.
    #[validate(length(min = 2, max = 100))]
    #[serde(default)]
    pub name: Option<String>,

    /// Updated slug.
    #[validate(length(min = 2, max = 100), custom(function = "validate_slug"))]
    #[serde(default)]
    pub slug: Option<String>,

    /// Updated description.
    #[validate(length(max = 5000))]
    #[serde(default)]
    pub description: Option<String>,

    /// Updated format settings.
    #[serde(default)]
    pub format_settings: Option<serde_json::Value>,

    /// Updated minimum participants.
    #[validate(range(min = 2, max = 1024))]
    #[serde(default)]
    pub min_participants: Option<i32>,

    /// Updated maximum participants.
    #[validate(range(min = 2, max = 1024))]
    #[serde(default)]
    pub max_participants: Option<i32>,

    /// Updated registration start time.
    #[serde(default)]
    pub registration_start: Option<DateTime<Utc>>,

    /// Updated registration end time.
    #[serde(default)]
    pub registration_end: Option<DateTime<Utc>>,

    /// Updated check-in required flag.
    #[serde(default)]
    pub check_in_required: Option<bool>,

    /// Updated check-in start time.
    #[serde(default)]
    pub check_in_start: Option<DateTime<Utc>>,

    /// Updated check-in end time.
    #[serde(default)]
    pub check_in_end: Option<DateTime<Utc>>,

    /// Updated start time.
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,

    /// Updated end time.
    #[serde(default)]
    pub ends_at: Option<DateTime<Utc>>,

    /// Updated timezone hint.
    #[validate(length(max = 50))]
    #[serde(default)]
    pub timezone_hint: Option<String>,

    /// Updated default match format.
    #[serde(default)]
    pub default_match_format: Option<String>,

    /// Updated default map veto format.
    #[serde(default)]
    pub default_map_veto_format: Option<String>,

    /// Updated prize pool.
    #[serde(default)]
    pub prize_pool: Option<serde_json::Value>,

    /// Updated rules URL.
    #[validate(url, custom(function = "validate_http_url"))]
    #[serde(default)]
    pub rules_url: Option<String>,

    /// Updated settings.
    #[serde(default)]
    pub settings: Option<serde_json::Value>,

    /// Updated eligibility restrictions for tournament registration.
    #[serde(default)]
    pub eligibility_restrictions: Option<EligibilityRestrictionsInput>,

    /// Updated withdrawal policy.
    #[serde(default)]
    pub withdrawal_policy: Option<String>,
}

impl TryFrom<UpdateTournamentRequest> for UpdateTournamentCommand {
    type Error = crate::error::ApiError;

    fn try_from(req: UpdateTournamentRequest) -> Result<Self, Self::Error> {
        let default_match_format = req
            .default_match_format
            .map(|f| {
                f.parse::<MatchFormat>()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid match format"))
            })
            .transpose()?;

        let withdrawal_policy = req
            .withdrawal_policy
            .map(|p| {
                p.parse::<WithdrawalPolicy>()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid withdrawal policy"))
            })
            .transpose()?;

        Ok(Self {
            name: req.name,
            slug: req.slug,
            description: req.description,
            logo_url: None,
            banner_url: None,
            format_settings: req.format_settings,
            min_participants: req.min_participants,
            max_participants: req.max_participants,
            registration_start: req.registration_start,
            registration_end: req.registration_end,
            check_in_required: req.check_in_required,
            check_in_start: req.check_in_start,
            check_in_end: req.check_in_end,
            starts_at: req.starts_at,
            ends_at: req.ends_at,
            timezone_hint: req.timezone_hint,
            default_match_format,
            default_map_veto_format: req.default_map_veto_format,
            prize_pool: req.prize_pool,
            rules_url: req.rules_url,
            settings: merge_eligibility_into_settings(req.settings, req.eligibility_restrictions),
            withdrawal_policy,
        })
    }
}

// =============================================================================
// TOURNAMENT STAGE REQUESTS
// =============================================================================

/// Request to create a tournament stage.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateTournamentStageRequest {
    /// Stage name.
    #[validate(length(min = 2, max = 100))]
    pub name: String,

    /// Stage order (determines sequence).
    #[validate(range(min = 1, max = 100))]
    pub stage_order: i32,

    /// Stage format: `single_elimination`, `double_elimination`, `round_robin`, swiss, `group_stage`.
    pub format: String,

    /// Format-specific settings.
    #[serde(default)]
    pub format_settings: Option<serde_json::Value>,

    /// Number of participants who advance.
    #[validate(range(min = 1, max = 256))]
    #[serde(default)]
    pub advancement_count: Option<i32>,

    /// Match format override for this stage.
    #[serde(default)]
    pub match_format: Option<String>,

    /// Stage start time.
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,

    /// Stage end time.
    #[serde(default)]
    pub ends_at: Option<DateTime<Utc>>,
}

impl CreateTournamentStageRequest {
    /// Convert to command with tournament ID.
    pub fn into_command(
        self,
        tournament_id: TournamentId,
    ) -> Result<CreateTournamentStageCommand, crate::error::ApiError> {
        let format: StageFormat = self
            .format
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid stage format"))?;

        let match_format = self
            .match_format
            .map(|f| {
                f.parse::<MatchFormat>()
                    .map_err(|_| crate::error::ApiError::bad_request("Invalid match format"))
            })
            .transpose()?;

        Ok(CreateTournamentStageCommand {
            tournament_id,
            name: self.name,
            stage_order: self.stage_order,
            format,
            format_settings: self.format_settings,
            advancement_count: self.advancement_count,
            advancement_rule: portal_core::types::AdvancementRule::TopN,
            match_format,
            map_veto_format: None,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
        })
    }
}

// =============================================================================
// TOURNAMENT REGISTRATION REQUESTS
// =============================================================================

/// Request to register a team for a tournament.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterTeamRequest {
    /// Team season ID to register.
    pub team_season_id: String,

    /// Display name for the team in this tournament.
    #[validate(length(min = 2, max = 50))]
    pub participant_name: String,

    /// Optional logo URL.
    #[validate(url)]
    #[serde(default)]
    pub participant_logo_url: Option<String>,
}

impl RegisterTeamRequest {
    /// Parse the team season ID.
    pub fn parse_team_season_id(&self) -> Result<LeagueTeamSeasonId, crate::error::ApiError> {
        self.team_season_id
            .parse()
            .map_err(|_| crate::error::ApiError::bad_request("Invalid team season ID format"))
    }
}

/// Request to register a player for an individual tournament.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterPlayerRequest {
    /// Display name for the player in this tournament.
    #[validate(length(min = 2, max = 50))]
    pub participant_name: String,
}

impl RegisterPlayerRequest {
    /// Parse player ID from the authenticated user.
    pub fn into_command(self, player_id: PlayerId) -> (PlayerId, String) {
        (player_id, self.participant_name)
    }
}

/// Request to check in for a tournament.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckInRequest {
    // No additional fields - registration ID from path
}

/// Request to withdraw from a tournament.
#[derive(Debug, Deserialize, ToSchema)]
pub struct WithdrawRequest {
    /// Optional reason for withdrawal.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Request to reject a registration.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RejectRegistrationRequest {
    /// Reason for rejection.
    #[validate(length(min = 1, max = 500))]
    #[serde(default)]
    pub reason: Option<String>,
}

/// Request to disqualify a participant.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DisqualifyRequest {
    /// Reason for disqualification.
    #[validate(length(min = 1, max = 500))]
    pub reason: String,
}

/// Request to auto-seed participants.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AutoSeedRequest {
    /// Seeding algorithm to use: random, rating, season_rank.
    #[serde(default = "default_seeding_algorithm")]
    pub algorithm: String,
}

fn default_seeding_algorithm() -> String {
    "random".to_string()
}

/// Request to manually set seeds.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ManualSeedRequest {
    /// List of seed assignments (registration_id, seed_number).
    #[validate(length(min = 1))]
    pub seeds: Vec<SeedAssignment>,
}

/// Individual seed assignment.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SeedAssignment {
    /// Registration ID.
    pub registration_id: String,
    /// Seed number (1 = highest seed).
    pub seed: i32,
}

// =============================================================================
// TOURNAMENT MATCH REQUESTS
// =============================================================================

/// Request to schedule a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ScheduleMatchRequest {
    /// Scheduled start time.
    pub scheduled_at: DateTime<Utc>,
}

/// Request to submit match results.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SubmitMatchResultRequest {
    /// Score for participant 1.
    #[validate(range(min = 0, max = 10))]
    pub participant1_score: i32,

    /// Score for participant 2.
    #[validate(range(min = 0, max = 10))]
    pub participant2_score: i32,

    /// Optional VOD URL.
    #[validate(url)]
    #[serde(default)]
    pub vod_url: Option<String>,
}

/// Request to dispute a match result.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DisputeMatchRequest {
    /// Reason for the dispute.
    #[validate(length(min = 10, max = 1000))]
    pub reason: String,
}

/// Request to resolve a match dispute.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResolveDisputeRequest {
    /// Resolution decision.
    #[validate(length(min = 10, max = 1000))]
    pub resolution: String,

    /// Updated score for participant 1.
    #[validate(range(min = 0, max = 10))]
    #[serde(default)]
    pub participant1_score: Option<i32>,

    /// Updated score for participant 2.
    #[validate(range(min = 0, max = 10))]
    #[serde(default)]
    pub participant2_score: Option<i32>,
}

// =============================================================================
// QUERY PARAMETERS
// =============================================================================

/// Query parameters for listing tournaments.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ListTournamentsQuery {
    /// Filter by game ID.
    #[serde(default)]
    pub game_id: Option<String>,

    /// Filter by league ID.
    #[serde(default)]
    pub league_id: Option<String>,

    /// Filter by season ID.
    #[serde(default)]
    pub season_id: Option<String>,

    /// Filter by status.
    #[serde(default)]
    pub status: Option<String>,

    /// Filter by format.
    #[serde(default)]
    pub format: Option<String>,

    /// Search by name.
    #[serde(default)]
    pub search: Option<String>,
}

// =============================================================================
// MATCH LIFECYCLE REQUESTS
// =============================================================================

/// Request to check in for a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct MatchCheckInRequest {
    /// Registration ID of the participant checking in.
    pub registration_id: String,
}

/// Request for admin to force a match status transition.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminMatchTransitionRequest {
    /// Target status to transition to.
    pub to_status: String,

    /// Reason for the admin override.
    #[validate(length(min = 5, max = 500))]
    pub override_reason: String,
}

/// Request to forfeit a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ForfeitMatchRequest {
    /// Registration ID of the participant forfeiting.
    pub registration_id: String,
}

// =============================================================================
// SCHEDULE PROPOSAL REQUESTS
// =============================================================================

/// Request to propose match times.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ProposeScheduleRequest {
    /// Proposed time slots (1-5 options).
    #[validate(length(min = 1, max = 5))]
    pub proposed_times: Vec<DateTime<Utc>>,

    /// Optional message to the opponent about the proposal.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub notes: Option<String>,
}

/// Request to accept a schedule proposal.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AcceptScheduleProposalRequest {
    /// ID of the proposal to accept.
    pub proposal_id: String,

    /// Selected time from the proposed times.
    pub selected_time: DateTime<Utc>,
}

/// Request to reject a schedule proposal.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RejectScheduleProposalRequest {
    /// ID of the proposal to reject.
    pub proposal_id: String,

    /// Optional reason for the rejection, shown to the proposer.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub reason: Option<String>,
}

/// Request to counter-propose new times.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CounterProposeRequest {
    /// ID of the original proposal.
    pub original_proposal_id: String,

    /// New proposed time slots (1-5 options).
    #[validate(length(min = 1, max = 5))]
    pub proposed_times: Vec<DateTime<Utc>>,

    /// Optional message to the opponent about the counter-proposal.
    #[validate(length(max = 1000))]
    #[serde(default)]
    pub notes: Option<String>,
}

/// Request for admin to directly schedule a match.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct AdminScheduleRequest {
    /// Time to schedule the match.
    pub scheduled_at: DateTime<Utc>,

    /// Optional notes for the scheduling decision.
    #[serde(default)]
    pub notes: Option<String>,
}

/// Query parameters for listing the current user's tournament matches.
#[derive(Debug, Clone, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct MyMatchesQuery {
    /// Filter by match status (e.g., "in_progress", "scheduled", "completed").
    pub status: Option<String>,

    /// Filter by tournament ID.
    pub tournament_id: Option<String>,

    /// Maximum number of results to return (default: 50, max: 100).
    #[serde(default = "default_my_matches_limit")]
    pub limit: Option<i64>,

    /// Offset for pagination (default: 0).
    #[serde(default)]
    pub offset: Option<i64>,
}

// Serde `default = "..."` for an `Option<i64>` field requires the function to
// return `Option<i64>`, so the wrap is necessary here.
#[allow(clippy::unnecessary_wraps)]
fn default_my_matches_limit() -> Option<i64> {
    Some(50)
}

// =============================================================================
// TOURNAMENT MAP POOL REQUESTS
// =============================================================================

/// Request to set a tournament-specific map pool.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct SetTournamentMapPoolRequest {
    /// Map IDs for the tournament pool (must exist in the game's map catalog).
    #[validate(length(min = 1, max = 20))]
    pub map_ids: Vec<String>,
}
