# Phase 3 Frontend Implementation - Batch 2: Game Lobby & Pick-Ban System

## Context

You are implementing **Phase 3 Frontend Batch 2** for a multi-game competitive gaming portal. This batch covers the real-time game lobby experience including the map pick-ban (veto) system and result submission workflow.

**Prerequisites**: Frontend Batch 1 must be complete.

**Tech Stack** (same as Batch 1):
- Vue 3 + TypeScript
- Vuetify 3, Pinia, Vue Router
- openapi-fetch for REST API
- **NEW**: WebSocket for real-time updates

**Backend APIs Ready** (from Phase 3 Batch 2):
- Veto: `/v1/matches/{match_id}/veto/*`
- Results: `/v1/matches/{match_id}/result/*`

**Key Design Considerations**:
- Pick-ban requires real-time updates (WebSocket)
- Turn-based UI with countdown timers
- Responsive for desktop and mobile
- Accessible keyboard navigation

---

## Your Task

Implement the real-time game lobby experience, including the pick-ban system and result submission. This is the most interactive part of the frontend.

### Sub-Phases in This Batch

| Sub-Phase | Name | Description |
|-----------|------|-------------|
| F3.5 | WebSocket Infrastructure | Real-time connection management |
| F3.6 | Match Lobby | Pre-match lobby with check-in and status |
| F3.7 | Pick-Ban Interface | Interactive map veto system |
| F3.8 | Result Submission | Claim/confirm result workflow |

---

## Sub-Phase F3.5: WebSocket Infrastructure

### Scope

Set up WebSocket connection management for real-time match updates.

### Deliverables

#### 1. WebSocket Service: `src/services/websocket.ts`

```typescript
import { ref, onUnmounted } from 'vue'
import { getAuthToken } from '@/api/client'

export type MatchEvent =
  | { type: 'match_status_changed'; payload: { match_id: string; status: string } }
  | { type: 'veto_action'; payload: VetoActionEvent }
  | { type: 'veto_turn_changed'; payload: { team_turn: string; deadline: string } }
  | { type: 'veto_completed'; payload: { selected_maps: string[] } }
  | { type: 'result_submitted'; payload: { claim_id: string } }
  | { type: 'result_confirmed'; payload: { match_id: string } }
  | { type: 'participant_checked_in'; payload: { registration_id: string } }

interface WebSocketOptions {
  onMessage: (event: MatchEvent) => void
  onConnect?: () => void
  onDisconnect?: () => void
  onError?: (error: Event) => void
}

export function useMatchWebSocket(matchId: string, options: WebSocketOptions) {
  const ws = ref<WebSocket | null>(null)
  const isConnected = ref(false)
  const reconnectAttempts = ref(0)
  const maxReconnectAttempts = 5

  function connect() {
    const wsUrl = `${import.meta.env.VITE_WS_URL || 'ws://localhost:3000'}/ws/matches/${matchId}`
    const token = getAuthToken()

    ws.value = new WebSocket(wsUrl, token ? [token] : undefined)

    ws.value.onopen = () => {
      isConnected.value = true
      reconnectAttempts.value = 0
      options.onConnect?.()
    }

    ws.value.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as MatchEvent
        options.onMessage(data)
      } catch (e) {
        console.error('Failed to parse WebSocket message:', e)
      }
    }

    ws.value.onclose = () => {
      isConnected.value = false
      options.onDisconnect?.()
      attemptReconnect()
    }

    ws.value.onerror = (error) => {
      options.onError?.(error)
    }
  }

  function attemptReconnect() {
    if (reconnectAttempts.value < maxReconnectAttempts) {
      reconnectAttempts.value++
      const delay = Math.min(1000 * Math.pow(2, reconnectAttempts.value), 30000)
      setTimeout(connect, delay)
    }
  }

  function disconnect() {
    if (ws.value) {
      ws.value.close()
      ws.value = null
    }
  }

  function send(message: object) {
    if (ws.value?.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify(message))
    }
  }

  // Auto-connect on mount
  connect()

  // Cleanup on unmount
  onUnmounted(() => {
    disconnect()
  })

  return {
    isConnected,
    reconnectAttempts,
    send,
    disconnect,
    reconnect: connect,
  }
}
```

#### 2. Composable: `src/composables/useMatchEvents.ts`

Higher-level composable for match-specific events:

```typescript
export function useMatchEvents(matchId: Ref<string>) {
  const matchStore = useMatchStore()
  const vetoStore = useVetoStore()

  const { isConnected, reconnect } = useMatchWebSocket(matchId.value, {
    onMessage: (event) => {
      switch (event.type) {
        case 'match_status_changed':
          matchStore.handleStatusChange(event.payload)
          break
        case 'veto_action':
          vetoStore.handleVetoAction(event.payload)
          break
        case 'veto_turn_changed':
          vetoStore.handleTurnChange(event.payload)
          break
        case 'veto_completed':
          vetoStore.handleVetoComplete(event.payload)
          break
        case 'result_submitted':
          matchStore.handleResultSubmitted(event.payload)
          break
        case 'result_confirmed':
          matchStore.handleResultConfirmed(event.payload)
          break
        case 'participant_checked_in':
          matchStore.handleCheckIn(event.payload)
          break
      }
    },
    onConnect: () => {
      console.log('Connected to match updates')
    },
    onDisconnect: () => {
      console.log('Disconnected from match updates')
    },
  })

  return { isConnected, reconnect }
}
```

### Acceptance Criteria (F3.5)

- [ ] WebSocket connects with auth token
- [ ] Auto-reconnect on disconnect
- [ ] Event routing to appropriate stores
- [ ] Cleanup on component unmount

---

## Sub-Phase F3.6: Match Lobby

### Scope

Create the pre-match lobby where participants check in and prepare.

### Deliverables

#### 1. Pinia Store: `src/stores/match.ts`

```typescript
export const useMatchStore = defineStore('match', () => {
  const match = ref<TournamentMatch | null>(null)
  const matchStatus = ref<MatchStatusDetails | null>(null)
  const statusHistory = ref<MatchStatusLog[]>([])
  const loading = ref(false)

  // Match data
  async function fetchMatch(tournamentId: string, matchId: string) { /* ... */ }
  async function fetchMatchStatus(tournamentId: string, matchId: string) { /* ... */ }
  async function fetchStatusHistory(tournamentId: string, matchId: string) { /* ... */ }

  // Actions
  async function checkIn(tournamentId: string, matchId: string, registrationId: string) { /* ... */ }
  async function forfeit(tournamentId: string, matchId: string, registrationId: string, reason: string) { /* ... */ }

  // WebSocket handlers
  function handleStatusChange(payload: { match_id: string; status: string }) {
    if (match.value?.id === payload.match_id) {
      match.value.status = payload.status
      fetchMatchStatus(/* ... */) // Refresh full status
    }
  }

  function handleCheckIn(payload: { registration_id: string }) {
    // Update local check-in state
  }

  return { /* ... */ }
})
```

#### 2. Page: `src/pages/MatchLobbyPage.vue`

Full-page match lobby experience:

```vue
<template>
  <v-container fluid class="match-lobby pa-0">
    <!-- Connection Status Banner -->
    <ConnectionStatusBanner :connected="isConnected" @reconnect="reconnect" />

    <!-- Match Header -->
    <MatchLobbyHeader :match="match" :tournament="tournament" />

    <!-- Main Content -->
    <v-row class="ma-0">
      <!-- Participant 1 -->
      <v-col cols="12" md="5">
        <ParticipantCard
          :participant="participant1"
          :is-checked-in="participant1CheckedIn"
          :is-current-user="isParticipant1"
          side="left"
        />
      </v-col>

      <!-- Center Status -->
      <v-col cols="12" md="2" class="d-flex align-center justify-center">
        <MatchStatusCenter
          :status="match.status"
          :scheduled-at="match.scheduled_at"
          :check-in-deadline="match.check_in_deadline"
        />
      </v-col>

      <!-- Participant 2 -->
      <v-col cols="12" md="5">
        <ParticipantCard
          :participant="participant2"
          :is-checked-in="participant2CheckedIn"
          :is-current-user="isParticipant2"
          side="right"
        />
      </v-col>
    </v-row>

    <!-- Action Panel -->
    <v-row class="ma-0 mt-4">
      <v-col cols="12">
        <MatchActionPanel
          :match="match"
          :is-participant="isParticipant"
          :is-checked-in="isCurrentUserCheckedIn"
          @check-in="handleCheckIn"
          @start-veto="navigateToVeto"
          @submit-result="showResultDialog = true"
        />
      </v-col>
    </v-row>

    <!-- Match Timeline -->
    <v-row class="ma-0 mt-4">
      <v-col cols="12">
        <MatchStatusTimeline :history="statusHistory" :current-status="match.status" />
      </v-col>
    </v-row>

    <!-- Result Dialog -->
    <ResultSubmissionDialog
      v-model="showResultDialog"
      :match="match"
      @submitted="handleResultSubmitted"
    />
  </v-container>
</template>
```

#### 3. Component: `src/components/match/ParticipantCard.vue`

Shows participant info in lobby:
- Player/team name and avatar
- Check-in status indicator
- Ready state
- Side indicator (left/right for visual balance)

#### 4. Component: `src/components/match/MatchStatusCenter.vue`

Central status display:
- Large status icon
- Countdown to scheduled time
- Check-in deadline countdown
- "VS" separator

#### 5. Component: `src/components/match/MatchActionPanel.vue`

Contextual actions based on status:

```vue
<template>
  <v-card>
    <v-card-text class="text-center">
      <!-- Checking In Status -->
      <template v-if="match.status === 'checking_in'">
        <template v-if="!isCheckedIn">
          <v-btn color="success" size="large" @click="$emit('check-in')">
            <v-icon start>mdi-check</v-icon>
            Check In
          </v-btn>
          <p class="mt-2 text-caption">
            Check in before <CountdownTimer :deadline="match.check_in_deadline" />
          </p>
        </template>
        <template v-else>
          <v-chip color="success">
            <v-icon start>mdi-check-circle</v-icon>
            You're checked in!
          </v-chip>
          <p class="mt-2">Waiting for opponent...</p>
        </template>
      </template>

      <!-- Pick/Ban Status -->
      <template v-else-if="match.status === 'pick_ban'">
        <v-btn color="primary" size="large" @click="$emit('start-veto')">
          <v-icon start>mdi-map</v-icon>
          Go to Map Veto
        </v-btn>
      </template>

      <!-- In Progress -->
      <template v-else-if="match.status === 'in_progress'">
        <v-chip color="info">Match in progress</v-chip>
        <p class="mt-2">Good luck!</p>
      </template>

      <!-- Awaiting Result -->
      <template v-else-if="match.status === 'awaiting_result'">
        <v-btn color="primary" size="large" @click="$emit('submit-result')">
          <v-icon start>mdi-trophy</v-icon>
          Submit Result
        </v-btn>
      </template>

      <!-- Completed -->
      <template v-else-if="match.status === 'completed'">
        <MatchResultDisplay :match="match" />
      </template>
    </v-card-text>
  </v-card>
</template>
```

#### 6. Component: `src/components/match/CountdownTimer.vue`

Reusable countdown component:
- Shows time remaining to deadline
- Color changes as deadline approaches
- Pulsing animation when < 1 minute

### Acceptance Criteria (F3.6)

- [ ] Lobby displays both participants
- [ ] Check-in button works
- [ ] Status updates in real-time
- [ ] Countdown timers work
- [ ] Actions change based on status

---

## Sub-Phase F3.7: Pick-Ban Interface

### Scope

Create the interactive map veto (pick-ban) system. This is the most complex UI component.

### Deliverables

#### 1. Pinia Store: `src/stores/veto.ts`

```typescript
export const useVetoStore = defineStore('veto', () => {
  const session = ref<VetoSession | null>(null)
  const actions = ref<VetoAction[]>([])
  const currentActionNumber = ref(0)
  const currentTeamTurn = ref<string | null>(null)
  const actionDeadline = ref<string | null>(null)
  const remainingMaps = ref<string[]>([])
  const selectedMaps = ref<MapSelection[]>([])
  const loading = ref(false)

  // API calls
  async function fetchSession(matchId: string) { /* ... */ }
  async function createSession(matchId: string, formatId: string, timeoutSeconds: number) { /* ... */ }
  async function startSession(matchId: string) { /* ... */ }
  async function recordCoinFlip(matchId: string, winnerId: string, goesFirst: boolean) { /* ... */ }
  async function performAction(matchId: string, mapId: string) { /* ... */ }
  async function selectSide(matchId: string, actionNumber: number, side: string) { /* ... */ }

  // WebSocket handlers
  function handleVetoAction(payload: VetoActionEvent) {
    // Add to actions list
    actions.value.push(payload.action)
    // Update remaining maps
    remainingMaps.value = remainingMaps.value.filter(m => m !== payload.action.map_id)
    // If it's a pick, add to selected
    if (payload.action.action_type === 'pick' || payload.action.action_type === 'decider') {
      selectedMaps.value.push({
        map_id: payload.action.map_id,
        picked_by: payload.action.performed_by_registration_id,
        game_number: selectedMaps.value.length + 1,
      })
    }
  }

  function handleTurnChange(payload: { team_turn: string; deadline: string }) {
    currentTeamTurn.value = payload.team_turn
    actionDeadline.value = payload.deadline
    currentActionNumber.value++
  }

  function handleVetoComplete(payload: { selected_maps: string[] }) {
    session.value!.status = 'completed'
  }

  // Computed
  const isMyTurn = computed(() => {
    // Check if current user's registration is currentTeamTurn
    return /* ... */
  })

  const currentActionType = computed(() => {
    // Get from veto format sequence
    return /* ... */
  })

  return { /* ... */ }
})
```

#### 2. Page: `src/pages/VetoPage.vue`

Full-page veto experience:

```vue
<template>
  <div class="veto-page">
    <!-- Connection Banner -->
    <ConnectionStatusBanner :connected="isConnected" />

    <!-- Header -->
    <VetoHeader
      :match="match"
      :format="vetoFormat"
      :action-number="currentActionNumber"
      :total-actions="totalActions"
    />

    <!-- Main Veto Area -->
    <div class="veto-content">
      <!-- Team Indicators -->
      <div class="veto-teams">
        <VetoTeamPanel
          :team="team1"
          :is-current-turn="currentTeamTurn === team1.registration_id"
          side="left"
        />
        <VetoTeamPanel
          :team="team2"
          :is-current-turn="currentTeamTurn === team2.registration_id"
          side="right"
        />
      </div>

      <!-- Map Pool Grid -->
      <VetoMapGrid
        :maps="mapPool"
        :remaining-maps="remainingMaps"
        :selected-maps="selectedMaps"
        :banned-maps="bannedMaps"
        :selectable="isMyTurn"
        :current-action-type="currentActionType"
        @select="handleMapSelect"
      />

      <!-- Action History -->
      <VetoActionHistory :actions="actions" :teams="[team1, team2]" />

      <!-- Turn Indicator / Action Button -->
      <VetoActionBar
        :is-my-turn="isMyTurn"
        :action-type="currentActionType"
        :deadline="actionDeadline"
        :selected-map="pendingSelection"
        @confirm="confirmAction"
        @cancel="cancelSelection"
      />
    </div>

    <!-- Side Selection Modal -->
    <SideSelectionModal
      v-model="showSideSelection"
      :map="sideSelectionMap"
      :options="sideOptions"
      @select="handleSideSelect"
    />

    <!-- Coin Flip Modal (for admins/first action) -->
    <CoinFlipModal
      v-if="session?.status === 'coin_flip'"
      :teams="[team1, team2]"
      @result="handleCoinFlipResult"
    />
  </div>
</template>

<style scoped>
.veto-page {
  min-height: 100vh;
  background: linear-gradient(180deg, #1a1a2e 0%, #16213e 100%);
}

.veto-content {
  max-width: 1400px;
  margin: 0 auto;
  padding: 20px;
}
</style>
```

#### 3. Component: `src/components/veto/VetoMapGrid.vue`

The core map selection grid:

```vue
<template>
  <div class="map-grid">
    <VetoMapCard
      v-for="map in maps"
      :key="map.id"
      :map="map"
      :state="getMapState(map.id)"
      :selectable="selectable && isMapSelectable(map.id)"
      :selected="pendingSelection === map.id"
      :action-type="currentActionType"
      @click="handleClick(map.id)"
    />
  </div>
</template>

<style scoped>
.map-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 16px;
  padding: 20px;
}
</style>
```

#### 4. Component: `src/components/veto/VetoMapCard.vue`

Individual map card with states:

```vue
<template>
  <div
    class="map-card"
    :class="{
      'map-card--banned': state === 'banned',
      'map-card--picked': state === 'picked',
      'map-card--available': state === 'available',
      'map-card--selectable': selectable,
      'map-card--selected': selected,
    }"
    @click="selectable && $emit('click')"
  >
    <!-- Map Image -->
    <div class="map-card__image">
      <img :src="map.image_url" :alt="map.display_name" />
      <!-- Overlay for banned/picked -->
      <div v-if="state !== 'available'" class="map-card__overlay">
        <v-icon v-if="state === 'banned'" size="64" color="error">mdi-close-circle</v-icon>
        <v-icon v-if="state === 'picked'" size="64" color="success">mdi-check-circle</v-icon>
      </div>
    </div>

    <!-- Map Name -->
    <div class="map-card__name">
      {{ map.display_name }}
    </div>

    <!-- Action indicator when selectable -->
    <div v-if="selectable" class="map-card__action-hint">
      <v-chip :color="actionType === 'ban' ? 'error' : 'success'" size="small">
        {{ actionType === 'ban' ? 'Ban' : 'Pick' }}
      </v-chip>
    </div>

    <!-- Selection ring when selected -->
    <div v-if="selected" class="map-card__selection-ring" />
  </div>
</template>

<style scoped>
.map-card {
  position: relative;
  border-radius: 12px;
  overflow: hidden;
  cursor: default;
  transition: transform 0.2s, box-shadow 0.2s;
}

.map-card--selectable {
  cursor: pointer;
}

.map-card--selectable:hover {
  transform: scale(1.05);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

.map-card--banned {
  opacity: 0.5;
  filter: grayscale(1);
}

.map-card--selected {
  outline: 3px solid var(--v-primary-base);
}

.map-card__overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.6);
}
</style>
```

#### 5. Component: `src/components/veto/VetoActionBar.vue`

Bottom action bar:

```vue
<template>
  <div class="action-bar" :class="{ 'action-bar--my-turn': isMyTurn }">
    <template v-if="isMyTurn">
      <div class="action-bar__timer">
        <CountdownTimer :deadline="deadline" @expired="$emit('timeout')" />
      </div>
      <div class="action-bar__message">
        Your turn to <strong>{{ actionType }}</strong> a map
      </div>
      <div class="action-bar__actions" v-if="selectedMap">
        <v-btn color="error" variant="outlined" @click="$emit('cancel')">
          Cancel
        </v-btn>
        <v-btn :color="actionType === 'ban' ? 'error' : 'success'" @click="$emit('confirm')">
          Confirm {{ actionType }}
        </v-btn>
      </div>
    </template>
    <template v-else>
      <div class="action-bar__waiting">
        <v-progress-circular indeterminate size="20" class="mr-2" />
        Waiting for opponent...
      </div>
    </template>
  </div>
</template>
```

#### 6. Component: `src/components/veto/VetoActionHistory.vue`

Shows history of picks/bans:

```vue
<template>
  <div class="action-history">
    <div class="action-history__title">Veto History</div>
    <div class="action-history__list">
      <div
        v-for="action in actions"
        :key="action.id"
        class="action-item"
        :class="`action-item--${action.action_type}`"
      >
        <span class="action-item__team">
          {{ getTeamName(action.performed_by_registration_id) }}
        </span>
        <span class="action-item__type">
          {{ action.action_type === 'ban' ? 'banned' : 'picked' }}
        </span>
        <span class="action-item__map">
          {{ action.map_id }}
        </span>
      </div>
    </div>
  </div>
</template>
```

#### 7. Component: `src/components/veto/SideSelectionModal.vue`

Side selection after pick:

```vue
<template>
  <v-dialog :model-value="modelValue" persistent max-width="500">
    <v-card>
      <v-card-title>Choose Your Side</v-card-title>
      <v-card-subtitle>
        Select which side to start on {{ map?.display_name }}
      </v-card-subtitle>
      <v-card-text>
        <v-row>
          <v-col v-for="option in options" :key="option.id" cols="6">
            <v-card
              :color="selected === option.id ? 'primary' : undefined"
              @click="selected = option.id"
              hover
            >
              <v-card-text class="text-center">
                <v-icon size="48">{{ getSideIcon(option.id) }}</v-icon>
                <div class="text-h6">{{ option.display_name }}</div>
              </v-card-text>
            </v-card>
          </v-col>
        </v-row>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn color="primary" :disabled="!selected" @click="confirm">
          Confirm
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
```

### Acceptance Criteria (F3.7)

- [ ] Map grid displays correctly
- [ ] Turn indicator shows whose turn
- [ ] Countdown timer works
- [ ] Map selection highlights correctly
- [ ] Ban/pick actions work via API
- [ ] Real-time updates from WebSocket
- [ ] Side selection modal works
- [ ] Action history displays
- [ ] Session completes correctly

---

## Sub-Phase F3.8: Result Submission

### Scope

Implement the result claim and confirmation workflow.

### Deliverables

#### 1. Pinia Store: `src/stores/results.ts`

```typescript
export const useResultsStore = defineStore('results', () => {
  const currentClaim = ref<ResultClaim | null>(null)
  const claimHistory = ref<ResultClaim[]>([])
  const loading = ref(false)

  async function submitClaim(matchId: string, claim: SubmitResultClaimRequest) { /* ... */ }
  async function confirmClaim(matchId: string, claimId: string) { /* ... */ }
  async function disputeClaim(matchId: string, claimId: string, reason: string) { /* ... */ }
  async function fetchCurrentClaim(matchId: string) { /* ... */ }
  async function fetchClaimHistory(matchId: string) { /* ... */ }

  return { /* ... */ }
})
```

#### 2. Component: `src/components/match/ResultSubmissionDialog.vue`

Full result submission form:

```vue
<template>
  <v-dialog :model-value="modelValue" max-width="700" persistent>
    <v-card>
      <v-card-title>Submit Match Result</v-card-title>
      <v-card-text>
        <!-- Winner Selection -->
        <v-radio-group v-model="winnerId" label="Who won?">
          <v-radio :value="match.participant1_registration_id">
            <template #label>
              <ParticipantLabel :participant="participant1" />
            </template>
          </v-radio>
          <v-radio :value="match.participant2_registration_id">
            <template #label>
              <ParticipantLabel :participant="participant2" />
            </template>
          </v-radio>
        </v-radio-group>

        <!-- Series Score -->
        <v-row class="mt-4">
          <v-col cols="5">
            <v-text-field
              v-model.number="participant1Score"
              type="number"
              min="0"
              label="Score"
              :rules="[v => v >= 0 || 'Must be positive']"
            />
          </v-col>
          <v-col cols="2" class="d-flex align-center justify-center">
            <span class="text-h5">-</span>
          </v-col>
          <v-col cols="5">
            <v-text-field
              v-model.number="participant2Score"
              type="number"
              min="0"
              label="Score"
              :rules="[v => v >= 0 || 'Must be positive']"
            />
          </v-col>
        </v-row>

        <!-- Game-by-Game Results (for BO3/BO5) -->
        <template v-if="requiresGameResults">
          <v-divider class="my-4" />
          <div class="text-subtitle-1 mb-2">Game-by-Game Results</div>
          <GameResultInput
            v-for="(game, index) in gameResults"
            :key="index"
            v-model="gameResults[index]"
            :game-number="index + 1"
            :maps="selectedMaps"
            :participant1="participant1"
            :participant2="participant2"
          />
          <v-btn variant="text" @click="addGame" :disabled="gameResults.length >= maxGames">
            <v-icon start>mdi-plus</v-icon>
            Add Game
          </v-btn>
        </template>

        <!-- Notes -->
        <v-textarea
          v-model="notes"
          label="Notes (optional)"
          placeholder="Any additional information about the match..."
          rows="2"
          class="mt-4"
        />
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="$emit('update:modelValue', false)">
          Cancel
        </v-btn>
        <v-btn color="primary" :loading="loading" :disabled="!isValid" @click="submit">
          Submit Result
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
```

#### 3. Component: `src/components/match/GameResultInput.vue`

Single game result row:

```vue
<template>
  <v-card variant="outlined" class="mb-2">
    <v-card-text>
      <v-row align="center">
        <v-col cols="12" sm="4">
          <v-select
            v-model="localValue.map_id"
            :items="maps"
            item-title="display_name"
            item-value="id"
            label="Map"
            density="compact"
          />
        </v-col>
        <v-col cols="5" sm="3">
          <v-text-field
            v-model.number="localValue.participant1_score"
            type="number"
            min="0"
            :label="participant1.name"
            density="compact"
          />
        </v-col>
        <v-col cols="2" class="text-center">
          <span>-</span>
        </v-col>
        <v-col cols="5" sm="3">
          <v-text-field
            v-model.number="localValue.participant2_score"
            type="number"
            min="0"
            :label="participant2.name"
            density="compact"
          />
        </v-col>
      </v-row>
    </v-card-text>
  </v-card>
</template>
```

#### 4. Component: `src/components/match/ResultConfirmationCard.vue`

Shows pending result for confirmation:

```vue
<template>
  <v-card>
    <v-card-title>Result Pending Confirmation</v-card-title>
    <v-card-subtitle>
      Submitted by {{ claim.submitted_by_name }} • {{ formatTimeAgo(claim.created_at) }}
    </v-card-subtitle>
    <v-card-text>
      <!-- Claimed Result -->
      <div class="result-display">
        <div class="result-team" :class="{ winner: isWinner(participant1) }">
          <ParticipantLabel :participant="participant1" />
          <span class="score">{{ claim.claimed_participant1_score }}</span>
        </div>
        <div class="result-vs">vs</div>
        <div class="result-team" :class="{ winner: isWinner(participant2) }">
          <span class="score">{{ claim.claimed_participant2_score }}</span>
          <ParticipantLabel :participant="participant2" />
        </div>
      </div>

      <!-- Game Results -->
      <div v-if="claim.game_results.length" class="mt-4">
        <GameResultRow
          v-for="game in claim.game_results"
          :key="game.game_number"
          :game="game"
        />
      </div>

      <!-- Auto-confirm Timer -->
      <v-alert v-if="claim.auto_confirm_at" type="info" variant="tonal" class="mt-4">
        Will auto-confirm in <CountdownTimer :deadline="claim.auto_confirm_at" />
      </v-alert>
    </v-card-text>

    <v-card-actions v-if="canRespond">
      <v-btn color="error" variant="outlined" @click="showDisputeDialog = true">
        <v-icon start>mdi-alert-circle</v-icon>
        Dispute
      </v-btn>
      <v-spacer />
      <v-btn color="success" @click="handleConfirm">
        <v-icon start>mdi-check</v-icon>
        Confirm Result
      </v-btn>
    </v-card-actions>

    <!-- Dispute Dialog -->
    <DisputeDialog
      v-model="showDisputeDialog"
      @dispute="handleDispute"
    />
  </v-card>
</template>
```

#### 5. Component: `src/components/match/DisputeDialog.vue`

Dispute reason input:

```vue
<template>
  <v-dialog :model-value="modelValue" max-width="500">
    <v-card>
      <v-card-title>Dispute Result</v-card-title>
      <v-card-text>
        <v-textarea
          v-model="reason"
          label="Reason for dispute"
          placeholder="Explain why the submitted result is incorrect..."
          :rules="[v => v.length >= 10 || 'Please provide a detailed reason']"
          rows="4"
        />
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="$emit('update:modelValue', false)">
          Cancel
        </v-btn>
        <v-btn color="error" :disabled="reason.length < 10" @click="submit">
          Submit Dispute
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
```

### Acceptance Criteria (F3.8)

- [ ] Result submission form works
- [ ] Game-by-game input for series
- [ ] Score validation works
- [ ] Pending claim displays correctly
- [ ] Confirm/dispute actions work
- [ ] Auto-confirm countdown shows
- [ ] Real-time updates on confirmation

---

## Implementation Guidelines

### Real-Time Considerations

1. **Debounce reconnection**: Don't spam reconnect attempts
2. **Optimistic updates**: Update UI immediately, reconcile on server response
3. **Error recovery**: Show clear error states and retry options
4. **Offline handling**: Queue actions when offline, replay on reconnect

### Performance

1. **Lazy load veto page**: Large assets (map images)
2. **Virtualize action history**: For long veto sessions
3. **Memoize computeds**: Expensive state derivations

### Accessibility

1. **Keyboard navigation**: Tab through maps, Enter to select
2. **Screen reader**: Announce turn changes, time warnings
3. **Focus management**: Focus map grid on turn start
4. **Color + icons**: Don't rely on color alone for status

### File Organization

```
src/
├── components/
│   ├── match/
│   │   ├── CountdownTimer.vue
│   │   ├── DisputeDialog.vue
│   │   ├── GameResultInput.vue
│   │   ├── MatchActionPanel.vue
│   │   ├── MatchLobbyHeader.vue
│   │   ├── MatchStatusCenter.vue
│   │   ├── ParticipantCard.vue
│   │   ├── ResultConfirmationCard.vue
│   │   └── ResultSubmissionDialog.vue
│   └── veto/
│       ├── CoinFlipModal.vue
│       ├── SideSelectionModal.vue
│       ├── VetoActionBar.vue
│       ├── VetoActionHistory.vue
│       ├── VetoHeader.vue
│       ├── VetoMapCard.vue
│       ├── VetoMapGrid.vue
│       └── VetoTeamPanel.vue
├── pages/
│   ├── MatchLobbyPage.vue
│   └── VetoPage.vue
├── services/
│   └── websocket.ts
└── stores/
    ├── match.ts
    ├── results.ts
    └── veto.ts
```

---

## Verification Checklist

### Sub-Phase F3.5
- [ ] WebSocket connects
- [ ] Events route to stores
- [ ] Reconnection works

### Sub-Phase F3.6
- [ ] Lobby displays participants
- [ ] Check-in works
- [ ] Status updates live

### Sub-Phase F3.7
- [ ] Map grid renders
- [ ] Turn indicator works
- [ ] Actions submit correctly
- [ ] Live updates work
- [ ] Session completes

### Sub-Phase F3.8
- [ ] Submit result works
- [ ] Confirm/dispute works
- [ ] Auto-confirm countdown shows

---

## Output

After completing this batch:

1. Test full match flow (check-in → veto → result)
2. Test WebSocket reconnection
3. Test on mobile viewport
4. Document any backend WebSocket requirements

**Proceed to Frontend Batch 3 (Admin Tools) after this batch is complete.**
