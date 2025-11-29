//! Audit repository trait for entity change tracking.

use crate::entities::audit::{ChangeType, EntityChange, EntityChangeId};
use async_trait::async_trait;
use portal_core::{DomainError, PlayerId};
use uuid::Uuid;

/// Repository trait for audit trail operations.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EntityChangeRepository: Send + Sync {
    /// Record a new entity change.
    async fn create(&self, change: CreateEntityChange) -> Result<EntityChange, DomainError>;

    /// Find a change by ID.
    async fn find_by_id(&self, id: EntityChangeId) -> Result<Option<EntityChange>, DomainError>;

    /// List changes for an entity.
    async fn list_by_entity(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError>;

    /// Count changes for an entity.
    async fn count_by_entity(
        &self,
        entity_type: &str,
        entity_id: Uuid,
    ) -> Result<i64, DomainError>;

    /// List changes for an entity's specific field.
    async fn list_by_field(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError>;

    /// List recent changes made by a player.
    async fn list_by_player(
        &self,
        player_id: PlayerId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EntityChange>, DomainError>;

    /// Mark a change as reverted.
    async fn mark_reverted(
        &self,
        id: EntityChangeId,
        reverted_by: PlayerId,
        reason: Option<String>,
    ) -> Result<EntityChange, DomainError>;

    /// Get the most recent change for a field.
    async fn get_latest_field_change(
        &self,
        entity_type: &str,
        entity_id: Uuid,
        field_name: &str,
    ) -> Result<Option<EntityChange>, DomainError>;
}

/// Data for creating a new entity change record.
#[derive(Debug, Clone)]
pub struct CreateEntityChange {
    /// Type of entity that was changed.
    pub entity_type: String,

    /// ID of the entity that was changed.
    pub entity_id: Uuid,

    /// Type of change made.
    pub change_type: ChangeType,

    /// Name of the field that was changed.
    pub field_name: Option<String>,

    /// Previous value as JSON.
    pub old_value: Option<serde_json::Value>,

    /// New value as JSON.
    pub new_value: Option<serde_json::Value>,

    /// Player who made the change.
    pub changed_by: PlayerId,

    /// Request ID for correlation.
    pub request_id: Option<String>,

    /// IP address of the client.
    pub ip_address: Option<String>,

    /// User agent of the client.
    pub user_agent: Option<String>,
}
