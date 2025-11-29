# League Teams & Seasons - Design Document

## Overview

This document outlines the transition from global teams to league-scoped teams with season support. This is a breaking change that removes the old team system entirely.

## Data Model

### Entity Relationship Diagram

```
┌──────────┐     ┌──────────┐     ┌───────────────┐     ┌──────────────┐     ┌────────────────────┐
│  Games   │────▶│ Leagues  │────▶│ LeagueSeasons │────▶│ LeagueTeams  │────▶│ LeagueTeamMembers  │
└──────────┘ 1:N └──────────┘ 1:N └───────────────┘ 1:N └──────────────┘ 1:N └────────────────────┘
                      │                   │                    │
                      │                   │                    │
                      ▼                   ▼                    ▼
               ┌──────────────┐    ┌─────────────┐    ┌───────────────────────┐
               │LeagueMembers │    │   Users     │    │ LeagueTeamInvitations │
               │(admins/mods) │    │ (captains)  │    └───────────────────────┘
               └──────────────┘    └─────────────┘
```

### Core Entities

#### 1. LeagueSeason
Represents a competition period within a league.

| Field | Type | Description |
|-------|------|-------------|
| id | UUID | Primary key |
| league_id | UUID | Parent league |
| name | String | "Season 1", "Winter 2024" |
| slug | String | URL-friendly identifier |
| description | String? | Optional description |
| registration_start | DateTime? | When teams can start forming |
| registration_end | DateTime? | Deadline for roster finalization |
| season_start | DateTime? | Competition begins |
| season_end | DateTime? | Competition ends |
| team_size_min | i32 | Minimum players per team |
| team_size_max | i32 | Maximum players per team |
| max_substitutes | i32 | Max substitutes allowed |
| max_teams | i32? | Team cap (null = unlimited) |
| roster_lock_status | Enum | open, soft_lock, hard_lock |
| status | Enum | draft, registration, active, playoffs, completed, cancelled |
| settings | JSON | Additional configuration |

#### 2. LeagueTeam
A team participating in a specific season.

| Field | Type | Description |
|-------|------|-------------|
| id | UUID | Primary key |
| season_id | UUID | Parent season |
| name | String | Team name (unique in season) |
| tag | String | 2-5 char tag (unique in season) |
| description | String? | Team description |
| logo_url | String? | Team logo |
| banner_url | String? | Team banner |
| primary_color | String? | Hex color |
| secondary_color | String? | Hex color |
| captain_user_id | UUID | Team captain |
| status | Enum | forming, pending, active, disqualified, disbanded, eliminated |
| matches_played | i32 | Statistics |
| matches_won | i32 | Statistics |
| matches_lost | i32 | Statistics |
| matches_drawn | i32 | Statistics |
| seed | i32? | Tournament seeding |
| rating | i32? | Team rating/ELO |

#### 3. LeagueTeamMember
A player on a league team.

| Field | Type | Description |
|-------|------|-------------|
| id | UUID | Primary key |
| team_id | UUID | Parent team |
| user_id | UUID | The user |
| role | Enum | captain, player, substitute |
| position | String? | Game-specific position |
| jersey_number | i32? | 0-99 |
| status | Enum | active, inactive, left, removed |
| joined_at | DateTime | When joined |
| left_at | DateTime? | When left (if applicable) |
| added_by | UUID? | Who added them |

### Key Constraints

1. **One Primary Team Per Season**: A user can only be `captain` or `player` (not `substitute`) on ONE team per season. Enforced by database index.

2. **Substitute Restrictions**: Substitutes can be on multiple teams but cannot play against their primary team. (Enforced at application level during match validation)

3. **Roster Lock Enforcement**:
   - `open`: Any roster changes allowed
   - `soft_lock`: Only substitute add/remove allowed
   - `hard_lock`: No roster changes

---

## API Design

### URL Structure

```
/v1/leagues/{league_id}/seasons                    # Season management
/v1/leagues/{league_id}/seasons/{season_id}        # Single season
/v1/leagues/{league_id}/seasons/{season_id}/teams  # Teams in season
/v1/league-teams/{team_id}                         # Direct team access
/v1/league-teams/{team_id}/members                 # Team roster
/v1/league-team-invitations/{id}                   # Invitation actions
/v1/users/me/league-teams                          # My team memberships
```

### Endpoints

#### Season Management

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| POST | `/v1/leagues/{id}/seasons` | Create season | league.seasons.manage |
| GET | `/v1/leagues/{id}/seasons` | List seasons | Public |
| GET | `/v1/leagues/{id}/seasons/{id}` | Get season | Public |
| PATCH | `/v1/leagues/{id}/seasons/{id}` | Update season | league.seasons.manage |
| POST | `/v1/leagues/{id}/seasons/{id}/activate` | Start registration | league.seasons.manage |
| POST | `/v1/leagues/{id}/seasons/{id}/lock-rosters` | Lock rosters | league.rosters.lock |
| POST | `/v1/leagues/{id}/seasons/{id}/unlock-rosters` | Unlock rosters | league.rosters.lock |

#### Team Management

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| POST | `/v1/leagues/{id}/seasons/{id}/teams` | Create team | Authenticated |
| GET | `/v1/leagues/{id}/seasons/{id}/teams` | List teams | Public |
| GET | `/v1/league-teams/{id}` | Get team | Public |
| PATCH | `/v1/league-teams/{id}` | Update team | Team captain |
| POST | `/v1/league-teams/{id}/register` | Submit for registration | Team captain |
| POST | `/v1/league-teams/{id}/disband` | Disband team | Team captain |
| DELETE | `/v1/league-teams/{id}` | Delete team (admin) | league.teams.manage |

#### Roster Management

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| GET | `/v1/league-teams/{id}/members` | List roster | Public |
| POST | `/v1/league-teams/{id}/members` | Add member directly | Team captain |
| PATCH | `/v1/league-teams/{id}/members/{user_id}` | Update member | Team captain |
| DELETE | `/v1/league-teams/{id}/members/{user_id}` | Remove member | Team captain |
| POST | `/v1/league-teams/{id}/leave` | Leave team | Member |
| POST | `/v1/league-teams/{id}/transfer-captain` | Transfer captaincy | Team captain |

#### Invitations

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| POST | `/v1/league-teams/{id}/invitations` | Invite user | Team captain |
| GET | `/v1/league-teams/{id}/invitations` | List team invitations | Team captain |
| POST | `/v1/league-teams/{id}/apply` | Request to join | Authenticated |
| GET | `/v1/users/me/league-team-invitations` | My invitations | Authenticated |
| POST | `/v1/league-team-invitations/{id}/accept` | Accept | Invitee |
| POST | `/v1/league-team-invitations/{id}/decline` | Decline | Invitee |
| DELETE | `/v1/league-team-invitations/{id}` | Cancel | Inviter |

#### User-Centric

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| GET | `/v1/users/me/league-teams` | My team memberships | Authenticated |
| GET | `/v1/users/me/league-team-invitations` | My pending invitations | Authenticated |

---

## Implementation Plan

### Phase 1: Database Migration (Already Done)
- [x] Create `league_seasons` table
- [x] Create `league_teams` table
- [x] Create `league_team_members` table
- [x] Create `league_team_invitations` table
- [x] Add one-primary-team constraint
- [x] Add helper views
- [x] Deprecate old tables

### Phase 2: Domain Layer
Create new domain entities and repository traits:

```
portal-domain/src/entities/
├── league_season.rs      # LeagueSeason, SeasonStatus, RosterLockStatus
├── league_team.rs        # LeagueTeam, TeamStatus, TeamRole
└── mod.rs                # Update exports

portal-domain/src/repositories/
├── league_season.rs      # LeagueSeasonRepository trait
├── league_team.rs        # LeagueTeamRepository trait
├── league_team_member.rs # LeagueTeamMemberRepository trait
└── mod.rs                # Update exports

portal-domain/src/services/
├── league_season.rs      # LeagueSeasonService
├── league_team.rs        # LeagueTeamService
└── mod.rs                # Update exports
```

### Phase 3: Database Layer
Implement repository adapters:

```
portal-db/src/entities/
├── league_season.rs      # DB row types
├── league_team.rs        # DB row types
└── mod.rs

portal-db/src/adapters/
├── league_season.rs      # PgLeagueSeasonRepository
├── league_team.rs        # PgLeagueTeamRepository
├── league_team_member.rs # PgLeagueTeamMemberRepository
└── mod.rs
```

### Phase 4: API Layer
Create handlers and DTOs:

```
portal-api/src/dto/requests/
├── league_season.rs      # CreateSeasonRequest, UpdateSeasonRequest
├── league_team.rs        # CreateTeamRequest, UpdateTeamRequest
└── mod.rs

portal-api/src/dto/responses/
├── league_season.rs      # SeasonResponse, SeasonDetailResponse
├── league_team.rs        # TeamResponse, TeamMemberResponse
└── mod.rs

portal-api/src/handlers/
├── league_seasons.rs     # Season handlers
├── league_teams.rs       # Team handlers
└── mod.rs
```

### Phase 5: Remove Old Team System
Delete legacy code:

```
DELETE: portal-domain/src/entities/team.rs
DELETE: portal-domain/src/repositories/team.rs
DELETE: portal-domain/src/repositories/team_member.rs
DELETE: portal-domain/src/services/team.rs
DELETE: portal-domain/src/services/team_invitation.rs

DELETE: portal-db/src/entities/team.rs
DELETE: portal-db/src/adapters/team.rs
DELETE: portal-db/src/adapters/team_member.rs
DELETE: portal-db/src/adapters/team_invitation.rs

DELETE: portal-api/src/handlers/teams.rs
DELETE: portal-api/src/handlers/invitations.rs (team invitations)
DELETE: portal-api/src/dto/requests/team.rs
DELETE: portal-api/src/dto/responses/team.rs

UPDATE: portal-api/src/routes/mod.rs    # Remove team routes
UPDATE: portal-api/src/openapi.rs       # Remove team schemas
UPDATE: portal-api/src/state.rs         # Remove team services
```

### Phase 6: Update Tests
- Delete old team tests
- Create comprehensive league team tests
- Test roster lock enforcement
- Test one-primary-team constraint
- Test substitute restrictions

### Phase 7: Update OpenAPI
- Remove all old team schemas and paths
- Add new season/team schemas and paths
- Update tag descriptions

---

## File Changes Summary

### New Files to Create

| File | Description |
|------|-------------|
| `portal-domain/src/entities/league_season.rs` | Season entity |
| `portal-domain/src/entities/league_team.rs` | Team entity |
| `portal-domain/src/repositories/league_season.rs` | Season repo trait |
| `portal-domain/src/repositories/league_team.rs` | Team repo trait |
| `portal-domain/src/repositories/league_team_member.rs` | Member repo trait |
| `portal-domain/src/services/league_season.rs` | Season service |
| `portal-domain/src/services/league_team.rs` | Team service |
| `portal-db/src/entities/league_season.rs` | DB entities |
| `portal-db/src/entities/league_team.rs` | DB entities |
| `portal-db/src/adapters/league_season.rs` | Season adapter |
| `portal-db/src/adapters/league_team.rs` | Team adapter |
| `portal-db/src/adapters/league_team_member.rs` | Member adapter |
| `portal-api/src/dto/requests/league_season.rs` | Request DTOs |
| `portal-api/src/dto/requests/league_team.rs` | Request DTOs |
| `portal-api/src/dto/responses/league_season.rs` | Response DTOs |
| `portal-api/src/dto/responses/league_team.rs` | Response DTOs |
| `portal-api/src/handlers/league_seasons.rs` | Season handlers |
| `portal-api/src/handlers/league_teams.rs` | Team handlers |
| `portal-api/tests/league_teams_test.rs` | Integration tests |
| `portal-api/tests/league_seasons_test.rs` | Integration tests |

### Files to Delete

| File | Reason |
|------|--------|
| `portal-domain/src/entities/team.rs` | Replaced by league_team |
| `portal-domain/src/repositories/team.rs` | Replaced |
| `portal-domain/src/repositories/team_member.rs` | Replaced |
| `portal-domain/src/services/team.rs` | Replaced |
| `portal-domain/src/services/team_invitation.rs` | Replaced |
| `portal-db/src/entities/team.rs` | Replaced |
| `portal-db/src/adapters/team.rs` | Replaced |
| `portal-db/src/adapters/team_member.rs` | Replaced |
| `portal-db/src/adapters/team_invitation.rs` | Replaced |
| `portal-api/src/handlers/teams.rs` | Replaced |
| `portal-api/src/handlers/invitations.rs` | Replaced |
| `portal-api/src/dto/requests/team.rs` | Replaced |
| `portal-api/src/dto/responses/team.rs` | Replaced |
| `portal-api/src/dto/responses/invitation.rs` | Replaced |
| `portal-api/tests/teams_test.rs` | Replaced |

### Files to Modify

| File | Changes |
|------|---------|
| `portal-domain/src/entities/mod.rs` | Update exports |
| `portal-domain/src/repositories/mod.rs` | Update exports |
| `portal-domain/src/services/mod.rs` | Update exports |
| `portal-db/src/entities/mod.rs` | Update exports |
| `portal-db/src/adapters/mod.rs` | Update exports |
| `portal-api/src/routes/mod.rs` | Replace team routes |
| `portal-api/src/state.rs` | Replace team services |
| `portal-api/src/openapi.rs` | Replace team schemas |
| `portal-api/src/dto/requests/mod.rs` | Update exports |
| `portal-api/src/dto/responses/mod.rs` | Update exports |
| `portal-api/src/handlers/mod.rs` | Update exports |
| `portal-core/src/ids.rs` | Add LeagueSeasonId, LeagueTeamId |
| `portal-core/src/permissions.rs` | Already has league permissions |
| `portal-test/src/builders/mod.rs` | Remove TeamBuilder |

---

## Business Logic

### Team Creation Flow

```
1. User creates team in a season
   ├── Check: Season status is 'registration'
   ├── Check: User is a league member OR league is open
   ├── Check: User doesn't already captain a team in this season
   ├── Check: Team name/tag unique in season
   └── Create team with status 'forming', user as captain

2. Captain invites players
   ├── Check: Roster not locked
   ├── Check: Team not at max capacity
   ├── Check: Target user not already primary on another team (if inviting as player)
   └── Create invitation

3. Players accept invitations
   ├── Check: Invitation valid and not expired
   ├── Check: If joining as player, not already primary elsewhere
   └── Add to roster

4. Captain submits team for registration
   ├── Check: Minimum roster size met
   └── Set status to 'pending'

5. Admin approves team (if league requires it)
   └── Set status to 'active'
```

### Roster Lock Enforcement

```python
def can_modify_roster(season, change_type):
    if season.roster_lock_status == 'open':
        return True
    elif season.roster_lock_status == 'soft_lock':
        # Only substitute changes allowed
        return change_type in ['add_substitute', 'remove_substitute']
    else:  # hard_lock
        return False
```

### Substitute Conflict Check

```python
def can_substitute_play(substitute, match):
    # Get substitute's primary team in this season
    primary_team = get_primary_team(substitute, match.season_id)

    if primary_team is None:
        return True  # No primary team, can sub anywhere

    # Cannot play against their primary team
    if match.team_a == primary_team or match.team_b == primary_team:
        return False

    return True
```

---

## Error Codes

| Code | Description |
|------|-------------|
| `SEASON_NOT_FOUND` | Season doesn't exist |
| `SEASON_NOT_ACCEPTING_TEAMS` | Season not in registration phase |
| `TEAM_NOT_FOUND` | Team doesn't exist |
| `TEAM_NAME_TAKEN` | Name already used in season |
| `TEAM_TAG_TAKEN` | Tag already used in season |
| `ALREADY_ON_TEAM` | User already primary on a team |
| `ROSTER_LOCKED` | Cannot modify roster |
| `ROSTER_FULL` | Team at max capacity |
| `ROSTER_INCOMPLETE` | Not enough players to register |
| `NOT_TEAM_CAPTAIN` | Action requires captain role |
| `CANNOT_LEAVE_AS_CAPTAIN` | Must transfer captaincy first |
| `INVITATION_NOT_FOUND` | Invitation doesn't exist |
| `INVITATION_EXPIRED` | Invitation has expired |

---

## Timeline Estimate

| Phase | Effort |
|-------|--------|
| Phase 1: Migration | ✅ Complete |
| Phase 2: Domain Layer | ~200 lines |
| Phase 3: Database Layer | ~400 lines |
| Phase 4: API Layer | ~600 lines |
| Phase 5: Remove Old | ~100 lines deleted |
| Phase 6: Tests | ~400 lines |
| Phase 7: OpenAPI | ~50 lines |

**Total new code**: ~1,650 lines
**Total deleted**: ~1,500 lines (old team system)
