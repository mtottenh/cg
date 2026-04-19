# CLAUDE.md

Multi-Game Competitive Gaming Portal — Rust / Axum / SQLx / PostgreSQL backend with a plugin-based architecture for multiple game titles.

**Status**: Core platform + tournament system production-ready. Matchmaking, OAuth, lobbies, game-server integration, substitutes are planned (no code yet — see `docs/gaming-portal-hld.md`).

**Scale**: 194 handlers · 228 OpenAPI paths · 44 migrations · 27 services · 362+ integration tests.

## Tech Stack

Rust 1.85+ (Edition 2024) · Axum 0.8 · SQLx 0.8 · PostgreSQL 16+ · Tokio · utoipa 5 / utoipa-swagger-ui 9 · JWT + Argon2id · tracing · testcontainers + fake.

## Workspace

```
portal-core      Shared types (43 ID newtypes), errors, 28 permission constants. No async deps.
portal-domain    24 entities, 27 services (generic over repo traits), 44 repository traits.
portal-db        SQLx entities, Pg*Repository adapters, 44 migrations.
portal-api       194 handlers, 15 route modules, 150+ DTOs, OpenAPI registration.
portal-storage   File storage (LocalStorage default, S3Storage feature flag).
portal-test      18 builders, TestApp helper, testcontainers harness.
portal-cli       Admin CLI (users, roles, players, games, bans, audit, leagues, db, bootstrap).
portal-app       Server entry point.
portal-plugins   Per-game plugins (CS2: demo parsing, evidence validation).
portal-cache     Redis wrapper (stub).
```

**Layering** (do not violate): `app → api → domain → core` and `api → db → domain`. `core` has no async deps. `domain` defines repository traits; `db` implements them as `Pg*Repository`. `api` is HTTP only and converts DTOs ↔ domain types.

## Non-Negotiable Patterns

### OpenAPI on every endpoint
Every handler needs `#[utoipa::path(...)]` AND must be registered in `portal-api/src/openapi.rs` under both `paths(...)` and `components(schemas(...))`. All request/response DTOs derive `ToSchema`. 22 tags currently registered — reuse existing tags where possible. See any handler in `portal-api/src/handlers/` for the pattern.

### Three-layer types
Each entity exists as: DB entity (`portal-db/src/entities/`, derives `FromRow`) → Domain entity (`portal-domain/src/entities/`, behavior) → API DTO (`portal-api/src/dto/`, derives `ToSchema`). Conversions via `From`/`TryFrom`.

### Repository + Service pattern
Repository traits live in `portal-domain/src/repositories/`. Services in `portal-domain/src/services/` are generic over those traits (enables test doubles). Postgres implementations are `Pg*Repository` in `portal-db/src/adapters/`.

### RBAC
Permission constants in `portal-core/src/permissions.rs` (28 constants across `team`, `league`, `tournament`, `match_`, `admin`). Use the `PermissionChecker` extractor in handlers:
- `require_permission(&auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)` — global
- `require_team_permission(&auth, team_id, permissions::team::SETTINGS_MANAGE)` — scoped (admin override falls back automatically)

### Strongly-typed IDs
All 43 entity IDs are newtypes in `portal-core/src/ids.rs`. New IDs: UUID v7 via `Id::new()`. Parse from string with `.parse()?`.

### Errors
`DomainError` (core) → `RepositoryError` (db) → `ApiError` (api, RFC 7807). API conversion lives in `portal-api/src/error.rs`.

### SQL safety
Parameterized SQLx queries only (`$1`, `$2` …). Never string-interpolate user input.

Adapters use the **runtime** query form (`sqlx::query_as::<_, Row>(...)`, bound to a `FromRow` struct) rather than the `query!`/`query_as!` macros. That means the schema is verified at test time, not compile time. `.sqlx/` is effectively unused until the schema stabilises and we migrate to the macro form. See `docs/audit-remediation.md` I3.

## Common Commands

```bash
# Setup
docker compose up -d postgres
cp .env.example .env   # default DATABASE_URL=postgres://portal:portal@localhost:5433/portal_dev

# Build / run
cargo build
cargo run -p portal-app          # server (auto-runs migrations)
cargo run -p portal-cli -- --help

# Quality gates
cargo fmt
cargo clippy -- -D warnings
cargo check

# Tests (need Docker for testcontainers)
cargo test
cargo test -p portal-api
RUST_LOG=debug cargo test test_name -- --nocapture

# Migrations (manual; usually auto on server start)
sqlx migrate run --database-url postgres://portal:portal@localhost:5433/portal_dev

# SQLx offline builds (rare — only a handful of compile-time macro call sites exist today)
DATABASE_URL=postgres://portal:portal@localhost:5433/portal_dev cargo sqlx prepare --workspace
```

If the local DB is stale: `docker compose down -v && docker compose up -d postgres`, then run migrations.

## Testing

Integration tests in `portal-api/tests/` use `TestApp` (testcontainers-backed). 18 builders in `portal-test/src/builders/` all expose `build()` and `build_persisted(pool)`. Dev auth (`Bearer dev-token`) is gated behind the `test-utils` feature flag and only for Rust integration tests.

## Adding Things — Where to Look

Rather than memorize a checklist, mirror an existing well-implemented feature. Good references:
- **New entity end-to-end**: `league_team` (migration → db entity → repo trait + Pg impl → domain entity → service → DTO → handler → routes → openapi → tests).
- **New endpoint on existing entity**: any handler in `portal-api/src/handlers/leagues/`.
- **New permission**: add constant in `portal-core/src/permissions.rs`, seed in the relevant role migration.

After any schema or query change: run migrations and `cargo check --workspace` against the live DB. `cargo sqlx prepare --workspace` only needs to run if you added a `query!`/`query_as!` macro invocation (rare today).

## API Discovery

- Swagger UI: http://localhost:3000/swagger-ui
- OpenAPI JSON: http://localhost:3000/api-docs/openapi.json
- Route registration: `portal-api/src/routes/` and `portal-api/src/openapi.rs`
- Migrations / current schema: `migrations/`

Don't enumerate endpoints from this file — query the OpenAPI spec or read the routes module. The same goes for the table list (`migrations/`) and permission constants (`portal-core/src/permissions.rs`).

## File Uploads

Image processing via `portal-storage`. Sizes/limits defined on `ImageType` in that crate (`PlayerAvatar`, `PlayerBanner`, `LeagueTeamLogo`, `LeagueTeamBanner`).

## Further Reading

- `docs/gaming-portal-hld.md` — high-level design, planned features
- `docs/gaming-portal-api-routes.md` — full API spec
- `docs/gaming-portal-database-schema.md` — complete schema reference