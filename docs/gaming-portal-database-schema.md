# Database Schema Reference
## Multi-Game Competitive Gaming Portal

**Version:** 1.0  
**Database:** PostgreSQL 15+  
**ORM:** SQLx (compile-time verified queries)  
**Last Updated:** November 2024

---

## Table of Contents

1. [Schema Overview](#1-schema-overview)
2. [Naming Conventions](#2-naming-conventions)
3. [Core Identity Tables](#3-core-identity-tables)
4. [Authentication & Security Tables](#4-authentication--security-tables)
5. [RBAC Tables](#5-rbac-tables)
6. [Player & Team Tables](#6-player--team-tables)
7. [Game & Match Tables](#7-game--match-tables)
8. [Lobby Tables](#8-lobby-tables)
9. [Tournament & League Tables](#9-tournament--league-tables)
10. [Substitute System Tables](#10-substitute-system-tables)
11. [Game Server Tables](#11-game-server-tables)
12. [Plugin Data Tables](#12-plugin-data-tables)
13. [Saga & Workflow Tables](#13-saga--workflow-tables)
14. [Audit & System Tables](#14-audit--system-tables)
15. [Indexes & Performance](#15-indexes--performance)
16. [Migration Strategy](#16-migration-strategy)
17. [Entity Relationship Diagram](#17-entity-relationship-diagram)

---

## 1. Schema Overview

### 1.1 Database Statistics

| Category | Table Count | Description |
|----------|-------------|-------------|
| Core Identity | 3 | Users, Players, Profiles |
| Auth & Security | 5 | Tokens, OAuth, Sessions, Bans |
| RBAC | 4 | Roles, Permissions, Assignments |
| Players & Teams | 5 | Teams, Members, Invitations, Relationships, Game Profiles |
| Games & Matches | 6 | Games, Queues, Queue Entries, Matches, Players, Maps |
| Lobbies | 3 | Lobbies, Players, Chat |
| Leagues & Tournaments | 10 | Leagues, Members, Invitations, Seasons, Standings, Tournaments, Participants, Brackets |
| Substitutes | 5 | Pool, Availability, Requests |
| Game Servers | 4 | Servers, Reservations, Events, Configurations |
| Plugin Data | 2 | Generic plugin storage |
| Sagas | 2 | Workflow orchestration |
| Audit | 3 | Logs, Events, Notifications |
| **Total** | **52** | |

### 1.2 Core Data Model Relationships

**Players & Teams (M:N):**
- Players can belong to **multiple teams** simultaneously via `team_members`
- Teams can be created by **any player** - no special permissions required
- Team creators automatically become captains (`role='captain'`, `is_founder=true`)
- Captains are the team admin role with full management permissions
- Captains can promote other members to captain, invite/remove players
- Founders cannot be demoted or removed; there must always be at least one captain

**Games & Statistics:**
- Players have **per-game profiles** in `player_game_profiles` (one per game played)
- **Platform manages**: rating (Glicko-2), rating_deviation, volatility, match statistics
- **Plugins define**: `game_specific_stats` (JSONB schema), rank tiers, available maps
- Plugin provides `player_stats_schema()` for validation and `calculate_player_stats()` for computation

**Leagues & Membership:**
- Leagues are **game-specific** (`game_id` required) with optional divisions
- Games can have **multiple leagues** (e.g., Division 1, Division 2, Regional leagues)
- League hierarchy supported via `parent_league_id`, `division`, `tier_name`
- Access controlled via `access_type`: `open`, `invite_only`, `application`
- Players can belong to **zero or more leagues** via `league_members` table
- Must be a league member before participating in league seasons/tournaments

**Tournaments:**
- **Global tournaments**: `league_id = NULL`, created by platform admins, open to all
- **League tournaments**: `league_id` set, created by league admins, league members only
- **Multiple tournaments can run concurrently** (no restrictions)
- Custom `map_pool` (JSONB) supported if game plugin's `supports_custom_map_pool()` returns true
- Map pool fallback chain: tournament → league default → game default

**Permission Model:**
| Action | Required Permission |
|--------|---------------------|
| Create team | Any authenticated player |
| Manage team | Team captain |
| Create global tournament | Platform admin |
| Create league tournament | League admin (`membership_type = 'admin'`) |
| Set custom map pool | Tournament creator (if plugin allows) |

### 1.3 Core Design Principles

- **UUIDs for primary keys**: Enable distributed ID generation
- **Soft deletes where appropriate**: Maintain referential integrity for historical data
- **JSONB for flexible data**: Plugin-specific data, game metadata, preferences
- **Timestamptz for all timestamps**: Timezone-aware datetime handling
- **Explicit foreign keys**: Referential integrity with appropriate cascade rules
- **Check constraints**: Data validation at database level

---

## 2. Naming Conventions

### 2.1 General Rules

```
Tables:        snake_case, plural nouns (users, teams, matches)
Columns:       snake_case (created_at, player_id)
Primary Keys:  id (UUID)
Foreign Keys:  {referenced_table_singular}_id (user_id, team_id)
Indexes:       idx_{table}_{columns} (idx_users_email)
Constraints:   {table}_{type}_{description} (users_check_status)
Enums:         Referenced via VARCHAR with CHECK constraints
```

### 2.2 Common Column Patterns

```sql
-- Standard timestamp columns
created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()

-- Soft delete pattern
deleted_at TIMESTAMPTZ,
is_deleted BOOLEAN GENERATED ALWAYS AS (deleted_at IS NOT NULL) STORED

-- Status pattern
status VARCHAR(32) NOT NULL DEFAULT 'active',
CONSTRAINT {table}_check_status CHECK (status IN ('active', 'inactive', ...))
```

---

## 3. Core Identity Tables

### 3.1 users

Primary user account table for authentication.

```sql
CREATE TABLE users (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    username VARCHAR(32) NOT NULL,
    email VARCHAR(255) NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified_at TIMESTAMPTZ,
    
    -- Authentication
    password_hash VARCHAR(255),  -- NULL for OAuth-only accounts
    password_changed_at TIMESTAMPTZ,
    
    -- Two-Factor Authentication
    two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    two_factor_secret VARCHAR(255),
    two_factor_backup_codes JSONB,  -- Encrypted backup codes
    
    -- Account Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    status_reason TEXT,
    status_changed_at TIMESTAMPTZ,
    
    -- Metadata
    locale VARCHAR(10) DEFAULT 'en-US',
    timezone VARCHAR(64) DEFAULT 'UTC',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT users_username_unique UNIQUE (username),
    CONSTRAINT users_email_unique UNIQUE (email),
    CONSTRAINT users_check_status CHECK (status IN (
        'active', 'inactive', 'suspended', 'banned', 'pending_verification'
    )),
    CONSTRAINT users_check_username_format CHECK (
        username ~ '^[a-zA-Z0-9_-]{3,32}$'
    ),
    CONSTRAINT users_check_email_format CHECK (
        email ~ '^[^@]+@[^@]+\.[^@]+$'
    )
);

-- Indexes
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(lower(username));
CREATE INDEX idx_users_status ON users(status) WHERE status != 'active';
CREATE INDEX idx_users_created_at ON users(created_at DESC);

-- Triggers
CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
```

### 3.2 players

Gaming identity linked to a user account.

```sql
CREATE TABLE players (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    
    -- Profile Information
    display_name VARCHAR(32) NOT NULL,
    display_name_normalized VARCHAR(32) GENERATED ALWAYS AS (lower(display_name)) STORED,
    avatar_url VARCHAR(512),
    banner_url VARCHAR(512),
    bio TEXT,
    
    -- Location
    country_code CHAR(2),
    region VARCHAR(64),
    timezone VARCHAR(64),
    
    -- Social Links
    social_links JSONB DEFAULT '{}',
    -- Example: {"twitter": "handle", "twitch": "channel", "discord": "user#1234"}
    
    -- Privacy Settings
    privacy_settings JSONB DEFAULT '{
        "show_online_status": true,
        "show_match_history": true,
        "show_statistics": true,
        "allow_friend_requests": true,
        "allow_team_invites": true
    }',
    
    -- Platform Settings
    notification_settings JSONB DEFAULT '{}',
    ui_preferences JSONB DEFAULT '{}',
    
    -- Steam Integration
    steam_id VARCHAR(32),
    steam_id_64 BIGINT,
    steam_profile JSONB,
    
    -- Metadata
    featured_badge_id UUID,
    title VARCHAR(64),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT players_check_country_code CHECK (
        country_code IS NULL OR country_code ~ '^[A-Z]{2}$'
    ),
    CONSTRAINT players_steam_id_unique UNIQUE (steam_id),
    CONSTRAINT players_steam_id_64_unique UNIQUE (steam_id_64)
);

-- Indexes
CREATE INDEX idx_players_user_id ON players(user_id);
CREATE INDEX idx_players_display_name ON players(display_name_normalized);
CREATE INDEX idx_players_steam_id ON players(steam_id) WHERE steam_id IS NOT NULL;
CREATE INDEX idx_players_steam_id_64 ON players(steam_id_64) WHERE steam_id_64 IS NOT NULL;
CREATE INDEX idx_players_country ON players(country_code) WHERE country_code IS NOT NULL;

-- Full-text search index
CREATE INDEX idx_players_search ON players USING gin(
    to_tsvector('english', display_name || ' ' || COALESCE(bio, ''))
);
```

### 3.3 player_game_profiles

Per-game statistics and rankings for each player. Each player can have a profile for each game they play, with game-specific ELO/rating and statistics.

The core rating system (Glicko-2) is managed by the platform, while game-specific statistics are defined and calculated by the game plugin (e.g., kills/deaths for CS2, APM for RTS games).

```sql
CREATE TABLE player_game_profiles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    
    -- Rating System (Glicko-2) - Platform managed
    rating INTEGER NOT NULL DEFAULT 1500,
    rating_deviation INTEGER NOT NULL DEFAULT 350,
    volatility DECIMAL(10, 8) NOT NULL DEFAULT 0.06,
    peak_rating INTEGER NOT NULL DEFAULT 1500,
    peak_rating_at TIMESTAMPTZ,
    
    -- Rank Display (plugin defines tiers, platform calculates placement)
    rank_tier VARCHAR(32),
    rank_division INTEGER,
    rank_points INTEGER DEFAULT 0,
    rank_updated_at TIMESTAMPTZ,
    
    -- Match Statistics - Platform managed
    matches_played INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    win_streak INTEGER NOT NULL DEFAULT 0,
    best_win_streak INTEGER NOT NULL DEFAULT 0,
    
    -- Time Statistics
    total_playtime_minutes INTEGER NOT NULL DEFAULT 0,
    avg_match_duration_minutes INTEGER,
    
    -- Game-Specific Stats (defined and populated by game plugin)
    -- Structure varies by game. Examples:
    -- CS2: {"kills": 1000, "deaths": 800, "headshot_pct": 0.45, "adr": 85.2}
    -- AoE4: {"games_as_civ": {"english": 50, "french": 30}, "avg_apm": 120}
    game_specific_stats JSONB DEFAULT '{}',
    
    -- Achievements & Badges (plugin-defined)
    achievements JSONB DEFAULT '[]',
    equipped_badge_id VARCHAR(64),
    
    -- Timestamps
    first_match_at TIMESTAMPTZ,
    last_match_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT player_game_profiles_unique UNIQUE (player_id, game_id),
    CONSTRAINT player_game_profiles_check_rating CHECK (rating >= 0 AND rating <= 5000),
    CONSTRAINT player_game_profiles_check_rd CHECK (rating_deviation >= 0),
    CONSTRAINT player_game_profiles_check_wins CHECK (wins >= 0),
    CONSTRAINT player_game_profiles_check_losses CHECK (losses >= 0)
);

-- Indexes
CREATE INDEX idx_player_game_profiles_player ON player_game_profiles(player_id);
CREATE INDEX idx_player_game_profiles_game ON player_game_profiles(game_id);
CREATE INDEX idx_player_game_profiles_rating ON player_game_profiles(game_id, rating DESC);
CREATE INDEX idx_player_game_profiles_matches ON player_game_profiles(game_id, matches_played DESC);
```

---

## 4. Authentication & Security Tables

### 4.1 oauth_connections

External OAuth provider connections.

```sql
CREATE TABLE oauth_connections (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Provider Information
    provider VARCHAR(32) NOT NULL,
    provider_user_id VARCHAR(255) NOT NULL,
    provider_username VARCHAR(255),
    provider_email VARCHAR(255),
    provider_avatar_url VARCHAR(512),
    
    -- Tokens (encrypted at rest)
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TIMESTAMPTZ,
    token_scope VARCHAR(512),
    
    -- Provider-Specific Data
    provider_data JSONB DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT oauth_connections_unique UNIQUE (provider, provider_user_id),
    CONSTRAINT oauth_connections_check_provider CHECK (provider IN (
        'steam', 'discord', 'twitch', 'google', 'twitter', 'faceit', 'battlenet'
    ))
);

-- Indexes
CREATE INDEX idx_oauth_user ON oauth_connections(user_id);
CREATE INDEX idx_oauth_provider ON oauth_connections(provider, provider_user_id);
CREATE INDEX idx_oauth_steam ON oauth_connections(provider_user_id) 
    WHERE provider = 'steam';
```

### 4.2 refresh_tokens

JWT refresh token storage for revocation support.

```sql
CREATE TABLE refresh_tokens (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Token Data
    token_hash VARCHAR(64) NOT NULL,  -- SHA-256 hash of actual token
    token_family UUID NOT NULL,  -- For rotation detection
    
    -- Device Information
    device_id VARCHAR(64),
    device_name VARCHAR(128),
    device_type VARCHAR(32),
    user_agent TEXT,
    ip_address INET,
    
    -- Lifecycle
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason VARCHAR(64),
    
    -- Replaced Token Tracking
    replaced_by UUID REFERENCES refresh_tokens(id),
    
    -- Constraints
    CONSTRAINT refresh_tokens_token_hash_unique UNIQUE (token_hash),
    CONSTRAINT refresh_tokens_check_device_type CHECK (device_type IN (
        'web', 'mobile_ios', 'mobile_android', 'desktop', 'api', 'unknown'
    ))
);

-- Indexes
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);
CREATE INDEX idx_refresh_tokens_family ON refresh_tokens(token_family);
CREATE INDEX idx_refresh_tokens_expires ON refresh_tokens(expires_at) 
    WHERE revoked_at IS NULL;

-- Cleanup job target
CREATE INDEX idx_refresh_tokens_cleanup ON refresh_tokens(expires_at)
    WHERE revoked_at IS NULL AND expires_at < NOW();
```

### 4.3 user_sessions

Active session tracking for session management UI.

```sql
CREATE TABLE user_sessions (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    refresh_token_id UUID REFERENCES refresh_tokens(id) ON DELETE SET NULL,
    
    -- Session Info
    session_token_hash VARCHAR(64) NOT NULL,
    
    -- Device & Location
    device_fingerprint VARCHAR(64),
    device_name VARCHAR(128),
    device_type VARCHAR(32),
    browser VARCHAR(64),
    os VARCHAR(64),
    ip_address INET,
    ip_country CHAR(2),
    ip_city VARCHAR(128),
    
    -- Activity Tracking
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    
    -- Status
    is_current BOOLEAN DEFAULT FALSE,
    terminated_at TIMESTAMPTZ,
    terminated_reason VARCHAR(64),
    
    -- Constraints
    CONSTRAINT user_sessions_token_hash_unique UNIQUE (session_token_hash)
);

-- Indexes
CREATE INDEX idx_sessions_user ON user_sessions(user_id);
CREATE INDEX idx_sessions_active ON user_sessions(user_id, last_active_at DESC)
    WHERE terminated_at IS NULL;
```

### 4.4 password_reset_tokens

Password reset request tracking.

```sql
CREATE TABLE password_reset_tokens (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Token
    token_hash VARCHAR(64) NOT NULL,
    
    -- Request Info
    requested_ip INET,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    
    -- Completion
    used_at TIMESTAMPTZ,
    used_ip INET,
    
    -- Constraints
    CONSTRAINT password_reset_tokens_hash_unique UNIQUE (token_hash)
);

-- Indexes
CREATE INDEX idx_password_reset_user ON password_reset_tokens(user_id);
CREATE INDEX idx_password_reset_token ON password_reset_tokens(token_hash)
    WHERE used_at IS NULL;
```

### 4.5 bans

User ban records.

```sql
CREATE TABLE bans (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Target
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Admin who issued ban
    banned_by UUID REFERENCES users(id) ON DELETE SET NULL,
    
    -- Ban Scope
    ban_type VARCHAR(32) NOT NULL,
    scope_type VARCHAR(32),
    scope_id UUID,  -- game_id, tournament_id, team_id depending on scope
    
    -- Ban Details
    reason TEXT NOT NULL,
    internal_notes TEXT,
    evidence_urls JSONB DEFAULT '[]',
    
    -- Duration
    starts_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ends_at TIMESTAMPTZ,  -- NULL for permanent
    is_permanent BOOLEAN GENERATED ALWAYS AS (ends_at IS NULL) STORED,
    
    -- Appeal
    appeal_status VARCHAR(32),
    appeal_text TEXT,
    appeal_submitted_at TIMESTAMPTZ,
    appeal_resolved_by UUID REFERENCES users(id),
    appeal_resolved_at TIMESTAMPTZ,
    appeal_resolution TEXT,
    
    -- Lifting
    lifted_at TIMESTAMPTZ,
    lifted_by UUID REFERENCES users(id),
    lift_reason TEXT,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT bans_check_type CHECK (ban_type IN (
        'platform', 'game', 'tournament', 'league', 'matchmaking', 'chat'
    )),
    CONSTRAINT bans_check_scope CHECK (
        (scope_type IS NULL AND scope_id IS NULL) OR
        (scope_type IS NOT NULL AND scope_id IS NOT NULL)
    ),
    CONSTRAINT bans_check_appeal_status CHECK (appeal_status IS NULL OR appeal_status IN (
        'pending', 'under_review', 'approved', 'denied'
    ))
);

-- Indexes
CREATE INDEX idx_bans_user ON bans(user_id);
CREATE INDEX idx_bans_active ON bans(user_id, ban_type)
    WHERE lifted_at IS NULL AND (ends_at IS NULL OR ends_at > NOW());
CREATE INDEX idx_bans_scope ON bans(scope_type, scope_id)
    WHERE scope_type IS NOT NULL;
CREATE INDEX idx_bans_appeals ON bans(appeal_status)
    WHERE appeal_status IN ('pending', 'under_review');
```

---

## 5. RBAC Tables

### 5.1 roles

Role definitions.

```sql
CREATE TABLE roles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(64) NOT NULL,
    display_name VARCHAR(128) NOT NULL,
    description TEXT,
    
    -- Categorization
    category VARCHAR(32) NOT NULL DEFAULT 'custom',
    
    -- Hierarchy
    priority INTEGER NOT NULL DEFAULT 0,  -- Higher = more powerful
    parent_role_id UUID REFERENCES roles(id) ON DELETE SET NULL,
    
    -- Flags
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,  -- Auto-assigned to new users
    is_assignable BOOLEAN NOT NULL DEFAULT TRUE,
    
    -- Visual
    color VARCHAR(7),  -- Hex color code
    icon VARCHAR(64),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT roles_name_unique UNIQUE (name),
    CONSTRAINT roles_check_category CHECK (category IN (
        'system', 'admin', 'moderator', 'tournament', 'team', 'player', 'custom'
    )),
    CONSTRAINT roles_check_color CHECK (color IS NULL OR color ~ '^#[0-9A-Fa-f]{6}$')
);

-- Default roles
INSERT INTO roles (id, name, display_name, category, priority, is_system, is_default) VALUES
    ('00000000-0000-0000-0000-000000000001', 'platform_admin', 'Platform Administrator', 'system', 1000, TRUE, FALSE),
    ('00000000-0000-0000-0000-000000000002', 'moderator', 'Moderator', 'moderator', 500, TRUE, FALSE),
    ('00000000-0000-0000-0000-000000000003', 'verified_player', 'Verified Player', 'player', 100, TRUE, FALSE),
    ('00000000-0000-0000-0000-000000000004', 'player', 'Player', 'player', 10, TRUE, TRUE);
```

### 5.2 permissions

Permission definitions.

```sql
CREATE TABLE permissions (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(64) NOT NULL,
    display_name VARCHAR(128) NOT NULL,
    description TEXT,
    
    -- Categorization
    category VARCHAR(32) NOT NULL,
    
    -- Resource Type (for scoped permissions)
    resource_type VARCHAR(32),
    
    -- Flags
    is_dangerous BOOLEAN NOT NULL DEFAULT FALSE,  -- Requires extra confirmation
    requires_2fa BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT permissions_name_unique UNIQUE (name),
    CONSTRAINT permissions_check_category CHECK (category IN (
        'auth', 'player', 'team', 'match', 'tournament', 
        'league', 'game', 'admin', 'moderation', 'system'
    ))
);

-- Core permissions
INSERT INTO permissions (name, display_name, category, resource_type) VALUES
    -- Auth
    ('auth.manage_sessions', 'Manage Own Sessions', 'auth', NULL),
    
    -- Player
    ('player.profile.read', 'View Player Profiles', 'player', 'player'),
    ('player.profile.update', 'Update Own Profile', 'player', 'player'),
    ('player.profile.update.any', 'Update Any Profile', 'player', 'player'),
    
    -- Team
    ('team.create', 'Create Teams', 'team', NULL),
    ('team.manage', 'Manage Own Team', 'team', 'team'),
    ('team.manage.any', 'Manage Any Team', 'team', 'team'),
    ('team.delete', 'Delete Own Team', 'team', 'team'),
    ('team.delete.any', 'Delete Any Team', 'team', 'team'),
    ('team.members.manage', 'Manage Team Members', 'team', 'team'),
    ('team.members.invite', 'Invite Team Members', 'team', 'team'),
    
    -- Match
    ('match.view', 'View Matches', 'match', NULL),
    ('match.create', 'Create Matches', 'match', NULL),
    ('match.admin', 'Administrate Matches', 'match', 'match'),
    
    -- Tournament
    ('tournament.view', 'View Tournaments', 'tournament', NULL),
    ('tournament.create', 'Create Tournaments', 'tournament', NULL),
    ('tournament.manage', 'Manage Tournaments', 'tournament', 'tournament'),
    ('tournament.admin', 'Administrate Tournaments', 'tournament', 'tournament'),
    
    -- Queue
    ('queue.join', 'Join Matchmaking Queues', 'match', NULL),
    
    -- Lobby
    ('lobby.create', 'Create Lobbies', 'match', NULL),
    ('lobby.join', 'Join Lobbies', 'match', NULL),
    ('lobby.admin', 'Administrate Lobbies', 'match', 'lobby'),
    
    -- Admin
    ('admin.users.view', 'View User Admin', 'admin', NULL),
    ('admin.users.manage', 'Manage Users', 'admin', NULL),
    ('admin.bans.manage', 'Manage Bans', 'admin', NULL),
    ('admin.games.manage', 'Manage Games', 'admin', NULL),
    ('admin.system.configure', 'System Configuration', 'admin', NULL);
```

### 5.3 role_permissions

Permission assignments to roles.

```sql
CREATE TABLE role_permissions (
    -- Composite Primary Key
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    
    -- Grant Options
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    
    -- Primary Key
    PRIMARY KEY (role_id, permission_id)
);

-- Indexes
CREATE INDEX idx_role_permissions_role ON role_permissions(role_id);
CREATE INDEX idx_role_permissions_permission ON role_permissions(permission_id);
```

### 5.4 user_roles

Role assignments to users.

```sql
CREATE TABLE user_roles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    
    -- Scope (NULL for global roles)
    scope_type VARCHAR(32),
    scope_id UUID,
    
    -- Grant Information
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason TEXT,
    
    -- Expiration
    expires_at TIMESTAMPTZ,
    
    -- Revocation
    revoked_at TIMESTAMPTZ,
    revoked_by UUID REFERENCES users(id) ON DELETE SET NULL,
    revoke_reason TEXT,
    
    -- Constraints
    CONSTRAINT user_roles_unique UNIQUE NULLS NOT DISTINCT (
        user_id, role_id, scope_type, scope_id
    ),
    CONSTRAINT user_roles_check_scope CHECK (
        (scope_type IS NULL AND scope_id IS NULL) OR
        (scope_type IS NOT NULL AND scope_id IS NOT NULL)
    ),
    CONSTRAINT user_roles_check_scope_type CHECK (scope_type IS NULL OR scope_type IN (
        'game', 'team', 'tournament', 'league', 'season'
    ))
);

-- Indexes
CREATE INDEX idx_user_roles_user ON user_roles(user_id);
CREATE INDEX idx_user_roles_role ON user_roles(role_id);
CREATE INDEX idx_user_roles_scope ON user_roles(scope_type, scope_id)
    WHERE scope_type IS NOT NULL;
CREATE INDEX idx_user_roles_active ON user_roles(user_id, role_id)
    WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > NOW());
```

---

## 6. Player & Team Tables

### 6.1 teams

Team organizations. Teams can be created by any player, who becomes the founding captain. Players may belong to multiple teams simultaneously (e.g., different teams for different games, or a main team and a casual team).

```sql
CREATE TABLE teams (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(64) NOT NULL,
    name_normalized VARCHAR(64) GENERATED ALWAYS AS (lower(name)) STORED,
    tag VARCHAR(5) NOT NULL,
    tag_normalized VARCHAR(5) GENERATED ALWAYS AS (lower(tag)) STORED,
    
    -- Profile
    description TEXT,
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    primary_color VARCHAR(7),
    secondary_color VARCHAR(7),
    
    -- Founding Captain (creator of the team, always has captain role)
    -- This is the original creator; captain privileges are managed via team_members.role
    created_by UUID NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    
    -- Game Association (NULL for multi-game teams)
    game_id VARCHAR(32) REFERENCES games(id) ON DELETE SET NULL,
    
    -- Settings
    settings JSONB DEFAULT '{
        "require_approval": true,
        "public_roster": true,
        "allow_scrim_requests": true,
        "max_roster_size": 10
    }',
    
    -- Social
    social_links JSONB DEFAULT '{}',
    website_url VARCHAR(512),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    disbanded_at TIMESTAMPTZ,
    disbanded_reason TEXT,
    
    -- Statistics
    total_matches INTEGER NOT NULL DEFAULT 0,
    total_wins INTEGER NOT NULL DEFAULT 0,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT teams_name_unique UNIQUE (name_normalized),
    CONSTRAINT teams_tag_format CHECK (tag ~ '^[a-zA-Z0-9]{2,5}$'),
    CONSTRAINT teams_check_status CHECK (status IN (
        'active', 'inactive', 'disbanded', 'suspended'
    )),
    CONSTRAINT teams_check_colors CHECK (
        (primary_color IS NULL OR primary_color ~ '^#[0-9A-Fa-f]{6}$') AND
        (secondary_color IS NULL OR secondary_color ~ '^#[0-9A-Fa-f]{6}$')
    )
);

-- Indexes
CREATE INDEX idx_teams_created_by ON teams(created_by);
CREATE INDEX idx_teams_game ON teams(game_id) WHERE game_id IS NOT NULL;
CREATE INDEX idx_teams_status ON teams(status) WHERE status = 'active';
CREATE INDEX idx_teams_name ON teams(name_normalized);
CREATE INDEX idx_teams_tag ON teams(tag_normalized);

-- Full-text search
CREATE INDEX idx_teams_search ON teams USING gin(
    to_tsvector('english', name || ' ' || tag || ' ' || COALESCE(description, ''))
);
```

### 6.2 team_members

Team roster membership. A player can be a member of multiple teams simultaneously.

**Role Hierarchy:**
- `captain`: Team administrator. Can invite/remove players, promote others to captain, manage team settings, and disband team. The team creator is automatically a captain.
- `officer`: Assistant admin. Can invite players and manage roster but cannot disband team or remove captains.
- `player`: Regular team member. Can participate in matches.
- `substitute`: Backup player. Can participate when primary players unavailable.
- `coach`: Non-playing staff. Can spectate and communicate during matches.
- `manager`: Non-playing staff. Handles team operations.

```sql
CREATE TABLE team_members (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Role (determines permissions within the team)
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    role_title VARCHAR(64),  -- Custom display title (e.g., "IGL", "Entry Fragger")
    
    -- Founder Flag (only true for the original team creator)
    is_founder BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Position (game-specific, plugin-defined)
    primary_position VARCHAR(32),
    secondary_position VARCHAR(32),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    
    -- Jersey/Number
    jersey_number INTEGER,
    
    -- Invited By
    invited_by UUID REFERENCES players(id),
    
    -- Timestamps
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT team_members_unique UNIQUE (team_id, player_id),
    CONSTRAINT team_members_check_role CHECK (role IN (
        'captain', 'officer', 'player', 'substitute', 'coach', 'manager'
    )),
    CONSTRAINT team_members_check_status CHECK (status IN (
        'active', 'inactive', 'benched', 'trial'
    )),
    CONSTRAINT team_members_check_jersey CHECK (
        jersey_number IS NULL OR (jersey_number >= 0 AND jersey_number <= 99)
    )
);

-- Indexes
CREATE INDEX idx_team_members_team ON team_members(team_id);
CREATE INDEX idx_team_members_player ON team_members(player_id);
CREATE INDEX idx_team_members_active ON team_members(team_id, status)
    WHERE status = 'active' AND left_at IS NULL;
CREATE INDEX idx_team_members_role ON team_members(team_id, role);
CREATE INDEX idx_team_members_captains ON team_members(team_id)
    WHERE role = 'captain' AND status = 'active';

-- Ensure at least one captain exists (enforced at application level, with this trigger as safety)
-- Application should prevent removing the last captain
```

### 6.3 team_invitations

Team invitation and join request management.

```sql
CREATE TABLE team_invitations (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Invitation Details
    type VARCHAR(20) NOT NULL,  -- 'invite' or 'request'
    role VARCHAR(32) NOT NULL DEFAULT 'player',
    message TEXT,
    
    -- Sender
    invited_by UUID REFERENCES players(id) ON DELETE SET NULL,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Response
    responded_at TIMESTAMPTZ,
    response_message TEXT,
    
    -- Expiration
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT team_invitations_check_type CHECK (type IN ('invite', 'request')),
    CONSTRAINT team_invitations_check_status CHECK (status IN (
        'pending', 'accepted', 'declined', 'expired', 'cancelled'
    ))
);

-- Indexes
CREATE INDEX idx_team_invitations_team ON team_invitations(team_id);
CREATE INDEX idx_team_invitations_player ON team_invitations(player_id);
CREATE INDEX idx_team_invitations_pending ON team_invitations(player_id, status)
    WHERE status = 'pending';
```

### 6.4 player_relationships

Friendships and blocks between players.

```sql
CREATE TABLE player_relationships (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Players (player_a_id < player_b_id to ensure uniqueness)
    player_a_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    player_b_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Relationship State (from player_a's perspective)
    relationship_type VARCHAR(20) NOT NULL,
    
    -- Friendship specific
    friendship_status VARCHAR(20),  -- For friend type
    
    -- Request tracking
    requested_by UUID REFERENCES players(id),
    requested_at TIMESTAMPTZ,
    responded_at TIMESTAMPTZ,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT player_relationships_unique UNIQUE (player_a_id, player_b_id),
    CONSTRAINT player_relationships_check_order CHECK (player_a_id < player_b_id),
    CONSTRAINT player_relationships_check_type CHECK (relationship_type IN (
        'friend', 'blocked'
    )),
    CONSTRAINT player_relationships_check_status CHECK (
        relationship_type != 'friend' OR 
        friendship_status IN ('pending', 'accepted')
    )
);

-- Indexes
CREATE INDEX idx_relationships_player_a ON player_relationships(player_a_id);
CREATE INDEX idx_relationships_player_b ON player_relationships(player_b_id);
CREATE INDEX idx_relationships_friends ON player_relationships(player_a_id, relationship_type)
    WHERE relationship_type = 'friend' AND friendship_status = 'accepted';
```

### 6.5 team_game_profiles

Per-game statistics for teams.

```sql
CREATE TABLE team_game_profiles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    
    -- Rating
    rating INTEGER NOT NULL DEFAULT 1500,
    rating_deviation INTEGER NOT NULL DEFAULT 350,
    peak_rating INTEGER NOT NULL DEFAULT 1500,
    
    -- Statistics
    matches_played INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    
    -- Game-Specific
    game_specific_stats JSONB DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT team_game_profiles_unique UNIQUE (team_id, game_id)
);

-- Indexes
CREATE INDEX idx_team_game_profiles_team ON team_game_profiles(team_id);
CREATE INDEX idx_team_game_profiles_rating ON team_game_profiles(game_id, rating DESC);
```

---

## 7. Game & Match Tables

### 7.1 games

Supported games registry.

```sql
CREATE TABLE games (
    -- Primary Key (slug-based)
    id VARCHAR(32) PRIMARY KEY,
    
    -- Display Information
    display_name VARCHAR(64) NOT NULL,
    short_name VARCHAR(16),
    description TEXT,
    
    -- Media
    icon_url VARCHAR(512),
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    
    -- Configuration
    config JSONB DEFAULT '{}',
    default_queue_config JSONB DEFAULT '{}',
    default_lobby_config JSONB DEFAULT '{}',
    
    -- Plugin Reference
    plugin_version VARCHAR(32),
    
    -- Matchmaking Settings
    team_size INTEGER NOT NULL DEFAULT 5,
    min_team_size INTEGER NOT NULL DEFAULT 1,
    supports_solo_queue BOOLEAN NOT NULL DEFAULT TRUE,
    supports_team_queue BOOLEAN NOT NULL DEFAULT TRUE,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    maintenance_message TEXT,
    
    -- Sorting/Display
    display_order INTEGER NOT NULL DEFAULT 0,
    is_featured BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT games_check_id CHECK (id ~ '^[a-z0-9_-]{2,32}$'),
    CONSTRAINT games_check_status CHECK (status IN (
        'active', 'maintenance', 'disabled', 'coming_soon'
    )),
    CONSTRAINT games_check_team_size CHECK (team_size >= min_team_size)
);

-- Indexes
CREATE INDEX idx_games_status ON games(status);
CREATE INDEX idx_games_display ON games(display_order, display_name);
```

### 7.2 match_queues

Matchmaking queue definitions.

```sql
CREATE TABLE match_queues (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    
    -- Identity
    name VARCHAR(64) NOT NULL,
    description TEXT,
    
    -- Queue Type
    queue_type VARCHAR(32) NOT NULL,
    
    -- Team Configuration
    team_size INTEGER NOT NULL,
    team_count INTEGER NOT NULL DEFAULT 2,
    allow_parties BOOLEAN NOT NULL DEFAULT TRUE,
    max_party_size INTEGER,
    
    -- Matchmaking Settings
    config JSONB NOT NULL DEFAULT '{}',
    -- Example: {
    --   "rating_range_initial": 100,
    --   "rating_range_expansion_rate": 50,
    --   "rating_range_max": 500,
    --   "max_wait_time_seconds": 300,
    --   "region_locked": true
    -- }
    
    -- Rank Restrictions
    min_rank_tier VARCHAR(32),
    max_rank_tier VARCHAR(32),
    min_matches_required INTEGER DEFAULT 0,
    
    -- Schedule
    schedule JSONB,  -- Time-based availability
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT match_queues_check_type CHECK (queue_type IN (
        'ranked', 'unranked', 'pug', 'competitive', 'casual'
    )),
    CONSTRAINT match_queues_check_status CHECK (status IN (
        'active', 'paused', 'disabled', 'maintenance'
    ))
);

-- Indexes
CREATE INDEX idx_match_queues_game ON match_queues(game_id);
CREATE INDEX idx_match_queues_active ON match_queues(game_id, status)
    WHERE status = 'active';
```

### 7.3 queue_entries

Players currently in matchmaking queues.

```sql
CREATE TABLE queue_entries (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    queue_id UUID NOT NULL REFERENCES match_queues(id) ON DELETE CASCADE,
    
    -- Entry Type
    entry_type VARCHAR(20) NOT NULL,  -- 'solo' or 'party'
    party_id UUID,  -- Links party members together
    party_leader_id UUID REFERENCES players(id),
    
    -- Player
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Matchmaking Data
    rating INTEGER NOT NULL,
    rating_deviation INTEGER NOT NULL,
    
    -- Preferences (game-specific)
    preferences JSONB DEFAULT '{}',
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'searching',
    
    -- Match Found
    pending_match_id UUID,
    match_accepted BOOLEAN,
    
    -- Timing
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    estimated_wait_seconds INTEGER,
    
    -- Constraints
    CONSTRAINT queue_entries_unique UNIQUE (queue_id, player_id),
    CONSTRAINT queue_entries_check_type CHECK (entry_type IN ('solo', 'party')),
    CONSTRAINT queue_entries_check_status CHECK (status IN (
        'searching', 'match_found', 'accepted', 'expired'
    ))
);

-- Indexes
CREATE INDEX idx_queue_entries_queue ON queue_entries(queue_id, status);
CREATE INDEX idx_queue_entries_player ON queue_entries(player_id);
CREATE INDEX idx_queue_entries_party ON queue_entries(party_id)
    WHERE party_id IS NOT NULL;
CREATE INDEX idx_queue_entries_searching ON queue_entries(queue_id, rating, joined_at)
    WHERE status = 'searching';
```

### 7.4 matches

Core match records.

```sql
CREATE TABLE matches (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Game
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE RESTRICT,
    
    -- Match Type & Source
    match_type VARCHAR(32) NOT NULL,
    source_type VARCHAR(32),  -- queue, tournament, scrim, custom
    source_id UUID,  -- queue_id, tournament_match_id, etc.
    
    -- Format
    format VARCHAR(32) NOT NULL DEFAULT 'bo1',
    maps_to_win INTEGER NOT NULL DEFAULT 1,
    
    -- Teams
    team_1_id UUID REFERENCES teams(id),
    team_2_id UUID REFERENCES teams(id),
    team_1_name VARCHAR(64),  -- Snapshot at match time
    team_2_name VARCHAR(64),
    
    -- Result
    winner_team_slot INTEGER,  -- 1, 2, or NULL for draw/incomplete
    team_1_score INTEGER,
    team_2_score INTEGER,
    result_type VARCHAR(32),  -- completed, forfeit, cancelled, technical
    
    -- Server Information
    server_id UUID,
    server_address VARCHAR(255),
    server_password VARCHAR(64),
    gotv_address VARCHAR(255),
    
    -- Scheduling
    scheduled_at TIMESTAMPTZ,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Timing
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    duration_seconds INTEGER,
    
    -- Match Data
    metadata JSONB DEFAULT '{}',  -- Game-specific data
    vod_url VARCHAR(512),
    stats_processed BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT matches_check_type CHECK (match_type IN (
        'ranked', 'unranked', 'pug', 'tournament', 'league', 'scrim', 'custom'
    )),
    CONSTRAINT matches_check_format CHECK (format IN (
        'bo1', 'bo3', 'bo5', 'bo7'
    )),
    CONSTRAINT matches_check_status CHECK (status IN (
        'pending', 'lobby', 'picking', 'ready', 'live', 
        'paused', 'completed', 'cancelled', 'forfeit'
    )),
    CONSTRAINT matches_check_winner CHECK (
        winner_team_slot IS NULL OR winner_team_slot IN (1, 2)
    )
);

-- Indexes
CREATE INDEX idx_matches_game ON matches(game_id);
CREATE INDEX idx_matches_status ON matches(status);
CREATE INDEX idx_matches_type ON matches(match_type);
CREATE INDEX idx_matches_teams ON matches(team_1_id, team_2_id);
CREATE INDEX idx_matches_source ON matches(source_type, source_id)
    WHERE source_type IS NOT NULL;
CREATE INDEX idx_matches_created ON matches(created_at DESC);
CREATE INDEX idx_matches_scheduled ON matches(scheduled_at)
    WHERE scheduled_at IS NOT NULL AND status = 'pending';
```

### 7.5 match_players

Player participation in matches.

```sql
CREATE TABLE match_players (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    match_id UUID NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    
    -- Team Assignment
    team_slot INTEGER NOT NULL,  -- 1 or 2
    
    -- Role/Position
    role VARCHAR(32),
    is_captain BOOLEAN NOT NULL DEFAULT FALSE,
    is_substitute BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Participation
    participation_status VARCHAR(20) NOT NULL DEFAULT 'confirmed',
    
    -- Rating Changes
    rating_before INTEGER,
    rating_after INTEGER,
    rating_change INTEGER,
    rd_before INTEGER,
    rd_after INTEGER,
    
    -- Player Stats (game-specific)
    stats JSONB DEFAULT '{}',
    
    -- Performance Metrics
    performance_rating DECIMAL(5, 2),  -- Calculated performance score
    mvp_points INTEGER DEFAULT 0,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT match_players_unique UNIQUE (match_id, player_id),
    CONSTRAINT match_players_check_slot CHECK (team_slot IN (1, 2)),
    CONSTRAINT match_players_check_status CHECK (participation_status IN (
        'confirmed', 'no_show', 'left_early', 'substituted', 'removed'
    ))
);

-- Indexes
CREATE INDEX idx_match_players_match ON match_players(match_id);
CREATE INDEX idx_match_players_player ON match_players(player_id);
CREATE INDEX idx_match_players_team ON match_players(team_id)
    WHERE team_id IS NOT NULL;
CREATE INDEX idx_match_players_history ON match_players(player_id, created_at DESC);
```

### 7.6 match_maps

Individual map results within a match.

```sql
CREATE TABLE match_maps (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    match_id UUID NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    
    -- Map Info
    map_number INTEGER NOT NULL,
    map_name VARCHAR(64) NOT NULL,
    
    -- Pick/Ban
    picked_by INTEGER,  -- Team slot that picked
    
    -- Sides
    team_1_start_side VARCHAR(10),
    team_2_start_side VARCHAR(10),
    
    -- Result
    team_1_score INTEGER,
    team_2_score INTEGER,
    winner_team_slot INTEGER,
    
    -- Overtime
    went_to_overtime BOOLEAN DEFAULT FALSE,
    overtime_team_1_score INTEGER,
    overtime_team_2_score INTEGER,
    
    -- Timing
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    duration_seconds INTEGER,
    
    -- Demo/VOD
    demo_url VARCHAR(512),
    
    -- Detailed Stats
    stats JSONB DEFAULT '{}',
    round_history JSONB DEFAULT '[]',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT match_maps_unique UNIQUE (match_id, map_number)
);

-- Indexes
CREATE INDEX idx_match_maps_match ON match_maps(match_id);
```

---

## 8. Lobby Tables

### 8.1 lobbies

Pre-match lobby sessions.

```sql
CREATE TABLE lobbies (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    game_id VARCHAR(32) NOT NULL REFERENCES games(id),
    match_id UUID REFERENCES matches(id),
    created_by UUID REFERENCES players(id),
    
    -- Configuration
    config JSONB NOT NULL,
    -- Example: {
    --   "team_size": 5,
    --   "map_pool": ["de_dust2", "de_mirage", ...],
    --   "format": "bo3",
    --   "pick_ban_format": "standard",
    --   "ready_timeout_seconds": 30
    -- }
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'waiting',
    phase VARCHAR(32) NOT NULL DEFAULT 'gathering',
    
    -- Plugin State
    plugin_state JSONB DEFAULT '{}',
    
    -- Map Selection Results
    selected_maps JSONB DEFAULT '[]',
    
    -- Access
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    join_password VARCHAR(64),
    invite_code VARCHAR(16),
    
    -- Timing
    phase_started_at TIMESTAMPTZ,
    phase_timeout_at TIMESTAMPTZ,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ,
    close_reason VARCHAR(32),
    
    -- Constraints
    CONSTRAINT lobbies_check_status CHECK (status IN (
        'waiting', 'picking', 'ready', 'starting', 'started', 'closed', 'cancelled'
    )),
    CONSTRAINT lobbies_invite_code_unique UNIQUE (invite_code)
);

-- Indexes
CREATE INDEX idx_lobbies_game ON lobbies(game_id);
CREATE INDEX idx_lobbies_match ON lobbies(match_id) WHERE match_id IS NOT NULL;
CREATE INDEX idx_lobbies_status ON lobbies(status);
CREATE INDEX idx_lobbies_invite ON lobbies(invite_code) WHERE invite_code IS NOT NULL;
CREATE INDEX idx_lobbies_public ON lobbies(game_id, created_at DESC)
    WHERE is_public = TRUE AND status = 'waiting';
```

### 8.2 lobby_players

Players in a lobby.

```sql
CREATE TABLE lobby_players (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    lobby_id UUID NOT NULL REFERENCES lobbies(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Team Assignment
    team_slot INTEGER,  -- 1, 2, or NULL for unassigned
    
    -- Roles
    is_captain BOOLEAN NOT NULL DEFAULT FALSE,
    is_host BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Status
    ready_status VARCHAR(20) NOT NULL DEFAULT 'not_ready',
    connection_status VARCHAR(20) NOT NULL DEFAULT 'connected',
    
    -- Session
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    leave_reason VARCHAR(32),
    
    -- Reconnection
    reconnect_token VARCHAR(64),
    reconnect_expires_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT lobby_players_unique UNIQUE (lobby_id, player_id),
    CONSTRAINT lobby_players_check_slot CHECK (team_slot IS NULL OR team_slot IN (1, 2)),
    CONSTRAINT lobby_players_check_ready CHECK (ready_status IN (
        'not_ready', 'ready', 'away'
    )),
    CONSTRAINT lobby_players_check_connection CHECK (connection_status IN (
        'connected', 'disconnected', 'reconnecting'
    ))
);

-- Indexes
CREATE INDEX idx_lobby_players_lobby ON lobby_players(lobby_id);
CREATE INDEX idx_lobby_players_player ON lobby_players(player_id);
CREATE INDEX idx_lobby_players_active ON lobby_players(lobby_id, connection_status)
    WHERE left_at IS NULL;
```

### 8.3 lobby_chat

Lobby chat messages.

```sql
CREATE TABLE lobby_chat (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    lobby_id UUID NOT NULL REFERENCES lobbies(id) ON DELETE CASCADE,
    player_id UUID REFERENCES players(id) ON DELETE SET NULL,
    
    -- Message
    message_type VARCHAR(20) NOT NULL DEFAULT 'chat',
    content TEXT NOT NULL,
    
    -- Metadata
    metadata JSONB DEFAULT '{}',
    
    -- Timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Moderation
    deleted_at TIMESTAMPTZ,
    deleted_by UUID REFERENCES players(id),
    
    -- Constraints
    CONSTRAINT lobby_chat_check_type CHECK (message_type IN (
        'chat', 'system', 'action', 'pick', 'ban', 'ready'
    ))
);

-- Indexes
CREATE INDEX idx_lobby_chat_lobby ON lobby_chat(lobby_id, created_at);
```

---

## 9. Tournament & League Tables

### 9.1 leagues

League organizations. Leagues are game-specific and can represent different skill divisions (e.g., Division 1, Division 2). Players must be league members before participating in league seasons or tournaments.

```sql
CREATE TABLE leagues (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(128) NOT NULL,
    slug VARCHAR(64) NOT NULL,
    description TEXT,
    
    -- Game (required - leagues are game-specific)
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    
    -- Hierarchy (for divisions within a league system)
    parent_league_id UUID REFERENCES leagues(id) ON DELETE SET NULL,
    division INTEGER,  -- 1 = top division, 2 = second, etc.
    tier_name VARCHAR(32),  -- "Premier", "Challenger", "Open", etc.
    
    -- Region (for regional divisions)
    region VARCHAR(32),  -- "na", "eu", "asia", etc.
    
    -- Access Control
    access_type VARCHAR(20) NOT NULL DEFAULT 'open',
    -- 'open': Anyone can join
    -- 'invite_only': Only invited players can join
    -- 'application': Players apply, admins approve
    
    -- Membership Requirements
    min_rating INTEGER,  -- Minimum player rating to join
    max_rating INTEGER,  -- Maximum player rating (for lower divisions)
    min_matches INTEGER DEFAULT 0,  -- Minimum matches played to join
    
    -- Media
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    
    -- Ownership & Administration
    owner_id UUID REFERENCES users(id) ON DELETE SET NULL,
    organization_name VARCHAR(128),
    
    -- Settings
    settings JSONB DEFAULT '{
        "allow_team_participation": true,
        "allow_solo_participation": false,
        "require_roster_lock": false,
        "max_teams_per_player": 1
    }',
    
    -- Map Pool (default for league tournaments, plugin-defined available maps)
    default_map_pool JSONB,  -- e.g., ["de_dust2", "de_mirage", ...]
    
    -- Contact
    contact_email VARCHAR(255),
    discord_url VARCHAR(512),
    website_url VARCHAR(512),
    rules_url VARCHAR(512),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT leagues_slug_unique UNIQUE (slug),
    CONSTRAINT leagues_check_access_type CHECK (access_type IN (
        'open', 'invite_only', 'application'
    )),
    CONSTRAINT leagues_check_status CHECK (status IN (
        'active', 'inactive', 'archived'
    )),
    CONSTRAINT leagues_check_division CHECK (division IS NULL OR division > 0),
    CONSTRAINT leagues_check_rating_range CHECK (
        min_rating IS NULL OR max_rating IS NULL OR min_rating <= max_rating
    )
);

-- Indexes
CREATE INDEX idx_leagues_game ON leagues(game_id);
CREATE INDEX idx_leagues_owner ON leagues(owner_id);
CREATE INDEX idx_leagues_slug ON leagues(slug);
CREATE INDEX idx_leagues_parent ON leagues(parent_league_id) WHERE parent_league_id IS NOT NULL;
CREATE INDEX idx_leagues_division ON leagues(game_id, division) WHERE division IS NOT NULL;
CREATE INDEX idx_leagues_region ON leagues(game_id, region) WHERE region IS NOT NULL;
```

### 9.2 league_members

Persistent league membership. Players must be league members to participate in seasons or league-specific tournaments. This is separate from season_participants which tracks per-season registration.

```sql
CREATE TABLE league_members (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Membership Type
    membership_type VARCHAR(20) NOT NULL DEFAULT 'player',
    -- 'player': Regular member
    -- 'admin': League administrator (can manage league, create tournaments)
    -- 'moderator': Can moderate but not configure league
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    status_reason TEXT,
    
    -- Standing within league
    current_rating INTEGER,  -- League-specific rating (optional)
    current_rank INTEGER,
    
    -- Join Information
    joined_via VARCHAR(20),  -- 'direct', 'invitation', 'application', 'promotion', 'relegation'
    invited_by UUID REFERENCES players(id),
    application_message TEXT,
    approved_by UUID REFERENCES users(id),
    
    -- Statistics
    seasons_participated INTEGER NOT NULL DEFAULT 0,
    
    -- Timestamps
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT league_members_unique UNIQUE (league_id, player_id),
    CONSTRAINT league_members_check_type CHECK (membership_type IN (
        'player', 'admin', 'moderator'
    )),
    CONSTRAINT league_members_check_status CHECK (status IN (
        'pending', 'active', 'suspended', 'left', 'removed', 'relegated', 'promoted'
    ))
);

-- Indexes
CREATE INDEX idx_league_members_league ON league_members(league_id);
CREATE INDEX idx_league_members_player ON league_members(player_id);
CREATE INDEX idx_league_members_active ON league_members(league_id, status)
    WHERE status = 'active';
CREATE INDEX idx_league_members_admins ON league_members(league_id, membership_type)
    WHERE membership_type IN ('admin', 'moderator');
CREATE INDEX idx_league_members_rating ON league_members(league_id, current_rating DESC)
    WHERE status = 'active';
```

### 9.3 league_invitations

Invitations for invite-only leagues and applications for application-based leagues.

```sql
CREATE TABLE league_invitations (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Type
    type VARCHAR(20) NOT NULL,  -- 'invitation' or 'application'
    
    -- Sender (for invitations)
    invited_by UUID REFERENCES users(id),
    
    -- Application Details
    message TEXT,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Response
    responded_at TIMESTAMPTZ,
    responded_by UUID REFERENCES users(id),  -- Admin who approved/rejected
    response_message TEXT,
    
    -- Expiration
    expires_at TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '14 days'),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT league_invitations_check_type CHECK (type IN ('invitation', 'application')),
    CONSTRAINT league_invitations_check_status CHECK (status IN (
        'pending', 'accepted', 'declined', 'expired', 'cancelled'
    ))
);

-- Indexes
CREATE INDEX idx_league_invitations_league ON league_invitations(league_id);
CREATE INDEX idx_league_invitations_player ON league_invitations(player_id);
CREATE INDEX idx_league_invitations_pending ON league_invitations(league_id, status)
    WHERE status = 'pending';
```

### 9.4 seasons

League seasons.

```sql
CREATE TABLE seasons (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    
    -- Identity
    name VARCHAR(128) NOT NULL,
    number INTEGER,  -- Season 1, Season 2, etc.
    
    -- Format
    format VARCHAR(32) NOT NULL,
    format_config JSONB DEFAULT '{}',
    
    -- Schedule
    registration_opens_at TIMESTAMPTZ,
    registration_closes_at TIMESTAMPTZ,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    playoffs_start_at TIMESTAMPTZ,
    
    -- Participation
    min_participants INTEGER,
    max_participants INTEGER,
    participant_count INTEGER NOT NULL DEFAULT 0,
    
    -- Prizes
    prize_pool JSONB,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'upcoming',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT seasons_check_format CHECK (format IN (
        'round_robin', 'swiss', 'ladder', 'weekly_matches'
    )),
    CONSTRAINT seasons_check_status CHECK (status IN (
        'upcoming', 'registration', 'active', 'playoffs', 'completed', 'cancelled'
    )),
    CONSTRAINT seasons_check_dates CHECK (starts_at < ends_at)
);

-- Indexes
CREATE INDEX idx_seasons_league ON seasons(league_id);
CREATE INDEX idx_seasons_status ON seasons(status);
CREATE INDEX idx_seasons_dates ON seasons(starts_at, ends_at);
```

### 9.5 season_participants

Season registrations.

```sql
CREATE TABLE season_participants (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    participant_type VARCHAR(20) NOT NULL,
    participant_id UUID NOT NULL,  -- player_id or team_id
    
    -- Registration
    registered_by UUID REFERENCES users(id),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'registered',
    
    -- Division/Group
    division VARCHAR(32),
    group_name VARCHAR(32),
    seed INTEGER,
    
    -- Checks
    checked_in BOOLEAN NOT NULL DEFAULT FALSE,
    checked_in_at TIMESTAMPTZ,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT season_participants_unique UNIQUE (season_id, participant_id),
    CONSTRAINT season_participants_check_type CHECK (participant_type IN ('player', 'team')),
    CONSTRAINT season_participants_check_status CHECK (status IN (
        'pending', 'registered', 'confirmed', 'withdrawn', 'disqualified'
    ))
);

-- Indexes
CREATE INDEX idx_season_participants_season ON season_participants(season_id);
CREATE INDEX idx_season_participants_participant ON season_participants(participant_id);
```

### 9.6 season_standings

Current standings within a season.

```sql
CREATE TABLE season_standings (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    participant_id UUID NOT NULL REFERENCES season_participants(id) ON DELETE CASCADE,
    
    -- Standing
    rank INTEGER,
    previous_rank INTEGER,
    
    -- Points
    points INTEGER NOT NULL DEFAULT 0,
    
    -- Match Record
    matches_played INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    
    -- Map Record (for tiebreakers)
    maps_won INTEGER NOT NULL DEFAULT 0,
    maps_lost INTEGER NOT NULL DEFAULT 0,
    map_differential INTEGER GENERATED ALWAYS AS (maps_won - maps_lost) STORED,
    
    -- Round Record (for tiebreakers)
    rounds_won INTEGER NOT NULL DEFAULT 0,
    rounds_lost INTEGER NOT NULL DEFAULT 0,
    round_differential INTEGER GENERATED ALWAYS AS (rounds_won - rounds_lost) STORED,
    
    -- Head-to-head tiebreaker data
    h2h_data JSONB DEFAULT '{}',
    
    -- Streak
    current_streak INTEGER NOT NULL DEFAULT 0,  -- Positive = wins, negative = losses
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT season_standings_unique UNIQUE (season_id, participant_id),
    CONSTRAINT season_standings_check_status CHECK (status IN (
        'active', 'eliminated', 'qualified', 'champion'
    ))
);

-- Indexes
CREATE INDEX idx_standings_season ON season_standings(season_id);
CREATE INDEX idx_standings_rank ON season_standings(season_id, rank);
CREATE INDEX idx_standings_points ON season_standings(season_id, points DESC, map_differential DESC);
```

### 9.7 tournaments

Tournament definitions. Tournaments can be:
- **Global**: Created by platform admins, open to all players (league_id = NULL)
- **League-specific**: Created by league admins, restricted to league members (league_id NOT NULL)

Multiple tournaments can run concurrently.

```sql
CREATE TABLE tournaments (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    game_id VARCHAR(32) NOT NULL REFERENCES games(id),
    
    -- League Association (NULL = global/open tournament)
    league_id UUID REFERENCES leagues(id) ON DELETE SET NULL,
    season_id UUID REFERENCES seasons(id) ON DELETE SET NULL,
    
    -- Creator
    created_by UUID NOT NULL REFERENCES users(id),
    
    -- Identity
    name VARCHAR(128) NOT NULL,
    slug VARCHAR(64),
    description TEXT,
    
    -- Media
    logo_url VARCHAR(512),
    banner_url VARCHAR(512),
    
    -- Format
    format VARCHAR(32) NOT NULL,
    format_config JSONB DEFAULT '{}',
    participant_type VARCHAR(20) NOT NULL DEFAULT 'team',
    team_size INTEGER,
    
    -- Match Settings
    default_match_format VARCHAR(10) NOT NULL DEFAULT 'bo3',
    match_settings JSONB DEFAULT '{}',
    
    -- Map Pool (plugin provides available maps, admin can customize)
    -- NULL = use game's default map pool or league's default_map_pool
    map_pool JSONB,  -- e.g., ["de_dust2", "de_mirage", "de_inferno"]
    map_pick_ban_format VARCHAR(32) DEFAULT 'standard',
    
    -- Participation
    min_participants INTEGER NOT NULL DEFAULT 2,
    max_participants INTEGER,
    current_participants INTEGER NOT NULL DEFAULT 0,
    
    -- Eligibility (for league tournaments)
    require_league_membership BOOLEAN GENERATED ALWAYS AS (league_id IS NOT NULL) STORED,
    min_player_rating INTEGER,
    max_player_rating INTEGER,
    
    -- Schedule
    registration_opens_at TIMESTAMPTZ,
    registration_closes_at TIMESTAMPTZ,
    check_in_opens_at TIMESTAMPTZ,
    check_in_closes_at TIMESTAMPTZ,
    starts_at TIMESTAMPTZ NOT NULL,
    
    -- Seeding
    seeding_method VARCHAR(32) DEFAULT 'rating',
    
    -- Prizes
    prize_pool JSONB,
    entry_fee JSONB,
    
    -- Rules
    rules_url VARCHAR(512),
    rules_text TEXT,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'draft',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT tournaments_check_format CHECK (format IN (
        'single_elimination', 'double_elimination', 
        'round_robin', 'swiss', 'group_stage'
    )),
    CONSTRAINT tournaments_check_status CHECK (status IN (
        'draft', 'published', 'registration', 'check_in',
        'seeding', 'active', 'playoffs', 'completed', 'cancelled'
    )),
    CONSTRAINT tournaments_check_pick_ban CHECK (map_pick_ban_format IN (
        'standard', 'bo1_veto', 'bo3_veto', 'random', 'preset', 'captain_draft'
    ))
);

-- Indexes
CREATE INDEX idx_tournaments_game ON tournaments(game_id);
CREATE INDEX idx_tournaments_league ON tournaments(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX idx_tournaments_season ON tournaments(season_id) WHERE season_id IS NOT NULL;
CREATE INDEX idx_tournaments_global ON tournaments(game_id, status) WHERE league_id IS NULL;
CREATE INDEX idx_tournaments_status ON tournaments(status);
CREATE INDEX idx_tournaments_starts ON tournaments(starts_at) WHERE status IN ('published', 'registration');
CREATE INDEX idx_tournaments_created_by ON tournaments(created_by);
```

### 9.8 tournament_participants

Tournament registrations.

```sql
CREATE TABLE tournament_participants (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    participant_type VARCHAR(20) NOT NULL,
    participant_id UUID NOT NULL,
    
    -- Registration
    registered_by UUID REFERENCES users(id),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Seeding
    seed INTEGER,
    initial_seed INTEGER,
    
    -- Check-in
    checked_in BOOLEAN NOT NULL DEFAULT FALSE,
    checked_in_at TIMESTAMPTZ,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'registered',
    placement INTEGER,  -- Final placement
    
    -- Elimination tracking
    eliminated_at TIMESTAMPTZ,
    eliminated_by_match_id UUID,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT tournament_participants_unique UNIQUE (tournament_id, participant_id),
    CONSTRAINT tournament_participants_check_type CHECK (participant_type IN ('player', 'team')),
    CONSTRAINT tournament_participants_check_status CHECK (status IN (
        'pending', 'registered', 'confirmed', 'checked_in',
        'active', 'eliminated', 'withdrawn', 'disqualified', 'winner'
    ))
);

-- Indexes
CREATE INDEX idx_tournament_participants_tournament ON tournament_participants(tournament_id);
CREATE INDEX idx_tournament_participants_participant ON tournament_participants(participant_id);
CREATE INDEX idx_tournament_participants_seed ON tournament_participants(tournament_id, seed);
```

### 9.9 brackets

Tournament bracket structures.

```sql
CREATE TABLE brackets (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    tournament_id UUID NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    
    -- Identity
    bracket_type VARCHAR(32) NOT NULL,
    name VARCHAR(64),
    
    -- Structure
    structure JSONB NOT NULL,  -- Bracket structure definition
    total_rounds INTEGER NOT NULL,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT brackets_check_type CHECK (bracket_type IN (
        'winners', 'losers', 'grand_final', 'groups', 'swiss'
    )),
    CONSTRAINT brackets_check_status CHECK (status IN (
        'pending', 'active', 'completed'
    ))
);

-- Indexes
CREATE INDEX idx_brackets_tournament ON brackets(tournament_id);
```

### 9.10 bracket_matches

Individual matches within brackets.

```sql
CREATE TABLE bracket_matches (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    bracket_id UUID NOT NULL REFERENCES brackets(id) ON DELETE CASCADE,
    match_id UUID REFERENCES matches(id) ON DELETE SET NULL,
    
    -- Position
    round INTEGER NOT NULL,
    position INTEGER NOT NULL,
    match_number INTEGER,  -- Display number
    
    -- Participants (set when advanced to this match)
    participant_1_id UUID,
    participant_1_seed INTEGER,
    participant_1_from_match_id UUID REFERENCES bracket_matches(id),
    participant_1_is_loser BOOLEAN DEFAULT FALSE,  -- For double elim
    
    participant_2_id UUID,
    participant_2_seed INTEGER,
    participant_2_from_match_id UUID REFERENCES bracket_matches(id),
    participant_2_is_loser BOOLEAN DEFAULT FALSE,
    
    -- Result
    winner_id UUID,
    loser_id UUID,
    participant_1_score INTEGER,
    participant_2_score INTEGER,
    
    -- Format
    match_format VARCHAR(10),
    
    -- Advancement
    winner_advances_to_match_id UUID REFERENCES bracket_matches(id),
    loser_advances_to_match_id UUID REFERENCES bracket_matches(id),
    
    -- Scheduling
    scheduled_at TIMESTAMPTZ,
    estimated_start_at TIMESTAMPTZ,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT bracket_matches_unique UNIQUE (bracket_id, round, position),
    CONSTRAINT bracket_matches_check_status CHECK (status IN (
        'pending', 'ready', 'live', 'completed', 'bye', 'forfeit'
    ))
);

-- Indexes
CREATE INDEX idx_bracket_matches_bracket ON bracket_matches(bracket_id);
CREATE INDEX idx_bracket_matches_match ON bracket_matches(match_id) WHERE match_id IS NOT NULL;
CREATE INDEX idx_bracket_matches_round ON bracket_matches(bracket_id, round);
CREATE INDEX idx_bracket_matches_scheduled ON bracket_matches(scheduled_at)
    WHERE status IN ('pending', 'ready');
```

---

## 10. Substitute System Tables

### 10.1 league_substitute_pool

Players available as substitutes for leagues.

```sql
CREATE TABLE league_substitute_pool (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Preferences
    preferred_roles JSONB DEFAULT '[]',
    preferred_positions JSONB DEFAULT '[]',
    min_notice_hours INTEGER NOT NULL DEFAULT 24,
    max_games_per_week INTEGER,
    preferred_days JSONB DEFAULT '[]',  -- ["monday", "wednesday", "friday"]
    
    -- Compensation Preferences
    compensation_required BOOLEAN DEFAULT FALSE,
    compensation_notes TEXT,
    
    -- Skill Information
    skill_rating INTEGER,
    rank_tier VARCHAR(32),
    
    -- Verification
    verified_by UUID REFERENCES users(id),
    verified_at TIMESTAMPTZ,
    verification_notes TEXT,
    
    -- Track Record
    games_played_as_sub INTEGER NOT NULL DEFAULT 0,
    games_no_show INTEGER NOT NULL DEFAULT 0,
    reliability_score DECIMAL(3, 2) NOT NULL DEFAULT 1.00,
    average_rating DECIMAL(3, 2),  -- Average rating from teams
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    status_reason TEXT,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT league_sub_pool_unique UNIQUE (season_id, player_id),
    CONSTRAINT league_sub_pool_check_reliability CHECK (
        reliability_score >= 0 AND reliability_score <= 1
    ),
    CONSTRAINT league_sub_pool_check_status CHECK (status IN (
        'active', 'inactive', 'suspended', 'banned'
    ))
);

-- Indexes
CREATE INDEX idx_sub_pool_league ON league_substitute_pool(league_id);
CREATE INDEX idx_sub_pool_season ON league_substitute_pool(season_id);
CREATE INDEX idx_sub_pool_player ON league_substitute_pool(player_id);
CREATE INDEX idx_sub_pool_active ON league_substitute_pool(season_id, status, reliability_score DESC)
    WHERE status = 'active';
```

### 10.2 substitute_availability

Availability windows for substitutes.

```sql
CREATE TABLE substitute_availability (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    pool_entry_id UUID NOT NULL REFERENCES league_substitute_pool(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Time Window
    available_date DATE NOT NULL,
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    timezone VARCHAR(64) NOT NULL,
    
    -- UTC Conversion (for queries)
    start_utc TIMESTAMPTZ NOT NULL,
    end_utc TIMESTAMPTZ NOT NULL,
    
    -- Recurrence
    recurrence_rule VARCHAR(255),  -- iCal RRULE
    recurrence_parent_id UUID REFERENCES substitute_availability(id),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'available',
    
    -- Booking
    booked_for_request_id UUID,
    booked_at TIMESTAMPTZ,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT sub_availability_check_times CHECK (start_time < end_time),
    CONSTRAINT sub_availability_check_status CHECK (status IN (
        'available', 'tentative', 'booked', 'cancelled'
    ))
);

-- Indexes
CREATE INDEX idx_sub_avail_pool ON substitute_availability(pool_entry_id);
CREATE INDEX idx_sub_avail_player ON substitute_availability(player_id);
CREATE INDEX idx_sub_avail_date ON substitute_availability(available_date, status)
    WHERE status = 'available';
CREATE INDEX idx_sub_avail_utc ON substitute_availability(start_utc, end_utc)
    WHERE status = 'available';
```

### 10.3 substitute_requests

Requests for substitutes from teams.

```sql
CREATE TABLE substitute_requests (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    match_id UUID REFERENCES matches(id) ON DELETE SET NULL,
    bracket_match_id UUID REFERENCES bracket_matches(id) ON DELETE SET NULL,
    
    -- Requester
    requested_by UUID NOT NULL REFERENCES players(id),
    
    -- Details
    reason TEXT,
    urgency VARCHAR(20) NOT NULL DEFAULT 'normal',
    
    -- Requirements
    required_roles JSONB DEFAULT '[]',
    preferred_positions JSONB DEFAULT '[]',
    min_skill_rating INTEGER,
    max_skill_rating INTEGER,
    specific_requirements TEXT,
    
    -- Match Timing
    match_scheduled_at TIMESTAMPTZ NOT NULL,
    response_deadline TIMESTAMPTZ NOT NULL,
    
    -- Replacing
    replacing_player_id UUID REFERENCES players(id),
    
    -- Offer
    compensation_offered TEXT,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'open',
    
    -- Resolution
    filled_by UUID REFERENCES players(id),
    filled_at TIMESTAMPTZ,
    cancelled_reason TEXT,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT sub_requests_check_urgency CHECK (urgency IN (
        'low', 'normal', 'high', 'emergency'
    )),
    CONSTRAINT sub_requests_check_status CHECK (status IN (
        'open', 'pending_response', 'filled', 'cancelled', 'expired'
    ))
);

-- Indexes
CREATE INDEX idx_sub_requests_team ON substitute_requests(team_id);
CREATE INDEX idx_sub_requests_season ON substitute_requests(season_id);
CREATE INDEX idx_sub_requests_status ON substitute_requests(status, match_scheduled_at)
    WHERE status = 'open';
CREATE INDEX idx_sub_requests_match ON substitute_requests(match_id)
    WHERE match_id IS NOT NULL;
```

### 10.4 substitute_request_responses

Responses to substitute requests.

```sql
CREATE TABLE substitute_request_responses (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    request_id UUID NOT NULL REFERENCES substitute_requests(id) ON DELETE CASCADE,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    
    -- Response
    response_type VARCHAR(20) NOT NULL,
    message TEXT,
    
    -- Player's availability for this match
    confirmed_availability BOOLEAN,
    
    -- Team Decision
    team_decision VARCHAR(20),
    decision_by UUID REFERENCES players(id),
    decision_at TIMESTAMPTZ,
    decision_notes TEXT,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT sub_responses_unique UNIQUE (request_id, player_id),
    CONSTRAINT sub_responses_check_type CHECK (response_type IN (
        'interested', 'declined', 'maybe'
    )),
    CONSTRAINT sub_responses_check_decision CHECK (team_decision IS NULL OR team_decision IN (
        'pending', 'accepted', 'rejected', 'waitlisted'
    ))
);

-- Indexes
CREATE INDEX idx_sub_responses_request ON substitute_request_responses(request_id);
CREATE INDEX idx_sub_responses_player ON substitute_request_responses(player_id);
```

### 10.5 substitute_assignments

Completed substitute assignments.

```sql
CREATE TABLE substitute_assignments (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    request_id UUID REFERENCES substitute_requests(id) ON DELETE SET NULL,
    match_id UUID NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id),
    
    -- Players
    original_player_id UUID REFERENCES players(id),
    substitute_player_id UUID NOT NULL REFERENCES players(id),
    
    -- Assignment
    assigned_by UUID REFERENCES players(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Outcome
    substitute_participated BOOLEAN,
    participation_status VARCHAR(20),
    
    -- Feedback
    team_rating INTEGER,  -- 1-5
    team_feedback TEXT,
    team_feedback_at TIMESTAMPTZ,
    
    substitute_rating INTEGER,  -- 1-5
    substitute_feedback TEXT,
    substitute_feedback_at TIMESTAMPTZ,
    
    -- Compensation
    compensation_provided TEXT,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT sub_assignments_check_ratings CHECK (
        (team_rating IS NULL OR (team_rating >= 1 AND team_rating <= 5)) AND
        (substitute_rating IS NULL OR (substitute_rating >= 1 AND substitute_rating <= 5))
    )
);

-- Indexes
CREATE INDEX idx_sub_assignments_match ON substitute_assignments(match_id);
CREATE INDEX idx_sub_assignments_sub ON substitute_assignments(substitute_player_id);
CREATE INDEX idx_sub_assignments_request ON substitute_assignments(request_id)
    WHERE request_id IS NOT NULL;
```

---

## 11. Game Server Tables

### 11.1 game_servers

Registered game servers.

```sql
CREATE TABLE game_servers (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(128) NOT NULL,
    hostname VARCHAR(255),
    
    -- Connection
    ip_address INET NOT NULL,
    port INTEGER NOT NULL,
    rcon_port INTEGER,
    gotv_port INTEGER,
    
    -- Authentication
    rcon_password_encrypted TEXT,  -- Encrypted at rest
    server_token VARCHAR(255),
    
    -- Provider
    provider VARCHAR(32),  -- dathost, aws, self_hosted, etc.
    provider_server_id VARCHAR(255),
    
    -- Game
    game_id VARCHAR(32) NOT NULL REFERENCES games(id),
    
    -- Capabilities
    adapter_type VARCHAR(32) NOT NULL,  -- get5, ebot, custom
    max_players INTEGER,
    tickrate INTEGER,
    mods_available JSONB DEFAULT '[]',
    
    -- Location
    region VARCHAR(32) NOT NULL,
    datacenter VARCHAR(64),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'available',
    current_match_id UUID REFERENCES matches(id),
    
    -- Health
    last_health_check_at TIMESTAMPTZ,
    health_status VARCHAR(20),
    consecutive_failures INTEGER DEFAULT 0,
    
    -- Specs
    specs JSONB DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT game_servers_check_port CHECK (port > 0 AND port < 65536),
    CONSTRAINT game_servers_check_status CHECK (status IN (
        'available', 'reserved', 'configuring', 'in_match',
        'maintenance', 'offline', 'error'
    )),
    CONSTRAINT game_servers_check_health CHECK (health_status IS NULL OR health_status IN (
        'healthy', 'degraded', 'unhealthy', 'unknown'
    ))
);

-- Indexes
CREATE INDEX idx_game_servers_game ON game_servers(game_id);
CREATE INDEX idx_game_servers_status ON game_servers(status);
CREATE INDEX idx_game_servers_region ON game_servers(region, status);
CREATE INDEX idx_game_servers_available ON game_servers(game_id, region, status)
    WHERE status = 'available';
CREATE INDEX idx_game_servers_match ON game_servers(current_match_id)
    WHERE current_match_id IS NOT NULL;
```

### 11.2 server_reservations

Active server reservations for matches.

```sql
CREATE TABLE server_reservations (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    server_id UUID NOT NULL REFERENCES game_servers(id) ON DELETE CASCADE,
    match_id UUID NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    
    -- Reservation Details
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reserved_by VARCHAR(32),  -- 'matchmaking', 'tournament', 'manual'
    
    -- Connection Info (generated for this reservation)
    connect_password VARCHAR(64),
    gotv_password VARCHAR(64),
    
    -- Configuration Sent
    config_sent_at TIMESTAMPTZ,
    config_acknowledged_at TIMESTAMPTZ,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'reserved',
    
    -- Timing
    expected_start_at TIMESTAMPTZ,
    actual_start_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    release_reason VARCHAR(32),
    
    -- Constraints
    CONSTRAINT server_reservations_unique UNIQUE (server_id, match_id),
    CONSTRAINT server_reservations_check_status CHECK (status IN (
        'reserved', 'configuring', 'ready', 'active', 'completed', 'failed', 'released'
    ))
);

-- Indexes
CREATE INDEX idx_server_reservations_server ON server_reservations(server_id);
CREATE INDEX idx_server_reservations_match ON server_reservations(match_id);
CREATE INDEX idx_server_reservations_active ON server_reservations(server_id, status)
    WHERE status IN ('reserved', 'configuring', 'ready', 'active');
```

### 11.3 server_events

Events received from game servers.

```sql
CREATE TABLE server_events (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    server_id UUID REFERENCES game_servers(id) ON DELETE SET NULL,
    match_id UUID REFERENCES matches(id) ON DELETE SET NULL,
    
    -- Event Info
    event_type VARCHAR(64) NOT NULL,
    event_source VARCHAR(32),  -- get5, server_log, rcon, etc.
    
    -- Payload
    payload JSONB NOT NULL,
    
    -- Processing
    processed BOOLEAN NOT NULL DEFAULT FALSE,
    processed_at TIMESTAMPTZ,
    processing_error TEXT,
    
    -- Timestamp
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_timestamp TIMESTAMPTZ,  -- Timestamp from server
    
    -- Constraints
    CONSTRAINT server_events_check_type CHECK (event_type IN (
        'match_loaded', 'match_started', 'match_ended',
        'map_loaded', 'map_started', 'map_ended',
        'round_start', 'round_end',
        'player_connect', 'player_disconnect',
        'player_death', 'player_stats',
        'team_ready', 'going_live',
        'backup_loaded', 'demo_finished',
        'server_error', 'server_crash'
    ))
);

-- Indexes
CREATE INDEX idx_server_events_server ON server_events(server_id);
CREATE INDEX idx_server_events_match ON server_events(match_id);
CREATE INDEX idx_server_events_type ON server_events(event_type, received_at DESC);
CREATE INDEX idx_server_events_unprocessed ON server_events(received_at)
    WHERE processed = FALSE;

-- Partitioning for time-series data
-- Consider partitioning by month for large deployments
```

### 11.4 server_configurations

Saved server configuration templates.

```sql
CREATE TABLE server_configurations (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Identity
    name VARCHAR(128) NOT NULL,
    description TEXT,
    
    -- Scope
    game_id VARCHAR(32) NOT NULL REFERENCES games(id),
    scope_type VARCHAR(32),  -- NULL for global, 'league', 'tournament'
    scope_id UUID,
    
    -- Configuration
    config JSONB NOT NULL,
    
    -- Usage
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Ownership
    created_by UUID REFERENCES users(id),
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_server_configs_game ON server_configurations(game_id);
CREATE INDEX idx_server_configs_scope ON server_configurations(scope_type, scope_id)
    WHERE scope_type IS NOT NULL;
```

---

## 12. Plugin Data Tables

### 12.1 plugin_data

Generic plugin data storage.

```sql
CREATE TABLE plugin_data (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Scope
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    entity_type VARCHAR(64) NOT NULL,
    entity_id UUID NOT NULL,
    
    -- Data
    data JSONB NOT NULL,
    
    -- Schema Version
    schema_version INTEGER NOT NULL DEFAULT 1,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT plugin_data_unique UNIQUE (game_id, entity_type, entity_id)
);

-- Indexes
CREATE INDEX idx_plugin_data_lookup ON plugin_data(game_id, entity_type, entity_id);
CREATE INDEX idx_plugin_data_entity ON plugin_data(entity_type, entity_id);

-- GIN index for JSONB queries
CREATE INDEX idx_plugin_data_data ON plugin_data USING gin(data);
```

### 12.2 plugin_settings

Plugin configuration settings.

```sql
CREATE TABLE plugin_settings (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Scope
    game_id VARCHAR(32) NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    setting_key VARCHAR(128) NOT NULL,
    
    -- Value
    setting_value JSONB NOT NULL,
    
    -- Metadata
    description TEXT,
    is_secret BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT plugin_settings_unique UNIQUE (game_id, setting_key)
);

-- Indexes
CREATE INDEX idx_plugin_settings_game ON plugin_settings(game_id);
```

---

## 13. Saga & Workflow Tables

### 13.1 sagas

Saga state tracking for distributed transactions.

```sql
CREATE TABLE sagas (
    -- Primary Key
    id UUID PRIMARY KEY,
    
    -- Identity
    saga_type VARCHAR(64) NOT NULL,
    correlation_id VARCHAR(128),  -- For linking related operations
    
    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    current_step INTEGER NOT NULL DEFAULT 0,
    total_steps INTEGER,
    
    -- Context
    context JSONB NOT NULL,
    
    -- Initiator
    initiated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    
    -- Error Handling
    error_message TEXT,
    error_step INTEGER,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    
    -- Timing
    timeout_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT sagas_check_status CHECK (status IN (
        'pending', 'running', 'compensating', 
        'completed', 'failed', 'compensation_failed', 'timed_out'
    ))
);

-- Indexes
CREATE INDEX idx_sagas_status ON sagas(status);
CREATE INDEX idx_sagas_type_status ON sagas(saga_type, status);
CREATE INDEX idx_sagas_correlation ON sagas(correlation_id) 
    WHERE correlation_id IS NOT NULL;
CREATE INDEX idx_sagas_recovery ON sagas(status, updated_at)
    WHERE status IN ('running', 'compensating');
```

### 13.2 saga_steps

Individual step execution tracking.

```sql
CREATE TABLE saga_steps (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Relationships
    saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
    
    -- Step Info
    step_name VARCHAR(64) NOT NULL,
    step_index INTEGER NOT NULL,
    action_type VARCHAR(16) NOT NULL,  -- 'execute' or 'compensate'
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    
    -- Data
    input JSONB,
    output JSONB,
    error TEXT,
    
    -- Timing
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    duration_ms INTEGER,
    
    -- Constraints
    CONSTRAINT saga_steps_check_action CHECK (action_type IN ('execute', 'compensate')),
    CONSTRAINT saga_steps_check_status CHECK (status IN (
        'pending', 'running', 'completed', 'failed', 'skipped'
    ))
);

-- Indexes
CREATE INDEX idx_saga_steps_saga ON saga_steps(saga_id);
CREATE INDEX idx_saga_steps_order ON saga_steps(saga_id, step_index);
```

---

## 14. Audit & System Tables

### 14.1 audit_logs

Security and administrative action audit trail.

```sql
CREATE TABLE audit_logs (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Request Context
    request_id VARCHAR(64),
    trace_id VARCHAR(64),
    
    -- Actor
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_type VARCHAR(20) NOT NULL DEFAULT 'user',
    impersonated_by UUID REFERENCES users(id),
    
    -- Action
    action VARCHAR(64) NOT NULL,
    action_category VARCHAR(32) NOT NULL,
    
    -- Resource
    resource_type VARCHAR(64),
    resource_id UUID,
    resource_name VARCHAR(255),
    
    -- Request Details
    ip_address INET,
    user_agent TEXT,
    request_method VARCHAR(10),
    request_path VARCHAR(512),
    
    -- Change Data
    changes JSONB,  -- {field: {old: x, new: y}}
    
    -- Outcome
    outcome VARCHAR(20) NOT NULL,
    failure_reason TEXT,
    
    -- Timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT audit_logs_check_actor CHECK (actor_type IN (
        'user', 'system', 'api_key', 'webhook'
    )),
    CONSTRAINT audit_logs_check_category CHECK (action_category IN (
        'auth', 'user', 'team', 'match', 'tournament', 
        'admin', 'moderation', 'system', 'billing'
    )),
    CONSTRAINT audit_logs_check_outcome CHECK (outcome IN (
        'success', 'failure', 'error', 'denied'
    ))
);

-- Indexes
CREATE INDEX idx_audit_logs_user ON audit_logs(user_id, created_at DESC);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id, created_at DESC);
CREATE INDEX idx_audit_logs_action ON audit_logs(action, created_at DESC);
CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);

-- Partitioning recommendation: Partition by month
-- CREATE TABLE audit_logs_2024_01 PARTITION OF audit_logs
--     FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');
```

### 14.2 notifications

User notifications.

```sql
CREATE TABLE notifications (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Recipient
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Content
    type VARCHAR(64) NOT NULL,
    title VARCHAR(255) NOT NULL,
    body TEXT,
    
    -- Link
    action_url VARCHAR(512),
    action_text VARCHAR(64),
    
    -- Related Entity
    entity_type VARCHAR(64),
    entity_id UUID,
    
    -- Priority
    priority VARCHAR(20) NOT NULL DEFAULT 'normal',
    
    -- Status
    read_at TIMESTAMPTZ,
    dismissed_at TIMESTAMPTZ,
    
    -- Delivery
    channels JSONB DEFAULT '["in_app"]',  -- in_app, email, push, discord
    delivered_channels JSONB DEFAULT '[]',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    
    -- Constraints
    CONSTRAINT notifications_check_priority CHECK (priority IN (
        'low', 'normal', 'high', 'urgent'
    ))
);

-- Indexes
CREATE INDEX idx_notifications_user ON notifications(user_id, created_at DESC);
CREATE INDEX idx_notifications_unread ON notifications(user_id, read_at)
    WHERE read_at IS NULL;
CREATE INDEX idx_notifications_type ON notifications(type, created_at DESC);
```

### 14.3 system_settings

Global system configuration.

```sql
CREATE TABLE system_settings (
    -- Primary Key
    key VARCHAR(128) PRIMARY KEY,
    
    -- Value
    value JSONB NOT NULL,
    
    -- Metadata
    description TEXT,
    category VARCHAR(32),
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Timestamps
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id)
);

-- Seed default settings
INSERT INTO system_settings (key, value, category, description) VALUES
    ('maintenance_mode', 'false', 'system', 'Enable maintenance mode'),
    ('registration_enabled', 'true', 'auth', 'Allow new user registrations'),
    ('default_queue_enabled', 'true', 'matchmaking', 'Enable default matchmaking queues'),
    ('max_teams_per_user', '5', 'limits', 'Maximum teams a user can own');
```

---

## 15. Indexes & Performance

### 15.1 Composite Indexes for Common Queries

```sql
-- Player match history with game filter
CREATE INDEX idx_match_players_history_game ON match_players(player_id, created_at DESC)
    INCLUDE (match_id, team_slot, rating_change);

-- Team roster with active members
CREATE INDEX idx_team_members_roster ON team_members(team_id, role, status)
    INCLUDE (player_id, joined_at)
    WHERE status = 'active' AND left_at IS NULL;

-- Upcoming scheduled matches
CREATE INDEX idx_matches_upcoming ON matches(game_id, scheduled_at)
    INCLUDE (status, team_1_id, team_2_id)
    WHERE status IN ('pending', 'lobby', 'ready') 
    AND scheduled_at IS NOT NULL;

-- Leaderboard queries
CREATE INDEX idx_player_game_profiles_leaderboard ON player_game_profiles(game_id, rating DESC)
    INCLUDE (player_id, rank_tier, wins, losses)
    WHERE matches_played >= 10;

-- Active lobbies for a game
CREATE INDEX idx_lobbies_active ON lobbies(game_id, status, created_at DESC)
    INCLUDE (id)
    WHERE status NOT IN ('closed', 'cancelled');
```

### 15.2 Partial Indexes

```sql
-- Only active bans (improves ban check performance)
CREATE INDEX idx_bans_active_check ON bans(user_id, ban_type)
    WHERE lifted_at IS NULL AND (ends_at IS NULL OR ends_at > NOW());

-- Pending invitations only
CREATE INDEX idx_invitations_pending ON team_invitations(player_id, team_id)
    WHERE status = 'pending' AND expires_at > NOW();

-- Unprocessed server events
CREATE INDEX idx_server_events_pending ON server_events(received_at)
    WHERE processed = FALSE;
```

### 15.3 Expression Indexes

```sql
-- Case-insensitive username search
CREATE INDEX idx_users_username_lower ON users(lower(username));

-- Search by display name
CREATE INDEX idx_players_display_name_lower ON players(lower(display_name));

-- Date-only index for availability queries
CREATE INDEX idx_availability_date_only ON substitute_availability(date(start_utc));
```

---

## 16. Migration Strategy

### 16.1 Migration File Structure

```
migrations/
├── 00000000000000_initial_setup.sql
├── 20240101000000_create_users.sql
├── 20240101000001_create_players.sql
├── 20240101000002_create_auth_tables.sql
├── 20240101000003_create_rbac_tables.sql
├── 20240102000000_create_teams.sql
├── 20240102000001_create_games.sql
├── 20240102000002_create_matches.sql
├── 20240103000000_create_lobbies.sql
├── 20240103000001_create_tournaments.sql
├── 20240104000000_create_substitutes.sql
├── 20240104000001_create_servers.sql
├── 20240105000000_create_plugins.sql
├── 20240105000001_create_sagas.sql
├── 20240106000000_create_audit.sql
├── 20240106000001_seed_roles_permissions.sql
├── 20240106000002_seed_default_data.sql
└── plugins/
    ├── cs2/
    │   └── 20240201000000_cs2_initial.sql
    └── aoe4/
        └── 20240201000000_aoe4_initial.sql
```

### 16.2 Migration Commands

```bash
# Run all pending migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert

# Check migration status
sqlx migrate info

# Create new migration
sqlx migrate add create_new_table
```

### 16.3 Migration Best Practices

1. **Atomic migrations**: Each migration should be atomic and reversible
2. **No data loss**: Never remove columns without migration path
3. **Backward compatible**: Support rolling deployments
4. **Test migrations**: Test on production-like data
5. **Document changes**: Include comments explaining changes

---

## 17. Entity Relationship Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                    CORE RELATIONSHIPS                                        │
└─────────────────────────────────────────────────────────────────────────────────────────────┘

    ┌──────────┐        ┌──────────────┐        ┌──────────────┐
    │  users   │───1:1──│   players    │───M:N──│    teams     │
    └────┬─────┘        └──────┬───────┘        └──────┬───────┘
         │                     │                       │
         │              ┌──────┴───────┐               │
         │              │   player_    │               │
         │              │game_profiles │               │
         │              └──────────────┘               │
         │                     │                       │
    ┌────┴─────┐               │                ┌──────┴───────┐
    │ oauth_   │               │                │team_members  │
    │connections│              │                └──────────────┘
    └──────────┘               │                       │
         │                     │                       │
    ┌────┴─────┐        ┌──────┴───────┐        ┌──────┴───────┐
    │ refresh_ │        │   matches    │───M:N──│match_players │
    │ tokens   │        └──────────────┘        └──────────────┘
    └──────────┘               │
                               │
                        ┌──────┴───────┐
                        │ match_maps   │
                        └──────────────┘

┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                  TOURNAMENT RELATIONSHIPS                                    │
└─────────────────────────────────────────────────────────────────────────────────────────────┘

    ┌──────────┐        ┌──────────────┐        ┌──────────────┐
    │ leagues  │───1:M──│   seasons    │───1:M──│ tournaments  │
    └──────────┘        └──────┬───────┘        └──────┬───────┘
                               │                       │
                        ┌──────┴───────┐        ┌──────┴───────┐
                        │   season_    │        │  brackets    │
                        │ standings    │        └──────┬───────┘
                        └──────────────┘               │
                               │                ┌──────┴───────┐
                        ┌──────┴───────┐        │   bracket_   │
                        │   season_    │        │   matches    │
                        │participants  │        └──────────────┘
                        └──────────────┘

┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                   LOBBY & SERVER FLOW                                        │
└─────────────────────────────────────────────────────────────────────────────────────────────┘

    ┌──────────┐        ┌──────────────┐        ┌──────────────┐
    │ lobbies  │───1:M──│lobby_players │        │game_servers  │
    └────┬─────┘        └──────────────┘        └──────┬───────┘
         │                                             │
         │                                      ┌──────┴───────┐
         └──────────────────┬──────────────────│   server_    │
                            │                  │ reservations │
                     ┌──────┴───────┐          └──────────────┘
                     │   matches    │
                     └──────────────┘

┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                  SUBSTITUTE SYSTEM                                           │
└─────────────────────────────────────────────────────────────────────────────────────────────┘

    ┌──────────┐        ┌──────────────┐        ┌──────────────┐
    │  league_ │───1:M──│ substitute_  │        │ substitute_  │
    │sub_pool  │        │ availability │        │  requests    │
    └────┬─────┘        └──────────────┘        └──────┬───────┘
         │                                             │
         │                                      ┌──────┴───────┐
         │                                      │ substitute_  │
         │                                      │  responses   │
         │                                      └──────────────┘
         │                                             │
         └──────────────────┬──────────────────────────┘
                            │
                     ┌──────┴───────┐
                     │ substitute_  │
                     │ assignments  │
                     └──────────────┘
```

---

*Schema document prepared for engineering review.*
*Total Tables: 48 | PostgreSQL 15+ Required*
