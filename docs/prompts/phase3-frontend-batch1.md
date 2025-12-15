# Phase 3 Frontend Implementation - Batch 1: Tournament & Match Scheduling UI

## Context

You are implementing **Phase 3 Frontend Batch 1** for a multi-game competitive gaming portal. The frontend is built with Vue 3, TypeScript, Vuetify 3, Pinia, and Vue Router. The backend APIs for this batch are already complete.

**Tech Stack**:
- Vue 3 + TypeScript
- Vuetify 3 (UI framework)
- Pinia (state management)
- Vue Router 4
- openapi-fetch (type-safe API client)
- openapi-typescript (generates types from OpenAPI)

**Existing Patterns** (follow these):
- `src/api/client.ts` - Type-safe API client with auth middleware
- `src/stores/*.ts` - Pinia stores with async actions
- `src/composables/useAsyncAction.ts` - Loading/error state management
- `src/components/admin/*.vue` - Admin panel components

**Backend APIs Ready** (from Phase 3 Batch 1 & 2):
- Match lifecycle: `/v1/tournaments/{id}/matches/{match_id}/status`
- Scheduling: `/v1/tournaments/{id}/matches/{match_id}/schedule/*`
- Availability: `/v1/players/me/availability/*`

**Reference Files**:
- `src/pages/admin/AdminLeaguesPage.vue` - Example admin page pattern
- `src/stores/leagueTeams.ts` - Example Pinia store pattern
- `src/components/AvailabilityWindowsManager.vue` - Existing availability component

---

## Your Task

Implement the frontend components for tournament management and match scheduling. This batch focuses on the admin-facing tournament workflow and participant scheduling experience.

### Sub-Phases in This Batch

| Sub-Phase | Name | Description |
|-----------|------|-------------|
| F3.1 | Tournament Admin CRUD | Create, list, edit tournaments in admin panel |
| F3.2 | Tournament Public Views | Public tournament listing, detail, bracket views |
| F3.3 | Match Scheduling UI | Schedule proposals, acceptance workflow |
| F3.4 | Availability Settings | Player availability configuration |

---

## Sub-Phase F3.1: Tournament Admin CRUD

### Scope

Create the admin panel pages and components for tournament management.

### Deliverables

#### 1. Pinia Store: `src/stores/tournaments.ts`

```typescript
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api, handleApiError } from '@/api'
import type { components } from '@/api/types'

type Tournament = components['schemas']['TournamentResponse']
type TournamentSummary = components['schemas']['TournamentSummaryResponse']
type CreateTournamentRequest = components['schemas']['CreateTournamentRequest']
type UpdateTournamentRequest = components['schemas']['UpdateTournamentRequest']

export const useTournamentsStore = defineStore('tournaments', () => {
  const tournaments = ref<TournamentSummary[]>([])
  const currentTournament = ref<Tournament | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetchTournaments(filters?: { game_id?: string; status?: string }) {
    loading.value = true
    error.value = null
    try {
      const { data, error: apiError } = await api.GET('/v1/tournaments', {
        params: { query: filters }
      })
      if (apiError) handleApiError(apiError)
      tournaments.value = data?.data || []
    } catch (e) {
      error.value = (e as Error).message
      throw e
    } finally {
      loading.value = false
    }
  }

  async function fetchTournament(id: string) { /* ... */ }
  async function createTournament(req: CreateTournamentRequest) { /* ... */ }
  async function updateTournament(id: string, req: UpdateTournamentRequest) { /* ... */ }
  async function publishTournament(id: string) { /* ... */ }
  async function openRegistration(id: string) { /* ... */ }
  async function startTournament(id: string) { /* ... */ }

  return {
    tournaments,
    currentTournament,
    loading,
    error,
    fetchTournaments,
    fetchTournament,
    createTournament,
    updateTournament,
    publishTournament,
    openRegistration,
    startTournament,
  }
})
```

#### 2. Admin Page: `src/pages/admin/AdminTournamentsPage.vue`

Features:
- List tournaments with filters (game, status)
- Create tournament modal
- Quick actions (publish, open registration, start)
- Link to tournament detail/management

```vue
<template>
  <v-container>
    <v-row>
      <v-col>
        <h1 class="text-h4 mb-4">Tournaments</h1>
      </v-col>
      <v-col cols="auto">
        <v-btn color="primary" @click="showCreateModal = true">
          <v-icon start>mdi-plus</v-icon>
          Create Tournament
        </v-btn>
      </v-col>
    </v-row>

    <!-- Filters -->
    <v-row>
      <v-col cols="12" md="4">
        <v-select
          v-model="filters.game_id"
          :items="games"
          item-title="display_name"
          item-value="id"
          label="Game"
          clearable
        />
      </v-col>
      <v-col cols="12" md="4">
        <v-select
          v-model="filters.status"
          :items="statusOptions"
          label="Status"
          clearable
        />
      </v-col>
    </v-row>

    <!-- Tournament List -->
    <v-data-table
      :items="tournaments"
      :headers="headers"
      :loading="loading"
      @click:row="openTournament"
    >
      <template #item.status="{ item }">
        <TournamentStatusChip :status="item.status" />
      </template>
      <template #item.actions="{ item }">
        <TournamentActionsMenu :tournament="item" @action="handleAction" />
      </template>
    </v-data-table>

    <!-- Create Modal -->
    <TournamentCreateModal
      v-model="showCreateModal"
      @created="onTournamentCreated"
    />
  </v-container>
</template>
```

#### 3. Component: `src/components/admin/TournamentCreateModal.vue`

Form fields:
- Game selection (from games store)
- Name, slug, description
- Format (single_elimination, double_elimination, round_robin, swiss)
- Participant type (individual, team) + team size
- Min/max participants
- Registration type (open, approval, invite_only)
- Scheduling mode (live, self_scheduled, admin_scheduled)
- Match format (bo1, bo3, bo5)
- Dates (registration opens, starts at)

#### 4. Component: `src/components/admin/TournamentEditModal.vue`

Same fields as create, pre-populated with existing values.

#### 5. Component: `src/components/admin/TournamentActionsMenu.vue`

Contextual actions based on tournament status:
- Draft: Edit, Publish, Delete
- Published: Open Registration, Edit
- Registration Open: Close Registration, View Registrations
- Ready: Start Tournament
- In Progress: View Bracket, Manage Matches

#### 6. Route Configuration

Update `src/router/index.ts`:

```typescript
{
  path: '/admin/tournaments',
  name: 'admin-tournaments',
  component: () => import('@/pages/admin/AdminTournamentsPage.vue'),
  meta: { requiresAuth: true, layout: 'admin' }
},
{
  path: '/admin/tournaments/:id',
  name: 'admin-tournament-detail',
  component: () => import('@/pages/admin/AdminTournamentDetailPage.vue'),
  meta: { requiresAuth: true, layout: 'admin' }
}
```

### Acceptance Criteria (F3.1)

- [ ] Tournament list page with filtering
- [ ] Create tournament modal with all fields
- [ ] Edit tournament modal
- [ ] Status-aware action menu
- [ ] Proper loading/error states
- [ ] TypeScript types from OpenAPI

---

## Sub-Phase F3.2: Tournament Public Views

### Scope

Create public-facing tournament pages for participants and spectators.

### Deliverables

#### 1. Page: `src/pages/TournamentsPage.vue`

Public tournament listing with:
- Featured tournaments carousel
- Game filter tabs
- Status filters (upcoming, live, completed)
- Search
- Pagination

#### 2. Page: `src/pages/TournamentDetailPage.vue`

Tournament detail page with tabs:
- **Overview**: Description, rules, prize info
- **Participants**: Registered players/teams
- **Bracket**: Visual bracket display
- **Matches**: List of all matches
- **Schedule**: Upcoming match times

```vue
<template>
  <v-container>
    <!-- Header -->
    <TournamentHeader :tournament="tournament" />

    <!-- Registration CTA -->
    <TournamentRegistrationCard
      v-if="canRegister"
      :tournament="tournament"
      @register="handleRegister"
    />

    <!-- Tabs -->
    <v-tabs v-model="activeTab">
      <v-tab value="overview">Overview</v-tab>
      <v-tab value="participants">Participants</v-tab>
      <v-tab value="bracket">Bracket</v-tab>
      <v-tab value="matches">Matches</v-tab>
    </v-tabs>

    <v-tabs-window v-model="activeTab">
      <v-tabs-window-item value="overview">
        <TournamentOverview :tournament="tournament" />
      </v-tabs-window-item>
      <v-tabs-window-item value="participants">
        <TournamentParticipants :tournament-id="tournamentId" />
      </v-tabs-window-item>
      <v-tabs-window-item value="bracket">
        <TournamentBracket :tournament-id="tournamentId" />
      </v-tabs-window-item>
      <v-tabs-window-item value="matches">
        <TournamentMatches :tournament-id="tournamentId" />
      </v-tabs-window-item>
    </v-tabs-window>
  </v-container>
</template>
```

#### 3. Component: `src/components/tournament/TournamentBracket.vue`

Visual bracket display:
- Single elimination bracket rendering
- Double elimination with winners/losers brackets
- Match cards showing participants, scores, status
- Click to view match details
- Responsive layout (scroll on mobile)

#### 4. Component: `src/components/tournament/TournamentMatchCard.vue`

Match card showing:
- Match number/round
- Participant names/logos
- Scores (if completed)
- Status badge
- Scheduled time
- Link to match detail

#### 5. Component: `src/components/tournament/TournamentRegistrationCard.vue`

Registration widget:
- Shows registration status
- Register/withdraw buttons
- Team selection (for team tournaments)
- Check-in button (when check-in is open)

### Acceptance Criteria (F3.2)

- [ ] Public tournament list with filters
- [ ] Tournament detail page with tabs
- [ ] Bracket visualization (at least single elimination)
- [ ] Match cards with status
- [ ] Registration flow

---

## Sub-Phase F3.3: Match Scheduling UI

### Scope

Implement the match scheduling proposal workflow for self-scheduled tournaments.

### Deliverables

#### 1. Pinia Store: `src/stores/matchScheduling.ts`

```typescript
export const useMatchSchedulingStore = defineStore('matchScheduling', () => {
  // State
  const activeProposal = ref<ScheduleProposal | null>(null)
  const proposalHistory = ref<ScheduleProposal[]>([])
  const suggestedTimes = ref<SuggestedTime[]>([])

  // Actions
  async function proposeSchedule(matchId: string, times: string[]) { /* ... */ }
  async function acceptProposal(matchId: string, proposalId: string, selectedTime: string) { /* ... */ }
  async function rejectProposal(matchId: string, proposalId: string) { /* ... */ }
  async function counterPropose(matchId: string, proposalId: string, newTimes: string[]) { /* ... */ }
  async function fetchSuggestedTimes(matchId: string, from: string, to: string) { /* ... */ }
  async function fetchActiveProposal(matchId: string) { /* ... */ }

  return { /* ... */ }
})
```

#### 2. Page: `src/pages/MatchDetailPage.vue`

Match detail page showing:
- Participants
- Current status with visual indicator
- Scheduled time (if set)
- Scheduling panel (for self-scheduled)
- Check-in panel (when checking_in status)
- Veto panel (when pick_ban status) - placeholder for Batch 2
- Result submission (when awaiting_result) - placeholder for Batch 2

#### 3. Component: `src/components/match/MatchSchedulingPanel.vue`

Scheduling workflow:

```vue
<template>
  <v-card>
    <v-card-title>Schedule Match</v-card-title>
    <v-card-text>
      <!-- No active proposal - can propose -->
      <template v-if="!activeProposal">
        <p>Propose times for this match:</p>
        <ScheduleTimePicker
          v-model="proposedTimes"
          :suggested-times="suggestedTimes"
          :max-times="5"
        />
        <v-btn color="primary" @click="submitProposal">
          Send Proposal
        </v-btn>
      </template>

      <!-- Active proposal exists -->
      <template v-else>
        <ProposalCard
          :proposal="activeProposal"
          :is-proposer="isProposer"
          @accept="handleAccept"
          @reject="handleReject"
          @counter="showCounterDialog = true"
        />
      </template>
    </v-card-text>
  </v-card>
</template>
```

#### 4. Component: `src/components/match/ScheduleTimePicker.vue`

Time selection widget:
- Calendar view for date selection
- Time slots based on availability
- Highlight suggested times (overlap with opponent)
- Allow up to 5 time options
- Timezone display

#### 5. Component: `src/components/match/ProposalCard.vue`

Shows proposal details:
- Proposer name
- Proposed times (selectable for acceptor)
- Expiration countdown
- Accept/Reject/Counter buttons (for responder)
- Cancel button (for proposer)
- Status if already responded

#### 6. Component: `src/components/match/MatchStatusTimeline.vue`

Visual timeline showing:
- Status progression
- Timestamps for each transition
- Current status highlighted
- Upcoming deadlines

### Acceptance Criteria (F3.3)

- [ ] Propose times with calendar picker
- [ ] View/respond to proposals
- [ ] Counter-proposal workflow
- [ ] Suggested times based on availability
- [ ] Expiration countdown display
- [ ] Match status timeline

---

## Sub-Phase F3.4: Availability Settings

### Scope

Enhance the existing availability components and add tournament-specific settings.

### Deliverables

#### 1. Enhance Store: `src/stores/availability.ts`

Add methods for:
- Tournament-specific availability
- Date-range queries
- Suggestion generation

#### 2. Page: `src/pages/PlayerAvailabilityPage.vue`

Full availability management:
- Weekly recurring windows (existing component)
- Override/exception dates
- Calendar view showing effective availability
- Tournament-specific availability toggle

#### 3. Component: `src/components/AvailabilityCalendarView.vue` (enhance)

Calendar visualization:
- Month view with availability overlay
- Day detail on click
- Color coding (available, busy, override)
- Match indicators (scheduled matches shown)

#### 4. Component: `src/components/match/OpponentAvailabilityPreview.vue`

For scheduling panel:
- Shows opponent's availability (read-only)
- Highlights overlap with your availability
- Used in ScheduleTimePicker

### Acceptance Criteria (F3.4)

- [ ] Full availability management page
- [ ] Calendar view with month navigation
- [ ] Tournament-specific availability
- [ ] Opponent availability preview in scheduling

---

## Implementation Guidelines

### Component Patterns

Follow existing codebase patterns:

1. **Composables**: Use `useAsyncAction` for loading states
2. **Stores**: Pinia stores with setup syntax
3. **Modals**: Use `v-model` for visibility, emit events
4. **Forms**: Use Vuetify form components with validation
5. **Error Handling**: Use ErrorAlert component for errors

### File Organization

```
src/
├── components/
│   ├── admin/
│   │   ├── TournamentCreateModal.vue
│   │   ├── TournamentEditModal.vue
│   │   └── TournamentActionsMenu.vue
│   ├── match/
│   │   ├── MatchSchedulingPanel.vue
│   │   ├── MatchStatusTimeline.vue
│   │   ├── ProposalCard.vue
│   │   └── ScheduleTimePicker.vue
│   └── tournament/
│       ├── TournamentBracket.vue
│       ├── TournamentHeader.vue
│       ├── TournamentMatchCard.vue
│       └── TournamentRegistrationCard.vue
├── pages/
│   ├── admin/
│   │   ├── AdminTournamentsPage.vue
│   │   └── AdminTournamentDetailPage.vue
│   ├── TournamentsPage.vue
│   ├── TournamentDetailPage.vue
│   └── MatchDetailPage.vue
└── stores/
    ├── tournaments.ts
    └── matchScheduling.ts
```

### Testing Strategy

1. Unit test stores with mocked API
2. Component tests with Vue Test Utils
3. E2E tests for critical flows (registration, scheduling)

### API Type Generation

After backend changes, regenerate types:
```bash
npm run generate:api
```

---

## Verification Checklist

### Sub-Phase F3.1
- [ ] Admin tournaments page loads
- [ ] Create tournament works
- [ ] Edit tournament works
- [ ] Status actions work (publish, etc.)

### Sub-Phase F3.2
- [ ] Public tournament list works
- [ ] Tournament detail page works
- [ ] Bracket renders correctly
- [ ] Registration works

### Sub-Phase F3.3
- [ ] Propose schedule works
- [ ] Accept/reject proposal works
- [ ] Counter-proposal works
- [ ] Time suggestions display

### Sub-Phase F3.4
- [ ] Availability calendar works
- [ ] Overrides work
- [ ] Opponent availability shows

---

## Output

After completing this batch:

1. Ensure all components render correctly
2. Verify API integration works
3. Test responsive layouts
4. Note any UX improvements needed

**Proceed to Frontend Batch 2 (Pick-Ban & Result UI) after this batch is complete.**
