//! Ban service with business logic.

use crate::entities::user::UserStatus;
use crate::entities::{Ban, BanFilters, BanType, CreateBanCommand, LiftBanCommand};
use crate::repositories::refresh_token::RefreshTokenRepository;
use crate::repositories::{BanRepository, PaginatedBans, UserRepository};
use portal_core::{BanId, DomainError, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for ban-related business logic.
///
/// Platform-scoped bans are *enforced* here, not just recorded: creating
/// one flips `users.status` to `banned` and revokes every refresh token
/// for the user (so residual access is capped at the access-token
/// lifetime), mirroring the CLI ban path. Lifting the last active
/// platform ban restores `active`. Scoped bans (league/tournament/chat/
/// matchmaking) only write the bans table, as before.
pub struct BanService<BR, UR, RT>
where
    BR: BanRepository,
    UR: UserRepository,
    RT: RefreshTokenRepository,
{
    ban_repo: Arc<BR>,
    user_repo: Arc<UR>,
    refresh_token_repo: Arc<RT>,
}

impl<BR, UR, RT> BanService<BR, UR, RT>
where
    BR: BanRepository,
    UR: UserRepository,
    RT: RefreshTokenRepository,
{
    /// Create a new ban service.
    pub const fn new(ban_repo: Arc<BR>, user_repo: Arc<UR>, refresh_token_repo: Arc<RT>) -> Self {
        Self {
            ban_repo,
            user_repo,
            refresh_token_repo,
        }
    }

    /// Get a ban by ID.
    #[instrument(skip(self))]
    pub async fn get_ban(&self, id: BanId) -> Result<Ban, DomainError> {
        self.ban_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::BanNotFound(id))
    }

    /// Create a new ban.
    ///
    /// Platform bans additionally set the user's account status to
    /// `banned` and revoke all their refresh tokens, so the ban takes
    /// effect immediately (login and token refresh both gate on
    /// `users.status`).
    ///
    /// # Arguments
    /// * `cmd` - The ban creation command
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
                    (Some(a), Some(b)) => a == b && existing.scope_id == cmd.scope_id,
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

        // Enforce platform bans immediately. Ordering matters: the ban row
        // is committed first, so a failure here leaves an over-restrictive
        // record (visible, liftable) rather than an unenforced ban.
        if ban.ban_type == BanType::Platform && ban.is_active() {
            self.user_repo
                .update_status(ban.user_id, UserStatus::Banned, Some(&ban.reason))
                .await?;
            self.refresh_token_repo
                .revoke_all_for_user(ban.user_id.as_uuid())
                .await?;
            info!(
                ban_id = %ban.id,
                user_id = %ban.user_id,
                "Platform ban enforced: status set to banned, refresh tokens revoked"
            );
        }

        Ok(ban)
    }

    /// Lift (revoke) a ban early.
    ///
    /// Lifting the last active platform ban restores the user's account
    /// status to `active` (only when the account is currently `banned` —
    /// a CLI/admin suspension is never clobbered).
    #[instrument(skip(self))]
    pub async fn lift_ban(&self, cmd: LiftBanCommand) -> Result<Ban, DomainError> {
        // Verify ban exists
        let ban = self.get_ban(cmd.ban_id).await?;

        // Business rule: Can't lift an already lifted ban
        if ban.is_lifted() {
            return Err(DomainError::InvalidState(
                "Ban has already been lifted".into(),
            ));
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

        // Restore account status when no active platform ban remains.
        if ban.ban_type == BanType::Platform {
            let still_platform_banned = self
                .ban_repo
                .get_active_for_user(ban.user_id)
                .await?
                .iter()
                .any(|b| b.ban_type == BanType::Platform && b.is_active());

            if !still_platform_banned {
                let user = self
                    .user_repo
                    .find_by_id(ban.user_id)
                    .await?
                    .ok_or(DomainError::UserNotFound(ban.user_id))?;
                if user.status == UserStatus::Banned {
                    self.user_repo
                        .update_status(ban.user_id, UserStatus::Active, cmd.lift_reason.as_deref())
                        .await?;
                    info!(
                        ban_id = %ban.id,
                        user_id = %ban.user_id,
                        "Platform ban lifted: account status restored to active"
                    );
                }
            }
        }

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

impl<BR, UR, RT> Clone for BanService<BR, UR, RT>
where
    BR: BanRepository,
    UR: UserRepository,
    RT: RefreshTokenRepository,
{
    fn clone(&self) -> Self {
        Self {
            ban_repo: Arc::clone(&self.ban_repo),
            user_repo: Arc::clone(&self.user_repo),
            refresh_token_repo: Arc::clone(&self.refresh_token_repo),
        }
    }
}
