//! Authentication utilities.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use portal_core::DomainError;

/// Hash a password using Argon2id.
///
/// Returns the hashed password as a PHC string that can be stored in the database.
pub fn hash_password(password: &str) -> Result<String, DomainError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| DomainError::Internal(format!("Failed to hash password: {e}")))
}

/// Verify a password against a stored hash.
///
/// Returns `true` if the password matches the hash, `false` otherwise.
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, DomainError> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| DomainError::Internal(format!("Invalid password hash format: {e}")))?;

    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "secure_password_123!";
        let hash = hash_password(password).unwrap();

        // Hash should be non-empty
        assert!(!hash.is_empty());

        // Should verify correctly
        assert!(verify_password(password, &hash).unwrap());

        // Wrong password should not verify
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_passwords_different_hashes() {
        let hash1 = hash_password("password1").unwrap();
        let hash2 = hash_password("password1").unwrap();

        // Same password should produce different hashes (due to salt)
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(verify_password("password1", &hash1).unwrap());
        assert!(verify_password("password1", &hash2).unwrap());
    }
}
