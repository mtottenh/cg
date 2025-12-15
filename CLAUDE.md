# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

This is a **Multi-Game Competitive Gaming Portal** backend built in Rust using Axum, SQLx, and PostgreSQL. The system supports competitive gaming across multiple game titles through a plugin-based architecture.

**Current Status**: Active development. Core authentication, players, leagues, league teams (seasonal rosters), and RBAC systems are implemented. Matches and tournaments are planned.

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

The project is a Cargo workspace with the following crates:

```
crates/
├── portal-core/      # Shared types, IDs, errors, permission constants
├── portal-domain/    # Domain entities, services, repository traits
├── portal-db/        # SQLx entities, repositories, adapters
├── portal-api/       # Axum handlers, routes, DTOs, OpenAPI specs
├── portal-storage/   # File storage abstraction (local, S3)
├── portal-test/      # Test utilities (testcontainers, builders)
├── portal-cli/       # Admin CLI tool
├── portal-app/       # Server entry point
├── portal-plugins/   # Plugin system (planned)
└── portal-cache/     # Caching layer (planned)
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
- **portal-api**: HTTP layer only, converts DTOs ↔ domain types.

## Critical: OpenAPI Documentation

**Every API endpoint MUST have OpenAPI documentation using utoipa.**

### Adding New Endpoints Checklist

1. **Handler**: Add `#[utoipa::path(...)]` attribute with all parameters, request body, responses
2. **DTOs**: Derive `ToSchema` for all request/response types
3. **Register in openapi.rs**: Add handler to `paths(...)` and schemas to `components(schemas(...))`
4. **Tags**: Use appropriate tag (league_teams, players, auth, etc.)

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

### OpenAPI Registration (portal-api/src/openapi.rs)

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // Add your handler here
        league_teams::create_league_team,
        league_teams::get_league_team,
        // ... etc
    ),
    components(
        schemas(
            // Add your DTOs here
            CreateLeagueTeamRequest,
            LeagueTeamResponse,
            // ... etc
        )
    ),
    // ...
)]
pub struct ApiDoc;
```

## Implemented Features

### Auth (`/v1/auth`)
- `POST /register` - Register new user with player profile
- `POST /login` - Authenticate and get JWT token

### Leagues (`/v1/leagues`)
- `POST /` - Create a league
- `GET /` - List leagues with pagination
- `GET /{id}` - Get league by ID
- `PATCH /{id}` - Update league settings

### League Seasons (`/v1/leagues/{league_id}/seasons`)
- `POST /` - Create a new season
- `GET /` - List seasons for a league
- `GET /{season_id}` - Get season details
- `PATCH /{season_id}` - Update season (dates, status)

### League Teams (`/v1/leagues/{league_id}/teams`)
Teams are scoped to leagues with seasonal participation:
- `POST /` - Create team within league (creator becomes captain)
- `GET /` - List teams in league
- `GET /{team_id}` - Get team details
- `PATCH /{team_id}` - Update team (requires `team.settings.manage`)

### League Team Seasons (`/v1/leagues/{league_id}/seasons/{season_id}/teams`)
- `POST /` - Register team for season
- `GET /` - List teams participating in season
- `GET /{team_id}` - Get team's season participation details

### League Team Members (`/v1/leagues/{league_id}/teams/{team_id}/members`)
Seasonal rosters:
- `GET /` - List team members for current season
- `PATCH /{member_id}` - Update member role (requires `team.roles.manage`)
- `DELETE /{member_id}` - Remove member (requires `team.roster.manage`)
- `POST /leave` - Leave team voluntarily

### League Team Invitations (`/v1/leagues/{league_id}/teams/{team_id}/invitations`)
- `POST /` - Invite player to team for season
- `GET /` - Get team's pending invitations
- `GET /me` - Get current player's pending invitations
- `POST /{id}/accept` - Accept invitation
- `POST /{id}/decline` - Decline invitation
- `DELETE /{id}` - Cancel invitation (captain)

### Players (`/v1/players`)
- `GET /` - Search players
- `GET /{id}` - Get player by ID
- `GET /{id}/teams` - Get player's league team memberships
- `GET /me` - Get current player's profile
- `PATCH /me` - Update current player's profile
- `POST /me/avatar` - Upload player avatar (multipart)
- `POST /me/banner` - Upload player banner (multipart)

### Users (`/v1/users`)
- `GET /me` - Get current user

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

Permissions use a scoped model defined in `portal-core/src/permissions.rs`:

```rust
// Permission constants
pub mod team {
    pub const ROSTER_MANAGE: &str = "team.roster.manage";
    pub const SETTINGS_MANAGE: &str = "team.settings.manage";
    pub const ROLES_MANAGE: &str = "team.roles.manage";
}

// In handlers, use PermissionChecker extractor:
perm_checker
    .require_team_permission(&auth, team_id, permissions::team::SETTINGS_MANAGE)
    .await?;
```

### Strongly-Typed IDs

All entity IDs use newtype wrappers from `portal-core/src/ids.rs`:

```rust
// Use the ID types, never raw UUIDs in domain code
let team_id: LeagueTeamId = "...".parse()?;
let player_id = PlayerId::new(); // UUID v7
```

## Database

### Migrations

Located in `/migrations/`. Run automatically on server startup.

Current tables:
- `users`, `players`, `player_game_profiles`
- `games`
- `leagues`, `league_members`, `league_invitations`
- `league_seasons`
- `league_teams`, `league_team_seasons`, `league_team_members`, `league_team_invitations`
- `roles`, `permissions`, `role_permissions`, `user_roles`
- `bans`
- `entity_changes` (audit log)

### Running Migrations Manually

```bash
# Via sqlx-cli
sqlx migrate run

# Or let the server run them on startup
cargo run -p portal-app
```

## Development Commands

### Prerequisites

```bash
# Start PostgreSQL (via Docker Compose)
docker compose up -d

# Set environment variables
cp .env.example .env
# Edit .env with your DATABASE_URL
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
# Generate query metadata
cargo sqlx prepare --workspace

# Commit .sqlx/ directory
```

## Testing Patterns

### Integration Tests

Use `TestApp` wrapper with testcontainers:

```rust
#[tokio::test]
async fn test_create_league_team() {
    let app = TestApp::new().await;  // Spins up PostgreSQL container

    // First create a league
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

Use builders from `portal-test` for creating test data:

```rust
let user = UserBuilder::new()
    .username("testuser")
    .email("test@example.com")
    .build_persisted(app.pool())
    .await;

let league = LeagueBuilder::new()
    .name("Test League")
    .build_persisted(app.pool())
    .await;

let team = LeagueTeamBuilder::new()
    .name("Test Team")
    .league_id(league.id)
    .build_persisted(app.pool())
    .await;
```

### Dev Auth Mode

Dev auth mode is used only for Rust integration tests (enabled via `test-utils` feature flag). The frontend dev mode button has been removed - E2E tests now use real admin authentication via `loginAsAdmin()`.

For Rust integration tests, the `AuthenticatedUser` extractor accepts `Bearer dev-token` and uses the seeded dev user when the `test-utils` feature is enabled.

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
// Domain errors automatically convert to ApiError
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
// Supported image types with size limits
ImageType::LeagueTeamLogo   // 512x512, max 2MB
ImageType::LeagueTeamBanner // 1920x480, max 5MB
ImageType::PlayerAvatar     // 256x256, max 1MB
ImageType::PlayerBanner     // 1920x480, max 5MB
```

Storage backends: `LocalStorage` (default), `S3Storage` (feature flag).

## Security Considerations

1. **Passwords**: Hashed with Argon2id
2. **SQL Injection**: Use SQLx parameterized queries only (`$1`, `$2`)
3. **RBAC**: Check permissions before every protected operation
4. **Input Validation**: Use `validator` crate on request DTOs
5. **Secrets**: Use environment variables, never commit `.env`

## Adding New Features

### New Entity Checklist

1. **Migration**: Create in `/migrations/` with next sequence number
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

### New API Endpoint Checklist

1. Add handler with `#[utoipa::path]` attribute
2. Add route in routes module
3. Register handler in `openapi.rs` paths
4. Register any new DTOs in `openapi.rs` schemas
5. Add integration test
6. Update this file if it's a new feature area

## Documentation References

Design documentation in `/docs/`:
- `gaming-portal-hld.md`: High-level design, architecture
- `gaming-portal-api-routes.md`: Full API specification
- `gaming-portal-database-schema.md`: Complete database schema
