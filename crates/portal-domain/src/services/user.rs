//! User service with business logic.

use crate::auth::{hash_password, verify_dummy_for_timing, verify_password};
use crate::entities::user::UserStatus;
use crate::entities::{Player, User};
use crate::jwt::generate_access_token;
use crate::repositories::{CreatePlayer, CreateUser, PlayerRepository, UserRepository};
use portal_core::{DomainError, PlayerId, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Command for registering a new user.
#[derive(Debug, Clone)]
pub struct RegisterUserCommand {
    /// Username.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Plain-text password (will be hashed).
    pub password: String,
    /// Display name for the player profile.
    pub display_name: String,
}

/// Command for authenticating a user.
#[derive(Debug, Clone)]
pub struct LoginCommand {
    /// Username or email.
    pub username_or_email: String,
    /// Plain-text password.
    pub password: String,
}

/// Result of successful authentication.
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// JWT access token.
    pub access_token: String,
    /// User ID.
    pub user_id: UserId,
    /// Player ID.
    pub player_id: PlayerId,
    /// Username.
    pub username: String,
}

/// Service for user-related business logic.
pub struct UserService<UR, PR>
where
    UR: UserRepository,
    PR: PlayerRepository,
{
    user_repo: Arc<UR>,
    player_repo: Arc<PR>,
}

impl<UR, PR> UserService<UR, PR>
where
    UR: UserRepository,
    PR: PlayerRepository,
{
    /// Create a new user service.
    pub const fn new(user_repo: Arc<UR>, player_repo: Arc<PR>) -> Self {
        Self {
            user_repo,
            player_repo,
        }
    }

    /// Get a user by ID.
    #[instrument(skip(self))]
    pub async fn get_user(&self, id: UserId) -> Result<User, DomainError> {
        self.user_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::UserNotFound(id.to_string()))
    }

    /// Get the current authenticated user.
    ///
    /// This is the same as `get_user` but semantically indicates it's for the "me" endpoint.
    #[instrument(skip(self))]
    pub async fn get_current_user(&self, id: UserId) -> Result<User, DomainError> {
        self.get_user(id).await
    }

    /// Find a user by email.
    #[instrument(skip(self))]
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError> {
        self.user_repo.find_by_email(email).await
    }

    /// Find a user by username.
    #[instrument(skip(self))]
    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        self.user_repo.find_by_username(username).await
    }

    /// Check if a username is available.
    #[instrument(skip(self))]
    pub async fn is_username_available(&self, username: &str) -> Result<bool, DomainError> {
        let exists = self.user_repo.username_exists(username).await?;
        Ok(!exists)
    }

    /// Check if an email is available.
    #[instrument(skip(self))]
    pub async fn is_email_available(&self, email: &str) -> Result<bool, DomainError> {
        let exists = self.user_repo.email_exists(email).await?;
        Ok(!exists)
    }

    /// Register a new user with a player profile.
    ///
    /// This creates both a User and a Player in a single operation.
    /// The Player ID will match the User ID for 1:1 mapping.
    #[instrument(skip(self, cmd), fields(username = %cmd.username, email = %cmd.email))]
    pub async fn register_user(
        &self,
        cmd: RegisterUserCommand,
    ) -> Result<(User, Player), DomainError> {
        // Check if username is already taken
        if self.user_repo.username_exists(&cmd.username).await? {
            return Err(DomainError::Conflict(format!(
                "username '{}' is already taken",
                cmd.username
            )));
        }

        // Check if email is already taken
        if self.user_repo.email_exists(&cmd.email).await? {
            return Err(DomainError::Conflict(format!(
                "email '{}' is already registered",
                cmd.email
            )));
        }

        // Hash the password (dispatched to spawn_blocking inside hash_password)
        let password_hash = hash_password(cmd.password).await?;

        // Generate a shared ID for User and Player
        let user_id = UserId::new();
        let player_id = PlayerId::from(user_id.as_uuid());

        // Create the user
        let user = self
            .user_repo
            .create(CreateUser {
                id: Some(user_id),
                username: cmd.username,
                email: cmd.email,
                password_hash,
            })
            .await?;

        // Create the player profile with the same ID
        let player = self
            .player_repo
            .create(CreatePlayer {
                id: player_id,
                user_id,
                display_name: cmd.display_name,
            })
            .await?;

        info!(user_id = %user.id, player_id = %player.id, "User registered");

        Ok((user, player))
    }

    /// Authenticate a user and return an access token.
    ///
    /// # Arguments
    /// * `cmd` - Login credentials
    /// * `jwt_secret` - Secret key for signing the JWT
    ///
    /// # Returns
    /// Authentication result including the access token on success.
    #[instrument(skip(self, cmd, jwt_secret), fields(username_or_email = %cmd.username_or_email))]
    pub async fn authenticate(
        &self,
        cmd: LoginCommand,
        jwt_secret: &str,
    ) -> Result<AuthResult, DomainError> {
        // Find user by username or email. If they don't exist, spend the
        // same Argon2 work as a real verify so an attacker can't enumerate
        // accounts via response timing.
        let user_creds = match self
            .user_repo
            .find_for_auth(&cmd.username_or_email)
            .await?
        {
            Some(c) => c,
            None => {
                verify_dummy_for_timing().await?;
                return Err(DomainError::InvalidCredentials);
            }
        };

        // Check if account is active
        if user_creds.status != UserStatus::Active {
            return Err(DomainError::Forbidden(format!(
                "account is {}",
                user_creds.status
            )));
        }

        // Verify password. Same dummy-verify treatment for users without a
        // password hash (e.g. social-only accounts) so the existence of such
        // accounts isn't observable via timing either.
        let password_hash = match user_creds.password_hash.clone() {
            Some(h) => h,
            None => {
                verify_dummy_for_timing().await?;
                return Err(DomainError::InvalidCredentials);
            }
        };

        let is_valid = verify_password(cmd.password, password_hash).await?;
        if !is_valid {
            return Err(DomainError::InvalidCredentials);
        }

        // Find player to confirm they exist
        let player = self
            .player_repo
            .find_by_user_id(user_creds.id)
            .await?
            .ok_or_else(|| DomainError::PlayerNotFound(user_creds.id.to_string()))?;

        // Generate access token
        let access_token = generate_access_token(
            user_creds.id.as_uuid(),
            player.id.as_uuid(),
            &user_creds.username,
            jwt_secret,
        )?;

        // Update last login timestamp
        self.user_repo.update_last_login(user_creds.id).await?;

        info!(user_id = %user_creds.id, "User authenticated successfully");

        Ok(AuthResult {
            access_token,
            user_id: user_creds.id,
            player_id: player.id,
            username: user_creds.username,
        })
    }

    /// Generate a token for a user (used after registration).
    ///
    /// # Arguments
    /// * `user` - The user to generate a token for
    /// * `player` - The player profile
    /// * `jwt_secret` - Secret key for signing the JWT
    #[instrument(skip(self, jwt_secret))]
    pub fn generate_token_for_user(
        &self,
        user: &User,
        player: &Player,
        jwt_secret: &str,
    ) -> Result<String, DomainError> {
        generate_access_token(
            user.id.as_uuid(),
            player.id.as_uuid(),
            &user.username,
            jwt_secret,
        )
    }
}

impl<UR, PR> Clone for UserService<UR, PR>
where
    UR: UserRepository,
    PR: PlayerRepository,
{
    fn clone(&self) -> Self {
        Self {
            user_repo: Arc::clone(&self.user_repo),
            player_repo: Arc::clone(&self.player_repo),
        }
    }
}
