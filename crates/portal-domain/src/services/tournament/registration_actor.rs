//! Who speaks for a tournament registration (P-168).
//!
//! # Why this module exists
//!
//! "Is this caller allowed to act for this participant?" was answered in at
//! least three mutually contradictory ways:
//!
//! 1. `ResultService::find_user_registration` matched
//!    `registration.registered_by == user_id` — i.e. **only the one person who
//!    clicked "register"**. For a team registration that meant a co-captain,
//!    or a captain who replaced the original registrant, could not submit or
//!    confirm a result at all: they got `NotAuthorized` while the frontend
//!    (which gates on team membership) cheerfully rendered the submission
//!    panel to them. `EvidenceService` and `SchedulingService` carried
//!    hand-copies of the same test, so evidence upload and schedule proposals
//!    refused the same people.
//! 2. `raise_dispute`, the dispute thread read, the withdrawal handler and the
//!    result-review acknowledgement used "the registered player, or an active
//!    member of the registration's team-season" — five separate open-coded
//!    copies of one rule.
//! 3. So the same co-captain could raise a dispute about a result they were
//!    never allowed to submit. The backend disagreed with itself.
//!
//! This module is now the single definition. Rule (2) wins, because it is the
//! one the product already behaves as if it had: the frontend offers every
//! participant-only affordance to any active roster member, and an event where
//! only one nominated human can report a score is not how team tournaments are
//! run. Captains-only would also have been defensible, but it would have meant
//! *removing* the dispute/withdraw rights those members already have, and
//! teaching the frontend to hide four panels — strictly more product change
//! for a worse product.
//!
//! For **individual** registrations the answer is unchanged: the registered
//! player, or the user who created the row.

use portal_core::{DomainError, PlayerId, TournamentRegistrationId, UserId};

use crate::entities::tournament::{TournamentMatch, TournamentRegistration};
use crate::repositories::LeagueTeamMemberRepository;
use crate::repositories::tournament::TournamentRegistrationRepository;

/// The identity of whoever is trying to act for a registration.
///
/// Both halves are needed and they are not interchangeable: registrations are
/// owned by a `PlayerId` (and rosters are keyed by it), while the "who created
/// this row" attribution is a `UserId`. Passing only one is what made the
/// rule impossible to share between the domain services (which had the user)
/// and the handlers (which had the player).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistrationActor {
    /// The acting account.
    pub user_id: UserId,
    /// The acting player profile.
    pub player_id: PlayerId,
}

impl RegistrationActor {
    /// Create an actor from the two halves of an authenticated identity.
    #[must_use]
    pub const fn new(user_id: UserId, player_id: PlayerId) -> Self {
        Self { user_id, player_id }
    }
}

/// **THE** rule: may `actor` act on behalf of `registration`?
///
/// - Team registration: the actor must be an active member of the
///   registration's team-season (`left_at IS NULL`) — the same test
///   `is_dispute_participant` has always used.
/// - Individual (or ad-hoc) registration: the actor must be the registered
///   player, or the user who created the row.
///
/// Deliberately *not* consulted for team registrations: `registered_by`. A
/// captain who registered the team and then left it no longer speaks for it,
/// which is the mirror image of the defect this replaces.
pub async fn speaks_for_registration<LTMR>(
    member_repo: &LTMR,
    registration: &TournamentRegistration,
    actor: RegistrationActor,
) -> Result<bool, DomainError>
where
    LTMR: LeagueTeamMemberRepository + ?Sized,
{
    if let Some(team_season_id) = registration.team_season_id {
        return member_repo.is_member(team_season_id, actor.player_id).await;
    }

    Ok(registration.player_id == Some(actor.player_id)
        || registration.registered_by == actor.user_id)
}

/// Which of a match's two registrations does `actor` speak for?
///
/// Replaces the three copies of `find_user_registration` that each scanned the
/// match's two participant rows for `registered_by == user_id`.
///
/// Returns [`DomainError::NotAuthorized`] (HTTP 403) when neither side is
/// theirs — staff and spectators included.
pub async fn find_actor_registration<TRR, LTMR>(
    registration_repo: &TRR,
    member_repo: &LTMR,
    match_: &TournamentMatch,
    actor: RegistrationActor,
) -> Result<TournamentRegistrationId, DomainError>
where
    TRR: TournamentRegistrationRepository + ?Sized,
    LTMR: LeagueTeamMemberRepository + ?Sized,
{
    for reg_id in [
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ]
    .into_iter()
    .flatten()
    {
        let Some(registration) = registration_repo.find_by_id(reg_id).await? else {
            continue;
        };
        if speaks_for_registration(member_repo, &registration, actor).await? {
            return Ok(reg_id);
        }
    }

    Err(DomainError::NotAuthorized(
        "User is not authorized to act for any participant in this match".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::league_team::MockLeagueTeamMemberRepository;
    use chrono::Utc;
    use portal_core::LeagueTeamSeasonId;
    use portal_core::TournamentId;
    use portal_core::types::TournamentRegistrationStatus;

    fn registration() -> TournamentRegistration {
        TournamentRegistration {
            id: TournamentRegistrationId::new(),
            tournament_id: TournamentId::new(),
            team_season_id: None,
            player_id: None,
            adhoc_team_id: None,
            participant_name: "Subject".to_string(),
            participant_logo_url: None,
            registered_by: UserId::new(),
            registered_at: Utc::now(),
            checked_in: false,
            checked_in_at: None,
            checked_in_by: None,
            seed: None,
            seed_rating: None,
            status: TournamentRegistrationStatus::Approved,
            admin_notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            withdrawn_at: None,
        }
    }

    /// The defect: a team-mate who did not click "register" was refused.
    #[tokio::test]
    async fn test_active_roster_member_speaks_for_a_team_registration() {
        let team_season_id = LeagueTeamSeasonId::new();
        let mut reg = registration();
        reg.team_season_id = Some(team_season_id);

        let actor = RegistrationActor::new(UserId::new(), PlayerId::new());

        let mut member_repo = MockLeagueTeamMemberRepository::new();
        member_repo.expect_is_member().returning(|_, _| Ok(true));

        assert!(
            speaks_for_registration(&member_repo, &reg, actor)
                .await
                .unwrap(),
            "an active roster member must speak for their team's registration \
             even though someone else registered it"
        );
    }

    #[tokio::test]
    async fn test_non_member_does_not_speak_for_a_team_registration() {
        let mut reg = registration();
        reg.team_season_id = Some(LeagueTeamSeasonId::new());

        // Even the user who created the row loses standing once they are off
        // the roster: team authority follows the roster, not history.
        let actor = RegistrationActor::new(reg.registered_by, PlayerId::new());

        let mut member_repo = MockLeagueTeamMemberRepository::new();
        member_repo.expect_is_member().returning(|_, _| Ok(false));

        assert!(
            !speaks_for_registration(&member_repo, &reg, actor)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_individual_registration_is_unchanged() {
        let player_id = PlayerId::new();
        let mut reg = registration();
        reg.player_id = Some(player_id);

        let member_repo = MockLeagueTeamMemberRepository::new();

        // The registered player.
        assert!(
            speaks_for_registration(
                &member_repo,
                &reg,
                RegistrationActor::new(UserId::new(), player_id)
            )
            .await
            .unwrap()
        );
        // The account that created the row.
        assert!(
            speaks_for_registration(
                &member_repo,
                &reg,
                RegistrationActor::new(reg.registered_by, PlayerId::new())
            )
            .await
            .unwrap()
        );
        // A stranger.
        assert!(
            !speaks_for_registration(
                &member_repo,
                &reg,
                RegistrationActor::new(UserId::new(), PlayerId::new())
            )
            .await
            .unwrap()
        );
    }
}
