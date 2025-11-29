# High-Level Design Document
## Multi-Game Competitive Gaming Portal

**Version:** 1.0  
**Status:** Draft for Engineering Review  
**Last Updated:** November 2024

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
   - 1.1 [Core Data Model](#11-core-data-model)
2. [System Architecture Overview](#2-system-architecture-overview)
3. [Cascading Operations & Saga Pattern](#3-cascading-operations--saga-pattern)
4. [Middleware Requirements](#4-middleware-requirements)
5. [WebSocket Lobby System Design](#5-websocket-lobby-system-design)
6. [Plugin System Architecture](#6-plugin-system-architecture)
7. [Module Descriptions](#7-module-descriptions)
8. [Data Design](#8-data-design)
9. [Substitute & Availability System](#9-substitute--availability-system)
10. [REST API Design Overview](#10-rest-api-design-overview)
11. [Security & Permissions Model](#11-security--permissions-model)
12. [Scalability & Performance Considerations](#12-scalability--performance-considerations)
13. [Deployment & DevOps Notes](#13-deployment--devops-notes)
14. [Appendices](#14-appendices)

---

## 1. Executive Summary

This document describes the high-level design for a Rust-based backend platform supporting a multi-game competitive gaming portal. The system employs a plugin architecture enabling game-specific implementations of matchmaking, ranking, metadata, and lobby interactions while maintaining a secure, scalable core platform.

### Key Design Goals

- **Extensibility:** Plugin-based architecture for game-specific logic without core modifications
- **Performance:** Async-first design leveraging Rust's zero-cost abstractions and Tokio runtime
- **Security:** Defense-in-depth with strong RBAC, sandboxed plugins, and secure WebSocket channels
- **Scalability:** Horizontal scaling for stateless services, sticky sessions for WebSocket lobbies
- **Maintainability:** Clear module boundaries, comprehensive observability, and type-safe interfaces

### Technology Stack Summary

| Layer | Technology | Justification |
|-------|------------|---------------|
| Language | Rust 1.75+ | Memory safety, performance, async ecosystem maturity |
| Web Framework | Axum 0.7+ | Tower middleware ecosystem, WebSocket support, ergonomic extractors |
| Database | PostgreSQL 15+ | JSONB for plugin data, strong consistency, mature tooling |
| Database Access | SQLx 0.7+ | Compile-time query verification, async, minimal overhead |
| Async Runtime | Tokio 1.x | Industry standard, comprehensive feature set |
| Serialization | serde + serde_json | De facto standard, excellent performance |
| Authentication | JWT (jsonwebtoken) + OAuth2 | Stateless auth, industry standard |
| Observability | tracing + OpenTelemetry | Structured logging, distributed tracing |

---

## 1.1 Core Data Model

This section defines the fundamental entity relationships that govern the entire system. All services, schemas, and APIs must maintain consistency with this model.

### Players, Teams & Membership

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        PLAYERS & TEAMS                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────┐    M:N     ┌──────────┐                                   │
│  │  Player  │◄──────────►│   Team   │                                   │
│  └────┬─────┘            └────┬─────┘                                   │
│       │                       │                                          │
│       │ • Players may belong to MULTIPLE teams simultaneously            │
│       │ • Teams can be created by ANY player                            │
│       │ • Creator automatically becomes captain (is_founder=true)       │
│       │ • Captains = team admin role (invite, remove, promote)          │
│       │ • Multiple captains allowed; founders cannot be demoted         │
│       │                                                                  │
│       │              team_members                                        │
│       │         ┌────────────────────┐                                  │
│       │         │ role: captain |    │                                  │
│       │         │       officer |    │                                  │
│       │         │       player |     │                                  │
│       │         │       substitute   │                                  │
│       │         │ is_founder: bool   │                                  │
│       │         └────────────────────┘                                  │
│       │                                                                  │
└───────┼──────────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                     PER-GAME STATISTICS                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────┐    1:N     ┌────────────────────┐                         │
│  │  Player  │───────────►│ player_game_profile │                         │
│  └──────────┘            └─────────┬──────────┘                         │
│                                    │                                     │
│       Each player has a separate profile PER GAME containing:            │
│       • Rating (Glicko-2): rating, rating_deviation, volatility          │
│       • Match stats: wins, losses, playtime                              │
│       • game_specific_stats (JSONB): Plugin-defined structure            │
│                                                                          │
│       Plugin Interface:                                                  │
│       ┌────────────────────────────────────────────────────────────┐    │
│       │ fn player_stats_schema(&self) -> &JsonSchema;              │    │
│       │ fn calculate_player_stats(&self, ...) -> serde_json::Value;│    │
│       │ fn format_player_stats(&self, ...) -> Vec<DisplayStat>;    │    │
│       └────────────────────────────────────────────────────────────┘    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Games, Leagues & Tournaments

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       GAMES & LEAGUES                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────┐    1:N     ┌──────────┐    1:N     ┌──────────────┐       │
│  │   Game   │───────────►│  League  │───────────►│ league_member │       │
│  └──────────┘            └────┬─────┘            └──────────────┘       │
│                               │                                          │
│  • Leagues are GAME-SPECIFIC (game_id required)                         │
│  • Games can have MULTIPLE leagues (e.g., Division 1, Division 2)       │
│  • Leagues have hierarchy: parent_league_id, division, tier_name        │
│  • Regional divisions supported: region field                            │
│                                                                          │
│  League Access Control:                                                  │
│  ┌────────────────────────────────────────────────────────────────┐     │
│  │ access_type:                                                    │     │
│  │   • 'open' - Anyone can join                                   │     │
│  │   • 'invite_only' - Only invited players can join              │     │
│  │   • 'application' - Players apply, admins approve              │     │
│  └────────────────────────────────────────────────────────────────┘     │
│                                                                          │
│  League Membership:                                                      │
│  ┌────────────────────────────────────────────────────────────────┐     │
│  │ • Players can belong to ZERO OR MORE leagues                    │     │
│  │ • Membership tracked in league_members table                    │     │
│  │ • membership_type: player | admin | moderator                   │     │
│  │ • League admins have full management permissions                │     │
│  └────────────────────────────────────────────────────────────────┘     │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                         TOURNAMENTS                                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Tournament Types:                                                       │
│  ┌────────────────────────────────────────────────────────────────┐     │
│  │ GLOBAL TOURNAMENTS (league_id = NULL)                          │     │
│  │   • Created by: Platform admins                                │     │
│  │   • Open to: All players                                       │     │
│  │   • API: POST /v1/tournaments                                  │     │
│  └────────────────────────────────────────────────────────────────┘     │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────┐     │
│  │ LEAGUE TOURNAMENTS (league_id NOT NULL)                        │     │
│  │   • Created by: League admins                                  │     │
│  │   • Open to: League members only                               │     │
│  │   • API: POST /v1/leagues/{league_id}/tournaments              │     │
│  └────────────────────────────────────────────────────────────────┘     │
│                                                                          │
│  Concurrency:                                                            │
│  • MULTIPLE tournaments can run concurrently                            │
│  • No limit on concurrent tournaments (global or per-league)            │
│                                                                          │
│  Map Pool Selection:                                                     │
│  ┌────────────────────────────────────────────────────────────────┐     │
│  │ 1. Game plugin defines: available_maps(), default_map_pool()   │     │
│  │ 2. Plugin declares: supports_custom_map_pool() -> bool         │     │
│  │ 3. If supported, tournament creator can set custom map_pool    │     │
│  │ 4. Falls back to: tournament.map_pool → league.default_map_pool│     │
│  │                    → game.default_map_pool                      │     │
│  └────────────────────────────────────────────────────────────────┘     │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Entity Relationship Summary

| Relationship | Cardinality | Description |
|--------------|-------------|-------------|
| Player ↔ Team | M:N | Players can be on multiple teams |
| Player → PlayerGameProfile | 1:N | One profile per game |
| Game → League | 1:N | Leagues are game-specific |
| League → LeagueMember | 1:N | Players join leagues explicitly |
| Player ↔ League | M:N | Via league_members |
| League → Tournament | 1:N | League-specific tournaments |
| Game → Tournament | 1:N | Global tournaments (no league) |
| Tournament → Participant | 1:N | Registration per tournament |

### Permission Hierarchy for Key Actions

| Action | Required Permission |
|--------|---------------------|
| Create team | Any authenticated player |
| Manage team (invite, remove, settings) | Team captain (`role = 'captain'`) |
| Promote to captain | Team captain (founder can't be demoted) |
| Disband team | Team founder or platform admin |
| Create global tournament | Platform admin |
| Create league tournament | League admin (`membership_type = 'admin'`) |
| Set tournament map pool | Tournament creator (if `supports_custom_map_pool()`) |
| Join league (open) | Any authenticated player |
| Join league (invite_only) | Must have invitation |
| Join league (application) | Submit application, admin approval |

---

## 2. System Architecture Overview

### 2.1 High-Level Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Load Balancer                                   │
│                    (nginx/HAProxy with WebSocket support)                    │
└─────────────────────────────────┬───────────────────────────────────────────┘
                                  │
         ┌────────────────────────┼────────────────────────────┐
         │                        │                            │
         ▼                        ▼                            ▼
┌─────────────────┐    ┌─────────────────┐          ┌─────────────────┐
│   API Gateway   │    │   API Gateway   │   ...    │   API Gateway   │
│   Instance 1    │    │   Instance 2    │          │   Instance N    │
└────────┬────────┘    └────────┬────────┘          └────────┬────────┘
         │                      │                            │
         └──────────────────────┼────────────────────────────┘
                                │
    ┌───────────────────────────┼───────────────────────────────┐
    │                    Service Mesh / Internal Network         │
    │  ┌─────────────────────────────────────────────────────┐  │
    │  │                    Core Services                     │  │
    │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  │  │
    │  │  │   Auth   │ │   RBAC   │ │  Player  │ │  Team  │  │  │
    │  │  │ Service  │ │ Service  │ │ Service  │ │Service │  │  │
    │  │  └──────────┘ └──────────┘ └──────────┘ └────────┘  │  │
    │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  │  │
    │  │  │Matchmake │ │Tournament│ │  Lobby   │ │ Admin  │  │  │
    │  │  │ Service  │ │  Engine  │ │ Service  │ │Service │  │  │
    │  │  └──────────┘ └──────────┘ └──────────┘ └────────┘  │  │
    │  └─────────────────────────────────────────────────────┘  │
    │                           │                                │
    │  ┌─────────────────────────────────────────────────────┐  │
    │  │                  Plugin Manager                      │  │
    │  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐    │  │
    │  │  │ AoE Plugin  │ │ CS2 Plugin  │ │ Game N ...  │    │  │
    │  │  └─────────────┘ └─────────────┘ └─────────────┘    │  │
    │  └─────────────────────────────────────────────────────┘  │
    └───────────────────────────┬───────────────────────────────┘
                                │
         ┌──────────────────────┼──────────────────────┐
         │                      │                      │
         ▼                      ▼                      ▼
┌─────────────────┐  ┌─────────────────┐    ┌─────────────────┐
│   PostgreSQL    │  │     Redis       │    │  Object Store   │
│   (Primary)     │  │  (Cache/PubSub) │    │    (S3/Minio)   │
└─────────────────┘  └─────────────────┘    └─────────────────┘
```

### 2.2 Major Components and Services

#### Core Platform Services

| Service | Responsibility | Stateful? |
|---------|---------------|-----------|
| **API Gateway** | Request routing, auth verification, rate limiting | No |
| **Auth Service** | User authentication, token issuance/refresh, OAuth flows | No |
| **RBAC Service** | Permission evaluation, role management | No (cached) |
| **Player Service** | Player profiles, statistics, match history | No |
| **Team Service** | Team management, rosters, invitations | No |
| **Matchmaking Service** | Queue management, match creation | Partially (queues in Redis) |
| **Tournament Engine** | Leagues, seasons, brackets, scheduling | No |
| **Lobby Service** | WebSocket management, game sessions | Yes (connection state) |
| **Admin Service** | Configuration, moderation, analytics | No |
| **Plugin Manager** | Plugin lifecycle, routing, validation | No |

#### Supporting Infrastructure

| Component | Purpose | Technology |
|-----------|---------|------------|
| **Message Broker** | Inter-service events, lobby broadcasts | Redis Pub/Sub or NATS |
| **Cache Layer** | Session cache, permission cache, hot data | Redis |
| **Object Storage** | User uploads, replay files, static assets | S3-compatible |
| **Search Index** | Player/team search (optional) | Meilisearch or PostgreSQL FTS |

### 2.3 Core vs Plugin Module Boundaries

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CORE PLATFORM                                │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  • User/Team identity and authentication                       │  │
│  │  • Generic match records and outcomes                          │  │
│  │  • Tournament structure (brackets, scheduling)                 │  │
│  │  • Lobby lifecycle (create, join, start, end)                 │  │
│  │  • Permission framework and enforcement                        │  │
│  │  • Plugin registration and dispatch                            │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                       │
│                     Plugin Interface                                 │
│                              │                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    PLUGIN DOMAIN                               │  │
│  │  • Game-specific matchmaking criteria                         │  │
│  │  • Ranking algorithms (Elo, Glicko-2, TrueSkill, custom)      │  │
│  │  • Match metadata schemas                                      │  │
│  │  • Lobby state machines (pick/ban, map veto, ready checks)    │  │
│  │  • Game-specific validation rules                              │  │
│  │  • Statistics aggregation logic                                │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.4 API Routing and Middleware Layers

```
Request Flow:
─────────────────────────────────────────────────────────────────────────

                    HTTP Request / WebSocket Upgrade
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Tower Middleware Stack                          │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │ 1. Request ID (tower-request-id)                                ││
│  │    └─ Assigns unique trace ID to each request                   ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 2. Tracing Layer (tower-http::trace)                            ││
│  │    └─ Structured logging with span context                      ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 3. Rate Limiting (governor / tower-governor)                    ││
│  │    └─ Per-IP and per-user rate limiting                         ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 4. CORS (tower-http::cors)                                      ││
│  │    └─ Cross-origin request handling                             ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 5. Authentication Extractor                                     ││
│  │    └─ JWT validation, user context injection                    ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 6. RBAC Middleware (custom)                                     ││
│  │    └─ Permission evaluation per route                           ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 7. Input Validation (validator + custom)                        ││
│  │    └─ Request body/query validation                             ││
│  ├─────────────────────────────────────────────────────────────────┤│
│  │ 8. Audit Logging                                                ││
│  │    └─ Security-relevant action logging                          ││
│  └─────────────────────────────────────────────────────────────────┘│
│                              │                                       │
│                              ▼                                       │
│                      Axum Router                                     │
│         ┌────────────┬────────────┬────────────┐                    │
│         │  /api/v1   │  /ws/v1    │  /admin    │                    │
│         │  REST API  │ WebSocket  │  Admin API │                    │
│         └────────────┴────────────┴────────────┘                    │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.5 High-Level Data and Control Flow

#### REST API Flow

```
Client Request
      │
      ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Validate   │───▶│   Authorize  │───▶│   Execute    │
│   Request    │    │   (RBAC)     │    │   Handler    │
└──────────────┘    └──────────────┘    └──────┬───────┘
                                               │
                    ┌──────────────────────────┼──────────────────┐
                    │                          │                  │
                    ▼                          ▼                  ▼
             ┌──────────────┐          ┌──────────────┐   ┌──────────────┐
             │   Database   │          │    Cache     │   │   Plugin     │
             │   (SQLx)     │          │   (Redis)    │   │   Dispatch   │
             └──────────────┘          └──────────────┘   └──────────────┘
                    │                          │                  │
                    └──────────────────────────┼──────────────────┘
                                               │
                                               ▼
                                        ┌──────────────┐
                                        │   Response   │
                                        │  Serializer  │
                                        └──────────────┘
```

#### WebSocket Lobby Flow

```
Client Connect
      │
      ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Upgrade    │───▶│   Auth WS    │───▶│  Join Lobby  │
│   Handshake  │    │   Session    │    │   Actor      │
└──────────────┘    └──────────────┘    └──────┬───────┘
                                               │
                                               ▼
                                     ┌─────────────────────┐
                                     │   Lobby Actor       │
                                     │   ┌─────────────┐   │
                                     │   │ Core State  │   │
                                     │   │ Management  │   │
                                     │   └──────┬──────┘   │
                                     │          │          │
                                     │   ┌──────▼──────┐   │
                                     │   │  Plugin     │   │
                                     │   │  State      │   │
                                     │   │  Machine    │   │
                                     │   └─────────────┘   │
                                     └─────────────────────┘
                                               │
                    ┌──────────────────────────┼──────────────────┐
                    │                          │                  │
                    ▼                          ▼                  ▼
             ┌──────────────┐          ┌──────────────┐   ┌──────────────┐
             │  Broadcast   │          │    Persist   │   │   Notify     │
             │  to Clients  │          │    State     │   │   Services   │
             └──────────────┘          └──────────────┘   └──────────────┘
```

### 2.6 Best-Practice Rust Patterns

#### Async Architecture

```rust
// Example: Service trait pattern with async
#[async_trait]
pub trait MatchmakingService: Send + Sync {
    async fn enqueue_player(
        &self,
        game_id: GameId,
        player: PlayerId,
        preferences: MatchPreferences,
    ) -> Result<QueueTicket, MatchmakingError>;
    
    async fn dequeue_player(
        &self,
        ticket: QueueTicket,
    ) -> Result<(), MatchmakingError>;
    
    async fn get_queue_status(
        &self,
        ticket: QueueTicket,
    ) -> Result<QueueStatus, MatchmakingError>;
}
```

#### Error Handling Strategy

```rust
// Layered error types using thiserror
use thiserror::Error;

// Domain-level errors
#[derive(Error, Debug)]
pub enum MatchmakingError {
    #[error("Player {0} is already in queue")]
    AlreadyQueued(PlayerId),
    
    #[error("Queue {0} is full")]
    QueueFull(QueueId),
    
    #[error("Invalid game configuration: {0}")]
    InvalidConfig(String),
    
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    
    #[error(transparent)]
    Plugin(#[from] PluginError),
}

// Infrastructure-level errors
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Entity not found: {entity_type} with id {id}")]
    NotFound { entity_type: &'static str, id: String },
    
    #[error("Conflict: {0}")]
    Conflict(String),
}

// API response error type
#[derive(Serialize)]
pub struct ApiError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub title: String,
    pub status: u16,
    pub detail: Option<String>,
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
}
```

#### Module Organization

```
src/
├── main.rs                    # Application entry point
├── lib.rs                     # Library root (for testing)
├── config/                    # Configuration management
│   ├── mod.rs
│   └── settings.rs
├── api/                       # HTTP/WS handlers
│   ├── mod.rs
│   ├── routes.rs              # Route definitions
│   ├── extractors/            # Custom Axum extractors
│   ├── handlers/              # Request handlers by domain
│   │   ├── auth.rs
│   │   ├── players.rs
│   │   └── ...
│   └── ws/                    # WebSocket handlers
│       ├── mod.rs
│       └── lobby.rs
├── domain/                    # Business logic
│   ├── mod.rs
│   ├── auth/
│   ├── matchmaking/
│   ├── tournaments/
│   └── lobbies/
├── infrastructure/            # External integrations
│   ├── mod.rs
│   ├── db/                    # Database repositories
│   ├── cache/                 # Redis integration
│   └── messaging/             # Event bus
├── plugins/                   # Plugin system
│   ├── mod.rs
│   ├── traits.rs              # Plugin interfaces
│   ├── manager.rs             # Plugin registry
│   └── games/                 # Built-in game plugins
│       ├── aoe/
│       └── cs2/
├── middleware/                # Tower middleware
│   ├── mod.rs
│   ├── auth.rs
│   ├── rbac.rs
│   └── audit.rs
└── shared/                    # Shared types and utilities
    ├── mod.rs
    ├── types.rs               # Common type definitions
    └── utils.rs
```

---

## 3. Cascading Operations & Saga Pattern

Complex business operations in the gaming platform often require coordinated updates across multiple entities and services. For example, disbanding a team must update team state, remove tournament registrations, notify affected leagues, and clean up pending invitations. This section describes the architectural approach for handling such cascading updates reliably.

### 3.1 Problem Statement

Many operations have cross-cutting effects:

| Operation | Cascade Effects |
|-----------|-----------------|
| **Disband Team** | Update team status → Remove from tournaments → Cancel pending matches → Notify league admins → Revoke member roles → Archive match history |
| **Ban Player** | Suspend account → Remove from active lobbies → Forfeit ongoing matches → Remove from queues → Notify team owners → Update tournament brackets |
| **Cancel Tournament** | Update status → Refund entry fees → Notify all participants → Release scheduled match slots → Update league standings |
| **Player Leaves Team** | Update roster → Check minimum roster requirements → Potentially forfeit tournament slots → Update captain if needed |
| **Season Ends** | Finalize standings → Calculate rewards → Archive season data → Trigger playoff generation → Update player statistics |

### 3.2 Saga Pattern Architecture

We implement the **Saga Pattern** for distributed transactions, using an orchestration-based approach for better visibility and control.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SAGA ORCHESTRATOR                                  │
└─────────────────────────────────────────────────────────────────────────────┘

                              ┌─────────────────┐
                              │  Saga Executor  │
                              │  (Coordinator)  │
                              └────────┬────────┘
                                       │
         ┌─────────────────────────────┼─────────────────────────────┐
         │                             │                             │
         ▼                             ▼                             ▼
┌─────────────────┐          ┌─────────────────┐          ┌─────────────────┐
│   Step 1        │          │   Step 2        │          │   Step 3        │
│   Execute ──────┼─Success──│   Execute ──────┼─Success──│   Execute       │
│   Compensate ◄──┼─Failure──│   Compensate ◄──┼─Failure──│   Compensate    │
└─────────────────┘          └─────────────────┘          └─────────────────┘

Flow:
1. Execute steps sequentially (or with defined parallelism)
2. On failure, execute compensation actions in reverse order
3. Record saga state for recovery after crashes
```

### 3.3 Saga Definition Framework

```rust
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

/// Unique identifier for a saga instance
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SagaId(pub Uuid);

/// Represents a single step in a saga
#[async_trait]
pub trait SagaStep: Send + Sync {
    /// Human-readable name for logging/debugging
    fn name(&self) -> &'static str;
    
    /// Execute the forward action
    async fn execute(&self, ctx: &mut SagaContext) -> Result<StepOutput, SagaError>;
    
    /// Compensate/rollback this step
    async fn compensate(&self, ctx: &mut SagaContext) -> Result<(), SagaError>;
    
    /// Whether this step can be retried on transient failure
    fn is_retryable(&self) -> bool { true }
    
    /// Maximum retry attempts
    fn max_retries(&self) -> u32 { 3 }
}

/// Context passed through saga execution
#[derive(Debug, Serialize, Deserialize)]
pub struct SagaContext {
    pub saga_id: SagaId,
    pub initiated_by: UserId,
    pub initiated_at: DateTime<Utc>,
    pub input: serde_json::Value,
    pub step_outputs: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, String>,
}

/// Saga definition combining multiple steps
pub struct SagaDefinition {
    pub name: &'static str,
    pub steps: Vec<Box<dyn SagaStep>>,
    pub timeout: Duration,
    pub on_complete: Option<Box<dyn SagaCallback>>,
    pub on_failure: Option<Box<dyn SagaCallback>>,
}

/// Persisted saga state for crash recovery
#[derive(Debug, Serialize, Deserialize)]
pub struct SagaState {
    pub saga_id: SagaId,
    pub saga_type: String,
    pub status: SagaStatus,
    pub current_step: usize,
    pub context: SagaContext,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SagaStatus {
    Pending,
    Running,
    Compensating,
    Completed,
    Failed,
    CompensationFailed,
}
```

### 3.4 Saga Executor Implementation

```rust
pub struct SagaExecutor {
    saga_repository: Arc<dyn SagaRepository>,
    event_bus: Arc<dyn EventBus>,
    metrics: Arc<SagaMetrics>,
}

impl SagaExecutor {
    /// Execute a saga with full transaction coordination
    pub async fn execute(
        &self,
        definition: &SagaDefinition,
        input: serde_json::Value,
        initiated_by: UserId,
    ) -> Result<SagaResult, SagaError> {
        let saga_id = SagaId(Uuid::new_v4());
        
        // Initialize saga state
        let mut context = SagaContext {
            saga_id: saga_id.clone(),
            initiated_by,
            initiated_at: Utc::now(),
            input,
            step_outputs: HashMap::new(),
            metadata: HashMap::new(),
        };
        
        let mut state = SagaState {
            saga_id: saga_id.clone(),
            saga_type: definition.name.to_string(),
            status: SagaStatus::Running,
            current_step: 0,
            context: context.clone(),
            started_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            error: None,
        };
        
        // Persist initial state
        self.saga_repository.save(&state).await?;
        
        // Execute steps with timeout
        let result = tokio::time::timeout(
            definition.timeout,
            self.execute_steps(definition, &mut context, &mut state),
        ).await;
        
        match result {
            Ok(Ok(())) => {
                state.status = SagaStatus::Completed;
                state.completed_at = Some(Utc::now());
                self.saga_repository.save(&state).await?;
                
                if let Some(callback) = &definition.on_complete {
                    callback.invoke(&context).await;
                }
                
                self.event_bus.publish(SagaCompletedEvent {
                    saga_id,
                    saga_type: definition.name.to_string(),
                }).await;
                
                Ok(SagaResult::Completed(context))
            }
            Ok(Err(e)) => {
                // Execute compensation
                self.compensate(definition, &mut context, &mut state).await?;
                
                if let Some(callback) = &definition.on_failure {
                    callback.invoke(&context).await;
                }
                
                Err(e)
            }
            Err(_) => {
                // Timeout - trigger compensation
                state.error = Some("Saga timed out".to_string());
                self.compensate(definition, &mut context, &mut state).await?;
                Err(SagaError::Timeout)
            }
        }
    }
    
    async fn execute_steps(
        &self,
        definition: &SagaDefinition,
        context: &mut SagaContext,
        state: &mut SagaState,
    ) -> Result<(), SagaError> {
        for (idx, step) in definition.steps.iter().enumerate() {
            state.current_step = idx;
            state.updated_at = Utc::now();
            self.saga_repository.save(state).await?;
            
            tracing::info!(
                saga_id = %state.saga_id.0,
                step = step.name(),
                "Executing saga step"
            );
            
            let result = self.execute_with_retry(step.as_ref(), context).await;
            
            match result {
                Ok(output) => {
                    context.step_outputs.insert(
                        step.name().to_string(),
                        output.data,
                    );
                }
                Err(e) => {
                    state.error = Some(e.to_string());
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }
    
    async fn compensate(
        &self,
        definition: &SagaDefinition,
        context: &mut SagaContext,
        state: &mut SagaState,
    ) -> Result<(), SagaError> {
        state.status = SagaStatus::Compensating;
        self.saga_repository.save(state).await?;
        
        // Compensate in reverse order, starting from current step
        for idx in (0..=state.current_step).rev() {
            let step = &definition.steps[idx];
            
            tracing::info!(
                saga_id = %state.saga_id.0,
                step = step.name(),
                "Compensating saga step"
            );
            
            if let Err(e) = step.compensate(context).await {
                tracing::error!(
                    saga_id = %state.saga_id.0,
                    step = step.name(),
                    error = %e,
                    "Compensation failed"
                );
                state.status = SagaStatus::CompensationFailed;
                self.saga_repository.save(state).await?;
                
                // Alert for manual intervention
                self.event_bus.publish(CompensationFailedEvent {
                    saga_id: state.saga_id.clone(),
                    step: step.name().to_string(),
                    error: e.to_string(),
                }).await;
                
                return Err(SagaError::CompensationFailed(e.to_string()));
            }
        }
        
        state.status = SagaStatus::Failed;
        state.completed_at = Some(Utc::now());
        self.saga_repository.save(state).await?;
        
        Ok(())
    }
    
    async fn execute_with_retry(
        &self,
        step: &dyn SagaStep,
        context: &mut SagaContext,
    ) -> Result<StepOutput, SagaError> {
        let mut attempts = 0;
        let max_retries = if step.is_retryable() { step.max_retries() } else { 1 };
        
        loop {
            match step.execute(context).await {
                Ok(output) => return Ok(output),
                Err(e) if e.is_transient() && attempts < max_retries => {
                    attempts += 1;
                    let backoff = Duration::from_millis(100 * 2_u64.pow(attempts));
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

### 3.5 Example: Disband Team Saga

```rust
/// Complete saga for disbanding a team
pub fn disband_team_saga() -> SagaDefinition {
    SagaDefinition {
        name: "disband_team",
        steps: vec![
            Box::new(ValidateTeamCanDisband),
            Box::new(RemoveFromActiveTournaments),
            Box::new(CancelPendingMatches),
            Box::new(RevokeTeamMemberRoles),
            Box::new(CancelPendingInvitations),
            Box::new(NotifyLeagueAdmins),
            Box::new(UpdateTeamStatus),
            Box::new(ArchiveTeamData),
        ],
        timeout: Duration::from_secs(60),
        on_complete: Some(Box::new(SendDisbandNotifications)),
        on_failure: None,
    }
}

// Step 1: Validate team can be disbanded
struct ValidateTeamCanDisband;

#[async_trait]
impl SagaStep for ValidateTeamCanDisband {
    fn name(&self) -> &'static str { "validate_team_can_disband" }
    
    async fn execute(&self, ctx: &mut SagaContext) -> Result<StepOutput, SagaError> {
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .ok_or(SagaError::InvalidInput("team_id required"))?;
        
        let team_service = ctx.get_service::<TeamService>()?;
        let team = team_service.get_team(team_id).await
            .map_err(|e| SagaError::StepFailed(e.to_string()))?;
        
        // Check no active matches in progress
        let match_service = ctx.get_service::<MatchService>()?;
        let active_matches = match_service
            .get_active_matches_for_team(team_id)
            .await?;
        
        if !active_matches.is_empty() {
            return Err(SagaError::ValidationFailed(
                "Cannot disband team with active matches".into()
            ));
        }
        
        ctx.metadata.insert("team_name".into(), team.name.clone());
        
        Ok(StepOutput {
            data: json!({ "team": team }),
        })
    }
    
    async fn compensate(&self, _ctx: &mut SagaContext) -> Result<(), SagaError> {
        // Validation step has no side effects to compensate
        Ok(())
    }
}

// Step 2: Remove from active tournaments
struct RemoveFromActiveTournaments;

#[async_trait]
impl SagaStep for RemoveFromActiveTournaments {
    fn name(&self) -> &'static str { "remove_from_tournaments" }
    
    async fn execute(&self, ctx: &mut SagaContext) -> Result<StepOutput, SagaError> {
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .unwrap();
        
        let tournament_service = ctx.get_service::<TournamentService>()?;
        
        // Get all active tournament registrations
        let registrations = tournament_service
            .get_team_tournament_registrations(team_id, TournamentStatus::Active)
            .await?;
        
        // Store for potential compensation
        let registration_ids: Vec<_> = registrations.iter()
            .map(|r| r.id)
            .collect();
        
        // Withdraw from each tournament
        for reg in &registrations {
            tournament_service
                .withdraw_participant(reg.tournament_id, team_id.into())
                .await?;
        }
        
        Ok(StepOutput {
            data: json!({
                "withdrawn_registrations": registration_ids,
                "tournaments_affected": registrations.len(),
            }),
        })
    }
    
    async fn compensate(&self, ctx: &mut SagaContext) -> Result<(), SagaError> {
        // Re-register team in tournaments
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .unwrap();
        
        if let Some(output) = ctx.step_outputs.get("remove_from_tournaments") {
            let registration_ids: Vec<Uuid> = serde_json::from_value(
                output["withdrawn_registrations"].clone()
            )?;
            
            let tournament_service = ctx.get_service::<TournamentService>()?;
            
            for reg_id in registration_ids {
                // Restore registration (implementation would need to support this)
                tournament_service.restore_registration(reg_id).await?;
            }
        }
        
        Ok(())
    }
}

// Step 3: Cancel pending matches
struct CancelPendingMatches;

#[async_trait]
impl SagaStep for CancelPendingMatches {
    fn name(&self) -> &'static str { "cancel_pending_matches" }
    
    async fn execute(&self, ctx: &mut SagaContext) -> Result<StepOutput, SagaError> {
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .unwrap();
        
        let match_service = ctx.get_service::<MatchService>()?;
        
        let pending_matches = match_service
            .get_pending_matches_for_team(team_id)
            .await?;
        
        let mut cancelled = Vec::new();
        for m in &pending_matches {
            match_service.cancel_match(m.id, CancelReason::TeamDisbanded).await?;
            cancelled.push(m.id);
        }
        
        Ok(StepOutput {
            data: json!({
                "cancelled_matches": cancelled,
            }),
        })
    }
    
    async fn compensate(&self, ctx: &mut SagaContext) -> Result<(), SagaError> {
        // Restore cancelled matches to pending state
        if let Some(output) = ctx.step_outputs.get("cancel_pending_matches") {
            let cancelled: Vec<Uuid> = serde_json::from_value(
                output["cancelled_matches"].clone()
            )?;
            
            let match_service = ctx.get_service::<MatchService>()?;
            for match_id in cancelled {
                match_service.restore_match(match_id.into()).await?;
            }
        }
        Ok(())
    }
}

// Step 7: Update team status (the actual state change)
struct UpdateTeamStatus;

#[async_trait]
impl SagaStep for UpdateTeamStatus {
    fn name(&self) -> &'static str { "update_team_status" }
    
    async fn execute(&self, ctx: &mut SagaContext) -> Result<StepOutput, SagaError> {
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .unwrap();
        
        let team_service = ctx.get_service::<TeamService>()?;
        
        // Get current status for compensation
        let team = team_service.get_team(team_id).await?;
        let previous_status = team.status.clone();
        
        // Update to disbanded
        team_service.update_team_status(team_id, TeamStatus::Disbanded).await?;
        
        Ok(StepOutput {
            data: json!({
                "previous_status": previous_status,
            }),
        })
    }
    
    async fn compensate(&self, ctx: &mut SagaContext) -> Result<(), SagaError> {
        let team_id: TeamId = ctx.input["team_id"].as_str()
            .and_then(|s| s.parse().ok())
            .unwrap();
        
        if let Some(output) = ctx.step_outputs.get("update_team_status") {
            let previous_status: TeamStatus = serde_json::from_value(
                output["previous_status"].clone()
            )?;
            
            let team_service = ctx.get_service::<TeamService>()?;
            team_service.update_team_status(team_id, previous_status).await?;
        }
        Ok(())
    }
}
```

### 3.6 Saga Recovery After Crashes

```rust
/// Background worker to recover incomplete sagas after server restart
pub struct SagaRecoveryWorker {
    saga_repository: Arc<dyn SagaRepository>,
    saga_executor: Arc<SagaExecutor>,
    saga_definitions: HashMap<String, SagaDefinition>,
}

impl SagaRecoveryWorker {
    pub async fn run(&self) {
        // On startup, find sagas that were in progress
        let incomplete_sagas = self.saga_repository
            .find_by_status(vec![SagaStatus::Running, SagaStatus::Compensating])
            .await
            .unwrap_or_default();
        
        for saga_state in incomplete_sagas {
            tracing::warn!(
                saga_id = %saga_state.saga_id.0,
                saga_type = %saga_state.saga_type,
                status = ?saga_state.status,
                "Recovering incomplete saga"
            );
            
            if let Some(definition) = self.saga_definitions.get(&saga_state.saga_type) {
                match saga_state.status {
                    SagaStatus::Running => {
                        // Resume compensation from the failed step
                        self.saga_executor
                            .resume_compensation(definition, saga_state)
                            .await;
                    }
                    SagaStatus::Compensating => {
                        // Continue compensation
                        self.saga_executor
                            .continue_compensation(definition, saga_state)
                            .await;
                    }
                    _ => {}
                }
            }
        }
    }
}
```

### 3.7 Saga State Persistence Schema

```sql
-- Saga execution tracking
CREATE TABLE sagas (
    id UUID PRIMARY KEY,
    saga_type VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL,
    current_step INTEGER NOT NULL DEFAULT 0,
    context JSONB NOT NULL,
    initiated_by UUID REFERENCES users(id),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error TEXT,
    
    -- Index for recovery queries
    CONSTRAINT valid_status CHECK (status IN (
        'pending', 'running', 'compensating', 
        'completed', 'failed', 'compensation_failed'
    ))
);

CREATE INDEX idx_sagas_status ON sagas(status) WHERE status IN ('running', 'compensating');
CREATE INDEX idx_sagas_type_status ON sagas(saga_type, status);

-- Saga step execution log (for debugging/auditing)
CREATE TABLE saga_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_id UUID REFERENCES sagas(id) ON DELETE CASCADE,
    step_name VARCHAR(64) NOT NULL,
    step_index INTEGER NOT NULL,
    action VARCHAR(16) NOT NULL, -- 'execute' or 'compensate'
    status VARCHAR(16) NOT NULL, -- 'success', 'failed', 'skipped'
    input JSONB,
    output JSONB,
    error TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_ms INTEGER
);

CREATE INDEX idx_saga_steps_saga ON saga_steps(saga_id);
```

### 3.8 Common Saga Definitions

```rust
// Registry of all saga definitions
pub fn register_sagas() -> HashMap<String, SagaDefinition> {
    let mut sagas = HashMap::new();
    
    sagas.insert("disband_team".into(), disband_team_saga());
    sagas.insert("ban_player".into(), ban_player_saga());
    sagas.insert("cancel_tournament".into(), cancel_tournament_saga());
    sagas.insert("end_season".into(), end_season_saga());
    sagas.insert("player_leave_team".into(), player_leave_team_saga());
    sagas.insert("merge_teams".into(), merge_teams_saga());
    sagas.insert("transfer_team_ownership".into(), transfer_ownership_saga());
    
    sagas
}

// Ban player saga
pub fn ban_player_saga() -> SagaDefinition {
    SagaDefinition {
        name: "ban_player",
        steps: vec![
            Box::new(ValidateBanRequest),
            Box::new(RemoveFromActiveLobbies),
            Box::new(RemoveFromMatchmakingQueues),
            Box::new(ForfeitActiveMatches),
            Box::new(NotifyAffectedTeams),
            Box::new(UpdateTournamentBrackets),
            Box::new(CreateBanRecord),
            Box::new(SuspendUserAccount),
        ],
        timeout: Duration::from_secs(120),
        on_complete: Some(Box::new(SendBanNotifications)),
        on_failure: None,
    }
}

// End season saga
pub fn end_season_saga() -> SagaDefinition {
    SagaDefinition {
        name: "end_season",
        steps: vec![
            Box::new(ValidateSeasonCanEnd),
            Box::new(FinalizeAllMatches),
            Box::new(CalculateFinalStandings),
            Box::new(DistributeRewards),
            Box::new(UpdatePlayerStatistics),
            Box::new(GeneratePlayoffBracket),
            Box::new(ArchiveSeasonData),
            Box::new(UpdateSeasonStatus),
        ],
        timeout: Duration::from_secs(300),
        on_complete: Some(Box::new(SendSeasonEndNotifications)),
        on_failure: None,
    }
}
```

### 3.9 API Integration

```rust
// Exposing saga operations via REST API
pub fn saga_routes() -> Router<AppState> {
    Router::new()
        // Trigger sagas via API
        .route("/teams/:team_id/disband", post(disband_team_handler))
        .route("/players/:player_id/ban", post(ban_player_handler))
        
        // Monitor saga status
        .route("/sagas/:saga_id", get(get_saga_status))
        .route("/sagas/:saga_id/steps", get(get_saga_steps))
}

async fn disband_team_handler(
    State(state): State<AppState>,
    Path(team_id): Path<TeamId>,
    auth: AuthenticatedUser,
) -> Result<Json<SagaStarted>, ApiError> {
    // Verify permission
    state.rbac_service
        .check_permission(auth.user_id, Permission::TeamDelete, Some(team_id.into()))
        .await?;
    
    // Start saga
    let saga_id = state.saga_executor
        .execute(
            &disband_team_saga(),
            json!({ "team_id": team_id.to_string() }),
            auth.user_id,
        )
        .await?;
    
    Ok(Json(SagaStarted {
        saga_id,
        status_url: format!("/api/v1/sagas/{}", saga_id.0),
    }))
}
```

---

## 4. Middleware Requirements

### 4.1 Middleware Stack Overview

| Layer | Crate | Purpose | Build vs Reuse |
|-------|-------|---------|----------------|
| Request ID | `tower-request-id` | Trace correlation | Reuse |
| Tracing | `tower-http::trace` | Request/response logging | Reuse |
| Rate Limiting | `tower_governor` | DDoS protection | Reuse |
| CORS | `tower-http::cors` | Cross-origin handling | Reuse |
| Compression | `tower-http::compression` | Response compression | Reuse |
| Auth Verification | Custom | JWT validation | Build |
| RBAC | Custom | Permission enforcement | Build |
| Input Validation | `validator` + Custom | Request validation | Hybrid |
| Audit Logging | Custom | Security audit trail | Build |

### 4.2 RBAC Enforcement Middleware

```rust
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

pub struct RbacMiddleware {
    rbac_service: Arc<dyn RbacService>,
}

impl RbacMiddleware {
    pub async fn check_permission(
        State(state): State<AppState>,
        auth: AuthenticatedUser,
        request: Request,
        next: Next,
    ) -> Result<Response, ApiError> {
        // Extract required permission from route metadata
        let required_permission = request
            .extensions()
            .get::<RequiredPermission>()
            .cloned();
        
        if let Some(permission) = required_permission {
            // Extract resource context (e.g., team_id, tournament_id)
            let resource_context = extract_resource_context(&request);
            
            // Evaluate permission
            let allowed = state.rbac_service
                .check_permission(
                    auth.user_id,
                    permission,
                    resource_context,
                )
                .await?;
            
            if !allowed {
                return Err(ApiError::forbidden("Insufficient permissions"));
            }
        }
        
        Ok(next.run(request).await)
    }
}

// Route definition with permission metadata
pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/teams/:team_id", delete(delete_team))
        .route_layer(RequirePermission::new(Permission::TeamManage))
}
```

### 4.3 Audit Logging

```rust
// Audit event structure
#[derive(Serialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub request_id: String,
    pub user_id: Option<UserId>,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub ip_address: IpAddr,
    pub user_agent: Option<String>,
    pub outcome: AuditOutcome,
    pub metadata: serde_json::Value,
}

#[derive(Serialize)]
pub enum AuditAction {
    // Auth events
    Login, Logout, TokenRefresh, PasswordChange,
    // Resource events
    Create, Read, Update, Delete,
    // Admin events
    ConfigChange, RoleAssignment, BanUser,
    // Game events
    MatchStart, MatchEnd, TournamentCreate,
}

// Middleware integration
pub async fn audit_middleware(
    State(state): State<AppState>,
    auth: Option<AuthenticatedUser>,
    request: Request,
    next: Next,
) -> Response {
    let request_id = request.extensions()
        .get::<RequestId>()
        .map(|id| id.to_string());
    
    let audit_context = AuditContext::from_request(&request, &auth);
    
    let response = next.run(request).await;
    
    // Async emit audit event (non-blocking)
    if should_audit(&audit_context, &response) {
        let event = build_audit_event(audit_context, &response);
        state.audit_emitter.emit(event).await;
    }
    
    response
}
```

### 4.4 Input Validation

```rust
use validator::Validate;
use axum::{Json, extract::rejection::JsonRejection};

// Request DTO with validation rules
#[derive(Deserialize, Validate)]
pub struct CreateTeamRequest {
    #[validate(length(min = 3, max = 32))]
    pub name: String,
    
    #[validate(length(min = 2, max = 5))]
    pub tag: String,
    
    #[validate(length(max = 500))]
    pub description: Option<String>,
    
    #[validate(url)]
    pub logo_url: Option<String>,
}

// Custom extractor with validation
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        
        value.validate()
            .map_err(|e| ApiError::validation_error(e))?;
        
        Ok(ValidatedJson(value))
    }
}
```

### 4.5 Authentication Token Verification

```rust
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub username: String,
    pub roles: Vec<RoleId>,
    pub token_id: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// Axum extractor for authenticated requests
#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Extract bearer token from Authorization header
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("Missing authorization token"))?;
        
        // Decode and validate JWT
        let token_data = decode::<Claims>(
            auth_header,
            &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|e| match e.kind() {
            ErrorKind::ExpiredSignature => ApiError::unauthorized("Token expired"),
            _ => ApiError::unauthorized("Invalid token"),
        })?;
        
        // Check token revocation (optional, for logout support)
        let app_state = parts.extensions.get::<AppState>().unwrap();
        if app_state.token_blacklist.is_revoked(&token_data.claims.jti).await {
            return Err(ApiError::unauthorized("Token revoked"));
        }
        
        Ok(AuthenticatedUser::from_claims(token_data.claims))
    }
}
```

### 4.6 Recommended Crates Summary

| Component | Crate | Version | Notes |
|-----------|-------|---------|-------|
| HTTP Framework | `axum` | 0.7+ | WebSocket support, Tower ecosystem |
| Middleware | `tower` | 0.4+ | Service composition |
| HTTP Middleware | `tower-http` | 0.5+ | CORS, compression, tracing |
| Rate Limiting | `tower_governor` | 0.3+ | Token bucket algorithm |
| Tracing | `tracing` | 0.1+ | Structured logging |
| Tracing Subscriber | `tracing-subscriber` | 0.3+ | Log output formatting |
| JWT | `jsonwebtoken` | 9+ | JWT encode/decode |
| Validation | `validator` | 0.16+ | Derive-based validation |
| Request ID | `tower-request-id` | 0.3+ | UUID request correlation |

---

## 5. WebSocket Lobby System Design

### 5.1 Core Lobby Service Responsibilities

The Lobby Service manages real-time game sessions from creation through completion:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Lobby Service Core                                │
├─────────────────────────────────────────────────────────────────────────┤
│  • Connection Management                                                 │
│    - WebSocket upgrade handling                                          │
│    - Session authentication and authorization                            │
│    - Heartbeat/keepalive management                                      │
│    - Graceful disconnection and cleanup                                  │
│                                                                          │
│  • Lobby Lifecycle                                                       │
│    - Create lobby with game-specific configuration                       │
│    - Player join/leave handling                                          │
│    - State transitions (waiting → picking → ready → active → complete)  │
│    - Timeout and abandonment handling                                    │
│                                                                          │
│  • Event Distribution                                                    │
│    - Broadcast state changes to all lobby members                        │
│    - Targeted messages to specific players                               │
│    - External service notifications (match service, stats)               │
│                                                                          │
│  • Plugin Coordination                                                   │
│    - Delegate game-specific message handling to plugins                  │
│    - Validate plugin state transitions                                   │
│    - Enforce plugin-defined rules                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Authentication and RBAC within WebSocket Channels

```rust
// WebSocket authentication flow
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<WsConnectParams>,
) -> Result<Response, ApiError> {
    // Validate JWT from query parameter or header
    let auth = validate_ws_token(&params.token, &state.config)?;
    
    // Check lobby access permission
    let lobby_id = params.lobby_id;
    let can_access = state.rbac_service
        .check_permission(
            auth.user_id,
            Permission::LobbyJoin,
            ResourceContext::Lobby(lobby_id),
        )
        .await?;
    
    if !can_access {
        return Err(ApiError::forbidden("Cannot access this lobby"));
    }
    
    // Upgrade connection with authenticated context
    Ok(ws.on_upgrade(move |socket| {
        handle_ws_connection(socket, auth, lobby_id, state)
    }))
}

// Per-message authorization within lobby
async fn handle_lobby_message(
    session: &WsSession,
    message: LobbyMessage,
    lobby: &Lobby,
) -> Result<(), LobbyError> {
    // Check message-level permissions
    match &message {
        LobbyMessage::Kick { target_player } => {
            require_permission(session, Permission::LobbyKick, lobby)?;
        }
        LobbyMessage::SetConfig { .. } => {
            require_permission(session, Permission::LobbyAdmin, lobby)?;
        }
        LobbyMessage::PluginAction(action) => {
            // Delegate to plugin for game-specific permission check
            lobby.plugin.authorize_action(session, action)?;
        }
        _ => {} // Standard player actions
    }
    
    // Process message
    lobby.handle_message(session.player_id, message).await
}
```

### 5.3 Session Lifecycle, Reconnect Logic, Timeouts

```rust
// Session state machine
pub enum SessionState {
    Connecting,
    Authenticated { user_id: UserId },
    InLobby { lobby_id: LobbyId, player_slot: u8 },
    Disconnected { 
        since: Instant,
        lobby_id: Option<LobbyId>,
        reconnect_token: String,
    },
}

// Lobby actor managing sessions
pub struct LobbyActor {
    lobby_id: LobbyId,
    sessions: HashMap<PlayerId, SessionHandle>,
    disconnected: HashMap<PlayerId, DisconnectedSession>,
    state: LobbyState,
    plugin_state: Box<dyn GameLobbyState>,
    config: LobbyConfig,
}

impl LobbyActor {
    // Handle player disconnect
    async fn handle_disconnect(&mut self, player_id: PlayerId) {
        if let Some(session) = self.sessions.remove(&player_id) {
            let reconnect_token = generate_reconnect_token();
            
            self.disconnected.insert(player_id, DisconnectedSession {
                disconnected_at: Instant::now(),
                reconnect_token: reconnect_token.clone(),
                preserved_state: session.player_state.clone(),
            });
            
            // Start reconnect timeout
            self.schedule_reconnect_timeout(player_id, self.config.reconnect_window);
            
            // Notify other players
            self.broadcast(LobbyEvent::PlayerDisconnected {
                player_id,
                reconnect_allowed: true,
            }).await;
        }
    }
    
    // Handle reconnection attempt
    async fn handle_reconnect(
        &mut self,
        player_id: PlayerId,
        reconnect_token: &str,
        new_socket: WebSocket,
    ) -> Result<(), LobbyError> {
        let disconnected = self.disconnected
            .remove(&player_id)
            .ok_or(LobbyError::ReconnectExpired)?;
        
        if disconnected.reconnect_token != reconnect_token {
            return Err(LobbyError::InvalidReconnectToken);
        }
        
        // Restore session
        let session = SessionHandle::new(new_socket, disconnected.preserved_state);
        self.sessions.insert(player_id, session);
        
        // Send current state to reconnected player
        self.send_full_state_sync(player_id).await?;
        
        // Notify others
        self.broadcast(LobbyEvent::PlayerReconnected { player_id }).await;
        
        Ok(())
    }
}

// Timeout configuration
pub struct LobbyConfig {
    pub heartbeat_interval: Duration,      // 15 seconds
    pub heartbeat_timeout: Duration,       // 45 seconds
    pub reconnect_window: Duration,        // 2 minutes
    pub idle_lobby_timeout: Duration,      // 30 minutes
    pub pick_phase_timeout: Duration,      // Game-specific, from plugin
}
```

### 5.4 Game-Agnostic Lobby Event Bus

```rust
// Core lobby events (game-agnostic)
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreLobbyEvent {
    // Connection events
    PlayerJoined { player_id: PlayerId, player_info: PlayerInfo },
    PlayerLeft { player_id: PlayerId, reason: LeaveReason },
    PlayerDisconnected { player_id: PlayerId, reconnect_allowed: bool },
    PlayerReconnected { player_id: PlayerId },
    
    // Lobby state events
    LobbyConfigUpdated { config: LobbyConfig },
    TeamAssignment { player_id: PlayerId, team: TeamSlot },
    ReadyStateChanged { player_id: PlayerId, ready: bool },
    
    // Phase transitions
    PhaseChanged { from: LobbyPhase, to: LobbyPhase },
    CountdownStarted { phase: LobbyPhase, duration_secs: u32 },
    CountdownCancelled { phase: LobbyPhase },
    
    // Match events
    MatchStarting { match_id: MatchId, server_info: Option<ServerInfo> },
    MatchEnded { match_id: MatchId, result: MatchResult },
    
    // Chat
    ChatMessage { sender: PlayerId, message: String },
    SystemMessage { message: String },
}

// Plugin events wrapper
#[derive(Serialize, Deserialize, Clone)]
pub struct PluginLobbyEvent {
    pub game_id: GameId,
    pub event_type: String,
    pub payload: serde_json::Value,
}

// Outbound message envelope
#[derive(Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum LobbyOutboundMessage {
    Core(CoreLobbyEvent),
    Plugin(PluginLobbyEvent),
    Error { code: String, message: String },
    StateSync { full_state: LobbyStateSnapshot },
}

// Event bus for lobby
pub struct LobbyEventBus {
    // Broadcast to all connected clients
    broadcast_tx: broadcast::Sender<LobbyOutboundMessage>,
    
    // External service notifications
    external_tx: mpsc::Sender<ExternalNotification>,
}

impl LobbyEventBus {
    pub async fn emit_core(&self, event: CoreLobbyEvent) {
        let _ = self.broadcast_tx.send(LobbyOutboundMessage::Core(event));
    }
    
    pub async fn emit_plugin(&self, event: PluginLobbyEvent) {
        let _ = self.broadcast_tx.send(LobbyOutboundMessage::Plugin(event));
    }
    
    pub async fn notify_external(&self, notification: ExternalNotification) {
        let _ = self.external_tx.send(notification).await;
    }
}
```

### 5.5 Plugin-Defined Message Types and State Machines

```rust
// Plugin interface for lobby behavior
#[async_trait]
pub trait GameLobbyPlugin: Send + Sync {
    /// Game identifier
    fn game_id(&self) -> GameId;
    
    /// Create initial game-specific lobby state
    fn create_lobby_state(&self, config: &GameConfig) -> Box<dyn GameLobbyState>;
    
    /// Validate incoming plugin message
    fn validate_message(
        &self,
        message: &serde_json::Value,
    ) -> Result<ValidatedMessage, ValidationError>;
    
    /// Get the state machine definition for this game's lobby
    fn state_machine(&self) -> &GameStateMachine;
}

// Game-specific lobby state trait
#[async_trait]
pub trait GameLobbyState: Send + Sync {
    /// Handle game-specific action from a player
    async fn handle_action(
        &mut self,
        player_id: PlayerId,
        action: ValidatedMessage,
        ctx: &LobbyContext,
    ) -> Result<Vec<PluginLobbyEvent>, GameError>;
    
    /// Check if an action is allowed in current state
    fn is_action_allowed(
        &self,
        player_id: PlayerId,
        action: &ValidatedMessage,
    ) -> bool;
    
    /// Get current phase
    fn current_phase(&self) -> &str;
    
    /// Check if lobby is ready to start match
    fn is_ready_to_start(&self) -> bool;
    
    /// Serialize state for sync
    fn snapshot(&self) -> serde_json::Value;
    
    /// Handle timeout for current phase
    async fn handle_timeout(
        &mut self,
        ctx: &LobbyContext,
    ) -> Result<Vec<PluginLobbyEvent>, GameError>;
}

// Example: CS2 Pick/Ban State Machine
pub struct Cs2LobbyState {
    phase: Cs2Phase,
    map_pool: Vec<Map>,
    bans: Vec<MapBan>,
    picks: Vec<MapPick>,
    current_team: TeamSlot,
    side_selections: HashMap<Map, SideSelection>,
}

#[derive(Clone, Debug)]
pub enum Cs2Phase {
    Setup,
    MapBan { ban_number: u8, team: TeamSlot },
    MapPick { pick_number: u8, team: TeamSlot },
    SideSelect { map: Map, team: TeamSlot },
    Ready,
}

#[async_trait]
impl GameLobbyState for Cs2LobbyState {
    async fn handle_action(
        &mut self,
        player_id: PlayerId,
        action: ValidatedMessage,
        ctx: &LobbyContext,
    ) -> Result<Vec<PluginLobbyEvent>, GameError> {
        match action.action_type.as_str() {
            "ban_map" => {
                let map: Map = serde_json::from_value(action.payload)?;
                self.handle_ban(player_id, map, ctx).await
            }
            "pick_map" => {
                let map: Map = serde_json::from_value(action.payload)?;
                self.handle_pick(player_id, map, ctx).await
            }
            "select_side" => {
                let side: Side = serde_json::from_value(action.payload)?;
                self.handle_side_select(player_id, side, ctx).await
            }
            _ => Err(GameError::UnknownAction),
        }
    }
    
    fn is_action_allowed(&self, player_id: PlayerId, action: &ValidatedMessage) -> bool {
        // Check if it's this player's turn and action matches phase
        match &self.phase {
            Cs2Phase::MapBan { team, .. } => {
                action.action_type == "ban_map" && 
                self.is_team_captain(player_id, *team)
            }
            Cs2Phase::MapPick { team, .. } => {
                action.action_type == "pick_map" && 
                self.is_team_captain(player_id, *team)
            }
            Cs2Phase::SideSelect { team, .. } => {
                action.action_type == "select_side" && 
                self.is_team_captain(player_id, *team)
            }
            _ => false,
        }
    }
    
    // ... other implementations
}
```

### 5.6 Recommended WebSocket Crates

| Crate | Purpose | Version | Notes |
|-------|---------|---------|-------|
| `axum` | WebSocket upgrade | 0.7+ | Built-in WS support via `extract::ws` |
| `tokio-tungstenite` | Low-level WS | 0.21+ | Underlying implementation |
| `tokio::sync::broadcast` | Lobby broadcasts | - | Multi-consumer channel |
| `tokio::sync::mpsc` | Actor messages | - | Single-consumer channel |
| `dashmap` | Concurrent sessions | 5+ | Lock-free concurrent map |
| `parking_lot` | Synchronization | 0.12+ | Faster mutexes |

### 5.7 Connection Architecture

```
                         ┌───────────────────────┐
                         │   Connection Manager   │
                         │  (per server instance) │
                         └───────────┬───────────┘
                                     │
           ┌─────────────────────────┼─────────────────────────┐
           │                         │                         │
           ▼                         ▼                         ▼
    ┌─────────────┐          ┌─────────────┐          ┌─────────────┐
    │   Lobby A   │          │   Lobby B   │          │   Lobby C   │
    │   Actor     │          │   Actor     │          │   Actor     │
    └──────┬──────┘          └──────┬──────┘          └──────┬──────┘
           │                        │                        │
    ┌──────┴──────┐          ┌──────┴──────┐          ┌──────┴──────┐
    │  Sessions   │          │  Sessions   │          │  Sessions   │
    │ P1  P2  P3  │          │ P4  P5      │          │ P6  P7  P8  │
    └─────────────┘          └─────────────┘          └─────────────┘

Each lobby actor:
- Owns its WebSocket connections
- Maintains game state
- Processes messages sequentially (no data races)
- Broadcasts events to all members
```

---

## 6. Plugin System Architecture

### 6.1 Plugin Interface Definition

```rust
// Core plugin trait - all game plugins must implement
#[async_trait]
pub trait GamePlugin: Send + Sync + 'static {
    // === Identity ===
    
    /// Unique identifier for this game
    fn game_id(&self) -> GameId;
    
    /// Human-readable name
    fn display_name(&self) -> &str;
    
    /// Plugin version
    fn version(&self) -> Version;
    
    /// Minimum core platform version required
    fn required_core_version(&self) -> VersionReq;
    
    // === Map Pool Configuration ===
    
    /// Get all available maps for this game
    fn available_maps(&self) -> Vec<GameMap>;
    
    /// Get default competitive map pool
    fn default_map_pool(&self) -> Vec<String>;
    
    /// Validate a custom map pool selection
    fn validate_map_pool(&self, maps: &[String]) -> Result<(), MapPoolError>;
    
    /// Whether this game supports custom map pool selection for tournaments/leagues
    fn supports_custom_map_pool(&self) -> bool { true }
    
    // === Metadata Schema ===
    
    /// JSON schema for match metadata
    fn match_metadata_schema(&self) -> &JsonSchema;
    
    /// JSON schema for player game-specific stats
    fn player_stats_schema(&self) -> &JsonSchema;
    
    /// JSON schema for lobby configuration
    fn lobby_config_schema(&self) -> &JsonSchema;
    
    // === Statistics ===
    
    /// Extract and calculate player statistics from match data
    /// Called after each match to update player_game_profiles.game_specific_stats
    fn calculate_player_stats(
        &self,
        match_data: &MatchData,
        player_id: PlayerId,
        existing_stats: &serde_json::Value,
    ) -> Result<serde_json::Value, StatsError>;
    
    /// Get display-friendly statistics for a player
    fn format_player_stats(
        &self,
        stats: &serde_json::Value,
    ) -> Vec<DisplayStat>;
    
    /// Define rank tiers for this game (used for leaderboards and display)
    fn rank_tiers(&self) -> Vec<RankTier>;
    
    /// Calculate which rank tier a player belongs to based on rating
    fn rating_to_rank_tier(&self, rating: i32) -> Option<RankTier>;
    
    // === Matchmaking ===
    
    /// Create matchmaking adapter for this game
    fn matchmaking_adapter(&self) -> Arc<dyn MatchmakingAdapter>;
    
    // === Ranking ===
    
    /// Create ranking calculator for this game
    fn ranking_calculator(&self) -> Arc<dyn RankingCalculator>;
    
    // === Lobby ===
    
    /// Create lobby plugin for managing game-specific lobby state
    fn lobby_plugin(&self) -> Arc<dyn GameLobbyPlugin>;
    
    // === Server Integration ===
    
    /// Create game server adapter (if game supports server integration)
    fn server_adapter(&self) -> Option<Arc<dyn GameServerAdapter>> { None }
    
    // === Lifecycle ===
    
    /// Called when plugin is loaded
    async fn on_load(&self, ctx: &PluginContext) -> Result<(), PluginError>;
    
    /// Called when plugin is unloaded
    async fn on_unload(&self) -> Result<(), PluginError>;
}

/// Represents a map available in a game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameMap {
    pub id: String,          // e.g., "de_dust2"
    pub display_name: String, // e.g., "Dust II"
    pub thumbnail_url: Option<String>,
    pub is_competitive: bool,
    pub supports_modes: Vec<String>,  // e.g., ["defuse", "hostage"]
}

/// Represents a rank tier defined by the game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RankTier {
    pub id: String,         // e.g., "global_elite"
    pub name: String,       // e.g., "Global Elite"
    pub min_rating: i32,
    pub max_rating: Option<i32>,  // None for top tier
    pub icon_url: Option<String>,
    pub color: Option<String>,
}

/// Display-friendly statistic for UI
#[derive(Clone, Debug, Serialize)]
pub struct DisplayStat {
    pub key: String,        // e.g., "kd_ratio"
    pub label: String,      // e.g., "K/D Ratio"
    pub value: String,      // e.g., "1.24"
    pub category: String,   // e.g., "Combat", "Economy", "Utility"
}
```

### 6.2 Matchmaking Rules Interface

```rust
#[async_trait]
pub trait MatchmakingAdapter: Send + Sync {
    /// Extract matchmaking criteria from player preferences
    fn extract_criteria(
        &self,
        preferences: &serde_json::Value,
    ) -> Result<MatchCriteria, MatchmakingError>;
    
    /// Calculate compatibility score between two players/groups
    fn compatibility_score(
        &self,
        seeker: &MatchCriteria,
        candidate: &MatchCriteria,
    ) -> f64;
    
    /// Check if a potential match meets minimum requirements
    fn is_valid_match(
        &self,
        participants: &[MatchCriteria],
        config: &QueueConfig,
    ) -> bool;
    
    /// Balance teams given matched players
    fn balance_teams(
        &self,
        participants: Vec<MatchParticipant>,
        team_size: usize,
    ) -> Result<TeamAssignment, MatchmakingError>;
    
    /// Estimated wait time based on current queue
    fn estimate_wait_time(
        &self,
        criteria: &MatchCriteria,
        queue_stats: &QueueStats,
    ) -> Duration;
}

// Example implementation for CS2
pub struct Cs2MatchmakingAdapter;

impl MatchmakingAdapter for Cs2MatchmakingAdapter {
    fn extract_criteria(
        &self,
        preferences: &serde_json::Value,
    ) -> Result<MatchCriteria, MatchmakingError> {
        let prefs: Cs2Preferences = serde_json::from_value(preferences.clone())?;
        
        Ok(MatchCriteria {
            skill_rating: prefs.skill_rating,
            region: prefs.preferred_region,
            game_specific: json!({
                "map_pool": prefs.map_pool,
                "prime_status": prefs.prime,
                "max_ping": prefs.max_ping,
            }),
        })
    }
    
    fn compatibility_score(&self, seeker: &MatchCriteria, candidate: &MatchCriteria) -> f64 {
        let mut score = 0.0;
        
        // Skill rating proximity (higher = better)
        let rating_diff = (seeker.skill_rating - candidate.skill_rating).abs();
        score += (1.0 - (rating_diff / 1000.0).min(1.0)) * 0.5;
        
        // Region match
        if seeker.region == candidate.region {
            score += 0.3;
        }
        
        // Map pool overlap
        let seeker_maps: HashSet<_> = seeker.game_specific["map_pool"]
            .as_array().unwrap().iter().collect();
        let candidate_maps: HashSet<_> = candidate.game_specific["map_pool"]
            .as_array().unwrap().iter().collect();
        let overlap = seeker_maps.intersection(&candidate_maps).count();
        score += (overlap as f64 / seeker_maps.len() as f64) * 0.2;
        
        score
    }
    
    // ... other implementations
}
```

### 6.3 Ranking Logic Interface

```rust
#[async_trait]
pub trait RankingCalculator: Send + Sync {
    /// Calculate rating changes after a match
    fn calculate_rating_changes(
        &self,
        match_result: &MatchResult,
        participants: &[RankedParticipant],
    ) -> Vec<RatingChange>;
    
    /// Get initial rating for new players
    fn initial_rating(&self) -> Rating;
    
    /// Calculate uncertainty/confidence in rating
    fn calculate_uncertainty(&self, player: &RankedParticipant) -> f64;
    
    /// Get rank tier from rating
    fn rating_to_tier(&self, rating: &Rating) -> RankTier;
    
    /// Check for rank-up/rank-down
    fn check_rank_change(
        &self,
        old_rating: &Rating,
        new_rating: &Rating,
    ) -> Option<RankChange>;
}

// Example: Glicko-2 implementation
pub struct Glicko2Calculator {
    tau: f64,           // System constant
    initial_rating: f64,
    initial_rd: f64,
    initial_volatility: f64,
}

impl RankingCalculator for Glicko2Calculator {
    fn calculate_rating_changes(
        &self,
        match_result: &MatchResult,
        participants: &[RankedParticipant],
    ) -> Vec<RatingChange> {
        // Implementation of Glicko-2 algorithm
        let mut changes = Vec::new();
        
        for participant in participants {
            let opponents: Vec<_> = participants
                .iter()
                .filter(|p| p.team != participant.team)
                .collect();
            
            let outcome = match match_result.winner {
                Some(team) if team == participant.team => 1.0,
                Some(_) => 0.0,
                None => 0.5, // Draw
            };
            
            let new_rating = self.glicko2_update(
                participant.rating,
                participant.rd,
                participant.volatility,
                &opponents,
                outcome,
            );
            
            changes.push(RatingChange {
                player_id: participant.player_id,
                old_rating: participant.rating,
                new_rating,
                // ... other fields
            });
        }
        
        changes
    }
    
    // ... other implementations
}
```

### 6.4 Metadata Schemas

```rust
// Plugin provides JSON Schema for validation
pub trait MetadataSchemaProvider {
    fn match_metadata_schema(&self) -> &JsonSchema;
    fn player_stats_schema(&self) -> &JsonSchema;
}

// Example CS2 match metadata
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Cs2MatchMetadata {
    pub map: String,
    pub game_mode: Cs2GameMode,
    pub score: TeamScores,
    pub rounds: Vec<RoundData>,
    pub mvp: Option<PlayerId>,
    pub server_id: String,
    pub demo_url: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Cs2PlayerMatchStats {
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub headshot_percentage: f32,
    pub adr: f32, // Average damage per round
    pub kast: f32, // Kill/Assist/Survive/Trade percentage
    pub rating: f32, // HLTV-style rating
    pub mvp_rounds: u32,
    pub utility_damage: u32,
    pub flash_assists: u32,
}

// Schema registration during plugin load
impl GamePlugin for Cs2Plugin {
    fn match_metadata_schema(&self) -> &JsonSchema {
        &self.schemas.match_metadata
    }
    
    fn player_stats_schema(&self) -> &JsonSchema {
        &self.schemas.player_stats
    }
}
```

### 6.5 Versioning and Compatibility

```rust
use semver::{Version, VersionReq};

// Plugin manifest
#[derive(Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: GameId,
    pub name: String,
    pub version: Version,
    pub required_core_version: VersionReq,
    pub authors: Vec<String>,
    pub description: String,
    pub migrations: Vec<MigrationEntry>,
}

// Version compatibility check
pub struct PluginRegistry {
    plugins: HashMap<GameId, LoadedPlugin>,
    core_version: Version,
}

impl PluginRegistry {
    pub fn register(&mut self, plugin: Box<dyn GamePlugin>) -> Result<(), PluginError> {
        // Check core version compatibility
        if !plugin.required_core_version().matches(&self.core_version) {
            return Err(PluginError::IncompatibleVersion {
                plugin: plugin.game_id(),
                required: plugin.required_core_version(),
                actual: self.core_version.clone(),
            });
        }
        
        // Check for conflicting plugin
        if let Some(existing) = self.plugins.get(&plugin.game_id()) {
            return Err(PluginError::AlreadyRegistered {
                plugin: plugin.game_id(),
                existing_version: existing.version.clone(),
            });
        }
        
        // Register plugin
        self.plugins.insert(plugin.game_id(), LoadedPlugin::new(plugin));
        Ok(())
    }
}

// Migration support for plugin data
pub struct MigrationEntry {
    pub from_version: Version,
    pub to_version: Version,
    pub migration_sql: String,
}
```

### 6.6 Build vs Reuse Decision: Plugin Loading Strategy

**Recommendation: Compile-Time Plugin Crates**

| Approach | Pros | Cons |
|----------|------|------|
| **Dynamic Loading (dlopen)** | Hot reload, third-party plugins | Unsafe FFI, ABI stability issues, security risks |
| **Compile-Time Crates** ✓ | Type safety, performance, security | Requires recompilation, no hot reload |
| **WASM Plugins** | Sandboxed, portable | Performance overhead, limited capabilities |

**Justification for Compile-Time Approach:**

1. **Type Safety:** Rust's trait system ensures plugins implement required interfaces correctly
2. **Performance:** No FFI overhead, full optimization
3. **Security:** No arbitrary code loading, plugins reviewed before compilation
4. **Debugging:** Full stack traces, integrated tooling
5. **Stability:** No ABI compatibility concerns

```rust
// Plugin registration at compile time
pub fn register_plugins(registry: &mut PluginRegistry) -> Result<(), PluginError> {
    // Built-in plugins
    registry.register(Box::new(aoe4::Aoe4Plugin::new()))?;
    registry.register(Box::new(cs2::Cs2Plugin::new()))?;
    
    // Feature-gated third-party plugins
    #[cfg(feature = "plugin-valorant")]
    registry.register(Box::new(valorant::ValorantPlugin::new()))?;
    
    Ok(())
}
```

### 6.7 Plugin Sandbox Boundaries

Even with compile-time plugins, boundaries must be enforced:

```rust
// Plugin context limits what plugins can access
pub struct PluginContext {
    // Database access scoped to plugin's data
    pub db: ScopedDbPool,
    
    // Configuration read-only access
    pub config: Arc<PluginConfig>,
    
    // Event emitter (no direct broadcast access)
    pub events: PluginEventEmitter,
    
    // Metrics scoped to plugin namespace
    pub metrics: ScopedMetrics,
}

// Scoped database access
pub struct ScopedDbPool {
    pool: PgPool,
    game_id: GameId,
}

impl ScopedDbPool {
    // All queries automatically scoped to game_id
    pub async fn get_plugin_data<T: DeserializeOwned>(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Option<T>, DbError> {
        sqlx::query_as(
            "SELECT data FROM plugin_data 
             WHERE game_id = $1 AND entity_type = $2 AND entity_id = $3"
        )
        .bind(&self.game_id)
        .bind(entity_type)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await
    }
    
    // Plugin cannot access other games' data
    pub async fn set_plugin_data<T: Serialize>(
        &self,
        entity_type: &str,
        entity_id: &str,
        data: &T,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO plugin_data (game_id, entity_type, entity_id, data)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (game_id, entity_type, entity_id) 
             DO UPDATE SET data = $4, updated_at = NOW()"
        )
        .bind(&self.game_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(serde_json::to_value(data)?)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

### 6.8 Game Server Integration

Plugins can integrate with external game servers to configure matches after lobby pick/ban phases complete. This enables automated server setup for Bo1, Bo3, Bo5 series with the correct maps, settings, and player configurations.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     GAME SERVER INTEGRATION FLOW                             │
└─────────────────────────────────────────────────────────────────────────────┘

  Lobby Completes              Plugin Configures           Server Ready
  Pick/Ban Phase               Game Server                 for Players
        │                            │                          │
        ▼                            ▼                          ▼
┌───────────────┐            ┌───────────────┐          ┌───────────────┐
│ LobbyComplete │───────────▶│ ServerConfig  │─────────▶│  ServerReady  │
│    Event      │            │   Generator   │          │    Event      │
└───────────────┘            └───────┬───────┘          └───────────────┘
                                     │
                    ┌────────────────┼────────────────┐
                    │                │                │
                    ▼                ▼                ▼
             ┌───────────┐    ┌───────────┐    ┌───────────┐
             │   RCON    │    │  Server   │    │   Cloud   │
             │  Command  │    │   API     │    │ Provider  │
             └───────────┘    └───────────┘    └───────────┘
```

#### 6.8.1 Game Server Adapter Interface

```rust
/// Trait for game server integration
#[async_trait]
pub trait GameServerAdapter: Send + Sync {
    /// Get adapter identifier
    fn adapter_id(&self) -> &str;
    
    /// Check if a server is available and healthy
    async fn health_check(&self, server: &GameServer) -> Result<ServerHealth, ServerError>;
    
    /// Reserve a server for a match
    async fn reserve_server(
        &self,
        match_id: MatchId,
        requirements: &ServerRequirements,
    ) -> Result<ServerReservation, ServerError>;
    
    /// Configure server for a specific match setup
    async fn configure_match(
        &self,
        server: &GameServer,
        config: &MatchServerConfig,
    ) -> Result<(), ServerError>;
    
    /// Send RCON command to server
    async fn rcon_command(
        &self,
        server: &GameServer,
        command: &str,
    ) -> Result<String, ServerError>;
    
    /// Get current server status
    async fn get_status(&self, server: &GameServer) -> Result<ServerStatus, ServerError>;
    
    /// Release server after match completion
    async fn release_server(
        &self,
        reservation: &ServerReservation,
    ) -> Result<(), ServerError>;
    
    /// Handle server events (map change, match end, etc.)
    async fn handle_server_event(
        &self,
        server: &GameServer,
        event: ServerEvent,
    ) -> Result<(), ServerError>;
}

/// Server requirements for match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRequirements {
    pub game_id: GameId,
    pub region: Region,
    pub min_slots: u8,
    pub game_mode: String,
    pub required_mods: Vec<String>,
    pub tickrate: Option<u32>,
    pub competitive_settings: bool,
}

/// Match-specific server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchServerConfig {
    pub match_id: MatchId,
    pub format: MatchFormat,
    pub maps: Vec<MapConfig>,
    pub teams: [TeamConfig; 2],
    pub rules: GameRules,
    pub recording: RecordingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchFormat {
    pub series_type: SeriesType,  // Bo1, Bo3, Bo5
    pub current_map: u8,
    pub maps_to_win: u8,
    pub side_selection: SideSelectionRule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SeriesType {
    Bo1,
    Bo3,
    Bo5,
    Bo7,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    pub map_name: String,
    pub map_index: u8,
    pub picked_by: Option<TeamSlot>,
    pub starting_sides: Option<[Side; 2]>,
    pub overtime_rules: OvertimeRules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub team_id: TeamId,
    pub team_name: String,
    pub team_tag: String,
    pub players: Vec<PlayerServerConfig>,
    pub coach: Option<PlayerServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerServerConfig {
    pub player_id: PlayerId,
    pub display_name: String,
    pub game_uid: String,  // Steam ID, game-specific ID, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    pub record_demo: bool,
    pub record_pov: bool,
    pub stream_gotv: bool,
    pub gotv_delay_secs: u32,
}
```

#### 6.8.2 Server Pool Management

```rust
/// Manages pool of available game servers
pub struct ServerPoolManager {
    servers: DashMap<ServerId, ManagedServer>,
    reservations: DashMap<MatchId, ServerReservation>,
    adapters: HashMap<String, Arc<dyn GameServerAdapter>>,
    health_checker: ServerHealthChecker,
}

#[derive(Debug, Clone)]
pub struct ManagedServer {
    pub id: ServerId,
    pub adapter_id: String,
    pub address: SocketAddr,
    pub region: Region,
    pub game_id: GameId,
    pub status: ServerPoolStatus,
    pub current_match: Option<MatchId>,
    pub specs: ServerSpecs,
    pub last_health_check: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServerPoolStatus {
    Available,
    Reserved,
    Configuring,
    InMatch,
    Maintenance,
    Offline,
}

impl ServerPoolManager {
    /// Find and reserve an available server for a match
    pub async fn allocate_server(
        &self,
        match_id: MatchId,
        requirements: &ServerRequirements,
    ) -> Result<ServerReservation, ServerError> {
        // Find matching available server
        let server = self.find_available_server(requirements).await?;
        
        // Get appropriate adapter
        let adapter = self.adapters
            .get(&server.adapter_id)
            .ok_or(ServerError::AdapterNotFound)?;
        
        // Reserve through adapter
        let reservation = adapter.reserve_server(match_id, requirements).await?;
        
        // Update pool status
        self.servers.alter(&server.id, |_, mut s| {
            s.status = ServerPoolStatus::Reserved;
            s.current_match = Some(match_id);
            s
        });
        
        self.reservations.insert(match_id, reservation.clone());
        
        Ok(reservation)
    }
    
    /// Configure server with match settings after lobby completes
    pub async fn setup_match(
        &self,
        match_id: MatchId,
        config: MatchServerConfig,
    ) -> Result<ServerConnectionInfo, ServerError> {
        let reservation = self.reservations
            .get(&match_id)
            .ok_or(ServerError::NoReservation)?;
        
        let server = self.servers
            .get(&reservation.server_id)
            .ok_or(ServerError::ServerNotFound)?;
        
        let adapter = self.adapters
            .get(&server.adapter_id)
            .ok_or(ServerError::AdapterNotFound)?;
        
        // Update status
        self.servers.alter(&server.id, |_, mut s| {
            s.status = ServerPoolStatus::Configuring;
            s
        });
        
        // Configure the server
        adapter.configure_match(&server.to_game_server(), &config).await?;
        
        // Update status to in-match
        self.servers.alter(&server.id, |_, mut s| {
            s.status = ServerPoolStatus::InMatch;
            s
        });
        
        Ok(ServerConnectionInfo {
            address: server.address.to_string(),
            port: server.address.port(),
            password: reservation.password.clone(),
            gotv_address: reservation.gotv_address.clone(),
            gotv_port: reservation.gotv_port,
        })
    }
}
```

#### 6.8.3 CS2 Server Adapter Example

```rust
/// Counter-Strike 2 server adapter using RCON and Get5
pub struct Cs2ServerAdapter {
    rcon_client: RconClient,
    config: Cs2ServerConfig,
}

impl Cs2ServerAdapter {
    /// Generate Get5 match configuration
    fn generate_get5_config(&self, config: &MatchServerConfig) -> Get5MatchConfig {
        Get5MatchConfig {
            matchid: config.match_id.to_string(),
            num_maps: config.maps.len() as u8,
            maplist: config.maps.iter().map(|m| m.map_name.clone()).collect(),
            skip_veto: true,  // Veto already done in lobby
            side_type: match config.format.side_selection {
                SideSelectionRule::Standard => "standard",
                SideSelectionRule::NeverKnife => "never_knife",
                SideSelectionRule::AlwaysKnife => "always_knife",
            }.to_string(),
            players_per_team: 5,
            min_players_to_ready: 1,
            team1: Get5Team {
                name: config.teams[0].team_name.clone(),
                tag: config.teams[0].team_tag.clone(),
                players: config.teams[0].players.iter()
                    .map(|p| (p.game_uid.clone(), p.display_name.clone()))
                    .collect(),
            },
            team2: Get5Team {
                name: config.teams[1].team_name.clone(),
                tag: config.teams[1].team_tag.clone(),
                players: config.teams[1].players.iter()
                    .map(|p| (p.game_uid.clone(), p.display_name.clone()))
                    .collect(),
            },
            cvars: self.generate_competitive_cvars(),
        }
    }
    
    fn generate_competitive_cvars(&self) -> HashMap<String, String> {
        let mut cvars = HashMap::new();
        cvars.insert("sv_cheats".into(), "0".into());
        cvars.insert("mp_autoteambalance".into(), "0".into());
        cvars.insert("mp_limitteams".into(), "0".into());
        cvars.insert("sv_hibernate_when_empty".into(), "0".into());
        // ... more competitive settings
        cvars
    }
}

#[async_trait]
impl GameServerAdapter for Cs2ServerAdapter {
    fn adapter_id(&self) -> &str { "cs2_get5" }
    
    async fn configure_match(
        &self,
        server: &GameServer,
        config: &MatchServerConfig,
    ) -> Result<(), ServerError> {
        // Connect to RCON
        let mut rcon = self.rcon_client.connect(
            &server.address,
            &server.rcon_password,
        ).await?;
        
        // Generate Get5 config
        let get5_config = self.generate_get5_config(config);
        let config_json = serde_json::to_string(&get5_config)?;
        
        // Upload config to server
        rcon.command(&format!(
            "get5_loadmatch_url \"{}\"",
            self.config.match_config_url(&config.match_id)
        )).await?;
        
        // Or load directly
        // rcon.command(&format!("get5_loadmatch {}", config_json)).await?;
        
        Ok(())
    }
    
    async fn rcon_command(
        &self,
        server: &GameServer,
        command: &str,
    ) -> Result<String, ServerError> {
        let mut rcon = self.rcon_client.connect(
            &server.address,
            &server.rcon_password,
        ).await?;
        
        let response = rcon.command(command).await?;
        Ok(response)
    }
    
    async fn handle_server_event(
        &self,
        server: &GameServer,
        event: ServerEvent,
    ) -> Result<(), ServerError> {
        match event {
            ServerEvent::MapEnd { map_name, winner, score } => {
                // Update match state, check if series complete
            }
            ServerEvent::MatchEnd { match_id, winner, final_score } => {
                // Finalize match, trigger stats collection
            }
            ServerEvent::PlayerDisconnect { steam_id, reason } => {
                // Handle player disconnect during match
            }
            _ => {}
        }
        Ok(())
    }
    
    // ... other implementations
}

/// Get5 match configuration structure
#[derive(Serialize)]
struct Get5MatchConfig {
    matchid: String,
    num_maps: u8,
    maplist: Vec<String>,
    skip_veto: bool,
    side_type: String,
    players_per_team: u8,
    min_players_to_ready: u8,
    team1: Get5Team,
    team2: Get5Team,
    cvars: HashMap<String, String>,
}

#[derive(Serialize)]
struct Get5Team {
    name: String,
    tag: String,
    players: HashMap<String, String>,  // steam_id -> name
}
```

#### 6.8.4 Server Event Webhook Handler

```rust
/// Handles callbacks from game servers
pub struct ServerWebhookHandler {
    server_pool: Arc<ServerPoolManager>,
    match_service: Arc<dyn MatchService>,
    stats_service: Arc<dyn StatsService>,
    event_bus: Arc<dyn EventBus>,
}

impl ServerWebhookHandler {
    /// Handle Get5 webhook events
    pub async fn handle_get5_event(
        &self,
        event: Get5WebhookEvent,
    ) -> Result<(), ServerError> {
        match event.event {
            "series_start" => {
                self.event_bus.publish(MatchEvent::SeriesStarted {
                    match_id: event.matchid.parse()?,
                }).await;
            }
            
            "map_result" => {
                let map_result = event.map_result.unwrap();
                self.match_service.record_map_result(
                    event.matchid.parse()?,
                    MapResult {
                        map_number: map_result.map_number,
                        map_name: map_result.map_name,
                        team1_score: map_result.team1_score,
                        team2_score: map_result.team2_score,
                        winner: map_result.winner,
                    },
                ).await?;
            }
            
            "series_end" => {
                let series_result = event.series_result.unwrap();
                
                // Record final result
                self.match_service.complete_match(
                    event.matchid.parse()?,
                    MatchResult {
                        winner: series_result.winner,
                        team1_maps: series_result.team1_series_score,
                        team2_maps: series_result.team2_series_score,
                    },
                ).await?;
                
                // Release server
                self.server_pool.release_server(event.matchid.parse()?).await?;
                
                // Trigger demo upload
                if let Some(demo_url) = event.demo_upload_url {
                    self.stats_service.queue_demo_processing(
                        event.matchid.parse()?,
                        demo_url,
                    ).await?;
                }
            }
            
            "player_stats" => {
                // Real-time stats update
                let stats = event.player_stats.unwrap();
                self.stats_service.update_live_stats(
                    event.matchid.parse()?,
                    stats,
                ).await?;
            }
            
            "round_end" => {
                // Broadcast live score update
                self.event_bus.publish(MatchEvent::RoundEnded {
                    match_id: event.matchid.parse()?,
                    round: event.round_number.unwrap(),
                    team1_score: event.team1_score.unwrap(),
                    team2_score: event.team2_score.unwrap(),
                }).await;
            }
            
            _ => {
                tracing::debug!("Unhandled Get5 event: {}", event.event);
            }
        }
        
        Ok(())
    }
}

/// API route for server webhooks
pub fn server_webhook_routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks/servers/get5", post(handle_get5_webhook))
        .route("/webhooks/servers/dathost", post(handle_dathost_webhook))
        .route("/webhooks/servers/generic", post(handle_generic_webhook))
}
```

#### 6.8.5 Cloud Server Provider Integration

```rust
/// Integration with cloud game server providers (e.g., Dathost, GCP, AWS)
#[async_trait]
pub trait CloudServerProvider: Send + Sync {
    /// Spin up a new server instance
    async fn create_server(
        &self,
        config: CloudServerConfig,
    ) -> Result<CloudServerInstance, ProviderError>;
    
    /// Start an existing server
    async fn start_server(&self, instance_id: &str) -> Result<(), ProviderError>;
    
    /// Stop a server
    async fn stop_server(&self, instance_id: &str) -> Result<(), ProviderError>;
    
    /// Terminate and delete a server
    async fn terminate_server(&self, instance_id: &str) -> Result<(), ProviderError>;
    
    /// Get server connection details
    async fn get_connection_info(
        &self,
        instance_id: &str,
    ) -> Result<ServerConnectionInfo, ProviderError>;
    
    /// List available server templates/images
    async fn list_templates(&self) -> Result<Vec<ServerTemplate>, ProviderError>;
}

/// Dathost provider implementation
pub struct DathostProvider {
    api_client: DathostApiClient,
    config: DathostConfig,
}

#[async_trait]
impl CloudServerProvider for DathostProvider {
    async fn create_server(
        &self,
        config: CloudServerConfig,
    ) -> Result<CloudServerInstance, ProviderError> {
        let response = self.api_client.post("/game-servers")
            .json(&DathostCreateRequest {
                game: &config.game,
                location: &config.region,
                name: &config.name,
                // Dathost-specific settings
                slots: config.slots,
                enable_gotv: config.recording.stream_gotv,
            })
            .send()
            .await?;
        
        let server: DathostServer = response.json().await?;
        
        Ok(CloudServerInstance {
            instance_id: server.id,
            provider: "dathost".into(),
            address: server.ip,
            port: server.ports.game,
            rcon_password: server.rcon_password,
            status: InstanceStatus::Starting,
        })
    }
    
    // ... other implementations
}
```

#### 6.8.6 Integration with Lobby Flow

```rust
/// Called when lobby pick/ban phase completes
pub async fn on_lobby_complete(
    lobby: &Lobby,
    lobby_result: &LobbyResult,
    server_pool: &ServerPoolManager,
    match_service: &dyn MatchService,
) -> Result<ServerConnectionInfo, LobbyError> {
    // Create match record
    let match_record = match_service.create_match(CreateMatchRequest {
        game_id: lobby.game_id.clone(),
        match_type: MatchType::from_lobby(&lobby),
        source_id: Some(lobby.id.into()),
        teams: lobby_result.teams.clone(),
    }).await?;
    
    // Build server requirements from lobby config
    let requirements = ServerRequirements {
        game_id: lobby.game_id.clone(),
        region: lobby.config.region.clone(),
        min_slots: (lobby_result.teams[0].players.len() 
                  + lobby_result.teams[1].players.len()) as u8,
        game_mode: lobby.config.game_mode.clone(),
        required_mods: lobby.config.required_mods.clone(),
        tickrate: lobby.config.tickrate,
        competitive_settings: true,
    };
    
    // Allocate server
    let reservation = server_pool.allocate_server(match_record.id, &requirements).await?;
    
    // Build match server config from lobby result
    let server_config = MatchServerConfig {
        match_id: match_record.id,
        format: MatchFormat {
            series_type: lobby_result.series_type,
            current_map: 0,
            maps_to_win: lobby_result.series_type.maps_to_win(),
            side_selection: lobby_result.side_selection_rule,
        },
        maps: lobby_result.maps.iter().enumerate().map(|(i, m)| {
            MapConfig {
                map_name: m.name.clone(),
                map_index: i as u8,
                picked_by: m.picked_by,
                starting_sides: m.starting_sides,
                overtime_rules: lobby.config.overtime_rules.clone(),
            }
        }).collect(),
        teams: [
            build_team_config(&lobby_result.teams[0]),
            build_team_config(&lobby_result.teams[1]),
        ],
        rules: lobby.config.game_rules.clone(),
        recording: RecordingConfig {
            record_demo: true,
            record_pov: false,
            stream_gotv: lobby.config.enable_gotv,
            gotv_delay_secs: 90,
        },
    };
    
    // Configure and start server
    let connection_info = server_pool.setup_match(match_record.id, server_config).await?;
    
    // Update match with server info
    match_service.update_match_server(match_record.id, &connection_info).await?;
    
    Ok(connection_info)
}
```

---

## 7. Module Descriptions

### 7.1 Auth Service

**Responsibility:** User authentication, token management, OAuth integration

```rust
#[async_trait]
pub trait AuthService: Send + Sync {
    // Local authentication
    async fn register(&self, req: RegisterRequest) -> Result<AuthResponse, AuthError>;
    async fn login(&self, req: LoginRequest) -> Result<AuthResponse, AuthError>;
    async fn logout(&self, token: &str) -> Result<(), AuthError>;
    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthResponse, AuthError>;
    
    // Password management
    async fn change_password(&self, user_id: UserId, req: ChangePasswordRequest) -> Result<(), AuthError>;
    async fn request_password_reset(&self, email: &str) -> Result<(), AuthError>;
    async fn reset_password(&self, token: &str, new_password: &str) -> Result<(), AuthError>;
    
    // OAuth
    async fn oauth_authorize(&self, provider: OAuthProvider) -> Result<AuthorizeUrl, AuthError>;
    async fn oauth_callback(&self, provider: OAuthProvider, code: &str) -> Result<AuthResponse, AuthError>;
    async fn link_oauth(&self, user_id: UserId, provider: OAuthProvider, code: &str) -> Result<(), AuthError>;
    
    // Session management
    async fn list_sessions(&self, user_id: UserId) -> Result<Vec<Session>, AuthError>;
    async fn revoke_session(&self, user_id: UserId, session_id: SessionId) -> Result<(), AuthError>;
    async fn revoke_all_sessions(&self, user_id: UserId) -> Result<(), AuthError>;
    
    // 2FA (optional)
    async fn enable_2fa(&self, user_id: UserId) -> Result<TwoFactorSetup, AuthError>;
    async fn verify_2fa(&self, user_id: UserId, code: &str) -> Result<(), AuthError>;
    async fn disable_2fa(&self, user_id: UserId, code: &str) -> Result<(), AuthError>;
}
```

**Key Decisions:**
- JWT access tokens (15 min expiry) + refresh tokens (7 day expiry)
- Refresh tokens stored in database for revocation support
- Password hashing: Argon2id via `argon2` crate
- OAuth: `oauth2` crate for flow management

### 7.2 RBAC Service

**Responsibility:** Role management, permission evaluation, access control

```rust
#[async_trait]
pub trait RbacService: Send + Sync {
    // Permission checking
    async fn check_permission(
        &self,
        user_id: UserId,
        permission: Permission,
        resource: Option<ResourceContext>,
    ) -> Result<bool, RbacError>;
    
    async fn get_user_permissions(
        &self,
        user_id: UserId,
        resource: Option<ResourceContext>,
    ) -> Result<HashSet<Permission>, RbacError>;
    
    // Role management
    async fn create_role(&self, req: CreateRoleRequest) -> Result<Role, RbacError>;
    async fn update_role(&self, role_id: RoleId, req: UpdateRoleRequest) -> Result<Role, RbacError>;
    async fn delete_role(&self, role_id: RoleId) -> Result<(), RbacError>;
    async fn list_roles(&self) -> Result<Vec<Role>, RbacError>;
    
    // User role assignment
    async fn assign_role(
        &self,
        user_id: UserId,
        role_id: RoleId,
        scope: RoleScope,
    ) -> Result<(), RbacError>;
    
    async fn revoke_role(
        &self,
        user_id: UserId,
        role_id: RoleId,
        scope: RoleScope,
    ) -> Result<(), RbacError>;
    
    async fn get_user_roles(&self, user_id: UserId) -> Result<Vec<UserRole>, RbacError>;
}

// Resource-scoped permissions
pub enum RoleScope {
    Global,
    Game(GameId),
    Team(TeamId),
    Tournament(TournamentId),
    League(LeagueId),
}
```

**Permission Hierarchy:**

```
Platform Admin
├── Manage all games
├── Manage all users
├── Manage all tournaments
└── System configuration

Game Admin (per game)
├── Manage game plugins
├── Manage game queues
└── Game-specific moderation

Tournament Admin (per tournament)
├── Manage tournament settings
├── Manage participants
├── Manage brackets
└── Handle disputes

Team Owner (per team)
├── Manage roster
├── Manage team settings
└── Register for tournaments

Team Captain (per team)
├── Lobby leadership
├── Map picks/bans
└── Ready checks

Player (global)
├── Join queues
├── Join lobbies
├── View stats
└── Participate in matches
```

### 7.3 Players/Teams Service

**Responsibility:** Player profiles, team management, statistics, relationships

```rust
#[async_trait]
pub trait PlayerService: Send + Sync {
    // Profile management
    async fn get_player(&self, player_id: PlayerId) -> Result<Player, PlayerError>;
    async fn update_profile(&self, player_id: PlayerId, req: UpdateProfileRequest) -> Result<Player, PlayerError>;
    async fn search_players(&self, query: &str, pagination: Pagination) -> Result<Page<PlayerSummary>, PlayerError>;
    
    // Statistics
    async fn get_player_stats(&self, player_id: PlayerId, game_id: Option<GameId>) -> Result<PlayerStats, PlayerError>;
    async fn get_match_history(&self, player_id: PlayerId, filter: MatchHistoryFilter) -> Result<Page<MatchSummary>, PlayerError>;
    async fn get_ranking(&self, player_id: PlayerId, game_id: GameId) -> Result<PlayerRanking, PlayerError>;
    
    // Social
    async fn get_friends(&self, player_id: PlayerId) -> Result<Vec<Friend>, PlayerError>;
    async fn send_friend_request(&self, from: PlayerId, to: PlayerId) -> Result<(), PlayerError>;
    async fn accept_friend_request(&self, player_id: PlayerId, request_id: RequestId) -> Result<(), PlayerError>;
    async fn remove_friend(&self, player_id: PlayerId, friend_id: PlayerId) -> Result<(), PlayerError>;
    async fn block_player(&self, player_id: PlayerId, blocked_id: PlayerId) -> Result<(), PlayerError>;
}

#[async_trait]
pub trait TeamService: Send + Sync {
    // Team CRUD
    async fn create_team(&self, owner_id: PlayerId, req: CreateTeamRequest) -> Result<Team, TeamError>;
    async fn get_team(&self, team_id: TeamId) -> Result<Team, TeamError>;
    async fn update_team(&self, team_id: TeamId, req: UpdateTeamRequest) -> Result<Team, TeamError>;
    async fn delete_team(&self, team_id: TeamId) -> Result<(), TeamError>;
    async fn search_teams(&self, query: &str, pagination: Pagination) -> Result<Page<TeamSummary>, TeamError>;
    
    // Roster management
    async fn get_roster(&self, team_id: TeamId) -> Result<Vec<TeamMember>, TeamError>;
    async fn invite_player(&self, team_id: TeamId, player_id: PlayerId, role: TeamRole) -> Result<Invitation, TeamError>;
    async fn accept_invitation(&self, invitation_id: InvitationId) -> Result<(), TeamError>;
    async fn remove_member(&self, team_id: TeamId, player_id: PlayerId) -> Result<(), TeamError>;
    async fn update_member_role(&self, team_id: TeamId, player_id: PlayerId, role: TeamRole) -> Result<(), TeamError>;
    
    // Team stats
    async fn get_team_stats(&self, team_id: TeamId, game_id: Option<GameId>) -> Result<TeamStats, TeamError>;
    async fn get_team_match_history(&self, team_id: TeamId, filter: MatchHistoryFilter) -> Result<Page<MatchSummary>, TeamError>;
}
```

### 7.4 Matchmaking Service

**Responsibility:** Queue management, match creation, player grouping

```rust
#[async_trait]
pub trait MatchmakingService: Send + Sync {
    // Queue operations
    async fn join_queue(
        &self,
        game_id: GameId,
        queue_id: QueueId,
        players: Vec<PlayerId>,
        preferences: serde_json::Value,
    ) -> Result<QueueTicket, MatchmakingError>;
    
    async fn leave_queue(&self, ticket: QueueTicket) -> Result<(), MatchmakingError>;
    
    async fn get_queue_status(&self, ticket: QueueTicket) -> Result<QueueStatus, MatchmakingError>;
    
    async fn get_estimated_wait(&self, game_id: GameId, queue_id: QueueId) -> Result<Duration, MatchmakingError>;
    
    // Queue management (admin)
    async fn create_queue(&self, game_id: GameId, config: QueueConfig) -> Result<Queue, MatchmakingError>;
    async fn update_queue(&self, queue_id: QueueId, config: QueueConfig) -> Result<Queue, MatchmakingError>;
    async fn pause_queue(&self, queue_id: QueueId) -> Result<(), MatchmakingError>;
    async fn resume_queue(&self, queue_id: QueueId) -> Result<(), MatchmakingError>;
    
    // Match acceptance
    async fn accept_match(&self, ticket: QueueTicket) -> Result<(), MatchmakingError>;
    async fn decline_match(&self, ticket: QueueTicket) -> Result<(), MatchmakingError>;
}

// Internal matchmaking loop
pub struct MatchmakingWorker {
    queues: Arc<QueueManager>,
    plugins: Arc<PluginRegistry>,
    lobby_service: Arc<dyn LobbyService>,
}

impl MatchmakingWorker {
    pub async fn run(&self) {
        loop {
            for queue in self.queues.active_queues().await {
                if let Some(match_found) = self.try_create_match(&queue).await {
                    self.initiate_match_acceptance(match_found).await;
                }
            }
            
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    
    async fn try_create_match(&self, queue: &Queue) -> Option<PendingMatch> {
        let plugin = self.plugins.get(&queue.game_id)?;
        let adapter = plugin.matchmaking_adapter();
        
        let candidates = self.queues.get_candidates(&queue.id).await;
        
        // Use plugin's matchmaking logic
        adapter.find_match(candidates, &queue.config)
    }
}
```

### 7.5 Tournament/League Engine

**Responsibility:** Tournament structures, scheduling, bracket progression

```rust
#[async_trait]
pub trait TournamentService: Send + Sync {
    // Tournament CRUD
    async fn create_tournament(&self, req: CreateTournamentRequest) -> Result<Tournament, TournamentError>;
    async fn get_tournament(&self, id: TournamentId) -> Result<Tournament, TournamentError>;
    async fn update_tournament(&self, id: TournamentId, req: UpdateTournamentRequest) -> Result<Tournament, TournamentError>;
    async fn delete_tournament(&self, id: TournamentId) -> Result<(), TournamentError>;
    
    // Registration
    async fn register_participant(&self, tournament_id: TournamentId, participant: ParticipantRegistration) -> Result<(), TournamentError>;
    async fn withdraw_participant(&self, tournament_id: TournamentId, participant_id: ParticipantId) -> Result<(), TournamentError>;
    async fn seed_participants(&self, tournament_id: TournamentId, seeds: Vec<Seed>) -> Result<(), TournamentError>;
    
    // Bracket management
    async fn generate_bracket(&self, tournament_id: TournamentId) -> Result<Bracket, TournamentError>;
    async fn get_bracket(&self, tournament_id: TournamentId) -> Result<Bracket, TournamentError>;
    async fn report_match_result(&self, match_id: BracketMatchId, result: MatchResult) -> Result<(), TournamentError>;
    async fn advance_bracket(&self, tournament_id: TournamentId) -> Result<(), TournamentError>;
    
    // Scheduling
    async fn schedule_match(&self, match_id: BracketMatchId, scheduled_time: DateTime<Utc>) -> Result<(), TournamentError>;
    async fn reschedule_match(&self, match_id: BracketMatchId, new_time: DateTime<Utc>) -> Result<(), TournamentError>;
}

// Bracket formats
pub enum BracketFormat {
    SingleElimination,
    DoubleElimination,
    RoundRobin,
    Swiss { rounds: u8 },
    GroupStage { groups: u8, advance_per_group: u8 },
}

// League/Season management
#[async_trait]
pub trait LeagueService: Send + Sync {
    async fn create_league(&self, req: CreateLeagueRequest) -> Result<League, LeagueError>;
    async fn create_season(&self, league_id: LeagueId, req: CreateSeasonRequest) -> Result<Season, LeagueError>;
    async fn get_standings(&self, season_id: SeasonId) -> Result<Standings, LeagueError>;
    async fn get_schedule(&self, season_id: SeasonId) -> Result<Vec<ScheduledMatch>, LeagueError>;
}
```

### 7.6 Lobby Service

**Responsibility:** Real-time game sessions, WebSocket management

```rust
#[async_trait]
pub trait LobbyService: Send + Sync {
    // Lobby lifecycle
    async fn create_lobby(&self, game_id: GameId, config: LobbyConfig) -> Result<Lobby, LobbyError>;
    async fn get_lobby(&self, lobby_id: LobbyId) -> Result<Lobby, LobbyError>;
    async fn close_lobby(&self, lobby_id: LobbyId) -> Result<(), LobbyError>;
    
    // Player management
    async fn join_lobby(&self, lobby_id: LobbyId, player_id: PlayerId) -> Result<JoinToken, LobbyError>;
    async fn leave_lobby(&self, lobby_id: LobbyId, player_id: PlayerId) -> Result<(), LobbyError>;
    async fn kick_player(&self, lobby_id: LobbyId, player_id: PlayerId) -> Result<(), LobbyError>;
    
    // State queries
    async fn get_lobby_state(&self, lobby_id: LobbyId) -> Result<LobbyState, LobbyError>;
    async fn list_player_lobbies(&self, player_id: PlayerId) -> Result<Vec<LobbySummary>, LobbyError>;
    
    // WebSocket connection
    async fn connect(&self, lobby_id: LobbyId, player_id: PlayerId, socket: WebSocket) -> Result<(), LobbyError>;
}
```

### 7.7 Plugin Manager

**Responsibility:** Plugin registration, lifecycle, dispatch

```rust
pub struct PluginManager {
    plugins: RwLock<HashMap<GameId, Arc<dyn GamePlugin>>>,
    core_version: Version,
    db_pool: PgPool,
}

impl PluginManager {
    pub async fn register_plugin(&self, plugin: Box<dyn GamePlugin>) -> Result<(), PluginError> {
        // Version compatibility check
        if !plugin.required_core_version().matches(&self.core_version) {
            return Err(PluginError::IncompatibleVersion { /* ... */ });
        }
        
        // Run plugin migrations
        self.run_plugin_migrations(&plugin).await?;
        
        // Create plugin context
        let ctx = PluginContext::new(&self.db_pool, plugin.game_id());
        
        // Initialize plugin
        plugin.on_load(&ctx).await?;
        
        // Register
        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin.game_id(), Arc::from(plugin));
        
        Ok(())
    }
    
    pub async fn get_plugin(&self, game_id: &GameId) -> Option<Arc<dyn GamePlugin>> {
        self.plugins.read().await.get(game_id).cloned()
    }
    
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins.read().await
            .values()
            .map(|p| PluginInfo {
                game_id: p.game_id(),
                name: p.display_name().to_string(),
                version: p.version(),
            })
            .collect()
    }
    
    pub async fn dispatch<F, R>(&self, game_id: &GameId, f: F) -> Result<R, PluginError>
    where
        F: FnOnce(&dyn GamePlugin) -> R,
    {
        let plugin = self.get_plugin(game_id).await
            .ok_or(PluginError::NotFound(game_id.clone()))?;
        Ok(f(plugin.as_ref()))
    }
}
```

### 7.8 Admin Service

**Responsibility:** Platform configuration, moderation, analytics

```rust
#[async_trait]
pub trait AdminService: Send + Sync {
    // Game management
    async fn list_games(&self) -> Result<Vec<GameInfo>, AdminError>;
    async fn enable_game(&self, game_id: GameId) -> Result<(), AdminError>;
    async fn disable_game(&self, game_id: GameId) -> Result<(), AdminError>;
    async fn update_game_config(&self, game_id: GameId, config: GameConfig) -> Result<(), AdminError>;
    
    // User moderation
    async fn ban_user(&self, user_id: UserId, req: BanRequest) -> Result<Ban, AdminError>;
    async fn unban_user(&self, user_id: UserId) -> Result<(), AdminError>;
    async fn get_user_bans(&self, user_id: UserId) -> Result<Vec<Ban>, AdminError>;
    async fn list_bans(&self, filter: BanFilter, pagination: Pagination) -> Result<Page<Ban>, AdminError>;
    
    // Platform settings
    async fn get_settings(&self) -> Result<PlatformSettings, AdminError>;
    async fn update_settings(&self, settings: PlatformSettings) -> Result<(), AdminError>;
    
    // Analytics
    async fn get_platform_stats(&self) -> Result<PlatformStats, AdminError>;
    async fn get_game_stats(&self, game_id: GameId) -> Result<GameStats, AdminError>;
    async fn get_active_users(&self, period: TimePeriod) -> Result<ActiveUsersStats, AdminError>;
}
```

---

## 8. Data Design

### 8.0 Core Data Model Relationships

This section describes the key relationships in the platform's data model.

**Players & Teams:**
- A **Player** can belong to **multiple Teams** simultaneously (via `team_members`)
- Teams are created by players; the creator becomes a **captain** with `is_founder=true`
- **Captains** are the team admin role - they can invite/remove members, promote others to captain
- Multiple captains can exist per team, but founders cannot be demoted

**Games & Statistics:**
- Each **Game** has a plugin that defines game-specific statistics and map pools
- **Players** have per-game profiles (`player_game_profiles`) containing:
  - Platform-managed: Glicko-2 rating, match statistics
  - Plugin-managed: Game-specific stats (e.g., K/D for FPS, APM for RTS)
- The plugin's `calculate_player_stats()` is called after each match

**Leagues & Divisions:**
- **Leagues** are game-specific and can represent divisions (Division 1, Division 2)
- Leagues have an `access_type`: `open`, `invite_only`, or `application`
- Players must be **league members** (`league_members`) to participate in league activities
- League admins (`membership_type='admin'`) can create league-specific tournaments

**Tournaments:**
- **Global tournaments**: Created by platform admins, open to all players (`league_id=NULL`)
- **League tournaments**: Created by league admins, restricted to league members
- Multiple tournaments can run **concurrently**
- Tournament `map_pool` can be customized if `plugin.supports_custom_map_pool()` returns true

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        CORE DATA MODEL RELATIONSHIPS                         │
└─────────────────────────────────────────────────────────────────────────────┘

PLAYERS & TEAMS (M:N - players can be on multiple teams)
─────────────────────────────────────────────────────────
Player ──< team_members >── Team
           │
           ├── role: captain | officer | player | substitute | coach
           ├── is_founder: bool (creator cannot be demoted)
           └── status: active | inactive | benched

GAMES & PLAYER STATISTICS (per-game ELO/stats)
──────────────────────────────────────────────
Player ──< player_game_profiles >── Game
           │
           ├── rating (Glicko-2, platform-managed)
           ├── rating_deviation
           ├── volatility
           └── game_specific_stats (JSONB, plugin-defined)
                   │
                   └── Plugin.calculate_player_stats() updates after each match

LEAGUES & MEMBERSHIP (players can be in multiple leagues)
─────────────────────────────────────────────────────────
                    ┌── game_id (required, leagues are game-specific)
                    │
League ────────────<├── division (1, 2, 3... for tiers)
    │               ├── access_type (open | invite_only | application)
    │               └── default_map_pool (JSONB)
    │
    └──< league_members >── Player
           │
           ├── membership_type: player | admin | moderator
           └── status: pending | active | suspended

TOURNAMENTS (global vs league-specific)
───────────────────────────────────────
                         ┌── league_id = NULL → Global (platform admin creates)
                         │
Tournament ─────────────<├── league_id = X → League-specific (league admin creates)
    │                    │                   └── Only league members can register
    │                    ├── map_pool (JSONB, can override league default)
    │                    └── Multiple tournaments can run concurrently
    │
    └──< tournament_participants >── Team/Player

PERMISSIONS (RBAC with scoped access)
────────────────────────────────────
User ──< user_roles >── Role ──< role_permissions >── Permission
                         │
                         └── scope: Global | Game | League | Team | Tournament
```

### 8.1 High-Level Entity Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ENTITY RELATIONSHIP DIAGRAM                        │
└─────────────────────────────────────────────────────────────────────────────┘

┌──────────┐       ┌──────────────┐       ┌──────────────┐
│   User   │───────│  UserRole    │───────│    Role      │
└────┬─────┘       └──────────────┘       └──────────────┘
     │                                           │
     │ 1:1                                       │
     ▼                                           ▼
┌──────────┐                             ┌──────────────┐
│  Player  │                             │RolePermission│
└────┬─────┘                             └──────────────┘
     │                                           │
     │ M:N                                       │
     ▼                                           ▼
┌──────────────┐                         ┌──────────────┐
│  TeamMember  │                         │  Permission  │
└──────┬───────┘                         └──────────────┘
       │
       │ M:1
       ▼
┌──────────┐       ┌──────────────┐       ┌──────────────┐
│   Team   │───────│ TeamInvite   │       │    Game      │
└────┬─────┘       └──────────────┘       └──────┬───────┘
     │                                           │
     │                                           │
     ▼                                           ▼
┌──────────────┐   ┌──────────────┐       ┌──────────────┐
│    Match     │───│MatchPlayer   │       │ MatchQueue   │
└──────┬───────┘   └──────────────┘       └──────────────┘
       │
       │
       ▼
┌──────────────┐   ┌──────────────┐       ┌──────────────┐
│    Lobby     │───│ LobbyPlayer  │       │  PluginData  │
└──────────────┘   └──────────────┘       └──────────────┘

┌──────────────┐   ┌──────────────┐       ┌──────────────┐
│   League     │───│   Season     │───────│  Standing    │
└──────────────┘   └──────────────┘       └──────────────┘
       │
       ▼
┌──────────────┐   ┌──────────────┐       ┌──────────────┐
│ Tournament   │───│   Bracket    │───────│ BracketMatch │
└──────────────┘   └──────────────┘       └──────────────┘
```

### 8.2 Essential Tables in PostgreSQL

```sql
-- ============================================
-- Core User & Auth Tables
-- ============================================

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(32) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255),  -- NULL for OAuth-only users
    email_verified BOOLEAN DEFAULT FALSE,
    two_factor_enabled BOOLEAN DEFAULT FALSE,
    two_factor_secret VARCHAR(255),
    status VARCHAR(20) DEFAULT 'active',  -- active, suspended, banned
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE oauth_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(32) NOT NULL,  -- steam, discord, twitch, google
    provider_user_id VARCHAR(255) NOT NULL,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(provider, provider_user_id)
);

CREATE TABLE refresh_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL,
    device_info JSONB,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- ============================================
-- RBAC Tables
-- ============================================

CREATE TABLE roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(64) UNIQUE NOT NULL,
    description TEXT,
    is_system BOOLEAN DEFAULT FALSE,  -- System roles cannot be deleted
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(64) UNIQUE NOT NULL,
    description TEXT,
    category VARCHAR(32)  -- auth, player, team, tournament, admin
);

CREATE TABLE role_permissions (
    role_id UUID REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID REFERENCES roles(id) ON DELETE CASCADE,
    scope_type VARCHAR(32),  -- NULL for global, 'team', 'tournament', 'game'
    scope_id UUID,           -- ID of scoped resource
    granted_by UUID REFERENCES users(id),
    granted_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    UNIQUE(user_id, role_id, scope_type, scope_id)
);

-- ============================================
-- Player & Team Tables
-- ============================================

CREATE TABLE players (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    display_name VARCHAR(32) NOT NULL,
    avatar_url VARCHAR(512),
    bio TEXT,
    country_code CHAR(2),
    timezone VARCHAR(64),
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE player_game_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    game_id VARCHAR(32) NOT NULL,
    rating INTEGER DEFAULT 1500,
    rating_deviation INTEGER DEFAULT 350,
    volatility DECIMAL(10, 8) DEFAULT 0.06,
    rank_tier VARCHAR(32),
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    draws INTEGER DEFAULT 0,
    game_specific_stats JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(player_id, game_id)
);

CREATE TABLE teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(64) NOT NULL,
    tag VARCHAR(5) NOT NULL,
    description TEXT,
    logo_url VARCHAR(512),
    owner_id UUID REFERENCES players(id),
    game_id VARCHAR(32),  -- NULL for multi-game teams
    status VARCHAR(20) DEFAULT 'active',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    role VARCHAR(32) NOT NULL,  -- owner, captain, player, sub, coach
    joined_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(team_id, player_id)
);

CREATE TABLE team_invitations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    invited_by UUID REFERENCES players(id),
    role VARCHAR(32) NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',  -- pending, accepted, declined, expired
    message TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ DEFAULT NOW() + INTERVAL '7 days'
);

-- ============================================
-- Game & Match Tables
-- ============================================

CREATE TABLE games (
    id VARCHAR(32) PRIMARY KEY,
    display_name VARCHAR(64) NOT NULL,
    description TEXT,
    icon_url VARCHAR(512),
    banner_url VARCHAR(512),
    status VARCHAR(20) DEFAULT 'active',
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE match_queues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id VARCHAR(32) REFERENCES games(id),
    name VARCHAR(64) NOT NULL,
    description TEXT,
    queue_type VARCHAR(32) NOT NULL,  -- ranked, unranked, pug
    team_size INTEGER NOT NULL,
    config JSONB NOT NULL,
    status VARCHAR(20) DEFAULT 'active',  -- active, paused, disabled
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id VARCHAR(32) REFERENCES games(id),
    match_type VARCHAR(32) NOT NULL,  -- queue, tournament, scrim
    source_id UUID,  -- queue_id or tournament_match_id
    status VARCHAR(20) DEFAULT 'pending',  -- pending, lobby, active, completed, cancelled
    scheduled_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    result JSONB,  -- winner, scores, etc.
    metadata JSONB,  -- Game-specific match data
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_matches_game_status ON matches(game_id, status);
CREATE INDEX idx_matches_created_at ON matches(created_at DESC);

CREATE TABLE match_players (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID REFERENCES matches(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id),
    team_id UUID REFERENCES teams(id),
    team_slot INTEGER,  -- 0 or 1
    stats JSONB,  -- Game-specific player stats
    rating_before INTEGER,
    rating_after INTEGER,
    rating_change INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_match_players_player ON match_players(player_id);
CREATE INDEX idx_match_players_match ON match_players(match_id);

-- ============================================
-- Lobby Tables
-- ============================================

CREATE TABLE lobbies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id VARCHAR(32) REFERENCES games(id),
    match_id UUID REFERENCES matches(id),
    status VARCHAR(20) DEFAULT 'waiting',  -- waiting, picking, ready, started, closed
    config JSONB NOT NULL,
    plugin_state JSONB,  -- Game-specific lobby state
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    closed_at TIMESTAMPTZ
);

CREATE TABLE lobby_players (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    lobby_id UUID REFERENCES lobbies(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id),
    team_slot INTEGER,
    is_ready BOOLEAN DEFAULT FALSE,
    is_captain BOOLEAN DEFAULT FALSE,
    connected BOOLEAN DEFAULT TRUE,
    joined_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(lobby_id, player_id)
);

-- ============================================
-- Tournament & League Tables
-- ============================================

CREATE TABLE leagues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(128) NOT NULL,
    description TEXT,
    game_id VARCHAR(32) REFERENCES games(id),
    logo_url VARCHAR(512),
    owner_id UUID REFERENCES users(id),
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID REFERENCES leagues(id) ON DELETE CASCADE,
    name VARCHAR(128) NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    registration_opens TIMESTAMPTZ,
    registration_closes TIMESTAMPTZ,
    status VARCHAR(20) DEFAULT 'upcoming',  -- upcoming, registration, active, playoffs, completed
    format VARCHAR(32) NOT NULL,  -- round_robin, swiss, ladder
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE season_standings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID REFERENCES seasons(id) ON DELETE CASCADE,
    participant_type VARCHAR(20) NOT NULL,  -- player, team
    participant_id UUID NOT NULL,
    points INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    draws INTEGER DEFAULT 0,
    tiebreaker_1 INTEGER DEFAULT 0,
    tiebreaker_2 INTEGER DEFAULT 0,
    rank INTEGER,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(season_id, participant_id)
);

CREATE TABLE tournaments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(128) NOT NULL,
    description TEXT,
    game_id VARCHAR(32) REFERENCES games(id),
    league_id UUID REFERENCES leagues(id),
    season_id UUID REFERENCES seasons(id),
    format VARCHAR(32) NOT NULL,  -- single_elim, double_elim, round_robin, swiss
    participant_type VARCHAR(20) NOT NULL,  -- player, team
    team_size INTEGER,
    max_participants INTEGER,
    registration_opens TIMESTAMPTZ,
    registration_closes TIMESTAMPTZ,
    starts_at TIMESTAMPTZ,
    status VARCHAR(20) DEFAULT 'draft',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tournament_participants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,
    participant_type VARCHAR(20) NOT NULL,
    participant_id UUID NOT NULL,
    seed INTEGER,
    checked_in BOOLEAN DEFAULT FALSE,
    status VARCHAR(20) DEFAULT 'registered',  -- registered, confirmed, eliminated, winner
    registered_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tournament_id, participant_id)
);

CREATE TABLE brackets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tournament_id UUID REFERENCES tournaments(id) ON DELETE CASCADE,
    bracket_type VARCHAR(32) NOT NULL,  -- winners, losers, grand_final
    structure JSONB NOT NULL,  -- Bracket structure definition
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE bracket_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID REFERENCES brackets(id) ON DELETE CASCADE,
    match_id UUID REFERENCES matches(id),
    round INTEGER NOT NULL,
    position INTEGER NOT NULL,
    participant_1_id UUID,
    participant_2_id UUID,
    winner_id UUID,
    scheduled_at TIMESTAMPTZ,
    best_of INTEGER DEFAULT 1,
    scores JSONB,
    status VARCHAR(20) DEFAULT 'pending',
    next_match_id UUID REFERENCES bracket_matches(id),
    loser_next_match_id UUID REFERENCES bracket_matches(id)
);

-- ============================================
-- Plugin Data Tables
-- ============================================

CREATE TABLE plugin_data (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id VARCHAR(32) NOT NULL,
    entity_type VARCHAR(64) NOT NULL,  -- player_stats, match_metadata, lobby_state, etc.
    entity_id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(game_id, entity_type, entity_id)
);

CREATE INDEX idx_plugin_data_lookup ON plugin_data(game_id, entity_type, entity_id);

-- ============================================
-- Audit & System Tables
-- ============================================

CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    request_id VARCHAR(64),
    user_id UUID REFERENCES users(id),
    action VARCHAR(64) NOT NULL,
    resource_type VARCHAR(64),
    resource_id UUID,
    ip_address INET,
    user_agent TEXT,
    outcome VARCHAR(20) NOT NULL,  -- success, failure, error
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_user ON audit_logs(user_id, timestamp DESC);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id, timestamp DESC);

CREATE TABLE bans (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    banned_by UUID REFERENCES users(id),
    ban_type VARCHAR(32) NOT NULL,  -- platform, game, tournament
    scope_id UUID,  -- game_id or tournament_id for scoped bans
    reason TEXT NOT NULL,
    evidence TEXT,
    starts_at TIMESTAMPTZ DEFAULT NOW(),
    ends_at TIMESTAMPTZ,  -- NULL for permanent
    lifted_at TIMESTAMPTZ,
    lifted_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- ============================================
-- Indexes for Performance
-- ============================================

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_players_display_name ON players(display_name);
CREATE INDEX idx_teams_game ON teams(game_id);
CREATE INDEX idx_match_players_composite ON match_players(player_id, match_id);
CREATE INDEX idx_lobbies_game_status ON lobbies(game_id, status);
CREATE INDEX idx_tournaments_game_status ON tournaments(game_id, status);
```

### 8.3 Using SQLx

```rust
// Example repository using SQLx
pub struct PlayerRepository {
    pool: PgPool,
}

impl PlayerRepository {
    pub async fn get_by_id(&self, id: PlayerId) -> Result<Option<Player>, DbError> {
        sqlx::query_as!(
            Player,
            r#"
            SELECT 
                id,
                user_id,
                display_name,
                avatar_url,
                bio,
                country_code,
                timezone,
                settings as "settings: Json<PlayerSettings>",
                created_at,
                updated_at
            FROM players
            WHERE id = $1
            "#,
            id.0
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }
    
    pub async fn get_game_profile(
        &self,
        player_id: PlayerId,
        game_id: &str,
    ) -> Result<Option<PlayerGameProfile>, DbError> {
        sqlx::query_as!(
            PlayerGameProfile,
            r#"
            SELECT 
                id,
                player_id,
                game_id,
                rating,
                rating_deviation,
                volatility,
                rank_tier,
                wins,
                losses,
                draws,
                game_specific_stats as "game_specific_stats: Json<serde_json::Value>",
                created_at,
                updated_at
            FROM player_game_profiles
            WHERE player_id = $1 AND game_id = $2
            "#,
            player_id.0,
            game_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }
    
    pub async fn update_rating(
        &self,
        player_id: PlayerId,
        game_id: &str,
        new_rating: i32,
        new_rd: i32,
        new_volatility: f64,
    ) -> Result<(), DbError> {
        sqlx::query!(
            r#"
            UPDATE player_game_profiles
            SET 
                rating = $3,
                rating_deviation = $4,
                volatility = $5,
                updated_at = NOW()
            WHERE player_id = $1 AND game_id = $2
            "#,
            player_id.0,
            game_id,
            new_rating,
            new_rd,
            new_volatility
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

### 8.4 How Plugins Attach Custom Data

```rust
// Plugin data access pattern
pub struct PluginDataRepository {
    pool: PgPool,
}

impl PluginDataRepository {
    pub async fn get<T: DeserializeOwned>(
        &self,
        game_id: &str,
        entity_type: &str,
        entity_id: Uuid,
    ) -> Result<Option<T>, DbError> {
        let row = sqlx::query!(
            r#"
            SELECT data
            FROM plugin_data
            WHERE game_id = $1 AND entity_type = $2 AND entity_id = $3
            "#,
            game_id,
            entity_type,
            entity_id
        )
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(r) => Ok(Some(serde_json::from_value(r.data)?)),
            None => Ok(None),
        }
    }
    
    pub async fn upsert<T: Serialize>(
        &self,
        game_id: &str,
        entity_type: &str,
        entity_id: Uuid,
        data: &T,
    ) -> Result<(), DbError> {
        let json_data = serde_json::to_value(data)?;
        
        sqlx::query!(
            r#"
            INSERT INTO plugin_data (game_id, entity_type, entity_id, data)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (game_id, entity_type, entity_id)
            DO UPDATE SET data = $4, updated_at = NOW()
            "#,
            game_id,
            entity_type,
            entity_id,
            json_data
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}

// Usage in plugin
impl Cs2Plugin {
    async fn save_match_stats(
        &self,
        ctx: &PluginContext,
        match_id: Uuid,
        stats: &Cs2MatchStats,
    ) -> Result<(), PluginError> {
        ctx.db.upsert("cs2", "match_stats", match_id, stats).await?;
        Ok(())
    }
}
```

### 8.5 Migration/Versioning Strategy

```
migrations/
├── 20240101000000_initial_schema.sql
├── 20240102000000_add_oauth_connections.sql
├── 20240103000000_add_tournaments.sql
└── plugins/
    ├── cs2/
    │   ├── 20240201000000_cs2_initial.sql
    │   └── 20240215000000_cs2_add_demo_storage.sql
    └── aoe4/
        └── 20240201000000_aoe4_initial.sql
```

```rust
// Migration runner
use sqlx::migrate::Migrator;

pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrationError> {
    // Core migrations
    let core_migrator = Migrator::new(Path::new("./migrations")).await?;
    core_migrator.run(pool).await?;
    
    Ok(())
}

// Plugin-specific migrations run during plugin registration
pub async fn run_plugin_migrations(
    pool: &PgPool,
    game_id: &str,
) -> Result<(), MigrationError> {
    let path = format!("./migrations/plugins/{}", game_id);
    if Path::new(&path).exists() {
        let migrator = Migrator::new(Path::new(&path)).await?;
        migrator.run(pool).await?;
    }
    Ok(())
}
```

---

## 9. Substitute & Availability System

This section describes the per-league substitute and availability system, allowing players to opt-in as substitutes and post their availability for league matches.

### 9.1 System Overview

The substitute system enables:

- **Players** to register as available substitutes for specific leagues/seasons
- **Team managers** to find and request substitutes for upcoming matches
- **League admins** to manage substitute pools and policies
- **Automatic matching** of substitute needs with available players

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        SUBSTITUTE SYSTEM FLOW                                │
└─────────────────────────────────────────────────────────────────────────────┘

   Player Posts             Team Needs Sub          System Matches
   Availability             for Match              & Notifies
        │                        │                      │
        ▼                        ▼                      ▼
┌───────────────┐        ┌───────────────┐      ┌───────────────┐
│  Availability │        │  Substitute   │      │   Matching    │
│    Calendar   │───────▶│    Request    │─────▶│    Engine     │
└───────────────┘        └───────────────┘      └───────┬───────┘
        │                        │                      │
        │                        │                      ▼
        │                        │              ┌───────────────┐
        ▼                        ▼              │ Notifications │
┌───────────────┐        ┌───────────────┐      │  to Matching  │
│   Skill &     │        │   Urgency &   │      │    Players    │
│   Preferences │        │  Requirements │      └───────────────┘
└───────────────┘        └───────────────┘              │
                                                        ▼
                                                ┌───────────────┐
                                                │   Confirm &   │
                                                │   Assign Sub  │
                                                └───────────────┘
```

### 9.2 Data Model

```sql
-- ============================================
-- Substitute Pool Tables
-- ============================================

-- Players registered as potential substitutes for a league/season
CREATE TABLE league_substitute_pool (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID REFERENCES leagues(id) ON DELETE CASCADE,
    season_id UUID REFERENCES seasons(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    
    -- Player preferences
    preferred_roles JSONB DEFAULT '[]',        -- e.g., ["support", "entry", "awp"]
    preferred_positions JSONB DEFAULT '[]',    -- Game-specific positions
    min_notice_hours INTEGER DEFAULT 24,       -- Minimum notice required
    max_games_per_week INTEGER,                -- Limit on commitments
    
    -- Skill information
    skill_rating INTEGER,                      -- Rating in this game
    rank_tier VARCHAR(32),
    verified_by_admin BOOLEAN DEFAULT FALSE,
    
    -- Status
    status VARCHAR(20) DEFAULT 'active',       -- active, inactive, suspended
    games_played_as_sub INTEGER DEFAULT 0,
    reliability_score DECIMAL(3,2) DEFAULT 1.0, -- 0.0 to 1.0
    
    -- Metadata
    notes TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    
    UNIQUE(season_id, player_id)
);

CREATE INDEX idx_sub_pool_season ON league_substitute_pool(season_id, status);
CREATE INDEX idx_sub_pool_player ON league_substitute_pool(player_id);

-- Player availability windows
CREATE TABLE substitute_availability (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pool_entry_id UUID REFERENCES league_substitute_pool(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    
    -- Time window
    available_date DATE NOT NULL,
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    timezone VARCHAR(64) NOT NULL,
    
    -- Recurrence (optional)
    recurrence_rule VARCHAR(255),             -- iCal RRULE format
    recurrence_end_date DATE,
    
    -- Status
    status VARCHAR(20) DEFAULT 'available',   -- available, tentative, booked, cancelled
    booked_for_match_id UUID REFERENCES matches(id),
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_availability_date ON substitute_availability(available_date, status);
CREATE INDEX idx_availability_player ON substitute_availability(player_id, available_date);

-- Substitute requests from teams
CREATE TABLE substitute_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID REFERENCES teams(id) ON DELETE CASCADE,
    match_id UUID REFERENCES matches(id),
    season_id UUID REFERENCES seasons(id),
    
    -- Request details
    requested_by UUID REFERENCES players(id),
    reason VARCHAR(255),
    urgency VARCHAR(20) DEFAULT 'normal',     -- low, normal, high, emergency
    
    -- Requirements
    required_roles JSONB DEFAULT '[]',
    min_skill_rating INTEGER,
    max_skill_rating INTEGER,
    specific_requirements TEXT,
    
    -- Match timing
    match_scheduled_at TIMESTAMPTZ NOT NULL,
    response_deadline TIMESTAMPTZ NOT NULL,
    
    -- Status tracking
    status VARCHAR(20) DEFAULT 'open',        -- open, filled, cancelled, expired
    filled_by UUID REFERENCES players(id),
    filled_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_sub_requests_status ON substitute_requests(status, match_scheduled_at);
CREATE INDEX idx_sub_requests_season ON substitute_requests(season_id, status);

-- Substitute request responses/applications
CREATE TABLE substitute_request_responses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id UUID REFERENCES substitute_requests(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    
    -- Response
    response_type VARCHAR(20) NOT NULL,       -- apply, decline, maybe
    message TEXT,
    
    -- Team decision
    team_decision VARCHAR(20),                -- pending, accepted, rejected
    decision_by UUID REFERENCES players(id),
    decision_at TIMESTAMPTZ,
    decision_reason TEXT,
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    UNIQUE(request_id, player_id)
);

-- Substitute assignment history
CREATE TABLE substitute_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID REFERENCES matches(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id),
    original_player_id UUID REFERENCES players(id),
    substitute_player_id UUID REFERENCES players(id),
    request_id UUID REFERENCES substitute_requests(id),
    
    -- Assignment details
    assigned_at TIMESTAMPTZ DEFAULT NOW(),
    assigned_by UUID REFERENCES players(id),
    
    -- Outcome tracking
    substitute_showed BOOLEAN,
    performance_rating INTEGER,               -- 1-5 rating by team
    team_feedback TEXT,
    substitute_feedback TEXT,
    
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_sub_assignments_match ON substitute_assignments(match_id);
CREATE INDEX idx_sub_assignments_sub ON substitute_assignments(substitute_player_id);
```

### 9.3 Service Interface

```rust
#[async_trait]
pub trait SubstituteService: Send + Sync {
    // === Substitute Pool Management ===
    
    /// Register as a substitute for a league/season
    async fn register_as_substitute(
        &self,
        player_id: PlayerId,
        season_id: SeasonId,
        preferences: SubstitutePreferences,
    ) -> Result<PoolEntry, SubstituteError>;
    
    /// Update substitute registration
    async fn update_registration(
        &self,
        pool_entry_id: PoolEntryId,
        preferences: SubstitutePreferences,
    ) -> Result<PoolEntry, SubstituteError>;
    
    /// Withdraw from substitute pool
    async fn withdraw_from_pool(
        &self,
        player_id: PlayerId,
        season_id: SeasonId,
    ) -> Result<(), SubstituteError>;
    
    /// Get player's substitute registrations
    async fn get_player_registrations(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<PoolEntry>, SubstituteError>;
    
    // === Availability Management ===
    
    /// Post availability window
    async fn post_availability(
        &self,
        player_id: PlayerId,
        season_id: SeasonId,
        availability: AvailabilityWindow,
    ) -> Result<Availability, SubstituteError>;
    
    /// Update availability
    async fn update_availability(
        &self,
        availability_id: AvailabilityId,
        updates: AvailabilityUpdate,
    ) -> Result<Availability, SubstituteError>;
    
    /// Cancel availability
    async fn cancel_availability(
        &self,
        availability_id: AvailabilityId,
    ) -> Result<(), SubstituteError>;
    
    /// Get player's availability for date range
    async fn get_availability(
        &self,
        player_id: PlayerId,
        season_id: SeasonId,
        from: Date,
        to: Date,
    ) -> Result<Vec<Availability>, SubstituteError>;
    
    /// Bulk set recurring availability
    async fn set_recurring_availability(
        &self,
        player_id: PlayerId,
        season_id: SeasonId,
        schedule: RecurringSchedule,
    ) -> Result<Vec<Availability>, SubstituteError>;
    
    // === Substitute Requests ===
    
    /// Create substitute request for a match
    async fn create_request(
        &self,
        team_id: TeamId,
        match_id: MatchId,
        request: SubstituteRequestInput,
    ) -> Result<SubstituteRequest, SubstituteError>;
    
    /// Find available substitutes for a request
    async fn find_available_substitutes(
        &self,
        request_id: RequestId,
    ) -> Result<Vec<AvailableSubstitute>, SubstituteError>;
    
    /// Respond to a substitute request
    async fn respond_to_request(
        &self,
        request_id: RequestId,
        player_id: PlayerId,
        response: RequestResponse,
    ) -> Result<(), SubstituteError>;
    
    /// Accept a substitute for the request
    async fn accept_substitute(
        &self,
        request_id: RequestId,
        player_id: PlayerId,
        assigned_by: PlayerId,
    ) -> Result<SubstituteAssignment, SubstituteError>;
    
    /// Cancel a substitute request
    async fn cancel_request(
        &self,
        request_id: RequestId,
    ) -> Result<(), SubstituteError>;
    
    // === Queries ===
    
    /// Get open requests for a season
    async fn get_open_requests(
        &self,
        season_id: SeasonId,
        filter: RequestFilter,
    ) -> Result<Vec<SubstituteRequest>, SubstituteError>;
    
    /// Get substitute history for a player
    async fn get_substitute_history(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<SubstituteAssignment>, SubstituteError>;
    
    // === Admin ===
    
    /// Verify a substitute's credentials
    async fn verify_substitute(
        &self,
        pool_entry_id: PoolEntryId,
        verified_by: UserId,
    ) -> Result<(), SubstituteError>;
    
    /// Suspend a substitute from the pool
    async fn suspend_substitute(
        &self,
        pool_entry_id: PoolEntryId,
        reason: &str,
    ) -> Result<(), SubstituteError>;
}
```

### 9.4 Domain Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstitutePreferences {
    pub preferred_roles: Vec<String>,
    pub preferred_positions: Vec<String>,
    pub min_notice_hours: u32,
    pub max_games_per_week: Option<u32>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityWindow {
    pub date: NaiveDate,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub timezone: String,
    pub status: AvailabilityStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AvailabilityStatus {
    Available,
    Tentative,
    Booked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringSchedule {
    pub days_of_week: Vec<Weekday>,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub timezone: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub exceptions: Vec<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstituteRequestInput {
    pub reason: String,
    pub urgency: RequestUrgency,
    pub required_roles: Vec<String>,
    pub skill_range: Option<SkillRange>,
    pub specific_requirements: Option<String>,
    pub response_deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RequestUrgency {
    Low,      // > 1 week notice
    Normal,   // 2-7 days notice
    High,     // 24-48 hours notice
    Emergency // < 24 hours notice
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableSubstitute {
    pub player_id: PlayerId,
    pub display_name: String,
    pub skill_rating: i32,
    pub rank_tier: String,
    pub preferred_roles: Vec<String>,
    pub reliability_score: f64,
    pub games_as_sub: u32,
    pub match_score: f64, // How well they match the request
}
```

### 9.5 Matching Algorithm

```rust
pub struct SubstituteMatchingEngine {
    pool_repository: Arc<dyn SubstitutePoolRepository>,
    availability_repository: Arc<dyn AvailabilityRepository>,
}

impl SubstituteMatchingEngine {
    /// Find and rank available substitutes for a request
    pub async fn find_matches(
        &self,
        request: &SubstituteRequest,
    ) -> Result<Vec<AvailableSubstitute>, SubstituteError> {
        // 1. Get all active pool members for the season
        let pool = self.pool_repository
            .get_active_for_season(request.season_id)
            .await?;
        
        // 2. Filter by availability
        let match_time = request.match_scheduled_at;
        let available_players = self.filter_by_availability(
            &pool,
            match_time,
            request.min_notice_hours(),
        ).await?;
        
        // 3. Filter by skill requirements
        let skill_filtered = self.filter_by_skill(
            available_players,
            request.min_skill_rating,
            request.max_skill_rating,
        );
        
        // 4. Score and rank matches
        let mut scored: Vec<_> = skill_filtered
            .into_iter()
            .map(|p| {
                let score = self.calculate_match_score(&p, request);
                AvailableSubstitute {
                    player_id: p.player_id,
                    display_name: p.display_name,
                    skill_rating: p.skill_rating,
                    rank_tier: p.rank_tier,
                    preferred_roles: p.preferred_roles,
                    reliability_score: p.reliability_score,
                    games_as_sub: p.games_played_as_sub,
                    match_score: score,
                }
            })
            .collect();
        
        // Sort by match score descending
        scored.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap());
        
        Ok(scored)
    }
    
    fn calculate_match_score(
        &self,
        player: &PoolMember,
        request: &SubstituteRequest,
    ) -> f64 {
        let mut score = 0.0;
        
        // Role match (0-30 points)
        let role_matches = player.preferred_roles
            .iter()
            .filter(|r| request.required_roles.contains(r))
            .count();
        if !request.required_roles.is_empty() {
            score += (role_matches as f64 / request.required_roles.len() as f64) * 30.0;
        } else {
            score += 30.0; // No specific role required
        }
        
        // Skill proximity (0-25 points)
        if let (Some(min), Some(max)) = (request.min_skill_rating, request.max_skill_rating) {
            let target = (min + max) / 2;
            let diff = (player.skill_rating - target).abs();
            let range = max - min;
            if range > 0 {
                score += (1.0 - (diff as f64 / range as f64).min(1.0)) * 25.0;
            }
        } else {
            score += 20.0; // No skill requirement
        }
        
        // Reliability score (0-25 points)
        score += player.reliability_score * 25.0;
        
        // Experience as substitute (0-10 points)
        let exp_score = (player.games_played_as_sub as f64).min(20.0) / 2.0;
        score += exp_score;
        
        // Admin verified bonus (0-10 points)
        if player.verified_by_admin {
            score += 10.0;
        }
        
        score
    }
}
```

### 9.6 Notifications

```rust
// Notification events for substitute system
pub enum SubstituteNotification {
    // To potential substitutes
    NewRequestMatchingAvailability {
        player_id: PlayerId,
        request: SubstituteRequest,
        match_time: DateTime<Utc>,
    },
    
    RequestUrgencyIncreased {
        player_id: PlayerId,
        request_id: RequestId,
        new_urgency: RequestUrgency,
    },
    
    // To substitute who applied
    ApplicationAccepted {
        player_id: PlayerId,
        request_id: RequestId,
        match_details: MatchDetails,
    },
    
    ApplicationRejected {
        player_id: PlayerId,
        request_id: RequestId,
        reason: Option<String>,
    },
    
    // To team
    SubstituteApplied {
        team_id: TeamId,
        request_id: RequestId,
        applicant: PlayerSummary,
    },
    
    RequestExpiringSoon {
        team_id: TeamId,
        request_id: RequestId,
        expires_in: Duration,
    },
    
    // To both
    SubstituteAssigned {
        request_id: RequestId,
        substitute: PlayerSummary,
        team: TeamSummary,
        match_time: DateTime<Utc>,
    },
    
    // Post-match
    FeedbackRequested {
        assignment_id: AssignmentId,
        requesting_from: PlayerId,
    },
}
```

### 9.7 API Endpoints

```
/v1/leagues/{league_id}/seasons/{season_id}/substitute-pool
├── GET    /                      # List substitute pool
├── POST   /                      # Register as substitute
├── GET    /me                    # Get own registration
├── PATCH  /me                    # Update registration
├── DELETE /me                    # Withdraw from pool

/v1/substitute/availability
├── GET    /                      # Get my availability
├── POST   /                      # Post availability window
├── POST   /recurring             # Set recurring availability
├── PATCH  /{id}                  # Update availability
├── DELETE /{id}                  # Cancel availability

/v1/substitute/requests
├── GET    /                      # List requests (filtered)
├── POST   /                      # Create request
├── GET    /{id}                  # Get request details
├── GET    /{id}/matches          # Get matching substitutes
├── POST   /{id}/respond          # Respond to request
├── POST   /{id}/accept/{player}  # Accept a substitute
├── DELETE /{id}                  # Cancel request

/v1/substitute/assignments
├── GET    /                      # Get assignment history
├── GET    /{id}                  # Get assignment details
├── POST   /{id}/feedback         # Submit feedback
```

---

## 10. REST API Design Overview

### 10.1 Naming Conventions and Structure

```
Base URL: https://api.gaming-portal.com/v1

Resource Naming:
- Use plural nouns: /players, /teams, /matches
- Use kebab-case for multi-word resources: /match-queues, /bracket-matches
- Nest related resources: /teams/{team_id}/members
- Use query parameters for filtering: /matches?game_id=cs2&status=active

HTTP Methods:
- GET: Retrieve resources
- POST: Create resources
- PUT: Full update (rarely used)
- PATCH: Partial update
- DELETE: Remove resources
```

### 10.2 API Endpoint Structure

```
/v1
├── /auth
│   ├── POST   /register
│   ├── POST   /login
│   ├── POST   /logout
│   ├── POST   /refresh
│   ├── POST   /password/reset-request
│   ├── POST   /password/reset
│   ├── /oauth
│   │   ├── GET    /{provider}/authorize
│   │   └── GET    /{provider}/callback
│   └── /steam                            # Steam OpenID (special handling)
│       ├── GET    /login                 # Redirect to Steam
│       ├── GET    /callback              # Steam callback
│       └── POST   /link                  # Link Steam to existing account
│
├── /users
│   ├── GET    /me
│   ├── PATCH  /me
│   ├── GET    /me/sessions
│   ├── DELETE /me/sessions/{session_id}
│   └── GET    /me/linked-accounts        # View linked OAuth/Steam accounts
│
├── /players
│   ├── GET    /                         # Search players
│   ├── GET    /{player_id}
│   ├── PATCH  /{player_id}              # Update profile (owner only)
│   ├── GET    /{player_id}/stats
│   ├── GET    /{player_id}/matches
│   ├── GET    /{player_id}/teams
│   └── GET    /{player_id}/rankings
│
├── /teams
│   ├── GET    /
│   ├── POST   /
│   ├── GET    /{team_id}
│   ├── PATCH  /{team_id}
│   ├── DELETE /{team_id}
│   ├── GET    /{team_id}/members
│   ├── POST   /{team_id}/members/invite
│   ├── DELETE /{team_id}/members/{player_id}
│   ├── PATCH  /{team_id}/members/{player_id}
│   ├── GET    /{team_id}/invitations
│   ├── POST   /{team_id}/invitations/{id}/accept
│   ├── POST   /{team_id}/invitations/{id}/decline
│   ├── GET    /{team_id}/stats
│   └── GET    /{team_id}/matches
│
├── /games
│   ├── GET    /
│   ├── GET    /{game_id}
│   ├── GET    /{game_id}/queues
│   ├── GET    /{game_id}/leaderboard
│   └── GET    /{game_id}/stats
│
├── /matchmaking
│   ├── POST   /queues/{queue_id}/join
│   ├── DELETE /queues/{queue_id}/leave
│   ├── GET    /tickets/{ticket_id}
│   ├── POST   /tickets/{ticket_id}/accept
│   └── POST   /tickets/{ticket_id}/decline
│
├── /matches
│   ├── GET    /
│   ├── GET    /{match_id}
│   ├── GET    /{match_id}/players
│   └── GET    /{match_id}/stats
│
├── /lobbies
│   ├── POST   /                         # Create lobby (for scrims)
│   ├── GET    /{lobby_id}
│   ├── POST   /{lobby_id}/join
│   ├── POST   /{lobby_id}/leave
│   └── WS     /{lobby_id}/ws            # WebSocket upgrade
│
├── /tournaments
│   ├── GET    /
│   ├── POST   /
│   ├── GET    /{tournament_id}
│   ├── PATCH  /{tournament_id}
│   ├── DELETE /{tournament_id}
│   ├── GET    /{tournament_id}/participants
│   ├── POST   /{tournament_id}/participants
│   ├── DELETE /{tournament_id}/participants/{id}
│   ├── GET    /{tournament_id}/bracket
│   ├── POST   /{tournament_id}/bracket/generate
│   ├── GET    /{tournament_id}/matches
│   └── PATCH  /{tournament_id}/matches/{match_id}
│
├── /leagues
│   ├── GET    /
│   ├── POST   /
│   ├── GET    /{league_id}
│   ├── GET    /{league_id}/seasons
│   ├── POST   /{league_id}/seasons
│   ├── GET    /{league_id}/seasons/{season_id}
│   └── GET    /{league_id}/seasons/{season_id}/standings
│
├── /substitute                           # Substitute system
│   ├── /pool
│   │   ├── GET    /                      # List pools I'm in
│   │   ├── POST   /                      # Register as substitute
│   │   ├── PATCH  /{pool_id}             # Update registration
│   │   └── DELETE /{pool_id}             # Withdraw from pool
│   ├── /availability
│   │   ├── GET    /                      # Get my availability
│   │   ├── POST   /                      # Post availability window
│   │   ├── POST   /recurring             # Set recurring availability
│   │   ├── PATCH  /{id}                  # Update availability
│   │   └── DELETE /{id}                  # Cancel availability
│   ├── /requests
│   │   ├── GET    /                      # List requests (open/mine)
│   │   ├── POST   /                      # Create request
│   │   ├── GET    /{id}                  # Get request details
│   │   ├── GET    /{id}/matches          # Get matching substitutes
│   │   ├── POST   /{id}/respond          # Respond to request
│   │   ├── POST   /{id}/accept/{player}  # Accept a substitute
│   │   └── DELETE /{id}                  # Cancel request
│   └── /assignments
│       ├── GET    /                      # Get assignment history
│       ├── GET    /{id}                  # Get assignment details
│       └── POST   /{id}/feedback         # Submit feedback
│
├── /sagas                                # Saga monitoring
│   ├── GET    /{saga_id}                 # Get saga status
│   └── GET    /{saga_id}/steps           # Get saga step details
│
└── /admin
    ├── GET    /games
    ├── PATCH  /games/{game_id}
    ├── GET    /users
    ├── GET    /users/{user_id}
    ├── POST   /users/{user_id}/ban
    ├── DELETE /users/{user_id}/ban
    ├── GET    /bans
    ├── GET    /stats
    ├── GET    /audit-logs
    └── /substitute                       # Admin substitute management
        ├── GET    /pools/{season_id}     # View substitute pool
        ├── POST   /pools/{pool_id}/verify # Verify a substitute
        └── POST   /pools/{pool_id}/suspend # Suspend a substitute
```

### 10.3 Versioning Strategy

```
URL Path Versioning: /v1/, /v2/

Version in Header (optional):
Accept: application/vnd.gaming-portal.v1+json

Deprecation Headers:
Deprecation: true
Sunset: Sat, 01 Jan 2026 00:00:00 GMT
Link: <https://api.gaming-portal.com/v2/players>; rel="successor-version"
```

### 10.4 Validation/Error Handling (RFC 7807)

```rust
// Problem Details response structure
#[derive(Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ValidationError>>,
}

#[derive(Serialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}

// Example responses
impl ProblemDetails {
    pub fn not_found(resource: &str, id: &str) -> Self {
        Self {
            problem_type: "https://api.gaming-portal.com/problems/not-found".into(),
            title: "Resource Not Found".into(),
            status: 404,
            detail: Some(format!("{} with id '{}' was not found", resource, id)),
            instance: None,
            errors: None,
        }
    }
    
    pub fn validation_failed(errors: Vec<ValidationError>) -> Self {
        Self {
            problem_type: "https://api.gaming-portal.com/problems/validation-error".into(),
            title: "Validation Failed".into(),
            status: 400,
            detail: Some("One or more fields failed validation".into()),
            instance: None,
            errors: Some(errors),
        }
    }
    
    pub fn forbidden(detail: &str) -> Self {
        Self {
            problem_type: "https://api.gaming-portal.com/problems/forbidden".into(),
            title: "Forbidden".into(),
            status: 403,
            detail: Some(detail.into()),
            instance: None,
            errors: None,
        }
    }
}
```

**Example Error Response:**

```json
{
  "type": "https://api.gaming-portal.com/problems/validation-error",
  "title": "Validation Failed",
  "status": 400,
  "detail": "One or more fields failed validation",
  "errors": [
    {
      "field": "name",
      "message": "Team name must be between 3 and 64 characters",
      "code": "length"
    },
    {
      "field": "tag",
      "message": "Team tag must be alphanumeric",
      "code": "format"
    }
  ]
}
```

### 10.5 Standard Response Envelopes

```rust
// Success response
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

// Paginated response
#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[derive(Serialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total_items: u64,
    pub total_pages: u32,
}

// List response with cursor pagination
#[derive(Serialize)]
pub struct CursorPaginatedResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
```

---

## 11. Security & Permissions Model

### 11.1 Detailed RBAC Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           PERMISSION HIERARCHY                               │
└─────────────────────────────────────────────────────────────────────────────┘

PLATFORM_ADMIN (Global)
├── All permissions
├── Create global tournaments
├── Manage all leagues
└── Cannot be scoped

GAME_ADMIN (Per Game)
├── game:manage
├── game:queues:manage
├── game:plugin:configure
├── matches:admin
└── players:moderate

LEAGUE_ADMIN (Per League - assigned to league.membership_type = 'admin')
├── league:manage                    -- Configure league settings
├── league:members:manage            -- Invite/remove members, approve applications
├── league:tournaments:create        -- Create league-specific tournaments
├── league:tournaments:manage        -- Manage league tournaments
├── league:map_pool:configure        -- Set custom map pool (if plugin allows)
├── league:seasons:manage            -- Create and manage seasons
├── league:divisions:manage          -- Manage divisions/tiers within league
└── league:substitute_pool:manage    -- Manage substitute pool

TOURNAMENT_ADMIN (Per Tournament - can be league admin or platform admin)
├── tournament:manage
├── tournament:participants:manage
├── tournament:bracket:manage
├── tournament:bracket:generate
├── tournament:matches:admin
├── tournament:map_pool:override     -- Override map pool for specific tournament
└── tournament:disputes:resolve

TEAM_CAPTAIN (Per Team - role = 'captain' in team_members)
├── team:manage                      -- Update team profile, settings
├── team:disband                     -- Disband team (triggers saga)
├── team:members:invite              -- Invite new players
├── team:members:remove              -- Remove members (except other captains)
├── team:members:promote             -- Promote to captain/officer
├── team:tournaments:register        -- Register team for tournaments
├── lobby:lead                       -- Lead team in lobbies
├── lobby:picks                      -- Make pick/ban decisions
└── match:ready                      -- Mark team as ready

TEAM_OFFICER (Per Team - role = 'officer' in team_members)
├── team:members:invite              -- Invite new players
├── team:roster:manage               -- Manage roster positions
├── lobby:picks                      -- Make pick/ban decisions
└── match:ready                      -- Mark team as ready

PLAYER (Global - all authenticated users)
├── profile:read
├── profile:update
├── teams:create                     -- Create new team (becomes captain)
├── teams:join                       -- Accept team invitations
├── teams:leave                      -- Leave teams
├── leagues:join                     -- Join open leagues
├── leagues:apply                    -- Apply to application-based leagues
├── queues:join                      -- Join matchmaking queues
├── lobbies:create                   -- Create custom lobbies
├── lobbies:join
├── matches:view
├── tournaments:register             -- Register self for individual tournaments
└── substitute:register              -- Register as substitute

GUEST (Unauthenticated)
├── games:view
├── players:view
├── teams:view
├── matches:view
├── tournaments:view
└── leaderboards:view
```

**Key Relationships:**
- Players can be on **multiple teams** simultaneously
- A player who creates a team automatically becomes a **captain** (role='captain', is_founder=true)
- Captains are the **team admin role** - they can manage the team, invite/remove members, promote others
- Multiple captains can exist per team, but founders cannot be demoted
- League admins are members with `membership_type = 'admin'` in `league_members`
- League admins can create **league-specific tournaments** restricted to league members
- Platform admins can create **global tournaments** open to all players

### 11.2 Permission Scoping

```rust
// Permission with optional scope
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScopedPermission {
    pub permission: Permission,
    pub scope: PermissionScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PermissionScope {
    Global,
    Game(GameId),
    Team(TeamId),
    Tournament(TournamentId),
    League(LeagueId),
    Match(MatchId),
}

// Permission evaluation
impl RbacService {
    pub async fn check(&self, user_id: UserId, required: ScopedPermission) -> bool {
        // 1. Check for platform admin (has all permissions)
        if self.is_platform_admin(user_id).await {
            return true;
        }
        
        // 2. Check global permissions
        let global_perms = self.get_global_permissions(user_id).await;
        if global_perms.contains(&required.permission) {
            return true;
        }
        
        // 3. Check scoped permissions
        match &required.scope {
            PermissionScope::Global => false,
            PermissionScope::Game(game_id) => {
                self.check_game_permission(user_id, game_id, &required.permission).await
            }
            PermissionScope::Team(team_id) => {
                self.check_team_permission(user_id, team_id, &required.permission).await
            }
            PermissionScope::Tournament(tournament_id) => {
                self.check_tournament_permission(user_id, tournament_id, &required.permission).await
            }
            // ... other scopes
        }
    }
}
```

### 11.3 Authentication Strategy

**Primary: JWT Bearer Tokens**

```rust
// JWT Claims structure
#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // User ID
    pub jti: String,        // Token ID (for revocation)
    pub username: String,
    pub roles: Vec<String>, // Global roles
    pub iat: i64,           // Issued at
    pub exp: i64,           // Expiration
    pub nbf: i64,           // Not before
}

// Token configuration
pub struct TokenConfig {
    pub access_token_ttl: Duration,   // 15 minutes
    pub refresh_token_ttl: Duration,  // 7 days
    pub algorithm: Algorithm,          // HS256 or RS256
    pub issuer: String,
    pub audience: String,
}
```

**OAuth2 Integration:**

```rust
// Supported providers
pub enum OAuthProvider {
    Steam,
    Discord,
    Twitch,
    Google,
}

// OAuth flow
impl AuthService {
    pub async fn oauth_callback(
        &self,
        provider: OAuthProvider,
        code: &str,
    ) -> Result<AuthResponse, AuthError> {
        // Exchange code for tokens
        let oauth_tokens = self.oauth_client
            .exchange_code(provider, code)
            .await?;
        
        // Fetch user info from provider
        let provider_user = self.fetch_provider_user(provider, &oauth_tokens).await?;
        
        // Find or create user
        let user = self.find_or_create_oauth_user(provider, &provider_user).await?;
        
        // Generate platform tokens
        let tokens = self.generate_tokens(&user)?;
        
        Ok(AuthResponse {
            access_token: tokens.access,
            refresh_token: tokens.refresh,
            expires_in: self.config.access_token_ttl.as_secs(),
            user: user.into(),
        })
    }
}
```

### 11.4 Steam OpenID Authentication

Steam uses OpenID 2.0 rather than OAuth 2.0, requiring special handling:

```rust
use reqwest::Client;
use url::Url;

pub struct SteamAuthProvider {
    http_client: Client,
    realm: String,      // Your domain, e.g., "https://gaming-portal.com"
    return_to: String,  // Callback URL
    api_key: String,    // Steam Web API key
}

impl SteamAuthProvider {
    /// Generate Steam login URL for redirect
    pub fn get_login_url(&self) -> String {
        let mut url = Url::parse("https://steamcommunity.com/openid/login").unwrap();
        
        url.query_pairs_mut()
            .append_pair("openid.ns", "http://specs.openid.net/auth/2.0")
            .append_pair("openid.mode", "checkid_setup")
            .append_pair("openid.return_to", &self.return_to)
            .append_pair("openid.realm", &self.realm)
            .append_pair("openid.identity", "http://specs.openid.net/auth/2.0/identifier_select")
            .append_pair("openid.claimed_id", "http://specs.openid.net/auth/2.0/identifier_select");
        
        url.to_string()
    }
    
    /// Verify Steam callback and extract Steam ID
    pub async fn verify_callback(
        &self,
        query_params: &HashMap<String, String>,
    ) -> Result<SteamUser, SteamAuthError> {
        // Verify the OpenID response with Steam
        let mut verification_params = query_params.clone();
        verification_params.insert("openid.mode".into(), "check_authentication".into());
        
        let response = self.http_client
            .post("https://steamcommunity.com/openid/login")
            .form(&verification_params)
            .send()
            .await?;
        
        let body = response.text().await?;
        
        if !body.contains("is_valid:true") {
            return Err(SteamAuthError::InvalidResponse);
        }
        
        // Extract Steam ID from claimed_id
        // Format: https://steamcommunity.com/openid/id/76561198012345678
        let claimed_id = query_params
            .get("openid.claimed_id")
            .ok_or(SteamAuthError::MissingClaimedId)?;
        
        let steam_id = claimed_id
            .strip_prefix("https://steamcommunity.com/openid/id/")
            .ok_or(SteamAuthError::InvalidClaimedId)?;
        
        // Fetch user profile from Steam Web API
        let profile = self.fetch_steam_profile(steam_id).await?;
        
        Ok(profile)
    }
    
    /// Fetch Steam user profile using Web API
    async fn fetch_steam_profile(&self, steam_id: &str) -> Result<SteamUser, SteamAuthError> {
        let url = format!(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/?key={}&steamids={}",
            self.api_key, steam_id
        );
        
        let response: SteamApiResponse = self.http_client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        let player = response.response.players
            .into_iter()
            .next()
            .ok_or(SteamAuthError::UserNotFound)?;
        
        Ok(SteamUser {
            steam_id: steam_id.to_string(),
            steam_id_64: steam_id.parse()?,
            persona_name: player.personaname,
            avatar_url: player.avatarfull,
            profile_url: player.profileurl,
            country_code: player.loccountrycode,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SteamUser {
    pub steam_id: String,
    pub steam_id_64: u64,
    pub persona_name: String,
    pub avatar_url: String,
    pub profile_url: String,
    pub country_code: Option<String>,
}

// API routes for Steam auth
pub fn steam_auth_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/steam/login", get(steam_login_redirect))
        .route("/auth/steam/callback", get(steam_callback))
        .route("/auth/steam/link", post(link_steam_account))
}

async fn steam_login_redirect(
    State(state): State<AppState>,
) -> Redirect {
    let url = state.steam_auth.get_login_url();
    Redirect::temporary(&url)
}

async fn steam_callback(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Verify Steam callback
    let steam_user = state.steam_auth
        .verify_callback(&params)
        .await
        .map_err(|e| ApiError::unauthorized(format!("Steam auth failed: {}", e)))?;
    
    // Find or create user linked to Steam
    let user = state.auth_service
        .find_or_create_steam_user(&steam_user)
        .await?;
    
    // Generate tokens
    let tokens = state.auth_service.generate_tokens(&user)?;
    
    Ok(Json(AuthResponse {
        access_token: tokens.access,
        refresh_token: tokens.refresh,
        expires_in: state.config.access_token_ttl.as_secs(),
        user: user.into(),
    }))
}

// Link existing account to Steam
async fn link_steam_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<LinkResult>, ApiError> {
    let steam_user = state.steam_auth
        .verify_callback(&params)
        .await?;
    
    // Check if Steam account already linked to another user
    if let Some(existing) = state.auth_service
        .find_user_by_steam_id(&steam_user.steam_id)
        .await?
    {
        if existing.id != auth.user_id {
            return Err(ApiError::conflict("Steam account already linked to another user"));
        }
    }
    
    // Link Steam account
    state.auth_service
        .link_steam_account(auth.user_id, &steam_user)
        .await?;
    
    Ok(Json(LinkResult {
        provider: "steam".into(),
        provider_user_id: steam_user.steam_id,
        linked_at: Utc::now(),
    }))
}
```

**Steam Authentication Database Schema:**

```sql
-- Steam-specific columns in oauth_connections
-- (already covered by generic oauth_connections table)

-- Additional index for Steam ID lookups
CREATE INDEX idx_oauth_steam_id ON oauth_connections(provider_user_id) 
    WHERE provider = 'steam';

-- Store additional Steam profile data
ALTER TABLE players ADD COLUMN steam_profile JSONB;

-- Example steam_profile JSON:
-- {
--   "steam_id_64": 76561198012345678,
--   "persona_name": "PlayerName",
--   "avatar_url": "https://...",
--   "profile_visibility": "public",
--   "vac_banned": false,
--   "game_bans": 0
-- }
```

```rust
// WebSocket authentication flow
pub async fn ws_authenticate(
    token: &str,
    state: &AppState,
) -> Result<WsSession, WsAuthError> {
    // 1. Validate JWT
    let claims = validate_jwt(token, &state.jwt_config)?;
    
    // 2. Check if token is revoked
    if state.token_blacklist.is_revoked(&claims.jti).await {
        return Err(WsAuthError::TokenRevoked);
    }
    
    // 3. Load user permissions
    let permissions = state.rbac_service
        .get_user_permissions(claims.sub.parse()?)
        .await?;
    
    // 4. Create session with short-lived validity
    Ok(WsSession {
        user_id: claims.sub.parse()?,
        username: claims.username,
        permissions,
        authenticated_at: Utc::now(),
        // Re-authenticate periodically for long sessions
        reauthenticate_at: Utc::now() + Duration::minutes(30),
    })
}

// Per-message authorization
impl LobbyActor {
    fn authorize_message(&self, session: &WsSession, msg: &LobbyMessage) -> bool {
        match msg {
            LobbyMessage::Chat { .. } => true,
            LobbyMessage::Ready { .. } => true,
            LobbyMessage::Pick { .. } | LobbyMessage::Ban { .. } => {
                self.is_captain(session.user_id)
            }
            LobbyMessage::Kick { .. } => {
                session.permissions.contains(&Permission::LobbyAdmin)
            }
            LobbyMessage::ForceStart => {
                session.permissions.contains(&Permission::LobbyAdmin)
            }
            _ => false,
        }
    }
}
```

### 11.5 Plugin Sandboxing

```rust
// Plugin capability restrictions
pub struct PluginCapabilities {
    // Database access
    pub db_access: DbAccessLevel,
    
    // Network access
    pub allowed_domains: Vec<String>,
    
    // Resource limits
    pub max_memory_mb: u32,
    pub max_cpu_time_ms: u32,
    
    // Feature flags
    pub can_send_emails: bool,
    pub can_access_player_pii: bool,
}

#[derive(Clone)]
pub enum DbAccessLevel {
    None,
    OwnDataOnly,  // Only plugin_data for own game
    ReadOnlyCore, // Read access to core tables
    Full,         // Reserved for first-party plugins
}

// Enforced plugin context
pub struct SandboxedPluginContext {
    db: ScopedDbPool,           // Only own game's data
    http_client: FilteredClient, // Only allowed domains
    metrics: ScopedMetrics,      // Namespaced metrics
    events: FilteredEventEmitter, // Can only emit own events
}
```

### 11.6 Recommended Security Crates

| Purpose | Crate | Version | Notes |
|---------|-------|---------|-------|
| Password Hashing | `argon2` | 0.5+ | Argon2id recommended |
| JWT | `jsonwebtoken` | 9+ | HS256/RS256 support |
| CSRF Protection | `axum-csrf` | 0.9+ | Double-submit cookie |
| Rate Limiting | `governor` | 0.6+ | Token bucket algorithm |
| Secrets Management | `secrecy` | 0.8+ | Secure secret handling |
| Cryptography | `ring` | 0.17+ | General crypto operations |
| UUID | `uuid` | 1+ | Secure UUID generation |
| OAuth2 | `oauth2` | 4+ | OAuth 2.0 client |

---

## 12. Scalability & Performance Considerations

### 12.1 Async Runtime Configuration

```rust
// Tokio runtime configuration for production
fn build_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .thread_name("gaming-portal-worker")
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime")
}

// Connection pool configuration
pub struct DbConfig {
    pub max_connections: u32,        // 100 per instance
    pub min_connections: u32,        // 10
    pub acquire_timeout: Duration,   // 3 seconds
    pub idle_timeout: Duration,      // 10 minutes
    pub max_lifetime: Duration,      // 30 minutes
}

impl DbConfig {
    pub fn to_pool_options(&self) -> PgPoolOptions {
        PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.acquire_timeout)
            .idle_timeout(Some(self.idle_timeout))
            .max_lifetime(Some(self.max_lifetime))
    }
}
```

### 12.2 WebSocket Scaling Strategy

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    WEBSOCKET SCALING ARCHITECTURE                            │
└─────────────────────────────────────────────────────────────────────────────┘

                            ┌─────────────────┐
                            │  Load Balancer  │
                            │  (Sticky/Hash)  │
                            └────────┬────────┘
                                     │
            ┌────────────────────────┼────────────────────────┐
            │                        │                        │
            ▼                        ▼                        ▼
    ┌───────────────┐        ┌───────────────┐        ┌───────────────┐
    │   Server 1    │        │   Server 2    │        │   Server 3    │
    │ ┌───────────┐ │        │ ┌───────────┐ │        │ ┌───────────┐ │
    │ │ Lobby A   │ │        │ │ Lobby D   │ │        │ │ Lobby G   │ │
    │ │ Lobby B   │ │        │ │ Lobby E   │ │        │ │ Lobby H   │ │
    │ │ Lobby C   │ │        │ │ Lobby F   │ │        │ │ Lobby I   │ │
    │ └───────────┘ │        │ └───────────┘ │        │ └───────────┘ │
    └───────┬───────┘        └───────┬───────┘        └───────┬───────┘
            │                        │                        │
            └────────────────────────┼────────────────────────┘
                                     │
                                     ▼
                            ┌─────────────────┐
                            │   Redis PubSub  │
                            │ (Cross-server)  │
                            └─────────────────┘

Sticky Session Strategy:
- Hash lobby_id to determine target server
- Client connects to specific server based on lobby
- Cross-server communication via Redis PubSub
```

```rust
// Connection manager per server instance
pub struct ConnectionManager {
    // Local lobbies on this server instance
    lobbies: DashMap<LobbyId, LobbyActorHandle>,
    
    // Cross-server communication
    redis_pubsub: RedisPubSub,
    
    // Connection limits
    max_connections_per_instance: usize,
    current_connections: AtomicUsize,
}

impl ConnectionManager {
    pub async fn handle_connection(
        &self,
        lobby_id: LobbyId,
        socket: WebSocket,
        session: WsSession,
    ) -> Result<(), ConnectionError> {
        // Check connection limits
        let current = self.current_connections.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_connections_per_instance {
            self.current_connections.fetch_sub(1, Ordering::SeqCst);
            return Err(ConnectionError::ServerFull);
        }
        
        // Get or create lobby actor
        let lobby = self.lobbies
            .entry(lobby_id)
            .or_insert_with(|| self.spawn_lobby_actor(lobby_id));
        
        // Register connection with lobby
        lobby.add_connection(session, socket).await
    }
}
```

### 12.3 Load Balancing Strategy

```nginx
# nginx configuration for WebSocket support
upstream api_servers {
    least_conn;
    server api1:8080;
    server api2:8080;
    server api3:8080;
}

upstream ws_servers {
    # Sticky sessions based on lobby_id
    hash $arg_lobby_id consistent;
    server ws1:8081;
    server ws2:8081;
    server ws3:8081;
}

server {
    listen 443 ssl http2;
    server_name api.gaming-portal.com;

    # REST API - stateless, use least_conn
    location /v1/ {
        proxy_pass http://api_servers;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # WebSocket - sticky sessions
    location /ws/ {
        proxy_pass http://ws_servers;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

### 12.4 Caching Strategy

```rust
// Multi-layer caching
pub struct CacheManager {
    // L1: In-process cache (per instance)
    local: moka::sync::Cache<String, CachedValue>,
    
    // L2: Distributed cache (Redis)
    redis: RedisPool,
}

impl CacheManager {
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        // Try L1 first
        if let Some(value) = self.local.get(key) {
            return value.deserialize().ok();
        }
        
        // Try L2
        if let Ok(Some(data)) = self.redis.get::<Vec<u8>>(key).await {
            let value: T = serde_json::from_slice(&data).ok()?;
            
            // Populate L1
            self.local.insert(
                key.to_string(),
                CachedValue::new(&value, Duration::minutes(5)),
            );
            
            return Some(value);
        }
        
        None
    }
    
    pub async fn set<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) -> Result<(), CacheError> {
        let data = serde_json::to_vec(value)?;
        
        // Set in L2
        self.redis.set_ex(key, &data, ttl.as_secs()).await?;
        
        // Set in L1 with shorter TTL
        self.local.insert(
            key.to_string(),
            CachedValue::new(value, ttl.min(Duration::minutes(5))),
        );
        
        Ok(())
    }
}

// Cache configuration per entity type
pub struct CacheConfig {
    pub user_permissions_ttl: Duration,    // 5 minutes
    pub player_profile_ttl: Duration,      // 15 minutes
    pub team_info_ttl: Duration,           // 15 minutes
    pub leaderboard_ttl: Duration,         // 1 minute
    pub game_config_ttl: Duration,         // 1 hour
}
```

### 12.5 Concurrency and Memory Safety

```rust
// Safe concurrent state management
pub struct MatchmakingQueue {
    // Lock-free concurrent map for queue entries
    entries: DashMap<PlayerId, QueueEntry>,
    
    // Atomic counters for metrics
    total_queued: AtomicUsize,
    matches_made: AtomicU64,
    
    // Read-write lock for config changes
    config: RwLock<QueueConfig>,
}

impl MatchmakingQueue {
    pub fn enqueue(&self, player_id: PlayerId, entry: QueueEntry) -> Result<(), QueueError> {
        // Check for existing entry (no data race due to DashMap)
        if self.entries.contains_key(&player_id) {
            return Err(QueueError::AlreadyQueued);
        }
        
        // Insert atomically
        self.entries.insert(player_id, entry);
        self.total_queued.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }
    
    pub fn dequeue(&self, player_id: &PlayerId) -> Option<QueueEntry> {
        let entry = self.entries.remove(player_id);
        if entry.is_some() {
            self.total_queued.fetch_sub(1, Ordering::Relaxed);
        }
        entry.map(|(_, e)| e)
    }
    
    pub async fn update_config(&self, new_config: QueueConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }
}
```

### 12.6 Performance Metrics Targets

| Metric | Target | Notes |
|--------|--------|-------|
| API Response Time (p50) | < 50ms | Simple queries |
| API Response Time (p99) | < 200ms | Complex operations |
| WebSocket Message Latency | < 20ms | Within same region |
| Matchmaking Time (p50) | < 30s | Standard queues |
| Database Query Time (p99) | < 50ms | With proper indexing |
| Memory per WS Connection | < 64KB | Including buffers |
| Connections per Instance | 10,000+ | WebSocket servers |

---

## 13. Deployment & DevOps Notes

### 13.1 CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
name: CI/CD Pipeline

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: true

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: gaming_portal_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432
      redis:
        image: redis:7
        ports:
          - 6379:6379
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run migrations
        run: cargo sqlx migrate run
        env:
          DATABASE_URL: postgres://postgres:test@localhost:5432/gaming_portal_test
      - name: Run tests
        run: cargo test --all-features
        env:
          DATABASE_URL: postgres://postgres:test@localhost:5432/gaming_portal_test
          REDIS_URL: redis://localhost:6379

  build:
    needs: [lint, test]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build release
        run: cargo build --release
      - name: Build Docker image
        run: docker build -t gaming-portal:${{ github.sha }} .
      - name: Push to registry
        if: github.ref == 'refs/heads/main'
        run: |
          docker tag gaming-portal:${{ github.sha }} registry.example.com/gaming-portal:${{ github.sha }}
          docker push registry.example.com/gaming-portal:${{ github.sha }}

  deploy:
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - name: Deploy to staging
        run: |
          # Kubernetes deployment
          kubectl set image deployment/gaming-portal \
            gaming-portal=registry.example.com/gaming-portal:${{ github.sha }}
```

### 13.2 Docker Configuration

```dockerfile
# Dockerfile
# Build stage
FROM rust:1.75-bookworm as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build dependencies (cached layer)
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy source and build
COPY src/ src/
COPY migrations/ migrations/
COPY .sqlx/ .sqlx/
RUN touch src/main.rs
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/gaming-portal /app/gaming-portal
COPY migrations/ /app/migrations/

ENV RUST_LOG=info
EXPOSE 8080

CMD ["/app/gaming-portal"]
```

```yaml
# docker-compose.yml (development)
version: '3.8'

services:
  api:
    build: .
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db:5432/gaming_portal
      - REDIS_URL=redis://redis:6379
      - RUST_LOG=debug
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_started

  db:
    image: postgres:15
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: gaming_portal
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

### 13.3 Observability Stack

```rust
// Tracing configuration
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_opentelemetry::OpenTelemetryLayer;
use opentelemetry::sdk::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;

pub fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    // OpenTelemetry exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint("http://otel-collector:4317");
    
    let tracer_provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry::runtime::Tokio)
        .build();
    
    let tracer = tracer_provider.tracer("gaming-portal");
    
    // Subscriber with multiple layers
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .with(OpenTelemetryLayer::new(tracer))
        .init();
    
    Ok(())
}

// Custom metrics
use metrics::{counter, gauge, histogram};

pub fn record_request_metrics(method: &str, path: &str, status: u16, duration: Duration) {
    let labels = [
        ("method", method.to_string()),
        ("path", path.to_string()),
        ("status", status.to_string()),
    ];
    
    counter!("http_requests_total", &labels).increment(1);
    histogram!("http_request_duration_seconds", &labels).record(duration.as_secs_f64());
}

pub fn record_ws_metrics(action: &str, lobby_id: &str) {
    counter!("ws_messages_total", "action" => action.to_string(), "lobby" => lobby_id.to_string()).increment(1);
}

pub fn update_active_connections(count: usize) {
    gauge!("ws_active_connections").set(count as f64);
}
```

### 13.4 Kubernetes Deployment

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: gaming-portal-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: gaming-portal-api
  template:
    metadata:
      labels:
        app: gaming-portal-api
    spec:
      containers:
        - name: api
          image: registry.example.com/gaming-portal:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: gaming-portal-secrets
                  key: database-url
            - name: REDIS_URL
              valueFrom:
                secretKeyRef:
                  name: gaming-portal-secrets
                  key: redis-url
          resources:
            requests:
              memory: "256Mi"
              cpu: "250m"
            limits:
              memory: "1Gi"
              cpu: "1000m"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 10
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /ready
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 5

---
apiVersion: v1
kind: Service
metadata:
  name: gaming-portal-api
spec:
  selector:
    app: gaming-portal-api
  ports:
    - port: 80
      targetPort: 8080

---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: gaming-portal-api
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: gaming-portal-api
  minReplicas: 3
  maxReplicas: 20
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
    - type: Pods
      pods:
        metric:
          name: http_requests_per_second
        target:
          type: AverageValue
          averageValue: "1000"
```

### 13.5 Recommended DevOps Crates/Tools

| Category | Tool/Crate | Purpose |
|----------|------------|---------|
| Tracing | `tracing` + `tracing-subscriber` | Structured logging |
| OpenTelemetry | `opentelemetry` + `tracing-opentelemetry` | Distributed tracing |
| Metrics | `metrics` + `metrics-exporter-prometheus` | Application metrics |
| Health Checks | Custom Axum handlers | Liveness/readiness probes |
| Configuration | `config` | Environment-based config |
| Secrets | Kubernetes Secrets / HashiCorp Vault | Secret management |

---

## 14. Appendices

### Appendix A: Technology Decision Matrix

| Component | Option A | Option B | Decision | Rationale |
|-----------|----------|----------|----------|-----------|
| Web Framework | Axum | Actix-web | **Axum** | Tower ecosystem, ergonomics, WebSocket support |
| Database | PostgreSQL | CockroachDB | **PostgreSQL** | Maturity, JSONB, tooling, cost |
| DB Access | SQLx | Diesel | **SQLx** | Compile-time checks, async, flexibility |
| Plugin Loading | Dynamic (dlopen) | Compile-time | **Compile-time** | Type safety, security, stability |
| Message Broker | Redis Pub/Sub | NATS | **Redis Pub/Sub** | Simplicity, existing Redis usage |
| Caching | Redis | memcached | **Redis** | Data structures, pub/sub, persistence |
| Search | Meilisearch | PostgreSQL FTS | **PostgreSQL FTS** | One less service, good enough for scale |

### Appendix B: Crate Dependency Summary

```toml
# Cargo.toml (key dependencies)
[dependencies]
# Web framework
axum = { version = "0.7", features = ["ws", "macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "compression-gzip"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Authentication
jsonwebtoken = "9"
argon2 = "0.5"
oauth2 = "4"

# Validation
validator = { version = "0.16", features = ["derive"] }

# Error handling
thiserror = "1"
anyhow = "1"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
opentelemetry = "0.21"
tracing-opentelemetry = "0.22"
metrics = "0.21"

# Concurrency
dashmap = "5"
parking_lot = "0.12"

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
semver = "1"
url = "2"

# Rate limiting
governor = "0.6"
tower_governor = "0.3"

# Configuration
config = "0.14"
dotenvy = "0.15"
```

### Appendix C: Security Checklist

- [ ] JWT tokens use secure algorithm (HS256 minimum, RS256 preferred)
- [ ] Refresh tokens stored hashed in database
- [ ] Password hashing uses Argon2id with secure parameters
- [ ] Rate limiting on authentication endpoints
- [ ] CORS properly configured for production
- [ ] All user input validated before processing
- [ ] SQL queries use parameterized statements (SQLx compile-time)
- [ ] WebSocket connections authenticated before message processing
- [ ] Plugin data access scoped to own game
- [ ] Audit logging for security-relevant actions
- [ ] Secrets stored in secure vault (not in code/config files)
- [ ] HTTPS enforced in production
- [ ] Security headers set (CSP, X-Frame-Options, etc.)
- [ ] Dependency vulnerabilities scanned (cargo-audit)

### Appendix D: Glossary

| Term | Definition |
|------|------------|
| **PUG** | Pick-Up Game - informal match with random or selected players |
| **ELO/Glicko-2** | Rating systems for competitive games |
| **Bracket** | Tournament structure defining match progression |
| **Lobby** | Pre-match waiting room with pick/ban and ready checks |
| **Plugin** | Game-specific module implementing custom logic |
| **RBAC** | Role-Based Access Control |
| **Sticky Session** | Load balancer routing same client to same server |

---

*Document prepared for engineering review. Last updated: November 2024*
