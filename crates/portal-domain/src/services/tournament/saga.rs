//! Saga coordinator for multi-step tournament operations.
//!
//! Provides orchestration for sagas with compensation support.

use std::sync::Arc;

use async_trait::async_trait;
use portal_core::{DomainError, SagaId};
use tracing::{info, instrument, warn};

use crate::entities::saga::{SagaContext, SagaExecution, SagaStatus};
use crate::repositories::evidence::SagaExecutionRepository;

/// A saga step definition.
///
/// Each step has a forward action and an optional compensation action.
#[async_trait]
pub trait SagaStep<Input, Output>: Send + Sync {
    /// Name of this step for logging and tracking.
    fn name(&self) -> &str;

    /// Execute the forward action.
    async fn execute(&self, input: &Input) -> Result<Output, DomainError>;

    /// Execute compensation for this step (if supported).
    /// Default implementation does nothing.
    async fn compensate(&self, _input: &Input, _output: &Output) -> Result<(), DomainError> {
        Ok(())
    }

    /// Whether this step supports compensation.
    fn supports_compensation(&self) -> bool {
        false
    }
}

/// Result of saga execution.
#[derive(Debug, Clone)]
pub struct SagaResult<T> {
    /// The saga execution record.
    pub execution: SagaExecution,
    /// The final output (if successful).
    pub output: Option<T>,
    /// Whether the saga completed successfully.
    pub success: bool,
}

impl<T> SagaResult<T> {
    /// Create a successful result.
    pub fn success(execution: SagaExecution, output: T) -> Self {
        Self {
            execution,
            output: Some(output),
            success: true,
        }
    }

    /// Create a failed result.
    pub fn failure(execution: SagaExecution) -> Self {
        Self {
            execution,
            output: None,
            success: false,
        }
    }
}

/// Saga definition with typed steps.
///
/// This is a builder for defining saga workflows.
#[derive(Debug, Clone)]
pub struct SagaDefinition {
    /// Saga type name.
    pub saga_type: String,
    /// Version of this saga definition.
    pub version: i32,
    /// Maximum retry attempts per step.
    pub max_retries: i32,
}

impl SagaDefinition {
    /// Create a new saga definition.
    pub fn new(saga_type: impl Into<String>, version: i32) -> Self {
        Self {
            saga_type: saga_type.into(),
            version,
            max_retries: 3,
        }
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max_retries: i32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

/// Saga coordinator for orchestrating multi-step operations.
#[derive(Clone)]
pub struct SagaCoordinator<SR> {
    saga_repo: Arc<SR>,
}

impl<SR> SagaCoordinator<SR>
where
    SR: SagaExecutionRepository,
{
    /// Create a new saga coordinator.
    pub fn new(saga_repo: Arc<SR>) -> Self {
        Self { saga_repo }
    }

    /// Start a new saga execution.
    #[instrument(skip(self, input_data))]
    pub async fn start_saga(
        &self,
        definition: &SagaDefinition,
        context: SagaContext,
        input_data: serde_json::Value,
    ) -> Result<SagaExecution, DomainError> {
        let mut execution = SagaExecution::new(
            &definition.saga_type,
            definition.version,
            input_data.clone(),
            context,
        );
        execution.max_retries = definition.max_retries;

        let execution = self
            .saga_repo
            .create(crate::repositories::evidence::CreateSagaExecution {
                saga_type: definition.saga_type.clone(),
                saga_version: definition.version,
                tournament_id: execution.tournament_id,
                match_id: execution.match_id,
                correlation_id: execution.correlation_id.clone(),
                input_data,
                max_retries: definition.max_retries,
            })
            .await?;

        info!(
            saga_id = %execution.id,
            saga_type = %definition.saga_type,
            "Started saga execution"
        );

        Ok(execution)
    }

    /// Get saga execution by ID.
    #[instrument(skip(self))]
    pub async fn get_saga(&self, saga_id: SagaId) -> Result<Option<SagaExecution>, DomainError> {
        self.saga_repo.find_by_id(saga_id).await
    }

    /// Update saga status.
    #[instrument(skip(self))]
    pub async fn update_status(
        &self,
        saga_id: SagaId,
        status: SagaStatus,
    ) -> Result<SagaExecution, DomainError> {
        self.saga_repo.update_status(saga_id, status).await
    }

    /// Complete a saga step.
    #[instrument(skip(self, execution))]
    pub async fn complete_step(
        &self,
        execution: &mut SagaExecution,
        step_name: &str,
        output: Option<serde_json::Value>,
    ) -> Result<(), DomainError> {
        execution.complete_step(step_name.to_string(), output);
        self.saga_repo.update(execution).await?;

        info!(
            saga_id = %execution.id,
            step = step_name,
            "Completed saga step"
        );

        Ok(())
    }

    /// Record step failure.
    #[instrument(skip(self, execution))]
    pub async fn fail_step(
        &self,
        execution: &mut SagaExecution,
        step_name: &str,
        error: &str,
    ) -> Result<(), DomainError> {
        execution.fail_step(step_name.to_string(), error.to_string());
        self.saga_repo.update(execution).await?;

        warn!(
            saga_id = %execution.id,
            step = step_name,
            error = error,
            "Failed saga step"
        );

        Ok(())
    }

    /// Complete the saga successfully.
    #[instrument(skip(self, execution))]
    pub async fn complete_saga(
        &self,
        execution: &mut SagaExecution,
    ) -> Result<(), DomainError> {
        execution.complete();
        self.saga_repo.update(execution).await?;

        info!(
            saga_id = %execution.id,
            "Saga completed successfully"
        );

        Ok(())
    }

    /// Fail the saga.
    #[instrument(skip(self, execution))]
    pub async fn fail_saga(
        &self,
        execution: &mut SagaExecution,
        error: &str,
    ) -> Result<(), DomainError> {
        execution.fail(error.to_string());
        self.saga_repo.update(execution).await?;

        warn!(
            saga_id = %execution.id,
            error = error,
            "Saga failed"
        );

        Ok(())
    }

    /// Start compensation.
    #[instrument(skip(self, execution))]
    pub async fn start_compensation(
        &self,
        execution: &mut SagaExecution,
    ) -> Result<(), DomainError> {
        execution.start_compensation();
        self.saga_repo.update(execution).await?;

        info!(
            saga_id = %execution.id,
            "Starting saga compensation"
        );

        Ok(())
    }

    /// Complete compensation.
    #[instrument(skip(self, execution))]
    pub async fn complete_compensation(
        &self,
        execution: &mut SagaExecution,
    ) -> Result<(), DomainError> {
        execution.complete_compensation();
        self.saga_repo.update(execution).await?;

        info!(
            saga_id = %execution.id,
            "Saga compensation completed"
        );

        Ok(())
    }

    /// Find pending sagas that need processing.
    #[instrument(skip(self))]
    pub async fn find_pending(&self) -> Result<Vec<SagaExecution>, DomainError> {
        self.saga_repo.find_pending().await
    }

    /// Find stuck sagas (running for too long).
    #[instrument(skip(self))]
    pub async fn find_stuck(
        &self,
        running_since_before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<SagaExecution>, DomainError> {
        self.saga_repo.find_stuck(running_since_before).await
    }
}

/// Trait for executable sagas.
///
/// Implement this trait for specific saga types like `MatchCompletionSaga`.
#[async_trait]
pub trait Saga: Send + Sync {
    /// Input type for the saga.
    type Input: Send + Sync;
    /// Output type on successful completion.
    type Output: Send + Sync;

    /// Get the saga definition.
    fn definition(&self) -> SagaDefinition;

    /// Execute the saga.
    async fn execute(&self, input: Self::Input) -> Result<SagaResult<Self::Output>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::saga::SagaExecution;

    #[test]
    fn test_saga_definition_creation() {
        let def = SagaDefinition::new("test_saga", 1);

        assert_eq!(def.saga_type, "test_saga");
        assert_eq!(def.version, 1);
        assert_eq!(def.max_retries, 3); // default
    }

    #[test]
    fn test_saga_definition_with_max_retries() {
        let def = SagaDefinition::new("test_saga", 2).with_max_retries(5);

        assert_eq!(def.saga_type, "test_saga");
        assert_eq!(def.version, 2);
        assert_eq!(def.max_retries, 5);
    }

    #[test]
    fn test_saga_result_success() {
        let execution = create_test_execution();
        let result: SagaResult<String> = SagaResult::success(execution.clone(), "output".to_string());

        assert!(result.success);
        assert!(result.output.is_some());
        assert_eq!(result.output.unwrap(), "output");
        assert_eq!(result.execution.id, execution.id);
    }

    #[test]
    fn test_saga_result_failure() {
        let execution = create_test_execution();
        let result: SagaResult<String> = SagaResult::failure(execution.clone());

        assert!(!result.success);
        assert!(result.output.is_none());
        assert_eq!(result.execution.id, execution.id);
    }

    fn create_test_execution() -> SagaExecution {
        SagaExecution::new(
            "test_saga",
            1,
            serde_json::json!({}),
            SagaContext::default(),
        )
    }
}
