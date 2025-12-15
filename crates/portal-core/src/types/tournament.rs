//! Tournament-specific types and enums.
//!
//! These types are used for tournament management, bracket generation,
//! and match scheduling.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ============================================================================
// Tournament Format
// ============================================================================

/// The format/structure of a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TournamentFormat {
    /// Standard single elimination bracket.
    #[default]
    SingleElimination,
    /// Double elimination with winners and losers brackets.
    DoubleElimination,
    /// Round robin - everyone plays everyone.
    RoundRobin,
    /// Swiss system - dynamic pairing based on standings.
    Swiss,
    /// Group stage followed by playoff bracket.
    GroupsAndPlayoffs,
}

impl fmt::Display for TournamentFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SingleElimination => write!(f, "single_elimination"),
            Self::DoubleElimination => write!(f, "double_elimination"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Swiss => write!(f, "swiss"),
            Self::GroupsAndPlayoffs => write!(f, "groups_and_playoffs"),
        }
    }
}

impl FromStr for TournamentFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "single_elimination" => Ok(Self::SingleElimination),
            "double_elimination" => Ok(Self::DoubleElimination),
            "round_robin" => Ok(Self::RoundRobin),
            "swiss" => Ok(Self::Swiss),
            "groups_and_playoffs" => Ok(Self::GroupsAndPlayoffs),
            _ => Err(format!("invalid tournament format: {s}")),
        }
    }
}

impl TournamentFormat {
    /// Check if the format supports losers bracket.
    #[must_use]
    pub const fn has_losers_bracket(&self) -> bool {
        matches!(self, Self::DoubleElimination)
    }

    /// Check if the format uses standings (not bracket position).
    #[must_use]
    pub const fn uses_standings(&self) -> bool {
        matches!(self, Self::RoundRobin | Self::Swiss)
    }

    /// Check if the format supports bye allocation.
    #[must_use]
    pub const fn supports_byes(&self) -> bool {
        matches!(self, Self::SingleElimination | Self::DoubleElimination)
    }
}

// ============================================================================
// Participant Type
// ============================================================================

/// The type of participants in a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TournamentParticipantType {
    /// Team-based tournament using league team rosters.
    #[default]
    Team,
    /// Individual player tournament (1v1).
    Individual,
    /// Ad-hoc teams formed at registration time.
    AdHoc,
}

impl fmt::Display for TournamentParticipantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Team => write!(f, "team"),
            Self::Individual => write!(f, "individual"),
            Self::AdHoc => write!(f, "adhoc"),
        }
    }
}

impl FromStr for TournamentParticipantType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "team" => Ok(Self::Team),
            "individual" => Ok(Self::Individual),
            "adhoc" => Ok(Self::AdHoc),
            _ => Err(format!("invalid participant type: {s}")),
        }
    }
}

impl TournamentParticipantType {
    /// Check if this type requires a team size configuration.
    #[must_use]
    pub const fn requires_team_size(&self) -> bool {
        matches!(self, Self::Team | Self::AdHoc)
    }
}

// ============================================================================
// Registration Type
// ============================================================================

/// How registration is handled for a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationType {
    /// Anyone can register.
    #[default]
    Open,
    /// Only invited participants can register.
    InviteOnly,
    /// Must qualify through another tournament.
    Qualification,
    /// Open registration but requires admin approval.
    Approval,
}

impl fmt::Display for RegistrationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::InviteOnly => write!(f, "invite_only"),
            Self::Qualification => write!(f, "qualification"),
            Self::Approval => write!(f, "approval"),
        }
    }
}

impl FromStr for RegistrationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "invite_only" => Ok(Self::InviteOnly),
            "qualification" => Ok(Self::Qualification),
            "approval" => Ok(Self::Approval),
            _ => Err(format!("invalid registration type: {s}")),
        }
    }
}

impl RegistrationType {
    /// Check if registration requires approval.
    #[must_use]
    pub const fn requires_approval(&self) -> bool {
        matches!(self, Self::InviteOnly | Self::Qualification | Self::Approval)
    }
}

// ============================================================================
// Scheduling Mode
// ============================================================================

/// How matches are scheduled in a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingMode {
    /// All matches at fixed times, played in real-time.
    #[default]
    Live,
    /// Participants schedule matches within deadlines.
    SelfScheduled,
    /// Mix of live and self-scheduled rounds.
    Hybrid,
}

impl fmt::Display for SchedulingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Live => write!(f, "live"),
            Self::SelfScheduled => write!(f, "self_scheduled"),
            Self::Hybrid => write!(f, "hybrid"),
        }
    }
}

impl FromStr for SchedulingMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "live" => Ok(Self::Live),
            "self_scheduled" => Ok(Self::SelfScheduled),
            "hybrid" => Ok(Self::Hybrid),
            _ => Err(format!("invalid scheduling mode: {s}")),
        }
    }
}

// ============================================================================
// Tournament Match Status
// ============================================================================

/// Status of a tournament match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TournamentMatchStatus {
    /// Waiting for participants to be determined.
    #[default]
    Pending,
    /// Both participants set, awaiting scheduling/start.
    Ready,
    /// Match has been scheduled.
    Scheduled,
    /// Pre-match check-in phase.
    CheckingIn,
    /// Map veto/pick in progress.
    PickBan,
    /// Match is being played.
    InProgress,
    /// Waiting for result submission.
    AwaitingResult,
    /// Match completed normally.
    Completed,
    /// Match cancelled (bye, etc.).
    Cancelled,
    /// One participant forfeited.
    Forfeit,
    /// Result under dispute.
    Disputed,
}

impl fmt::Display for TournamentMatchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::Scheduled => write!(f, "scheduled"),
            Self::CheckingIn => write!(f, "checking_in"),
            Self::PickBan => write!(f, "pick_ban"),
            Self::InProgress => write!(f, "in_progress"),
            Self::AwaitingResult => write!(f, "awaiting_result"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Forfeit => write!(f, "forfeit"),
            Self::Disputed => write!(f, "disputed"),
        }
    }
}

impl FromStr for TournamentMatchStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "scheduled" => Ok(Self::Scheduled),
            "checking_in" => Ok(Self::CheckingIn),
            "pick_ban" => Ok(Self::PickBan),
            "in_progress" => Ok(Self::InProgress),
            "awaiting_result" => Ok(Self::AwaitingResult),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "forfeit" => Ok(Self::Forfeit),
            "disputed" => Ok(Self::Disputed),
            _ => Err(format!("invalid tournament match status: {s}")),
        }
    }
}

impl TournamentMatchStatus {
    /// Check if the match is in a terminal state.
    ///
    /// Terminal states are final - no further transitions are possible.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Forfeit
        )
    }

    /// Check if the match can be started.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(self, Self::Ready | Self::Scheduled | Self::CheckingIn)
    }

    /// Check if the match is awaiting action (actively in progress).
    ///
    /// Active states require participant or system action to proceed.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::CheckingIn | Self::PickBan | Self::InProgress | Self::AwaitingResult
        )
    }

    /// Check if the match can accept result submission.
    #[must_use]
    pub const fn can_submit_result(&self) -> bool {
        matches!(self, Self::InProgress | Self::AwaitingResult)
    }

    /// Check if the match can be scheduled.
    #[must_use]
    pub const fn can_schedule(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Check if the match can be forfeited.
    ///
    /// Forfeit is possible from any active state (after scheduling).
    #[must_use]
    pub const fn can_forfeit(&self) -> bool {
        matches!(
            self,
            Self::Scheduled
                | Self::CheckingIn
                | Self::PickBan
                | Self::InProgress
                | Self::AwaitingResult
        )
    }

    /// Check if the match can be cancelled.
    ///
    /// Cancellation is only possible before the match becomes active.
    #[must_use]
    pub const fn can_cancel(&self) -> bool {
        matches!(self, Self::Pending | Self::Ready | Self::Scheduled)
    }

    /// Check if the match is in a disputeable state.
    #[must_use]
    pub const fn can_dispute(&self) -> bool {
        matches!(self, Self::AwaitingResult)
    }

    /// Get the allowed transitions from this state.
    ///
    /// This implements the match lifecycle state machine as defined in the
    /// Phase 3 design document.
    #[must_use]
    pub fn allowed_transitions(&self) -> Vec<Self> {
        match self {
            // Pending: waiting for both participants to be determined
            Self::Pending => vec![Self::Ready, Self::Cancelled],
            // Ready: both participants set, can be scheduled
            Self::Ready => vec![Self::Scheduled, Self::Cancelled],
            // Scheduled: time assigned, can transition based on requirements
            Self::Scheduled => vec![
                Self::CheckingIn,
                Self::PickBan,
                Self::InProgress,
                Self::Forfeit,
                Self::Cancelled,
            ],
            // CheckingIn: pre-match check-in phase
            Self::CheckingIn => vec![Self::PickBan, Self::InProgress, Self::Forfeit],
            // PickBan: map veto in progress
            Self::PickBan => vec![Self::InProgress, Self::Forfeit],
            // InProgress: match being played
            Self::InProgress => vec![Self::AwaitingResult, Self::Forfeit],
            // AwaitingResult: waiting for result submission
            Self::AwaitingResult => vec![Self::Completed, Self::Disputed, Self::Forfeit],
            // Disputed: result under admin review
            Self::Disputed => vec![Self::Completed],
            // Terminal states - no further transitions
            Self::Completed | Self::Forfeit | Self::Cancelled => vec![],
        }
    }

    /// Valid transitions from this status.
    ///
    /// Alias for `allowed_transitions` for backwards compatibility.
    #[must_use]
    pub fn valid_transitions(&self) -> Vec<Self> {
        self.allowed_transitions()
    }

    /// Check if a transition to the given status is valid.
    #[must_use]
    pub fn can_transition_to(&self, target: Self) -> bool {
        self.allowed_transitions().contains(&target)
    }
}

// ============================================================================
// Registration Status
// ============================================================================

/// Status of a tournament registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TournamentRegistrationStatus {
    /// Awaiting approval.
    #[default]
    Pending,
    /// Approved, awaiting check-in.
    Approved,
    /// Checked in, ready to play.
    CheckedIn,
    /// Currently competing.
    Active,
    /// Eliminated from tournament.
    Eliminated,
    /// Removed for rule violation.
    Disqualified,
    /// Voluntarily withdrew.
    Withdrawn,
    /// Failed to check in.
    NoShow,
}

impl fmt::Display for TournamentRegistrationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::CheckedIn => write!(f, "checked_in"),
            Self::Active => write!(f, "active"),
            Self::Eliminated => write!(f, "eliminated"),
            Self::Disqualified => write!(f, "disqualified"),
            Self::Withdrawn => write!(f, "withdrawn"),
            Self::NoShow => write!(f, "no_show"),
        }
    }
}

impl FromStr for TournamentRegistrationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "checked_in" => Ok(Self::CheckedIn),
            "active" => Ok(Self::Active),
            "eliminated" => Ok(Self::Eliminated),
            "disqualified" => Ok(Self::Disqualified),
            "withdrawn" => Ok(Self::Withdrawn),
            "no_show" => Ok(Self::NoShow),
            _ => Err(format!("invalid registration status: {s}")),
        }
    }
}

impl TournamentRegistrationStatus {
    /// Check if the registration is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Eliminated | Self::Disqualified | Self::Withdrawn | Self::NoShow
        )
    }

    /// Check if the participant can compete.
    #[must_use]
    pub const fn can_compete(&self) -> bool {
        matches!(self, Self::CheckedIn | Self::Active)
    }

    /// Check if the participant can check in.
    #[must_use]
    pub const fn can_check_in(&self) -> bool {
        matches!(self, Self::Approved)
    }

    /// Check if the participant can withdraw.
    #[must_use]
    pub const fn can_withdraw(&self) -> bool {
        matches!(self, Self::Pending | Self::Approved | Self::CheckedIn | Self::Active)
    }
}

// ============================================================================
// Withdrawal Policy
// ============================================================================

/// How to handle participant withdrawal during a tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalPolicy {
    /// Opponent advances with walkover.
    #[default]
    Forfeit,
    /// Remaining participants are reseeded.
    Reseeding,
    /// Next on waitlist takes the slot.
    WaitlistPromotion,
    /// Manual admin intervention required.
    AdminDecision,
}

impl fmt::Display for WithdrawalPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forfeit => write!(f, "forfeit"),
            Self::Reseeding => write!(f, "reseeding"),
            Self::WaitlistPromotion => write!(f, "waitlist_promotion"),
            Self::AdminDecision => write!(f, "admin_decision"),
        }
    }
}

impl FromStr for WithdrawalPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "forfeit" => Ok(Self::Forfeit),
            "reseeding" => Ok(Self::Reseeding),
            "waitlist_promotion" => Ok(Self::WaitlistPromotion),
            "admin_decision" => Ok(Self::AdminDecision),
            _ => Err(format!("invalid withdrawal policy: {s}")),
        }
    }
}

// ============================================================================
// Stage Types
// ============================================================================

/// Format of a tournament stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StageFormat {
    /// Single elimination bracket.
    #[default]
    SingleElimination,
    /// Double elimination with losers bracket.
    DoubleElimination,
    /// Round robin format.
    RoundRobin,
    /// Swiss format.
    Swiss,
    /// Group stage (multiple round robins).
    GroupStage,
}

impl fmt::Display for StageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SingleElimination => write!(f, "single_elimination"),
            Self::DoubleElimination => write!(f, "double_elimination"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Swiss => write!(f, "swiss"),
            Self::GroupStage => write!(f, "group_stage"),
        }
    }
}

impl FromStr for StageFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "single_elimination" => Ok(Self::SingleElimination),
            "double_elimination" => Ok(Self::DoubleElimination),
            "round_robin" => Ok(Self::RoundRobin),
            "swiss" => Ok(Self::Swiss),
            "group_stage" => Ok(Self::GroupStage),
            _ => Err(format!("invalid stage format: {s}")),
        }
    }
}

/// Type of bracket within a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BracketType {
    /// Upper/winners bracket in double elimination.
    Winners,
    /// Lower/losers bracket in double elimination.
    Losers,
    /// Standard single elimination bracket.
    #[default]
    SingleElim,
    /// Round robin within a group.
    RoundRobin,
    /// Swiss pairing bracket.
    Swiss,
    /// Grand final match(es).
    GrandFinal,
}

impl fmt::Display for BracketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Winners => write!(f, "winners"),
            Self::Losers => write!(f, "losers"),
            Self::SingleElim => write!(f, "single_elim"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Swiss => write!(f, "swiss"),
            Self::GrandFinal => write!(f, "grand_final"),
        }
    }
}

impl FromStr for BracketType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "winners" => Ok(Self::Winners),
            "losers" => Ok(Self::Losers),
            "single_elim" => Ok(Self::SingleElim),
            "round_robin" => Ok(Self::RoundRobin),
            "swiss" => Ok(Self::Swiss),
            "grand_final" => Ok(Self::GrandFinal),
            _ => Err(format!("invalid bracket type: {s}")),
        }
    }
}

/// Status of a tournament stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    /// Stage not yet started.
    #[default]
    Pending,
    /// Stage is currently active.
    Active,
    /// Stage has been completed.
    Completed,
    /// Stage was cancelled.
    Cancelled,
}

impl fmt::Display for StageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for StageStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid stage status: {s}")),
        }
    }
}

impl StageStatus {
    /// Check if the stage is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Status of a tournament bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BracketStatus {
    /// Bracket not yet started.
    #[default]
    Pending,
    /// Bracket is currently active.
    Active,
    /// Bracket has been completed.
    Completed,
    /// Bracket was cancelled.
    Cancelled,
}

impl fmt::Display for BracketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Active => write!(f, "active"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for BracketStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid bracket status: {s}")),
        }
    }
}

impl BracketStatus {
    /// Check if the bracket is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Rule for advancement from one stage to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementRule {
    /// Top N by standing advance.
    #[default]
    TopN,
    /// Top N from each group advance.
    TopNPerGroup,
    /// Manual selection by admin.
    Manual,
}

impl fmt::Display for AdvancementRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopN => write!(f, "top_n"),
            Self::TopNPerGroup => write!(f, "top_n_per_group"),
            Self::Manual => write!(f, "manual"),
        }
    }
}

impl FromStr for AdvancementRule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "top_n" => Ok(Self::TopN),
            "top_n_per_group" => Ok(Self::TopNPerGroup),
            "manual" => Ok(Self::Manual),
            _ => Err(format!("invalid advancement rule: {s}")),
        }
    }
}

// ============================================================================
// Match Format
// ============================================================================

/// Format of a match (best of N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MatchFormat {
    /// Best of 1.
    #[default]
    Bo1,
    /// Best of 3.
    Bo3,
    /// Best of 5.
    Bo5,
    /// Best of 7.
    Bo7,
}

impl fmt::Display for MatchFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bo1 => write!(f, "bo1"),
            Self::Bo3 => write!(f, "bo3"),
            Self::Bo5 => write!(f, "bo5"),
            Self::Bo7 => write!(f, "bo7"),
        }
    }
}

impl FromStr for MatchFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bo1" => Ok(Self::Bo1),
            "bo3" => Ok(Self::Bo3),
            "bo5" => Ok(Self::Bo5),
            "bo7" => Ok(Self::Bo7),
            _ => Err(format!("invalid match format: {s}")),
        }
    }
}

impl MatchFormat {
    /// Get the number of games in this format.
    #[must_use]
    pub const fn game_count(&self) -> i32 {
        match self {
            Self::Bo1 => 1,
            Self::Bo3 => 3,
            Self::Bo5 => 5,
            Self::Bo7 => 7,
        }
    }

    /// Get the number of wins required to win the match.
    #[must_use]
    pub const fn wins_required(&self) -> i32 {
        (self.game_count() / 2) + 1
    }
}

// ============================================================================
// Seeding Algorithm
// ============================================================================

/// Algorithm used for seeding participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SeedingAlgorithm {
    /// Random seeding.
    #[default]
    Random,
    /// Based on player/team rating.
    Rating,
    /// Based on seasonal ranking.
    SeasonRank,
    /// Manual seeding by admin.
    Manual,
}

impl fmt::Display for SeedingAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Random => write!(f, "random"),
            Self::Rating => write!(f, "rating"),
            Self::SeasonRank => write!(f, "season_rank"),
            Self::Manual => write!(f, "manual"),
        }
    }
}

impl FromStr for SeedingAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "random" => Ok(Self::Random),
            "rating" => Ok(Self::Rating),
            "season_rank" => Ok(Self::SeasonRank),
            "manual" => Ok(Self::Manual),
            _ => Err(format!("invalid seeding algorithm: {s}")),
        }
    }
}

// ============================================================================
// Match Participant Source
// ============================================================================

/// Describes where a match participant comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchParticipantSource {
    /// Direct seed from registration.
    Seed(i32),
    /// Winner of a previous match.
    WinnerOf(String), // Match bracket position (e.g., "W1-1")
    /// Loser of a previous match (double elim).
    LoserOf(String),
    /// Bye - no opponent.
    Bye,
}

impl fmt::Display for MatchParticipantSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Seed(n) => write!(f, "seed_{n}"),
            Self::WinnerOf(pos) => write!(f, "winner_of_{pos}"),
            Self::LoserOf(pos) => write!(f, "loser_of_{pos}"),
            Self::Bye => write!(f, "bye"),
        }
    }
}

// ============================================================================
// Schedule Proposal Status
// ============================================================================

/// Status of a schedule proposal in the negotiation workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Proposal awaiting response from opponent.
    #[default]
    Pending,
    /// Proposal accepted, match scheduled.
    Accepted,
    /// Proposal rejected.
    Rejected,
    /// Counter-proposal was made.
    CounterProposed,
    /// Proposal expired without response.
    Expired,
    /// Proposal cancelled by proposer.
    Cancelled,
}

impl fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
            Self::Rejected => write!(f, "rejected"),
            Self::CounterProposed => write!(f, "counter_proposed"),
            Self::Expired => write!(f, "expired"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl FromStr for ProposalStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "counter_proposed" => Ok(Self::CounterProposed),
            "expired" => Ok(Self::Expired),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("invalid proposal status: {s}")),
        }
    }
}

impl ProposalStatus {
    /// Check if the proposal is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Rejected | Self::Expired | Self::Cancelled
        )
    }

    /// Check if the proposal can still be responded to.
    #[must_use]
    pub const fn can_respond(&self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Get the status as a string slice.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::CounterProposed => "counter_proposed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    /// Check if a transition to the given status is valid.
    #[must_use]
    pub fn can_transition_to(&self, target: Self) -> bool {
        matches!(
            (self, target),
            (
                Self::Pending,
                Self::Accepted
                    | Self::Rejected
                    | Self::CounterProposed
                    | Self::Expired
                    | Self::Cancelled
            )
        )
    }
}

// ============================================================================
// Availability Exception Type
// ============================================================================

/// Type of availability exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionType {
    /// Player is completely blocked (unavailable) on this date.
    #[default]
    Blocked,
    /// Custom override hours for this specific date.
    Override,
}

impl fmt::Display for ExceptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocked => write!(f, "blocked"),
            Self::Override => write!(f, "override"),
        }
    }
}

impl FromStr for ExceptionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "blocked" => Ok(Self::Blocked),
            "override" => Ok(Self::Override),
            _ => Err(format!("invalid exception type: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tournament_format_roundtrip() {
        for format in [
            TournamentFormat::SingleElimination,
            TournamentFormat::DoubleElimination,
            TournamentFormat::RoundRobin,
            TournamentFormat::Swiss,
            TournamentFormat::GroupsAndPlayoffs,
        ] {
            let s = format.to_string();
            let parsed: TournamentFormat = s.parse().unwrap();
            assert_eq!(format, parsed);
        }
    }

    #[test]
    fn test_match_format_games() {
        assert_eq!(MatchFormat::Bo1.game_count(), 1);
        assert_eq!(MatchFormat::Bo1.wins_required(), 1);

        assert_eq!(MatchFormat::Bo3.game_count(), 3);
        assert_eq!(MatchFormat::Bo3.wins_required(), 2);

        assert_eq!(MatchFormat::Bo5.game_count(), 5);
        assert_eq!(MatchFormat::Bo5.wins_required(), 3);
    }

    #[test]
    fn test_tournament_match_status_transitions() {
        assert!(TournamentMatchStatus::Pending.can_transition_to(TournamentMatchStatus::Ready));
        assert!(!TournamentMatchStatus::Pending.can_transition_to(TournamentMatchStatus::Completed));
        assert!(TournamentMatchStatus::InProgress.can_transition_to(TournamentMatchStatus::Completed));
    }

    #[test]
    fn test_registration_status() {
        assert!(!TournamentRegistrationStatus::Pending.can_compete());
        assert!(TournamentRegistrationStatus::CheckedIn.can_compete());
        assert!(TournamentRegistrationStatus::Active.can_compete());
        assert!(TournamentRegistrationStatus::Eliminated.is_terminal());
    }
}
