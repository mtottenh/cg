//! Ban service with business logic.

use crate::entities::{Ban, BanFilters, CreateBanCommand, LiftBanCommand};
use crate::repositories::{BanRepository, PaginatedBans};
use portal_core::{BanId, DomainError, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for ban-related business logic.
pub struct BanService<BR>
where
    BR: BanRepository,
{
    ban_repo: Arc<BR>,
}

impl<BR> BanService<BR>
where
    BR: BanRepository,
{
    /// Create a new ban service.
    pub const fn new(ban_repo: Arc<BR>) -> Self {
        Self { ban_repo }
    }

    /// Get a ban by ID.
    #[instrument(skip(self))]
    pub async fn get_ban(&self, id: BanId) -> Result<Ban, DomainError> {
        self.ban_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::BanNotFound(id.to_string()))
    }

    /// Create a new ban.
    ///
    /// # Arguments
    /// * `cmd` - The ban creation command
    /// * `issued_by` - The admin issuing the ban
    #[instrument(skip(self, cmd), fields(user_id = %cmd.user_id, ban_type = %cmd.ban_type))]
    pub async fn create_ban(&self, cmd: CreateBanCommand) -> Result<Ban, DomainError> {
        // Business rule: Can't create a ban if user already has an active ban of the same type
        // (unless it's a different scope)
        let active_bans = self.ban_repo.get_active_for_user(cmd.user_id).await?;

        for existing in &active_bans {
            if existing.ban_type == cmd.ban_type {
                // Check if scope matches
                let same_scope = match (&existing.scope_type, &cmd.scope_type) {
                    (None, None) => true,
                    (Some(a), Some(b)) => {
                        a == b && existing.scope_id == cmd.scope_id
                    }
                    _ => false,
                };

                if same_scope {
                    return Err(DomainError::Conflict(format!(
                        "User already has an active {} ban",
                        cmd.ban_type
                    )));
                }
            }
        }

        let ban = self.ban_repo.create(cmd).await?;
        info!(ban_id = %ban.id, user_id = %ban.user_id, ban_type = %ban.ban_type, "Ban created");

        Ok(ban)
    }

    /// Lift (revoke) a ban early.
    #[instrument(skip(self))]
    pub async fn lift_ban(&self, cmd: LiftBanCommand) -> Result<Ban, DomainError> {
        // Verify ban exists
        let ban = self.get_ban(cmd.ban_id).await?;

        // Business rule: Can't lift an already lifted ban
        if ban.is_lifted() {
            return Err(DomainError::InvalidState("Ban has already been lifted".into()));
        }

        // Business rule: Can't lift an expired ban
        if ban.is_expired() {
            return Err(DomainError::InvalidState("Ban has already expired".into()));
        }

        let ban = self
            .ban_repo
            .lift(cmd.ban_id, cmd.lifted_by, cmd.lift_reason.as_deref())
            .await?;

        info!(ban_id = %ban.id, lifted_by = %cmd.lifted_by, "Ban lifted");

        Ok(ban)
    }

    /// Check if a user is currently platform banned.
    #[instrument(skip(self))]
    pub async fn is_user_banned(&self, user_id: UserId) -> Result<bool, DomainError> {
        self.ban_repo.is_platform_banned(user_id).await
    }

    /// Get all active bans for a user.
    #[instrument(skip(self))]
    pub async fn get_active_bans(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError> {
        self.ban_repo.get_active_for_user(user_id).await
    }

    /// Get a user's complete ban history.
    #[instrument(skip(self))]
    pub async fn get_user_ban_history(&self, user_id: UserId) -> Result<Vec<Ban>, DomainError> {
        self.ban_repo.get_user_ban_history(user_id).await
    }

    /// List bans with filtering and pagination.
    #[instrument(skip(self))]
    pub async fn list_bans(
        &self,
        filters: BanFilters,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedBans, DomainError> {
        self.ban_repo.list(filters, page, per_page).await
    }
}

impl<BR> Clone for BanService<BR>
where
    BR: BanRepository,
{
    fn clone(&self) -> Self {
        Self {
            ban_repo: Arc::clone(&self.ban_repo),
        }
    }
}
