# Bracket Progression Design

> **Sub-Phase**: 3.9 (Bracket Progression)
> **Related**: [04-result-submission.md](./04-result-submission.md), [08-sagas-orchestration.md](./08-sagas-orchestration.md)

---

## Overview

Bracket Progression handles the automatic advancement of winners through tournament brackets and updates standings for non-elimination formats. This is a **saga-based** operation because it involves multiple coordinated updates that must be consistent.

### Key Features

- **Winner Advancement**: Automatically place winners in next matches
- **Loser Routing**: Route losers to losers bracket (double elimination)
- **Standings Updates**: Calculate standings for round robin and swiss
- **Completion Detection**: Detect when brackets/tournaments are complete
- **Rollback Support**: Handle dispute resolutions that change winners

---

## Progression Rules by Format

### Single Elimination

```
Match Result → Winner advances to winner_progresses_to match
            → Loser is eliminated (registration status → Eliminated)
```

### Double Elimination

```
Match Result (Winners Bracket)
    → Winner advances to winner_progresses_to match
    → Loser drops to loser_progresses_to match (Losers Bracket)

Match Result (Losers Bracket)
    → Winner advances to winner_progresses_to match
    → Loser is eliminated

Match Result (Grand Final)
    → If Winners Bracket winner wins: Tournament complete
    → If Losers Bracket winner wins: Grand Final Reset triggered
```

### Round Robin

```
Match Result → Update standings for both participants
            → Calculate new positions based on:
              1. Points (win=3, draw=1, loss=0)
              2. Head-to-head
              3. Game differential
              4. Games won
```

### Swiss

```
Match Result → Update standings
            → After round complete:
              → Generate next round pairings
              → Avoid rematches
              → Use Buchholz score for tiebreakers
```

---

## Database Schema

### tournament_standings (already exists, extend)

```sql
-- Add additional fields for standings
ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    head_to_head JSONB NOT NULL DEFAULT '{}';

ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    tiebreaker_score DECIMAL(10,4) NOT NULL DEFAULT 0;

ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    is_tied BOOLEAN NOT NULL DEFAULT false;

-- Index for efficient standings queries
CREATE INDEX idx_tournament_standings_position_points
    ON tournament_standings(bracket_id, points DESC, tiebreaker_score DESC);
```

### progression_log

```sql
CREATE TABLE progression_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Source match
    source_match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Target match (where winner/loser went)
    target_match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,

    -- Participant that advanced
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id),

    -- Type of progression
    progression_type VARCHAR(32) NOT NULL,

    -- Position in target match (1 or 2)
    target_position INTEGER,

    -- Saga reference
    saga_id UUID,

    -- Timestamps
    progressed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT progression_log_check_type CHECK (progression_type IN (
        'winner_advance', 'loser_drop', 'loser_eliminate', 'bye_advance'
    )),
    CONSTRAINT progression_log_check_position CHECK (target_position IN (1, 2))
);

CREATE INDEX idx_progression_log_source ON progression_log(source_match_id);
CREATE INDEX idx_progression_log_target ON progression_log(target_match_id);
CREATE INDEX idx_progression_log_saga ON progression_log(saga_id);

COMMENT ON TABLE progression_log IS 'Log of bracket progression events';
```

---

## Domain Entities

### Standing

```rust
use portal_core::ids::{TournamentBracketId, TournamentRegistrationId, TournamentStandingId};

/// Standing entry for a participant in a bracket.
#[derive(Debug, Clone)]
pub struct Standing {
    pub id: TournamentStandingId,
    pub bracket_id: TournamentBracketId,
    pub registration_id: TournamentRegistrationId,

    /// Current position (1 = first place)
    pub position: i32,

    /// Match statistics
    pub matches_played: i32,
    pub matches_won: i32,
    pub matches_lost: i32,
    pub matches_drawn: i32,

    /// Game statistics (for tiebreakers)
    pub game_wins: i32,
    pub game_losses: i32,
    pub game_differential: i32,

    /// Points (round robin: W=3, D=1, L=0)
    pub points: i32,

    /// Tiebreaker scores
    pub buchholz_score: Option<f64>,
    pub opponent_match_wins: Option<f64>,
    pub head_to_head: HeadToHead,
    pub tiebreaker_score: f64,

    /// Is this position tied with others?
    pub is_tied: bool,

    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeadToHead {
    /// Map of opponent registration_id -> (wins, losses, draws)
    pub records: HashMap<TournamentRegistrationId, (i32, i32, i32)>,
}

impl Standing {
    /// Calculate tiebreaker score for sorting.
    pub fn calculate_tiebreaker(&self, format: &BracketFormat) -> f64 {
        match format {
            BracketFormat::RoundRobin => {
                // Points * 10000 + game_diff * 100 + game_wins
                (self.points as f64 * 10000.0)
                    + (self.game_differential as f64 * 100.0)
                    + (self.game_wins as f64)
            }
            BracketFormat::Swiss => {
                // Points * 10000 + buchholz * 100 + opponent_match_wins
                (self.points as f64 * 10000.0)
                    + (self.buchholz_score.unwrap_or(0.0) * 100.0)
                    + (self.opponent_match_wins.unwrap_or(0.0))
            }
            _ => self.points as f64,
        }
    }
}
```

### ProgressionResult

```rust
/// Result of a progression operation.
#[derive(Debug, Clone)]
pub struct ProgressionResult {
    /// The match that was completed
    pub completed_match_id: TournamentMatchId,

    /// Winner advancement info
    pub winner_advancement: Option<Advancement>,

    /// Loser routing info (double elim) or elimination
    pub loser_result: LoserResult,

    /// Updated standings (for RR/Swiss)
    pub updated_standings: Vec<Standing>,

    /// Whether bracket is now complete
    pub bracket_complete: bool,

    /// Whether tournament is now complete
    pub tournament_complete: bool,

    /// Next matches that became Ready
    pub newly_ready_matches: Vec<TournamentMatchId>,
}

#[derive(Debug, Clone)]
pub struct Advancement {
    pub registration_id: TournamentRegistrationId,
    pub to_match_id: TournamentMatchId,
    pub position: ParticipantPosition,
}

#[derive(Debug, Clone)]
pub enum LoserResult {
    Eliminated(TournamentRegistrationId),
    DroppedToLosers {
        registration_id: TournamentRegistrationId,
        to_match_id: TournamentMatchId,
        position: ParticipantPosition,
    },
    None,  // For formats without elimination
}

#[derive(Debug, Clone, Copy)]
pub enum ParticipantPosition {
    Participant1,
    Participant2,
}
```

---

## Service Design

### ProgressionService

```rust
pub struct ProgressionService<TMR, TBR, TSR, TRR, TSTR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TSR: TournamentStageRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingRepository,
{
    match_repo: Arc<TMR>,
    bracket_repo: Arc<TBR>,
    stage_repo: Arc<TSR>,
    registration_repo: Arc<TRR>,
    standing_repo: Arc<TSTR>,
}

impl<TMR, TBR, TSR, TRR, TSTR> ProgressionService<TMR, TBR, TSR, TRR, TSTR>
where
    TMR: TournamentMatchRepository,
    TBR: TournamentBracketRepository,
    TSR: TournamentStageRepository,
    TRR: TournamentRegistrationRepository,
    TSTR: TournamentStandingRepository,
{
    /// Process match completion and advance winner.
    ///
    /// This is the main entry point called when a match completes.
    /// It orchestrates all progression logic based on bracket type.
    ///
    /// **NOTE**: This should be called within a saga for consistency.
    pub async fn process_match_completion(
        &self,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        loser_registration_id: TournamentRegistrationId,
    ) -> Result<ProgressionResult, DomainError>;

    /// Advance winner to their next match.
    async fn advance_winner(
        &self,
        source_match: &TournamentMatch,
        winner_registration_id: TournamentRegistrationId,
    ) -> Result<Option<Advancement>, DomainError>;

    /// Route loser (drop to losers bracket or eliminate).
    async fn route_loser(
        &self,
        source_match: &TournamentMatch,
        loser_registration_id: TournamentRegistrationId,
    ) -> Result<LoserResult, DomainError>;

    /// Update standings for round robin or swiss.
    async fn update_standings(
        &self,
        bracket_id: TournamentBracketId,
        match_: &TournamentMatch,
        winner_registration_id: TournamentRegistrationId,
    ) -> Result<Vec<Standing>, DomainError>;

    /// Check if bracket is complete.
    async fn check_bracket_completion(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<bool, DomainError>;

    /// Check if tournament is complete.
    async fn check_tournament_completion(
        &self,
        tournament_id: TournamentId,
    ) -> Result<bool, DomainError>;

    /// Find matches that are now ready (both participants set).
    async fn find_newly_ready_matches(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatchId>, DomainError>;

    // --- Rollback Operations (for disputes) ---

    /// Revert a progression (remove winner from next match).
    ///
    /// Used when a dispute overturns a result.
    pub async fn revert_progression(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError>;

    /// Re-apply progression with a different winner.
    ///
    /// Used after dispute resolution.
    pub async fn reapply_progression(
        &self,
        match_id: TournamentMatchId,
        new_winner_registration_id: TournamentRegistrationId,
    ) -> Result<ProgressionResult, DomainError>;
}
```

### StandingsService

```rust
pub struct StandingsService<TSTR, TMR, TRR>
where
    TSTR: TournamentStandingRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    standing_repo: Arc<TSTR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
}

impl<TSTR, TMR, TRR> StandingsService<TSTR, TMR, TRR>
where
    TSTR: TournamentStandingRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Initialize standings for a bracket.
    ///
    /// Called when bracket is created.
    pub async fn initialize_standings(
        &self,
        bracket_id: TournamentBracketId,
        registrations: &[TournamentRegistration],
    ) -> Result<Vec<Standing>, DomainError>;

    /// Update standings after a match result.
    pub async fn update_for_match_result(
        &self,
        bracket_id: TournamentBracketId,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
        winner_games: i32,
        loser_games: i32,
        is_draw: bool,
    ) -> Result<Vec<Standing>, DomainError>;

    /// Recalculate all standings for a bracket.
    ///
    /// Used after dispute resolution or data correction.
    pub async fn recalculate_standings(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<Standing>, DomainError>;

    /// Get current standings for a bracket.
    pub async fn get_standings(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<Standing>, DomainError>;

    /// Calculate Buchholz score for Swiss format.
    async fn calculate_buchholz(
        &self,
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
    ) -> Result<f64, DomainError>;

    /// Sort and assign positions based on standings rules.
    async fn assign_positions(
        &self,
        standings: &mut Vec<Standing>,
        format: BracketFormat,
    ) -> Result<(), DomainError>;
}
```

---

## Progression Logic

### Single Elimination

```rust
impl ProgressionService {
    async fn process_single_elim(
        &self,
        match_: &TournamentMatch,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<ProgressionResult, DomainError> {
        let mut result = ProgressionResult::default();

        // 1. Advance winner
        if let Some(next_match_id) = match_.winner_progresses_to {
            let next_match = self.match_repo.find_by_id(next_match_id).await?
                .ok_or(DomainError::MatchNotFound(next_match_id))?;

            let position = self.determine_position(&next_match, match_.id)?;
            self.set_participant(&next_match, position, winner_id).await?;

            result.winner_advancement = Some(Advancement {
                registration_id: winner_id,
                to_match_id: next_match_id,
                position,
            });
        }

        // 2. Eliminate loser
        self.registration_repo.update_status(loser_id, RegistrationStatus::Eliminated).await?;
        result.loser_result = LoserResult::Eliminated(loser_id);

        // 3. Check if bracket complete
        result.bracket_complete = self.check_bracket_completion(match_.bracket_id).await?;

        // 4. Find newly ready matches
        if result.winner_advancement.is_some() {
            result.newly_ready_matches = self.find_newly_ready_matches(match_.bracket_id).await?;
        }

        Ok(result)
    }

    fn determine_position(
        &self,
        next_match: &TournamentMatch,
        source_match_id: TournamentMatchId,
    ) -> Result<ParticipantPosition, DomainError> {
        // Check participant1_source and participant2_source
        if let Some(source) = &next_match.participant1_source {
            if source.source_match_id == Some(source_match_id) {
                return Ok(ParticipantPosition::Participant1);
            }
        }
        if let Some(source) = &next_match.participant2_source {
            if source.source_match_id == Some(source_match_id) {
                return Ok(ParticipantPosition::Participant2);
            }
        }
        Err(DomainError::ProgressionSourceNotFound)
    }
}
```

### Double Elimination

```rust
impl ProgressionService {
    async fn process_double_elim(
        &self,
        match_: &TournamentMatch,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
        bracket: &TournamentBracket,
    ) -> Result<ProgressionResult, DomainError> {
        let mut result = ProgressionResult::default();

        // 1. Advance winner
        if let Some(next_match_id) = match_.winner_progresses_to {
            // ... same as single elim
        }

        // 2. Handle loser based on bracket type
        match bracket.bracket_type {
            BracketType::Winners => {
                // Drop to losers bracket
                if let Some(losers_match_id) = match_.loser_progresses_to {
                    let losers_match = self.match_repo.find_by_id(losers_match_id).await?
                        .ok_or(DomainError::MatchNotFound(losers_match_id))?;

                    let position = self.determine_position(&losers_match, match_.id)?;
                    self.set_participant(&losers_match, position, loser_id).await?;

                    result.loser_result = LoserResult::DroppedToLosers {
                        registration_id: loser_id,
                        to_match_id: losers_match_id,
                        position,
                    };
                }
            }
            BracketType::Losers | BracketType::GrandFinal => {
                // Eliminated
                self.registration_repo.update_status(loser_id, RegistrationStatus::Eliminated).await?;
                result.loser_result = LoserResult::Eliminated(loser_id);
            }
            _ => {}
        }

        // 3. Check for Grand Final Reset
        if bracket.bracket_type == BracketType::GrandFinal {
            // If loser bracket winner won, trigger reset
            // (This is detected by checking if winner came from losers bracket)
            if self.is_from_losers_bracket(match_, winner_id).await? {
                // Grand Final Reset match should already exist
                // Just check completion differently
            }
        }

        // 4. Check completion
        result.bracket_complete = self.check_bracket_completion(match_.bracket_id).await?;

        Ok(result)
    }
}
```

### Round Robin Standings

```rust
impl StandingsService {
    async fn update_for_match_result(
        &self,
        bracket_id: TournamentBracketId,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
        winner_games: i32,
        loser_games: i32,
        is_draw: bool,
    ) -> Result<Vec<Standing>, DomainError> {
        // Get current standings
        let mut winner_standing = self.standing_repo
            .find_by_bracket_and_registration(bracket_id, winner_id)
            .await?
            .ok_or(DomainError::StandingNotFound)?;

        let mut loser_standing = self.standing_repo
            .find_by_bracket_and_registration(bracket_id, loser_id)
            .await?
            .ok_or(DomainError::StandingNotFound)?;

        // Update match stats
        winner_standing.matches_played += 1;
        loser_standing.matches_played += 1;

        if is_draw {
            winner_standing.matches_drawn += 1;
            loser_standing.matches_drawn += 1;
            winner_standing.points += 1;
            loser_standing.points += 1;
        } else {
            winner_standing.matches_won += 1;
            loser_standing.matches_lost += 1;
            winner_standing.points += 3;
        }

        // Update game stats
        winner_standing.game_wins += winner_games;
        winner_standing.game_losses += loser_games;
        winner_standing.game_differential += winner_games - loser_games;

        loser_standing.game_wins += loser_games;
        loser_standing.game_losses += winner_games;
        loser_standing.game_differential += loser_games - winner_games;

        // Update head-to-head
        winner_standing.head_to_head.records
            .entry(loser_id)
            .and_modify(|(w, l, d)| {
                if is_draw { *d += 1 } else { *w += 1 }
            })
            .or_insert(if is_draw { (0, 0, 1) } else { (1, 0, 0) });

        loser_standing.head_to_head.records
            .entry(winner_id)
            .and_modify(|(w, l, d)| {
                if is_draw { *d += 1 } else { *l += 1 }
            })
            .or_insert(if is_draw { (0, 0, 1) } else { (0, 1, 0) });

        // Save updates
        self.standing_repo.update(&winner_standing).await?;
        self.standing_repo.update(&loser_standing).await?;

        // Recalculate positions for all standings
        let mut all_standings = self.standing_repo.find_by_bracket(bracket_id).await?;
        self.assign_positions(&mut all_standings, BracketFormat::RoundRobin).await?;

        Ok(all_standings)
    }
}
```

---

## API Endpoints

### GET /v1/tournaments/{tournament_id}/brackets/{bracket_id}/matches

Get bracket matches with progression info.

**Response**:
```json
{
  "data": {
    "bracket_id": "...",
    "bracket_type": "single_elim",
    "status": "active",
    "rounds": [
      {
        "round": 1,
        "name": "Round of 16",
        "matches": [
          {
            "id": "...",
            "position": "W1-1",
            "participant1": {"name": "Team A", "seed": 1},
            "participant2": {"name": "Team H", "seed": 8},
            "scores": [2, 1],
            "winner": "Team A",
            "status": "completed",
            "winner_progresses_to": "W2-1"
          }
        ]
      }
    ],
    "progression_complete": false
  }
}
```

### GET /v1/tournaments/{tournament_id}/standings

Get standings for all brackets.

**Response**:
```json
{
  "data": {
    "brackets": [
      {
        "bracket_id": "...",
        "bracket_name": "Group A",
        "format": "round_robin",
        "standings": [
          {
            "position": 1,
            "registration_id": "...",
            "name": "Team Alpha",
            "matches_played": 3,
            "matches_won": 3,
            "matches_lost": 0,
            "game_differential": 6,
            "points": 9,
            "is_tied": false
          },
          {
            "position": 2,
            "registration_id": "...",
            "name": "Team Beta",
            "matches_played": 3,
            "matches_won": 2,
            "matches_lost": 1,
            "game_differential": 2,
            "points": 6,
            "is_tied": true
          }
        ]
      }
    ]
  }
}
```

### GET /v1/tournaments/{tournament_id}/brackets/{bracket_id}/standings

Get standings for a specific bracket.

### POST /v1/admin/tournaments/{tournament_id}/brackets/{bracket_id}/recalculate-standings

Admin endpoint to recalculate standings.

---

## Tournament Completion

### Detection Logic

```rust
impl ProgressionService {
    async fn check_tournament_completion(
        &self,
        tournament_id: TournamentId,
    ) -> Result<bool, DomainError> {
        // Get all stages
        let stages = self.stage_repo.find_by_tournament(tournament_id).await?;

        // Check each stage in order
        for stage in stages {
            if stage.status != StageStatus::Completed {
                // Check if all brackets in this stage are complete
                let brackets = self.bracket_repo.find_by_stage(stage.id).await?;

                for bracket in brackets {
                    if !self.is_bracket_complete(&bracket).await? {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    async fn is_bracket_complete(&self, bracket: &TournamentBracket) -> Result<bool, DomainError> {
        match bracket.bracket_type {
            BracketType::SingleElim | BracketType::Winners | BracketType::Losers => {
                // Check if final match is completed
                let final_match = self.match_repo
                    .find_final_match(bracket.id)
                    .await?;

                Ok(final_match.is_some_and(|m| m.status == MatchStatus::Completed))
            }
            BracketType::GrandFinal => {
                // Grand final may have reset, check latest match
                let gf_matches = self.match_repo
                    .find_grand_final_matches(bracket.id)
                    .await?;

                Ok(gf_matches.iter().any(|m| {
                    m.status == MatchStatus::Completed &&
                    // Winner is from winners bracket OR this is reset match
                    (self.is_from_winners_bracket(m, m.winner_registration_id.unwrap())
                        || m.bracket_position == "GF-R")
                }))
            }
            BracketType::RoundRobin => {
                // All matches completed
                let matches = self.match_repo.find_by_bracket(bracket.id).await?;
                Ok(matches.iter().all(|m| m.status == MatchStatus::Completed))
            }
            BracketType::Swiss => {
                // All required rounds completed
                let current_round = bracket.current_round;
                let total_rounds = bracket.total_rounds;
                Ok(current_round >= total_rounds)
            }
        }
    }
}
```

---

## Error Handling

### New Error Types

```rust
pub enum ProgressionError {
    /// Match not completed
    MatchNotCompleted(TournamentMatchId),

    /// No progression target defined
    NoProgressionTarget(TournamentMatchId),

    /// Target match position already filled
    PositionAlreadyFilled {
        match_id: TournamentMatchId,
        position: ParticipantPosition,
    },

    /// Cannot determine position for advancement
    ProgressionSourceNotFound,

    /// Standing not found
    StandingNotFound {
        bracket_id: TournamentBracketId,
        registration_id: TournamentRegistrationId,
    },

    /// Bracket already complete
    BracketAlreadyComplete(TournamentBracketId),
}
```

---

## Testing Notes

### Unit Tests

- Position determination logic
- Standing calculation formulas
- Tiebreaker ordering
- Completion detection

### Integration Tests

```
test_single_elim_progression
test_double_elim_winner_advance
test_double_elim_loser_drop
test_double_elim_grand_final_reset
test_round_robin_standings_update
test_swiss_buchholz_calculation
test_tournament_completion_detection
test_progression_revert
test_progression_reapply
```

### Edge Case Tests

```
test_bye_progression
test_tied_standings
test_head_to_head_tiebreaker
test_cascade_progression_update
```
