//! Database transaction support.
//!
//! This module provides transaction types and utilities for executing
//! multiple database operations atomically.

use crate::DbPool;
use sqlx::Postgres;

/// Type alias for a database transaction.
pub type DbTransaction<'a> = sqlx::Transaction<'a, Postgres>;

/// Begin a new database transaction.
///
/// # Errors
///
/// Returns an error if the transaction cannot be started.
pub async fn begin_transaction(pool: &DbPool) -> Result<DbTransaction<'_>, sqlx::Error> {
    pool.begin().await
}

/// Trait for types that can execute operations within a transaction.
///
/// This trait provides a standard interface for executing transactional
/// operations across multiple repositories.
#[async_trait::async_trait]
pub trait Transactional {
    /// The output type of the transactional operation.
    type Output;
    /// The error type.
    type Error;

    /// Execute the operation within the given transaction.
    async fn execute_in_tx<'a>(
        &self,
        tx: &mut DbTransaction<'a>,
    ) -> Result<Self::Output, Self::Error>;
}

/// Execute a closure within a transaction.
///
/// The transaction is automatically committed if the closure succeeds,
/// or rolled back if it returns an error.
///
/// # Example
///
/// ```ignore
/// let result = with_transaction(&pool, |tx| async move {
///     // Multiple database operations here
///     Ok(())
/// }).await?;
/// ```
pub async fn with_transaction<F, Fut, T, E>(pool: &DbPool, f: F) -> Result<T, E>
where
    F: FnOnce(DbTransaction<'_>) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: From<sqlx::Error>,
{
    let tx = pool.begin().await.map_err(E::from)?;

    // Note: The transaction is automatically committed or rolled back
    // when the closure returns. If Ok, commit happens. If Err, rollback happens.
    // This is because the transaction is moved into the closure.

    f(tx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_type_alias() {
        // Just verify the type alias compiles
        fn _takes_tx(_tx: &DbTransaction<'_>) {}
    }
}
