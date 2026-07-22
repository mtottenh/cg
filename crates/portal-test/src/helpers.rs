//! Shared test helper functions.
//!
//! These helpers consolidate common test operations that were previously
//! duplicated across multiple test files with raw SQL queries. They use
//! repository methods from `portal-db` for consistency and maintainability.
//!
//! ## Example
//!
//! ```ignore
//! use portal_test::helpers::{get_cs2_game_id, assign_role_to_user, create_test_token};
//!
//! // Get CS2 game ID using repository
//! let game_id = get_cs2_game_id(&pool).await;
//!
//! // Assign a role to a user
//! assign_role_to_user(&pool, user_id, "admin").await;
//!
//! // Create a test JWT token
//! let token = create_test_token(user_id, player_id, "testuser", "jwt-secret");
//! ```

use portal_db::DbPool;
use portal_db::repositories::{GameRepository, RoleRepository};
use portal_domain::generate_access_token;
use uuid::Uuid;

/// Default JWT secret used in tests.
pub const TEST_JWT_SECRET: &str = "test-jwt-secret";

/// Get the CS2 game ID from the database.
///
/// CS2 is seeded by default in migrations, so this is guaranteed to exist.
///
/// # Panics
///
/// Panics if CS2 game is not found in the database (indicates corrupted test setup).
pub async fn get_cs2_game_id(pool: &DbPool) -> Uuid {
    get_game_id(pool, "cs2").await
}

/// Get a game ID by its slug.
///
/// # Panics
///
/// Panics if the game is not found in the database.
pub async fn get_game_id(pool: &DbPool, slug: &str) -> Uuid {
    let repo = GameRepository::new(pool.clone());
    repo.find_by_slug(slug)
        .await
        .expect("Database query failed")
        .unwrap_or_else(|| panic!("Game '{slug}' not found in database"))
        .id
}

/// Assign a global role to a user by role name.
///
/// This is useful for assigning roles like "admin", "moderator", etc.
/// for testing admin-only endpoints.
///
/// This function is idempotent - if the role is already assigned, it will
/// silently succeed without error.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `user_id` - The user ID to assign the role to
/// * `role_name` - The name of the role (e.g., "admin", "moderator")
///
/// # Panics
///
/// Panics if the role doesn't exist or if an unexpected error occurs.
pub async fn assign_role_to_user(pool: &DbPool, user_id: Uuid, role_name: &str) {
    let repo = RoleRepository::new(pool.clone());

    // Find the role by name
    let role = repo
        .find_by_name(role_name)
        .await
        .expect("Database query failed")
        .unwrap_or_else(|| panic!("Role '{role_name}' not found in database"));

    // Assign the role globally (no scope)
    let assignment = portal_db::entities::NewUserRole {
        user_id,
        role_id: role.id,
        scope_type: None,
        scope_id: None,
        granted_by: None,
        expires_at: None,
    };

    // Attempt to assign - ignore duplicate key errors (user already has role)
    match repo.assign_to_user(assignment).await {
        Ok(_) => {}
        Err(portal_db::RepositoryError::Duplicate { .. }) => {
            // Role already assigned, this is fine
        }
        Err(e) => panic!("Failed to assign role to user: {e}"),
    }
}

/// Assign a scoped role to a user (e.g., team captain, league admin).
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `user_id` - The user ID to assign the role to
/// * `role_name` - The name of the role (e.g., "team_captain", "league_admin")
/// * `scope_type` - The type of scope (e.g., "team", "league", "tournament")
/// * `scope_id` - The ID of the scoped entity
///
/// # Panics
///
/// Panics if the role doesn't exist or the assignment fails.
pub async fn assign_scoped_role_to_user(
    pool: &DbPool,
    user_id: Uuid,
    role_name: &str,
    scope_type: portal_core::ScopeType,
    scope_id: Uuid,
) {
    let repo = RoleRepository::new(pool.clone());

    repo.assign_scoped_role(user_id, role_name, scope_type, scope_id, None)
        .await
        .expect("Failed to assign scoped role to user");
}

/// Create a JWT test token for authentication.
///
/// # Arguments
///
/// * `user_id` - The user ID to encode in the token
/// * `player_id` - The player ID to encode in the token
/// * `username` - The username to encode in the token
/// * `secret` - The JWT secret (use `TEST_JWT_SECRET` for consistency)
///
/// # Returns
///
/// A JWT token string that can be used in `Authorization: Bearer <token>` header.
///
/// # Panics
///
/// Panics if token generation fails (should not happen with valid inputs).
pub fn create_test_token(user_id: Uuid, player_id: Uuid, username: &str, secret: &str) -> String {
    generate_access_token(user_id, player_id, username, secret).expect("Failed to create token")
}

/// Create a JWT test token with admin privileges.
///
/// # Arguments
///
/// * `user_id` - The user ID to encode in the token
/// * `player_id` - The player ID to encode in the token
/// * `username` - The username to encode in the token
/// * `secret` - The JWT secret (use `TEST_JWT_SECRET` for consistency)
///
/// # Returns
///
/// A JWT token string for the given user. Admin status is not encoded in the
/// token — tests that need admin privileges must also grant the user the
/// appropriate RBAC role (e.g. `super_admin`) in the database. Kept as
/// `create_admin_token` for test-side readability.
///
/// # Panics
///
/// Panics if token generation fails.
pub fn create_admin_token(user_id: Uuid, player_id: Uuid, username: &str, secret: &str) -> String {
    portal_domain::generate_access_token(user_id, player_id, username, secret)
        .expect("Failed to create admin token")
}

/// Get the dev user ID from the seeded test data.
///
/// The dev user is seeded by migrations and is used for testing with dev auth mode.
///
/// # Panics
///
/// Panics if the dev user is not found.
pub async fn get_dev_user_id(pool: &DbPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = 'devuser'")
        .fetch_one(pool)
        .await
        .expect("Dev user should exist (seeded by migrations)")
}

/// Get the dev user's player ID from the seeded test data.
///
/// # Panics
///
/// Panics if the dev player is not found.
pub async fn get_dev_player_id(pool: &DbPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT p.id FROM players p JOIN users u ON u.id = p.user_id WHERE u.username = 'devuser'",
    )
    .fetch_one(pool)
    .await
    .expect("Dev player should exist (seeded by migrations)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::UserBuilder;
    use crate::database::TestDb;

    #[tokio::test]
    async fn test_get_cs2_game_id() {
        let db = TestDb::new().await;
        let game_id = get_cs2_game_id(&db.pool).await;
        assert!(!game_id.is_nil());
    }

    #[tokio::test]
    async fn test_get_game_id() {
        let db = TestDb::new().await;
        // CS2 and AoE4 are seeded by migrations
        let cs2_id = get_game_id(&db.pool, "cs2").await;
        let aoe4_id = get_game_id(&db.pool, "aoe4").await;

        assert!(!cs2_id.is_nil());
        assert!(!aoe4_id.is_nil());
        assert_ne!(cs2_id, aoe4_id);
    }

    #[tokio::test]
    async fn test_assign_role_to_user() {
        let db = TestDb::new().await;

        // Create a test user
        let user = UserBuilder::new()
            .username("roletest")
            .build_persisted(&db.pool)
            .await;

        // Assign super_admin role (seeded by migrations)
        assign_role_to_user(&db.pool, user.id, "super_admin").await;

        // Verify the role was assigned
        let repo = RoleRepository::new(db.pool.clone());
        let roles = repo
            .get_user_roles(portal_core::UserId::from(user.id))
            .await
            .expect("Failed to get user roles");

        assert!(roles.iter().any(|r| r.name == "super_admin"));
    }

    #[tokio::test]
    async fn test_create_test_token() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();

        let token = create_test_token(user_id, player_id, "testuser", TEST_JWT_SECRET);

        // Token should be a valid JWT format (3 parts separated by dots)
        assert_eq!(token.split('.').count(), 3);
    }

    #[tokio::test]
    async fn test_create_admin_token() {
        let user_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();

        let token = create_admin_token(user_id, player_id, "adminuser", TEST_JWT_SECRET);

        // Token should be a valid JWT format
        assert_eq!(token.split('.').count(), 3);
    }

    // Note: Dev user is only seeded in development environments, not in test databases.
    // These helper functions are for integration tests that use DEV_AUTH mode.
    // Skipping this test as the dev user won't be present in test containers.
    #[tokio::test]
    #[ignore = "dev user only seeded in development environment, not test containers"]
    async fn test_get_dev_user_and_player() {
        let db = TestDb::new().await;

        let user_id = get_dev_user_id(&db.pool).await;
        let player_id = get_dev_player_id(&db.pool).await;

        assert!(!user_id.is_nil());
        assert!(!player_id.is_nil());
    }
}
