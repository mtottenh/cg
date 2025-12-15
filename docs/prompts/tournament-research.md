 # Tournament System Architecture Research & Design

  ## Context

  You are working on a multi-game competitive gaming
  portal backend built in Rust (Axum, SQLx, PostgreSQL).
   The codebase follows a clean layered architecture:

  - `portal-core`: Shared types, IDs, errors,
  permissions
  - `portal-domain`: Domain entities, services,
  repository traits (generic over repositories)
  - `portal-db`: PostgreSQL repository implementations
  - `portal-api`: Axum handlers, routes, DTOs, OpenAPI
  specs

  Review the existing implementations to understand
  patterns:
  - League teams system:
  `crates/portal-domain/src/services/league_team/`
  - Repository adapters:
  `crates/portal-db/src/adapters/league_team/`
  - Handlers:
  `crates/portal-api/src/handlers/league_teams/`
  - Database migrations: `migrations/`
  - Entity definitions:
  `crates/portal-domain/src/entities/`

  ## Research Task

  Perform a comprehensive analysis and create a detailed
   design document for implementing **Tournaments** in
  this backend. Think deeply about each aspect before
  writing. Consider edge cases, extensibility, and how
  the system will evolve.

  ### 1. Domain Model Research

  Analyze and design the core tournament entities:

  **Tournament Hierarchy:**
  - How do tournaments relate to leagues and seasons?
  - Can tournaments exist independently (one-off
  events)?
  - Should tournaments be nested (qualifiers → main
  event)?
  - How do we handle multi-stage tournaments (groups →
  playoffs)?

  **Tournament Types to Support:**
  - Single Elimination
  - Double Elimination
  - Round Robin
  - Swiss System
  - Group Stage + Playoffs (hybrid)
  - Custom/Plugin-defined formats

  **Participant Models:**
  - Team-based tournaments (existing league teams)
  - Solo/PuG tournaments (ad-hoc player registration)
  - Mixed formats (teams formed at registration)
  - How do we handle roster locks vs. flexible rosters?

  ### 2. Plugin System Integration

  Research how game-specific features should integrate:

  **Map/Arena System:**
  - Map pools per tournament
  - Map veto/pick-ban systems (game-specific rules)
  - Best-of-N series with map selection
  - How do different games handle this differently?

  **Game Plugin Contracts:**
  - What interface should game plugins implement?
  - How do we validate game-specific tournament
  settings?
  - Plugin-provided bracket seeding algorithms?
  - Plugin-provided match result validation?

  **Extensibility Points:**
  - Where should the plugin system hook into tournament
  logic?
  - How do we handle plugin-specific data in a generic
  way? (JSON columns? Trait objects?)

  ### 3. Registration & Seeding

  Design the registration flow:

  **Registration Models:**
  - Open registration with capacity limits
  - Invite-only tournaments
  - Qualification-based entry
  - Waitlists and check-in systems

  **Seeding:**
  - Manual seeding by admins
  - Rating/MMR-based seeding
  - Random seeding
  - Plugin-provided seeding algorithms
  - How do we handle seeding for different bracket
  types?

  **Eligibility:**
  - Player/team eligibility requirements
  - Game profile requirements (linked accounts)
  - Region restrictions
  - Rating/rank requirements

  ### 4. Match System

  Design how tournament matches work:

  **Match Lifecycle:**
  - Match scheduling (auto-generated vs manual)
  - Check-in requirements
  - Match states (scheduled → in_progress → completed → 
  disputed)
  - Result reporting (self-report, admin verify, API 
  integration)

  **Series/Games:**
  - Best-of-N series structure
  - Individual game results within a series
  - Map selection per game
  - Overtime/tiebreaker rules

  **Bracket Progression:**
  - How do match results update brackets?
  - Handling walkovers/forfeits/DQs
  - Bracket reseeding between stages

  ### 5. Database Schema Design

  Propose the PostgreSQL schema:

  **Core Tables:**
  - `tournaments` - Tournament metadata
  - `tournament_stages` - Multi-stage support
  - `tournament_brackets` - Bracket structure
  - `tournament_matches` - Individual matches
  - `tournament_match_games` - Games within a series
  - `tournament_registrations` - Participant
  registrations
  - `tournament_seeds` - Seeding information

  **Relationships:**
  - Tournament → League (optional, nullable)
  - Tournament → Season (optional, nullable)
  - Tournament → Game (required)
  - How do PuG tournaments reference players vs teams?

  **Plugin Data Storage:**
  - How to store game-specific settings?
  - How to store match-specific plugin data (map picks,
  etc.)?

  ### 6. Service Layer Architecture

  Design the service layer with proper separation:

  **Proposed Services:**
  - `TournamentService` - CRUD, lifecycle management
  - `TournamentRegistrationService` - Registration flow
  - `TournamentBracketService` - Bracket
  generation/management
  - `TournamentMatchService` - Match operations
  - `TournamentSeedingService` - Seeding algorithms

  **Generic Type Parameters:**
  - Which repositories does each service need?
  - How do we inject the game plugin system?
  - What traits should be mockable for testing?

  ### 7. Testing Strategy

  Plan the testing approach:

  **Unit Tests:**
  - Bracket generation algorithms
  - Seeding algorithms
  - Match progression logic
  - State machine transitions

  **Integration Tests:**
  - Full tournament lifecycle
  - Registration flows
  - Bracket updates on match completion

  **Test Fixtures:**
  - Tournament builder patterns
  - Bracket test data generators

  ### 8. API Design

  Outline the REST API structure:

  **Endpoints to Design:**
  - Tournament CRUD
  - Registration management
  - Bracket retrieval
  - Match operations
  - Admin operations

  **Consider:**
  - Pagination for large brackets
  - Real-time updates (WebSocket considerations)
  - Bulk operations

  ### 9. Implementation Phases

  Break down into implementable phases:

  **Phase 1: Core Foundation**
  - What's the minimum viable tournament?
  - Core entities and basic bracket types

  **Phase 2: Registration & Seeding**
  - Registration flows
  - Seeding system

  **Phase 3: Match System**
  - Match lifecycle
  - Result reporting

  **Phase 4: Plugin Integration**
  - Game-specific features
  - Map pools/veto

  **Phase 5: Advanced Features**
  - Multi-stage tournaments
  - Advanced bracket types

  ## Output Requirements

  Create a detailed plan document that includes:

  1. **Entity Diagrams** - Relationships between all
  tournament entities
  2. **Database Schema** - Complete SQL migration
  scripts (draft)
  3. **Trait Definitions** - Key repository and service
  traits
  4. **API Routes** - Full endpoint specification
  5. **State Machines** - Tournament and match state
  diagrams
  6. **Plugin Interface** - Game plugin contract for
  tournaments
  7. **Test Plan** - What tests are needed for each
  module
  8. **Implementation Order** - Dependency-ordered task
  breakdown

  Write the plan to: `docs/tournament-system-design.md`

  ## Key Constraints

  - Follow existing patterns in the codebase
  - Use strongly-typed IDs (like `TournamentId`,
  `TournamentMatchId`)
  - Implement repository traits in `portal-domain`,
  adapters in `portal-db`
  - All handlers need OpenAPI documentation
  - Design for testability with generic type parameters
  - Support the existing permission system
  - Consider future WebSocket support for live updates

  ## Questions to Answer

  As you research, explicitly answer:

  1. Should a tournament belong to a season, or be
  independent?
  2. How do we model a "pickup game" tournament vs team
  tournament?
  3. What's the plugin interface for custom bracket
  types?
  4. How do we handle partial match results in a series?
  5. What happens when a participant withdraws
  mid-tournament?
  6. How do we support both sync and async tournaments?
  7. What data needs to be denormalized for bracket
  display performance?
  8. How do we handle timezone issues for scheduled
  matches?

  Take your time. Think through each aspect thoroughly
  before designing. Reference existing code patterns.
  The goal is a comprehensive, implementable design.