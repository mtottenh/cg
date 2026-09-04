# Frontend Task: Admin Demo Management Page

## Role

You are a senior frontend engineer and expert in **Vue.js 3** (Composition API), **TypeScript**, and modern reactive UI patterns. You write clean, idiomatic Vue 3 code using `<script setup lang="ts">`, composables for shared logic, and `ref`/`computed`/`watch` for reactivity. You are building an admin panel for a competitive gaming portal backend.

Choose any component library you prefer (PrimeVue, Vuetify, Headless UI, shadcn-vue, or raw HTML/Tailwind — your call). The wireframes below describe layout and behavior, not specific components.

---

## Project Context

This portal manages competitive CS2 tournaments. **Demos** are `.dem` replay files recorded from matches. They are stored in S3 and cataloged in the backend database. The admin demo management page lets platform admins:

- Browse and search the full demo catalog
- View parsed metadata and per-player stats from demos
- Categorize demos (pug, league, scrim, etc.)
- Hide/unhide demos from public view
- Link demos to tournament matches (as evidence)
- Add admin notes
- Manually catalog new demos from S3
- Monitor the processing pipeline (pending/ready/failed counts)

Demos flow through a pipeline: **pending** (cataloged but not parsed) -> **ready** (stats parsed) -> optionally **archived**. Demos can also be **failed** (parser error). A separate scanner daemon catalogs demos automatically, but admins can also catalog manually.

### Authentication

All requests require a JWT token in the `Authorization: Bearer <token>` header. Admin endpoints require the user to have admin privileges (enforced server-side).

### API Error Format (RFC 7807)

All errors return:
```json
{
  "type": "about:blank",
  "title": "Not Found",
  "status": 404,
  "detail": "Demo not found: 550e8400-e29b-41d4-a716-446655440000",
  "request_id": "abc123"
}
```

### Base URL

```
/v1
```

---

## Data Model

### Enums

| Field | Values |
|-------|--------|
| `status` | `pending`, `processing`, `ready`, `failed`, `archived` |
| `category` | `uncategorized`, `pug`, `league`, `scrim`, `ignored` |
| `link_type` | `manual`, `auto_matched`, `evidence` |

### Entity Relationships

```
Demo  1──*  DemoPlayer       (players extracted from parsed demo)
Demo  *──*  TournamentMatch  (via DemoMatchLink join table)
Demo  *──1  Game             (game_id)
Demo  *──1? League           (optional league_id)
Demo  *──1? Tournament       (optional tournament_id)
```

---

## TypeScript Interfaces

Define these in a shared `types/demo.ts` file:

```ts
// === Responses ===

interface DemoResponse {
  id: string                        // UUID
  game_id: string                   // UUID
  file_name: string
  s3_bucket: string
  s3_key: string
  file_size_bytes: number | null
  category: string                  // "uncategorized" | "pug" | "league" | "scrim" | "ignored"
  is_hidden: boolean
  league_id: string | null          // UUID
  tournament_id: string | null      // UUID
  metadata: DemoMetadataResponse | null
  status: string                    // "pending" | "processing" | "ready" | "failed" | "archived"
  stats_fetched_at: string | null   // ISO 8601
  stats_fetch_error: string | null
  categorized_by_user_id: string | null
  categorized_at: string | null
  hidden_by_user_id: string | null
  hidden_at: string | null
  admin_notes: string | null
  discovered_at: string             // ISO 8601
  created_at: string
  updated_at: string
}

interface DemoMetadataResponse {
  map_name: string
  match_date: string | null         // ISO 8601
  team1_name: string
  team2_name: string
  team1_score: number
  team2_score: number
  total_rounds: number
  duration_seconds: number | null
}

interface DemoListResponse {
  demos: DemoResponse[]
  total: number
}

interface DemoPlayerResponse {
  id: string
  demo_id: string
  steam_id: string
  player_name: string
  team_name: string | null
  player_id: string | null          // UUID — linked portal player, if matched
  stats: DemoPlayerStatsResponse
  created_at: string
}

interface DemoPlayerStatsResponse {
  kills: number
  deaths: number
  assists: number
  damage: number
  adr: number                       // Average Damage per Round
  headshot_kills: number
  hs_percentage: number
  kd_ratio: number
}

interface DemoMatchLinkResponse {
  id: string
  demo_id: string
  match_id: string
  game_number: number | null
  link_type: string                 // "manual" | "auto_matched" | "evidence"
  confidence_score: number | null
  validated: boolean
  validated_at: string | null
  validation_result: any | null
  linked_by_user_id: string | null
  linked_at: string
  created_at: string
}

interface DemoMatchLinkWithDemoResponse {
  link: DemoMatchLinkResponse
  demo: DemoResponse
  players?: DemoPlayerResponse[]    // only if include_stats=true
}

interface DemoDownloadResponse {
  id: string
  file_name: string
  s3_bucket: string
  s3_key: string
  download_url: string
}

interface DemoStatusCountsResponse {
  pending: number
  processing: number
  ready: number
  failed: number
  archived: number
}

interface BatchCatalogResultResponse {
  created: DemoResponse[]
  existing: DemoResponse[]
  errors: BatchCatalogErrorResponse[]
}

interface BatchCatalogErrorResponse {
  s3_key: string
  error: string
}

interface DemoValidationResultResponse {
  is_valid: boolean
  confidence: number
  extracted_score: [number, number] | null
  claimed_score: [number, number]
  map_match: boolean
  warnings: string[]
  errors: string[]
}

// === Requests ===

interface CatalogDemoRequest {
  game_id: string                   // UUID — required
  file_name: string                 // 1-512 chars
  s3_bucket: string                 // 1-128 chars
  s3_key: string                    // 1-512 chars
  file_size_bytes?: number | null
}

interface BatchCatalogDemosRequest {
  game_id: string
  demos: BatchCatalogDemoEntry[]    // 1-500 items
}

interface BatchCatalogDemoEntry {
  file_name: string
  s3_bucket: string
  s3_key: string
  file_size_bytes?: number | null
}

interface CategorizeDemoRequest {
  category: string                  // 1-32 chars: "uncategorized" | "pug" | "league" | "scrim" | "ignored"
}

interface SetDemoVisibilityRequest {
  is_hidden: boolean
}

interface AssociateDemoRequest {
  league_id?: string | null         // UUID
  tournament_id?: string | null     // UUID
}

interface LinkDemoToMatchRequest {
  match_id: string                  // UUID — required
  game_number?: number | null       // which game in a Bo3/Bo5 series
  link_type?: string | null         // "manual" | "auto_matched" | "evidence" — defaults to "manual"
}

interface SetDemoNotesRequest {
  notes?: string | null             // max 2000 chars, null to clear
}

interface SubmitDemoStatsRequest {
  map_name?: string | null
  match_date?: string | null        // ISO 8601
  duration_seconds?: number | null
  team1_name?: string | null
  team2_name?: string | null
  team1_score?: number | null
  team2_score?: number | null
  total_rounds?: number | null
  game_metadata?: any | null        // game-specific JSON blob
  raw_stats: any                    // full parser output JSON — required
  players: DemoPlayerInput[]
}

interface DemoPlayerInput {
  steam_id: string
  player_name: string
  team_name?: string | null
  stats: any                        // JSON: { kills, deaths, assists, damage, adr, headshot_kills, hs_percentage }
}

interface MarkDemoFailedRequest {
  error: string                     // 1-2000 chars
}
```

---

## API Endpoints

### Public Endpoints (any authenticated user)

#### 1. List Demos

```
GET /v1/demos
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| `game_id` | UUID | Filter by game |
| `category` | string | Filter by category |
| `status` | string | Filter by status |
| `league_id` | UUID | Filter by league |
| `tournament_id` | UUID | Filter by tournament |
| `map_name` | string | Filter by map name |
| `team_name` | string | Filter by team name (partial match) |
| `steam_id` | string | Filter by player steam ID |
| `match_date_from` | string | ISO 8601 date lower bound |
| `match_date_to` | string | ISO 8601 date upper bound |
| `include_hidden` | bool | Include hidden demos (admin only, default false) |
| `limit` | number | Page size (default 20) |
| `offset` | number | Offset for pagination |

**Response:** `200` -> `{ data: DemoListResponse, request_id: string }`

---

#### 2. Get Demo

```
GET /v1/demos/{id}
```

**Response:** `200` -> `{ data: DemoResponse, request_id: string }`

**Errors:** `404` if demo not found or hidden (non-admin)

---

#### 3. Get Demo Players

```
GET /v1/demos/{id}/players
```

**Response:** `200` -> `{ data: DemoPlayerResponse[], request_id: string }`

---

#### 4. Get Demo Match Links

```
GET /v1/demos/{id}/links
```

**Response:** `200` -> `{ data: DemoMatchLinkResponse[], request_id: string }`

---

#### 5. Get Demo Download URL

```
GET /v1/demos/{id}/download
```

**Response:** `200` -> `{ data: DemoDownloadResponse, request_id: string }`

---

#### 6. Get Demos for Match

```
GET /v1/matches/{match_id}/demos?include_stats=true&game_number=1
```

**Response:** `200` -> `{ data: DemoMatchLinkWithDemoResponse[], request_id: string }`

---

### Admin Endpoints (requires admin role)

#### 7. Catalog Single Demo

```
POST /v1/admin/demos
```

**Body:** `CatalogDemoRequest`

**Response:**
- `201` -> `{ data: DemoResponse }` — newly created
- `200` -> `{ data: DemoResponse }` — already existed (idempotent on s3_key)

---

#### 8. Batch Catalog Demos

```
POST /v1/admin/demos/batch
```

**Body:** `BatchCatalogDemosRequest` (max 500 entries)

**Response:** `200` -> `{ data: BatchCatalogResultResponse }`

The response separates results into `created`, `existing`, and `errors` arrays.

---

#### 9. Categorize Demo

```
POST /v1/admin/demos/{id}/categorize
```

**Body:** `CategorizeDemoRequest`

**Response:** `200` -> `{ data: DemoResponse }`

---

#### 10. Set Demo Visibility

```
POST /v1/admin/demos/{id}/visibility
```

**Body:** `SetDemoVisibilityRequest`

**Response:** `200` -> `{ data: DemoResponse }`

---

#### 11. Associate Demo with League/Tournament

```
POST /v1/admin/demos/{id}/associate
```

**Body:** `AssociateDemoRequest`

**Response:** `200` -> `{ data: DemoResponse }`

---

#### 12. Link Demo to Match

```
POST /v1/admin/demos/{id}/link
```

**Body:** `LinkDemoToMatchRequest`

**Response:** `201` -> `{ data: DemoMatchLinkResponse }`

**Errors:** `409` if link already exists

---

#### 13. Unlink Demo from Match

```
DELETE /v1/admin/demos/{demo_id}/link/{match_id}
```

**Response:** `204` No Content

**Errors:** `404` if link doesn't exist

---

#### 14. Submit Demo Stats

```
POST /v1/admin/demos/{id}/stats
```

**Body:** `SubmitDemoStatsRequest`

**Response:** `200` -> `{ data: DemoResponse }`

Note: This replaces any existing stats/players (idempotent). The demo status transitions to `ready`.

---

#### 15. Mark Demo Stats Failed

```
POST /v1/admin/demos/{id}/stats-failed
```

**Body:** `MarkDemoFailedRequest`

**Response:** `200` -> `{ data: DemoResponse }`

---

#### 16. Delete Demo

```
DELETE /v1/admin/demos/{id}
```

**Response:** `204` No Content

**Errors:** `404` if not found

---

#### 17. Set Admin Notes

```
PATCH /v1/admin/demos/{id}/notes
```

**Body:** `SetDemoNotesRequest`

**Response:** `200` -> `{ data: DemoResponse }`

Pass `{ "notes": null }` to clear notes.

---

#### 18. Get Demo Status Counts (Dashboard)

```
GET /v1/admin/demos/stats
```

**Response:** `200` -> `{ data: DemoStatusCountsResponse }`

---

#### 19. Get Pending Demos

```
GET /v1/admin/demos/pending?limit=50
```

**Response:** `200` -> `{ data: DemoResponse[] }`

---

## Wireframes

### 1. Demo List View (Main Page)

```
+============================================================================+
|  Demo Management                                          [+ Catalog Demo] |
+============================================================================+
|                                                                            |
|  Pipeline Status                                                           |
|  +----------+ +----------+ +----------+ +----------+ +----------+          |
|  | Pending  | | Process. | |  Ready   | |  Failed  | | Archived |          |
|  |    42    | |    3     | |   1,847  | |    12    | |   305    |          |
|  +----------+ +----------+ +----------+ +----------+ +----------+          |
|                                                                            |
|  Filters                                                                   |
|  +------------+ +------------+ +------------+ +-------------+ +----------+ |
|  | Status: All| |Category:All| | Map: All   | | Team name.. | | Search.. | |
|  +------------+ +------------+ +------------+ +-------------+ +----------+ |
|  [ ] Include hidden                    Date: [from ___] - [to ___]         |
|                                                                            |
+----------------------------------------------------------------------------+
|  File Name              | Map       | Score    | Status  | Category | Date |
+----------------------------------------------------------------------------+
|  match_38271.dem.bz2    | de_mirage | 16 - 13  | ready   | league   | 3/4 |
|  match_38270.dem.bz2    | de_inferno| 13 - 16  | ready   | pug      | 3/4 |
|  match_38269.dem.bz2    | --        | --       | pending | --       | 3/3 |
|  match_38268.dem.bz2    | de_nuke   | --       | failed  | --       | 3/3 |
|  * match_38267.dem.bz2  | de_dust2  | 16 - 9   | ready   | scrim    | 3/2 |
|  (hidden)               |           |          |         |          |      |
+----------------------------------------------------------------------------+
|  << 1 2 3 ... 47 >>                                    Showing 1-20 of 924 |
+----------------------------------------------------------------------------+
```

**Behavior:**
- Status count cards are clickable — clicking one filters the table to that status
- Table rows are clickable — opens the detail panel (wireframe 2)
- Hidden demos shown with muted styling and "(hidden)" label when `include_hidden` is checked
- Pending/failed rows show "--" for unparsed metadata fields
- Status column uses colored badges: pending=yellow, processing=blue, ready=green, failed=red, archived=grey
- `[+ Catalog Demo]` button opens the catalog modal (wireframe 3)

---

### 2. Demo Detail / Edit Panel

Opens as a slide-over panel or dedicated route when clicking a row.

```
+====================================================+
|  Demo Detail                              [X Close] |
+====================================================+
|                                                      |
|  match_38271.dem.bz2                                 |
|  Status: [ready]   Category: [league v]              |
|                                                      |
|  Visibility: ( ) Public  (*) Hidden    [Save]        |
|                                                      |
+------------------------------------------------------+
|  Metadata                                            |
|  Map: de_mirage   Date: 2026-03-04 21:30            |
|  Score: team_Alpha 16 - 13 team_Bravo               |
|  Rounds: 29   Duration: 42m 18s                     |
+------------------------------------------------------+
|  Players                                             |
|  Team Alpha                    Team Bravo            |
|  +------------------------+   +------------------------+
|  | Player    K  D  A  ADR |   | Player    K  D  A  ADR |
|  | Alpha1   24 18  5 82.3 |   | Bravo1   20 22  7 75.1 |
|  | Alpha2   22 19  8 79.0 |   | Bravo2   19 21  4 68.9 |
|  | Alpha3   21 17  6 88.1 |   | Bravo3   18 23  9 72.4 |
|  | Alpha4   19 20  7 71.2 |   | Bravo4   17 20  5 65.8 |
|  | Alpha5   16 19  4 63.5 |   | Bravo5   15 22  3 61.2 |
|  +------------------------+   +------------------------+
+------------------------------------------------------+
|  Match Links                          [+ Link Match] |
|  +------------------------------------------------+  |
|  | Match #1204 (Bo3 G1) | manual | [Unlink]       |  |
|  | Match #1198 (Bo1)    | auto   | [Unlink]       |  |
|  +------------------------------------------------+  |
+------------------------------------------------------+
|  Association                                         |
|  League: [Select league...       v]                  |
|  Tournament: [Select tournament. v]    [Save]        |
+------------------------------------------------------+
|  Admin Notes                                         |
|  +----------------------------------------------+   |
|  | Reviewed — score confirmed, demo is clean.    |   |
|  +----------------------------------------------+   |
|  [Save Notes]                                        |
+------------------------------------------------------+
|  Info                                                |
|  S3: cs2-demos / matches/2026/03/match_38271.dem.bz2|
|  Size: 48.2 MB                                       |
|  Discovered: 2026-03-04 22:01                        |
|  Stats parsed: 2026-03-04 22:03                      |
|  [Download Demo]                      [Delete Demo]  |
+======================================================+
```

**Behavior:**
- Category dropdown saves immediately on change via `POST /categorize`
- Visibility radio saves on `[Save]` click via `POST /visibility`
- `[+ Link Match]` opens the link modal (wireframe 4)
- `[Unlink]` calls `DELETE /link/{match_id}` with confirmation dialog
- Association dropdowns save on `[Save]` click via `POST /associate`
- `[Save Notes]` calls `PATCH /notes`
- `[Download Demo]` calls `GET /download` and opens the returned URL
- `[Delete Demo]` requires confirmation dialog, then calls `DELETE /demos/{id}`
- If status is `failed`, show the `stats_fetch_error` in a red alert box above metadata

---

### 3. Catalog Demo Modal

```
+================================================+
|  Catalog New Demo(s)                    [X]    |
+================================================+
|                                                 |
|  Mode: (*) Single   ( ) Batch                  |
|                                                 |
|  Game: [CS2                           v]       |
|                                                 |
|  ---- Single Mode ----                         |
|  File Name:  [match_38275.dem.bz2      ]       |
|  S3 Bucket:  [cs2-10mans-demo-files    ]       |
|  S3 Key:     [matches/2026/03/match... ]       |
|  File Size:  [52428800                 ] bytes  |
|                                                 |
|  ---- Batch Mode ----                          |
|  S3 Bucket:  [cs2-10mans-demo-files    ]       |
|  S3 Prefix:  [matches/2026/03/         ]       |
|  (Lists matching .dem files from bucket)       |
|  +-------------------------------------------+ |
|  | [x] match_38275.dem.bz2       51.2 MB     | |
|  | [x] match_38276.dem.bz2       48.7 MB     | |
|  | [ ] match_38277.dem.bz2       49.1 MB     | |
|  +-------------------------------------------+ |
|  Selected: 2 of 3                              |
|                                                 |
|          [Cancel]              [Catalog]        |
+================================================+
```

**Behavior:**
- Single mode calls `POST /admin/demos` with `CatalogDemoRequest`
- Batch mode calls `POST /admin/demos/batch` with `BatchCatalogDemosRequest`
- After batch, show result summary: "Created: 2, Already existed: 1, Errors: 0"
- S3 bucket field should remember the last used value (localStorage)
- Note: The backend doesn't have an S3 listing endpoint — batch mode is for when the admin already knows the S3 keys. Populate the file list from a pasted list or manual entry. Consider a textarea for pasting multiple S3 keys (one per line).

---

### 4. Link to Match Modal

```
+================================================+
|  Link Demo to Match                     [X]    |
+================================================+
|                                                 |
|  Search match: [Tournament name or ID...   ]   |
|                                                 |
|  Results:                                       |
|  +-------------------------------------------+ |
|  | Match #1210 — Alpha vs Bravo              | |
|  |   Spring Cup / Round of 16 / Bo3          | |
|  +-------------------------------------------+ |
|  | Match #1209 — Gamma vs Delta              | |
|  |   Spring Cup / Quarterfinals / Bo1        | |
|  +-------------------------------------------+ |
|                                                 |
|  Selected: Match #1210                         |
|                                                 |
|  Game Number: [1   ] (for Bo3/Bo5 series)      |
|  Link Type:   [manual           v]             |
|               manual | auto_matched | evidence |
|                                                 |
|          [Cancel]                [Link]         |
+================================================+
```

**Behavior:**
- Match search hits your existing tournament match search/list endpoint
- After linking, the new link appears in the detail panel's Match Links section
- `409` response means the link already exists — show a toast

---

### 5. Failed/Pending Demo Actions

When viewing a demo with `status: failed`:

```
+------------------------------------------------------+
|  ! Stats Processing Failed                           |
|  Error: "Parse error: unexpected EOF at offset 4821" |
|                                                      |
|  [Retry Stats]   [Submit Stats Manually]             |
+------------------------------------------------------+
```

**Behavior:**
- `[Retry Stats]` — re-submits to the scanner pipeline (call `POST /stats-failed` with a retry message, or simply re-catalog)
- `[Submit Stats Manually]` — opens an inline form to fill in `SubmitDemoStatsRequest` fields and call `POST /stats`
- The manual stats form should have fields for map, teams, scores, rounds, duration, and a JSON editor for raw_stats and players

---

## CRUD Workflow Summary

### Create (Catalog)
1. Admin clicks `[+ Catalog Demo]` -> modal opens
2. Fills S3 coordinates + game selection
3. `POST /admin/demos` (single) or `POST /admin/demos/batch` (batch)
4. New demo(s) appear in table with `status: pending`
5. Toast: "Demo cataloged" or batch result summary

### Read (Browse & Inspect)
1. Page loads -> `GET /admin/demos/stats` for pipeline counts + `GET /demos?include_hidden=true&limit=20` for initial table
2. Admin applies filters -> re-fetch with query params
3. Admin clicks row -> `GET /demos/{id}` + `GET /demos/{id}/players` + `GET /demos/{id}/links` (parallel)
4. Download -> `GET /demos/{id}/download` -> open URL in new tab

### Update (Categorize, Hide, Associate, Notes, Link)
Each action is a separate endpoint. After success, update the local demo object from the response (the API returns the updated `DemoResponse`).

| Action | Endpoint | Trigger |
|--------|----------|---------|
| Change category | `POST /admin/demos/{id}/categorize` | Dropdown change |
| Toggle visibility | `POST /admin/demos/{id}/visibility` | Radio + Save |
| Set league/tournament | `POST /admin/demos/{id}/associate` | Dropdowns + Save |
| Edit admin notes | `PATCH /admin/demos/{id}/notes` | Save Notes button |
| Link to match | `POST /admin/demos/{id}/link` | Link modal submit |
| Unlink from match | `DELETE /admin/demos/{demo_id}/link/{match_id}` | Unlink button + confirm |
| Submit stats | `POST /admin/demos/{id}/stats` | Manual stats form |
| Mark failed | `POST /admin/demos/{id}/stats-failed` | Retry/mark failed action |

### Delete
1. Admin clicks `[Delete Demo]` in detail panel
2. Confirmation dialog: "Delete match_38271.dem.bz2? This cannot be undone."
3. `DELETE /admin/demos/{id}`
4. Close detail panel, remove row from table, show toast

---

## Composable Suggestions

```ts
// useDemos() — list management
// - demos: Ref<DemoResponse[]>
// - total: Ref<number>
// - loading: Ref<boolean>
// - filters: Ref<ListDemosQuery>
// - fetchDemos(): re-fetch with current filters
// - Uses watchDebounced on filters to auto-refetch

// useDemoDetail(id) — single demo CRUD
// - demo: Ref<DemoResponse | null>
// - players: Ref<DemoPlayerResponse[]>
// - links: Ref<DemoMatchLinkResponse[]>
// - loading: Ref<boolean>
// - categorize(category): Promise
// - setVisibility(hidden): Promise
// - associate(leagueId, tournamentId): Promise
// - setNotes(notes): Promise
// - linkToMatch(req): Promise
// - unlinkFromMatch(matchId): Promise
// - deleteDemmo(): Promise
// - downloadUrl(): Promise<string>

// useDemoStats() — pipeline dashboard
// - counts: Ref<DemoStatusCountsResponse>
// - fetchCounts(): refresh counts
// - Uses polling or manual refresh

// useDemoCatalog() — catalog workflows
// - catalogSingle(req): Promise<DemoResponse>
// - catalogBatch(req): Promise<BatchCatalogResultResponse>
// - submitStats(id, req): Promise<DemoResponse>
// - markFailed(id, error): Promise<DemoResponse>
```

---

## Implementation Notes

- **Pagination**: Use `limit` + `offset` query params. The `total` field in `DemoListResponse` gives total count for computing page numbers.
- **Optimistic updates**: For quick actions (categorize, visibility, notes), update the local state immediately and roll back on error.
- **Polling**: Consider polling `GET /admin/demos/stats` every 30-60s to keep pipeline counts fresh, since the scanner daemon is continuously processing.
- **File sizes**: Format `file_size_bytes` as human-readable (e.g., "48.2 MB").
- **Timestamps**: Format all ISO 8601 timestamps in the user's local timezone.
- **Empty states**: Show appropriate messages for empty tables ("No demos found"), no players (demo not parsed yet), no links, etc.
- **Status transitions**: Demos in `pending` or `failed` status won't have metadata or players — conditionally render those sections.
- **URL routing**: Consider `/admin/demos` for the list and `/admin/demos/:id` for the detail view (or use a slide-over panel if you prefer).
