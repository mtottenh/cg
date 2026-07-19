# Launch-Readiness Audit — 2026-07-19

Scope: test-coverage inventory (backend + frontend e2e) and the critical path to going
live today with team creation, tournament setup, bracket generation, match setup,
evidence upload, and demo parsing.

## Ground truth established today

- **Plain `cargo test` runs ZERO integration tests.** The integration target declares
  `required-features = ["test-utils"]`, so the documented commands silently skipped
  all 455 integration tests. Correct invocation:
  `cargo test -p portal-api --features test-utils`. (CLAUDE.md updated.)
- **No CI runs tests at all.** `.github/workflows/` contains only `build-deb.yml`.
- When actually run, the suite was red: 1 stale unit test in portal-core + 71
  integration failures. After today's fixes (below): green except environmental
  MinIO flake (see suite results at the bottom).

## Fixes applied today (test harness + real bugs)

1. **portal-core stale state-machine test** (`types/tournament.rs`): asserted the
   removed `InProgress → Completed` edge; rewritten to cover the current
   `InProgress → AwaitingResult → Completed/Disputed` flow.
2. **Test harness missing `ConnectInfo`** (`tests/integration/common/mod.rs`):
   `oneshot()` requests carry no peer address, so `tower_governor` on `/auth/*`
   returned 500 "Unable To Extract Key!" — 30+ tests failed. The harness now
   injects a `ConnectInfo` extension, mirroring the (uncommitted) production fix
   in `portal-app/src/main.rs`.
3. **`transition_match_to_ready` helpers assumed matches start `pending`** —
   bracket generation now emits `ready`, and same-state transitions are rejected.
   The shared helper is now state-aware; duplicate helpers in `results.rs` and
   `match_completion_saga.rs` were removed in favor of the shared one (30 tests).
4. **Real bug: slug-addressed game 404s surfaced as 500s.** `RepositoryError::NotFound`
   for entity "Game" was parsed as a UUID; slugs (user input) fell into the
   malformed-adapter-id branch → `Internal`. Added `DomainError::GameNotFoundBySlug`
   and mapped it to 404 (portal-core, portal-db, portal-api).
5. **Stale tests updated to current, better semantics:**
   - dispute lookups now require auth before revealing anything (IDOR posture) —
     tests authenticate;
   - `/demos/{id}/players|links` now 404 for a missing demo instead of 200-empty;
   - enriched player ratings land in `player_rating_history` (profiles derive
     rating at query time) — test asserts the history row;
   - veto side selection is a three-mode system now; WS tests set
     `picker_choice` and assert picker-selects (builder gained a
     `side_selection_mode` setter).
6. **Secrets/hygiene:** `.gitignore` now covers `*.credentials.dev`, `*.dem*`,
   local `uploads/`, `test-results/` (316 MB of demos + two plaintext API keys
   were one `git add .` from being committed).

## Backend integration coverage vs launch features

455 tests across 29 files. Per-feature endpoint coverage (from route ↔ test cross-reference):

| Feature | Coverage | Critical untested endpoints |
|---|---|---|
| Team creation (league_teams) | Good | `registrations/team` register-for-season, logo/banner upload, direct member add/remove |
| Tournament setup | Mixed | **PATCH update**, **close/reopen-registration**, **cancel/complete/finalize**, reject/disqualify/admin-check-in, process-no-shows, map-pool GET/PUT/DELETE |
| Bracket generation | **Well covered** (11 tests incl. Swiss next-round) | — |
| Match setup (veto/scheduling/check-in) | Good | schedule **accept** + **counter** (negotiation never completes in any test), single-match GET |
| Evidence upload | Good (S3 lifecycle vs real MinIO) | **validate-demo**, link-demo, demo-stats — the three CS2-demo-evidence endpoints |
| Demo pipeline | Mixed | **all four `/internal/demos/*` scanner endpoints** (scanner_e2e goes through admin routes instead), demo download |

**Top gap: `POST /tournaments/{id}/registrations/team` — tournament team registration
has zero tests** (only player registration is exercised). For a team-based launch this
is the highest-risk untested happy path.

## Frontend e2e (../web) — why "full e2e is difficult"

~218 Playwright tests in 16 specs, but:

1. **Playwright only auto-starts Vite.** API + Postgres + admin bootstrap user +
   seeded games are out-of-band prerequisites; `global-setup.ts` hard-fails without them.
2. **Seeding is silently partial.** The current `e2e/.seeded-state.json` has
   `tournamentId: null, matchIds: []` — and ~60 tests guard with
   `if (!state.tournamentId) test.skip()`, so match/dispute/results coverage
   silently evaporates instead of failing loudly. **This is the single mechanism
   converting "difficulty" into invisible zero-coverage.**
3. **Match check-in is architecturally untestable**: no admin endpoint forces the
   time-window-driven `ready → checking_in` transition (`match-checkin.spec.ts` is
   entirely `test.fixme`). Backend has `admin_match_transition` — the frontend
   suite doesn't use it / may need it exposed & wired.
4. **Zero e2e specs exist for:** veto/pick-ban flow, real evidence upload
   (only a "Phase 1 UI shell" spec), demo upload+parsing.
5. Dispute scenarios need 2nd/3rd seeded matches that setup never creates.
6. Unit/component coverage is thin (7 vitest files, ~26 tests) — behavioral burden
   is all on the (partially skipped) e2e suite.

## Demo/evidence pipeline readiness

- **Parsing is external by design**: the portal fetches pre-parsed
  `*.dem.stats.json` from `CS2_DEMO_SERVICE_URL` (default `demos.cs210mans.uk`).
  **If that service is down, all demo stats and CS2 evidence validation degrade.**
  No fallback exists. Verify liveness from prod before launch.
- Portal-side ingest/persist (scanner daemon → internal endpoints →
  `demos`/`demo_players`) is implemented and covered by `scanner_e2e.rs` against a
  mock stats server + MinIO — but through the **admin** routes, not `/internal/*`.
- **The scanner is not in docker-compose and not in the Dockerfile.** Container-only
  deploys will silently have no demo ingestion. It ships via cargo-deb/systemd —
  confirm which deploy path tonight uses and that prod env has
  `PORTAL_API_KEY`, `SCANNER_GAME_ID`, S3 credentials.
- `Cs2PluginWithEvidence.discover_evidence` returns empty (stub) — discovery relies
  entirely on the catalog path; acceptable only if the scanner runs.
- MSRV mismatch: workspace says `rust-version = 1.85`, deps now need ≥1.88
  (Dockerfile already bumped to rust:1.88).

## Critical path for today (ordered)

**Must do before going live**
1. Commit today's fixes + the uncommitted prod fixes (CORS allow-headers,
   `into_make_service_with_connect_info`, Dockerfile) — the ConnectInfo fix alone
   prevents every rate-limited route 500ing in prod.
2. Decide the scanner deploy (compose service or systemd .deb) and set prod
   credentials; verify the external demo service responds from the prod host.
3. Backend: add the missing happy-path tests for tonight's flows, in this order:
   a. tournament **team registration**;
   b. tournament **complete/finalize/cancel** (lifecycle end);
   c. schedule **accept** (negotiated schedule reaching agreement);
   d. evidence **validate-demo** against a mock stats server (reuse the
      `scanner_e2e.rs` mock);
   e. `/internal/demos/*` (port scanner_e2e to the real service surface).
4. Frontend: make `global-setup.ts` **fail loudly** when tournament/match seeding
   fails (it currently swallows it), fix the seeding, and rerun — that alone
   revives ~60 skipped tests. Fix the one real failure (edit-tournament test picked
   an already-started tournament; make it create a fresh draft).
5. Wire a minimal CI workflow: `cargo fmt --check`, `clippy -D warnings`,
   `cargo test -p portal-api --features test-utils` (with DB service), and the
   Playwright suite against compose.

**Should do (this week, not tonight)**
- E2e specs for veto flow, real evidence upload, demo catalog surfacing.
- Admin "force check-in window" usage in e2e to unblock `match-checkin.spec.ts`.
- Registration moderation tests (reject/disqualify/no-show), map-pool trio,
  team logo/banner upload tests.
- Bump `rust-version` to 1.88; add scanner to docker-compose for dev parity.
- Scanner unit tests (currently zero; only covered via scanner_e2e).
