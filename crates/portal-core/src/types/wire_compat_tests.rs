//! Wire-format compatibility guard for every enum that crosses the API boundary.
//!
//! These enums were historically serialised by handlers as `value.to_string()`
//! (i.e. `Display`). P-31 types the response DTO fields as the enums themselves,
//! which serialise via serde instead. The two MUST agree: if they ever diverge,
//! the wire format changes silently and every client's comparison against that
//! status breaks at once -- precisely the class of failure P-31 exists to prevent.
//!
//! Generated from the enums in this module that implement `Display` AND have only
//! unit variants. Enums with data-carrying variants are deliberately excluded --
//! their serde representation is structural, not a bare string, so `Display`
//! equality is not the right invariant for them.

#[test]
fn demo_demo_category_display_matches_serde() {
    use crate::types::demo::DemoCategory::*;
    for v in [Uncategorized, Pug, League, Scrim, Ignored] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DemoCategory::{v:?}"
        );
    }
}

#[test]
fn demo_demo_status_display_matches_serde() {
    use crate::types::demo::DemoStatus::*;
    for v in [Pending, Processing, Ready, Failed, Archived] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DemoStatus::{v:?}"
        );
    }
}

#[test]
fn demo_demo_link_type_display_matches_serde() {
    use crate::types::demo::DemoLinkType::*;
    for v in [Manual, AutoMatched, Evidence] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DemoLinkType::{v:?}"
        );
    }
}

#[test]
fn league_team_season_status_display_matches_serde() {
    use crate::types::league_team::SeasonStatus::*;
    for v in [Draft, Registration, Active, Playoffs, Completed, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for SeasonStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_roster_lock_status_display_matches_serde() {
    use crate::types::league_team::RosterLockStatus::*;
    for v in [Open, SoftLock, HardLock] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for RosterLockStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_status_display_matches_serde() {
    use crate::types::league_team::LeagueTeamStatus::*;
    for v in [Active, Inactive, Disbanded] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_season_status_display_matches_serde() {
    use crate::types::league_team::LeagueTeamSeasonStatus::*;
    for v in [
        Forming,
        Pending,
        Registered,
        Active,
        Eliminated,
        Disqualified,
        Withdrawn,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamSeasonStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_role_display_matches_serde() {
    use crate::types::league_team::LeagueTeamRole::*;
    for v in [Captain, Player, Substitute] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamRole::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_member_status_display_matches_serde() {
    use crate::types::league_team::LeagueTeamMemberStatus::*;
    for v in [Active, Inactive, Left, Removed] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamMemberStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_invitation_type_display_matches_serde() {
    use crate::types::league_team::LeagueTeamInvitationType::*;
    for v in [Invite, Request] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamInvitationType::{v:?}"
        );
    }
}

#[test]
fn league_team_league_team_invitation_status_display_matches_serde() {
    use crate::types::league_team::LeagueTeamInvitationStatus::*;
    for v in [Pending, Accepted, Declined, Expired, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueTeamInvitationStatus::{v:?}"
        );
    }
}

#[test]
fn permission_scope_type_display_matches_serde() {
    use crate::types::permission::ScopeType::*;
    for v in [Team, League, Tournament, Match] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ScopeType::{v:?}"
        );
    }
}

#[test]
fn status_entity_status_display_matches_serde() {
    use crate::types::status::EntityStatus::*;
    for v in [Active, Inactive, Deleted] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for EntityStatus::{v:?}"
        );
    }
}

#[test]
fn status_match_status_display_matches_serde() {
    use crate::types::status::MatchStatus::*;
    for v in [
        Pending, Ready, InProgress, Completed, Cancelled, Forfeited, Disputed,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for MatchStatus::{v:?}"
        );
    }
}

#[test]
fn status_tournament_status_display_matches_serde() {
    use crate::types::status::TournamentStatus::*;
    for v in [
        Draft,
        Published,
        Registration,
        Scheduled,
        InProgress,
        Completed,
        Finalized,
        Cancelled,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TournamentStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_tournament_format_display_matches_serde() {
    use crate::types::tournament::TournamentFormat::*;
    for v in [
        SingleElimination,
        DoubleElimination,
        RoundRobin,
        Swiss,
        GroupsAndPlayoffs,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TournamentFormat::{v:?}"
        );
    }
}

#[test]
fn tournament_tournament_participant_type_display_matches_serde() {
    use crate::types::tournament::TournamentParticipantType::*;
    for v in [Team, Individual, AdHoc] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TournamentParticipantType::{v:?}"
        );
    }
}

#[test]
fn tournament_registration_type_display_matches_serde() {
    use crate::types::tournament::RegistrationType::*;
    for v in [Open, InviteOnly, Qualification, Approval] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for RegistrationType::{v:?}"
        );
    }
}

#[test]
fn tournament_scheduling_mode_display_matches_serde() {
    use crate::types::tournament::SchedulingMode::*;
    for v in [Live, SelfScheduled, Hybrid] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for SchedulingMode::{v:?}"
        );
    }
}

#[test]
fn tournament_tournament_match_status_display_matches_serde() {
    use crate::types::tournament::TournamentMatchStatus::*;
    for v in [
        Pending,
        Ready,
        Scheduled,
        CheckingIn,
        PickBan,
        InProgress,
        AwaitingResult,
        Completed,
        Cancelled,
        Forfeit,
        Disputed,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TournamentMatchStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_tournament_registration_status_display_matches_serde() {
    use crate::types::tournament::TournamentRegistrationStatus::*;
    for v in [
        Pending,
        Approved,
        CheckedIn,
        Active,
        Eliminated,
        Disqualified,
        Withdrawn,
        NoShow,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TournamentRegistrationStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_withdrawal_policy_display_matches_serde() {
    use crate::types::tournament::WithdrawalPolicy::*;
    for v in [Forfeit, Reseeding, WaitlistPromotion, AdminDecision] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for WithdrawalPolicy::{v:?}"
        );
    }
}

#[test]
fn tournament_stage_format_display_matches_serde() {
    use crate::types::tournament::StageFormat::*;
    for v in [
        SingleElimination,
        DoubleElimination,
        RoundRobin,
        Swiss,
        GroupStage,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for StageFormat::{v:?}"
        );
    }
}

#[test]
fn tournament_bracket_type_display_matches_serde() {
    use crate::types::tournament::BracketType::*;
    for v in [Winners, Losers, SingleElim, RoundRobin, Swiss, GrandFinal] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for BracketType::{v:?}"
        );
    }
}

#[test]
fn tournament_stage_status_display_matches_serde() {
    use crate::types::tournament::StageStatus::*;
    for v in [Pending, Active, Completed, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for StageStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_bracket_status_display_matches_serde() {
    use crate::types::tournament::BracketStatus::*;
    for v in [Pending, Active, Completed, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for BracketStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_advancement_rule_display_matches_serde() {
    use crate::types::tournament::AdvancementRule::*;
    for v in [TopN, TopNPerGroup, Manual] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for AdvancementRule::{v:?}"
        );
    }
}

#[test]
fn tournament_match_format_display_matches_serde() {
    use crate::types::tournament::MatchFormat::*;
    for v in [Bo1, Bo3, Bo5, Bo7] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for MatchFormat::{v:?}"
        );
    }
}

#[test]
fn tournament_seeding_algorithm_display_matches_serde() {
    use crate::types::tournament::SeedingAlgorithm::*;
    for v in [Random, Rating, SeasonRank, Manual] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for SeedingAlgorithm::{v:?}"
        );
    }
}

#[test]
fn tournament_proposal_status_display_matches_serde() {
    use crate::types::tournament::ProposalStatus::*;
    for v in [
        Pending,
        Accepted,
        Rejected,
        CounterProposed,
        Expired,
        Cancelled,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ProposalStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_exception_type_display_matches_serde() {
    use crate::types::tournament::ExceptionType::*;
    for v in [Blocked, Override] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ExceptionType::{v:?}"
        );
    }
}

#[test]
fn lineup_lineup_status_display_matches_serde() {
    use crate::types::lineup::LineupStatus::*;
    for v in [Draft, Submitted, Locked] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LineupStatus::{v:?}"
        );
    }
}

#[test]
fn lineup_lineup_source_display_matches_serde() {
    use crate::types::lineup::LineupSource::*;
    for v in [Declared, Demo, Evidence, Admin] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LineupSource::{v:?}"
        );
    }
}

#[test]
fn lineup_participation_status_display_matches_serde() {
    use crate::types::lineup::ParticipationStatus::*;
    for v in [Confirmed, NoShow, LeftEarly, Substituted, Removed] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ParticipationStatus::{v:?}"
        );
    }
}
