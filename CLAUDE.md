# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

This is a **Multi-Game Competitive Gaming Portal** backend built in Rust using Axum, SQLx, and PostgreSQL. The system supports competitive gaming across multiple game titles through a plugin-based architecture.

**Current Status**: Active development. Core authentication, teams, players, invitations, and RBAC systems are implemented. Matches, tournaments, and leagues are planned.

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
4. **Tags**: Use appropriate tag (teams, players, auth, etc.)

### Example Handler with OpenAPI

```rust
/// Create a new team.
#[utoipa::path(
    post,
    path = "/v1/teams",
    request_body = CreateTeamRequest,
    responses(
        (status = 201, description = "Team created", body = DataResponse<TeamResponse>),
        (status = 400, description = "Validation error", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 409, description = "Team name or tag already taken", body = ApiError),
    ),
    security(("bearer_auth" = [])),
    tag = "teams"
)]
pub async fn create_team(...) -> ApiResult<...> { ... }
```

### OpenAPI Registration (portal-api/src/openapi.rs)

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // Add your handler here
        teams::create_team,
        teams::get_team,
        // ... etc
    ),
    components(
        schemas(
            // Add your DTOs here
            CreateTeamRequest,
            TeamResponse,
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

### Teams (`/v1/teams`)
- `POST /` - Create team (creator becomes founding captain)
- `GET /` - List teams with search/pagination
- `GET /{id}` - Get team by ID
- `PATCH /{id}` - Update team (requires `team.settings.manage`)
- `GET /{id}/members` - List team members
- `PATCH /{id}/members/{player_id}` - Update member role (requires `team.roles.manage`)
- `DELETE /{id}/members/{player_id}` - Remove member (requires `team.roster.manage`)
- `POST /{id}/leave` - Leave team voluntarily
- `POST /{id}/logo` - Upload team logo (multipart)
- `POST /{id}/banner` - Upload team banner (multipart)

### Team Invitations (`/v1/invitations`, `/v1/teams/{id}/invitations`)
- `POST /teams/{id}/invitations` - Invite player to team
- `GET /teams/{id}/invitations` - Get team's pending invitations
- `GET /invitations/me` - Get my pending invitations
- `GET /invitations/me/count` - Count my pending invitations
- `POST /invitations/{id}/accept` - Accept invitation
- `POST /invitations/{id}/decline` - Decline invitation
- `DELETE /invitations/{id}` - Cancel invitation (captain)

### Players (`/v1/players`)
- `GET /` - Search players
- `GET /{id}` - Get player by ID
- `GET /{id}/teams` - Get player's team memberships
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
pub trait TeamRepository: Send + Sync + 'static {
    async fn find_by_id(&self, id: TeamId) -> Result<Option<Team>, DomainError>;
    // ...
}

// Implementation in portal-db
pub struct PgTeamRepository { pool: DbPool }

#[async_trait]
impl TeamRepository for PgTeamRepository {
    async fn find_by_id(&self, id: TeamId) -> Result<Option<Team>, DomainError> {
        // SQLx query, convert DbTeam -> Team
    }
}
```

### Service Pattern

Services are generic over repository traits, enabling testability:

```rust
pub struct TeamService<TR, TMR, PR>
where
    TR: TeamRepository,
    TMR: TeamMemberRepository,
    PR: PlayerRepository,
{
    team_repo: Arc<TR>,
    member_repo: Arc<TMR>,
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
let team_id: TeamId = "...".parse()?;
let player_id = PlayerId::new(); // UUID v7
```

## Database

### Migrations

Located in `/migrations/`. Run automatically on server startup.

Current tables:
- `users`, `players`, `player_game_profiles`
- `games`
- `teams`, `team_members`, `team_invitations`
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
cargo test test_create_team
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
async fn test_create_team() {
    let app = TestApp::new().await;  // Spins up PostgreSQL container

    let response = app.post_json("/v1/teams", &json!({
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

let team = TeamBuilder::new()
    .name("Test Team")
    .with_founder(player.id)
    .build_persisted(app.pool())
    .await;
```

### Dev Auth Mode

When `DEV_AUTH=true`, the `AuthenticatedUser` extractor accepts `Bearer dev-token` and uses the seeded dev user. Useful for manual testing.

## CLI Tool

The `portal-cli` provides administrative operations:

```bash
# User management
portal user list
portal user get <user_id>

# Role management
portal role list
portal role assign <user_id> <role_name>

# Team management
portal team list
portal team get <team_id>

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
            DomainError::TeamNotFound(_) => ApiError::not_found(e.to_string()),
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
ImageType::TeamLogo     // 512x512, max 2MB
ImageType::TeamBanner   // 1920x480, max 5MB
ImageType::PlayerAvatar // 256x256, max 1MB
ImageType::PlayerBanner // 1920x480, max 5MB
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
