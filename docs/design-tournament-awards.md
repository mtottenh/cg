# Design: Tournament & Season Awards ("Swag 7", "Blind Monk", …)

Status: proposed 2026-07-19. Builds on the statistics-pipeline audit of the
same date (see `docs/` and the leaderboard gap list).

## 1. The product, precisely

A tournament (or league season) can carry a set of **awards**. Each award is
an organizer-branded name over a computable statistic: "Swag 7" = most MAG-7
kills, "Blind Monk" = most kills while flashed, "Headshot Machine" = most
headshot kills. Standings are live during play; on completion the award is
**finalized** — a permanent result with a winner (and podium) that appears on
the tournament page and in the winner's profile trophy case.

The naming requirement is the tell: the award space is long-tail and
creative. Organizers will invent awards we never anticipated. Therefore the
design's first law: **adding a new award must never require a code deploy.**

## 2. Concept model — three things, kept separate

The feature entangles three concepts; the design keeps them orthogonal:

1. **Stat** — a computable per-player quantity over demo data
   (`kills.weapon.mag7`, `kills.while_blind`, `headshot_kills`, `adr`).
   Game-specific in derivation, uniform in interface: (player, demo) → value.
2. **Award** — a *named, presented* claim over a stat within a scope:
   name, description, icon/color, the stat key, an aggregation
   (`sum` | `max_single_demo` | `avg_per_round`), a direction
   (`desc`, or `asc` for "fewest deaths"-style awards), and an optional
   qualifier (min rounds/matches — essential for ratio stats).
3. **Scope** — the aggregation boundary: `tournament` or `league_season`
   now; the model deliberately admits `match` (MVP-style) and `all_time`
   later. Awards also reserve `subject_type: player | team` (v1: player).

Plain leaderboards (previous audit's goal) fall out for free: a leaderboard
is an unnamed award; both ride the same aggregation engine.

## 3. Data architecture

### 3.1 Stat catalog (per game, plugin-owned)

`stat_definitions`: `(game_id, stat_key, label, category, value_type
count|ratio|duration, derivation, unit, description)`.

The **CS2 plugin** declares its catalog exactly as it declares veto formats
— the award engine itself is game-agnostic. Derivations for CS2 v1:

- **summary field** — copy a scalar from `player_summaries` (headshot_kills,
  adr, flash_assists, utility_damage, wallbangs, smoke_kills, blind_kills,
  bomb_plants, …).
- **weapon map entry** — explode `weapon_kills` into
  `kills.weapon.{name}` rows. Covers every "X-weapon kills" award —
  shotguns, MAG-7, knife, AWP — without per-weapon code.
- **event-derived** (phase 2) — computed by walking `rounds[].events[]`
  (e.g. pistol-round kills, 1vN clutches). Kept behind the same catalog
  interface so awards don't care how a stat is derived.

Composite stats (K/D and other arithmetic) are **plugin-defined derived
stats**, not user-authored expressions. No user-facing expression language
in v1 — the constrained (stat_key, aggregation, direction, qualifier) tuple
is the entire authoring surface, which keeps queries safe and plannable.

### 3.2 Fact store (EAV, extracted once at ingest)

`demo_player_stats`: `(demo_id, steam_id, player_id NULLABLE, stat_key,
value NUMERIC)` with indexes on `(stat_key, demo_id)` and
`(demo_id, steam_id)`.

Why EAV over wide columns: the stat space is open (every weapon × every
modifier); wide tables need a migration per stat, EAV plus the catalog is
runtime-extensible; and the leaderboard access pattern is exactly
EAV-shaped (`WHERE stat_key = $1 … GROUP BY player ORDER BY SUM(value)`).
Volume is a non-issue: ~50 keys × 10 players ≈ 500 rows/demo; 10k demos ≈
5M rows.

Extraction runs inside `save_demo_stats` (same idempotency pattern:
delete-and-reinsert per demo), walking `stats_json` against the game's
catalog. `stats_json` stays the immutable source of truth; facts are a
derived projection and carry an `extractor_version` so re-extraction after
catalog growth is a backfill job, not a schema event. The existing 7-column
`demo_players` table remains for the per-demo scoreboard UI; facts are the
aggregation surface.

### 3.3 Prerequisites in the pipeline (from the audit — unchanged)

1. **Auto-link demos to tournament matches at ingest** (steam-ID overlap +
   `matches_timeframe()` scoring → `demo_match_links` with
   `link_type='auto_matched'` + confidence; also stamp
   `demos.tournament_id`). Without this every scope query is empty. This is
   the single hard blocker and is worth building first regardless of awards.
2. **Identity resolution**: resolve `steam_id → player_id` at extraction
   (players.steam_id_64). Facts keep both; award standings rank only
   resolved players (you cannot hand a trophy to an anonymous steam id);
   the existing `link_to_player` backfill covers late account linking.
3. **Parser enrichment** (we own `portal-demo-stats` now — no external
   dependency): (a) emit `weapon_kills` keyed by **weapon name** (the
   consumer schema is `HashMap<String, i32>`, so name keys are
   drop-in; legacy external demos used opaque numeric IDs — standardize on
   names and re-parse historical demos through our own service rather than
   maintaining an ID mapping); (b) pin down the blind-kill semantics
   explicitly: `blind_kills` = kills scored **while the attacker is
   flashed** ("Blind Monk"), `blinded_kills` = kills of flashed victims —
   document both in the schema and test them; (c) phase 2: richer round
   events for event-derived stats.

## 4. Award definitions & lifecycle

### 4.1 Tables

`award_templates` — per-game, seeded + admin-extensible: the default
catalog organizers pick from ("Headshot Machine", "Swag 7", "Blind Monk",
"The Ninja" = most bomb defuses, "Utility King" = most utility damage …).
Template = default name/icon/description + metric tuple.

`awards` — instances: `(id, scope_type, scope_id, game_id, name,
description, icon, color, stat_key, aggregation, direction,
min_qualifier {rounds|matches, n}, subject_type, status
active|finalized|void, created_by, template_id NULLABLE)`.

`award_results` — immutable podium snapshot on finalization:
`(award_id, rank, player_id, value, matches_counted, finalized_at,
finalized_by trigger)`. Top 3 stored; rank shared on ties.

### 4.2 Lifecycle

- **Authoring**: organizer (RBAC: `tournament.settings.manage`, scoped —
  already enforced) attaches templates or builds custom awards from the
  stat catalog at any point before finalization. Rename/re-icon freely.
- **Live standings**: computed on read —
  `facts ⋈ demos ⋈ demo_match_links ⋈ tournament_matches` filtered to
  scope, `GROUP BY player`, qualifier applied, top-N + caller's own rank.
  Cheap at our scale; add a short TTL cache only if measurement demands it.
  No materialized views in v1.
- **Finalization**: hooked into the **lifecycle automation loop** —
  when a tournament reaches `completed` (season: closes), snapshot
  `award_results` and flip status. Idempotent and admin-re-runnable
  (recompute) until the tournament is `finalized`, then locked. Late demo
  parses before finalization simply flow into the next read; after locking
  they no longer matter — history is stable.
- **Ties**: shared rank, all tied players stored and displayed ("shared").
  No fabricated tiebreaks in v1; the field exists to add one later.
- **Void**: organizer can void an award (never delete a finalized one).

## 5. API surface

- `GET /v1/games/{game}/stat-catalog` — for the award-builder UI.
- `GET /v1/games/{game}/award-templates`
- `GET/POST /v1/tournaments/{id}/awards`, `PATCH/DELETE .../awards/{aid}`
  (organizer-scoped); same under `/v1/league-seasons/{id}/awards`.
- `GET .../awards/{aid}/standings?limit=` — live leaderboard.
- `POST .../awards/{aid}/finalize` — admin manual trigger (automation does
  it on completion).
- `GET /v1/players/{id}/awards` — trophy case.
- `GET /v1/tournaments/{id}/leaderboards?stat_key=&agg=` — the plain
  (unnamed) leaderboard, same engine.

## 6. Frontend surface

- **Tournament detail → new "Awards" tab**: award cards (name, icon,
  podium top-3, expandable standings, "your rank"). Live during play,
  trophy-styled once finalized.
- **Season page**: identical component, season scope (this also gives the
  league page its first stats content).
- **Organizer admin**: award manager — template checklist on tournament
  creation + custom builder (searchable stat dropdown from the catalog,
  name/icon/color, aggregation, qualifier).
- **Player profile**: trophy case section (finalized awards with
  tournament context) beside the existing stat cards.
- **Match detail**: wire the existing dead `fetchDemoStats` into a
  scoreboard; reuse stat labels from the catalog so naming is consistent.
- De-hardcode `GAME_SLUG='cs2'` in `usePlayerStats` along the way.

## 7. Phasing

1. **Foundations** (independently valuable): demo→match auto-linking +
   backfill; identity resolution; parser `weapon_kills`-by-name + blind
   semantics; re-parse historical demos.
2. **Stats core**: catalog + CS2 plugin definitions; EAV extraction +
  `extractor_version` backfill; plain leaderboard endpoint; tournament
  "Stats" tab (top fraggers etc.).
3. **Awards**: templates + instances + standings + finalization hook +
   awards tab + organizer builder + trophy case.
4. **Later**: event-derived stats (clutches, pistol rounds), match-scope
   MVP awards, team-subject awards, expression-composed stats, tiebreak
   policies, cross-tournament "hall of fame".

## 8. Explicitly rejected alternatives

- **Hardcoded metric enum in Rust** — every new award becomes a deploy;
  kills the long tail that motivates the feature.
- **Query-time JSONB aggregation over `stats_json`** — unindexable
  nested maps, weapon-ID keys, and every leaderboard read pays full-blob
  traversal. Extraction-once wins.
- **User-authored expressions/SQL** — unbounded query surface and a
  security/perf tarpit; the constrained tuple covers the named use cases,
  and composite stats belong in the plugin catalog.
- **Wide stat columns on `demo_players`** — migration-per-stat; contradicts
  runtime extensibility. (The existing 7 columns stay for the scoreboard.)
