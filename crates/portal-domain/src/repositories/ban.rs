//! Ban repository trait.

use crate::entities::{Ban, BanFilters, CreateBanCommand};
use async_trait::async_trait;
use portal_core::{BanId, DomainError, UserId};

/// Pagination metadata.
#[derive(Debug, Clone)]
pub struct PaginationMeta {
    pub page: i64,
    pub per_page: i64,
    pub total_items: i64,
    pub total_pages: i64,
}

/// Paginated bans result.
#[derive(Debug, Clone)]
pub struct PaginatedBans {
    pub items: Vec<Ban>,
    pub pagination: PaginationMeta,
}

/// Repository trait for ban operations.
#[async_trait]
pub trait BanRepository: Send + Sync {
    /// Find a ban by ID.
    async fn find_by_id(&self, id: BanId) -> Result<Option<Ban>, DomainError>;

    /// Create a new ban.
    async fn create(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError>;

    /// Create a ban and, when it is an active platform ban, enforce it in the
    /// SAME transaction: flip `users.status` to `banned` (with the ban reason)
    /// and revoke every one of the user's refresh tokens. Either all writes
    /// commit or none do, so a mid-operation failure can never leave a
    /// recorded-but-unenforced ban (an orphan `bans` row whose user still has
    /// an active account and live sessions). Non-platform / inactive bans only
    /// write the `bans` row, exactly like [`create`](Self::create).
    async fn create_and_enforce(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError>;

    /// Lift a ban (set `lifted_at`, `lifted_by`, `lift_reason`).
    async fn lift(
        &self,
        id: BanId,
        lifted_by: UserId,
        lift_reason: Option<&str>,
    ) -> Result<Ban, DomainError>;

    /// Get all active bans for a user.
    async fn get_active_for_user(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError>;

    /// Check if a user has an active platform ban.
    async fn is_platform_banned(&self, user_id: UserId) -> Result<bool, DomainError>;

    /// List bans with filters and pagination.
    async fn list(
        &self,
        filters: BanFilters,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedBans, DomainError>;

    /// Get all bans for a user (including inactive/expired).
    async fn get_user_ban_history(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError>;
}
