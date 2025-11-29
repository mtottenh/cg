# API Routes Reference
## Multi-Game Competitive Gaming Portal

**Version:** 1.0  
**Base URL:** `https://api.gaming-portal.com`  
**API Version:** v1  
**Last Updated:** November 2024

---

## Table of Contents

1. [Overview](#1-overview)
2. [Authentication](#2-authentication)
3. [Users](#3-users)
4. [Players](#4-players)
5. [Teams](#5-teams)
6. [Games](#6-games)
7. [Matchmaking](#7-matchmaking)
8. [Matches](#8-matches)
9. [Lobbies](#9-lobbies)
10. [Tournaments](#10-tournaments)
11. [Leagues & Seasons](#11-leagues--seasons)
12. [Substitutes](#12-substitutes)
13. [Game Servers](#13-game-servers)
14. [Admin](#14-admin)
15. [WebSocket Events](#15-websocket-events)
16. [Webhooks](#16-webhooks)

---

## 1. Overview

### 1.1 Core Data Model Concepts

Before diving into the API, understand these key relationships:

**Players & Teams (M:N Relationship):**
- Players can belong to **multiple teams** simultaneously
- Teams can be created by **any authenticated player**
- Team creators automatically become **captains** (team admin role) with `is_founder = true`
- Captains can: invite/remove members, promote others to captain, manage team settings
- Founders cannot be demoted; there must always be at least one captain
- Role hierarchy: `captain` > `officer` > `player` > `substitute` | `coach` | `manager`

**Games & Statistics (Plugin-Driven):**
- Each game has a plugin that defines available maps, rank tiers, and stats schema
- Players have **per-game profiles** with rating (Glicko-2) and game-specific statistics
- Platform manages: rating, rating_deviation, volatility, win/loss counts
- Plugin defines: game-specific stats schema (e.g., K/D ratio, ADR for CS2)
- Stats are calculated by the game's plugin via `calculate_player_stats()` after each match

**Leagues & Divisions (Game-Specific):**
- Leagues are **game-specific** (each league belongs to exactly one game)
- Games can have **multiple leagues** representing divisions (Div 1, Div 2, etc.)
- Leagues support hierarchy via `parent_league_id`, `division`, and `tier_name`
- League access types:
  - `open` - Any player can join
  - `invite_only` - Requires invitation from league admin
  - `application` - Player applies, admin approves
- Players can belong to **zero or more leagues** (membership tracked separately from seasons)
- Must be a league member to participate in league seasons/tournaments

**Tournaments (Global vs League-Specific):**
- **Global tournaments**: Created by platform admins, open to all players (`league_id = null`)
- **League tournaments**: Created by league admins, restricted to league members (`league_id` set)
- **Multiple tournaments can run concurrently** - no restrictions on overlapping tournaments
- Map pools can be customized per tournament if the game plugin's `supports_custom_map_pool()` returns true
- Map pool priority: tournament custom pool → league default pool → game default pool

**Permission Summary:**
| Action | Who Can Do It |
|--------|---------------|
| Create a team | Any authenticated player |
| Manage team (invite, kick, settings) | Team captain |
| Create global tournament | Platform admin |
| Create league tournament | League admin |
| Customize tournament map pool | Tournament creator (if plugin allows) |
| Join open league | Any authenticated player |
| Join invite-only league | Must have invitation |

### 1.2 Base URL Structure

```
https://api.gaming-portal.com/v1/{resource}
```

### 1.3 Authentication

All authenticated endpoints require a Bearer token:

```
Authorization: Bearer <access_token>
```

### 1.3 Common Headers

| Header | Description | Required |
|--------|-------------|----------|
| `Authorization` | Bearer token for auth | For protected routes |
| `Content-Type` | `application/json` | For POST/PUT/PATCH |
| `Accept` | `application/json` | Recommended |
| `X-Request-ID` | Client-generated request ID | Optional |
| `X-Idempotency-Key` | For idempotent operations | For mutations |

### 1.4 Standard Response Format

**Success Response:**
```json
{
  "data": { ... },
  "meta": {
    "request_id": "req_abc123",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

**Paginated Response:**
```json
{
  "data": [ ... ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total_items": 150,
    "total_pages": 8
  }
}
```

**Error Response (RFC 7807):**
```json
{
  "type": "https://api.gaming-portal.com/problems/validation-error",
  "title": "Validation Failed",
  "status": 400,
  "detail": "One or more fields failed validation",
  "instance": "/v1/teams",
  "errors": [
    {
      "field": "name",
      "message": "Name must be between 3 and 64 characters",
      "code": "length"
    }
  ]
}
```

### 1.5 Common Query Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `page` | integer | Page number (default: 1) |
| `per_page` | integer | Items per page (default: 20, max: 100) |
| `sort` | string | Sort field (prefix with `-` for desc) |
| `fields` | string | Comma-separated fields to include |
| `include` | string | Related resources to embed |

### 1.6 Rate Limiting

Rate limit headers returned on all responses:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1642248000
```

---

## 2. Authentication

### 2.1 Register

Create a new user account.

```
POST /v1/auth/register
```

**Request Body:**
```json
{
  "username": "player123",
  "email": "player@example.com",
  "password": "SecurePass123!",
  "display_name": "Player One",
  "accept_terms": true,
  "captcha_token": "recaptcha_token_here"
}
```

**Response:** `201 Created`
```json
{
  "data": {
    "user": {
      "id": "usr_abc123",
      "username": "player123",
      "email": "player@example.com",
      "email_verified": false,
      "created_at": "2024-01-15T10:30:00Z"
    },
    "player": {
      "id": "ply_def456",
      "display_name": "Player One"
    },
    "access_token": "eyJhbGc...",
    "refresh_token": "eyJhbGc...",
    "expires_in": 900
  }
}
```

### 2.2 Login

Authenticate with username/email and password.

```
POST /v1/auth/login
```

**Request Body:**
```json
{
  "login": "player@example.com",
  "password": "SecurePass123!",
  "remember_me": true,
  "device_name": "Chrome on Windows"
}
```

**Response:** `200 OK`
```json
{
  "data": {
    "access_token": "eyJhbGc...",
    "refresh_token": "eyJhbGc...",
    "expires_in": 900,
    "user": {
      "id": "usr_abc123",
      "username": "player123",
      "email": "player@example.com",
      "two_factor_enabled": false
    }
  }
}
```

**With 2FA Required:** `200 OK`
```json
{
  "data": {
    "requires_2fa": true,
    "two_factor_token": "2fa_xyz789"
  }
}
```

### 2.3 Two-Factor Verification

```
POST /v1/auth/2fa/verify
```

**Request Body:**
```json
{
  "two_factor_token": "2fa_xyz789",
  "code": "123456"
}
```

### 2.4 Refresh Token

```
POST /v1/auth/refresh
```

**Request Body:**
```json
{
  "refresh_token": "eyJhbGc..."
}
```

**Response:** `200 OK`
```json
{
  "data": {
    "access_token": "eyJhbGc...",
    "refresh_token": "eyJhbGc...",
    "expires_in": 900
  }
}
```

### 2.5 Logout

```
POST /v1/auth/logout
```

**Request Body:**
```json
{
  "refresh_token": "eyJhbGc...",
  "all_devices": false
}
```

### 2.6 Steam Login

```
GET /v1/auth/steam/login
```

Redirects to Steam OpenID login page.

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `return_url` | string | URL to redirect after auth |

### 2.7 Steam Callback

```
GET /v1/auth/steam/callback
```

Handles Steam OpenID callback. Returns tokens or redirects to frontend.

### 2.8 Link Steam Account

Link Steam to existing authenticated account.

```
POST /v1/auth/steam/link
```

**Query Parameters:** Steam OpenID callback parameters

**Response:** `200 OK`
```json
{
  "data": {
    "provider": "steam",
    "provider_user_id": "76561198012345678",
    "steam_persona_name": "PlayerName",
    "linked_at": "2024-01-15T10:30:00Z"
  }
}
```

### 2.9 OAuth Providers

```
GET /v1/auth/oauth/{provider}/authorize
GET /v1/auth/oauth/{provider}/callback
POST /v1/auth/oauth/{provider}/link
```

**Supported Providers:** `discord`, `twitch`, `google`

### 2.10 Password Reset

**Request Reset:**
```
POST /v1/auth/password/reset-request
```

```json
{
  "email": "player@example.com"
}
```

**Complete Reset:**
```
POST /v1/auth/password/reset
```

```json
{
  "token": "reset_token_here",
  "password": "NewSecurePass123!"
}
```

---

## 3. Users

### 3.1 Get Current User

```
GET /v1/users/me
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "usr_abc123",
    "username": "player123",
    "email": "player@example.com",
    "email_verified": true,
    "two_factor_enabled": false,
    "locale": "en-US",
    "timezone": "America/New_York",
    "created_at": "2024-01-15T10:30:00Z",
    "player": {
      "id": "ply_def456",
      "display_name": "Player One"
    },
    "linked_accounts": [
      {
        "provider": "steam",
        "provider_username": "SteamPlayer",
        "linked_at": "2024-01-15T10:30:00Z"
      }
    ]
  }
}
```

### 3.2 Update Current User

```
PATCH /v1/users/me
```

**Request Body:**
```json
{
  "email": "newemail@example.com",
  "locale": "en-GB",
  "timezone": "Europe/London"
}
```

### 3.3 Change Password

```
POST /v1/users/me/password
```

**Request Body:**
```json
{
  "current_password": "OldPass123!",
  "new_password": "NewPass456!"
}
```

### 3.4 List Sessions

```
GET /v1/users/me/sessions
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "ses_abc123",
      "device_name": "Chrome on Windows",
      "device_type": "web",
      "ip_address": "192.168.1.1",
      "location": "New York, US",
      "is_current": true,
      "created_at": "2024-01-15T10:30:00Z",
      "last_active_at": "2024-01-15T12:30:00Z"
    }
  ]
}
```

### 3.5 Revoke Session

```
DELETE /v1/users/me/sessions/{session_id}
```

### 3.6 Two-Factor Authentication

**Enable 2FA:**
```
POST /v1/users/me/2fa/enable
```

**Response:** `200 OK`
```json
{
  "data": {
    "secret": "JBSWY3DPEHPK3PXP",
    "qr_code_url": "otpauth://totp/GamingPortal:player@example.com?secret=...",
    "backup_codes": ["ABC123", "DEF456", ...]
  }
}
```

**Confirm 2FA:**
```
POST /v1/users/me/2fa/confirm
```

```json
{
  "code": "123456"
}
```

**Disable 2FA:**
```
POST /v1/users/me/2fa/disable
```

```json
{
  "code": "123456",
  "password": "YourPassword123!"
}
```

### 3.7 Linked Accounts

```
GET /v1/users/me/linked-accounts
DELETE /v1/users/me/linked-accounts/{provider}
```

---

## 4. Players

### 4.1 Search Players

```
GET /v1/players
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `q` | string | Search query (display name) |
| `game_id` | string | Filter by game |
| `country` | string | Filter by country code |
| `min_rating` | integer | Minimum rating |
| `max_rating` | integer | Maximum rating |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "ply_def456",
      "display_name": "Player One",
      "avatar_url": "https://...",
      "country_code": "US",
      "title": "Champion",
      "primary_game": {
        "game_id": "cs2",
        "rank_tier": "Global Elite",
        "rating": 2450
      }
    }
  ],
  "pagination": { ... }
}
```

### 4.2 Get Player Profile

```
GET /v1/players/{player_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "ply_def456",
    "display_name": "Player One",
    "avatar_url": "https://...",
    "banner_url": "https://...",
    "bio": "Competitive player since 2015",
    "country_code": "US",
    "timezone": "America/New_York",
    "social_links": {
      "twitter": "player1",
      "twitch": "player1_tv"
    },
    "steam_id": "76561198012345678",
    "title": "Champion",
    "featured_badge": { ... },
    "created_at": "2024-01-15T10:30:00Z",
    "is_online": true,
    "last_online_at": "2024-01-15T12:30:00Z"
  }
}
```

### 4.3 Update Own Profile

```
PATCH /v1/players/{player_id}
```

**Request Body:**
```json
{
  "display_name": "New Name",
  "bio": "Updated bio text",
  "country_code": "CA",
  "social_links": {
    "twitter": "newhandle"
  }
}
```

### 4.4 Get Player Statistics

Returns per-game statistics for a player. The platform manages core rating (Glicko-2) and match statistics, while game-specific stats are calculated by the game's plugin.

```
GET /v1/players/{player_id}/stats
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game (required) |
| `period` | string | `all_time`, `season`, `month`, `week` |

**Response:** `200 OK`
```json
{
  "data": {
    "game_id": "cs2",
    "rating": 2450,
    "rating_deviation": 45,
    "volatility": 0.06,
    "rank_tier": "Global Elite",
    "rank_position": 1523,
    "peak_rating": 2520,
    "peak_rating_at": "2024-01-10T15:30:00Z",
    "matches_played": 342,
    "wins": 198,
    "losses": 142,
    "draws": 2,
    "win_rate": 0.579,
    "current_win_streak": 5,
    "best_win_streak": 12,
    "total_playtime_hours": 523,
    "game_specific": {
      "kills": 8234,
      "deaths": 6892,
      "kd_ratio": 1.19,
      "headshot_percentage": 0.48,
      "avg_adr": 82.5,
      "clutches_won": 156,
      "mvp_count": 423
    },
    "recent_form": ["W", "W", "W", "L", "W"]
  }
}
```

**Note:** The `game_specific` object structure varies by game and is defined by the game's plugin via `player_stats_schema()`. Examples:
- **CS2**: kills, deaths, headshot_percentage, avg_adr, clutches_won
- **Age of Empires 4**: games_by_civ, avg_apm, favorite_civ, eco_score
- **Rocket League**: goals, assists, saves, shots, mvps
```

### 4.5 Get Match History

```
GET /v1/players/{player_id}/matches
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game |
| `match_type` | string | `ranked`, `tournament`, `scrim` |
| `result` | string | `win`, `loss`, `draw` |
| `from` | datetime | Start date |
| `to` | datetime | End date |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "mtc_abc123",
      "game_id": "cs2",
      "match_type": "ranked",
      "format": "bo1",
      "team_slot": 1,
      "team_name": "Team Alpha",
      "opponent_name": "Team Beta",
      "result": "win",
      "score": "16-12",
      "map": "de_mirage",
      "rating_change": +18,
      "stats": {
        "kills": 24,
        "deaths": 18,
        "assists": 5
      },
      "played_at": "2024-01-15T10:30:00Z",
      "duration_minutes": 42
    }
  ],
  "pagination": { ... }
}
```

### 4.6 Get Player Teams

```
GET /v1/players/{player_id}/teams
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "team_xyz789",
      "name": "Team Alpha",
      "tag": "ALPHA",
      "logo_url": "https://...",
      "role": "captain",
      "joined_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

### 4.7 Get Player Rankings

```
GET /v1/players/{player_id}/rankings
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "game_id": "cs2",
      "game_name": "Counter-Strike 2",
      "rating": 2450,
      "rank_tier": "Global Elite",
      "global_rank": 1523,
      "regional_rank": 234,
      "percentile": 99.2
    }
  ]
}
```

### 4.8 Friends

**List Friends:**
```
GET /v1/players/{player_id}/friends
```

**Send Friend Request:**
```
POST /v1/players/{player_id}/friends
```

```json
{
  "player_id": "ply_target123"
}
```

**Accept/Decline Request:**
```
POST /v1/players/{player_id}/friends/{request_id}/accept
POST /v1/players/{player_id}/friends/{request_id}/decline
```

**Remove Friend:**
```
DELETE /v1/players/{player_id}/friends/{friend_id}
```

---

## 5. Teams

Players can belong to multiple teams simultaneously (e.g., different teams for different games). When a player creates a team, they become the founding captain with full administrative permissions.

### 5.1 Search Teams

```
GET /v1/teams
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `q` | string | Search query |
| `game_id` | string | Filter by game |
| `recruiting` | boolean | Looking for players |
| `min_rating` | integer | Minimum team rating |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "team_xyz789",
      "name": "Team Alpha",
      "tag": "ALPHA",
      "logo_url": "https://...",
      "game_id": "cs2",
      "member_count": 5,
      "rating": 2100,
      "rank_tier": "Professional",
      "status": "active"
    }
  ],
  "pagination": { ... }
}
```

### 5.2 Create Team

Any player can create a team. The creator automatically becomes a captain (team admin) with the `is_founder` flag set to true.

```
POST /v1/teams
```

**Request Body:**
```json
{
  "name": "Team Alpha",
  "tag": "ALPHA",
  "description": "Competitive CS2 team",
  "game_id": "cs2",
  "primary_color": "#FF5500",
  "secondary_color": "#FFFFFF",
  "logo_url": "https://...",
  "website_url": "https://teamalpha.gg",
  "social_links": {
    "twitter": "teamalpha",
    "discord": "https://discord.gg/alpha"
  }
}
```

**Response:** `201 Created`
```json
{
  "data": {
    "id": "team_xyz789",
    "name": "Team Alpha",
    "tag": "ALPHA",
    "created_by": "ply_def456",
    "your_role": "captain",
    "is_founder": true,
    "created_at": "2024-01-15T10:30:00Z"
  }
}
```

### 5.3 Get Team

```
GET /v1/teams/{team_id}
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `include` | string | `members`, `stats`, `recent_matches` |

**Response:** `200 OK`
```json
{
  "data": {
    "id": "team_xyz789",
    "name": "Team Alpha",
    "tag": "ALPHA",
    "description": "Competitive CS2 team",
    "logo_url": "https://...",
    "banner_url": "https://...",
    "primary_color": "#FF5500",
    "game_id": "cs2",
    "captains": [
      {
        "id": "ply_def456",
        "display_name": "Player One",
        "is_founder": true
      }
    ],
    "member_count": 5,
    "rating": 2100,
    "status": "active",
    "stats": {
      "matches_played": 87,
      "wins": 52,
      "losses": 35,
      "win_rate": 0.598
    },
    "social_links": { ... },
    "created_at": "2024-01-15T10:30:00Z"
  }
}
```

### 5.4 Update Team

```
PATCH /v1/teams/{team_id}
```

**Required Permission:** Team captain (`role = 'captain'`)

**Request Body:**
```json
{
  "description": "Updated description",
  "primary_color": "#00FF55"
}
```

### 5.5 Delete Team

```
DELETE /v1/teams/{team_id}
```

**Note:** This triggers the `disband_team` saga which handles cascading updates.

**Response:** `202 Accepted`
```json
{
  "data": {
    "saga_id": "saga_abc123",
    "status": "processing",
    "status_url": "/v1/sagas/saga_abc123"
  }
}
```

### 5.6 Team Members

**List Members:**
```
GET /v1/teams/{team_id}/members
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "mem_abc123",
      "player": {
        "id": "ply_def456",
        "display_name": "Player One",
        "avatar_url": "https://..."
      },
      "role": "captain",
      "role_title": "In-Game Leader",
      "primary_position": "entry",
      "jersey_number": 1,
      "is_founder": true,
      "joined_at": "2024-01-15T10:30:00Z",
      "stats": {
        "matches_with_team": 45,
        "win_rate": 0.62
      }
    }
  ]
}
```

**Invite Player:**
```
POST /v1/teams/{team_id}/members/invite
```

**Required Permission:** Team captain or officer

```json
{
  "player_id": "ply_target123",
  "role": "player",
  "message": "We'd like you to join our team!"
}
```

**Remove Member:**
```
DELETE /v1/teams/{team_id}/members/{player_id}
```

**Required Permission:** Team captain. Cannot remove other captains unless you are the founder.

**Promote/Demote Member:**
```
PATCH /v1/teams/{team_id}/members/{player_id}
```

**Required Permission:** Team captain

**Role Hierarchy:**
- `captain`: Full admin rights. Can invite, remove, promote others to captain.
- `officer`: Can invite players and manage roster.
- `player`: Regular team member.
- `substitute`: Backup player.
- `coach`/`manager`: Non-playing staff.

```json
{
  "role": "captain",
  "primary_position": "awp",
  "role_title": "AWPer"
}
```

**Note:** Founders (`is_founder = true`) cannot be demoted or removed. There must always be at least one captain.

### 5.7 Team Invitations

**List Invitations:**
```
GET /v1/teams/{team_id}/invitations
```

**Accept Invitation:**
```
POST /v1/teams/{team_id}/invitations/{invitation_id}/accept
```

**Decline Invitation:**
```
POST /v1/teams/{team_id}/invitations/{invitation_id}/decline
```

### 5.8 My Teams

Get all teams the current player belongs to:
```
GET /v1/players/me/teams
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "team_id": "team_xyz789",
      "name": "Team Alpha",
      "tag": "ALPHA",
      "role": "captain",
      "is_founder": true,
      "game_id": "cs2"
    },
    {
      "team_id": "team_abc123",
      "name": "Casual Squad",
      "tag": "CAS",
      "role": "player",
      "is_founder": false,
      "game_id": "valorant"
    }
  ]
}
```

### 5.9 Team Statistics

```
GET /v1/teams/{team_id}/stats
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `period` | string | `all_time`, `season`, `month` |

### 5.10 Team Match History

```
GET /v1/teams/{team_id}/matches
```

---

## 6. Games

### 6.1 List Games

```
GET /v1/games
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `status` | string | `active`, `maintenance`, `coming_soon` |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "cs2",
      "display_name": "Counter-Strike 2",
      "short_name": "CS2",
      "icon_url": "https://...",
      "banner_url": "https://...",
      "status": "active",
      "team_size": 5,
      "is_featured": true,
      "player_count": 45230,
      "active_matches": 234
    }
  ]
}
```

### 6.2 Get Game Details

```
GET /v1/games/{game_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "cs2",
    "display_name": "Counter-Strike 2",
    "description": "Tactical first-person shooter",
    "icon_url": "https://...",
    "logo_url": "https://...",
    "banner_url": "https://...",
    "status": "active",
    "team_size": 5,
    "min_team_size": 1,
    "supports_solo_queue": true,
    "supports_team_queue": true,
    "supports_custom_map_pool": true,
    "rank_tiers": [
      {"id": "silver_1", "name": "Silver I", "min_rating": 0, "icon_url": "..."},
      {"id": "silver_2", "name": "Silver II", "min_rating": 100, "icon_url": "..."},
      {"id": "global_elite", "name": "Global Elite", "min_rating": 2800, "icon_url": "..."}
    ],
    "available_maps": [
      {"id": "de_dust2", "name": "Dust II", "is_competitive": true},
      {"id": "de_mirage", "name": "Mirage", "is_competitive": true},
      {"id": "de_inferno", "name": "Inferno", "is_competitive": true},
      {"id": "de_nuke", "name": "Nuke", "is_competitive": true},
      {"id": "de_ancient", "name": "Ancient", "is_competitive": true},
      {"id": "de_anubis", "name": "Anubis", "is_competitive": true},
      {"id": "de_vertigo", "name": "Vertigo", "is_competitive": true}
    ],
    "default_map_pool": ["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_ancient", "de_anubis", "de_vertigo"],
    "stats_schema": {
      "kills": {"type": "integer", "label": "Kills"},
      "deaths": {"type": "integer", "label": "Deaths"},
      "headshot_percentage": {"type": "float", "label": "HS%"},
      "avg_adr": {"type": "float", "label": "ADR"}
    },
    "statistics": {
      "total_players": 125000,
      "active_players_24h": 45230,
      "matches_today": 8923
    }
  }
}
```

**Notes:**
- `available_maps`: All maps the game plugin provides
- `default_map_pool`: Default competitive map pool
- `supports_custom_map_pool`: Whether leagues/tournaments can customize the map pool
- `stats_schema`: Schema for game-specific player statistics (defined by plugin)
- `rank_tiers`: Rank display tiers (defined by plugin)
```

### 6.3 Game Queues

```
GET /v1/games/{game_id}/queues
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "queue_ranked_5v5",
      "name": "Ranked 5v5",
      "description": "Competitive ranked matchmaking",
      "queue_type": "ranked",
      "team_size": 5,
      "status": "active",
      "estimated_wait_seconds": 45,
      "players_in_queue": 234
    }
  ]
}
```

### 6.4 Game Leaderboard

```
GET /v1/games/{game_id}/leaderboard
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `type` | string | `rating`, `wins`, `playtime` |
| `region` | string | Filter by region |
| `season_id` | uuid | Filter by season |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "rank": 1,
      "player": {
        "id": "ply_top1",
        "display_name": "TopPlayer",
        "avatar_url": "https://...",
        "country_code": "SE"
      },
      "rating": 3200,
      "wins": 523,
      "losses": 124,
      "win_rate": 0.808
    }
  ],
  "pagination": { ... }
}
```

---

## 7. Matchmaking

### 7.1 Join Queue

```
POST /v1/matchmaking/queues/{queue_id}/join
```

**Request Body:**
```json
{
  "party_members": ["ply_friend1", "ply_friend2"],
  "preferences": {
    "map_pool": ["de_dust2", "de_mirage"],
    "max_ping": 50,
    "preferred_region": "eu-west"
  }
}
```

**Response:** `201 Created`
```json
{
  "data": {
    "ticket_id": "tkt_abc123",
    "queue_id": "queue_ranked_5v5",
    "status": "searching",
    "party_size": 3,
    "estimated_wait_seconds": 60,
    "joined_at": "2024-01-15T10:30:00Z"
  }
}
```

### 7.2 Leave Queue

```
DELETE /v1/matchmaking/queues/{queue_id}/leave
```

### 7.3 Get Ticket Status

```
GET /v1/matchmaking/tickets/{ticket_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "ticket_id": "tkt_abc123",
    "status": "match_found",
    "queue_id": "queue_ranked_5v5",
    "wait_time_seconds": 45,
    "match_info": {
      "match_id": "mtc_pending123",
      "accept_deadline": "2024-01-15T10:31:00Z",
      "players_accepted": 7,
      "players_total": 10
    }
  }
}
```

### 7.4 Accept Match

```
POST /v1/matchmaking/tickets/{ticket_id}/accept
```

### 7.5 Decline Match

```
POST /v1/matchmaking/tickets/{ticket_id}/decline
```

### 7.6 Get Queue Estimate

```
GET /v1/matchmaking/queues/{queue_id}/estimate
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `rating` | integer | Player rating |
| `party_size` | integer | Party size |

**Response:** `200 OK`
```json
{
  "data": {
    "estimated_wait_seconds": 45,
    "players_in_queue": 234,
    "confidence": 0.85
  }
}
```

---

## 8. Matches

### 8.1 List Matches

```
GET /v1/matches
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game |
| `match_type` | string | `ranked`, `tournament`, `scrim` |
| `status` | string | `live`, `completed`, `scheduled` |
| `team_id` | uuid | Filter by team |
| `player_id` | uuid | Filter by player |
| `from` | datetime | Start date |
| `to` | datetime | End date |

### 8.2 Get Match Details

```
GET /v1/matches/{match_id}
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `include` | string | `players`, `maps`, `stats`, `timeline` |

**Response:** `200 OK`
```json
{
  "data": {
    "id": "mtc_abc123",
    "game_id": "cs2",
    "match_type": "ranked",
    "format": "bo3",
    "status": "completed",
    "teams": [
      {
        "slot": 1,
        "team_id": "team_xyz789",
        "name": "Team Alpha",
        "score": 2
      },
      {
        "slot": 2,
        "team_id": "team_abc456",
        "name": "Team Beta",
        "score": 1
      }
    ],
    "winner_slot": 1,
    "maps": [
      {
        "map_number": 1,
        "map_name": "de_mirage",
        "picked_by": 1,
        "team_1_score": 16,
        "team_2_score": 12,
        "winner_slot": 1
      },
      {
        "map_number": 2,
        "map_name": "de_inferno",
        "picked_by": 2,
        "team_1_score": 14,
        "team_2_score": 16,
        "winner_slot": 2
      },
      {
        "map_number": 3,
        "map_name": "de_nuke",
        "team_1_score": 16,
        "team_2_score": 9,
        "winner_slot": 1
      }
    ],
    "server": {
      "address": "192.168.1.100:27015",
      "gotv_address": "192.168.1.100:27020"
    },
    "started_at": "2024-01-15T10:30:00Z",
    "ended_at": "2024-01-15T12:15:00Z",
    "duration_minutes": 105,
    "vod_url": "https://..."
  }
}
```

### 8.3 Get Match Players

```
GET /v1/matches/{match_id}/players
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "player": {
        "id": "ply_def456",
        "display_name": "Player One"
      },
      "team_slot": 1,
      "role": "entry",
      "is_captain": true,
      "rating_before": 2432,
      "rating_after": 2450,
      "rating_change": 18,
      "stats": {
        "kills": 67,
        "deaths": 52,
        "assists": 14,
        "kd_ratio": 1.29,
        "adr": 84.2,
        "rating": 1.24
      }
    }
  ]
}
```

### 8.4 Get Match Statistics

```
GET /v1/matches/{match_id}/stats
```

**Response includes game-specific detailed statistics.**

---

## 9. Lobbies

### 9.1 Create Lobby

```
POST /v1/lobbies
```

**Request Body:**
```json
{
  "game_id": "cs2",
  "name": "Scrim vs Team Beta",
  "config": {
    "team_size": 5,
    "format": "bo3",
    "map_pool": ["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_vertigo"],
    "pick_ban_format": "standard",
    "overtime_enabled": true
  },
  "is_public": false,
  "password": "secretpass123"
}
```

**Response:** `201 Created`
```json
{
  "data": {
    "id": "lby_abc123",
    "invite_code": "ABC123",
    "join_url": "https://gaming-portal.com/lobby/ABC123",
    "created_at": "2024-01-15T10:30:00Z"
  }
}
```

### 9.2 Get Lobby

```
GET /v1/lobbies/{lobby_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "lby_abc123",
    "game_id": "cs2",
    "status": "picking",
    "phase": "map_ban",
    "config": { ... },
    "teams": [
      {
        "slot": 1,
        "name": "Team Alpha",
        "players": [
          {
            "id": "ply_def456",
            "display_name": "Player One",
            "is_captain": true,
            "ready_status": "ready"
          }
        ]
      },
      {
        "slot": 2,
        "name": "Team Beta",
        "players": [ ... ]
      }
    ],
    "map_pool": ["de_dust2", "de_mirage", ...],
    "banned_maps": ["de_vertigo", "de_ancient"],
    "picked_maps": ["de_mirage"],
    "current_action": {
      "type": "ban",
      "team_slot": 1,
      "timeout_at": "2024-01-15T10:31:30Z"
    },
    "created_at": "2024-01-15T10:30:00Z"
  }
}
```

### 9.3 Join Lobby

```
POST /v1/lobbies/{lobby_id}/join
```

**Request Body:**
```json
{
  "password": "secretpass123",
  "team_slot": 1
}
```

**Or via invite code:**
```
POST /v1/lobbies/join
```

```json
{
  "invite_code": "ABC123"
}
```

### 9.4 Leave Lobby

```
POST /v1/lobbies/{lobby_id}/leave
```

### 9.5 WebSocket Connection

```
GET /v1/lobbies/{lobby_id}/ws?token={jwt_token}
```

Upgrades to WebSocket connection for real-time lobby updates.

---

## 10. Tournaments

Tournaments can be either **global** (open to all players) or **league-specific** (restricted to league members). Multiple tournaments can run concurrently.

### 10.1 List Tournaments

```
GET /v1/tournaments
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game |
| `league_id` | string | Filter by league (null for global tournaments) |
| `status` | string | `registration`, `active`, `completed` |
| `format` | string | `single_elimination`, `double_elimination`, etc. |
| `participant_type` | string | `player`, `team` |
| `global_only` | boolean | Only show global (non-league) tournaments |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "trn_abc123",
      "name": "Weekly Cup #15",
      "game_id": "cs2",
      "league_id": null,
      "is_global": true,
      "status": "registration",
      "format": "single_elimination",
      "participant_count": 24,
      "max_participants": 32,
      "starts_at": "2024-01-27T19:00:00Z"
    },
    {
      "id": "trn_def456",
      "name": "Division 1 Championship",
      "game_id": "cs2",
      "league_id": "league_xyz",
      "league_name": "Pro League Division 1",
      "is_global": false,
      "status": "registration",
      "format": "double_elimination",
      "participant_count": 8,
      "max_participants": 16,
      "starts_at": "2024-02-01T19:00:00Z"
    }
  ]
}
```

### 10.2 Create Global Tournament

**Required Permission:** Platform admin

```
POST /v1/tournaments
```

**Request Body:**
```json
{
  "name": "Weekly Cup #15",
  "game_id": "cs2",
  "format": "single_elimination",
  "participant_type": "team",
  "team_size": 5,
  "max_participants": 32,
  "default_match_format": "bo3",
  "map_pool": ["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_ancient"],
  "registration_opens_at": "2024-01-20T00:00:00Z",
  "registration_closes_at": "2024-01-27T18:00:00Z",
  "starts_at": "2024-01-27T19:00:00Z",
  "seeding_method": "rating",
  "rules_text": "Standard competitive rules apply...",
  "prize_pool": {
    "currency": "USD",
    "distribution": {
      "1st": 500,
      "2nd": 250,
      "3rd-4th": 100
    }
  }
}
```

**Note:** For league-specific tournaments, use `POST /v1/leagues/{league_id}/tournaments` instead (see Section 11.4).

### 10.3 Get Tournament

```
GET /v1/tournaments/{tournament_id}
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `include` | string | `participants`, `bracket`, `matches` |

**Response:** `200 OK`
```json
{
  "data": {
    "id": "trn_abc123",
    "name": "Weekly Cup #15",
    "game_id": "cs2",
    "league_id": null,
    "is_global": true,
    "created_by": {
      "id": "usr_admin",
      "display_name": "Platform Admin"
    },
    "format": "single_elimination",
    "participant_type": "team",
    "map_pool": ["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_ancient"],
    "status": "registration",
    "participant_count": 24,
    "max_participants": 32,
    "registration_opens_at": "2024-01-20T00:00:00Z",
    "registration_closes_at": "2024-01-27T18:00:00Z",
    "starts_at": "2024-01-27T19:00:00Z"
  }
}
```

### 10.4 Update Tournament

```
PATCH /v1/tournaments/{tournament_id}
```

**Required Permission:** Tournament creator or platform admin (for global), league admin (for league tournaments)

### 10.5 Tournament Participants

**List Participants:**
```
GET /v1/tournaments/{tournament_id}/participants
```

**Register:**
```
POST /v1/tournaments/{tournament_id}/participants
```

**Note:** For league tournaments, the registering player/team must be a member of the league.

```json
{
  "team_id": "team_xyz789"
}
```

**Withdraw:**
```
DELETE /v1/tournaments/{tournament_id}/participants/{participant_id}
```

**Check In:**
```
POST /v1/tournaments/{tournament_id}/participants/{participant_id}/check-in
```

### 10.6 Tournament Bracket

**Get Bracket:**
```
GET /v1/tournaments/{tournament_id}/bracket
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "brk_abc123",
    "bracket_type": "winners",
    "total_rounds": 5,
    "matches": [
      {
        "id": "bm_round1_1",
        "round": 1,
        "position": 1,
        "match_number": 1,
        "participant_1": {
          "id": "team_xyz789",
          "name": "Team Alpha",
          "seed": 1
        },
        "participant_2": {
          "id": "team_abc456",
          "name": "Team Beta",
          "seed": 16
        },
        "status": "completed",
        "winner_id": "team_xyz789",
        "score": "2-0",
        "scheduled_at": "2024-01-27T19:00:00Z"
      }
    ]
  }
}
```

**Generate Bracket:**
```
POST /v1/tournaments/{tournament_id}/bracket/generate
```

### 10.7 Tournament Matches

**List Matches:**
```
GET /v1/tournaments/{tournament_id}/matches
```

**Report Result:**
```
PATCH /v1/tournaments/{tournament_id}/matches/{bracket_match_id}
```

```json
{
  "winner_id": "team_xyz789",
  "participant_1_score": 2,
  "participant_2_score": 1,
  "match_id": "mtc_abc123"
}
```

---

## 11. Leagues & Seasons

Leagues are game-specific competitive organizations with divisions. Players must be league members to participate in seasons or league-specific tournaments.

### 11.1 Leagues

**List Leagues:**
```
GET /v1/leagues
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game |
| `access_type` | string | `open`, `invite_only`, `application` |
| `division` | integer | Filter by division level |
| `region` | string | Filter by region |

**Response:** `200 OK`
```json
{
  "data": [
    {
      "id": "league_abc123",
      "name": "Pro League Division 1",
      "slug": "pro-league-div1",
      "game_id": "cs2",
      "division": 1,
      "tier_name": "Premier",
      "region": "eu",
      "access_type": "invite_only",
      "member_count": 120,
      "status": "active"
    }
  ]
}
```

**Create League:**
```
POST /v1/leagues
```

**Required Permission:** `admin:leagues:create` or platform admin

```json
{
  "name": "Pro League Division 1",
  "slug": "pro-league-div1",
  "game_id": "cs2",
  "description": "Premier competitive league",
  "division": 1,
  "tier_name": "Premier",
  "region": "eu",
  "access_type": "invite_only",
  "min_rating": 2000,
  "default_map_pool": ["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_ancient"]
}
```

**Get League:**
```
GET /v1/leagues/{league_id}
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `include` | string | `members`, `seasons`, `tournaments` |

**Update League:**
```
PATCH /v1/leagues/{league_id}
```

**Required Permission:** `league:manage` (league admin)

### 11.2 League Membership

Players must be members of a league to participate in its seasons and tournaments.

**List League Members:**
```
GET /v1/leagues/{league_id}/members
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `status` | string | `active`, `suspended`, `pending` |
| `membership_type` | string | `player`, `admin`, `moderator` |

**Join League (for open leagues):**
```
POST /v1/leagues/{league_id}/join
```

**Response:** `201 Created`
```json
{
  "data": {
    "membership_id": "mem_abc123",
    "league_id": "league_abc123",
    "status": "active",
    "joined_at": "2024-01-15T10:30:00Z"
  }
}
```

**Apply to League (for application-based leagues):**
```
POST /v1/leagues/{league_id}/apply
```

```json
{
  "message": "I have 5 years of competitive experience..."
}
```

**Response:** `201 Created`
```json
{
  "data": {
    "application_id": "inv_abc123",
    "status": "pending"
  }
}
```

**Leave League:**
```
DELETE /v1/leagues/{league_id}/members/me
```

**Get My Membership:**
```
GET /v1/leagues/{league_id}/members/me
```

### 11.3 League Invitations (Admin)

**List Pending Applications:**
```
GET /v1/leagues/{league_id}/applications
```

**Required Permission:** `league:members:manage` (league admin)

**Invite Player:**
```
POST /v1/leagues/{league_id}/invitations
```

```json
{
  "player_id": "ply_abc123"
}
```

**Approve Application:**
```
POST /v1/leagues/{league_id}/applications/{application_id}/approve
```

**Reject Application:**
```
POST /v1/leagues/{league_id}/applications/{application_id}/reject
```

```json
{
  "reason": "Rating below minimum requirement"
}
```

**Remove Member:**
```
DELETE /v1/leagues/{league_id}/members/{player_id}
```

**Promote to Admin:**
```
POST /v1/leagues/{league_id}/members/{player_id}/promote
```

```json
{
  "membership_type": "admin"
}
```

### 11.4 League Tournaments

League admins can create tournaments restricted to league members.

**List League Tournaments:**
```
GET /v1/leagues/{league_id}/tournaments
```

**Create League Tournament:**
```
POST /v1/leagues/{league_id}/tournaments
```

**Required Permission:** `league:tournaments:create` (league admin)

```json
{
  "name": "Weekly Cup #15",
  "format": "single_elimination",
  "starts_at": "2024-01-27T19:00:00Z",
  "max_participants": 16,
  "map_pool": ["de_dust2", "de_mirage", "de_inferno"]
}
```

**Note:** Participants must be league members. The `map_pool` can be customized if the game plugin's `supports_custom_map_pool()` returns true. If not specified, uses league's `default_map_pool`.

### 11.5 Seasons

**List Seasons:**
```
GET /v1/leagues/{league_id}/seasons
```

**Create Season:**
```
POST /v1/leagues/{league_id}/seasons
```

**Required Permission:** `league:seasons:manage` (league admin)

```json
{
  "name": "Season 5",
  "format": "round_robin",
  "starts_at": "2024-02-01T00:00:00Z",
  "ends_at": "2024-04-30T23:59:59Z",
  "registration_opens_at": "2024-01-15T00:00:00Z",
  "registration_closes_at": "2024-01-31T23:59:59Z",
  "max_participants": 16
}
```

**Get Season:**
```
GET /v1/leagues/{league_id}/seasons/{season_id}
```

### 11.6 Season Standings

```
GET /v1/leagues/{league_id}/seasons/{season_id}/standings
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "rank": 1,
      "participant": {
        "id": "team_xyz789",
        "name": "Team Alpha",
        "logo_url": "https://..."
      },
      "points": 27,
      "matches_played": 10,
      "wins": 9,
      "losses": 1,
      "draws": 0,
      "maps_won": 19,
      "maps_lost": 5,
      "map_differential": 14,
      "current_streak": 5,
      "form": ["W", "W", "W", "W", "W"]
    }
  ]
}
```

### 11.7 Season Schedule

```
GET /v1/leagues/{league_id}/seasons/{season_id}/schedule
```

---

## 12. Substitutes

### 12.1 Substitute Pool

**Register as Substitute:**
```
POST /v1/substitute/pool
```

```json
{
  "season_id": "ssn_abc123",
  "preferences": {
    "preferred_roles": ["support", "entry"],
    "preferred_positions": ["rifle", "awp"],
    "min_notice_hours": 24,
    "max_games_per_week": 3,
    "preferred_days": ["friday", "saturday", "sunday"]
  },
  "notes": "Available most evenings EST"
}
```

**Get My Registrations:**
```
GET /v1/substitute/pool
```

**Update Registration:**
```
PATCH /v1/substitute/pool/{pool_id}
```

**Withdraw:**
```
DELETE /v1/substitute/pool/{pool_id}
```

### 12.2 Availability

**Post Availability:**
```
POST /v1/substitute/availability
```

```json
{
  "season_id": "ssn_abc123",
  "windows": [
    {
      "date": "2024-01-27",
      "start_time": "18:00",
      "end_time": "23:00",
      "timezone": "America/New_York"
    }
  ]
}
```

**Set Recurring Availability:**
```
POST /v1/substitute/availability/recurring
```

```json
{
  "season_id": "ssn_abc123",
  "schedule": {
    "days_of_week": ["friday", "saturday"],
    "start_time": "18:00",
    "end_time": "23:00",
    "timezone": "America/New_York",
    "start_date": "2024-01-01",
    "end_date": "2024-04-30"
  }
}
```

**Get My Availability:**
```
GET /v1/substitute/availability
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `from` | date | Start date |
| `to` | date | End date |

**Update Availability:**
```
PATCH /v1/substitute/availability/{id}
```

**Cancel Availability:**
```
DELETE /v1/substitute/availability/{id}
```

### 12.3 Substitute Requests

**List Open Requests:**
```
GET /v1/substitute/requests
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `season_id` | uuid | Filter by season |
| `urgency` | string | `low`, `normal`, `high`, `emergency` |
| `status` | string | `open`, `filled` |

**Create Request:**
```
POST /v1/substitute/requests
```

```json
{
  "team_id": "team_xyz789",
  "match_id": "mtc_abc123",
  "season_id": "ssn_abc123",
  "reason": "Player unavailable due to travel",
  "urgency": "normal",
  "match_scheduled_at": "2024-01-27T19:00:00Z",
  "response_deadline": "2024-01-27T17:00:00Z",
  "requirements": {
    "required_roles": ["support"],
    "min_skill_rating": 2000,
    "max_skill_rating": 2500
  },
  "replacing_player_id": "ply_absent123"
}
```

**Get Request Details:**
```
GET /v1/substitute/requests/{id}
```

**Get Matching Substitutes:**
```
GET /v1/substitute/requests/{id}/matches
```

**Response:** `200 OK`
```json
{
  "data": [
    {
      "player_id": "ply_sub123",
      "display_name": "SubPlayer",
      "skill_rating": 2200,
      "rank_tier": "Supreme",
      "preferred_roles": ["support", "lurk"],
      "reliability_score": 0.95,
      "games_as_sub": 12,
      "match_score": 87.5
    }
  ]
}
```

**Respond to Request:**
```
POST /v1/substitute/requests/{id}/respond
```

```json
{
  "response_type": "interested",
  "message": "I'm available and can play support"
}
```

**Accept Substitute (Team):**
```
POST /v1/substitute/requests/{id}/accept/{player_id}
```

**Cancel Request:**
```
DELETE /v1/substitute/requests/{id}
```

### 12.4 Assignments

**Get Assignment History:**
```
GET /v1/substitute/assignments
```

**Submit Feedback:**
```
POST /v1/substitute/assignments/{id}/feedback
```

```json
{
  "rating": 5,
  "feedback": "Great communication and gameplay"
}
```

---

## 13. Game Servers

### 13.1 List Servers

```
GET /v1/servers
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | string | Filter by game |
| `region` | string | Filter by region |
| `status` | string | `available`, `in_match` |

**Required Permission:** `admin:servers:view`

### 13.2 Get Server Status

```
GET /v1/servers/{server_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "srv_abc123",
    "name": "EU West Server #1",
    "address": "192.168.1.100:27015",
    "region": "eu-west",
    "game_id": "cs2",
    "status": "available",
    "health_status": "healthy",
    "current_match_id": null,
    "specs": {
      "tickrate": 128,
      "max_players": 12
    },
    "last_health_check_at": "2024-01-15T10:30:00Z"
  }
}
```

### 13.3 Server Commands (Admin)

**Send RCON Command:**
```
POST /v1/servers/{server_id}/rcon
```

```json
{
  "command": "status"
}
```

**Restart Server:**
```
POST /v1/servers/{server_id}/restart
```

### 13.4 Server Reservations

**Get Reservation:**
```
GET /v1/servers/reservations/{match_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "res_abc123",
    "server_id": "srv_abc123",
    "match_id": "mtc_abc123",
    "status": "ready",
    "connect_info": {
      "address": "192.168.1.100:27015",
      "password": "matchpass123",
      "gotv_address": "192.168.1.100:27020"
    },
    "reserved_at": "2024-01-15T10:30:00Z"
  }
}
```

---

## 14. Admin

### 14.1 Users Management

**List Users:**
```
GET /v1/admin/users
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `q` | string | Search query |
| `status` | string | `active`, `suspended`, `banned` |
| `sort` | string | `created_at`, `last_login_at` |

**Get User:**
```
GET /v1/admin/users/{user_id}
```

**Update User:**
```
PATCH /v1/admin/users/{user_id}
```

### 14.2 Bans

**List Bans:**
```
GET /v1/admin/bans
```

**Create Ban:**
```
POST /v1/admin/users/{user_id}/ban
```

```json
{
  "ban_type": "platform",
  "reason": "Cheating detected",
  "duration_days": 30,
  "evidence_urls": ["https://evidence.example.com/123"]
}
```

**Lift Ban:**
```
DELETE /v1/admin/users/{user_id}/ban/{ban_id}
```

```json
{
  "reason": "Ban appeal approved"
}
```

### 14.3 Games Management

**List Games:**
```
GET /v1/admin/games
```

**Update Game:**
```
PATCH /v1/admin/games/{game_id}
```

```json
{
  "status": "maintenance",
  "maintenance_message": "Scheduled maintenance until 2:00 PM UTC"
}
```

### 14.4 Platform Statistics

```
GET /v1/admin/stats
```

**Response:** `200 OK`
```json
{
  "data": {
    "users": {
      "total": 125000,
      "active_24h": 45230,
      "new_today": 523
    },
    "matches": {
      "active": 234,
      "today": 8923,
      "this_week": 45234
    },
    "queues": {
      "players_searching": 1234
    },
    "servers": {
      "total": 50,
      "available": 32,
      "in_use": 18
    }
  }
}
```

### 14.5 Audit Logs

```
GET /v1/admin/audit-logs
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `user_id` | uuid | Filter by user |
| `action` | string | Filter by action |
| `resource_type` | string | Filter by resource |
| `from` | datetime | Start date |
| `to` | datetime | End date |

### 14.6 Sagas

**Get Saga Status:**
```
GET /v1/sagas/{saga_id}
```

**Response:** `200 OK`
```json
{
  "data": {
    "id": "saga_abc123",
    "saga_type": "disband_team",
    "status": "completed",
    "current_step": 8,
    "total_steps": 8,
    "steps": [
      {
        "name": "validate_team_can_disband",
        "status": "completed",
        "completed_at": "2024-01-15T10:30:01Z"
      },
      {
        "name": "remove_from_tournaments",
        "status": "completed",
        "completed_at": "2024-01-15T10:30:02Z"
      }
    ],
    "started_at": "2024-01-15T10:30:00Z",
    "completed_at": "2024-01-15T10:30:10Z"
  }
}
```

---

## 15. WebSocket Events

### 15.1 Connection

```
GET /v1/ws?token={jwt_token}
```

**Or for lobby-specific:**
```
GET /v1/lobbies/{lobby_id}/ws?token={jwt_token}
```

### 15.2 Client → Server Messages

**Ready Status:**
```json
{
  "type": "ready",
  "ready": true
}
```

**Team Change:**
```json
{
  "type": "join_team",
  "team_slot": 1
}
```

**Map Pick/Ban:**
```json
{
  "type": "map_action",
  "action": "ban",
  "map": "de_vertigo"
}
```

**Side Selection:**
```json
{
  "type": "side_select",
  "map": "de_mirage",
  "side": "ct"
}
```

**Chat:**
```json
{
  "type": "chat",
  "message": "glhf!"
}
```

**Heartbeat:**
```json
{
  "type": "ping"
}
```

### 15.3 Server → Client Messages

**Player Joined:**
```json
{
  "type": "player_joined",
  "player": {
    "id": "ply_def456",
    "display_name": "Player One"
  },
  "team_slot": 1
}
```

**Player Left:**
```json
{
  "type": "player_left",
  "player_id": "ply_def456",
  "reason": "disconnected"
}
```

**Ready State Changed:**
```json
{
  "type": "ready_changed",
  "player_id": "ply_def456",
  "ready": true
}
```

**Phase Changed:**
```json
{
  "type": "phase_changed",
  "from": "gathering",
  "to": "picking",
  "current_action": {
    "type": "ban",
    "team_slot": 1,
    "timeout_at": "2024-01-15T10:31:30Z"
  }
}
```

**Map Banned:**
```json
{
  "type": "map_banned",
  "map": "de_vertigo",
  "banned_by": 1
}
```

**Map Picked:**
```json
{
  "type": "map_picked",
  "map": "de_mirage",
  "picked_by": 2,
  "map_number": 1
}
```

**Side Selected:**
```json
{
  "type": "side_selected",
  "map": "de_mirage",
  "team_slot": 1,
  "side": "ct"
}
```

**Match Starting:**
```json
{
  "type": "match_starting",
  "match_id": "mtc_abc123",
  "server": {
    "address": "192.168.1.100:27015",
    "password": "matchpass123"
  }
}
```

**Chat Message:**
```json
{
  "type": "chat",
  "sender": {
    "id": "ply_def456",
    "display_name": "Player One"
  },
  "message": "glhf!",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

**State Sync (on reconnect):**
```json
{
  "type": "state_sync",
  "lobby": { ... },
  "your_slot": 1,
  "your_ready_status": "ready"
}
```

**Error:**
```json
{
  "type": "error",
  "code": "not_your_turn",
  "message": "It's not your turn to pick/ban"
}
```

---

## 16. Webhooks

### 16.1 Game Server Webhooks

**Get5 Events:**
```
POST /v1/webhooks/servers/get5
```

**Payload Examples:**

*Series Start:*
```json
{
  "event": "series_start",
  "matchid": "mtc_abc123",
  "team1_name": "Team Alpha",
  "team2_name": "Team Beta"
}
```

*Map End:*
```json
{
  "event": "map_result",
  "matchid": "mtc_abc123",
  "map_number": 1,
  "map_name": "de_mirage",
  "team1_score": 16,
  "team2_score": 12,
  "winner": "team1"
}
```

*Series End:*
```json
{
  "event": "series_end",
  "matchid": "mtc_abc123",
  "winner": "team1",
  "team1_series_score": 2,
  "team2_series_score": 1,
  "demo_upload_url": "https://..."
}
```

*Player Stats:*
```json
{
  "event": "player_stats",
  "matchid": "mtc_abc123",
  "map_number": 1,
  "steamid": "76561198012345678",
  "kills": 24,
  "deaths": 18,
  "assists": 5,
  "damage": 2534
}
```

### 16.2 Webhook Signatures

All webhooks include signature verification headers:

```
X-Webhook-Signature: sha256=abc123...
X-Webhook-Timestamp: 1642248000
X-Webhook-ID: whk_abc123
```

Verify signature:
```
HMAC-SHA256(timestamp + "." + body, webhook_secret)
```

---

## Appendix A: HTTP Status Codes

| Code | Meaning | Usage |
|------|---------|-------|
| 200 | OK | Successful GET, PATCH |
| 201 | Created | Successful POST |
| 202 | Accepted | Async operation started |
| 204 | No Content | Successful DELETE |
| 400 | Bad Request | Invalid input |
| 401 | Unauthorized | Missing/invalid auth |
| 403 | Forbidden | Insufficient permissions |
| 404 | Not Found | Resource doesn't exist |
| 409 | Conflict | Resource conflict |
| 422 | Unprocessable Entity | Validation failed |
| 429 | Too Many Requests | Rate limited |
| 500 | Internal Server Error | Server error |
| 503 | Service Unavailable | Maintenance mode |

---

## Appendix B: Error Codes

| Code | Description |
|------|-------------|
| `auth/invalid_credentials` | Invalid login credentials |
| `auth/token_expired` | Access token expired |
| `auth/token_revoked` | Token was revoked |
| `auth/2fa_required` | Two-factor authentication required |
| `validation/field_required` | Required field missing |
| `validation/field_invalid` | Field value invalid |
| `resource/not_found` | Resource not found |
| `resource/already_exists` | Resource already exists |
| `permission/denied` | Permission denied |
| `rate_limit/exceeded` | Rate limit exceeded |
| `team/already_member` | Already a team member |
| `team/roster_full` | Team roster is full |
| `queue/already_queued` | Already in queue |
| `match/already_started` | Match already started |
| `lobby/not_your_turn` | Not your turn in pick/ban |

---

*API Reference document prepared for engineering review.*
*Version: 1.0 | Base URL: https://api.gaming-portal.com/v1*
