//! Audit service for tracking entity changes.

use crate::entities::audit::{ChangeType, EntityChange, EntityChangeId, EntityHistory};
use crate::repositories::{CreateEntityChange, EntityChangeRepository};
use portal_core::{DomainError, PlayerId};
use serde::Serialize;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

/// Context for recording a change.
#[derive(Debug, Clone, Default)]
pub struct ChangeContext {
    /// Request ID for correlation.
    pub request_id: Option<String>,
    /// IP address of the client.
    pub ip_address: Option<String>,
    /// User agent of the client.
    pub user_agent: Option<String>,
}

/// Service for audit trail operations.
pub struct AuditService<ECR>
where
    ECR: EntityChangeRepository,
{
    entity_change_repo: Arc<ECR>,
}

impl<ECR> AuditService<ECR>
where
    ECR: EntityChangeRepository,
{
    /// Create a new audit service.
    pub fn new(entity_change_repo: Arc<ECR>) -> Self {
        Self { entity_change_repo }
    }

    /// Record a field change for an entity.
    #[instrument(skip(self, old_value, new_value, ctx))]
    pub async fn record_field_change<T: Serialize + std::fmt::Debug>(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
        old_value: Option<&T>,
        new_value: Option<&T>,
        changed_by: PlayerId,
        ctx: &ChangeContext,
    ) -> Result<EntityChange, DomainError> {
        let old_json = old_value.map(|v| serde_json::to_value(v).ok()).flatten();
        let new_json = new_value.map(|v| serde_json::to_value(v).ok()).flatten();

        let cmd = CreateEntityChange {
            entity_type: entity_type.to_string(),
            entity_id,
            change_type: ChangeType::Update,
            field_name: Some(field_name.to_string()),
            old_value: old_json,
            new_value: new_json,
            changed_by,
            request_id: ctx.request_id.clone(),
            ip_address: ctx.ip_address.clone(),
            user_agent: ctx.user_agent.clone(),
        };

        self.entity_change_repo.create(cmd).await
    }

    /// Record entity creation.
    #[instrument(skip(self, entity_data, ctx))]
    pub async fn record_creation<T: Serialize + std::fmt::Debug>(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        entity_data: &T,
        created_by: PlayerId,
        ctx: &ChangeContext,
    ) -> Result<EntityChange, DomainError> {
        let entity_json = serde_json::to_value(entity_data)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let cmd = CreateEntityChange {
            entity_type: entity_type.to_string(),
            entity_id,
            change_type: ChangeType::Create,
            field_name: None,
            old_value: None,
            new_value: Some(entity_json),
            changed_by: created_by,
            request_id: ctx.request_id.clone(),
            ip_address: ctx.ip_address.clone(),
            user_agent: ctx.user_agent.clone(),
        };

        self.entity_change_repo.create(cmd).await
    }

    /// Record entity deletion.
    #[instrument(skip(self, entity_data, ctx))]
    pub async fn record_deletion<T: Serialize + std::fmt::Debug>(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        entity_data: &T,
        deleted_by: PlayerId,
        ctx: &ChangeContext,
    ) -> Result<EntityChange, DomainError> {
        let entity_json = serde_json::to_value(entity_data)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let cmd = CreateEntityChange {
            entity_type: entity_type.to_string(),
            entity_id,
            change_type: ChangeType::Delete,
            field_name: None,
            old_value: Some(entity_json),
            new_value: None,
            changed_by: deleted_by,
            request_id: ctx.request_id.clone(),
            ip_address: ctx.ip_address.clone(),
            user_agent: ctx.user_agent.clone(),
        };

        self.entity_change_repo.create(cmd).await
    }

    /// Record multiple field changes at once.
    #[instrument(skip(self, changes, ctx))]
    pub async fn record_update(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        changes: Vec<FieldChange>,
        changed_by: PlayerId,
        ctx: &ChangeContext,
    ) -> Result<Vec<EntityChange>, DomainError> {
        let mut results = Vec::with_capacity(changes.len());

        for change in changes {
            let cmd = CreateEntityChange {
                entity_type: entity_type.to_string(),
                entity_id,
                change_type: ChangeType::Update,
                field_name: Some(change.field_name),
                old_value: change.old_value,
                new_value: change.new_value,
                changed_by,
                request_id: ctx.request_id.clone(),
                ip_address: ctx.ip_address.clone(),
                user_agent: ctx.user_agent.clone(),
            };

            let result = self.entity_change_repo.create(cmd).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get the change history for an entity.
    #[instrument(skip(self))]
    pub async fn get_entity_history(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<EntityHistory, DomainError> {
        let changes = self
            .entity_change_repo
            .list_by_entity(entity_type, entity_id, limit, offset)
            .await?;

        let total_changes = self
            .entity_change_repo
            .count_by_entity(entity_type, entity_id)
            .await?;

        Ok(EntityHistory {
            entity_type: entity_type.to_string(),
            entity_id,
            changes,
            total_changes,
        })
    }

    /// Get change history for a specific field.
    #[instrument(skip(self))]
    pub async fn get_field_history(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError> {
        self.entity_change_repo
            .list_by_field(entity_type, entity_id, field_name, limit, offset)
            .await
    }

    /// Get a specific change by ID.
    #[instrument(skip(self))]
    pub async fn get_change(&self, id: EntityChangeId) -> Result<EntityChange, DomainError> {
        self.entity_change_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Change not found: {}", id)))
    }

    /// Revert a specific change.
    ///
    /// This records the reversion and returns the original change marked as reverted.
    /// The actual entity update should be done by the caller.
    #[instrument(skip(self))]
    pub async fn mark_change_reverted(
        &self,
        change_id: EntityChangeId,
        reverted_by: PlayerId,
        reason: Option<String>,
    ) -> Result<EntityChange, DomainError> {
        // Get the original change
        let original = self.get_change(change_id).await?;

        // Check if already reverted
        if original.reverted_at.is_some() {
            return Err(DomainError::InvalidState(
                "Change has already been reverted".to_string(),
            ));
        }

        // Mark as reverted
        self.entity_change_repo
            .mark_reverted(change_id, reverted_by, reason)
            .await
    }
}

impl<ECR> Clone for AuditService<ECR>
where
    ECR: EntityChangeRepository,
{
    fn clone(&self) -> Self {
        Self {
            entity_change_repo: Arc::clone(&self.entity_change_repo),
        }
    }
}

/// A single field change for batch recording.
#[derive(Debug, Clone)]
pub struct FieldChange {
    /// Name of the field.
    pub field_name: String,
    /// Previous value.
    pub old_value: Option<serde_json::Value>,
    /// New value.
    pub new_value: Option<serde_json::Value>,
}

impl FieldChange {
    /// Create a new field change.
    pub fn new<T: Serialize>(
        field_name: impl Into<String>,
        old_value: Option<&T>,
        new_value: Option<&T>,
    ) -> Self {
        Self {
            field_name: field_name.into(),
            old_value: old_value.and_then(|v| serde_json::to_value(v).ok()),
            new_value: new_value.and_then(|v| serde_json::to_value(v).ok()),
        }
    }
}

/// Helper to detect changes between two structs.
pub struct ChangeDetector;

impl ChangeDetector {
    /// Compare two optional values and return a FieldChange if they differ.
    pub fn compare_optional<T: Serialize + PartialEq>(
        field_name: &str,
        old: &Option<T>,
        new: &Option<T>,
    ) -> Option<FieldChange> {
        if old != new {
            Some(FieldChange::new(field_name, old.as_ref(), new.as_ref()))
        } else {
            None
        }
    }

    /// Compare two values and return a FieldChange if they differ.
    pub fn compare<T: Serialize + PartialEq>(
        field_name: &str,
        old: &T,
        new: &T,
    ) -> Option<FieldChange> {
        if old != new {
            Some(FieldChange::new(field_name, Some(old), Some(new)))
        } else {
            None
        }
    }
}
