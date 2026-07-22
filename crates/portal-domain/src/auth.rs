//! Authentication utilities.
//!
//! Argon2id hashing is CPU-bound. Each call below dispatches the work to
//! `tokio::task::spawn_blocking` so the runtime's worker threads stay
//! available for I/O. Calling `Argon2::default().hash_password(...)` directly
//! from an `async fn` blocks a worker for tens to hundreds of milliseconds
//! and trivially starves the runtime under concurrent login load.
//!
//! # Tuning
//!
//! The Argon2id parameters used for **new** hashes are read from the
//! environment once at first use:
//!
//! * `PORTAL_ARGON2_M_COST` — memory cost in KiB (default 19456, OWASP 2023)
//! * `PORTAL_ARGON2_T_COST` — iteration count (default 2)
//! * `PORTAL_ARGON2_P_COST` — parallelism (default 1)
//!
//! Changing these only affects *newly* issued hashes. Verifications use the
//! parameters encoded in each stored PHC string, so existing users keep
//! authenticating with whatever cost they were hashed at — that's the
//! standard Argon2 upgrade story. To re-hash at new parameters, trigger a
//! password reset or verify-then-rehash-on-login.

use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use portal_core::DomainError;
use std::sync::OnceLock;

/// OWASP Argon2id recommended minimums (2023).
const DEFAULT_M_COST: u32 = 19_456;
const DEFAULT_T_COST: u32 = 2;
const DEFAULT_P_COST: u32 = 1;

/// Configured Argon2id instance used for **hashing** (verify paths use the
/// parameters encoded in the stored hash, not this one). Computed once per
/// process from env vars.
fn hashing_argon2() -> &'static Argon2<'static> {
    static A2: OnceLock<Argon2<'static>> = OnceLock::new();
    A2.get_or_init(|| {
        let m = read_env_u32("PORTAL_ARGON2_M_COST", DEFAULT_M_COST);
        let t = read_env_u32("PORTAL_ARGON2_T_COST", DEFAULT_T_COST);
        let p = read_env_u32("PORTAL_ARGON2_P_COST", DEFAULT_P_COST);
        let params = Params::new(m, t, p, None).unwrap_or_else(|e| {
            tracing::warn!(
                error = %e, m, t, p,
                "invalid Argon2 params from env; falling back to OWASP defaults"
            );
            Params::new(DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_COST, None)
                .expect("OWASP default Argon2 params are valid")
        });
        tracing::info!(
            m_cost = params.m_cost(),
            t_cost = params.t_cost(),
            p_cost = params.p_cost(),
            "Argon2id parameters configured"
        );
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    })
}

fn read_env_u32(key: &str, default: u32) -> u32 {
    match std::env::var(key) {
        Ok(v) => v.parse::<u32>().unwrap_or_else(|_| {
            tracing::warn!(key, value = %v, default, "invalid u32 for {key}; using default");
            default
        }),
        Err(_) => default,
    }
}

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
    hashing_argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| DomainError::Internal(format!("Failed to hash password: {e}")))
}

fn verify_password_blocking(password: &str, password_hash: &str) -> Result<bool, DomainError> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| DomainError::Internal(format!("Invalid password hash format: {e}")))?;
    // Verify uses params encoded in `parsed`, not our configured instance —
    // Argon2::default() is fine here and keeps existing hashes verifiable
    // even after a parameter upgrade.
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
        // Build the dummy hash with the currently-configured Argon2 so the
        // verify-cost of a user-not-found path matches the verify-cost of a
        // real user whose stored hash is already at current params.
        hashing_argon2()
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
        assert!(
            !verify_password("wrong_password".into(), hash)
                .await
                .unwrap()
        );
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
