# Gaming Portal Backend — Tech Lead Audit

_Date: 2026-04-19 · Scope: full workspace audit ahead of refactor cycle_

## 1. Snapshot

This is a well-disciplined Rust 2024 workspace — unusually so. The hard architectural rules from `CLAUDE.md` (layering, three-tier types, repository+service generics, RBAC via `PermissionChecker`, RFC 7807 errors) are genuinely upheld in code, not just in docs. Evidence of serious prior remediation work is everywhere: shutdown handling (`portal-app/src/main.rs:70–122`), Argon2 offloaded with configurable params (`portal-domain/src/auth.rs:38–89`), refresh-token rotation with replay detection (`portal-api/src/handlers/auth.rs:193–242`), request-ID correlation (`portal-api/src/middleware/request_id.rs` + `app.rs:80–91`), and path-traversal-hardened local uploads (`evidence.rs:1140–1207`).

**Biggest strength**: the service + repository trait abstraction is *real* — services take generic `UR: UserRepository, PR: PlayerRepository`, repo method bodies don't leak SQL, and atomicity is handled inside repository implementations via owned transactions (`portal-db/src/adapters/league_team/team.rs:359–449`).

**Biggest risk**: `AppState` in `portal-api/src/state.rs` is a 55-field god-struct where every service is pinned to a concrete `Pg*Repository` via type alias (lines 45–173). Monomorphization is already heavy and the building-block abstraction is effectively unused in production — tests still benefit from it, but it means there is a *single* concrete wiring and changing any repo forces a workspace-wide rebuild. Combined with `handlers/tournaments.rs` at 2436 lines and `services/tournament/service.rs` at 1585 lines, compile times and maintainability will degrade as the platform grows.

## 2. What's Working Well

- **Startup & shutdown** (`portal-app/src/main.rs`): JWT hard-fail with 32-byte floor (`:41–45`), graceful shutdown with SIGINT+SIGTERM (`:130–152`), `JoinHandle` tracked + bounded wait for background tasks (`:75–119`), pool drain on exit (`:122`). Textbook.
- **Auth stack**: access token deliberately has no `is_admin` claim (`portal-domain/src/jwt.rs:1–17` header comment), permissions re-checked from DB every request via `PermissionChecker`. Refresh-token replay triggers `revoke_all_for_user` (`handlers/auth.rs:207–222`). Race-safe `try_revoke` for rotation (`:234–242`).
- **`DomainError` → `ApiError` mapping** (`portal-api/src/error.rs:143–335`): one exhaustive match, typed IDs everywhere, status codes deliberately chosen per variant. `RepositoryError::Database(_)` collapses to `Self::Internal("database error")` with original logged, not returned — no SQL leakage (`portal-db/src/error.rs:215–225`).
- **RBAC**: `PermissionChecker::log_and_deny` (`extractors/permission.rs:31–45`) fails *closed* on DB errors with an `error!` log — a permissions outage doesn't silently deny everyone without evidence.
- **`DashMap` + `AtomicUsize` for websocket lobbies** (`websocket/lobby_manager.rs:16`, `lobby.rs:28–30`): zero `.lock().await` in the codebase — classic footgun is avoided.
- **Argon2id**: OWASP 2023 defaults (m=19456, t=2, p=1), env-overridable, `spawn_blocking` on hash *and* verify, dummy-verify on user-not-found for timing equivalence (`auth.rs:97–106`, `services/user.rs:223–256`).
- **Image decode bombs capped**: `image::Limits` with `max_alloc=256 MiB`, `16k×16k` dimensions (`portal-storage/src/image/processor.rs:19–98`).
- **Test isolation**: shared container + unique DB per test with atomic counter and drop cleanup (`portal-test/src/database.rs:94–178`). No shared-state flake risk.
- **Dev-token gating**: `DEV_TOKEN` branch is `#[cfg(feature = "test-utils")]` — compile-time eliminated from production (`extractors/auth.rs:11–34, 117–120`), integration tests have `required-features = ["test-utils"]` (`portal-api/Cargo.toml:99`).

## 3. Top Issues, Ranked by Impact

### Critical

**C1. Unauthenticated `get_dispute` exposes full dispute threads**
`portal-api/src/handlers/dispute.rs:230–244` takes no `AuthenticatedUser` extractor, no permission check, and calls `dispute_service.get_dispute_with_thread(dispute_id, /*include_internal=*/ false)`. Any anonymous request can iterate `DisputeId` UUIDs (they're v7, so sortable and guessable by timestamp to a first approximation) and read every user-visible dispute on the platform, including in-thread messages between participants. Why this matters: disputes contain match accusations, PII in free-text, and private communication.

**Fix sketch:**
```rust
pub async fn get_dispute(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    perm: PermissionChecker,
    Path(dispute_id): Path<DisputeId>,
    headers: HeaderMap,
) -> ApiResult<Json<DataResponse<DisputeWithThreadResponse>>> {
    let dispute = state.dispute_service.get_dispute_with_thread(dispute_id, false).await?;
    let is_admin = perm.has_admin_override(&auth, ScopeType::Tournament).await;
    let is_participant = state.dispute_service
        .is_dispute_participant(dispute_id, auth.player_id).await?;
    if !is_admin && !is_participant {
        return Err(ApiError::forbidden("Not a dispute participant"));
    }
    // ...
}
```

**C2. `demos::get_demo` returns any demo by ID with no visibility check**
`handlers/demos.rs:116–127` accepts `DemoId` from path, parameter is `_auth: AuthenticatedUser` (explicitly unused). `demo_service::get_demo` has no visibility gate. Demo files may be marked hidden/private (`SetDemoVisibilityRequest` exists in the DTO set), but the read path doesn't enforce visibility. Same IDOR class as C1, lower blast radius since demo payload is in S3 behind signed URLs, but metadata + player stats leak regardless. Same fix shape.

### Important

**I1. `AppState` is monolithic and pins every service to `Pg*Repository`**
`portal-api/src/state.rs:45–173` declares ~30 `App*Service` type aliases, each specializing a generic service to concrete Pg repos. The 55-field struct is cloned on every request (Axum clones `State` per request; cheap due to `Arc` wrappers, but still 55 `Arc::clone` calls). More importantly: **the whole point of `Service<R: Repository>` generics is swappable backends for tests and alternative stores — those generics are never exercised anywhere in production wiring**, so you're paying monomorphization cost for nothing visible.

This is the pending task **N2 (Split `AppState` into sub-states with `FromRef`)** and should be the single highest-priority refactor.

**Fix sketch (per area):**
```rust
#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: Arc<str>,
    pub user_service: AppUserService,
    pub refresh_token_repo: Arc<PgRefreshTokenRepository>,
    pub token_config: TokenConfig,
}

impl FromRef<AppState> for AuthState { /* ... */ }
```
Handler signatures then take `State(auth_state): State<AuthState>` and ignore the rest. Compile times improve and per-feature state dependencies become explicit.

**I2. 402 `sqlx::query_as` vs 0 `query_as!` — compile-time SQL checks abandoned**
`.sqlx/` is committed, so the offline cache is there, but not a single query uses the checked macro form (`portal-db/src/adapters/**`). CLAUDE.md claim "SQLx 0.8 (compile-time verified)" is aspirational. The runtime form means: a renamed column, a type mismatch, or a NULL-ability error ships unless the exact code path is exercised by an integration test. Given 362+ integration tests this is mostly caught, but it defeats one of SQLx's strongest selling points. The cost is mechanical refactor; the payoff is catching schema drift at `cargo check` time.

Migration strategy: do one adapter at a time, starting with high-churn ones (`tournament/*`, `league_team/*`). `cargo sqlx prepare --workspace` after each adapter; commit `.sqlx/` incrementally.

**I3. N+1 writes in `bulk_create` methods**
- `adapters/tournament/standings.rs:170–182`: `bulk_create` loops `self.create(cmd).await?` — N INSERTs instead of one `INSERT ... SELECT FROM UNNEST(...)` or multi-row VALUES. Called during bracket seeding / round generation when standings can be dozens-to-hundreds of rows.
- `adapters/tournament/match_.rs` `bulk_create`: same pattern, same fix.

Neither is wrapped in a transaction, so a partial failure also leaves orphan rows. Wrap the loop in `pool.begin()` + `commit()` at minimum; better, batch via:
```rust
sqlx::query(
    "INSERT INTO tournament_standings (id, bracket_id, participant_id, wins, losses, ...)
     SELECT * FROM UNNEST($1::uuid[], $2::uuid[], $3::uuid[], $4::int[], $5::int[], ...)"
).bind(&ids).bind(&bracket_ids)... .execute(&self.pool).await?;
```

**I4. Residual `is_owner(...)` checks in `league_teams/team.rs`**
Task I1 was marked completed but three handlers still do the ad-hoc pattern:
- `handlers/league_teams/team.rs:131–135` (`register_team_for_season`)
- `handlers/league_teams/team.rs:250–254` (`update_team`)
- `handlers/league_teams/team.rs:293–297` (`disband_team`)

All three: `if !team.is_owner(auth.player_id) && !perm.has_admin_override(&auth, ScopeType::Team).await { return Err(forbidden) }`. Functionally correct, but (a) it requires an extra `get_team()` round-trip before the service call and (b) it diverges from `require_team_permission` everywhere else. Reading the current team record just to check ownership means the check is not atomic with the write — if ownership changes between the check and the action, the wrong policy applies.

**Fix**: introduce a `team.settings.manage` scoped permission seeded for the owner role, then:
```rust
perm.require_team_permission(&auth, team_id.into(), permissions::team::SETTINGS_MANAGE).await?;
state.league_team_service.disband_team(team_id).await?;
```

**I5. `MatchCompletionSaga` non-atomic `register_for_season`**
`services/league_team/team.rs:195–278` makes two sequential calls (`team_season_repo.create` then `member_repo.add_member`), each grabbing its own connection. Failure between them orphans a `league_team_seasons` row with no captain. `transaction.rs` has `with_transaction` / `Transactional` helpers (`portal-db/src/transaction.rs`) that are widely unused. Either drop both writes into a repo method that owns the transaction (like `create_team_with_season_and_captain` already does) or plumb `&mut Transaction` through the affected repo trait methods.

**I6. JWT algorithm not pinned explicitly**
`jwt.rs:113` uses `Validation::default()`. In `jsonwebtoken` 9.x this defaults to `HS256` only, so `alg: none` is rejected today — but it's a library-version-coupled invariant and worth being defense-in-depth explicit:
```rust
let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
validation.leeway = 60;
```

**I7. `CS2_DEMO_SERVICE_URL` SSRF surface**
`state.rs:382` reads the env var raw, `portal-plugins/src/games/cs2/demo_client.rs:44–50` accepts any URL string into the `reqwest::Client`. If an attacker controls this env var (host-compromise-adjacent, but also misconfiguration) they can pivot internal GETs via `POST /evidence/validate-demo`-triggered fetches. Add schema + host allow-listing at startup:
```rust
let url = url::Url::parse(&raw)?;
if url.scheme() != "https" { bail!("demo service URL must be https"); }
if matches!(url.host_str(), Some(h) if is_private(h)) { bail!("private host"); }
```
Also cap response body via `reqwest::Response::bytes_stream()` + bounded accumulation — `Cs2DemoStats` JSON is small, a malicious server could return a gigabyte.

### Nice-to-have

**N1. Split `handlers/tournaments.rs` (2436 lines) and `services/tournament/service.rs` (1585 lines)**
Natural cut lines per the Explore agent's reading: lifecycle / registration / seeding / scheduling / map-pool / standings. Each 300–500 lines, one logical surface per file. Same principle for `handlers/evidence.rs` (1215 lines) and `services/tournament/match_completion.rs` (1462 lines).

**N2. Input validation inconsistency**
Some handlers use `ValidatedJson<T>` (enforces `Validate` in the extractor), some use `Json<T>` and call `req.validate()?` manually (`handlers/bans.rs:149`, `handlers/demos.rs:178, 238, 290, 337, 389`, `handlers/roles.rs:149, 197, 313, 524`), and a few take raw `Json` with no validate call at all (`handlers/steam_tracking.rs:108, 213`, `handlers/leagues.rs:410`, `handlers/players.rs:249`). Standardize on `ValidatedJson<T>` everywhere — the handlers that currently have no validate call are the concerning ones.

**N3. Pool config not env-driven**
`portal-db/src/pool.rs` has `PoolConfig::default()` (10 connections) and `PoolConfig::production()` (20), but `main.rs:30` always uses `default`. Prod deploy can't tune `MAX_POOL_CONNECTIONS` without a code change. Quick fix — read from env in `PoolConfig::default()`.

**N4. No `#[instrument]` on handlers or services**
Zero uses workspace-wide. Request-ID propagation is solid (middleware + `make_span_with`), but within a handler every nested service call is an unnamed future. Adding `#[instrument(skip(state, req), fields(user_id = %auth.user_id))]` on major handlers makes trace waterfalls readable.

**N5. Feature flag smell in `portal-storage`**
`default = ["local"]` + optional `s3`, but `portal-api/Cargo.toml:19` pulls `portal-storage = { path = "../portal-storage", features = ["s3"] }` unconditionally — you always compile both. Pick a lane: either make `s3` a runtime-only choice (both always compiled in, feature flag just exists for CLI-only builds) and remove the storage feature split entirely, or make `portal-api` conditional and actually ship a `local-only` build.

**N6. Missing OpenAPI annotations on 12 handlers**
All in `handlers/internal.rs` (11 handlers) and `evidence.rs:1140` (`local_evidence_upload`). The internal ones are arguably intentional — they're service-to-service with `X-API-Key`. Document the exclusion as a comment in `openapi.rs` and/or produce a second internal OpenAPI doc.

**N7. No metrics**
`tracing` only, no `metrics` / Prometheus. Fine at current scale; worth a line in the roadmap.

**N8. `GameBuilder` doesn't expose `build()` in-memory variant**
`portal-test/src/builders/game.rs` — minor inconsistency with the other 17 builders.

## 4. Architectural Debt

### The `AppState` problem

```rust
// portal-api/src/state.rs (55 fields, ~30 App*Service aliases)
pub struct AppState {
    pub db_pool: DbPool,
    pub jwt_secret: Arc<str>,
    pub user_service: AppUserService,          // = UserService<PgUserRepository, PgPlayerRepository>
    pub player_service: AppPlayerService,
    /* ... 27 more services ... */
    pub permission_repo: PermissionRepository,
    pub storage: Arc<dyn StorageBackend>,
    pub plugin_manager: Arc<PluginManager>,
    pub match_completion_saga: AppMatchCompletionSaga,
    /* ... */
}
```

Every handler that needs just `user_service` drags the whole thing in. Services are advertised as generic but always specialized to Pg. Two independent problems, one refactor:

```rust
// After: domain-scoped sub-states, each FromRef<AppState>
#[derive(Clone)] pub struct AuthState { /* jwt + user + refresh */ }
#[derive(Clone)] pub struct TournamentState { /* tournament + registration + seeding + ... */ }
#[derive(Clone)] pub struct LeagueTeamState { /* league_team + invitations + seasons */ }
#[derive(Clone)] pub struct EvidenceState { /* evidence + storage + plugins */ }
/* ... */

impl FromRef<AppState> for AuthState {
    fn from_ref(s: &AppState) -> Self { /* pluck fields */ }
}

// Handler:
pub async fn login(State(s): State<AuthState>, /* ... */) { ... }
```

### Repository-transaction plumbing is half-built

`portal-db/src/transaction.rs` defines `DbTransaction`, `begin_transaction`, `with_transaction`, `Transactional` — a thoughtful helper. Used in exactly two places:
- `adapters/league_team/team.rs:359–449` — works.
- `adapters/tournament/match_completion_tx.rs:65–99` — works, but only called from tests.

Every other multi-step write pattern in services calls sequential single-connection repo methods. Either commit to the pattern (each multi-step saga goes through a repo method that owns the transaction) or expose `&mut Transaction<'_, Postgres>` on trait methods and thread it through. The current middle ground leaks atomicity guarantees.

### `handlers/tournaments.rs` is a god module

```
handlers/tournaments/
├── mod.rs            (re-export + `tournaments::*` compat)
├── lifecycle.rs      (create → finalize)
├── registration.rs   (register / check-in / withdraw / dq)
├── seeding.rs        (auto / manual / clear)
├── matches.rs        (status / transitions / forfeits)
├── scheduling.rs     (propose / accept / counter / admin)
├── map_pool.rs       (GET / SET / DELETE)
└── standings.rs      (bracket standings, swiss round)
```

Same treatment for `services/tournament/service.rs`.

## 5. Security Findings

| # | Severity | Finding | File:Line |
|---|----------|---------|-----------|
| S1 | **Critical** | Unauthenticated dispute read exposes threads | `handlers/dispute.rs:230–244` |
| S2 | **Critical** | `demos::get_demo` has no visibility check | `handlers/demos.rs:116–127` |
| S3 | Medium | `CS2_DEMO_SERVICE_URL` SSRF (scheme + host not validated, no response size cap) | `state.rs:382`, `portal-plugins/src/games/cs2/demo_client.rs:44–50, 74–98` |
| S4 | Medium | JWT alg not explicitly pinned (OK in `jsonwebtoken` 9.x default, but implicit) | `portal-domain/src/jwt.rs:113` |
| S5 | Low | No max-body cap on reqwest response in demo client | `demo_client.rs:80–97` |
| S6 | Low | `RegisterRequest`/`LoginRequest` derive `Debug` — no redaction | `dto/requests/auth.rs` |
| S7 | Informational | CORS defaults to wildcard origin when env unset (warn is logged). `allow_credentials` is never set, so not a credentialed-request footgun | `app.rs:66–71` |
| S8 | Informational | Rate limit only on `/auth/*` — enumeration of `/players/by-username/*` not capped | `routes/auth.rs` vs `routes/players.rs` |

**Not findings** (verified):
- Path traversal in local upload — properly defended (`evidence.rs:1163–1200`).
- API key storage — SHA-256 hash at rest, DB-side `WHERE key_hash = $1` lookup.
- Refresh token storage — hash at rest, `try_revoke` for one-use semantics, replay detection on rotation.
- Argon2 params — OWASP compliant, env-overridable, `spawn_blocking`.
- SQL injection — all dynamic SQL is over compile-time constant column lists or parameter placeholders.
- Image decode bombs — `Limits` enforced.

## 6. Suggested Refactor Roadmap

**Week 1 — ship-blocking fixes**
1. Fix **C1** (`get_dispute` auth + participant check) and **C2** (`demos::get_demo` visibility). These are IDOR / information disclosure; they should not sit in a backlog.
2. **I7**: validate `CS2_DEMO_SERVICE_URL` at startup, add response-body size cap in `demo_client.rs`.
3. **I6**: explicit `Validation::new(Algorithm::HS256)` in `jwt.rs`.
4. **N2** (validation): migrate the handlers that currently accept raw `Json<T>` without validation (`handlers/steam_tracking.rs`, `handlers/leagues.rs:410`, `handlers/players.rs:249`) to `ValidatedJson<T>`.

**Weeks 2–3 — architectural debt paydown**
5. **I1 / N2 pending task**: split `AppState` into domain-scoped sub-states with `FromRef`. Do one domain at a time (`AuthState` → `TournamentState` → `LeagueTeamState` → ...). Land incrementally.
6. **I4**: collapse the residual `is_owner(...)` pattern in `handlers/league_teams/team.rs` to `require_team_permission` after seeding the corresponding scoped permission.
7. **I3**: convert `tournament/standings.rs` and `tournament/match_.rs` `bulk_create` to a single batched `INSERT` (or at minimum wrap in a transaction).
8. **N1**: split `handlers/tournaments.rs` and `services/tournament/service.rs` into modules.

**Next quarter — platform hardening**
9. **I2**: incremental migration to `sqlx::query!`/`query_as!` macros. One adapter per PR, regen `.sqlx/` each time.
10. **I5 / transaction plumbing**: decide on a transaction-ownership pattern (repo-owned vs trait-plumbed) and apply it uniformly. Audit every multi-step service write.
11. **N4**: add `#[instrument]` on all handlers + top-level service methods, with `skip(state, req)` and `fields(user_id, entity_id)`.
12. **N3**: pool config env-driven.
13. **N5**: clean up `portal-storage` feature flags (pick runtime or compile-time, not both).
14. **N7**: evaluate Prometheus metrics (latency histogram, error-rate counter per route) once `#[instrument]` is in place.

Overall: this is a healthy Rust codebase. The critical items are two missing authz checks; everything else is ordinary debt of a fast-moving project. The pending N2 refactor (`AppState` split) is the highest-leverage pre-scale-out change.
