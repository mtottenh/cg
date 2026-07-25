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
}

impl RosterChange {
    /// Short machine-readable label, recorded in the audit trail.
    const fn label(self) -> &'static str {
        match self {
            Self::Membership(LeagueTeamRole::Captain) => "membership:captain",
            Self::Membership(LeagueTeamRole::Player) => "membership:player",
            Self::Membership(LeagueTeamRole::Substitute) => "membership:substitute",
            Self::Role => "role_change",
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
/// * **Membership of a primary role** (captain / player) needs
///   `allows_primary_roster_changes()`.
/// * **Membership of a substitute** needs `allows_substitute_changes()` — which
///   `soft_lock` grants and `hard_lock` does not.
/// * **A role change** (captain <-> player) is gated on the lock **only**, and
///   only by `hard_lock`. See the note on [`RosterChange::Role`] below.
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
    match change {
        RosterChange::Membership(role) => {
            let allowed = if role.is_primary() {
                season.allows_primary_roster_changes()
            } else {
                season.allows_substitute_changes()
            };
            if allowed {
                return None;
            }
            // Both halves of `allows_*` can refuse; say which one did, because
            // "the roster is locked" is actively misleading when the lock is
            // open and it is the season status that closed the door.
            Some(if season.status.allows_roster_changes() {
                let who = if role.is_primary() {
                    "primary member"
                } else {
                    "substitute"
                };
                format!("roster is locked for {who} changes")
            } else {
                format!(
                    "roster is locked: season status '{}' does not allow roster changes",
                    season.status
                )
            })
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

/// Enforce the roster lock for `change` on `season`.
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

    #[test]
    fn role_change_is_not_gated_on_season_status() {
        // Season is past the point where members may be added or removed, but
        // the lock is open — naming a captain must still be possible.
        let season = season_with(RosterLockStatus::Open, SeasonStatus::Playoffs);

        assert!(
            refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Player)).is_some()
        );
        assert!(refusal_reason(&season, RosterChange::Role).is_none());
    }

    #[test]
    fn status_refusal_names_the_status_not_the_lock() {
        let season = season_with(RosterLockStatus::Open, SeasonStatus::Playoffs);
        let msg = refusal_reason(&season, RosterChange::Membership(LeagueTeamRole::Player))
            .expect("playoffs must refuse membership changes");
        assert!(
            msg.contains("season status 'playoffs'"),
            "misleading refusal message: {msg}"
        );
    }
}
