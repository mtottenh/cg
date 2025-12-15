//! Tournament domain entities.
//!
//! Tournaments are competitive events that can be standalone or integrated with leagues.
//! They support multiple formats (elimination, round robin, swiss) and both team-based
//! and individual competitions.
//!
//! Key relationships:
//!   Tournament -> `TournamentStage` (for multi-stage tournaments)
//!              -> `TournamentBracket` (bracket structure within stages)
//!                   -> `TournamentMatch` (matches within brackets)
//!                        -> `TournamentMatchGame` (individual games in a series)
//!              -> `TournamentRegistration` (participant entries)

use chrono::{DateTime, Utc};
use portal_core::types::{
    AdvancementRule, BracketStatus, BracketType, MatchFormat, MatchParticipantSource,
    RegistrationType, SchedulingMode, StageFormat, StageStatus, TournamentFormat,
    TournamentMatchStatus, TournamentParticipantType, TournamentRegistrationStatus,
    TournamentStatus, WithdrawalPolicy,
};
use portal_core::{
    GameId, LeagueId, LeagueSeasonId, LeagueTeamSeasonId, PlayerId, TournamentBracketId,
    TournamentId, TournamentMapPoolId, TournamentMatchGameId, TournamentMatchId,
    TournamentRegistrationId, TournamentStageId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// TOURNAMENT
// =============================================================================

/// A tournament event.
///
/// Tournaments can be standalone or integrated with leagues/seasons. They support
/// various formats (single elimination, double elimination, round robin, swiss,
/// groups + playoffs) and both team-based and individual competitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tournament {
    pub id: TournamentId,
    pub game_id: GameId,

    // Optional league/season linkage
    pub league_id: Option<LeagueId>,
    pub season_id: Option<LeagueSeasonId>,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,

    // Format
    pub format: TournamentFormat,
    pub format_settings: serde_json::Value,
    pub participant_type: TournamentParticipantType,
    pub team_size: Option<i32>,

    // Capacity
    pub min_participants: i32,
    pub max_participants: i32,

    // Registration
    pub registration_type: RegistrationType,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,

    // Scheduling
    pub scheduling_mode: SchedulingMode,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone_hint: Option<String>,

    // Match settings
    pub default_match_format: MatchFormat,
    pub default_map_veto_format: Option<String>,

    // Prize pool
    pub prize_pool: Option<serde_json::Value>,

    // Rules
    pub rules_url: Option<String>,
    pub settings: serde_json::Value,

    // Policies
    pub withdrawal_policy: WithdrawalPolicy,

    // Status
    pub status: TournamentStatus,

    // Ownership
    pub created_by: UserId,
    pub organization_id: Option<uuid::Uuid>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Tournament {
    /// Check if registration is currently open.
    #[must_use]
    pub fn is_registration_open(&self) -> bool {
        if !self.status.is_registration_open() {
            return false;
        }

        let now = Utc::now();

        // Check registration window if defined
        if let Some(start) = self.registration_start {
            if now < start {
                return false;
            }
        }

        if let Some(end) = self.registration_end {
            if now > end {
                return false;
            }
        }

        true
    }

    /// Check if check-in is currently open.
    #[must_use]
    pub fn is_check_in_open(&self) -> bool {
        if !self.check_in_required {
            return false;
        }

        let now = Utc::now();

        if let (Some(start), Some(end)) = (self.check_in_start, self.check_in_end) {
            now >= start && now <= end
        } else {
            false
        }
    }

    /// Check if the tournament is in draft status.
    #[must_use]
    pub const fn is_draft(&self) -> bool {
        matches!(self.status, TournamentStatus::Draft)
    }

    /// Check if the tournament is active (not completed or cancelled).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Check if the tournament is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if the tournament has started.
    #[must_use]
    pub const fn has_started(&self) -> bool {
        self.started_at.is_some()
    }

    /// Check if the tournament requires teams.
    #[must_use]
    pub const fn is_team_based(&self) -> bool {
        matches!(
            self.participant_type,
            TournamentParticipantType::Team | TournamentParticipantType::AdHoc
        )
    }

    /// Check if the tournament is for individuals.
    #[must_use]
    pub const fn is_individual(&self) -> bool {
        matches!(self.participant_type, TournamentParticipantType::Individual)
    }

    /// Check if the tournament is linked to a league.
    #[must_use]
    pub const fn is_league_tournament(&self) -> bool {
        self.league_id.is_some()
    }
}

/// Command to create a new tournament.
#[derive(Debug, Clone)]
pub struct CreateTournamentCommand {
    pub game_id: GameId,
    pub league_id: Option<LeagueId>,
    pub season_id: Option<LeagueSeasonId>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub format: TournamentFormat,
    pub format_settings: Option<serde_json::Value>,
    pub participant_type: TournamentParticipantType,
    pub team_size: Option<i32>,
    pub min_participants: i32,
    pub max_participants: i32,
    pub registration_type: RegistrationType,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: bool,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub scheduling_mode: SchedulingMode,
    pub starts_at: Option<DateTime<Utc>>,
    pub default_match_format: MatchFormat,
    pub default_map_veto_format: Option<String>,
    pub withdrawal_policy: WithdrawalPolicy,
    pub rules_url: Option<String>,
    pub settings: Option<serde_json::Value>,
}

/// Command to update a tournament.
#[derive(Debug, Clone, Default)]
pub struct UpdateTournamentCommand {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub banner_url: Option<String>,
    pub format_settings: Option<serde_json::Value>,
    pub min_participants: Option<i32>,
    pub max_participants: Option<i32>,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub check_in_required: Option<bool>,
    pub check_in_start: Option<DateTime<Utc>>,
    pub check_in_end: Option<DateTime<Utc>>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone_hint: Option<String>,
    pub default_match_format: Option<MatchFormat>,
    pub default_map_veto_format: Option<String>,
    pub prize_pool: Option<serde_json::Value>,
    pub rules_url: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub withdrawal_policy: Option<WithdrawalPolicy>,
}

// =============================================================================
// TOURNAMENT STAGE
// =============================================================================

/// A stage within a multi-stage tournament.
///
/// Examples: Group Stage, Quarterfinals, Semifinals, Grand Final.
/// Simple single-elimination tournaments typically have just one stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentStage {
    pub id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Identity
    pub name: String,
    pub stage_order: i32,

    // Format
    pub format: StageFormat,
    pub format_settings: serde_json::Value,

    // Advancement
    pub advancement_count: Option<i32>,
    pub advancement_rule: AdvancementRule,

    // Match settings (override tournament defaults)
    pub match_format: Option<MatchFormat>,
    pub map_veto_format: Option<String>,

    // Status
    pub status: StageStatus,

    // Timing
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TournamentStage {
    /// Check if the stage is pending.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, StageStatus::Pending)
    }

    /// Check if the stage is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, StageStatus::Active)
    }

    /// Check if the stage is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.status, StageStatus::Completed)
    }

    /// Check if the stage is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get the effective match format (stage override or tournament default).
    #[must_use]
    pub fn effective_match_format(&self, tournament_default: MatchFormat) -> MatchFormat {
        self.match_format.unwrap_or(tournament_default)
    }
}

/// Command to create a tournament stage.
#[derive(Debug, Clone)]
pub struct CreateTournamentStageCommand {
    pub tournament_id: TournamentId,
    pub name: String,
    pub stage_order: i32,
    pub format: StageFormat,
    pub format_settings: Option<serde_json::Value>,
    pub advancement_count: Option<i32>,
    pub advancement_rule: AdvancementRule,
    pub match_format: Option<MatchFormat>,
    pub map_veto_format: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

// =============================================================================
// TOURNAMENT BRACKET
// =============================================================================

/// A bracket within a tournament stage.
///
/// For single elimination, there's typically one bracket.
/// For double elimination, there are winners, losers, and grand final brackets.
/// For group stages, each group has its own bracket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentBracket {
    pub id: TournamentBracketId,
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Identity
    pub name: String,
    pub bracket_type: BracketType,

    // Structure
    pub total_rounds: i32,
    pub current_round: i32,

    // For groups
    pub group_number: Option<i32>,

    // Status
    pub status: BracketStatus,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TournamentBracket {
    /// Check if the bracket is pending.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, BracketStatus::Pending)
    }

    /// Check if the bracket is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.status, BracketStatus::Active)
    }

    /// Check if the bracket is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.status, BracketStatus::Completed)
    }

    /// Check if the bracket is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if this is a losers bracket.
    #[must_use]
    pub const fn is_losers_bracket(&self) -> bool {
        matches!(self.bracket_type, BracketType::Losers)
    }

    /// Check if this is a group bracket.
    #[must_use]
    pub const fn is_group(&self) -> bool {
        matches!(self.bracket_type, BracketType::RoundRobin) && self.group_number.is_some()
    }
}

/// Command to create a tournament bracket.
#[derive(Debug, Clone)]
pub struct CreateTournamentBracketCommand {
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,
    pub name: String,
    pub bracket_type: BracketType,
    pub total_rounds: i32,
    pub group_number: Option<i32>,
}

// =============================================================================
// TOURNAMENT REGISTRATION
// =============================================================================

/// A registration for tournament participation.
///
/// Represents either a team, individual player, or ad-hoc team depending
/// on the tournament's participant type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentRegistration {
    pub id: TournamentRegistrationId,
    pub tournament_id: TournamentId,

    // Participant identity (exactly one should be set)
    pub team_season_id: Option<LeagueTeamSeasonId>,
    pub player_id: Option<PlayerId>,
    pub adhoc_team_id: Option<uuid::Uuid>,

    // Denormalized display info
    pub participant_name: String,
    pub participant_logo_url: Option<String>,

    // Registration
    pub registered_by: UserId,
    pub registered_at: DateTime<Utc>,

    // Check-in
    pub checked_in: bool,
    pub checked_in_at: Option<DateTime<Utc>>,
    pub checked_in_by: Option<UserId>,

    // Seeding
    pub seed: Option<i32>,
    pub seed_rating: Option<i32>,

    // Status
    pub status: TournamentRegistrationStatus,

    // Admin notes
    pub admin_notes: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub withdrawn_at: Option<DateTime<Utc>>,
}

impl TournamentRegistration {
    /// Check if the registration is pending approval.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, TournamentRegistrationStatus::Pending)
    }

    /// Check if the participant can compete.
    #[must_use]
    pub const fn can_compete(&self) -> bool {
        self.status.can_compete()
    }

    /// Check if the participant can check in.
    #[must_use]
    pub const fn can_check_in(&self) -> bool {
        self.status.can_check_in()
    }

    /// Check if the participant has checked in.
    #[must_use]
    pub const fn is_checked_in(&self) -> bool {
        self.checked_in
    }

    /// Check if the registration is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if the participant can withdraw.
    #[must_use]
    pub const fn can_withdraw(&self) -> bool {
        self.status.can_withdraw()
    }

    /// Check if this is a team registration.
    #[must_use]
    pub const fn is_team(&self) -> bool {
        self.team_season_id.is_some()
    }

    /// Check if this is an individual registration.
    #[must_use]
    pub const fn is_individual(&self) -> bool {
        self.player_id.is_some()
    }
}

/// Command to register for a tournament (team).
#[derive(Debug, Clone)]
pub struct RegisterTeamCommand {
    pub tournament_id: TournamentId,
    pub team_season_id: LeagueTeamSeasonId,
    pub participant_name: String,
    pub participant_logo_url: Option<String>,
}

/// Command to register for a tournament (individual).
#[derive(Debug, Clone)]
pub struct RegisterPlayerCommand {
    pub tournament_id: TournamentId,
    pub player_id: PlayerId,
    pub participant_name: String,
}

// =============================================================================
// TOURNAMENT MATCH
// =============================================================================

/// A match within a tournament bracket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMatch {
    pub id: TournamentMatchId,
    pub bracket_id: TournamentBracketId,
    pub stage_id: TournamentStageId,
    pub tournament_id: TournamentId,

    // Position in bracket
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,

    // Participants
    pub participant1_registration_id: Option<TournamentRegistrationId>,
    pub participant2_registration_id: Option<TournamentRegistrationId>,

    // Denormalized participant info
    pub participant1_name: Option<String>,
    pub participant1_logo_url: Option<String>,
    pub participant1_seed: Option<i32>,
    pub participant2_name: Option<String>,
    pub participant2_logo_url: Option<String>,
    pub participant2_seed: Option<i32>,

    // Source tracking
    pub participant1_source: Option<MatchParticipantSource>,
    pub participant2_source: Option<MatchParticipantSource>,

    // Match format
    pub match_format: MatchFormat,
    pub maps_required: i32,

    // Scheduling
    pub scheduled_at: Option<DateTime<Utc>>,
    pub schedule_deadline: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    // Results
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: Option<TournamentRegistrationId>,
    pub loser_registration_id: Option<TournamentRegistrationId>,

    // Progression
    pub winner_progresses_to: Option<TournamentMatchId>,
    pub loser_progresses_to: Option<TournamentMatchId>,

    // Status
    pub status: TournamentMatchStatus,

    // Disputes
    pub disputed: bool,
    pub dispute_reason: Option<String>,
    pub dispute_resolved_by: Option<UserId>,
    pub dispute_resolution: Option<String>,
    pub dispute_resolved_at: Option<DateTime<Utc>>,

    // VOD/Stream
    pub stream_url: Option<String>,
    pub vod_url: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TournamentMatch {
    /// Check if the match is pending (waiting for participants).
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.status, TournamentMatchStatus::Pending)
    }

    /// Check if the match is ready to start.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.status, TournamentMatchStatus::Ready)
    }

    /// Check if the match can be started.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        self.status.can_start()
    }

    /// Check if the match is in progress.
    #[must_use]
    pub const fn is_in_progress(&self) -> bool {
        matches!(self.status, TournamentMatchStatus::InProgress)
    }

    /// Check if the match is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.status, TournamentMatchStatus::Completed)
    }

    /// Check if the match is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// Check if results can be submitted.
    #[must_use]
    pub const fn can_submit_result(&self) -> bool {
        self.status.can_submit_result()
    }

    /// Check if the match has both participants assigned.
    #[must_use]
    pub const fn has_both_participants(&self) -> bool {
        self.participant1_registration_id.is_some() && self.participant2_registration_id.is_some()
    }

    /// Check if there's a winner.
    #[must_use]
    pub const fn has_winner(&self) -> bool {
        self.winner_registration_id.is_some()
    }

    /// Check if this is a bye match (only one participant).
    #[must_use]
    pub const fn is_bye(&self) -> bool {
        (self.participant1_registration_id.is_some() && self.participant2_registration_id.is_none())
            || (self.participant1_registration_id.is_none()
                && self.participant2_registration_id.is_some())
    }

    /// Get the number of wins required to win this match.
    #[must_use]
    pub const fn wins_required(&self) -> i32 {
        self.match_format.wins_required()
    }

    /// Check if a participant has won the match.
    #[must_use]
    pub const fn check_winner(&self) -> Option<TournamentRegistrationId> {
        let wins_needed = self.wins_required();

        if self.participant1_score >= wins_needed {
            self.participant1_registration_id
        } else if self.participant2_score >= wins_needed {
            self.participant2_registration_id
        } else {
            None
        }
    }
}

/// Command to schedule a match.
#[derive(Debug, Clone)]
pub struct ScheduleMatchCommand {
    pub match_id: TournamentMatchId,
    pub scheduled_at: DateTime<Utc>,
}

/// Command to submit a match result.
#[derive(Debug, Clone)]
pub struct SubmitMatchResultCommand {
    pub match_id: TournamentMatchId,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: TournamentRegistrationId,
}

// =============================================================================
// TOURNAMENT MATCH GAME
// =============================================================================

/// An individual game within a tournament match series.
///
/// For Bo1 matches, there's one game. For Bo3/Bo5, there are multiple games.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMatchGame {
    pub id: TournamentMatchGameId,
    pub match_id: TournamentMatchId,

    // Game number in series
    pub game_number: i32,

    // Map selection
    pub map_id: Option<String>,
    pub map_picked_by: Option<TournamentRegistrationId>,
    pub side_selection_by: Option<TournamentRegistrationId>,

    // Results
    pub participant1_score: Option<i32>,
    pub participant2_score: Option<i32>,
    pub winner_registration_id: Option<TournamentRegistrationId>,

    // Timing
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,

    // Status
    pub status: GameStatus,

    // Game-specific data
    pub game_data: serde_json::Value,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Status of an individual game within a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    #[default]
    Pending,
    MapVeto,
    InProgress,
    Completed,
    Cancelled,
}

impl std::fmt::Display for GameStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::MapVeto => write!(f, "map_veto"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for GameStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "map_veto" => Ok(Self::MapVeto),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid game status: {s}")),
        }
    }
}

impl TournamentMatchGame {
    /// Check if the game is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.status, GameStatus::Completed)
    }

    /// Check if the game is in progress.
    #[must_use]
    pub const fn is_in_progress(&self) -> bool {
        matches!(self.status, GameStatus::InProgress)
    }

    /// Check if the game has a winner.
    #[must_use]
    pub const fn has_winner(&self) -> bool {
        self.winner_registration_id.is_some()
    }
}

/// Command to submit a game result.
#[derive(Debug, Clone)]
pub struct SubmitGameResultCommand {
    pub game_id: TournamentMatchGameId,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: TournamentRegistrationId,
    pub duration_seconds: Option<i32>,
    pub game_data: Option<serde_json::Value>,
}

// =============================================================================
// TOURNAMENT MAP POOL
// =============================================================================

/// Map pool configuration for a tournament.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentMapPool {
    pub id: TournamentMapPoolId,
    pub tournament_id: TournamentId,
    pub stage_id: Option<TournamentStageId>,
    pub maps: Vec<String>,
    pub veto_format_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// =============================================================================
// TOURNAMENT STANDINGS
// =============================================================================

/// Standings for a participant in a round robin or swiss bracket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentStanding {
    pub id: uuid::Uuid,
    pub bracket_id: TournamentBracketId,
    pub registration_id: TournamentRegistrationId,

    // Position
    pub position: i32,

    // Stats
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    // Tiebreakers
    pub game_wins: i32,
    pub game_losses: i32,
    pub game_differential: i32,
    pub buchholz_score: Option<f64>,
    pub opponent_match_wins: Option<f64>,

    // Extended tiebreakers
    pub head_to_head: HeadToHead,
    pub tiebreaker_score: f64,
    pub is_tied: bool,

    // Points
    pub points: i32,

    // Timestamps
    pub updated_at: DateTime<Utc>,
}

impl TournamentStanding {
    /// Calculate win rate.
    #[must_use]
    pub fn win_rate(&self) -> Option<f64> {
        if self.matches_played == 0 {
            None
        } else {
            Some(f64::from(self.matches_won) / f64::from(self.matches_played))
        }
    }

    /// Check if this standing beats another via head-to-head.
    #[must_use]
    pub fn beats_head_to_head(&self, other: &Self) -> Option<bool> {
        if let Some(record) = self.head_to_head.get(&other.registration_id) {
            if record.wins > record.losses {
                Some(true)
            } else if record.losses > record.wins {
                Some(false)
            } else {
                None // Tied
            }
        } else {
            None
        }
    }
}

/// Head-to-head records against other participants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeadToHead {
    /// Records indexed by opponent registration ID
    pub records: std::collections::HashMap<TournamentRegistrationId, HeadToHeadRecord>,
}

impl HeadToHead {
    /// Create empty head-to-head data.
    pub fn new() -> Self {
        Self {
            records: std::collections::HashMap::new(),
        }
    }

    /// Get record against a specific opponent.
    pub fn get(&self, opponent: &TournamentRegistrationId) -> Option<&HeadToHeadRecord> {
        self.records.get(opponent)
    }

    /// Record a win against an opponent.
    pub fn record_win(&mut self, opponent: TournamentRegistrationId) {
        let entry = self.records.entry(opponent).or_default();
        entry.wins += 1;
    }

    /// Record a loss against an opponent.
    pub fn record_loss(&mut self, opponent: TournamentRegistrationId) {
        let entry = self.records.entry(opponent).or_default();
        entry.losses += 1;
    }

    /// Record a draw against an opponent.
    pub fn record_draw(&mut self, opponent: TournamentRegistrationId) {
        let entry = self.records.entry(opponent).or_default();
        entry.draws += 1;
    }
}

/// Record of matches against a specific opponent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeadToHeadRecord {
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
}

// =============================================================================
// SEEDED PARTICIPANT (for bracket generation)
// =============================================================================

/// A participant with their seed for bracket generation.
#[derive(Debug, Clone)]
pub struct SeededParticipant {
    pub registration_id: TournamentRegistrationId,
    pub seed: i32,
    pub participant_name: String,
    pub participant_logo_url: Option<String>,
}

/// A generated match for bracket creation.
#[derive(Debug, Clone)]
pub struct GeneratedMatch {
    pub round: i32,
    pub match_number: i32,
    pub bracket_position: String,
    pub participant1_source: MatchParticipantSource,
    pub participant2_source: MatchParticipantSource,
    pub winner_progresses_to_position: Option<String>,
    pub loser_progresses_to_position: Option<String>,
}

/// A bye assignment in bracket generation.
#[derive(Debug, Clone)]
pub struct ByeAssignment {
    pub seed: i32,
    pub advances_to_position: String,
}
