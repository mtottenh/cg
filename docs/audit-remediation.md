# Audit Remediation Tracker

Living document for the 2026-04 architecture audit. Each row cites the original evidence and tracks remediation status.

**Status legend**: ☐ todo · ◐ in progress · ☑ done · ⏸ deferred (decision needed)

## Sprint progress

- **Critical**: 7 of 7 done. C4b landed with `tower_governor`; deployments behind a proxy need to configure forwarded-IP trust explicitly.

**Follow-up sprint closed C4b, I2, I4.** All Critical and Important items now resolved. Only the 8 Nice-to-have items remain open.
- **Important**: 12 of 12 done. I2 closed specifically for the cited bug (`LeagueTeamService::create_team` now atomic); a broader trait-wide transaction refactor remains a future opportunity if other multi-step writes surface.
- **Nice-to-have**: 0 of 8 done. Left for targeted follow-ups.

## Critical — Week 1

| # | Status | Item | Evidence |
|---|--------|------|----------|
| C1 | ☑ | Gate `local_evidence_upload`, fix path traversal | `portal-api/src/app.rs:22-25`, `handlers/evidence.rs:1128-1147` |
| C2 | ☑ | Hard-fail on missing `JWT_SECRET` | `portal-app/src/main.rs:38-39` |
| C3 | ☑ | Remove `DEV_AUTH_ENABLED` runtime bypass | `extractors/auth.rs:61-66,117-119`, `extractors/permission.rs:37-40,70,97,134,152` |
| C4 | ☑ | Argon2 on `spawn_blocking`; dummy-verify on user-not-found | `portal-domain/src/auth.rs:11-32`, `services/user.rs:139,186-209` |
| C4b | ☑ | Rate-limit `/auth/*` routes (`tower_governor` 0.7, per-IP via `SmartIpKeyExtractor`, env-tunable). Trust chain for forwarded IPs is deployment-specific — see docstring in `routes/auth.rs`. | `portal-api/src/routes/auth.rs:9-14` |
| C5 | ☑ | Exhaustive `RepositoryError→DomainError`; mask raw SQL text | `portal-db/src/error.rs:108-136`, `portal-api/src/error.rs:323` |
| C6 | ☑ | Respond with `application/problem+json` | `portal-api/src/error.rs:130-134` |

## Important — Weeks 2–3

| # | Status | Item | Evidence |
|---|--------|------|----------|
| I1 | ☑ | Replace hand-rolled `is_owner`/`is_captain` checks with override-aware equivalents (restores admin override) | `handlers/league_teams/team.rs:150,284,333`, `team_season.rs`, `invitation.rs` |
| I2 | ☑ | Multi-step write atomicity for `LeagueTeamService::create_team` via new `LeagueTeamRepository::create_team_with_season_and_captain` method that runs all three inserts in a single DB transaction. Full trait-wide `&mut Transaction` plumbing still a future refactor, but the specific orphan-data bug the audit flagged is closed. | `services/league_team/team.rs:105-203` |
| I3 | ☑ | Reconcile SQLx claim: CLAUDE.md updated to describe the runtime-query reality. Migration to the macro form deferred until schema stabilises. | 574 runtime `sqlx::query_*(` calls; `.sqlx/` nearly empty |
| I4 | ☑ | Replace `DomainError::*NotFound(String)` with typed IDs. 23 variants migrated (`TeamNotFound` stays `String` until `TeamId` exists). New `LookupFailed { resource, query }` variant covers slug/composite-key lookups. | `portal-core/src/errors.rs` |
| I5 | ☑ | Stop refetching `Player` every auth'd request; use `auth.player_id` | `handlers/league_teams/team.rs:83-86,143-146,277-280,326-329` |
| I6 | ☑ | Graceful shutdown; manage background task `JoinHandle` | `portal-app/src/main.rs:78`, `websocket/timeout_task.rs:60` |
| I7 | ☑ | Add `DefaultBodyLimit` (global + per-route) | `portal-api/src/app.rs` |
| I8 | ☑ | Log permission-check DB errors before fail-closed | `extractors/permission.rs:45,81,108,139,165` |
| I9 | ☑ | Request-ID propagation + trace-span correlation | `middleware/request_id.rs`, every handler's `get_request_id` |
| I10 | ☑ | CORS allow-list from env | `portal-api/src/app.rs:16-20` |
| I11 | ☑ | Set `image` crate decode limits | `portal-storage/src/image/processor.rs:72-76` |
| I12 | ☑ | Refresh-token reuse detection (revoke all on replay) | `handlers/auth.rs:191-214` |

## Nice-to-have — Next quarter

| # | Status | Item | Evidence |
|---|--------|------|----------|
| N1 | ☑ | 118 manual parses migrated to newtype-typed `Path<T>` extractors (96 single-arg + 14 one-line-parse + 8 tuple). Remaining 28 `Path<String>` sites are legitimately string (slug lookups like `games::get_game`, dev-only `local_evidence_upload`, or roles/bans/leagues that hit repos typed on raw `Uuid` — migrating those needs a repo signature change). | across `portal-api/src/handlers/` |
| N2 | ☐ | Split `AppState` into sub-states with `FromRef` | `portal-api/src/state.rs:183` |
| N3 | ☑ | OpenAPI coverage audited: 14 unannotated handlers are all legitimately excluded (11 internal service endpoints, 1 WebSocket upgrade, 1 local-dev upload). Module-level docs now document the exclusion rationale. No public handlers were missing. | `portal-api/src/handlers/internal.rs`, `veto_ws.rs`, `evidence.rs` |
| N4 | ☑ | Removed `is_admin` claim from JWT entirely. Every authz check already flows through `PermissionChecker` which hits the DB, so the claim was dead weight + a 15-min staleness window. DB is now the single source of truth. | `portal-domain/src/jwt.rs:26` |
| N5 | ☑ | Process-wide `tournament_id → plugin` cache (DashMap via OnceLock); removes 2 of 3 DB roundtrips per `/matches/*/evidence/*` call. | `handlers/evidence.rs:738-780` |
| N6 | ☐ | Clarify User/Player ID relationship (single `AccountId` or distinct IDs) | `services/user.rs:142-143` |
| N7 | ☑ | Argon2 params configurable via env (`PORTAL_ARGON2_{M,T,P}_COST`); OWASP 2023 defaults | `portal-domain/src/auth.rs` |
| N8 | ☐ | Property tests for CS2 demo parsing | `portal-plugins/` |

## Cross-cutting follow-ups

- Document `portal-scanner` crate purpose in `CLAUDE.md` (present in workspace, absent from docs).

## How to use this tracker

- Tick the status box (`☐` → `◐` → `☑`) in the commit that changes it.
- When you land a fix, link the commit SHA in a new `Notes` column or leave a short note under the table.
- If scope expands (e.g., I2 uncovers another service that needs the same treatment), add a sub-row rather than silently broadening the original.
