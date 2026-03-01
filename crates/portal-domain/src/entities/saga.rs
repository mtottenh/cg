//! Saga execution domain entities.
//!
//! Sagas are multi-step operations that must either all succeed or be compensated.
//! This module provides the state tracking for saga executions.

use chrono::{DateTime, Utc};
use portal_core::{SagaId, TournamentId, TournamentMatchId};
use serde::{Deserialize, Serialize};

// =============================================================================
// SAGA EXECUTION
// =============================================================================

/// A saga execution record.
///
/// Tracks the state of a multi-step saga operation, including
/// step history, error tracking, and retry information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaExecution {
    pub id: SagaId,

    /// Type of saga being executed
    pub saga_type: String,

    /// Version of the saga definition
    pub saga_version: i32,

    /// Context references
    pub tournament_id: Option<TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    pub correlation_id: Option<String>,

    /// Input data for the saga
    pub input_data: serde_json::Value,

    /// Current step index (0-based)
    pub current_step: i32,

    /// Current status
    pub status: SagaStatus,

    /// History of step executions
    pub step_history: Vec<StepRecord>,

    /// Last error message
    pub last_error: Option<String>,

    /// Retry tracking
    pub retry_count: i32,
    pub max_retries: i32,

    /// Timing
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SagaExecution {
    /// Create a new saga execution.
    pub fn new(
        saga_type: impl Into<String>,
        saga_version: i32,
        input_data: serde_json::Value,
        context: SagaContext,
    ) -> Self {
        Self {
            id: SagaId::new(),
            saga_type: saga_type.into(),
            saga_version,
            tournament_id: context.tournament_id,
            match_id: context.match_id,
            correlation_id: context.correlation_id,
            input_data,
            current_step: 0,
            status: SagaStatus::Pending,
            step_history: Vec::new(),
            last_error: None,
            retry_count: 0,
            max_retries: 3,
            started_at: None,
            completed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Check if the saga is pending.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        matches!(self.status, SagaStatus::Pending)
    }

    /// Check if the saga is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.status, SagaStatus::Running)
    }

    /// Check if the saga completed successfully.
    #[must_use]
    pub fn is_completed(&self) -> bool {
        matches!(self.status, SagaStatus::Completed)
    }

    /// Check if the saga failed.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self.status, SagaStatus::Failed)
    }

    /// Check if the saga is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        matches!(self.status, SagaStatus::Paused)
    }

    /// Check if the saga is in a terminal state.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            SagaStatus::Completed | SagaStatus::Failed | SagaStatus::Compensated
        )
    }

    /// Check if we can retry.
    #[must_use]
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Start the saga.
    pub fn start(&mut self) {
        self.status = SagaStatus::Running;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark a step as completed.
    pub fn complete_step(&mut self, step_name: String, output: Option<serde_json::Value>) {
        self.step_history.push(StepRecord {
            step: self.current_step,
            name: step_name,
            status: StepStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            output,
            error: None,
            retry_count: 0,
        });
        self.current_step += 1;
        self.retry_count = 0;
        self.updated_at = Utc::now();
    }

    /// Record a step failure.
    pub fn fail_step(&mut self, step_name: String, error: String) {
        self.step_history.push(StepRecord {
            step: self.current_step,
            name: step_name,
            status: StepStatus::Failed,
            started_at: Utc::now(),
            completed_at: None,
            output: None,
            error: Some(error.clone()),
            retry_count: self.retry_count,
        });
        self.last_error = Some(error);
        self.retry_count += 1;
        self.updated_at = Utc::now();
    }

    /// Complete the saga successfully.
    pub fn complete(&mut self) {
        self.status = SagaStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark the saga as failed.
    pub fn fail(&mut self, error: String) {
        self.status = SagaStatus::Failed;
        self.last_error = Some(error);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Pause the saga (waiting for external resolution).
    pub fn pause(&mut self, reason: String) {
        self.status = SagaStatus::Paused;
        self.last_error = Some(reason);
        self.updated_at = Utc::now();
    }

    /// Start compensation.
    pub fn start_compensation(&mut self) {
        self.status = SagaStatus::Compensating;
        self.updated_at = Utc::now();
    }

    /// Mark compensation as complete.
    pub fn complete_compensation(&mut self) {
        self.status = SagaStatus::Compensated;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// Context for saga execution.
#[derive(Debug, Clone, Default)]
pub struct SagaContext {
    pub tournament_id: Option<TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    pub correlation_id: Option<String>,
}

impl SagaContext {
    /// Create context with tournament reference.
    pub fn with_tournament(tournament_id: TournamentId) -> Self {
        Self {
            tournament_id: Some(tournament_id),
            ..Default::default()
        }
    }

    /// Create context with match reference.
    pub fn with_match(match_id: TournamentMatchId, tournament_id: TournamentId) -> Self {
        Self {
            tournament_id: Some(tournament_id),
            match_id: Some(match_id),
            ..Default::default()
        }
    }

    /// Add correlation ID.
    pub fn with_correlation(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}

/// Status of a saga execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SagaStatus {
    /// Not yet started
    #[default]
    Pending,
    /// Currently executing steps
    Running,
    /// Paused waiting for external resolution (e.g., review)
    Paused,
    /// All steps completed successfully
    Completed,
    /// Failed after max retries
    Failed,
    /// Running compensation
    Compensating,
    /// Compensation completed
    Compensated,
}

impl std::fmt::Display for SagaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Compensating => write!(f, "compensating"),
            Self::Compensated => write!(f, "compensated"),
        }
    }
}

impl std::str::FromStr for SagaStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "compensating" => Ok(Self::Compensating),
            "compensated" => Ok(Self::Compensated),
            _ => Err(format!("invalid saga status: {s}")),
        }
    }
}

/// Record of a saga step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    /// Step index (0-based)
    pub step: i32,
    /// Step name
    pub name: String,
    /// Step status
    pub status: StepStatus,
    /// When step started
    pub started_at: DateTime<Utc>,
    /// When step completed (if successful)
    pub completed_at: Option<DateTime<Utc>>,
    /// Step output data
    pub output: Option<serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Retry attempts for this step
    pub retry_count: i32,
}

/// Status of a saga step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Not yet executed
    #[default]
    Pending,
    /// Currently executing
    Running,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
    /// Skipped (due to earlier failure)
    Skipped,
    /// Compensated (rolled back)
    Compensated,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
            Self::Compensated => write!(f, "compensated"),
        }
    }
}

impl std::str::FromStr for StepStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            "compensated" => Ok(Self::Compensated),
            _ => Err(format!("invalid step status: {s}")),
        }
    }
}
