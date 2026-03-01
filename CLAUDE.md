# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

This is a **Multi-Game Competitive Gaming Portal** backend built in Rust using Axum, SQLx, and PostgreSQL. The system supports competitive gaming across multiple game titles through a plugin-based architecture.

**Current Status**: Active development. Core platform (auth, players, leagues, league teams, RBAC, bans) is production-ready. Tournament system (brackets, matches, veto, results, disputes, forfeits, progression) is implemented and tested. Matchmaking, OAuth, lobbies, game server integration, and substitute systems are planned but not started.

**Scale**: 194 handler functions, 228 OpenAPI paths, 44 database migrations, 27 domain services, 362+ integration tests.

## Technology Stack

| Component | Technology | Version |
|-----------|-----------|---------|
| Language | Rust | 1.85+ (Edition 2024) |
| Web Framework | Axum | 0.8 |
| Database | PostgreSQL | 16+ |
| Database Access | SQLx | 0.8 (compile-time query verification) |
| Async Runtime | Tokio | 1.x |
| Serialization | serde + serde_json | Latest |
| OpenAPI | utoipa + utoipa-swagger-ui | 5.x / 9.x |
| Authentication | JWT (jsonwebtoken) + argon2 | Latest |
| Observability | tracing + tracing-subscriber | Latest |
| Testing | testcontainers + fake | Latest |

## Workspace Structure

```
crates/
├── portal-core/      # Shared types (43 ID types), errors, 28 permission constants
├── portal-domain/    # 24 entity modules, 27 services, 44 repository traits
├── portal-db/        # SQLx entities, Pg*Repository adapters, 44 migrations
├── portal-api/       # 194 handlers, 15 route modules, 150+ DTOs, OpenAPI
├── portal-storage/   # File storage (LocalStorage, S3Storage)
├── portal-test/      # 18 test builders, TestApp helper, testcontainers
├── portal-cli/       # Admin CLI (users, roles, players, games, bans, audit, leagues, db, bootstrap)
├── portal-app/       # Server entry point
├── portal-plugins/   # CS2 plugin (demo parsing, evidence validation)
└── portal-cache/     # Redis connection wrapper (stub)
```

### Crate Dependencies (Layered Architecture)

```
portal-app ─────► portal-api ─────► portal-domain ─────► portal-core
                      │                   │
                      ▼                   ▼
                 portal-db ◄───────► portal-domain
                      │
                      ▼
                 portal-storage
```

- **portal-core**: No async dependencies, pure types. Used by all other crates.
- **portal-domain**: Defines repository traits, services use generic type parameters.
- **portal-db**: Implements repository traits with `Pg*Repository` adapters.
- **portal-api**: HTTP layer only, converts DTOs <-> domain types.

## Critical: OpenAPI Documentation

**Every API endpoint MUST have OpenAPI documentation using utoipa.**

### Adding New Endpoints Checklist

1. **Handler**: Add `#[utoipa::path(...)]` attribute with all parameters, request body, responses
2. **DTOs**: Derive `ToSchema` for all request/response types
3. **Register in openapi.rs**: Add handler to `paths(...)` and schemas to `components(schemas(...))`
4. **Tags**: Use appropriate tag (22 tags currently registered)

### Example Handler with OpenAPI

```rust
/// Create a new league team.
#[utoipa::path(
    post,
    path = "/v1/leagues/{league_id}/teams",
    request_body = CreateLeagueTeamRequest,
    responses(
        (status = 201, description = "Team created", body = DataResponse<LeagueTeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Team name or tag already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "league_teams"
)]
pub async fn create_league_team(...) -> ApiResult<...> { ... }
```

## Implemented Features

### Auth (`/v1/auth`) - 2 endpoints
- `POST /register` - Register new user with player profile
- `POST /login` - Authenticate and get JWT token

### Players (`/v1/players`) - 6 endpoints
- `GET /` - Search players
- `GET /me` - Get current player's profile
- `PATCH /me` - Update current player's profile
- `POST /me/avatar` - Upload player avatar (multipart)
- `POST /me/banner` - Upload player banner (multipart)
- `GET /{player_id}` - Get player by ID

### Users (`/v1/users`) - 3 endpoints
- `GET /me` - Get current user
- `GET /me/leagues` - Get my league memberships
- `GET /me/league-invitations` - Get my pending league invitations

### Games (`/v1/games`) - 8 endpoints
- `GET /` - List games
- `GET /{game_id}` - Get game details
- `GET /{game_id}/maps` - Get maps for game
- `GET /{game_id}/rank-tiers` - Get rank tiers
- `PATCH /{game_id}` - Update game
- `PUT /{game_id}/maps` - Set map pool
- `POST /{game_id}/enable` - Enable game
- `POST /{game_id}/disable` - Disable game

### Leagues (`/v1/leagues`) - 17 endpoints
Full CRUD, member management, applications, invitations, slug lookup.

### League Teams & Seasons - 27 endpoints
Teams scoped to leagues with seasonal participation. Includes:
- Season CRUD and team registration
- Team CRUD with ownership transfer
- Seasonal roster management (add/remove/promote/demote members)
- Invitation and application workflows

### Tournaments (`/v1/tournaments`) - 40 endpoints
- **CRUD**: Create, list (with filters), get by ID/slug, update
- **Lifecycle**: Publish, open registration, start
- **Stages & Brackets**: Create stages, view brackets and matches
- **Registration**: Team/player registration, approval/rejection, check-in, withdrawal
- **Seeding**: Auto-seed (multiple algorithms), manual seed, clear
- **Match Lifecycle**: Status tracking, check-in, scheduling, forfeit
- **Match Scheduling**: Propose/accept/reject/counter-propose schedule times
- **Admin**: Force match transitions, admin scheduling

### Map Veto System (`/v1/matches/{match_id}/veto`) - 6 endpoints + WebSocket
- Create/get veto sessions, start session
- Record coin flip, perform veto actions (ban/pick), select side
- WebSocket real-time veto at `/v1/ws/veto/{match_id}`
- Veto delegation system for team members

### Match Results (`/v1/matches/{match_id}/result`) - 5 endpoints
- Submit result claims, get/list claims
- Confirm result (opponent), dispute result

### Evidence Management (`/v1/matches/{match_id}/evidence`) - 13 endpoints
- Upload (initiate + complete), add link evidence
- Discover/link evidence, validate demos
- Get/delete evidence, access URLs

### Demo Catalog (`/v1/demos`) - 4 public + 8 admin endpoints
- Browse demos, get players/links
- Admin: catalog, categorize, set visibility, associate, link to matches

### Disputes (`/v1/disputes`) - 2 public + 8 admin endpoints
- Raise disputes, add messages, get dispute with thread
- Admin: list, add messages (internal), assign, resolve (uphold/overturn/rematch/adjusted/double-dq)

### Forfeits - 1 participant + 3 admin endpoints
- Withdraw from tournament
- Admin: forfeit match, disqualify registration, double forfeit

### Result Reviews - 2 participant + 4 admin endpoints
- Get review for match, acknowledge roster mismatch
- Admin: list pending, get by ID, approve, reject

### Progression - 1 public + 3 admin endpoints
- Get progression for match
- Admin: revert, reapply, process progression

### Availability - 11 endpoints
- Player availability windows (CRUD)
- Availability overrides/exceptions
- Match scheduling suggestions

### Bans (admin) - 5 endpoints
- Create/list/get/lift bans, get user bans

### Roles & Permissions (admin) - 11 endpoints
- Role CRUD, permission assignment
- User role assignment/revocation

### Admin Dashboard - 1 endpoint
- `GET /admin/stats` - Platform statistics

## Architecture Patterns

### Type Separation (Three-Layer Types)

Each entity has three type representations:

1. **DB Entity** (`portal-db/src/entities/`): Flat struct matching SQL row, derives `FromRow`
2. **Domain Entity** (`portal-domain/src/entities/`): Rich type with behavior
3. **API DTO** (`portal-api/src/dto/`): Request/response types, derives `ToSchema`

Conversions use `From`/`TryFrom` implementations.

### Repository Pattern

```rust
// Trait in portal-domain
#[async_trait]
pub trait LeagueTeamRepository: Send + Sync + 'static {
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError>;
    // ...
}

// Implementation in portal-db
pub struct PgLeagueTeamRepository { pool: DbPool }

#[async_trait]
impl LeagueTeamRepository for PgLeagueTeamRepository {
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError> {
        // SQLx query, convert DbLeagueTeam -> LeagueTeam
    }
}
```

### Service Pattern

Services are generic over repository traits, enabling testability:

```rust
pub struct LeagueTeamService<LTR, LTMR, PR>
where
    LTR: LeagueTeamRepository,
    LTMR: LeagueTeamMemberRepository,
    PR: PlayerRepository,
{
    team_repo: Arc<LTR>,
    member_repo: Arc<LTMR>,
    player_repo: Arc<PR>,
}
```

### RBAC Permission System

Permissions use a scoped model defined in `portal-core/src/permissions.rs` (28 constants across 5 modules: team, league, tournament, match_, admin).

```rust
// In handlers, use PermissionChecker extractor:

// For global admin permissions:
perm_checker
    .require_permission(&auth, permissions::admin::TOURNAMENTS_MANAGE_ANY)
    .await?;

// For scoped permissions (checks scoped permission + admin override fallback):
perm_checker
    .require_team_permission(&auth, team_id, permissions::team::SETTINGS_MANAGE)
    .await?;
```

### Strongly-Typed IDs

All entity IDs (43 types) use newtype wrappers from `portal-core/src/ids.rs`:

```rust
let team_id: LeagueTeamId = "...".parse()?;
let player_id = PlayerId::new(); // UUID v7
```

## Database

### Migrations

Located in `/migrations/`. 44 migrations total. Run automatically on server startup.

Current tables:
- **Identity**: `users`, `players`, `player_game_profiles`
- **Games**: `games`
- **Leagues**: `leagues`, `league_members`, `league_invitations`
- **League Teams**: `league_seasons`, `league_teams`, `league_team_seasons`, `league_team_members`, `league_team_invitations`
- **RBAC**: `roles`, `permissions`, `role_permissions`, `user_roles`
- **Bans**: `bans`
- **Audit**: `entity_changes`
- **Tournaments**: `tournaments`, `tournament_stages`, `tournament_brackets`, `tournament_registrations`, `tournament_matches`, `tournament_match_games`, `tournament_map_pools`, `tournament_map_pool_maps`
- **Match Workflow**: `match_status_logs`, `schedule_proposals`, `availability_windows`, `availability_exceptions`, `suggested_times`
- **Veto**: `veto_sessions`, `veto_actions`, `veto_lobby_messages`, `veto_delegates`
- **Results**: `result_claims`, `evidence`, `result_reviews`
- **Progression**: `sagas`, `progression_logs`
- **Forfeits**: `forfeits`
- **Disputes**: `disputes`, `dispute_messages`
- **Demos**: `demos`, `demo_match_links`, `demo_players`

### Running Migrations Manually

```bash
# Via sqlx-cli
sqlx migrate run --database-url postgres://portal:portal@localhost:5433/portal_dev

# Or let the server run them on startup
cargo run -p portal-app
```

## Development Commands

### Prerequisites

```bash
# Start PostgreSQL (via Docker Compose)
docker compose up -d postgres

# Set environment variables
cp .env.example .env
# Default DATABASE_URL: postgres://portal:portal@localhost:5433/portal_dev
```

### Building & Running

```bash
# Development build
cargo build

# Run the API server
cargo run -p portal-app

# Run the CLI
cargo run -p portal-cli -- --help

# Check without building
cargo check

# Format code
cargo fmt

# Lint (pedantic)
cargo clippy -- -D warnings
```

### Testing

```bash
# Run all tests (requires Docker for testcontainers)
cargo test

# Run specific crate tests
cargo test -p portal-api

# Run with logs
RUST_LOG=debug cargo test -- --nocapture

# Run specific test
cargo test test_create_league_team
```

### SQLx Offline Mode

For CI without database access:

```bash
# Generate query metadata (requires running database)
DATABASE_URL=postgres://portal:portal@localhost:5433/portal_dev cargo sqlx prepare --workspace

# If the DB volume is stale, reset it first:
docker compose down -v && docker compose up -d postgres
# Wait a few seconds, then run migrations:
sqlx migrate run --database-url postgres://portal:portal@localhost:5433/portal_dev
# Then regenerate:
DATABASE_URL=postgres://portal:portal@localhost:5433/portal_dev cargo sqlx prepare --workspace

# Commit .sqlx/ directory
```

## Testing Patterns

### Integration Tests

362+ integration tests across 16 test files in `portal-api/tests/`. Use `TestApp` wrapper with testcontainers:

```rust
#[tokio::test]
async fn test_create_league_team() {
    let app = TestApp::new().await;  // Spins up PostgreSQL container

    let league = LeagueBuilder::new()
        .name("Test League")
        .build_persisted(app.pool())
        .await;

    let response = app.post_json(&format!("/v1/leagues/{}/teams", league.id), &json!({
        "name": "Test Team",
        "tag": "TST"
    })).await;

    response.assert_status(StatusCode::CREATED);
}
```

### Test Builders

18 builders available in `portal-test/src/builders/`:

```
UserBuilder, PlayerBuilder, GameBuilder, LeagueBuilder, LeagueSeasonBuilder,
LeagueSeasonParticipantBuilder, LeagueTeamBuilder, LeagueTeamSeasonBuilder,
LeagueTeamMemberBuilder, LeagueTeamInvitationBuilder, TournamentBuilder,
TournamentStageBuilder, TournamentBracketBuilder, TournamentMatchBuilder,
TournamentRegistrationBuilder, DemoBuilder, DemoMatchLinkBuilder,
VetoSessionBuilder, VetoDelegateBuilder
```

All builders support `build()` and `build_persisted(pool)` patterns with sensible defaults.

### Dev Auth Mode

Dev auth mode is used only for Rust integration tests (enabled via `test-utils` feature flag). The `AuthenticatedUser` extractor accepts `Bearer dev-token` and uses the seeded dev user when the `test-utils` feature is enabled.

## CLI Tool

The `portal-cli` provides administrative operations:

```bash
# User management
portal user list
portal user get <user_id>

# Role management
portal role list
portal role assign <user_id> <role_name>

# League management
portal league list
portal league get <league_id>

# Database utilities
portal db stats
```

## API Documentation

- **Swagger UI**: http://localhost:3000/swagger-ui
- **OpenAPI JSON**: http://localhost:3000/api-docs/openapi.json

## Error Handling

Errors flow through layers:

1. **DomainError** (`portal-core`): Business rule violations
2. **RepositoryError** (`portal-db`): Database errors
3. **ApiError** (`portal-api`): HTTP responses (RFC 7807 format)

```rust
impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::LeagueTeamNotFound(_) => ApiError::not_found(e.to_string()),
            DomainError::NotAuthorized(_) => ApiError::forbidden(e.to_string()),
            // ...
        }
    }
}
```

## File Uploads

Image uploads use `portal-storage` with automatic processing:

```rust
ImageType::LeagueTeamLogo   // 512x512, max 2MB
ImageType::LeagueTeamBanner // 1920x480, max 5MB
ImageType::PlayerAvatar     // 256x256, max 1MB
ImageType::PlayerBanner     // 1920x480, max 5MB
```

Storage backends: `LocalStorage` (default), `S3Storage` (feature flag).

## Security Considerations

1. **Passwords**: Hashed with Argon2id
2. **SQL Injection**: Use SQLx parameterized queries only (`$1`, `$2`)
3. **RBAC**: Check permissions before every protected operation using `PermissionChecker` extractor
4. **Input Validation**: Use `validator` crate on request DTOs
5. **Secrets**: Use environment variables, never commit `.env`

## Adding New Features

### New Entity Checklist

1. **Migration**: Create in `/migrations/` with next sequence number (currently at 44)
2. **DB Entity**: Add to `portal-db/src/entities/`
3. **Repository**: Add trait to `portal-domain/src/repositories/`, impl to `portal-db/src/adapters/`
4. **Domain Entity**: Add to `portal-domain/src/entities/`
5. **Service**: Add to `portal-domain/src/services/`
6. **DTOs**: Add request/response to `portal-api/src/dto/`
7. **Handlers**: Add to `portal-api/src/handlers/` with `#[utoipa::path]`
8. **Routes**: Register in `portal-api/src/routes/`
9. **OpenAPI**: Register in `portal-api/src/openapi.rs` (paths AND schemas)
10. **State**: Add service to `AppState` if needed
11. **Tests**: Add integration tests in `portal-api/tests/`
12. **SQLx Cache**: Regenerate with `cargo sqlx prepare --workspace`

### New API Endpoint Checklist

1. Add handler with `#[utoipa::path]` attribute
2. Add route in routes module
3. Register handler in `openapi.rs` paths
4. Register any new DTOs in `openapi.rs` schemas
5. Add `PermissionChecker` extractor for admin/protected endpoints
6. Add integration test
7. Update this file if it's a new feature area

## Planned Features (Not Yet Started)

These features are described in design docs but have zero code:

- **Matchmaking System**: Queue-based matchmaking with wait time estimation
- **OAuth / External Auth**: Steam, Discord, Twitch integration; token refresh; 2FA
- **Lobby System**: Pre-match lobby creation with real-time WebSocket state
- **Substitute System**: Substitute pool management, availability, request/response workflow
- **Game Server Integration**: Server registration, health checks, RCON, Get5 webhooks

## Documentation References

Design documentation in `/docs/`:
- `gaming-portal-hld.md`: High-level design, architecture
- `gaming-portal-api-routes.md`: Full API specification
- `gaming-portal-database-schema.md`: Complete database schema
