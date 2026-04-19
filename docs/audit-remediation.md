# Audit Remediation Tracker

Living document for the 2026-04 architecture audit. Each row cites the original evidence and tracks remediation status.

**Status legend**: ☐ todo · ◐ in progress · ☑ done

## Critical — Week 1

| # | Status | Item | Evidence |
|---|--------|------|----------|
| C1 | ☐ | Gate `local_evidence_upload`, fix path traversal | `portal-api/src/app.rs:22-25`, `handlers/evidence.rs:1128-1147` |
| C2 | ☑ | Hard-fail on missing `JWT_SECRET` | `portal-app/src/main.rs:38-39` |
| C3 | ☐ | Remove `DEV_AUTH_ENABLED` runtime bypass | `extractors/auth.rs:61-66,117-119`, `extractors/permission.rs:37-40,70,97,134,152` |
| C4 | ☐ | Argon2 on `spawn_blocking`; dummy-verify on user-not-found | `portal-domain/src/auth.rs:11-32`, `services/user.rs:139,186-209` |
| C4b | ☐ | Rate-limit `/auth/*` routes | `portal-api/src/routes/auth.rs:9-14` |
| C5 | ☐ | Exhaustive `RepositoryError→DomainError`; mask raw SQL text | `portal-db/src/error.rs:108-136`, `portal-api/src/error.rs:323` |
| C6 | ☑ | Respond with `application/problem+json` | `portal-api/src/error.rs:130-134` |

## Important — Weeks 2–3

| # | Status | Item | Evidence |
|---|--------|------|----------|
| I1 | ☐ | Replace hand-rolled `is_owner` checks with `require_team_permission` (restores admin override) | `handlers/league_teams/team.rs:150,284,333` |
| I2 | ☐ | Thread `&mut Transaction` through multi-step writes | `services/league_team/team.rs:105-203` |
| I3 | ☐ | Reconcile SQLx claim: either migrate to `query!`/`query_as!` or fix CLAUDE.md | 574 runtime `sqlx::query_*(` calls; `.sqlx/` nearly empty |
| I4 | ☐ | Replace `DomainError::*NotFound(String)` with typed IDs | `portal-core/src/errors.rs` |
| I5 | ☐ | Stop refetching `Player` every auth'd request; use `auth.player_id` | `handlers/league_teams/team.rs:83-86,143-146,277-280,326-329` |
| I6 | ☐ | Graceful shutdown; manage background task `JoinHandle` | `portal-app/src/main.rs:78`, `websocket/timeout_task.rs:60` |
| I7 | ☐ | Add `DefaultBodyLimit` (global + per-route) | `portal-api/src/app.rs` |
| I8 | ☐ | Log permission-check DB errors before fail-closed | `extractors/permission.rs:45,81,108,139,165` |
| I9 | ☐ | Request-ID propagation + trace-span correlation | `middleware/request_id.rs`, every handler's `get_request_id` |
| I10 | ☐ | CORS allow-list from env | `portal-api/src/app.rs:16-20` |
| I11 | ☐ | Set `image` crate decode limits | `portal-storage/src/image/processor.rs:72-76` |
| I12 | ☐ | Refresh-token reuse detection (revoke all on replay) | `handlers/auth.rs:191-214` |

## Nice-to-have — Next quarter

| # | Status | Item | Evidence |
|---|--------|------|----------|
| N1 | ☐ | Use newtype-typed `Path` extractors (delete ~200 manual parses) | across `portal-api/src/handlers/` |
| N2 | ☐ | Split `AppState` into sub-states with `FromRef` | `portal-api/src/state.rs:183` |
| N3 | ☐ | Close OpenAPI coverage gap (249 handlers vs 235 `utoipa::path`) | `portal-api/src/handlers/` |
| N4 | ☐ | Re-check `is_admin` from DB on sensitive admin ops | `portal-domain/src/jwt.rs:26` |
| N5 | ☐ | Cache plugin resolution per match | `handlers/evidence.rs:738-780` |
| N6 | ☐ | Clarify User/Player ID relationship (single `AccountId` or distinct IDs) | `services/user.rs:142-143` |
| N7 | ☐ | Argon2 params configurable via env | `portal-domain/src/auth.rs` |
| N8 | ☐ | Property tests for CS2 demo parsing | `portal-plugins/` |

## Cross-cutting follow-ups

- Update `CLAUDE.md` once I3 is resolved — currently claims compile-time SQLx verification that doesn't hold.
- Document `portal-scanner` crate purpose in `CLAUDE.md` (present in workspace, absent from docs).

## How to use this tracker

- Tick the status box (`☐` → `◐` → `☑`) in the commit that changes it.
- When you land a fix, link the commit SHA in a new `Notes` column or leave a short note under the table.
- If scope expands (e.g., I2 uncovers another service that needs the same treatment), add a sub-row rather than silently broadening the original.
