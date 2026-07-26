//! Wire-format compatibility guard for entity enums that cross the API boundary.
//!
//! Mirrors `portal_core::types::wire_compat_tests`. Response DTOs historically
//! serialised these as `value.to_string()` (`Display`); P-31 types the DTO fields
//! as the enums themselves, which serialise via serde. If the two ever disagree
//! the wire format changes silently and every client breaks at once, so this must
//! hold before any field is retyped.
//!
//! Only enums that already derive `Serialize` are covered. Several others reach
//! clients solely via `Display` and have no serde impl at all; retyping a DTO
//! field to one of those means first choosing its serde representation, and this
//! guard is what would then verify the choice.

#[test]
fn audit_change_type_display_matches_serde() {
    use crate::entities::audit::ChangeType::*;
    for v in [Create, Update, Delete, Revert] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ChangeType::{v:?}"
        );
    }
}

#[test]
fn ban_ban_type_display_matches_serde() {
    use crate::entities::ban::BanType::*;
    for v in [Platform, Matchmaking, Chat, League, Tournament] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for BanType::{v:?}"
        );
    }
}

#[test]
fn demo_validation_team_side_display_matches_serde() {
    use crate::entities::demo_validation::TeamSide::*;
    for v in [Team1, Team2] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for TeamSide::{v:?}"
        );
    }
}

#[test]
fn dispute_dispute_reason_display_matches_serde() {
    use crate::entities::dispute::DisputeReason::*;
    for v in [
        WrongScore,
        WrongWinner,
        Cheating,
        RuleViolation,
        TechnicalIssue,
        PlayerMisconduct,
        Other,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DisputeReason::{v:?}"
        );
    }
}

#[test]
fn dispute_dispute_status_display_matches_serde() {
    use crate::entities::dispute::DisputeStatus::*;
    for v in [Pending, UnderReview, Resolved, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DisputeStatus::{v:?}"
        );
    }
}

#[test]
fn dispute_dispute_priority_display_matches_serde() {
    use crate::entities::dispute::DisputePriority::*;
    for v in [Low, Normal, High, Urgent] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DisputePriority::{v:?}"
        );
    }
}

#[test]
fn dispute_resolution_type_display_matches_serde() {
    use crate::entities::dispute::ResolutionType::*;
    for v in [Upheld, Overturned, Rematch, Adjusted, DoubleDq] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ResolutionType::{v:?}"
        );
    }
}

#[test]
fn dispute_author_type_display_matches_serde() {
    use crate::entities::dispute::AuthorType::*;
    for v in [Participant, Admin, System] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for AuthorType::{v:?}"
        );
    }
}

#[test]
fn evidence_evidence_source_display_matches_serde() {
    use crate::entities::evidence::EvidenceSource::*;
    for v in [ManualUpload, PluginDiscovery, GameServer, ExternalApi] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for EvidenceSource::{v:?}"
        );
    }
}

#[test]
fn evidence_evidence_status_display_matches_serde() {
    use crate::entities::evidence::EvidenceStatus::*;
    for v in [Pending, Active, Expired, Deleted, Quarantined] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for EvidenceStatus::{v:?}"
        );
    }
}

#[test]
fn evidence_evidence_access_type_display_matches_serde() {
    use crate::entities::evidence::EvidenceAccessType::*;
    for v in [View, Download, Share] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for EvidenceAccessType::{v:?}"
        );
    }
}

#[test]
fn forfeit_forfeit_type_display_matches_serde() {
    use crate::entities::forfeit::ForfeitType::*;
    for v in [NoShow, Withdrawal, Disqualification, TechnicalDefault] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ForfeitType::{v:?}"
        );
    }
}

#[test]
fn league_league_access_type_display_matches_serde() {
    use crate::entities::league::LeagueAccessType::*;
    for v in [Open, InviteOnly, Application] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueAccessType::{v:?}"
        );
    }
}

#[test]
fn league_league_status_display_matches_serde() {
    use crate::entities::league::LeagueStatus::*;
    for v in [Active, Archived, Suspended] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueStatus::{v:?}"
        );
    }
}

#[test]
fn league_league_membership_type_display_matches_serde() {
    use crate::entities::league::LeagueMembershipType::*;
    for v in [Admin, Moderator, Member] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueMembershipType::{v:?}"
        );
    }
}

#[test]
fn league_league_invitation_type_display_matches_serde() {
    use crate::entities::league::LeagueInvitationType::*;
    for v in [Invite, Application] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueInvitationType::{v:?}"
        );
    }
}

#[test]
fn league_league_invitation_status_display_matches_serde() {
    use crate::entities::league::LeagueInvitationStatus::*;
    for v in [Pending, Accepted, Rejected, Expired] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueInvitationStatus::{v:?}"
        );
    }
}

#[test]
fn league_team_league_season_participant_status_display_matches_serde() {
    use crate::entities::league_team::LeagueSeasonParticipantStatus::*;
    for v in [Registered, Active, Eliminated, Disqualified, Withdrawn] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for LeagueSeasonParticipantStatus::{v:?}"
        );
    }
}

#[test]
fn result_claim_claim_status_display_matches_serde() {
    use crate::entities::result_claim::ClaimStatus::*;
    for v in [Pending, Confirmed, Disputed, Superseded, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ClaimStatus::{v:?}"
        );
    }
}

#[test]
fn result_review_result_review_status_display_matches_serde() {
    use crate::entities::result_review::ResultReviewStatus::*;
    for v in [
        PendingAcknowledgment,
        PendingAdminReview,
        Acknowledged,
        Approved,
        Rejected,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for ResultReviewStatus::{v:?}"
        );
    }
}

#[test]
fn saga_saga_status_display_matches_serde() {
    use crate::entities::saga::SagaStatus::*;
    for v in [
        Pending,
        Running,
        Paused,
        Completed,
        Failed,
        Compensating,
        Compensated,
    ] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for SagaStatus::{v:?}"
        );
    }
}

#[test]
fn saga_step_status_display_matches_serde() {
    use crate::entities::saga::StepStatus::*;
    for v in [Pending, Running, Completed, Failed, Skipped, Compensated] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for StepStatus::{v:?}"
        );
    }
}

#[test]
fn tournament_game_status_display_matches_serde() {
    use crate::entities::tournament::GameStatus::*;
    for v in [Pending, MapVeto, InProgress, Completed, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for GameStatus::{v:?}"
        );
    }
}

#[test]
fn veto_veto_status_display_matches_serde() {
    use crate::entities::veto::VetoStatus::*;
    for v in [Pending, CoinFlip, InProgress, Completed, Cancelled] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for VetoStatus::{v:?}"
        );
    }
}

#[test]
fn veto_map_veto_status_display_matches_serde() {
    use crate::entities::veto::MapVetoStatus::*;
    for v in [Available, Banned, Picked, Decider] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for MapVetoStatus::{v:?}"
        );
    }
}

#[test]
fn veto_delegate_delegated_by_role_display_matches_serde() {
    use crate::entities::veto_delegate::DelegatedByRole::*;
    for v in [Captain, Owner, TournamentAdmin] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for DelegatedByRole::{v:?}"
        );
    }
}

#[test]
fn veto_lobby_message_veto_message_type_display_matches_serde() {
    use crate::entities::veto_lobby_message::VetoMessageType::*;
    for v in [Team, All, Admin, System] {
        assert_eq!(
            serde_json::Value::String(v.to_string()),
            serde_json::to_value(v).unwrap(),
            "Display and Serialize disagree for VetoMessageType::{v:?}"
        );
    }
}
