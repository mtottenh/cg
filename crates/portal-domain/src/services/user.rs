//! User service with business logic.

use crate::auth::{hash_password, verify_dummy_for_timing, verify_password};
use crate::entities::user::UserStatus;
use crate::entities::{Player, User};
use crate::jwt::generate_access_token;
use crate::repositories::{
    CreatePlayer, CreateUser, PlayerRepository, UpdatePlayer, UserRepository,
};
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

/// Generate a `(UserId, PlayerId)` pair whose underlying UUIDs are equal.
///
/// This is the single construction site that preserves the platform's
/// 1:1 user-to-player invariant. Every place that creates both IDs for a
/// newly-registered account should go through here so the invariant is
/// grep-able and future-us can flip it in one spot. See the docstring on
/// [`UserService::register_user`] for the full rationale (audit item N6).
#[must_use]
pub fn make_shared_account_ids() -> (UserId, PlayerId) {
    let user_id = UserId::new();
    let player_id = PlayerId::from(user_id.as_uuid());
    (user_id, player_id)
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
            .ok_or(DomainError::UserNotFound(id))
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
    ///
    /// # Shared identifier
    ///
    /// The Player ID is deliberately generated from the User ID's UUID
    /// (see [`make_shared_account_ids`]). That means at runtime every
    /// stored row satisfies `player.id.as_uuid() == user.id.as_uuid()`,
    /// and callers can navigate between the two without a DB lookup.
    ///
    /// This does blur the type-safety story the newtype pattern usually
    /// buys: `UserId` and `PlayerId` are different types at the compiler
    /// level, but the UUIDs underneath are the same. The audit flagged
    /// this (N6) and the decision was:
    ///
    /// 1. Keep the 1:1 invariant — matchmaking and lobby flows (planned)
    ///    assume a player maps to exactly one user. Breaking that now is
    ///    a data-model change that would require migrating every stored
    ///    player row.
    /// 2. Document the invariant explicitly rather than pretending the
    ///    IDs are unrelated. `make_shared_account_ids` is the single
    ///    construction site; future code that needs both IDs for a new
    ///    user should call through it.
    /// 3. If we ever need distinct IDs (e.g. multiple players per user
    ///    for alt accounts), migrate by pointing `register_user` at
    ///    `PlayerId::new()` and running a one-time data fix.
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

        // See the `# Shared identifier` note on `register_user`. Routing
        // every new-user-ID construction through `make_shared_account_ids`
        // makes the invariant obvious to future readers and gives us a
        // single seam to change when/if we decouple the two IDs.
        let (user_id, player_id) = make_shared_account_ids();

        // Create the user
        let user = self
            .user_repo
            .create(CreateUser {
                id: Some(user_id),
                username: cmd.username,
                email: cmd.email,
                password_hash: Some(password_hash),
                auth_provider: "local".to_string(),
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

    /// Find or create the account backing a verified Steam sign-in.
    ///
    /// Called *after* the Steam OpenID assertion has been verified (the
    /// caller owns that; this service never talks to Steam). Matches an
    /// existing player by `steam_id_64`; when none exists, provisions a
    /// new user (auth\_provider `steam`, no usable password) plus player
    /// with the `steam_id_64` set.
    ///
    /// Returns `(user, player, created)` where `created` is true when a
    /// new account was provisioned (the caller grants the default role).
    #[instrument(skip(self, persona_name))]
    pub async fn login_with_steam(
        &self,
        steam_id_64: i64,
        persona_name: Option<&str>,
    ) -> Result<(User, Player, bool), DomainError> {
        // Existing player with this SteamID64 → that account wins,
        // regardless of how it was originally created (local accounts
        // that linked their Steam ID included).
        if let Some(player) = self.player_repo.find_by_steam_id_64(steam_id_64).await? {
            let user = self.get_user(player.user_id).await?;
            if user.status != UserStatus::Active {
                return Err(DomainError::Forbidden(format!(
                    "account is {}",
                    user.status
                )));
            }
            // Self-heal: accounts provisioned before a persona was
            // obtainable carry the `steam_<id64>` placeholder as their
            // display name. Once we do know the persona, upgrade the
            // placeholder — but never overwrite a name the player (or a
            // previous enrichment) actually chose.
            let player = match persona_name.map(str::trim).filter(|p| !p.is_empty()) {
                Some(persona) if player.display_name == format!("steam_{steam_id_64}") => {
                    self.player_repo
                        .update(
                            player.id,
                            UpdatePlayer {
                                display_name: Some(persona.chars().take(32).collect()),
                                ..UpdatePlayer::default()
                            },
                        )
                        .await?
                }
                _ => player,
            };
            self.user_repo.update_last_login(user.id).await?;
            return Ok((user, player, false));
        }

        // Placeholder email — Steam's OpenID flow provides no email. The
        // address is deterministic per SteamID64 so a partially-created
        // account (user row inserted, player insert failed) is recovered
        // on the next sign-in instead of conflicting forever.
        let email = format!("steam_{steam_id_64}@steam.invalid");
        if let Some(user) = self.user_repo.find_by_email(&email).await? {
            let player = self
                .recover_partial_steam_account(&user, steam_id_64)
                .await?;
            self.user_repo.update_last_login(user.id).await?;
            return Ok((user, player, false));
        }

        let username = self
            .derive_available_username(persona_name, steam_id_64)
            .await?;
        let (user_id, player_id) = make_shared_account_ids();

        let user = self
            .user_repo
            .create(CreateUser {
                id: Some(user_id),
                username,
                email,
                password_hash: None,
                auth_provider: "steam".to_string(),
            })
            .await?;

        // Display name: the Steam persona verbatim (truncated to the
        // 32-char column) when we have it, else the generated username.
        let display_name = persona_name
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map_or_else(|| user.username.clone(), |p| p.chars().take(32).collect());

        let player = self
            .player_repo
            .create(CreatePlayer {
                id: player_id,
                user_id,
                display_name,
            })
            .await?;

        // Stamp the SteamID64 on the player (sets both steam_id and
        // steam_id_64 columns).
        let player = self
            .player_repo
            .update(
                player.id,
                UpdatePlayer {
                    steam_id: Some(steam_id_64.to_string()),
                    steam_id_64: Some(steam_id_64),
                    ..UpdatePlayer::default()
                },
            )
            .await?;

        self.user_repo.update_last_login(user.id).await?;
        info!(user_id = %user.id, steam_id_64, "User provisioned via Steam sign-in");

        Ok((user, player, true))
    }

    /// Finish provisioning an account whose user row exists but whose
    /// player row is missing or lacks the SteamID64 (a previous Steam
    /// sign-in died between the two inserts).
    async fn recover_partial_steam_account(
        &self,
        user: &User,
        steam_id_64: i64,
    ) -> Result<Player, DomainError> {
        let update = UpdatePlayer {
            steam_id: Some(steam_id_64.to_string()),
            steam_id_64: Some(steam_id_64),
            ..UpdatePlayer::default()
        };
        if let Some(player) = self.player_repo.find_by_user_id(user.id).await? {
            return self.player_repo.update(player.id, update).await;
        }
        let player = self
            .player_repo
            .create(CreatePlayer {
                id: PlayerId::from(user.id.as_uuid()),
                user_id: user.id,
                display_name: user.username.clone(),
            })
            .await?;
        self.player_repo.update(player.id, update).await
    }

    /// Derive a username satisfying the platform constraint
    /// (`^[a-zA-Z0-9_-]{3,32}$`, unique) from a Steam persona name,
    /// falling back to `steam_<id64>` and numeric suffixes on collision.
    async fn derive_available_username(
        &self,
        persona_name: Option<&str>,
        steam_id_64: i64,
    ) -> Result<String, DomainError> {
        let mut candidates: Vec<String> = Vec::new();
        if let Some(persona) = persona_name {
            let sanitized: String = persona
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .take(32)
                .collect();
            if sanitized.len() >= 3 {
                candidates.push(sanitized);
            }
        }
        candidates.push(format!("steam_{steam_id_64}"));

        for base in &candidates {
            if !self.user_repo.username_exists(base).await? {
                return Ok(base.clone());
            }
            for n in 2..=9u32 {
                let suffix = format!("_{n}");
                let head: String = base
                    .chars()
                    .take(32_usize.saturating_sub(suffix.len()))
                    .collect();
                let candidate = format!("{head}{suffix}");
                if !self.user_repo.username_exists(&candidate).await? {
                    return Ok(candidate);
                }
            }
        }

        Err(DomainError::Conflict(
            "could not derive an available username for Steam account".to_string(),
        ))
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
        let Some(user_creds) = self.user_repo.find_for_auth(&cmd.username_or_email).await? else {
            verify_dummy_for_timing().await?;
            return Err(DomainError::InvalidCredentials);
        };

        // Check if account is active
        if user_creds.status != UserStatus::Active {
            return Err(DomainError::Forbidden(format!(
                "account is {}",
                user_creds.status
            )));
        }

        // Provider-authenticated accounts (e.g. Steam) have no usable
        // password. Tell the user to use the right sign-in method instead
        // of a generic "invalid credentials".
        if user_creds.auth_provider != "local" {
            return Err(DomainError::WrongAuthProvider(
                user_creds.auth_provider.clone(),
            ));
        }

        // Verify password. Same dummy-verify treatment for users without a
        // password hash (e.g. social-only accounts) so the existence of such
        // accounts isn't observable via timing either.
        let Some(password_hash) = user_creds.password_hash.clone() else {
            verify_dummy_for_timing().await?;
            return Err(DomainError::InvalidCredentials);
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
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "player",
                query: format!("user:{}", user_creds.id),
            })?;

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
