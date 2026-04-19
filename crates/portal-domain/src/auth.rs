//! Authentication utilities.
//!
//! Argon2id hashing is CPU-bound. Each call below dispatches the work to
//! `tokio::task::spawn_blocking` so the runtime's worker threads stay
//! available for I/O. Calling `Argon2::default().hash_password(...)` directly
//! from an `async fn` blocks a worker for tens to hundreds of milliseconds
//! and trivially starves the runtime under concurrent login load.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use portal_core::DomainError;
use std::sync::OnceLock;

/// Hash a password using Argon2id.
///
/// Returns the hashed password as a PHC string that can be stored in the
/// database. Runs on the blocking thread pool.
pub async fn hash_password(password: String) -> Result<String, DomainError> {
    tokio::task::spawn_blocking(move || hash_password_blocking(&password))
        .await
        .map_err(|e| DomainError::Internal(format!("hash join error: {e}")))?
}

/// Verify a password against a stored hash. Runs on the blocking thread pool.
///
/// Returns `true` if the password matches the hash, `false` otherwise.
pub async fn verify_password(password: String, password_hash: String) -> Result<bool, DomainError> {
    tokio::task::spawn_blocking(move || verify_password_blocking(&password, &password_hash))
        .await
        .map_err(|e| DomainError::Internal(format!("verify join error: {e}")))?
}

/// Spend roughly the same CPU time as [`verify_password`] without revealing
/// whether the user exists.
///
/// Call this in the user-not-found branch of authentication so an attacker
/// cannot enumerate accounts by measuring response latency. The plaintext
/// passed in is irrelevant — the *work* is what matters.
pub async fn verify_dummy_for_timing() -> Result<(), DomainError> {
    let hash = dummy_hash();
    tokio::task::spawn_blocking(move || {
        if let Ok(parsed) = PasswordHash::new(hash) {
            let _ = Argon2::default().verify_password(b"never-matches", &parsed);
        }
    })
    .await
    .map_err(|e| DomainError::Internal(format!("dummy verify join error: {e}")))
}

fn hash_password_blocking(password: &str) -> Result<String, DomainError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| DomainError::Internal(format!("Failed to hash password: {e}")))
}

fn verify_password_blocking(password: &str, password_hash: &str) -> Result<bool, DomainError> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| DomainError::Internal(format!("Invalid password hash format: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Pre-computed Argon2id hash used by [`verify_dummy_for_timing`].
///
/// Computed lazily on first use and cached for the process lifetime so we
/// don't pay the salt+hash cost on every dummy call.
fn dummy_hash() -> &'static str {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    DUMMY_HASH.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"timing-canary", &salt)
            .expect("dummy hash construction must succeed")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hash_and_verify_password() {
        let password = "secure_password_123!".to_string();
        let hash = hash_password(password.clone()).await.unwrap();

        assert!(!hash.is_empty());
        assert!(verify_password(password, hash.clone()).await.unwrap());
        assert!(!verify_password("wrong_password".into(), hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_different_passwords_different_hashes() {
        let hash1 = hash_password("password1".into()).await.unwrap();
        let hash2 = hash_password("password1".into()).await.unwrap();
        assert_ne!(hash1, hash2);
        assert!(verify_password("password1".into(), hash1).await.unwrap());
        assert!(verify_password("password1".into(), hash2).await.unwrap());
    }

    #[tokio::test]
    async fn test_dummy_verify_runs() {
        // Just exercise the path so a regression is caught.
        verify_dummy_for_timing().await.unwrap();
    }
}
