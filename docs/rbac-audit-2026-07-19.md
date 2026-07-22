# RBAC / Authorization Audit — 2026-07-19

Full-surface authorization review of every handler module, conducted after
discovering that no tournament handler carried a permission check. Findings
below are ordered by severity; the "Fix" column records what was implemented
the same day. Reads were audited too; intentionally-public reads are listed
at the bottom for future product review.

## Root causes (systemic)

1. **`is_admin` gates on a read permission.** `PermissionService::is_admin`
   checks `users.view_all`, which the seed data grants to `moderator` and
   which any operator might grant to support/analytics roles. Every roles/
   bans mutation used it as its only gate.
2. **Registry ≠ seeds.** The permission constants `admin.users.manage`,
   `admin.bans.manage` existed in `portal-core/src/permissions.rs` but were
   never seeded and never checked; conversely handlers referenced
   `"tournament.admin"` / `"tournament.manage"` strings that exist nowhere.
   Fails closed, but means real admins could not use those endpoints.
3. **Service-layer comments promising checks that did not exist**
   (`withdraw_from_tournament` "verifies ownership" — it did not), and
   authorization errors being swallowed (`find_user_registration(...).ok()`).

## Critical

| Finding | Location | Fix |
|---|---|---|
| Privilege escalation: role assignment gated on `users.view_all` → a moderator can self-assign global `super_admin` | `handlers/roles.rs` (all mutations) | Gate all role/permission mutations on seeded `admin.users.manage`; priority ceiling on grants (cannot grant a role of priority ≥ your own) |
| Any user can withdraw any registration, force-forfeiting its matches | `handlers/forfeit.rs` → `services/tournament/forfeit.rs` | Ownership (registrant / team member) or `admin.tournaments.manage_any` |
| Any user can delete any match evidence | `services/tournament/evidence.rs::delete_evidence` | Uploader / match participant / admin only |
| No permission checks on ANY tournament lifecycle/moderation/seeding handler | `handlers/tournaments/*` | Creator granted scoped `tournament_admin` on create; all mutations behind `require_tournament_permission` (settings/participants/brackets) with platform-admin override |

## High

| Finding | Location | Fix |
|---|---|---|
| Evidence upload/link accept non-participants (auth error swallowed by `.ok()`) | `services/tournament/evidence.rs` | Propagate; participant-or-admin |
| `raise_dispute` / `add_dispute_message` never bind the caller to the participant registration | `services/tournament/dispute.rs`, `handlers/dispute.rs` | Caller must belong to the registration (player or team member) / dispute participant check |
| `record_coin_flip` + `start_veto_session` REST endpoints unauthorized | `handlers/veto.rs` | Participant-or-admin (same gate as session creation) |
| `acknowledge_result_review` acts on any registration for any caller — can resume bracket progression | `handlers/result_reviews.rs` | Bind caller to the registration |
| Ban create/lift gated on view permission | `handlers/bans.rs` | `admin.bans.manage` (seeded) |

## Medium

| Finding | Location | Fix |
|---|---|---|
| `generate_suggestions` writes suggestion rows for any match, ignores tournament id | `handlers/availability.rs` | Participant-or-admin, tournament-scoped |
| Progression admin endpoints check nonexistent `"tournament.admin"` (fails closed — broken for real admins) | `handlers/progression.rs` | `admin.tournaments.manage_any` |
| Undefined `"tournament.manage"` in WS admin fallback | `handlers/veto_ws.rs` | Aligned to the real constant |
| `DomainError::NotAuthorized` → HTTP 401 (should be 403) | `error.rs` | Mapped to 403; business-rule denial tests updated |

## Confirmed sound (spot-list)

- All 12 `/v1/internal/*` service endpoints: `AuthenticatedService` + a
  specific `service::*` permission each.
- Veto WebSocket: token-authenticated before any action; spectators are
  read-only; turn validation server-side.
- Result submit/confirm/dispute: participant membership enforced in the
  service (the IDOR fix from the week-1 audit held).
- League/league-team surface: scoped checks (`require_league_permission`,
  `require_team_permission`, captain-or-admin helper) consistently applied.
- Self-scoped surfaces (users/me, players/me, availability, steam-tracking):
  ownership enforced.
- `local_evidence_upload` traversal guards sound; capability-URL model
  documented (dev-only endpoint, disabled in S3 mode).

## Deliberately public reads (product review backlog)

`get_result_claim`(+history), `get_match_dispute`, `get_veto_session`,
`list_evidence`/`get_evidence`, evidence `access` presigned URLs (now logged),
`list_delegations`, player MM stats / match history / availability-by-date.
Each is a read; none mutate. If any should be participant-only, gate them in
a follow-up — they are enumerated here so the exposure is a decision, not an
accident.
