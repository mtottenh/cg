# Architecture Overview

This document describes the full system architecture: the API server, database, external bots/pollers, demo catalog pipeline, and how they all connect.

## System Components

```
                                    Steam Web API
                                         |
                                    cs2-poller          (poll share codes)
                                         |
                                         v
  S3 Bucket -----> portal-scanner ----> Portal API <---- cs2-enricher  (fetch GC data)
  (demo files)     (catalog demos)      (Rust/Axum)          |
                                            |                v
                                            v           Steam Game Coordinator
                                        PostgreSQL
                                            ^
                                            |
                                     External Demo Parser
                                     (demos.cs210mans.uk)
```

### 1. Portal API (`crates/portal-api`, `portal-app`)

The main Rust backend. Axum web server exposing ~228 OpenAPI endpoints. Handles auth, RBAC, players, leagues, tournaments, demos, evidence, disputes, and more.

**Key architectural layers:**

```
HTTP Handlers (portal-api)  -->  Domain Services (portal-domain)  -->  Repository Traits
        |                               |                                     |
      DTOs                        Domain Entities                    DB Adapters (portal-db)
   (request/response)           (business logic)                      (SQLx / PostgreSQL)
```

- **Handlers** convert DTOs to/from domain types, enforce auth/RBAC
- **Services** are generic over repository traits (testable with mocks)
- **Repositories** are implemented by `Pg*Repository` adapters using SQLx compile-time checked queries
- **State** (`AppState`) wires ~50 repositories and services together with `Arc` for thread-safe sharing

**Two auth modes:**
- `Authorization: Bearer <JWT>` -- normal user/player auth
- `X-API-Key: cgp_...` -- service auth for bots/scanners (internal endpoints)

### 2. PostgreSQL Database

57 migrations in `/migrations/`. Tables organized by domain:

| Domain | Tables |
|--------|--------|
| Identity | `users`, `players`, `player_game_profiles` |
| Games | `games`, `game_maps`, `rank_tiers` |
| Leagues | `leagues`, `league_members`, `league_invitations` |
| Teams | `league_teams`, `league_team_members`, `league_team_seasons`, `league_team_invitations` |
| Tournaments | `tournaments`, `tournament_stages`, `tournament_brackets`, `tournament_registrations`, `tournament_matches`, `tournament_match_games`, `tournament_map_pools` |
| Match Workflow | `match_status_logs`, `schedule_proposals`, `availability_windows`, `availability_exceptions`, `suggested_times` |
| Veto | `veto_sessions`, `veto_actions`, `veto_lobby_messages`, `veto_delegates` |
| Results & Disputes | `result_claims`, `evidence`, `evidence_access_log`, `result_reviews`, `disputes`, `dispute_messages`, `forfeits` |
| Demos | `demos`, `demo_match_links`, `demo_players` |
| Steam Integration | `steam_tracking`, `discovered_matches` |
| Player Stats | `player_rating_history`, `player_mm_stats`, `player_match_history` |
| RBAC | `roles`, `permissions`, `role_permissions`, `user_roles` |
| System | `api_keys`, `bans`, `entity_changes`, `sagas`, `progression_logs`, `refresh_tokens` |

### 3. Steam Bot Workspace (`../steam_bot/`)

A separate Rust workspace containing standalone daemons that interact with Steam APIs and the Portal API via internal endpoints.

#### 3a. cs2-poller (`bins/cs2-poller`)

**Purpose:** Poll Steam Web API for new CS2 match share codes from tracked players.

**Loop (every ~60s):**
1. `GET /v1/internal/steam-tracking/active?game=cs2` -- fetch players with tracking enabled
2. For each player, call Steam Web API (`GetNextMatchSharingCode`) using their `steam_id_64` + game auth code
3. Extract `share_code`, `match_id`, `outcome_id`, `token`
4. `POST /v1/internal/discovered-matches` -- submit new matches (idempotent on share_code)
5. `PATCH /v1/internal/steam-tracking/{id}/poll-result` -- update cursor / record errors

**Requires:** `STEAM_WEB_API_KEY`, `PORTAL_API_KEY`

**Libraries:** `cs2-webapi` (HTTP client for Steam Web API), `cs2-sharecode` (encode/decode share codes)

#### 3b. cs2-enricher (`bins/cs2-enricher`)

**Purpose:** Fetch full match scoreboard data from the CS2 Game Coordinator for pending discovered matches.

**Loop (every ~30s):**
1. `GET /v1/internal/discovered-matches/pending?game=cs2&limit=5`
2. `POST /v1/internal/discovered-matches/{id}/claim` -- atomic claim (prevents double-processing)
3. Connect to Steam GC using a dedicated bot account (real Steam client protocol via `steam-vent`)
4. Send `MatchListRequestFullGameInfo` with match_id/outcome_id/token
5. Receive `MatchList` response with full scoreboard (per-player kills/deaths/assists/scores, team scores, map, duration)
6. Optionally download + parse demo via `cs2-demo-rank` to extract rank changes
7. `POST /v1/internal/discovered-matches/{id}/enriched` -- submit GC data + demo URL + player ratings

**Requires:** `STEAM_USERNAME`, `STEAM_PASSWORD`, `STEAM_SHARED_SECRET` (TOTP), `PORTAL_API_KEY`

**Libraries:** `cs2-gc` (GC client wrapping `steam-vent`), `cs2-demo-rank` (demo rank extraction)

#### 3c. demo-downloader (`bins/demo-downloader`)

**Purpose:** CLI tool to download demo files from enriched discovered matches.

**Flow:**
1. `GET /v1/internal/discovered-matches/recent-demos?game=cs2` -- get matches with demo URLs
2. Download `.dem.bz2` files from Steam CDN
3. Store locally or upload to S3

#### 3d. Supporting Crates

| Crate | Purpose |
|-------|---------|
| `cs2-gc` | High-level Game Coordinator client (`Cs2GcClient`) wrapping `steam-vent` |
| `cs2-webapi` | Steam Web API HTTP client (share code polling) |
| `cs2-sharecode` | Encode/decode CS2 match share codes |
| `cs2-demo-rank` | Extract rank update data from `.dem` files |
| `cs2-provider` | Shared config/auth types for CS2 bots |
| `steam-totp` | Generate Steam Guard TOTP codes |

### 4. Portal Scanner (`crates/portal-scanner`)

A daemon that polls an S3 bucket for new `.dem` files and catalogs them into the demo system.

**Loop (every ~300s):**
1. List S3 objects matching prefix (e.g., `s3://cs2-10mans-demo-files/`)
2. Filter for `.dem` and `.dem.bz2` files
3. `POST /v1/internal/demos/batch` -- batch catalog up to 500 demos (idempotent on s3_key)
4. For each newly cataloged demo, fetch parsed stats from external service
5. `GET https://demos.cs210mans.uk/stats/{name}.dem.stats.json` -- external demo parser
6. `POST /v1/admin/demos/{id}/stats` -- submit parsed metadata + player stats
7. Retry loop for demos that previously failed stats fetching

**Requires:** `SCANNER_S3_BUCKET`, `SCANNER_S3_ENDPOINT`, `CS2_DEMO_SERVICE_URL`, `PORTAL_API_KEY`

### 5. Plugin System (`crates/portal-plugins`)

Game-agnostic plugin architecture. Currently only CS2 is implemented.

**Traits:**
- `GamePlugin` -- identity, maps, team sizes, stats schema, rating calculations, rank tiers
- `TournamentPlugin` -- map veto formats, match configuration
- `EvidencePlugin` -- demo discovery, evidence validation, metadata extraction

**CS2 Plugin** (`plugins/games/cs2/`):
- 7 active-duty maps, 5v5 format
- Premier rating system (0-35,000+) with 7 color tiers (Grey through Gold)
- Demo file extension: `.dem`, storage prefix: `cs2-demos/`
- Stats: kills, deaths, assists, ADR, HS%, MVPs, entry frags

---

## Data Flows

### A. Public Matchmaking Stats Pipeline

Shows how a player's CS2 matchmaking stats get from Steam into the platform.

```
Player enables Steam tracking (provides game auth code + initial share code)
  |
  v
[cs2-poller] polls Steam Web API every 60s
  --> discovers new share codes
  --> POST /internal/discovered-matches  (status: pending)
  |
  v
[cs2-enricher] claims pending matches every 30s
  --> connects to Steam GC with bot account
  --> fetches full scoreboard (MatchList protobuf)
  --> POST /internal/discovered-matches/{id}/enriched
        |
        +--> process_demo_ratings():
        |      Filter Premier rank data (rank_type_id=11)
        |      Convert Steam32 account_id -> SteamID64
        |      Insert player_rating_history records
        |
        +--> process_match_stats():
               For each player in GC data:
                 Insert player_match_history (individual match record)
                 Upsert player_mm_stats (accumulate lifetime aggregates)
  |
  v
Frontend reads:
  GET /v1/players/{id}/games/cs2/mm-stats        --> lifetime aggregates
  GET /v1/players/{id}/games/cs2/match-history    --> paginated match list
  GET /v1/players/{id}/games/cs2/rating-history   --> rating over time
```

### B. Demo Catalog Pipeline

Shows how demo files get discovered, parsed, and linked to matches.

```
Demo files land in S3 bucket (from game servers, manual uploads, or demo-downloader)
  |
  v
[portal-scanner] lists S3 every 300s
  --> filters for .dem / .dem.bz2 files
  --> POST /internal/demos/batch  (status: pending, idempotent on s3_key)
  |
  v
[portal-scanner] fetches stats from external parser
  --> GET https://demos.cs210mans.uk/stats/{name}.dem.stats.json
  --> POST /admin/demos/{id}/stats
        |
        +--> demos.parsed_metadata  (map, teams, scores, rounds, duration)
        +--> demos.raw_stats        (full JSON blob)
        +--> demo_players           (per-player stats: kills, deaths, ADR, HS%)
        +--> demos.status = ready
  |
  v
Linking (manual or automatic):
  POST /admin/demos/{id}/link  --> demo_match_links (link_type: manual/auto/discovered)
  |
  v
Consumption:
  GET /v1/demos                               --> browse catalog (public)
  GET /v1/demos/{id}/players                  --> player stats from demo
  GET /v1/matches/{match_id}/demos            --> demos linked to a tournament match
  POST /admin/evidence/{id}/validate          --> plugin validates demo against claimed result
```

### C. Tournament Match Evidence Flow

Shows how demos serve as evidence in the tournament result pipeline.

```
Match completes --> teams submit result claims
  |
  v
Evidence attached:
  - Upload demo (multipart -> S3 presigned URL)
  - Link external video/screenshot
  - Auto-discover from demo catalog (plugin-based)
  |
  v
Admin reviews:
  - Validate evidence (plugin parses demo, compares scores/players)
  - Approve/reject result
  - If disputed: dispute workflow with message thread
  |
  v
Result confirmed --> progression advances bracket
```

---

## Internal API Endpoints (Service Auth)

All require `X-API-Key` header. Used by bots and scanners, not by end users.

### Steam Tracking
| Method | Path | Consumer |
|--------|------|----------|
| `GET` | `/v1/internal/steam-tracking/active` | cs2-poller |
| `PATCH` | `/v1/internal/steam-tracking/{id}/poll-result` | cs2-poller |

### Discovered Matches
| Method | Path | Consumer |
|--------|------|----------|
| `POST` | `/v1/internal/discovered-matches` | cs2-poller |
| `GET` | `/v1/internal/discovered-matches/pending` | cs2-enricher |
| `POST` | `/v1/internal/discovered-matches/{id}/claim` | cs2-enricher |
| `POST` | `/v1/internal/discovered-matches/{id}/enriched` | cs2-enricher |
| `POST` | `/v1/internal/discovered-matches/{id}/failed` | cs2-enricher |
| `GET` | `/v1/internal/discovered-matches/recent-demos` | demo-downloader |

### Demos
| Method | Path | Consumer |
|--------|------|----------|
| `POST` | `/v1/internal/demos/batch` | portal-scanner |
| `GET` | `/v1/internal/demos/pending` | portal-scanner |
| `POST` | `/v1/admin/demos/{id}/stats` | portal-scanner |
| `POST` | `/v1/admin/demos/{id}/stats-failed` | portal-scanner |

---

## Concurrency & Idempotency Patterns

| Operation | Strategy |
|-----------|----------|
| Discovered match upsert | `UNIQUE(tracking_id, share_code)`, ON CONFLICT DO NOTHING |
| Demo cataloging | `UNIQUE(game_id, s3_bucket, s3_key)`, ON CONFLICT DO NOTHING |
| Enrichment claiming | `UPDATE ... SET status='enriching' WHERE status='pending'` (atomic; returns 409 if already claimed) |
| Stats accumulation | `INSERT ... ON CONFLICT DO UPDATE` (single atomic upsert) |
| Demo stats re-submission | Delete existing `demo_players`, re-insert (safe idempotent overwrite) |
| Match history | `UNIQUE(player_id, discovered_match_id)`, ON CONFLICT returns existing |

These patterns allow horizontal scaling -- multiple enricher or scanner instances can run safely without coordination.

---

## Discovered Match Lifecycle

```
pending  ──claim──>  enriching  ──enriched──>  enriched
   |                     |
   |                     +──failed──>  failed  (retry_count incremented)
   |                                     |
   +<───────────────────────────────────-+  (re-claimable if retry_count < max_retries)
```

## Demo Lifecycle

```
pending  ──stats submitted──>  ready
   |
   +──stats failed──>  failed  (retryable by scanner)
   |
   +──processing──>  processing  (intermediate)
   |
   +──────────────>  archived  (admin action)
```

---

## Key File Locations

| Component | Path |
|-----------|------|
| API server entry point | `crates/portal-app/src/main.rs` |
| API state / DI wiring | `crates/portal-api/src/state.rs` |
| Internal handlers (bot endpoints) | `crates/portal-api/src/handlers/internal.rs` |
| Demo handlers (admin/public) | `crates/portal-api/src/handlers/demos.rs` |
| Evidence handlers | `crates/portal-api/src/handlers/evidence.rs` |
| Player stats handlers | `crates/portal-api/src/handlers/player_game_profiles.rs` |
| Discovered match service | `crates/portal-domain/src/services/discovered_match.rs` |
| Demo service | `crates/portal-domain/src/services/demo.rs` |
| CS2 plugin | `crates/portal-plugins/src/games/cs2/` |
| Plugin traits | `crates/portal-plugins/src/traits.rs` |
| Scanner daemon | `crates/portal-scanner/src/main.rs` |
| Migrations | `migrations/` (57 files) |
| Steam bot workspace | `../steam_bot/` |
| cs2-poller | `../steam_bot/bins/cs2-poller/` |
| cs2-enricher | `../steam_bot/bins/cs2-enricher/` |
| CS2 GC client library | `../steam_bot/crates/cs2-gc/` |
