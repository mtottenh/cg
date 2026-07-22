//! Ban system business rule tests.
//!
//! These tests verify the design constraints from the ban system design documents.
//! Some tests may initially FAIL (Red) if constraints aren't enforced yet - that's expected TDD.

use chrono::{Duration, Utc};
use portal_db::DbPool;
use portal_db::entities::NewBan;
use portal_db::repositories::BanRepository;
use portal_test::database::TestDb;
use uuid::Uuid;

// ===========================================
// Test Helpers
// ===========================================

async fn create_test_user(pool: &DbPool, suffix: &str) -> Uuid {
    let user = sqlx::query_as::<_, (Uuid,)>(
        r"
        INSERT INTO users (username, email, password_hash)
        VALUES ($1, $2, 'hash')
        RETURNING id
        ",
    )
    .bind(format!("banrulesuser{suffix}"))
    .bind(format!("banrules{suffix}@example.com"))
    .fetch_one(pool)
    .await
    .unwrap();
    user.0
}

// ===========================================
// Ban System Rule Tests
// ===========================================

/// Test that an active platform ban blocks user access.
#[tokio::test]
async fn test_active_ban_blocks_user() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "blocked").await;

    // User is not banned initially
    assert!(!repo.is_banned(user_id).await.unwrap());

    // Create an active platform ban
    let new_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Test platform ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: None,
        ends_at: None, // Permanent
    };
    repo.create(new_ban).await.unwrap();

    // User should now be blocked
    assert!(repo.is_banned(user_id).await.unwrap());
}

/// Test that an expired ban no longer affects the user.
#[tokio::test]
async fn test_expired_ban_no_longer_active() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "expired").await;

    // Create a ban that has already expired (starts_at must be before ends_at)
    let new_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Expired ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: Some(Utc::now() - Duration::hours(2)),
        ends_at: Some(Utc::now() - Duration::hours(1)), // Expired
    };
    repo.create(new_ban).await.unwrap();

    // Expired ban should not block user
    assert!(!repo.is_banned(user_id).await.unwrap());

    // Expired ban should not appear in active bans
    let active = repo.get_active(user_id).await.unwrap();
    assert!(
        active.is_empty(),
        "Expired ban should not be in active list"
    );
}

/// Test that a lifted ban no longer affects the user.
#[tokio::test]
async fn test_lifted_ban_no_longer_active() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "lifted").await;

    // Create an active ban
    let new_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Will be lifted".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: None,
        ends_at: None,
    };
    repo.create(new_ban).await.unwrap();

    // Verify user is banned
    assert!(repo.is_banned(user_id).await.unwrap());

    // Lift the ban
    let lifted = repo.lift(user_id, None, Some("Appealed")).await.unwrap();
    assert_eq!(lifted.len(), 1);

    // Lifted ban should not block user
    assert!(!repo.is_banned(user_id).await.unwrap());

    // Lifted ban should not appear in active bans
    let active = repo.get_active(user_id).await.unwrap();
    assert!(active.is_empty(), "Lifted ban should not be in active list");
}

/// Test that multiple bans are all checked - any active platform ban blocks access.
#[tokio::test]
async fn test_multiple_bans_all_checked() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "multi").await;

    // Create an expired ban (starts_at must be before ends_at). Use a
    // different ban_type from the active platform ban below so both can
    // coexist: migration 0076's partial unique index allows only one
    // non-lifted ban per (user_id, ban_type) — and an expired-but-not-lifted
    // ban still occupies that slot (a partial index predicate can't reference
    // NOW()), so two same-type unscoped bans would collide.
    let expired_ban = NewBan {
        user_id,
        ban_type: "matchmaking".to_string(),
        reason: "Old expired ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: Some(Utc::now() - Duration::days(60)),
        ends_at: Some(Utc::now() - Duration::days(30)), // Long expired
    };
    repo.create(expired_ban).await.unwrap();

    // User should not be banned (only expired ban)
    assert!(!repo.is_banned(user_id).await.unwrap());

    // Create an active ban
    let active_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Current active ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: None,
        ends_at: None, // Permanent
    };
    repo.create(active_ban).await.unwrap();

    // User should now be banned
    assert!(repo.is_banned(user_id).await.unwrap());

    // Active bans should only include non-expired ones
    let active = repo.get_active(user_id).await.unwrap();
    assert_eq!(active.len(), 1, "Should only have one active ban");
}

/// Test that different ban types are handled correctly.
/// Only 'platform' bans block platform-wide access.
#[tokio::test]
async fn test_ban_types_scoped_correctly() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "scoped").await;

    // Create a league-scoped ban (not platform ban)
    let league_ban = NewBan {
        user_id,
        ban_type: "league".to_string(), // Not a platform ban
        reason: "League violation".to_string(),
        scope_type: Some("league".to_string()),
        scope_id: Some(Uuid::now_v7()),
        issued_by: None,
        starts_at: None,
        ends_at: None,
    };
    repo.create(league_ban).await.unwrap();

    // is_banned checks for platform bans only
    assert!(
        !repo.is_banned(user_id).await.unwrap(),
        "League ban should not trigger platform-wide block"
    );

    // But user should have an active ban record
    let active = repo.get_active(user_id).await.unwrap();
    assert_eq!(active.len(), 1, "User should have an active league ban");
}

/// Test that a timed ban becomes inactive after expiration.
#[tokio::test]
async fn test_timed_ban_expiry() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "timed").await;

    // Create a ban that expires in the future
    let future_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Temporary ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: None,
        ends_at: Some(Utc::now() + Duration::days(7)), // Expires in 7 days
    };
    repo.create(future_ban).await.unwrap();

    // User should be banned (ban hasn't expired yet)
    assert!(repo.is_banned(user_id).await.unwrap());

    // Active bans should include this ban
    let active = repo.get_active(user_id).await.unwrap();
    assert_eq!(active.len(), 1);
    assert!(
        active[0].ends_at.is_some(),
        "Timed ban should have an end date"
    );
}

/// Test that permanent bans (no end date) remain active indefinitely.
#[tokio::test]
async fn test_permanent_ban_stays_active() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "permanent").await;

    // Create a permanent ban
    let permanent_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Permanent ban".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: None,
        starts_at: None,
        ends_at: None, // No expiration = permanent
    };
    repo.create(permanent_ban).await.unwrap();

    // User should be banned
    assert!(repo.is_banned(user_id).await.unwrap());

    // Check the ban has no end date
    let active = repo.get_active(user_id).await.unwrap();
    assert_eq!(active.len(), 1);
    assert!(
        active[0].ends_at.is_none(),
        "Permanent ban should have no end date"
    );
}

/// Test that lifting a ban records the lift metadata.
#[tokio::test]
async fn test_lift_ban_records_metadata() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "liftmeta").await;
    let admin_id = create_test_user(&db.pool, "liftadmin").await;

    // Create a ban
    let new_ban = NewBan {
        user_id,
        ban_type: "platform".to_string(),
        reason: "Testing lift".to_string(),
        scope_type: None,
        scope_id: None,
        issued_by: Some(admin_id),
        starts_at: None,
        ends_at: None,
    };
    repo.create(new_ban).await.unwrap();

    // Lift the ban with metadata
    let lifted = repo
        .lift(user_id, Some(admin_id), Some("Appeal accepted"))
        .await
        .unwrap();

    assert_eq!(lifted.len(), 1);
    assert!(lifted[0].lifted_at.is_some(), "Should have lift timestamp");
    assert_eq!(lifted[0].lifted_by, Some(admin_id));
    assert_eq!(lifted[0].lift_reason, Some("Appeal accepted".to_string()));
}

/// Test that multiple active bans can coexist (e.g., platform ban + specific scope bans).
#[tokio::test]
async fn test_multiple_concurrent_bans() {
    let db = TestDb::new().await;
    let repo = BanRepository::new(db.pool.clone());

    let user_id = create_test_user(&db.pool, "concurrent").await;

    // Create multiple active bans
    let bans = vec![
        NewBan {
            user_id,
            ban_type: "platform".to_string(),
            reason: "Platform ban".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: None,
        },
        NewBan {
            user_id,
            ban_type: "matchmaking".to_string(),
            reason: "Matchmaking timeout".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: Some(Utc::now() + Duration::hours(1)),
        },
        NewBan {
            user_id,
            ban_type: "chat".to_string(),
            reason: "Chat mute".to_string(),
            scope_type: None,
            scope_id: None,
            issued_by: None,
            starts_at: None,
            ends_at: Some(Utc::now() + Duration::hours(24)),
        },
    ];

    for ban in bans {
        repo.create(ban).await.unwrap();
    }

    // All three bans should be active
    let active = repo.get_active(user_id).await.unwrap();
    assert_eq!(active.len(), 3, "All three bans should be active");

    // User should be banned (has platform ban)
    assert!(repo.is_banned(user_id).await.unwrap());
}
