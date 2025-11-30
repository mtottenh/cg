//! Test database management.
//!
//! Provides isolated `PostgreSQL` databases for testing using testcontainers.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use portal_test::prelude::*;
//!
//! #[tokio::test]
//! async fn test_with_database() {
//!     let db = TestDb::new().await;
//!
//!     // Each test gets its own isolated database
//!     sqlx::query("INSERT INTO users ...")
//!         .execute(db.pool())
//!         .await
//!         .unwrap();
//! }
//! ```
//!
//! ## Requirements
//!
//! - Docker must be installed and running
//! - First run pulls `postgres:16-alpine` (~150MB)

use crate::container::get_or_init_container;
use portal_db::DbPool;
use sqlx::migrate::MigrateDatabase;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, Postgres};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Counter for generating unique database names.
static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Test database wrapper that provides an isolated database.
///
/// Each `TestDb` instance creates a unique database within a shared
/// `PostgreSQL` container. The database is automatically dropped when
/// the `TestDb` is dropped.
pub struct TestDb {
    /// The database connection pool.
    pub pool: DbPool,
    /// Name of the test database.
    db_name: String,
}

impl TestDb {
    /// Create a new test database.
    ///
    /// This creates a fresh database with a unique name and runs all migrations.
    /// The first call will start the shared `PostgreSQL` container (takes ~3-5 sec),
    /// subsequent calls only create a new database (~100ms).
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Docker is not running
    /// - Container startup fails
    /// - Database creation or migration fails
    pub async fn new() -> Self {
        let container = get_or_init_container()
            .await
            .expect("Failed to start PostgreSQL container. Is Docker running?");

        let db_name = Self::generate_db_name();
        let db_url = format!("{}/{}", container.base_url(), db_name);

        // Create the database
        Postgres::create_database(&db_url)
            .await
            .expect("Failed to create test database");

        // Connect to the new database
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        Self { pool, db_name }
    }

    /// Generate a unique database name.
    fn generate_db_name() -> String {
        let db_number = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!(
            "portal_test_{}_{}_{}",
            std::process::id(),
            db_number,
            chrono::Utc::now().timestamp_millis()
        )
    }

    /// Get a reference to the connection pool.
    pub const fn pool(&self) -> &DbPool {
        &self.pool
    }

    /// Execute raw SQL for test setup.
    pub async fn execute(&self, sql: &str) -> Result<(), sqlx::Error> {
        self.pool.execute(sql).await?;
        Ok(())
    }

    /// Clean all data from the database (truncate all tables).
    ///
    /// Useful for resetting state between tests without recreating the database.
    pub async fn clean(&self) -> Result<(), sqlx::Error> {
        self.pool
            .execute(
                r"
                DO $$ DECLARE
                    r RECORD;
                BEGIN
                    FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
                        EXECUTE 'TRUNCATE TABLE ' || quote_ident(r.tablename) || ' CASCADE';
                    END LOOP;
                END $$;
                ",
            )
            .await?;
        Ok(())
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let pool = self.pool.clone();

        // Spawn a thread for async cleanup (can't await in Drop)
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for cleanup");

            rt.block_on(async {
                // Close the pool connections first
                pool.close().await;

                // Get the container to get the admin URL
                if let Ok(container) = get_or_init_container().await {
                    let admin_url = container.admin_url();

                    if let Ok(admin_pool) = PgPoolOptions::new()
                        .max_connections(1)
                        .connect(&admin_url)
                        .await
                    {
                        // Terminate any remaining connections
                        let _ = admin_pool
                            .execute(sqlx::query(&format!(
                                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
                            )))
                            .await;

                        // Drop the database
                        let _ = admin_pool
                            .execute(sqlx::query(&format!(
                                "DROP DATABASE IF EXISTS \"{db_name}\""
                            )))
                            .await;
                    }
                }
            });
        });
    }
}

/// Helper for running tests with a transaction that rolls back.
///
/// This is faster than creating a new database for each test,
/// but tests cannot see each other's changes.
pub struct TestTransaction {
    pool: DbPool,
}

impl TestTransaction {
    /// Create a new test transaction context.
    pub async fn new(pool: &DbPool) -> Self {
        // Start a transaction
        pool.execute("BEGIN")
            .await
            .expect("Failed to start transaction");
        Self { pool: pool.clone() }
    }

    /// Get the pool (which is in a transaction).
    pub const fn pool(&self) -> &DbPool {
        &self.pool
    }
}

impl Drop for TestTransaction {
    fn drop(&mut self) {
        // Rollback the transaction
        let pool = self.pool.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime for rollback");

            rt.block_on(async {
                let _ = pool.execute("ROLLBACK").await;
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_creation() {
        let db = TestDb::new().await;

        // Verify we can query
        let result: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_migrations_applied() {
        let db = TestDb::new().await;

        // Verify users table exists (from migrations)
        let result = sqlx::query("SELECT 1 FROM users LIMIT 0")
            .execute(db.pool())
            .await;
        assert!(result.is_ok(), "users table should exist after migrations");
    }

    #[tokio::test]
    async fn test_db_isolation() {
        let db1 = TestDb::new().await;
        let db2 = TestDb::new().await;

        // Get initial count in db2 (includes seeded users from migrations)
        let initial_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(db2.pool())
            .await
            .unwrap();

        // Insert into db1
        db1.execute("INSERT INTO users (username, email) VALUES ('isolation_test', 'isolation@example.com')")
            .await
            .unwrap();

        // db2 should not see db1's new data (count should be same as before)
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(db2.pool())
            .await
            .unwrap();
        assert_eq!(count.0, initial_count.0, "databases should be isolated");

        // Verify db1 has the new user (initial + 1)
        let db1_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(db1.pool())
            .await
            .unwrap();
        assert_eq!(db1_count.0, initial_count.0 + 1, "db1 should have the new user");
    }
}
