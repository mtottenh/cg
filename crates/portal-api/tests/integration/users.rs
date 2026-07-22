//! User API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;

#[tokio::test]
async fn test_get_current_user() {
    let app = TestApp::new().await;

    // Dev user is already seeded by migration 0013_seed_dev_user.sql
    // Just make the authenticated request
    let response = app.get_auth("/v1/users/me").await;

    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["username"], "devuser");
    assert_eq!(body["data"]["email"], "dev@example.com");
}

#[tokio::test]
async fn test_get_current_user_unauthorized() {
    let app = TestApp::new().await;

    // Try without authentication
    let response = app.get("/v1/users/me").await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ACCOUNT PROVISIONING ATOMICITY
// ============================================================================

/// Local registration used to insert the user and the player on two
/// separate connections. A failed player insert left an orphan user row:
/// re-registration then tripped the username/email uniqueness checks and
/// login died at the player lookup, so the username and email were bricked
/// with no recovery path.
///
/// `create_account` does both inserts in one transaction. Forcing the
/// player insert to fail (a colliding player primary key) must roll the
/// user insert back.
#[tokio::test]
async fn test_create_account_rolls_back_user_when_player_insert_fails() {
    use portal_core::{PlayerId, UserId};
    use portal_db::adapters::PgUserRepository;
    use portal_domain::repositories::{CreatePlayer, CreateUser, UserRepository};
    use portal_test::prelude::*;

    let app = TestApp::new().await;
    let repo = PgUserRepository::new(app.pool().clone());

    // An existing account whose player id we will collide with.
    let existing = UserBuilder::new().build_persisted(app.pool()).await;

    let username = format!("at_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let email = format!("{username}@example.com");

    let result = repo
        .create_account(
            CreateUser {
                id: Some(UserId::from(uuid::Uuid::now_v7())),
                username: username.clone(),
                email: email.clone(),
                password_hash: Some("$argon2id$fake".to_string()),
                auth_provider: "local".to_string(),
            },
            CreatePlayer {
                // Collides with the existing player's primary key.
                id: PlayerId::from(existing.id),
                user_id: UserId::from(uuid::Uuid::nil()), // overridden by the adapter
                display_name: username.clone(),
            },
        )
        .await;

    assert!(result.is_err(), "colliding player insert must fail");

    let orphans: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = $1 OR email = $2")
            .bind(&username)
            .bind(&email)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(
        orphans, 0,
        "the user row must be rolled back, leaving the username/email reusable"
    );
}

/// The happy path still writes both rows.
#[tokio::test]
async fn test_create_account_persists_user_and_player() {
    use portal_core::{PlayerId, UserId};
    use portal_db::adapters::PgUserRepository;
    use portal_domain::repositories::{CreatePlayer, CreateUser, UserRepository};

    let app = TestApp::new().await;
    let repo = PgUserRepository::new(app.pool().clone());

    let id = uuid::Uuid::now_v7();
    let username = format!("ok_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);

    let (user, player) = repo
        .create_account(
            CreateUser {
                id: Some(UserId::from(id)),
                username: username.clone(),
                email: format!("{username}@example.com"),
                password_hash: Some("$argon2id$fake".to_string()),
                auth_provider: "local".to_string(),
            },
            CreatePlayer {
                id: PlayerId::from(id),
                user_id: UserId::from(id),
                display_name: username.clone(),
            },
        )
        .await
        .expect("account creation should succeed");

    assert_eq!(user.id.as_uuid(), id);
    assert_eq!(player.id.as_uuid(), id, "player shares the user's UUID");
    assert_eq!(player.user_id.as_uuid(), id);
}
