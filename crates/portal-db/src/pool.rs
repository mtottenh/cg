//! Database connection pool management.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

/// Type alias for the database pool.
pub type DbPool = PgPool;

/// Database pool configuration.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of idle connections to maintain.
    pub min_connections: u32,
    /// Maximum time to wait for a connection from the pool.
    pub acquire_timeout: Duration,
    /// Maximum idle time before a connection is closed.
    pub idle_timeout: Duration,
    /// Maximum lifetime of a connection.
    pub max_lifetime: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}

impl PoolConfig {
    /// Configuration suitable for testing.
    #[must_use]
    pub const fn test() -> Self {
        Self {
            max_connections: 5,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(60),
            max_lifetime: Duration::from_secs(300),
        }
    }

    /// Configuration for production workloads.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_connections: 20,
            min_connections: 5,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }
}

/// Create a database connection pool.
///
/// # Arguments
/// * `database_url` - `PostgreSQL` connection string
/// * `config` - Pool configuration options
///
/// # Errors
/// Returns an error if the connection cannot be established.
pub async fn create_pool(database_url: &str, config: PoolConfig) -> Result<DbPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(database_url)?;

    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .connect_with(options)
        .await
}

/// Create a pool from the `DATABASE_URL` environment variable.
///
/// # Errors
/// Returns an error if `DATABASE_URL` is not set or connection fails.
pub async fn create_pool_from_env() -> Result<DbPool, sqlx::Error> {
    let url = std::env::var("DATABASE_URL").map_err(|_| {
        sqlx::Error::Configuration("DATABASE_URL environment variable not set".into())
    })?;
    create_pool(&url, PoolConfig::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 1);
    }

    #[test]
    fn test_test_config() {
        let config = PoolConfig::test();
        assert!(config.max_connections < PoolConfig::default().max_connections);
    }
}
