# Result Submission Design

> **Sub-Phase**: 3.6 (Result Submission)
> **Related**: [03-match-lifecycle.md](./03-match-lifecycle.md), [05-evidence-system.md](./05-evidence-system.md)

---

## Overview

The Result Submission system handles how match results are reported and confirmed. It uses a claim/confirm workflow where one team submits a result and the opponent confirms, disputes, or lets it auto-confirm after a timeout.

### Key Features

- **Claim/Confirm Workflow**: Prevents disputes by requiring opponent acknowledgment
- **Auto-Confirmation Timeout**: Results auto-confirm if opponent doesn't respond
- **Game-by-Game Results**: Series matches (Bo3, Bo5) track individual game scores
- **Evidence Linking**: Results can be linked to evidence (demos, screenshots)
- **Score Validation**: Plugin-based validation for game-specific rules

---

## Result Workflow

```
                                    ┌─────────────────────┐
                                    │ Match: AwaitingResult│
                                    └──────────┬──────────┘
                                               │
                                               │ Either team submits
                                               ▼
                                    ┌─────────────────────┐
                                    │   Result Claim      │
                                    │   Status: Pending   │
                                    └──────────┬──────────┘
                                               │
              ┌────────────────────────────────┼────────────────────────────────┐
              │                                │                                │
              ▼                                ▼                                ▼
    ┌─────────────────┐             ┌─────────────────┐             ┌─────────────────┐
    │    Opponent     │             │    Opponent     │             │   Timeout       │
    │   Confirms      │             │   Disputes      │             │  (auto-confirm) │
    └────────┬────────┘             └────────┬────────┘             └────────┬────────┘
             │                               │                               │
             ▼                               ▼                               ▼
    ┌─────────────────┐             ┌─────────────────┐             ┌─────────────────┐
    │  Result Claim   │             │  Result Claim   │             │  Result Claim   │
    │ Status: Confirmed│            │ Status: Disputed│             │Status: Confirmed│
    └────────┬────────┘             └────────┬────────┘             └────────┬────────┘
             │                               │                               │
             ▼                               ▼                               ▼
    ┌─────────────────┐             ┌─────────────────┐             ┌─────────────────┐
    │ Match: Completed │             │ Match: Disputed │             │ Match: Completed│
    │                 │             │ (Admin review)   │             │                 │
    └─────────────────┘             └─────────────────┘             └─────────────────┘
```

---

## Database Schema

### result_claims

```sql
CREATE TABLE result_claims (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Who submitted
    submitted_by_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    submitted_by_user_id UUID NOT NULL REFERENCES users(id),

    -- Claimed result
    claimed_winner_registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    claimed_participant1_score INTEGER NOT NULL,
    claimed_participant2_score INTEGER NOT NULL,

    -- Game-by-game results (for series)
    game_results JSONB NOT NULL DEFAULT '[]',

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',

    -- Confirmation
    confirmed_at TIMESTAMPTZ,
    confirmed_by_registration_id UUID REFERENCES tournament_registrations(id),
    confirmed_by_user_id UUID REFERENCES users(id),

    -- Auto-confirmation
    auto_confirm_at TIMESTAMPTZ,
    was_auto_confirmed BOOLEAN NOT NULL DEFAULT false,

    -- Evidence links
    evidence_ids UUID[] NOT NULL DEFAULT '{}',

    -- Notes
    submitter_notes TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT result_claims_check_status CHECK (status IN (
        'pending', 'confirmed', 'disputed', 'superseded', 'cancelled'
    )),
    CONSTRAINT result_claims_scores_non_negative CHECK (
        claimed_participant1_score >= 0 AND
        claimed_participant2_score >= 0
    )
);

CREATE INDEX idx_result_claims_match ON result_claims(match_id);
CREATE INDEX idx_result_claims_status ON result_claims(status);
CREATE INDEX idx_result_claims_auto_confirm ON result_claims(auto_confirm_at)
    WHERE status = 'pending' AND auto_confirm_at IS NOT NULL;

CREATE TRIGGER result_claims_updated_at
    BEFORE UPDATE ON result_claims
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE result_claims IS 'Result submission claims awaiting confirmation';
```

### game_results JSON structure

```json
{
  "game_results": [
    {
      "game_number": 1,
      "map_id": "de_mirage",
      "participant1_score": 16,
      "participant2_score": 12,
      "winner_registration_id": "...",
      "started_at": "2025-01-15T19:05:00Z",
      "completed_at": "2025-01-15T19:45:00Z",
      "duration_seconds": 2400,
      "evidence_ids": ["..."]
    },
    {
      "game_number": 2,
      "map_id": "de_inferno",
      "participant1_score": 10,
      "participant2_score": 16,
      "winner_registration_id": "...",
      "started_at": "2025-01-15T19:50:00Z",
      "completed_at": "2025-01-15T20:35:00Z",
      "duration_seconds": 2700,
      "evidence_ids": []
    }
  ]
}
```

---

## Domain Entities

### ResultClaim

```rust
use chrono::{DateTime, Utc};
use portal_core::ids::{
    EvidenceId, ResultClaimId, TournamentMatchId, TournamentRegistrationId, UserId,
};

/// A result claim for a match.
#[derive(Debug, Clone)]
pub struct ResultClaim {
    pub id: ResultClaimId,
    pub match_id: TournamentMatchId,

    /// Who submitted the claim
    pub submitted_by_registration_id: TournamentRegistrationId,
    pub submitted_by_user_id: UserId,

    /// Claimed result
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub claimed_participant1_score: i32,
    pub claimed_participant2_score: i32,

    /// Game-by-game results (for series)
    pub game_results: Vec<GameResult>,

    /// Current status
    pub status: ClaimStatus,

    /// Confirmation info
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirmed_by_registration_id: Option<TournamentRegistrationId>,
    pub confirmed_by_user_id: Option<UserId>,

    /// Auto-confirmation
    pub auto_confirm_at: Option<DateTime<Utc>>,
    pub was_auto_confirmed: bool,

    /// Evidence links
    pub evidence_ids: Vec<EvidenceId>,

    /// Notes
    pub submitter_notes: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    /// Awaiting opponent confirmation
    Pending,
    /// Confirmed by opponent or auto-confirmed
    Confirmed,
    /// Disputed by opponent
    Disputed,
    /// Superseded by a newer claim
    Superseded,
    /// Cancelled by submitter
    Cancelled,
}

impl ClaimStatus {
    pub fn can_transition_to(&self, target: ClaimStatus) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Confirmed)
            | (Self::Pending, Self::Disputed)
            | (Self::Pending, Self::Superseded)
            | (Self::Pending, Self::Cancelled)
        )
    }
}
```

### GameResult

```rust
/// Result for a single game in a series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameResult {
    pub game_number: i32,
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub winner_registration_id: TournamentRegistrationId,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub evidence_ids: Vec<EvidenceId>,
}

impl GameResult {
    /// Validate the game result.
    pub fn validate(&self, match_format: &MatchFormat) -> Result<(), String> {
        // Scores must be non-negative
        if self.participant1_score < 0 || self.participant2_score < 0 {
            return Err("Scores must be non-negative".to_string());
        }

        // Winner must match scores
        let p1_wins = self.participant1_score > self.participant2_score;
        let expected_winner = if p1_wins {
            // This is a simplification - real validation needs registration IDs
            true
        } else {
            false
        };

        Ok(())
    }
}
```

---

## Service Design

### ResultService

```rust
pub struct ResultService<RCR, TMR, TMGR, TRR, MLS, PM>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TMGR: TournamentMatchGameRepository,
    TRR: TournamentRegistrationRepository,
    MLS: MatchLifecycleService,
    PM: PluginManager,
{
    claim_repo: Arc<RCR>,
    match_repo: Arc<TMR>,
    match_game_repo: Arc<TMGR>,
    registration_repo: Arc<TRR>,
    lifecycle_service: Arc<MLS>,
    plugin_manager: Arc<PM>,
    auto_confirm_timeout: Duration,  // e.g., 15 minutes
}

impl<RCR, TMR, TMGR, TRR, MLS, PM> ResultService<RCR, TMR, TMGR, TRR, MLS, PM>
where
    RCR: ResultClaimRepository,
    TMR: TournamentMatchRepository,
    TMGR: TournamentMatchGameRepository,
    TRR: TournamentRegistrationRepository,
    MLS: MatchLifecycleService,
    PM: PluginManager,
{
    /// Submit a result claim for a match.
    ///
    /// # Errors
    /// - `MatchNotAwaitingResult` if match is wrong status
    /// - `NotParticipant` if user not in match
    /// - `InvalidResult` if scores/winner invalid
    /// - `PendingClaimExists` if there's already a pending claim
    pub async fn submit_claim(
        &self,
        match_id: TournamentMatchId,
        claim: SubmitResultClaim,
        submitted_by: UserId,
    ) -> Result<ResultClaim, DomainError>;

    /// Confirm a result claim.
    ///
    /// Must be called by the opponent of the claim submitter.
    pub async fn confirm_claim(
        &self,
        claim_id: ResultClaimId,
        confirmed_by: UserId,
    ) -> Result<ResultClaim, DomainError>;

    /// Dispute a result claim.
    ///
    /// Creates a dispute and moves match to Disputed status.
    pub async fn dispute_claim(
        &self,
        claim_id: ResultClaimId,
        disputed_by: UserId,
        reason: String,
    ) -> Result<ResultClaim, DomainError>;

    /// Cancel a result claim (by submitter).
    pub async fn cancel_claim(
        &self,
        claim_id: ResultClaimId,
        cancelled_by: UserId,
    ) -> Result<ResultClaim, DomainError>;

    /// Process auto-confirmation for expired claims.
    ///
    /// Called by background job.
    pub async fn process_auto_confirmations(&self) -> Result<Vec<ResultClaim>, DomainError>;

    /// Get pending claim for a match.
    pub async fn get_pending_claim(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<ResultClaim>, DomainError>;

    /// Get all claims for a match (for history).
    pub async fn get_claim_history(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<ResultClaim>, DomainError>;

    /// Validate result claim against game plugin rules.
    async fn validate_claim(
        &self,
        match_: &TournamentMatch,
        claim: &SubmitResultClaim,
    ) -> Result<(), DomainError>;

    /// Apply confirmed result to match and trigger progression.
    async fn apply_result(
        &self,
        match_id: TournamentMatchId,
        claim: &ResultClaim,
    ) -> Result<TournamentMatch, DomainError>;
}

#[derive(Debug, Clone)]
pub struct SubmitResultClaim {
    pub claimed_winner_registration_id: TournamentRegistrationId,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub game_results: Vec<GameResultInput>,
    pub evidence_ids: Vec<EvidenceId>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameResultInput {
    pub game_number: i32,
    pub map_id: String,
    pub participant1_score: i32,
    pub participant2_score: i32,
    pub duration_seconds: Option<i64>,
    pub evidence_ids: Vec<EvidenceId>,
}
```

---

## Validation Rules

### Score Validation

```rust
impl ResultService {
    /// Validate overall match result.
    fn validate_match_result(
        &self,
        match_: &TournamentMatch,
        claim: &SubmitResultClaim,
    ) -> Result<(), ValidationError> {
        // 1. Winner must be a participant
        if claim.claimed_winner_registration_id != match_.participant1_registration_id.unwrap()
            && claim.claimed_winner_registration_id != match_.participant2_registration_id.unwrap()
        {
            return Err(ValidationError::InvalidWinner);
        }

        // 2. Scores must match winner
        let p1_wins = claim.participant1_score > claim.participant2_score;
        let claimed_p1_wins =
            claim.claimed_winner_registration_id == match_.participant1_registration_id.unwrap();

        if p1_wins != claimed_p1_wins {
            return Err(ValidationError::ScoreWinnerMismatch);
        }

        // 3. For series, validate game count
        let required_wins = match_.match_format.wins_required();
        let games_count = claim.game_results.len();

        // Must have at least required_wins games
        if games_count < required_wins as usize {
            return Err(ValidationError::InsufficientGames {
                required: required_wins,
                provided: games_count as u32,
            });
        }

        // 4. Validate each game result
        for (i, game) in claim.game_results.iter().enumerate() {
            self.validate_game_result(game, i + 1)?;
        }

        // 5. Sum of game wins must match series score
        let p1_game_wins = claim.game_results.iter()
            .filter(|g| g.participant1_score > g.participant2_score)
            .count() as i32;
        let p2_game_wins = claim.game_results.iter()
            .filter(|g| g.participant2_score > g.participant1_score)
            .count() as i32;

        if p1_game_wins != claim.participant1_score || p2_game_wins != claim.participant2_score {
            return Err(ValidationError::GameScoresMismatch);
        }

        Ok(())
    }

    /// Validate a single game result.
    fn validate_game_result(
        &self,
        game: &GameResultInput,
        expected_number: usize,
    ) -> Result<(), ValidationError> {
        // Game number must be sequential
        if game.game_number != expected_number as i32 {
            return Err(ValidationError::NonSequentialGameNumber);
        }

        // Scores must be non-negative
        if game.participant1_score < 0 || game.participant2_score < 0 {
            return Err(ValidationError::NegativeScore);
        }

        // Cannot be a tie (must have winner)
        if game.participant1_score == game.participant2_score {
            return Err(ValidationError::TiedGame);
        }

        Ok(())
    }
}
```

### Plugin Validation

```rust
/// Extended GamePlugin for result validation.
pub trait ResultValidationPlugin: GamePlugin {
    /// Validate a game result for this game.
    ///
    /// Can check:
    /// - Score ranges (e.g., CS2 max rounds)
    /// - Map validity
    /// - Game-specific rules
    fn validate_game_result(
        &self,
        map_id: &str,
        participant1_score: i32,
        participant2_score: i32,
    ) -> Result<(), String>;
}

// CS2 implementation
impl ResultValidationPlugin for Cs2Plugin {
    fn validate_game_result(
        &self,
        map_id: &str,
        participant1_score: i32,
        participant2_score: i32,
    ) -> Result<(), String> {
        // Validate map exists
        if !self.available_maps().iter().any(|m| m.id == map_id) {
            return Err(format!("Unknown map: {}", map_id));
        }

        // Standard CS2 scoring
        let winner_score = participant1_score.max(participant2_score);
        let loser_score = participant1_score.min(participant2_score);

        // Normal game: 13-0 to 13-12, or OT: 13+, 14+, etc.
        if winner_score < 13 {
            return Err("Winner must have at least 13 rounds".to_string());
        }

        if winner_score == 13 && loser_score > 12 {
            return Err("Invalid score for regulation game".to_string());
        }

        // Overtime rules: must win by 4 in MR3
        if winner_score > 13 {
            let ot_rounds = winner_score - 12;
            let ot_loser = loser_score - 12;

            // Check OT is valid (groups of 6 rounds in MR3)
            if ot_rounds < 4 || (winner_score - loser_score) < 4 {
                return Err("Invalid overtime score".to_string());
            }
        }

        Ok(())
    }
}
```

---

## API Endpoints

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/result

Submit a result claim.

**Request**:
```json
{
  "winner_registration_id": "...",
  "participant1_score": 2,
  "participant2_score": 1,
  "game_results": [
    {
      "game_number": 1,
      "map_id": "de_mirage",
      "participant1_score": 16,
      "participant2_score": 12
    },
    {
      "game_number": 2,
      "map_id": "de_inferno",
      "participant1_score": 10,
      "participant2_score": 16
    },
    {
      "game_number": 3,
      "map_id": "de_ancient",
      "participant1_score": 16,
      "participant2_score": 8
    }
  ],
  "evidence_ids": ["..."],
  "notes": "GG, close game on map 1"
}
```

**Response** (201 Created):
```json
{
  "data": {
    "claim_id": "...",
    "match_id": "...",
    "status": "pending",
    "claimed_winner": {
      "registration_id": "...",
      "name": "Team Alpha"
    },
    "scores": {
      "participant1": 2,
      "participant2": 1
    },
    "auto_confirm_at": "2025-01-15T21:00:00Z",
    "submitted_at": "2025-01-15T20:45:00Z"
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/result/confirm

Confirm a result claim.

**Request**:
```json
{
  "claim_id": "..."
}
```

**Response** (200 OK):
```json
{
  "data": {
    "claim": {
      "id": "...",
      "status": "confirmed",
      "confirmed_at": "2025-01-15T20:50:00Z"
    },
    "match": {
      "id": "...",
      "status": "completed",
      "winner": {
        "registration_id": "...",
        "name": "Team Alpha"
      }
    }
  }
}
```

### POST /v1/tournaments/{tournament_id}/matches/{match_id}/result/dispute

Dispute a result claim.

**Request**:
```json
{
  "claim_id": "...",
  "reason": "Score for map 2 is incorrect - we won 16-14 not 16-10",
  "evidence_ids": ["..."]
}
```

**Response** (200 OK):
```json
{
  "data": {
    "claim": {
      "id": "...",
      "status": "disputed"
    },
    "dispute": {
      "id": "...",
      "status": "pending",
      "reason": "..."
    },
    "match": {
      "id": "...",
      "status": "disputed"
    }
  }
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/result

Get current result claim status.

**Response**:
```json
{
  "data": {
    "pending_claim": {
      "id": "...",
      "status": "pending",
      "submitted_by": {
        "registration_id": "...",
        "name": "Team Alpha"
      },
      "claimed_winner": "Team Alpha",
      "scores": {
        "participant1": 2,
        "participant2": 1
      },
      "auto_confirm_at": "2025-01-15T21:00:00Z",
      "remaining_seconds": 542
    },
    "can_submit": false,
    "can_confirm": true,
    "can_dispute": true
  }
}
```

### GET /v1/tournaments/{tournament_id}/matches/{match_id}/result/history

Get result claim history for a match.

---

## Background Jobs

### Auto-Confirmation Job

```rust
/// Background job to process auto-confirmations.
pub struct AutoConfirmationJob {
    result_service: Arc<ResultService>,
    check_interval: Duration,
}

impl AutoConfirmationJob {
    pub async fn run(&self) {
        loop {
            match self.result_service.process_auto_confirmations().await {
                Ok(confirmed) => {
                    for claim in confirmed {
                        log::info!(
                            "Auto-confirmed result claim {} for match {}",
                            claim.id,
                            claim.match_id
                        );
                        // Notify both teams
                        notify_result_auto_confirmed(&claim).await;
                    }
                }
                Err(e) => {
                    log::error!("Failed to process auto-confirmations: {}", e);
                }
            }

            tokio::time::sleep(self.check_interval).await;
        }
    }
}
```

---

## Error Handling

### New Error Types

```rust
pub enum ResultError {
    /// Match not in AwaitingResult status
    MatchNotAwaitingResult(TournamentMatchId),

    /// User not a participant
    NotParticipant {
        user_id: UserId,
        match_id: TournamentMatchId,
    },

    /// Pending claim already exists
    PendingClaimExists(ResultClaimId),

    /// Claim not found
    ClaimNotFound(ResultClaimId),

    /// Cannot confirm own claim
    CannotConfirmOwnClaim,

    /// Claim not pending
    ClaimNotPending(ResultClaimId),

    /// Invalid winner (not a participant)
    InvalidWinner(TournamentRegistrationId),

    /// Scores don't match winner
    ScoreWinnerMismatch,

    /// Game count doesn't match series format
    InvalidGameCount {
        expected_min: u32,
        provided: u32,
    },

    /// Game scores don't sum to series score
    GameScoresMismatch,

    /// Plugin validation failed
    ValidationFailed(String),
}
```

---

## Testing Notes

### Unit Tests

- Claim status transitions
- Score validation logic
- Game count validation
- CS2 score validation (regulation + OT)

### Integration Tests

```
test_submit_result_claim
test_submit_result_claim_not_participant
test_submit_result_claim_wrong_status
test_confirm_result_claim
test_confirm_own_claim_fails
test_dispute_result_claim
test_cancel_result_claim
test_auto_confirmation
test_submit_second_claim_supersedes
test_bo3_result_validation
test_bo5_result_validation
test_cs2_score_validation
test_result_triggers_progression
```

### Edge Case Tests

```
test_simultaneous_confirmation_and_dispute
test_auto_confirm_at_exact_deadline
test_claim_with_evidence_linking
test_partial_game_results
```
