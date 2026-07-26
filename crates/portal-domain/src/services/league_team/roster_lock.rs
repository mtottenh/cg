//! The **single** roster-lock enforcement point.
//!
//! # Why this module exists (P-15)
//!
//! The roster lock used to be enforced by copy-pasted `if` blocks inside each
//! service method, and the copies had drifted apart:
//!
//! * `LeagueTeamService::add_member_authorized` asked **both** questions —
//!   `allows_primary_roster_changes()` for captains/players **and**
//!   `allows_substitute_changes()` for substitutes.
//! * `LeagueTeamInvitationService::create_invitation` / `accept_invitation`
//!   asked only the *primary* question, so a **substitute** could be invited
//!   and seated under a `hard_lock` — the invitation path silently bypassed a
//!   lock the direct path enforced.
//! * `create_join_request` asked neither.
//! * `LeagueTeamService::leave_team` asked only the primary question, so a
//!   substitute could walk off a hard-locked roster.
//!
//! Every one of those is the *same* defect: more than one place decides what
//! the lock means. So there is now exactly one — [`enforce_roster_lock`] — and
//! every path that mutates a seasonal roster goes through it. Adding a second
//! copy of this check anywhere is how P-15 comes back.
//!
//! # Why the admin override lives here too (P-18)
//!
//! The check used to be unconditional, which meant a banned player could not be
//! substituted mid-playoffs by anybody, including a platform admin. The bypass
//! therefore has to exist — but an unaudited bypass makes the lock meaningless,
//! so it is expressed as [`AuditedOverride`]: the *only* way to skip the check
//! is to hand this function an audit sink, and the audit row is written with
//! `?` **before** `Ok(())` is returned. If the audit write fails, the override
//! fails. There is no code path that bypasses the lock without recording it.
//!
//! An audit row is written **only when a refusal was actually overridden** — if
//! the lock would have allowed the change anyway, the override is a no-op and
//! nothing is recorded. That makes the presence of a row mean exactly one
//! thing: "a roster lock was bypassed here, by this person, for this reason".
//!
//! # What the lock means since P-148
//!
//! The lock is an **optional, per-season decision**, and it is the *only* thing
//! that decides whether a live roster may change. The season's *phase* no
//! longer gates roster composition; the single surviving status rule is that a
//! terminal (`completed` / `cancelled`) season is frozen. See
//! [`refusal_reason`] for the ruling, the reasoning and the P-147 founding
//! exemption that goes with it.

use crate::entities::audit::ChangeType;
use crate::entities::league_team::LeagueSeason;
use crate::repositories::{CreateEntityChange, EntityChangeRepository};
use portal_core::types::LeagueTeamRole;
use portal_core::{DomainError, LeagueTeamSeasonId, PlayerId};
use tracing::warn;

/// The kind of roster mutation being attempted.
///
/// The distinction matters because the two `RosterLockStatus` predicates are
/// not interchangeable: `soft_lock` permits substitute churn while freezing the
/// primary roster, and a captaincy change is neither of those things.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterChange {
    /// A member holding `role` is being seated on, or lifted off, the roster
    /// (add / remove / invite / accept / leave).
    Membership(LeagueTeamRole),

    /// An existing member is moving between the two **primary** roles
    /// (captain <-> player) — see [`enforce_roster_lock`] for why this is
    /// gated differently (P-16).
    Role,

    /// A roster is being **created**: `create_team` / `register_for_season`
    /// seat the founding captain as part of entering a season (P-147).
    ///
    /// Reached through [`ensure_roster_may_be_founded`], never through
    /// [`enforce_roster_lock`] — there is no `league_team_seasons` row to
    /// attribute an audit entry to yet, and this variant is never refused by
    /// the lock, so an override could never be taken. See
    /// [`refusal_reason`] for why it is exempt.
    Founding,
}

impl RosterChange {
    /// Short machine-readable label, recorded in the audit trail.
    const fn label(self) -> &'static str {
        match self {
            Self::Membership(LeagueTeamRole::Captain) => "membership:captain",
            Self::Membership(LeagueTeamRole::Player) => "membership:player",
            Self::Membership(LeagueTeamRole::Substitute) => "membership:substitute",
            Self::Role => "role_change",
            Self::Founding => "founding_captain",
        }
    }
}

/// An admin's emergency bypass of the roster lock (P-18).
///
/// Constructed at the HTTP boundary, which is the only layer that knows whether
/// the caller actually holds the platform team-admin override and which request
/// this was.
#[derive(Debug, Clone)]
pub struct RosterLockOverride {
    /// The player record of the admin who invoked the override.
    pub overridden_by: PlayerId,
    /// The operator-supplied justification. Required, and never empty — the
    /// HTTP layer rejects a blank reason before this is built.
    pub reason: String,
    /// Correlation id of the request that carried the override, if any.
    pub request_id: Option<String>,
}

/// An override plus the sink it must be recorded in.
///
/// Deliberately bundled: the type system makes it impossible to pass an
/// override without also passing somewhere to record it.
pub(crate) struct AuditedOverride<'a> {
    pub audit: &'a dyn EntityChangeRepository,
    pub over: &'a RosterLockOverride,
}

/// Why this change is refused, or `None` if the lock permits it.
///
/// This is the one place the lock's meaning is decided.
///
/// * A **terminal** season (`completed` / `cancelled`) refuses everything, lock
///   or no lock — see below.
/// * Otherwise **membership of a primary role** (captain / player) needs
///   `allows_primary_roster_changes()`.
/// * **Membership of a substitute** needs `allows_substitute_changes()` — which
///   `soft_lock` grants and `hard_lock` does not.
/// * **A role change** (captain <-> player) is gated on the lock **only**, and
///   only by `hard_lock`. See the note on [`RosterChange::Role`] below.
/// * **Founding** a roster is not lock-gated at all — see below.
///
/// ## P-148: the lock decides, not the season phase
///
/// Until this fix every `allows_*` predicate was ANDed with
/// `SeasonStatus::allows_roster_changes()` (`draft | registration`), so season
/// status was the outer gate and the lock only ever had a say *before* the
/// competition started. The lock was inert in exactly the window it was sold
/// as protecting.
///
/// The owner ruled that the lock is an **optional, per-season decision**: *"I
/// think the roster lock should really be an 'optional' thing/thing that is a
/// per tournament decision, (again, this is a casual league, so adding team
/// members half way through may be okay)."* So `draft`, `registration`,
/// `active` and `playoffs` all defer to `roster_lock_status`, whose DB default
/// is `open` (migration 0025) — a casual league gets casual behaviour for
/// free, and a league that wants strictness sets `soft_lock` or `hard_lock`
/// when it chooses to.
///
/// The one status rule that survives is `is_terminal()`. A `completed` or
/// `cancelled` season's roster is the historical record of who played; letting
/// it move would corrupt that, and "optional" was never meant to include
/// rewriting finished seasons. It is checked first, and it applies to *every*
/// variant, including [`RosterChange::Role`] — which was previously ungated on
/// status entirely.
///
/// ## P-147: why founding a roster is exempt
///
/// `create_team` and `register_for_season` seat the founding captain via
/// `create_team_with_season_and_captain` / `create_with_captain` without
/// passing through [`enforce_roster_lock`]. Making that omission explicit is
/// the point of [`RosterChange::Founding`]: the exemption is now a decision
/// recorded *here*, in the one place the lock's meaning is decided, instead of
/// a hole left by two call sites that never asked.
///
/// It is exempt because **whether a season accepts new teams is already
/// decided, once, by `is_registration_open()` / `can_register_team()`** —
/// status must be `registration` and `now` must be inside the registration
/// window. Making the lock a second veto on registration would give the
/// product two knobs for one question and let them disagree: a season would
/// report `status: "registration"` inside its advertised window while
/// `POST .../teams` returned 400, with nothing in the season payload
/// explaining why. An operator who wants to stop new teams closes registration
/// — that control exists and is the one clients read.
///
/// The lock governs how an **existing** roster may change. Note the
/// consequence is narrow: once a season leaves `registration`,
/// `can_register_team()` is false anyway, so no team can be founded mid-season
/// however the lock is set. The only reachable case is a hard-locked season
/// that is *still* in its registration window, which is the operator's own
/// deliberate configuration.
///
/// ## P-16: why a role change is gated differently
///
/// A promotion or demotion moves a member between two roles that are *both*
/// primary (`LeagueTeamRole::is_primary()` is true for `Captain` and `Player`),
/// so it never changes which players are eligible to compete — it is not a
/// roster change in the sense the lock exists to prevent.
///
/// * Under `soft_lock` ("minor changes allowed") it is **permitted**: the lock
///   already permits substitute swaps, which affect the competing roster far
///   more than who holds the armband. Refusing it would make the API stricter
///   than its own documented semantics, and stricter than the admin UI, which
///   deliberately leaves promote/demote enabled under a soft lock.
/// * Under `hard_lock` ("no roster changes allowed") it is **refused**:
///   captaincy *is* the authority that performs roster changes — invite, remove
///   and accept-request are all `is_captain`-gated — so letting it move during
///   a freeze reopens the freeze by proxy. The admin UI already disables the
///   whole member-action menu under a hard lock, so this is the direction that
///   closes the divergence without turning a live control into a lie.
/// * It is **not** gated on season status. Unlike add/remove, a role change
///   does not alter the competing roster, and a season in `active`/`playoffs`
///   with an open lock legitimately still needs to be able to name a captain
///   (the incumbent quits, is banned, or goes silent). Gating on season status
///   would make promote/demote impossible for the entire competitive phase.
fn refusal_reason(season: &LeagueSeason, change: RosterChange) -> Option<String> {
    // P-148: the only season-status rule left. A finished or abandoned season's
    // roster is the record of who played it, so it is frozen for every kind of
    // change and by every caller — an admin override cannot buy past this
    // either, because the reason it refuses is not the lock.
    if season.status.is_terminal() {
        return Some(format!(
            "season status '{}' is final; its roster is now history and cannot be changed",
            season.status
        ));
    }

    match change {
        // P-147. Governed by `is_registration_open()` / `can_register_team()`,
        // not by the lock. Deliberate and stated here rather than left implicit
        // at the two call sites.
        RosterChange::Founding => None,
        RosterChange::Membership(role) => {
            let allowed = if role.is_primary() {
                season.allows_primary_roster_changes()
            } else {
                season.allows_substitute_changes()
            };
            if allowed {
                return None;
            }
            // Name which half of the lock refused: "the roster is locked" alone
            // does not tell a captain that a substitute would still be accepted.
            let who = if role.is_primary() {
                "primary member"
            } else {
                "substitute"
            };
            Some(format!("roster is locked for {who} changes"))
        }
        RosterChange::Role => {
            if season.roster_lock_status.allows_any_changes() {
                None
            } else {
                Some(
                    "roster is locked: captaincy changes are not permitted under a hard lock"
                        .to_string(),
                )
            }
        }
    }
}

/// Enforce the roster lock for the **creation** of a roster (P-147).
///
/// `create_team` and `register_for_season` seat a founding captain before any
/// `league_team_seasons` row exists, so there is nothing to attribute an audit
/// row to and no override to take — which is why this is a separate, synchronous
/// entry point rather than a call to [`enforce_roster_lock`]. It still routes
/// the decision through [`refusal_reason`], so the exemption is a rule stated
/// in one place instead of a check two call sites forgot to make.
pub(crate) fn ensure_roster_may_be_founded(season: &LeagueSeason) -> Result<(), DomainError> {
    match refusal_reason(season, RosterChange::Founding) {
        Some(refusal) => Err(DomainError::InvalidState(refusal)),
        None => Ok(()),
    }
}

/// Enforce the roster lock for `change` on `season`.
///
/// `change` is a mutation of an **existing** roster; use
/// [`ensure_roster_may_be_founded`] for [`RosterChange::Founding`], which has no
/// `team_season_id` to attribute an override to.
///
/// Returns `Ok(())` when the lock permits the change, or when `override_`
/// carries an admin bypass — in which case the bypass is recorded in the audit
/// trail *before* this returns, and a failure to record it fails the whole
/// operation.
pub(crate) async fn enforce_roster_lock(
    season: &LeagueSeason,
    team_season_id: LeagueTeamSeasonId,
    change: RosterChange,
    override_: Option<AuditedOverride<'_>>,
) -> Result<(), DomainError> {
    let Some(refusal) = refusal_reason(season, change) else {
        // Nothing was locked, so nothing was overridden — no audit row.
        return Ok(());
    };

    let Some(AuditedOverride { audit, over }) = override_ else {
        return Err(DomainError::InvalidState(refusal));
    };

    audit
        .create(CreateEntityChange {
            entity_type: "league_team_season".to_string(),
            entity_id: team_season_id.as_uuid(),
            change_type: ChangeType::Update,
            field_name: Some("roster_lock_override".to_string()),
            old_value: Some(serde_json::json!({
                "season_id": season.id.to_string(),
                "season_status": season.status.to_string(),
                "roster_lock_status": season.roster_lock_status.to_string(),
                "refused_because": refusal,
            })),
            new_value: Some(serde_json::json!({
                "change": change.label(),
                "reason": over.reason,
            })),
            changed_by: over.overridden_by,
            request_id: over.request_id.clone(),
            ip_address: None,
            user_agent: None,
        })
        .await?;

    warn!(
        team_season_id = %team_season_id,
        season_id = %season.id,
        roster_lock_status = %season.roster_lock_status,
        season_status = %season.status,
        change = change.label(),
        overridden_by = %over.overridden_by,
        reason = %over.reason,
        "Roster lock overridden by admin"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::league_team::tests::helpers::make_season;
    use portal_core::LeagueId;
    use portal_core::types::{RosterLockStatus, SeasonStatus};

    fn season_with(lock: RosterLockStatus, status: SeasonStatus) -> LeagueSeason {
        let mut season = make_season(LeagueId::new());
        season.roster_lock_status = lock;
        season.status = status;
        season
    }

    #[test]
    fn open_lock_permits_everything() {
        let season = season_with(RosterLockStatus::Open, SeasonStatus::Registration);
        for change in [
            RosterChange::Membership(LeagueTeamRole::Captain),
            RosterChange::Membership(LeagueTeamRole::Player),
            RosterChange::Membership(LeagueTeamRole::Substitute),
            RosterChange::Role,
        ] {
            assert!(
                refusal_reason(&season, change).is_none(),
                "open lock refused {change:?}"
            );
        }
    }

    #[test]
    fn soft_lock_freezes_primaries_but_not_substitutes_or_captaincy() {
        let season = season_with(RosterLockStatus::SoftLock, SeasonStatus::Registration);

        assert!(
            refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Captain)).is_some()
        );
        assert!(
            refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Player)).is_some()
        );
        assert!(
            refusal_reason(
                &season,
                RosterChange::Membership(LeagueTeamRole::Substitute)
            )
            .is_none()
        );
        // P-16 decision: a soft lock permits captaincy changes.
        assert!(refusal_reason(&season, RosterChange::Role).is_none());
    }

    #[test]
    fn hard_lock_freezes_everything_including_captaincy() {
        let season = season_with(RosterLockStatus::HardLock, SeasonStatus::Registration);

        assert!(
            refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Captain)).is_some()
        );
        assert!(
            refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Player)).is_some()
        );
        assert!(
            refusal_reason(
                &season,
                RosterChange::Membership(LeagueTeamRole::Substitute)
            )
            .is_some()
        );
        // P-16 decision: a hard lock refuses captaincy changes too.
        assert!(refusal_reason(&season, RosterChange::Role).is_some());
    }

    /// **Spec change (P-148 — the owner's "the roster lock should really be an
    /// optional thing" ruling).** This test used to be
    /// `role_change_is_not_gated_on_season_status` and asserted that a
    /// `playoffs` season with an OPEN lock still refused a membership change —
    /// i.e. that the season phase, not the lock, was the outer gate. That is
    /// exactly the rule the ruling reverses, so the assertion is inverted, not
    /// deleted: the phase no longer has a vote, and an open lock in playoffs
    /// permits both membership and captaincy changes.
    #[test]
    fn an_open_lock_permits_changes_in_every_non_terminal_phase() {
        for status in [
            SeasonStatus::Draft,
            SeasonStatus::Registration,
            SeasonStatus::Active,
            SeasonStatus::Playoffs,
        ] {
            let season = season_with(RosterLockStatus::Open, status);
            for change in [
                RosterChange::Membership(LeagueTeamRole::Captain),
                RosterChange::Membership(LeagueTeamRole::Player),
                RosterChange::Membership(LeagueTeamRole::Substitute),
                RosterChange::Role,
                RosterChange::Founding,
            ] {
                assert!(
                    refusal_reason(&season, change).is_none(),
                    "an open lock refused {change:?} in a '{status}' season"
                );
            }
        }
    }

    /// P-148 — the lock, not the phase, is what freezes a live season. A
    /// mid-competition season is the whole reason the lock exists.
    #[test]
    fn a_lock_set_mid_competition_is_what_freezes_the_roster() {
        let hard = season_with(RosterLockStatus::HardLock, SeasonStatus::Active);
        assert!(
            refusal_reason(&hard, RosterChange::Membership(LeagueTeamRole::Player)).is_some(),
            "a hard lock must freeze an active season"
        );
        assert!(
            refusal_reason(&hard, RosterChange::Membership(LeagueTeamRole::Substitute)).is_some()
        );

        let soft = season_with(RosterLockStatus::SoftLock, SeasonStatus::Playoffs);
        assert!(
            refusal_reason(&soft, RosterChange::Membership(LeagueTeamRole::Player)).is_some(),
            "a soft lock must still freeze the primary roster in playoffs"
        );
        assert!(
            refusal_reason(&soft, RosterChange::Membership(LeagueTeamRole::Substitute)).is_none(),
            "a soft lock must still permit substitutes in playoffs"
        );
    }

    /// **Spec change (P-148).** This test used to be
    /// `status_refusal_names_the_status_not_the_lock` and pinned the message
    /// for the "season phase closed the door" refusal, a refusal that no longer
    /// exists for `playoffs`. The surviving status refusal is the terminal one,
    /// so the message assertion moves onto it — and it must still name the
    /// *status*, because "the roster is locked" would be a lie about a
    /// completed season whose lock is wide open.
    #[test]
    fn a_terminal_season_refuses_everything_and_says_the_status_did_it() {
        for status in [SeasonStatus::Completed, SeasonStatus::Cancelled] {
            let season = season_with(RosterLockStatus::Open, status);
            for change in [
                RosterChange::Membership(LeagueTeamRole::Captain),
                RosterChange::Membership(LeagueTeamRole::Player),
                RosterChange::Membership(LeagueTeamRole::Substitute),
                RosterChange::Role,
                RosterChange::Founding,
            ] {
                let msg = refusal_reason(&season, change).unwrap_or_else(|| {
                    panic!("a '{status}' season permitted {change:?} — history is not editable")
                });
                assert!(
                    msg.contains(&format!("season status '{status}'")),
                    "misleading refusal message: {msg}"
                );
            }
        }
    }

    /// P-147 — founding a roster is exempt from the lock (registration is the
    /// control for that), but not from the terminal freeze.
    #[test]
    fn founding_a_roster_answers_to_registration_not_to_the_lock() {
        for lock in [
            RosterLockStatus::Open,
            RosterLockStatus::SoftLock,
            RosterLockStatus::HardLock,
        ] {
            let season = season_with(lock, SeasonStatus::Registration);
            assert!(
                refusal_reason(&season, RosterChange::Founding).is_none(),
                "a '{lock}' season refused a founding captain; registration, not the lock, \
                 decides whether new teams are accepted"
            );
            assert!(ensure_roster_may_be_founded(&season).is_ok());
        }

        let dead = season_with(RosterLockStatus::Open, SeasonStatus::Cancelled);
        assert!(
            ensure_roster_may_be_founded(&dead).is_err(),
            "a cancelled season must not gain a founding captain"
        );
    }
}
