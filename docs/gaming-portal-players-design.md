# Players & Profiles Vertical Slice Design
## Multi-Game Competitive Gaming Portal

**Version:** 1.0
**Status:** Draft for Engineering Review
**Last Updated:** November 2024

---

## Table of Contents

1. [Overview](#1-overview)
2. [Domain Model](#2-domain-model)
3. [Core Layer (`portal-core`)](#3-core-layer-portal-core)
4. [Database Layer (`portal-db`)](#4-database-layer-portal-db)
5. [Domain Layer (`portal-domain`)](#5-domain-layer-portal-domain)
6. [API Layer (`portal-api`)](#6-api-layer-portal-api)
7. [Integration Points](#7-integration-points)
8. [Implementation Checklist](#8-implementation-checklist)

---

## 1. Overview

### 1.1 Purpose

This document provides a comprehensive design specification for the **Players & Profiles** vertical slice of the gaming portal. It covers the complete implementation across all architectural layers: Core (types/IDs), Database (entities/repositories), Domain (services/business logic), and API (handlers/DTOs).

### 1.2 Scope

The Players & Profiles domain encompasses:

- **Player Profiles**: Gaming identity linked to user accounts
- **Game Profiles**: Per-game statistics, ratings (Glicko-2), and plugin-defined stats
- **Player Relationships**: Friends, blocks, and friend requests
- **Badges & Achievements**: Player recognition and display items
- **Privacy & Settings**: Player preferences and visibility controls

### 1.3 Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Rating System | Glicko-2 | Superior to ELO for variable activity; handles uncertainty |
| Game Stats Storage | JSONB | Flexible schema for plugin-defined statistics |
| Relationship Model | Bidirectional with ordering | `player_a_id < player_b_id` ensures uniqueness |
| Profile-User Separation | 1:1 relationship | Clean separation of auth (user) and gaming (player) concerns |

### 1.4 Dependencies

- `portal-core`: ID types, validation types, enums
- `portal-db`: Entity definitions, repository implementations
- `portal-domain`: Service traits, business logic
- `portal-api`: HTTP handlers, DTOs
- **External**: Game plugins for stats schema and calculations

---

## 2. Domain Model

### 2.1 Entity Relationships

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           PLAYERS DOMAIN MODEL                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────┐      1:1       ┌──────────┐                                   │
│  │   User   │───────────────►│  Player  │                                   │
│  └──────────┘                └────┬─────┘                                   │
│  (Auth domain)                    │                                          │
│                                   │                                          │
│                    ┌──────────────┼──────────────┐                          │
│                    │              │              │                           │
│                    ▼              ▼              ▼                           │
│           ┌────────────┐  ┌─────────────┐  ┌────────────┐                   │
│           │   Game     │  │ Relationship│  │   Badge    │                   │
│           │  Profile   │  │  (Friend/   │  │ Assignment │                   │
│           │            │  │   Block)    │  │            │                   │
│           └────────────┘  └─────────────┘  └────────────┘                   │
│                 │                                   │                        │
│                 │ 1:N (one per game)               │ M:N                    │
│                 ▼                                   ▼                        │
│           ┌────────────┐                    ┌────────────┐                  │
│           │   Game     │                    │   Badge    │                  │
│           │ (plugin)   │                    │ Definition │                  │
│           └────────────┘                    └────────────┘                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Player Entity

The core gaming identity for a user.

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `user_id` | UUID | FK to users (1:1, unique) |
| `display_name` | VARCHAR(32) | Public display name |
| `display_name_normalized` | VARCHAR(32) | Lowercase for search (generated) |
| `avatar_url` | VARCHAR(512) | Profile picture URL |
| `banner_url` | VARCHAR(512) | Profile banner URL |
| `bio` | TEXT | Player biography |
| `country_code` | CHAR(2) | ISO 3166-1 alpha-2 |
| `region` | VARCHAR(64) | Geographic region |
| `timezone` | VARCHAR(64) | IANA timezone |
| `social_links` | JSONB | External profiles (Twitter, Twitch, etc.) |
| `privacy_settings` | JSONB | Visibility preferences |
| `notification_settings` | JSONB | Notification preferences |
| `ui_preferences` | JSONB | UI customization |
| `steam_id` | VARCHAR(32) | Steam ID (legacy format) |
| `steam_id_64` | BIGINT | Steam ID 64-bit |
| `steam_profile` | JSONB | Cached Steam profile data |
| `featured_badge_id` | UUID | Currently displayed badge |
| `title` | VARCHAR(64) | Display title |
| `created_at` | TIMESTAMPTZ | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | Last update timestamp |

### 2.3 Player Game Profile Entity

Per-game statistics and rating for a player.

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `player_id` | UUID | FK to players |
| `game_id` | VARCHAR(32) | FK to games |
| **Glicko-2 Rating** | | |
| `rating` | INTEGER | Current rating (default: 1500) |
| `rating_deviation` | INTEGER | Rating uncertainty (default: 350) |
| `volatility` | DECIMAL(10,8) | Rating volatility (default: 0.06) |
| `peak_rating` | INTEGER | Highest achieved rating |
| `peak_rating_at` | TIMESTAMPTZ | When peak was achieved |
| **Rank Display** | | |
| `rank_tier` | VARCHAR(32) | Plugin-defined tier (e.g., "Gold", "Diamond") |
| `rank_division` | INTEGER | Division within tier |
| `rank_points` | INTEGER | Points toward next division |
| `rank_updated_at` | TIMESTAMPTZ | Last rank calculation |
| **Match Statistics** | | |
| `matches_played` | INTEGER | Total matches |
| `wins` | INTEGER | Win count |
| `losses` | INTEGER | Loss count |
| `draws` | INTEGER | Draw count |
| `win_streak` | INTEGER | Current win streak |
| `best_win_streak` | INTEGER | Best ever win streak |
| **Time Statistics** | | |
| `total_playtime_minutes` | INTEGER | Total play time |
| `avg_match_duration_minutes` | INTEGER | Average match length |
| **Plugin Data** | | |
| `game_specific_stats` | JSONB | Plugin-defined statistics |
| `achievements` | JSONB | Game achievements |
| `equipped_badge_id` | VARCHAR(64) | Game-specific badge |
| **Timestamps** | | |
| `first_match_at` | TIMESTAMPTZ | First match played |
| `last_match_at` | TIMESTAMPTZ | Most recent match |
| `created_at` | TIMESTAMPTZ | Profile creation |
| `updated_at` | TIMESTAMPTZ | Last update |

**Unique Constraint**: `(player_id, game_id)`

### 2.4 Player Relationship Entity

Tracks friendships and blocks between players.

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `player_a_id` | UUID | First player (must be < player_b_id) |
| `player_b_id` | UUID | Second player |
| `relationship_type` | VARCHAR(20) | `friend` or `blocked` |
| `friendship_status` | VARCHAR(20) | `pending` or `accepted` (for friends) |
| `requested_by` | UUID | Who initiated the relationship |
| `requested_at` | TIMESTAMPTZ | When request was sent |
| `responded_at` | TIMESTAMPTZ | When request was answered |
| `created_at` | TIMESTAMPTZ | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | Last update timestamp |

**Constraints**:
- `player_a_id < player_b_id` ensures uniqueness for bidirectional relationships
- Unique on `(player_a_id, player_b_id)`

### 2.5 Badge Entity

Achievement badges that players can earn and display.

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `slug` | VARCHAR(64) | Unique identifier (e.g., "champion_2024") |
| `name` | VARCHAR(128) | Display name |
| `description` | TEXT | How to earn the badge |
| `icon_url` | VARCHAR(512) | Badge image |
| `rarity` | VARCHAR(32) | `common`, `uncommon`, `rare`, `epic`, `legendary` |
| `category` | VARCHAR(64) | Badge category |
| `game_id` | VARCHAR(32) | NULL for platform badges, set for game-specific |
| `is_active` | BOOLEAN | Whether badge can be earned |
| `awarded_count` | INTEGER | Number of players with this badge |
| `metadata` | JSONB | Additional badge data |
| `created_at` | TIMESTAMPTZ | Creation timestamp |

### 2.6 Player Badge Entity

Assignment of badges to players.

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Primary key |
| `player_id` | UUID | FK to players |
| `badge_id` | UUID | FK to badges |
| `awarded_at` | TIMESTAMPTZ | When badge was earned |
| `awarded_reason` | TEXT | Context for award |
| `metadata` | JSONB | Award-specific data |

**Unique Constraint**: `(player_id, badge_id)`

---

## 3. Core Layer (`portal-core`)

### 3.1 ID Types

```rust
// src/ids.rs - Add to existing define_id! macro usage

define_id!(PlayerId, "ply");
define_id!(PlayerGameProfileId, "pgp");
define_id!(PlayerRelationshipId, "rel");
define_id!(BadgeId, "bdg");
define_id!(PlayerBadgeId, "pbdg");
```

### 3.2 Player Types

```rust
// src/types/player.rs

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Player account status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
    /// Active and in good standing
    Active,
    /// Temporarily inactive (self-imposed)
    Inactive,
    /// Suspended by platform (temporary)
    Suspended,
    /// Permanently banned
    Banned,
}

impl PlayerStatus {
    /// Check if player can participate in matches.
    #[must_use]
    pub fn can_play(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if player profile is visible.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Active | Self::Inactive)
    }
}

impl fmt::Display for PlayerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Suspended => write!(f, "suspended"),
            Self::Banned => write!(f, "banned"),
        }
    }
}

impl FromStr for PlayerStatus {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            "suspended" => Ok(Self::Suspended),
            "banned" => Ok(Self::Banned),
            _ => Err(crate::CoreError::InvalidEnumValue {
                enum_name: "PlayerStatus",
                value: s.to_string(),
            }),
        }
    }
}

/// Relationship type between players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    /// Mutual friendship
    Friend,
    /// One player has blocked another
    Blocked,
}

impl fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Friend => write!(f, "friend"),
            Self::Blocked => write!(f, "blocked"),
        }
    }
}

impl FromStr for RelationshipType {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "friend" => Ok(Self::Friend),
            "blocked" => Ok(Self::Blocked),
            _ => Err(crate::CoreError::InvalidEnumValue {
                enum_name: "RelationshipType",
                value: s.to_string(),
            }),
        }
    }
}

/// Friendship request/acceptance status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendshipStatus {
    /// Request sent, awaiting response
    Pending,
    /// Both parties accepted
    Accepted,
}

impl fmt::Display for FriendshipStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Accepted => write!(f, "accepted"),
        }
    }
}

impl FromStr for FriendshipStatus {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            _ => Err(crate::CoreError::InvalidEnumValue {
                enum_name: "FriendshipStatus",
                value: s.to_string(),
            }),
        }
    }
}

/// Badge rarity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl BadgeRarity {
    /// Get display color for the rarity.
    #[must_use]
    pub fn color(&self) -> &'static str {
        match self {
            Self::Common => "#9e9e9e",
            Self::Uncommon => "#4caf50",
            Self::Rare => "#2196f3",
            Self::Epic => "#9c27b0",
            Self::Legendary => "#ff9800",
        }
    }
}

impl fmt::Display for BadgeRarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Common => write!(f, "common"),
            Self::Uncommon => write!(f, "uncommon"),
            Self::Rare => write!(f, "rare"),
            Self::Epic => write!(f, "epic"),
            Self::Legendary => write!(f, "legendary"),
        }
    }
}
```

### 3.3 Validation Types

```rust
// src/types/player.rs (continued)

use crate::CoreError;

/// Validated display name (3-32 characters, alphanumeric with spaces/underscores).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DisplayName(String);

impl DisplayName {
    /// Create a new display name with validation.
    pub fn new(name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        let trimmed = name.trim();

        if trimmed.len() < 3 {
            return Err(CoreError::ValidationError {
                field: "display_name".to_string(),
                message: "Display name must be at least 3 characters".to_string(),
            });
        }
        if trimmed.len() > 32 {
            return Err(CoreError::ValidationError {
                field: "display_name".to_string(),
                message: "Display name must be at most 32 characters".to_string(),
            });
        }

        // Allow alphanumeric, spaces, underscores, hyphens
        let valid = trimmed.chars().all(|c| {
            c.is_alphanumeric() || c == ' ' || c == '_' || c == '-'
        });

        if !valid {
            return Err(CoreError::ValidationError {
                field: "display_name".to_string(),
                message: "Display name can only contain letters, numbers, spaces, underscores, and hyphens".to_string(),
            });
        }

        Ok(Self(trimmed.to_string()))
    }

    /// Get the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get normalized (lowercase) version.
    #[must_use]
    pub fn normalized(&self) -> String {
        self.0.to_lowercase()
    }
}

impl TryFrom<String> for DisplayName {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DisplayName> for String {
    fn from(name: DisplayName) -> Self {
        name.0
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validated ISO 3166-1 alpha-2 country code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CountryCode(String);

impl CountryCode {
    /// Create a new country code with validation.
    pub fn new(code: impl Into<String>) -> Result<Self, CoreError> {
        let code = code.into().to_uppercase();

        if code.len() != 2 {
            return Err(CoreError::ValidationError {
                field: "country_code".to_string(),
                message: "Country code must be exactly 2 characters".to_string(),
            });
        }

        if !code.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(CoreError::ValidationError {
                field: "country_code".to_string(),
                message: "Country code must be uppercase letters".to_string(),
            });
        }

        Ok(Self(code))
    }

    /// Get the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CountryCode {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CountryCode> for String {
    fn from(code: CountryCode) -> Self {
        code.0
    }
}

/// Player bio with length validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Bio(String);

impl Bio {
    /// Maximum bio length.
    pub const MAX_LENGTH: usize = 500;

    /// Create a new bio with validation.
    pub fn new(bio: impl Into<String>) -> Result<Self, CoreError> {
        let bio = bio.into();

        if bio.len() > Self::MAX_LENGTH {
            return Err(CoreError::ValidationError {
                field: "bio".to_string(),
                message: format!("Bio must be at most {} characters", Self::MAX_LENGTH),
            });
        }

        Ok(Self(bio))
    }

    /// Get the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Bio {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Bio> for String {
    fn from(bio: Bio) -> Self {
        bio.0
    }
}
```

### 3.4 Privacy Settings Types

```rust
// src/types/player.rs (continued)

/// Player privacy settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Show online/offline status
    #[serde(default = "default_true")]
    pub show_online_status: bool,

    /// Show match history to others
    #[serde(default = "default_true")]
    pub show_match_history: bool,

    /// Show statistics to others
    #[serde(default = "default_true")]
    pub show_statistics: bool,

    /// Allow friend requests
    #[serde(default = "default_true")]
    pub allow_friend_requests: bool,

    /// Allow team invitations
    #[serde(default = "default_true")]
    pub allow_team_invites: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            show_online_status: true,
            show_match_history: true,
            show_statistics: true,
            allow_friend_requests: true,
            allow_team_invites: true,
        }
    }
}

/// Social links structure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialLinks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitch: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub youtube: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub instagram: Option<String>,
}
```

### 3.5 Glicko-2 Types

```rust
// src/types/rating.rs

use serde::{Deserialize, Serialize};

/// Glicko-2 rating parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Glicko2Rating {
    /// Current rating (typically 1500 for new players)
    pub rating: i32,

    /// Rating deviation (uncertainty, typically 350 for new players)
    pub rating_deviation: i32,

    /// Volatility (expected fluctuation, typically 0.06)
    pub volatility: f64,
}

impl Default for Glicko2Rating {
    fn default() -> Self {
        Self {
            rating: 1500,
            rating_deviation: 350,
            volatility: 0.06,
        }
    }
}

impl Glicko2Rating {
    /// Create a new default rating for a new player.
    #[must_use]
    pub fn new_player() -> Self {
        Self::default()
    }

    /// Check if this is a provisional rating (high deviation).
    #[must_use]
    pub fn is_provisional(&self) -> bool {
        self.rating_deviation > 100
    }

    /// Get the 95% confidence interval.
    #[must_use]
    pub fn confidence_interval(&self) -> (i32, i32) {
        let margin = (2.0 * self.rating_deviation as f64) as i32;
        (self.rating - margin, self.rating + margin)
    }
}

/// Match result for rating calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    Win,
    Loss,
    Draw,
}

impl MatchResult {
    /// Get the score value for Glicko-2 calculation.
    #[must_use]
    pub fn score(&self) -> f64 {
        match self {
            Self::Win => 1.0,
            Self::Loss => 0.0,
            Self::Draw => 0.5,
        }
    }
}
```

### 3.6 Error Types

```rust
// src/error.rs - Add to existing error enum

/// Player-related errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PlayerError {
    #[error("Player not found: {0}")]
    NotFound(PlayerId),

    #[error("Player already exists for user: {0}")]
    AlreadyExists(UserId),

    #[error("Player is not active: {0}")]
    NotActive(PlayerId),

    #[error("Display name already taken: {0}")]
    DisplayNameTaken(String),

    #[error("Cannot modify another player's profile")]
    NotOwner,

    #[error("Player is banned")]
    Banned,

    #[error("Steam ID already linked to another player")]
    SteamIdTaken,
}

/// Player relationship errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RelationshipError {
    #[error("Cannot create relationship with self")]
    SelfRelationship,

    #[error("Relationship already exists")]
    AlreadyExists,

    #[error("Friend request not found")]
    RequestNotFound,

    #[error("Player has blocked you")]
    Blocked,

    #[error("Player does not accept friend requests")]
    FriendRequestsDisabled,

    #[error("Not friends with this player")]
    NotFriends,
}

/// Game profile errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum GameProfileError {
    #[error("Game profile not found for player {player_id} in game {game_id}")]
    NotFound { player_id: PlayerId, game_id: String },

    #[error("Game profile already exists")]
    AlreadyExists,

    #[error("Game not found: {0}")]
    GameNotFound(String),

    #[error("Game is not active: {0}")]
    GameNotActive(String),
}
```

---

## 4. Database Layer (`portal-db`)

### 4.1 Entity Definitions

```rust
// src/entities/player.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `players` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub display_name_normalized: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: serde_json::Value,
    pub privacy_settings: serde_json::Value,
    pub notification_settings: serde_json::Value,
    pub ui_preferences: serde_json::Value,
    pub steam_id: Option<String>,
    pub steam_id_64: Option<i64>,
    pub steam_profile: Option<serde_json::Value>,
    pub featured_badge_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for inserting a new player.
#[derive(Debug, Clone)]
pub struct NewPlayer {
    pub user_id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub country_code: Option<String>,
    pub timezone: Option<String>,
}

/// Data for updating an existing player.
#[derive(Debug, Clone, Default)]
pub struct UpdatePlayer {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: Option<serde_json::Value>,
    pub privacy_settings: Option<serde_json::Value>,
    pub notification_settings: Option<serde_json::Value>,
    pub ui_preferences: Option<serde_json::Value>,
    pub steam_id: Option<String>,
    pub steam_id_64: Option<i64>,
    pub featured_badge_id: Option<Uuid>,
    pub title: Option<String>,
}
```

```rust
// src/entities/player_game_profile.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `player_game_profiles` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerGameProfileRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub game_id: String,

    // Glicko-2 Rating
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub peak_rating: i32,
    pub peak_rating_at: Option<DateTime<Utc>>,

    // Rank Display
    pub rank_tier: Option<String>,
    pub rank_division: Option<i32>,
    pub rank_points: Option<i32>,
    pub rank_updated_at: Option<DateTime<Utc>>,

    // Match Statistics
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub win_streak: i32,
    pub best_win_streak: i32,

    // Time Statistics
    pub total_playtime_minutes: i32,
    pub avg_match_duration_minutes: Option<i32>,

    // Plugin Data
    pub game_specific_stats: serde_json::Value,
    pub achievements: serde_json::Value,
    pub equipped_badge_id: Option<String>,

    // Timestamps
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new game profile.
#[derive(Debug, Clone)]
pub struct NewPlayerGameProfile {
    pub player_id: Uuid,
    pub game_id: String,
}

/// Data for updating rating after a match.
#[derive(Debug, Clone)]
pub struct UpdatePlayerRating {
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub match_result: MatchResultUpdate,
    pub match_duration_minutes: Option<i32>,
    pub game_specific_stats: Option<serde_json::Value>,
}

/// Match result for statistics update.
#[derive(Debug, Clone, Copy)]
pub enum MatchResultUpdate {
    Win,
    Loss,
    Draw,
}
```

```rust
// src/entities/player_relationship.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `player_relationships` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerRelationshipRow {
    pub id: Uuid,
    pub player_a_id: Uuid,
    pub player_b_id: Uuid,
    pub relationship_type: String,
    pub friendship_status: Option<String>,
    pub requested_by: Option<Uuid>,
    pub requested_at: Option<DateTime<Utc>>,
    pub responded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Data for creating a new relationship.
#[derive(Debug, Clone)]
pub struct NewPlayerRelationship {
    /// The player initiating (always stored as smaller UUID)
    pub player_a_id: Uuid,
    /// The other player (always stored as larger UUID)
    pub player_b_id: Uuid,
    pub relationship_type: String,
    pub friendship_status: Option<String>,
    pub requested_by: Uuid,
}

impl NewPlayerRelationship {
    /// Create a new relationship, ensuring proper ordering.
    pub fn new(from_player: Uuid, to_player: Uuid, rel_type: &str) -> Self {
        let (player_a_id, player_b_id) = if from_player < to_player {
            (from_player, to_player)
        } else {
            (to_player, from_player)
        };

        Self {
            player_a_id,
            player_b_id,
            relationship_type: rel_type.to_string(),
            friendship_status: if rel_type == "friend" {
                Some("pending".to_string())
            } else {
                None
            },
            requested_by: from_player,
        }
    }
}
```

```rust
// src/entities/badge.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Database row for the `badges` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BadgeRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub rarity: String,
    pub category: Option<String>,
    pub game_id: Option<String>,
    pub is_active: bool,
    pub awarded_count: i32,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Database row for the `player_badges` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct PlayerBadgeRow {
    pub id: Uuid,
    pub player_id: Uuid,
    pub badge_id: Uuid,
    pub awarded_at: DateTime<Utc>,
    pub awarded_reason: Option<String>,
    pub metadata: serde_json::Value,
}

/// Data for awarding a badge to a player.
#[derive(Debug, Clone)]
pub struct NewPlayerBadge {
    pub player_id: Uuid,
    pub badge_id: Uuid,
    pub awarded_reason: Option<String>,
    pub metadata: Option<serde_json::Value>,
}
```

### 4.2 Repository Implementations

```rust
// src/repositories/player_repository.rs

use crate::entities::player::{NewPlayer, PlayerRow, UpdatePlayer};
use crate::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

/// Player repository for database operations.
pub struct PlayerRepository {
    pool: PgPool,
}

impl PlayerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a player by ID.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<PlayerRow>, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            SELECT id, user_id, display_name, display_name_normalized,
                   avatar_url, banner_url, bio, country_code, region, timezone,
                   social_links, privacy_settings, notification_settings,
                   ui_preferences, steam_id, steam_id_64, steam_profile,
                   featured_badge_id, title, created_at, updated_at
            FROM players
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Find a player by user ID.
    pub async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerRow>, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            SELECT id, user_id, display_name, display_name_normalized,
                   avatar_url, banner_url, bio, country_code, region, timezone,
                   social_links, privacy_settings, notification_settings,
                   ui_preferences, steam_id, steam_id_64, steam_profile,
                   featured_badge_id, title, created_at, updated_at
            FROM players
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Find a player by display name (case-insensitive).
    pub async fn find_by_display_name(&self, name: &str) -> Result<Option<PlayerRow>, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            SELECT id, user_id, display_name, display_name_normalized,
                   avatar_url, banner_url, bio, country_code, region, timezone,
                   social_links, privacy_settings, notification_settings,
                   ui_preferences, steam_id, steam_id_64, steam_profile,
                   featured_badge_id, title, created_at, updated_at
            FROM players
            WHERE display_name_normalized = lower($1)
            "#,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Search players by name with pagination.
    pub async fn search(
        &self,
        query: Option<&str>,
        country_code: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerRow>, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            SELECT id, user_id, display_name, display_name_normalized,
                   avatar_url, banner_url, bio, country_code, region, timezone,
                   social_links, privacy_settings, notification_settings,
                   ui_preferences, steam_id, steam_id_64, steam_profile,
                   featured_badge_id, title, created_at, updated_at
            FROM players
            WHERE ($1::text IS NULL OR display_name_normalized LIKE '%' || lower($1) || '%')
              AND ($2::text IS NULL OR country_code = $2)
            ORDER BY display_name_normalized
            LIMIT $3 OFFSET $4
            "#,
            query,
            country_code,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Create a new player.
    pub async fn create(&self, new_player: NewPlayer) -> Result<PlayerRow, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            INSERT INTO players (user_id, display_name, avatar_url, country_code, timezone)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, user_id, display_name, display_name_normalized,
                      avatar_url, banner_url, bio, country_code, region, timezone,
                      social_links, privacy_settings, notification_settings,
                      ui_preferences, steam_id, steam_id_64, steam_profile,
                      featured_badge_id, title, created_at, updated_at
            "#,
            new_player.user_id,
            new_player.display_name,
            new_player.avatar_url,
            new_player.country_code,
            new_player.timezone
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Update an existing player.
    pub async fn update(&self, id: Uuid, update: UpdatePlayer) -> Result<PlayerRow, DbError> {
        sqlx::query_as!(
            PlayerRow,
            r#"
            UPDATE players SET
                display_name = COALESCE($2, display_name),
                avatar_url = COALESCE($3, avatar_url),
                banner_url = COALESCE($4, banner_url),
                bio = COALESCE($5, bio),
                country_code = COALESCE($6, country_code),
                region = COALESCE($7, region),
                timezone = COALESCE($8, timezone),
                social_links = COALESCE($9, social_links),
                privacy_settings = COALESCE($10, privacy_settings),
                notification_settings = COALESCE($11, notification_settings),
                ui_preferences = COALESCE($12, ui_preferences),
                steam_id = COALESCE($13, steam_id),
                steam_id_64 = COALESCE($14, steam_id_64),
                featured_badge_id = COALESCE($15, featured_badge_id),
                title = COALESCE($16, title),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, user_id, display_name, display_name_normalized,
                      avatar_url, banner_url, bio, country_code, region, timezone,
                      social_links, privacy_settings, notification_settings,
                      ui_preferences, steam_id, steam_id_64, steam_profile,
                      featured_badge_id, title, created_at, updated_at
            "#,
            id,
            update.display_name,
            update.avatar_url,
            update.banner_url,
            update.bio,
            update.country_code,
            update.region,
            update.timezone,
            update.social_links,
            update.privacy_settings,
            update.notification_settings,
            update.ui_preferences,
            update.steam_id,
            update.steam_id_64,
            update.featured_badge_id,
            update.title
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Check if a display name is available.
    pub async fn is_display_name_available(&self, name: &str, exclude_id: Option<Uuid>) -> Result<bool, DbError> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM players
                WHERE display_name_normalized = lower($1)
                  AND ($2::uuid IS NULL OR id != $2)
            ) as "exists!"
            "#,
            name,
            exclude_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(!result)
    }
}
```

### 4.3 Game Profile Repository

```rust
// src/repositories/player_game_profile_repository.rs

use crate::entities::player_game_profile::{
    NewPlayerGameProfile, PlayerGameProfileRow, UpdatePlayerRating,
};
use crate::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PlayerGameProfileRepository {
    pool: PgPool,
}

impl PlayerGameProfileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a game profile by player and game.
    pub async fn find_by_player_and_game(
        &self,
        player_id: Uuid,
        game_id: &str,
    ) -> Result<Option<PlayerGameProfileRow>, DbError> {
        sqlx::query_as!(
            PlayerGameProfileRow,
            r#"
            SELECT * FROM player_game_profiles
            WHERE player_id = $1 AND game_id = $2
            "#,
            player_id,
            game_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Get all game profiles for a player.
    pub async fn find_all_for_player(
        &self,
        player_id: Uuid,
    ) -> Result<Vec<PlayerGameProfileRow>, DbError> {
        sqlx::query_as!(
            PlayerGameProfileRow,
            r#"
            SELECT * FROM player_game_profiles
            WHERE player_id = $1
            ORDER BY last_match_at DESC NULLS LAST
            "#,
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Create a new game profile.
    pub async fn create(
        &self,
        new_profile: NewPlayerGameProfile,
    ) -> Result<PlayerGameProfileRow, DbError> {
        sqlx::query_as!(
            PlayerGameProfileRow,
            r#"
            INSERT INTO player_game_profiles (player_id, game_id)
            VALUES ($1, $2)
            RETURNING *
            "#,
            new_profile.player_id,
            new_profile.game_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Update rating and stats after a match.
    pub async fn update_after_match(
        &self,
        id: Uuid,
        update: UpdatePlayerRating,
    ) -> Result<PlayerGameProfileRow, DbError> {
        let (win_inc, loss_inc, draw_inc) = match update.match_result {
            MatchResultUpdate::Win => (1, 0, 0),
            MatchResultUpdate::Loss => (0, 1, 0),
            MatchResultUpdate::Draw => (0, 0, 1),
        };

        sqlx::query_as!(
            PlayerGameProfileRow,
            r#"
            UPDATE player_game_profiles SET
                rating = $2,
                rating_deviation = $3,
                volatility = $4,
                peak_rating = GREATEST(peak_rating, $2),
                peak_rating_at = CASE WHEN $2 > peak_rating THEN NOW() ELSE peak_rating_at END,
                matches_played = matches_played + 1,
                wins = wins + $5,
                losses = losses + $6,
                draws = draws + $7,
                win_streak = CASE
                    WHEN $5 = 1 THEN win_streak + 1
                    ELSE 0
                END,
                best_win_streak = CASE
                    WHEN $5 = 1 AND win_streak + 1 > best_win_streak
                    THEN win_streak + 1
                    ELSE best_win_streak
                END,
                total_playtime_minutes = total_playtime_minutes + COALESCE($8, 0),
                game_specific_stats = COALESCE($9, game_specific_stats),
                first_match_at = COALESCE(first_match_at, NOW()),
                last_match_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
            id,
            update.rating,
            update.rating_deviation,
            update.volatility,
            win_inc,
            loss_inc,
            draw_inc,
            update.match_duration_minutes,
            update.game_specific_stats
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Get leaderboard for a game.
    pub async fn get_leaderboard(
        &self,
        game_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PlayerGameProfileRow>, DbError> {
        sqlx::query_as!(
            PlayerGameProfileRow,
            r#"
            SELECT * FROM player_game_profiles
            WHERE game_id = $1
              AND matches_played >= 10
            ORDER BY rating DESC, rating_deviation ASC
            LIMIT $2 OFFSET $3
            "#,
            game_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }
}
```

### 4.4 Relationship Repository

```rust
// src/repositories/player_relationship_repository.rs

use crate::entities::player_relationship::{NewPlayerRelationship, PlayerRelationshipRow};
use crate::error::DbError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PlayerRelationshipRepository {
    pool: PgPool,
}

impl PlayerRelationshipRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find relationship between two players.
    pub async fn find_between(
        &self,
        player_a: Uuid,
        player_b: Uuid,
    ) -> Result<Option<PlayerRelationshipRow>, DbError> {
        let (a, b) = if player_a < player_b {
            (player_a, player_b)
        } else {
            (player_b, player_a)
        };

        sqlx::query_as!(
            PlayerRelationshipRow,
            r#"
            SELECT * FROM player_relationships
            WHERE player_a_id = $1 AND player_b_id = $2
            "#,
            a,
            b
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Get all friends for a player.
    pub async fn get_friends(&self, player_id: Uuid) -> Result<Vec<Uuid>, DbError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                CASE
                    WHEN player_a_id = $1 THEN player_b_id
                    ELSE player_a_id
                END as "friend_id!"
            FROM player_relationships
            WHERE (player_a_id = $1 OR player_b_id = $1)
              AND relationship_type = 'friend'
              AND friendship_status = 'accepted'
            "#,
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(rows.into_iter().map(|r| r.friend_id).collect())
    }

    /// Get pending friend requests for a player.
    pub async fn get_pending_requests(&self, player_id: Uuid) -> Result<Vec<PlayerRelationshipRow>, DbError> {
        sqlx::query_as!(
            PlayerRelationshipRow,
            r#"
            SELECT * FROM player_relationships
            WHERE (player_a_id = $1 OR player_b_id = $1)
              AND relationship_type = 'friend'
              AND friendship_status = 'pending'
              AND requested_by != $1
            "#,
            player_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Create a new relationship.
    pub async fn create(
        &self,
        new_rel: NewPlayerRelationship,
    ) -> Result<PlayerRelationshipRow, DbError> {
        sqlx::query_as!(
            PlayerRelationshipRow,
            r#"
            INSERT INTO player_relationships
                (player_a_id, player_b_id, relationship_type, friendship_status, requested_by, requested_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            RETURNING *
            "#,
            new_rel.player_a_id,
            new_rel.player_b_id,
            new_rel.relationship_type,
            new_rel.friendship_status,
            new_rel.requested_by
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Accept a friend request.
    pub async fn accept_friend_request(&self, id: Uuid) -> Result<PlayerRelationshipRow, DbError> {
        sqlx::query_as!(
            PlayerRelationshipRow,
            r#"
            UPDATE player_relationships SET
                friendship_status = 'accepted',
                responded_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND relationship_type = 'friend' AND friendship_status = 'pending'
            RETURNING *
            "#,
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)
    }

    /// Delete a relationship.
    pub async fn delete(&self, id: Uuid) -> Result<bool, DbError> {
        let result = sqlx::query!(
            r#"DELETE FROM player_relationships WHERE id = $1"#,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if player is blocked.
    pub async fn is_blocked(&self, player_id: Uuid, by_player_id: Uuid) -> Result<bool, DbError> {
        let (a, b) = if player_id < by_player_id {
            (player_id, by_player_id)
        } else {
            (by_player_id, player_id)
        };

        let result = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM player_relationships
                WHERE player_a_id = $1 AND player_b_id = $2
                  AND relationship_type = 'blocked'
            ) as "exists!"
            "#,
            a,
            b
        )
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(result)
    }
}
```

---

## 5. Domain Layer (`portal-domain`)

### 5.1 Domain Entities

```rust
// src/entities/player.rs

use chrono::{DateTime, Utc};
use portal_core::{PlayerId, UserId, PrivacySettings, SocialLinks};

/// Player domain entity.
#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub user_id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: SocialLinks,
    pub privacy_settings: PrivacySettings,
    pub steam_id: Option<String>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Player {
    /// Check if Steam account is linked.
    #[must_use]
    pub fn has_steam_linked(&self) -> bool {
        self.steam_id.is_some()
    }

    /// Check if profile is publicly visible based on privacy settings.
    #[must_use]
    pub fn is_profile_public(&self) -> bool {
        self.privacy_settings.show_statistics
    }

    /// Check if match history is visible.
    #[must_use]
    pub fn is_match_history_visible(&self) -> bool {
        self.privacy_settings.show_match_history
    }

    /// Check if friend requests are allowed.
    #[must_use]
    pub fn accepts_friend_requests(&self) -> bool {
        self.privacy_settings.allow_friend_requests
    }

    /// Check if team invitations are allowed.
    #[must_use]
    pub fn accepts_team_invites(&self) -> bool {
        self.privacy_settings.allow_team_invites
    }
}

// Conversion from database row
impl From<portal_db::entities::PlayerRow> for Player {
    fn from(row: portal_db::entities::PlayerRow) -> Self {
        Self {
            id: PlayerId::from(row.id),
            user_id: UserId::from(row.user_id),
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            banner_url: row.banner_url,
            bio: row.bio,
            country_code: row.country_code,
            region: row.region,
            timezone: row.timezone,
            social_links: serde_json::from_value(row.social_links).unwrap_or_default(),
            privacy_settings: serde_json::from_value(row.privacy_settings).unwrap_or_default(),
            steam_id: row.steam_id,
            title: row.title,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
```

```rust
// src/entities/player_game_profile.rs

use chrono::{DateTime, Utc};
use portal_core::{PlayerId, PlayerGameProfileId, Glicko2Rating};

/// Player game profile domain entity.
#[derive(Debug, Clone)]
pub struct PlayerGameProfile {
    pub id: PlayerGameProfileId,
    pub player_id: PlayerId,
    pub game_id: String,
    pub rating: Glicko2Rating,
    pub peak_rating: i32,
    pub peak_rating_at: Option<DateTime<Utc>>,
    pub rank_tier: Option<String>,
    pub rank_division: Option<i32>,
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub win_streak: i32,
    pub best_win_streak: i32,
    pub total_playtime_minutes: i32,
    pub game_specific_stats: serde_json::Value,
    pub first_match_at: Option<DateTime<Utc>>,
    pub last_match_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PlayerGameProfile {
    /// Calculate win rate as a percentage.
    #[must_use]
    pub fn win_rate(&self) -> f64 {
        if self.matches_played == 0 {
            return 0.0;
        }
        (self.wins as f64 / self.matches_played as f64) * 100.0
    }

    /// Check if rating is provisional.
    #[must_use]
    pub fn is_provisional(&self) -> bool {
        self.rating.is_provisional()
    }

    /// Get total hours played.
    #[must_use]
    pub fn total_hours_played(&self) -> f64 {
        self.total_playtime_minutes as f64 / 60.0
    }
}

impl From<portal_db::entities::PlayerGameProfileRow> for PlayerGameProfile {
    fn from(row: portal_db::entities::PlayerGameProfileRow) -> Self {
        Self {
            id: PlayerGameProfileId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: row.game_id,
            rating: Glicko2Rating {
                rating: row.rating,
                rating_deviation: row.rating_deviation,
                volatility: row.volatility,
            },
            peak_rating: row.peak_rating,
            peak_rating_at: row.peak_rating_at,
            rank_tier: row.rank_tier,
            rank_division: row.rank_division,
            matches_played: row.matches_played,
            wins: row.wins,
            losses: row.losses,
            draws: row.draws,
            win_streak: row.win_streak,
            best_win_streak: row.best_win_streak,
            total_playtime_minutes: row.total_playtime_minutes,
            game_specific_stats: row.game_specific_stats,
            first_match_at: row.first_match_at,
            last_match_at: row.last_match_at,
            created_at: row.created_at,
        }
    }
}
```

### 5.2 Repository Traits

```rust
// src/repositories/player.rs

use crate::entities::Player;
use async_trait::async_trait;
use portal_core::{PlayerId, UserId};
use std::error::Error;

/// Player repository trait.
#[async_trait]
pub trait PlayerRepository: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn find_by_id(&self, id: PlayerId) -> Result<Option<Player>, Self::Error>;
    async fn find_by_user_id(&self, user_id: UserId) -> Result<Option<Player>, Self::Error>;
    async fn find_by_display_name(&self, name: &str) -> Result<Option<Player>, Self::Error>;

    async fn search(
        &self,
        query: Option<&str>,
        country_code: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Player>, Self::Error>;

    async fn create(&self, new_player: CreatePlayer) -> Result<Player, Self::Error>;
    async fn update(&self, id: PlayerId, update: UpdatePlayer) -> Result<Player, Self::Error>;
    async fn is_display_name_available(&self, name: &str, exclude_id: Option<PlayerId>) -> Result<bool, Self::Error>;
}

/// Data for creating a player.
#[derive(Debug, Clone)]
pub struct CreatePlayer {
    pub user_id: UserId,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub country_code: Option<String>,
    pub timezone: Option<String>,
}

/// Data for updating a player.
#[derive(Debug, Clone, Default)]
pub struct UpdatePlayer {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: Option<portal_core::SocialLinks>,
    pub privacy_settings: Option<portal_core::PrivacySettings>,
    pub steam_id: Option<String>,
    pub title: Option<String>,
}
```

### 5.3 Player Service

```rust
// src/services/player.rs

use crate::entities::{Player, PlayerGameProfile};
use crate::repositories::{PlayerRepository, PlayerGameProfileRepository, PlayerRelationshipRepository};
use portal_core::{PlayerId, UserId, PlayerError, RelationshipError};
use std::sync::Arc;
use tracing::instrument;

/// Player service for business logic.
pub struct PlayerService<PR, GPR, RR>
where
    PR: PlayerRepository,
    GPR: PlayerGameProfileRepository,
    RR: PlayerRelationshipRepository,
{
    player_repo: Arc<PR>,
    game_profile_repo: Arc<GPR>,
    relationship_repo: Arc<RR>,
}

impl<PR, GPR, RR> PlayerService<PR, GPR, RR>
where
    PR: PlayerRepository,
    GPR: PlayerGameProfileRepository,
    RR: PlayerRelationshipRepository,
{
    pub fn new(
        player_repo: Arc<PR>,
        game_profile_repo: Arc<GPR>,
        relationship_repo: Arc<RR>,
    ) -> Self {
        Self {
            player_repo,
            game_profile_repo,
            relationship_repo,
        }
    }

    /// Get a player by ID.
    #[instrument(skip(self))]
    pub async fn get_player(&self, id: PlayerId) -> Result<Player, PlayerError> {
        self.player_repo
            .find_by_id(id)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))?
            .ok_or(PlayerError::NotFound(id))
    }

    /// Get a player by user ID.
    #[instrument(skip(self))]
    pub async fn get_player_by_user(&self, user_id: UserId) -> Result<Player, PlayerError> {
        self.player_repo
            .find_by_user_id(user_id)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))?
            .ok_or_else(|| PlayerError::NotFoundForUser(user_id))
    }

    /// Create a new player profile.
    #[instrument(skip(self))]
    pub async fn create_player(
        &self,
        user_id: UserId,
        display_name: String,
        avatar_url: Option<String>,
        country_code: Option<String>,
    ) -> Result<Player, PlayerError> {
        // Check if player already exists for user
        if self.player_repo.find_by_user_id(user_id).await
            .map_err(|e| PlayerError::Internal(e.to_string()))?
            .is_some()
        {
            return Err(PlayerError::AlreadyExists(user_id));
        }

        // Check display name availability
        if !self.player_repo.is_display_name_available(&display_name, None).await
            .map_err(|e| PlayerError::Internal(e.to_string()))?
        {
            return Err(PlayerError::DisplayNameTaken(display_name));
        }

        let create_data = crate::repositories::CreatePlayer {
            user_id,
            display_name,
            avatar_url,
            country_code,
            timezone: None,
        };

        self.player_repo
            .create(create_data)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))
    }

    /// Update a player profile.
    #[instrument(skip(self))]
    pub async fn update_player(
        &self,
        id: PlayerId,
        requesting_user_id: UserId,
        update: crate::repositories::UpdatePlayer,
    ) -> Result<Player, PlayerError> {
        let player = self.get_player(id).await?;

        // Verify ownership
        if player.user_id != requesting_user_id {
            return Err(PlayerError::NotOwner);
        }

        // Check display name availability if changing
        if let Some(ref new_name) = update.display_name {
            if !self.player_repo.is_display_name_available(new_name, Some(id)).await
                .map_err(|e| PlayerError::Internal(e.to_string()))?
            {
                return Err(PlayerError::DisplayNameTaken(new_name.clone()));
            }
        }

        self.player_repo
            .update(id, update)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))
    }

    /// Get all game profiles for a player.
    #[instrument(skip(self))]
    pub async fn get_game_profiles(&self, player_id: PlayerId) -> Result<Vec<PlayerGameProfile>, PlayerError> {
        self.game_profile_repo
            .find_all_for_player(player_id)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))
    }

    /// Send a friend request.
    #[instrument(skip(self))]
    pub async fn send_friend_request(
        &self,
        from_player_id: PlayerId,
        to_player_id: PlayerId,
    ) -> Result<(), RelationshipError> {
        if from_player_id == to_player_id {
            return Err(RelationshipError::SelfRelationship);
        }

        // Check if target player accepts friend requests
        let target = self.player_repo
            .find_by_id(to_player_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
            .ok_or(RelationshipError::PlayerNotFound)?;

        if !target.accepts_friend_requests() {
            return Err(RelationshipError::FriendRequestsDisabled);
        }

        // Check if blocked
        if self.relationship_repo.is_blocked(from_player_id, to_player_id).await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
        {
            return Err(RelationshipError::Blocked);
        }

        // Check if relationship already exists
        if self.relationship_repo.find_between(from_player_id, to_player_id).await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
            .is_some()
        {
            return Err(RelationshipError::AlreadyExists);
        }

        // Create friend request
        self.relationship_repo
            .create_friend_request(from_player_id, to_player_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Accept a friend request.
    #[instrument(skip(self))]
    pub async fn accept_friend_request(
        &self,
        player_id: PlayerId,
        request_id: portal_core::PlayerRelationshipId,
    ) -> Result<(), RelationshipError> {
        let relationship = self.relationship_repo
            .find_by_id(request_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
            .ok_or(RelationshipError::RequestNotFound)?;

        // Verify the player is the recipient
        let is_recipient = relationship.player_a_id == player_id.into() || relationship.player_b_id == player_id.into();
        let is_not_requester = relationship.requested_by != Some(player_id.into());

        if !is_recipient || !is_not_requester {
            return Err(RelationshipError::RequestNotFound);
        }

        self.relationship_repo
            .accept_friend_request(request_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Remove a friend.
    #[instrument(skip(self))]
    pub async fn remove_friend(
        &self,
        player_id: PlayerId,
        friend_id: PlayerId,
    ) -> Result<(), RelationshipError> {
        let relationship = self.relationship_repo
            .find_between(player_id, friend_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
            .ok_or(RelationshipError::NotFriends)?;

        self.relationship_repo
            .delete(relationship.id.into())
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Block a player.
    #[instrument(skip(self))]
    pub async fn block_player(
        &self,
        blocker_id: PlayerId,
        blocked_id: PlayerId,
    ) -> Result<(), RelationshipError> {
        if blocker_id == blocked_id {
            return Err(RelationshipError::SelfRelationship);
        }

        // Remove any existing relationship first
        if let Some(rel) = self.relationship_repo.find_between(blocker_id, blocked_id).await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?
        {
            self.relationship_repo.delete(rel.id.into()).await
                .map_err(|e| RelationshipError::Internal(e.to_string()))?;
        }

        // Create block relationship
        self.relationship_repo
            .create_block(blocker_id, blocked_id)
            .await
            .map_err(|e| RelationshipError::Internal(e.to_string()))?;

        Ok(())
    }

    /// Get friends list for a player.
    #[instrument(skip(self))]
    pub async fn get_friends(&self, player_id: PlayerId) -> Result<Vec<Player>, PlayerError> {
        let friend_ids = self.relationship_repo
            .get_friends(player_id)
            .await
            .map_err(|e| PlayerError::Internal(e.to_string()))?;

        let mut friends = Vec::with_capacity(friend_ids.len());
        for id in friend_ids {
            if let Some(player) = self.player_repo.find_by_id(id.into()).await
                .map_err(|e| PlayerError::Internal(e.to_string()))?
            {
                friends.push(player);
            }
        }

        Ok(friends)
    }
}
```

---

## 6. API Layer (`portal-api`)

### 6.1 Request/Response DTOs

```rust
// src/dto/player.rs

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Request to create a player profile.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePlayerRequest {
    /// Display name (3-32 characters)
    pub display_name: String,
    /// Avatar URL
    pub avatar_url: Option<String>,
    /// ISO 3166-1 alpha-2 country code
    pub country_code: Option<String>,
}

/// Request to update a player profile.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePlayerRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: Option<SocialLinksDto>,
    pub privacy_settings: Option<PrivacySettingsDto>,
}

/// Player response DTO.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlayerResponse {
    pub id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub bio: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub social_links: SocialLinksDto,
    pub steam_id: Option<String>,
    pub title: Option<String>,
    pub is_online: bool,
    pub created_at: String,
}

/// Player search result DTO (abbreviated).
#[derive(Debug, Serialize, ToSchema)]
pub struct PlayerSummaryResponse {
    pub id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub country_code: Option<String>,
    pub title: Option<String>,
    pub primary_game: Option<PrimaryGameDto>,
}

/// Primary game info for player search results.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrimaryGameDto {
    pub game_id: String,
    pub rank_tier: Option<String>,
    pub rating: i32,
}

/// Social links DTO.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SocialLinksDto {
    pub twitter: Option<String>,
    pub twitch: Option<String>,
    pub discord: Option<String>,
    pub youtube: Option<String>,
}

/// Privacy settings DTO.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PrivacySettingsDto {
    pub show_online_status: bool,
    pub show_match_history: bool,
    pub show_statistics: bool,
    pub allow_friend_requests: bool,
    pub allow_team_invites: bool,
}

/// Player statistics response.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlayerStatsResponse {
    pub game_id: String,
    pub rating: i32,
    pub rating_deviation: i32,
    pub volatility: f64,
    pub rank_tier: Option<String>,
    pub rank_division: Option<i32>,
    pub peak_rating: i32,
    pub matches_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub win_rate: f64,
    pub win_streak: i32,
    pub best_win_streak: i32,
    pub total_playtime_hours: f64,
    pub game_specific: serde_json::Value,
}

/// Friend request DTO.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SendFriendRequestRequest {
    pub player_id: Uuid,
}

/// Friend list response item.
#[derive(Debug, Serialize, ToSchema)]
pub struct FriendResponse {
    pub id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub is_online: bool,
    pub last_online_at: Option<String>,
}
```

### 6.2 Handlers

```rust
// src/handlers/players.rs

use crate::dto::player::*;
use crate::extractors::{AuthenticatedUser, ValidatedJson};
use crate::state::AppState;
use crate::error::ApiError;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use uuid::Uuid;

/// Search players.
#[utoipa::path(
    get,
    path = "/v1/players",
    params(
        ("q" = Option<String>, Query, description = "Search query"),
        ("country" = Option<String>, Query, description = "Country code filter"),
        ("game_id" = Option<String>, Query, description = "Game filter"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("per_page" = Option<i64>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "Players found", body = PaginatedResponse<PlayerSummaryResponse>),
    ),
    tag = "Players"
)]
pub async fn search_players(
    State(state): State<AppState>,
    Query(params): Query<SearchPlayersParams>,
) -> Result<Json<PaginatedResponse<PlayerSummaryResponse>>, ApiError> {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let offset = (page - 1) * per_page;

    let players = state.player_service
        .search_players(params.q.as_deref(), params.country.as_deref(), per_page, offset)
        .await?;

    let responses: Vec<PlayerSummaryResponse> = players
        .into_iter()
        .map(|p| PlayerSummaryResponse::from(p))
        .collect();

    Ok(Json(PaginatedResponse {
        data: responses,
        pagination: Pagination {
            page,
            per_page,
            total_items: 0, // Would need count query
            total_pages: 0,
        },
    }))
}

/// Get player profile.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "Player found", body = PlayerResponse),
        (status = 404, description = "Player not found"),
    ),
    tag = "Players"
)]
pub async fn get_player(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
) -> Result<Json<DataResponse<PlayerResponse>>, ApiError> {
    let player = state.player_service
        .get_player(player_id.into())
        .await?;

    Ok(Json(DataResponse {
        data: PlayerResponse::from(player),
    }))
}

/// Update player profile.
#[utoipa::path(
    patch,
    path = "/v1/players/{player_id}",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
    ),
    request_body = UpdatePlayerRequest,
    responses(
        (status = 200, description = "Player updated", body = PlayerResponse),
        (status = 403, description = "Not authorized"),
        (status = 404, description = "Player not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "Players"
)]
pub async fn update_player(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(player_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdatePlayerRequest>,
) -> Result<Json<DataResponse<PlayerResponse>>, ApiError> {
    let update = portal_domain::repositories::UpdatePlayer {
        display_name: req.display_name,
        avatar_url: req.avatar_url,
        banner_url: req.banner_url,
        bio: req.bio,
        country_code: req.country_code,
        region: req.region,
        timezone: req.timezone,
        social_links: req.social_links.map(|s| s.into()),
        privacy_settings: req.privacy_settings.map(|p| p.into()),
        ..Default::default()
    };

    let player = state.player_service
        .update_player(player_id.into(), auth.user_id, update)
        .await?;

    Ok(Json(DataResponse {
        data: PlayerResponse::from(player),
    }))
}

/// Get player statistics for a game.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/stats",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
        ("game_id" = String, Query, description = "Game ID"),
    ),
    responses(
        (status = 200, description = "Stats found", body = PlayerStatsResponse),
        (status = 404, description = "Profile not found"),
    ),
    tag = "Players"
)]
pub async fn get_player_stats(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
    Query(params): Query<GetStatsParams>,
) -> Result<Json<DataResponse<PlayerStatsResponse>>, ApiError> {
    let profile = state.player_service
        .get_game_profile(player_id.into(), &params.game_id)
        .await?;

    Ok(Json(DataResponse {
        data: PlayerStatsResponse::from(profile),
    }))
}

/// Get player's friends list.
#[utoipa::path(
    get,
    path = "/v1/players/{player_id}/friends",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "Friends list", body = Vec<FriendResponse>),
    ),
    tag = "Players"
)]
pub async fn get_friends(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
) -> Result<Json<DataResponse<Vec<FriendResponse>>>, ApiError> {
    let friends = state.player_service
        .get_friends(player_id.into())
        .await?;

    let responses: Vec<FriendResponse> = friends
        .into_iter()
        .map(FriendResponse::from)
        .collect();

    Ok(Json(DataResponse { data: responses }))
}

/// Send a friend request.
#[utoipa::path(
    post,
    path = "/v1/players/{player_id}/friends",
    params(
        ("player_id" = Uuid, Path, description = "Player ID (self)"),
    ),
    request_body = SendFriendRequestRequest,
    responses(
        (status = 201, description = "Request sent"),
        (status = 400, description = "Cannot send request"),
        (status = 409, description = "Already friends/requested"),
    ),
    security(("bearer_auth" = [])),
    tag = "Players"
)]
pub async fn send_friend_request(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(player_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<SendFriendRequestRequest>,
) -> Result<Json<()>, ApiError> {
    // Verify the authenticated user owns this player profile
    let player = state.player_service.get_player(player_id.into()).await?;
    if player.user_id != auth.user_id {
        return Err(ApiError::forbidden("Cannot send friend requests for other players"));
    }

    state.player_service
        .send_friend_request(player_id.into(), req.player_id.into())
        .await?;

    Ok(Json(()))
}

/// Accept a friend request.
#[utoipa::path(
    post,
    path = "/v1/players/{player_id}/friends/{request_id}/accept",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
        ("request_id" = Uuid, Path, description = "Request ID"),
    ),
    responses(
        (status = 200, description = "Request accepted"),
        (status = 404, description = "Request not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "Players"
)]
pub async fn accept_friend_request(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((player_id, request_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<()>, ApiError> {
    let player = state.player_service.get_player(player_id.into()).await?;
    if player.user_id != auth.user_id {
        return Err(ApiError::forbidden("Cannot accept friend requests for other players"));
    }

    state.player_service
        .accept_friend_request(player_id.into(), request_id.into())
        .await?;

    Ok(Json(()))
}

/// Remove a friend.
#[utoipa::path(
    delete,
    path = "/v1/players/{player_id}/friends/{friend_id}",
    params(
        ("player_id" = Uuid, Path, description = "Player ID"),
        ("friend_id" = Uuid, Path, description = "Friend's player ID"),
    ),
    responses(
        (status = 204, description = "Friend removed"),
        (status = 404, description = "Not friends"),
    ),
    security(("bearer_auth" = [])),
    tag = "Players"
)]
pub async fn remove_friend(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((player_id, friend_id)): Path<(Uuid, Uuid)>,
) -> Result<(), ApiError> {
    let player = state.player_service.get_player(player_id.into()).await?;
    if player.user_id != auth.user_id {
        return Err(ApiError::forbidden("Cannot modify friend list for other players"));
    }

    state.player_service
        .remove_friend(player_id.into(), friend_id.into())
        .await?;

    Ok(())
}
```

### 6.3 Routes

```rust
// src/routes/players.rs

use crate::handlers::players;
use axum::{
    routing::{get, post, patch, delete},
    Router,
};
use crate::state::AppState;

pub fn player_routes() -> Router<AppState> {
    Router::new()
        // Player profiles
        .route("/v1/players", get(players::search_players))
        .route("/v1/players/:player_id", get(players::get_player))
        .route("/v1/players/:player_id", patch(players::update_player))

        // Player statistics
        .route("/v1/players/:player_id/stats", get(players::get_player_stats))
        .route("/v1/players/:player_id/games", get(players::get_game_profiles))
        .route("/v1/players/:player_id/games/:game_id", get(players::get_game_profile))

        // Player rankings
        .route("/v1/players/:player_id/rankings", get(players::get_rankings))

        // Match history
        .route("/v1/players/:player_id/matches", get(players::get_match_history))

        // Friends
        .route("/v1/players/:player_id/friends", get(players::get_friends))
        .route("/v1/players/:player_id/friends", post(players::send_friend_request))
        .route("/v1/players/:player_id/friends/:request_id/accept", post(players::accept_friend_request))
        .route("/v1/players/:player_id/friends/:request_id/decline", post(players::decline_friend_request))
        .route("/v1/players/:player_id/friends/:friend_id", delete(players::remove_friend))

        // Teams (player's teams)
        .route("/v1/players/:player_id/teams", get(players::get_player_teams))

        // Badges
        .route("/v1/players/:player_id/badges", get(players::get_player_badges))
}
```

---

## 7. Integration Points

### 7.1 Match Completion Flow

When a match completes, the following updates occur:

1. **Match Service** calls `PlayerGameProfileRepository.update_after_match()`
2. **Glicko-2 Calculator** computes new ratings based on match result and opponent ratings
3. **Game Plugin** calculates game-specific stats via `calculate_player_stats()`
4. **Rank Service** (plugin) determines new rank tier via `determine_rank_tier()`

```rust
// Flow in match completion handler
pub async fn on_match_complete(
    &self,
    match_id: MatchId,
    results: MatchResults,
) -> Result<(), MatchError> {
    // 1. Calculate new ratings for all players
    let rating_updates = self.glicko2_calculator.calculate_updates(&results)?;

    // 2. Get game plugin
    let plugin = self.plugin_manager.get_plugin(&results.game_id)?;

    // 3. Update each player's profile
    for (player_id, result) in results.player_results.iter() {
        let profile = self.game_profile_repo
            .find_by_player_and_game(*player_id, &results.game_id)
            .await?
            .ok_or(MatchError::ProfileNotFound)?;

        // Calculate game-specific stats
        let game_stats = plugin.calculate_player_stats(
            &profile.game_specific_stats,
            result,
        )?;

        // Update profile with new rating and stats
        let update = UpdatePlayerRating {
            rating: rating_updates.get(player_id).unwrap().rating,
            rating_deviation: rating_updates.get(player_id).unwrap().deviation,
            volatility: rating_updates.get(player_id).unwrap().volatility,
            match_result: result.outcome.into(),
            match_duration_minutes: Some(results.duration_minutes),
            game_specific_stats: Some(game_stats),
        };

        self.game_profile_repo.update_after_match(profile.id, update).await?;

        // Determine new rank tier
        let new_tier = plugin.determine_rank_tier(update.rating)?;
        if new_tier != profile.rank_tier {
            self.game_profile_repo.update_rank(profile.id, new_tier).await?;
        }
    }

    Ok(())
}
```

### 7.2 Team Membership

Players are linked to teams through `team_members`. When querying a player's teams:

```rust
// In PlayerService
pub async fn get_player_teams(&self, player_id: PlayerId) -> Result<Vec<TeamMembership>, Error> {
    self.team_member_repo
        .find_by_player(player_id)
        .await
}
```

### 7.3 Privacy Enforcement

Player privacy settings are enforced at the API layer:

```rust
// In get_player_stats handler
pub async fn get_player_stats(
    State(state): State<AppState>,
    auth: Option<AuthenticatedUser>,
    Path(player_id): Path<Uuid>,
    Query(params): Query<GetStatsParams>,
) -> Result<Json<DataResponse<PlayerStatsResponse>>, ApiError> {
    let player = state.player_service.get_player(player_id.into()).await?;

    // Check privacy settings
    let is_owner = auth.as_ref().map(|a| a.user_id == player.user_id).unwrap_or(false);

    if !is_owner && !player.privacy_settings.show_statistics {
        return Err(ApiError::forbidden("Player statistics are private"));
    }

    // Continue with stats retrieval...
}
```

---

## 8. Implementation Checklist

### 8.1 Core Layer
- [ ] Add `PlayerId`, `PlayerGameProfileId`, `PlayerRelationshipId`, `BadgeId` to `ids.rs`
- [ ] Create `src/types/player.rs` with enums and validation types
- [ ] Create `src/types/rating.rs` with Glicko-2 types
- [ ] Add player errors to `src/error.rs`

### 8.2 Database Layer
- [ ] Create `src/entities/player.rs` (extend existing)
- [ ] Create `src/entities/player_game_profile.rs`
- [ ] Create `src/entities/player_relationship.rs`
- [ ] Create `src/entities/badge.rs`
- [ ] Create `src/repositories/player_repository.rs`
- [ ] Create `src/repositories/player_game_profile_repository.rs`
- [ ] Create `src/repositories/player_relationship_repository.rs`
- [ ] Create `src/repositories/badge_repository.rs`
- [ ] Add badges table to migrations

### 8.3 Domain Layer
- [ ] Create `src/entities/player.rs` domain entity
- [ ] Create `src/entities/player_game_profile.rs` domain entity
- [ ] Create `src/repositories/player.rs` trait
- [ ] Create `src/repositories/player_game_profile.rs` trait
- [ ] Create `src/repositories/player_relationship.rs` trait
- [ ] Create `src/services/player.rs` service

### 8.4 API Layer
- [ ] Create `src/dto/player.rs` DTOs
- [ ] Create `src/handlers/players.rs` handlers
- [ ] Create `src/routes/players.rs` routes
- [ ] Add OpenAPI documentation

### 8.5 Database Migrations
- [ ] Create badges table migration
- [ ] Create player_badges table migration
- [ ] Add any missing indexes

---

## Appendix A: Database Migrations

### A.1 Badges Table

```sql
-- migrations/YYYYMMDD_create_badges.sql

CREATE TABLE badges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    icon_url VARCHAR(512),
    rarity VARCHAR(32) NOT NULL DEFAULT 'common',
    category VARCHAR(64),
    game_id VARCHAR(32) REFERENCES games(id) ON DELETE SET NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    awarded_count INTEGER NOT NULL DEFAULT 0,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT badges_check_rarity CHECK (rarity IN (
        'common', 'uncommon', 'rare', 'epic', 'legendary'
    ))
);

CREATE INDEX idx_badges_slug ON badges(slug);
CREATE INDEX idx_badges_game ON badges(game_id) WHERE game_id IS NOT NULL;
CREATE INDEX idx_badges_category ON badges(category) WHERE category IS NOT NULL;
```

### A.2 Player Badges Table

```sql
-- migrations/YYYYMMDD_create_player_badges.sql

CREATE TABLE player_badges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    badge_id UUID NOT NULL REFERENCES badges(id) ON DELETE CASCADE,
    awarded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    awarded_reason TEXT,
    metadata JSONB DEFAULT '{}',

    CONSTRAINT player_badges_unique UNIQUE (player_id, badge_id)
);

CREATE INDEX idx_player_badges_player ON player_badges(player_id);
CREATE INDEX idx_player_badges_badge ON player_badges(badge_id);
```

---

*End of Players & Profiles Design Document*
