//! Refresh token generation and hashing utilities.

use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random refresh token (32 bytes → hex string).
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Hash a refresh token using SHA-256 for database storage.
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_refresh_token_length() {
        let token = generate_refresh_token();
        // 32 bytes = 64 hex characters
        assert_eq!(token.len(), 64);
    }

    #[test]
    fn test_generate_refresh_token_uniqueness() {
        let t1 = generate_refresh_token();
        let t2 = generate_refresh_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_hash_refresh_token_deterministic() {
        let token = "test-token";
        let h1 = hash_refresh_token(token);
        let h2 = hash_refresh_token(token);
        assert_eq!(h1, h2);
        // SHA-256 = 32 bytes = 64 hex chars
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_hash_refresh_token_different_inputs() {
        let h1 = hash_refresh_token("token-a");
        let h2 = hash_refresh_token("token-b");
        assert_ne!(h1, h2);
    }
}
