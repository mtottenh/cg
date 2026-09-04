# Frontend Task: Player Profile — Dual Stats Cards

## Overview

The player profile page needs two separate stats cards for CS2 players:

1. **Public Matchmaking Card** — Shows the player's Steam matchmaking stats: CS Rating (ELO), rank tier, K/D/A, win rate, headshot %, and recent match history
2. **League/Tournament Card** — Shows the player's tournament stats from our platform: W/L record, win streak, plugin-formatted display stats (KDA, ADR, etc. from parsed demos)

These are backed by different data sources and different API endpoints. Public MM stats come from Steam Game Coordinator data. Tournament stats come from our match completion pipeline and demo analysis.

## API Endpoints

Base URL: `/v1/players/{player_id}/games/{game_id}`

The `game_id` parameter accepts either a UUID or a slug (e.g., `cs2`).

### 1. Public MM Stats — `GET /mm-stats`

Returns aggregate stats from all public matchmaking games.

**Response:**
```json
{
  "data": {
    "rating": 11297,
    "peak_rating": 11400,
    "rank_tier": "Gold 2",
    "rank_color": "#FFD700",
    "matches_played": 27,
    "wins": 10,
    "losses": 15,
    "draws": 2,
    "win_rate": 37.037,
    "kills": 310,
    "deaths": 445,
    "assists": 116,
    "kd_ratio": 0.6966,
    "headshots": 141,
    "hs_percent": 45.483,
    "mvps": 48,
    "entry_3k": 0,
    "entry_4k": 0,
    "entry_5k": 0,
    "first_match_at": "2026-01-31T23:45:12+00:00",
    "last_match_at": "2026-03-04T23:13:45+00:00"
  },
  "request_id": "abc123"
}
```

**Notes:**
- Returns `404` if the player has no public MM data yet
- `rank_tier` and `rank_color` may be `null` if the player hasn't been ranked yet
- `rating` is the CS2 Premier rating (0-35000+); for Competitive/Wingman it would be a tier ID (1-18) but we currently only track Premier
- All combat stats (kills, deaths, assists, etc.) are lifetime aggregates across all public matches

### 2. Match History — `GET /match-history?limit=20&offset=0`

Returns paginated individual match results from public matchmaking.

**Query params:**
- `limit` (optional, default 20, max 100)
- `offset` (optional, default 0)

**Response:**
```json
{
  "data": [
    {
      "id": "4c19c984-ca58-4dd8-9c93-ce899480e4bb",
      "map": "de_inferno",
      "match_time": "2026-03-04T23:13:45+00:00",
      "team_scores": [13, 8],
      "match_duration_secs": 2340,
      "match_result": "loss",
      "kills": 8,
      "deaths": 16,
      "assists": 2,
      "score": 22,
      "headshots": 3,
      "mvps": 0
    },
    {
      "id": "6e003125-08bb-47ce-ba6a-8255aadfdc88",
      "map": "",
      "match_time": "2026-03-02T23:20:55+00:00",
      "team_scores": [1, 13],
      "match_duration_secs": 1680,
      "match_result": "loss",
      "kills": 4,
      "deaths": 14,
      "assists": 1,
      "score": 11,
      "headshots": 1,
      "mvps": 0
    }
  ],
  "request_id": "abc123"
}
```

**Notes:**
- Ordered by `match_time` descending (most recent first)
- `map` may be an empty string for older matches where the enricher didn't extract it from the demo. New matches will have the map name populated (e.g., `de_inferno`, `de_dust2`, `de_mirage`).
- `match_result` is one of: `"win"`, `"loss"`, `"draw"`
- `team_scores` is `[team1_score, team2_score]` — the player's team isn't indicated by position, use `match_result` instead

### 3. Rating History — `GET /rating-history?limit=100`

Returns the player's CS Rating over time (for graphing).

**Response:**
```json
{
  "data": [
    {
      "id": "4c19c984-...",
      "player_id": "019cadeb-...",
      "game_id": "403627ae-...",
      "rating": 11297,
      "source": "demo_rank_update",
      "recorded_at": "2026-02-25T21:17:16+00:00",
      "created_at": "2026-03-06T13:44:50+00:00"
    }
  ],
  "request_id": "abc123"
}
```

**Notes:**
- Ordered by `recorded_at` descending
- `recorded_at` is when the match was actually played (not when we ingested it)
- Use this to render a rating-over-time line chart
- Entries with `rating: 0` may exist for pre-Premier placement matches; filter these out for charting

### 4. League/Tournament Stats — `GET /` (existing)

Returns the player's tournament/league profile with plugin-formatted display stats.

**Response:**
```json
{
  "data": {
    "id": "550e8400-...",
    "player_id": "019cadeb-...",
    "game_id": "403627ae-...",
    "matches_played": 0,
    "wins": 0,
    "losses": 0,
    "draws": 0,
    "win_rate": 0.0,
    "win_streak": 0,
    "best_win_streak": 0,
    "display_stats": [
      {
        "key": "elo_current",
        "label": "CS Rating",
        "value": "11,297",
        "category": "Rating",
        "sort_order": 1,
        "color": "#FFD700"
      },
      {
        "key": "kd_ratio",
        "label": "K/D Ratio",
        "value": "0.00",
        "category": "Combat",
        "sort_order": 20
      }
    ],
    "first_match_at": null,
    "last_match_at": null
  },
  "request_id": "abc123"
}
```

**Notes:**
- `matches_played`, `wins`, `losses` etc. here are **tournament-only** counts (will be 0 until the player competes in platform tournaments)
- `display_stats` are plugin-formatted and grouped by `category` — render them grouped
- The `"Rating"` category stats here (elo_current, elo_peak) come from the same source as the public MM card's rating — they overlap. On the league card, you may want to hide the Rating category stats and only show Combat/General stats once the player has tournament matches

## Suggested UI Layout

```
Player Profile Page
├── Player Info (avatar, name, steam link, etc.)
│
├── Public Matchmaking Card
│   ├── Header: "Public Matchmaking"
│   ├── Rating section
│   │   ├── Current CS Rating: 11,297 (with rank_color badge)
│   │   ├── Peak Rating: 11,400
│   │   └── Rank Tier: "Gold 2" (colored badge)
│   ├── Rating History Chart (line graph from /rating-history)
│   ├── Stats grid (2-3 columns)
│   │   ├── Matches: 27
│   │   ├── W/L/D: 10-15-2
│   │   ├── Win Rate: 37.0%
│   │   ├── K/D: 0.70
│   │   ├── HS%: 45.5%
│   │   └── MVPs: 48
│   └── Recent Matches table (from /match-history)
│       ├── Map | Date | Score | K/D/A | Result
│       ├── de_inferno | Mar 4 | 13-8 | 8/16/2 | Loss
│       ├── ??? | Mar 2 | 1-13 | 4/14/1 | Loss
│       └── [Load more...]
│
├── League/Tournament Card
│   ├── Header: "Tournament Stats"
│   ├── Record: 0-0-0 (W/L/D)
│   ├── Win Streak: 0 | Best: 0
│   ├── Display Stats (from display_stats, grouped by category)
│   │   ├── Combat: K/D, Kills, Deaths, Assists, HS%, ADR
│   │   └── General: Matches, Win Rate
│   └── (Empty state: "No tournament matches yet")
```

## Implementation Notes

- **Fetch both cards in parallel** — the endpoints are independent
- **Handle 404 on mm-stats gracefully** — show an empty state ("No public matchmaking data") if the player hasn't been tracked yet
- **Rating chart**: filter out `rating: 0` entries, plot `recorded_at` (x-axis) vs `rating` (y-axis)
- **Match history pagination**: start with 10 entries, add "Load more" button that increments `offset`
- **Map name may be empty**: for matches where `map` is `""`, show a placeholder ("Unknown" or a generic icon)
- **match_result coloring**: win = green, loss = red, draw = yellow/neutral
- **K/D ratio formatting**: 2 decimal places (e.g., "0.70", "1.45")
- **Win rate**: 1 decimal place with % suffix
- **HS%**: 1 decimal place with % suffix
- **Responsive**: cards should stack vertically on mobile, side-by-side on desktop

## Swagger Docs

Full OpenAPI documentation is available at:
- **Swagger UI**: `http://localhost:3000/swagger-ui`
- **OpenAPI JSON**: `http://localhost:3000/api-docs/openapi.json`

Search for the `players` tag to find all player-related endpoints.
