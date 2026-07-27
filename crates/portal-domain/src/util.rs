//! Small shared utilities.

use rand::Rng;

/// Generate a random code of `len` characters drawn from `alphabet`.
///
/// The single generator behind PUG join codes and game-server connect
/// passwords — the alphabets differ (URL-friendly upper-case vs
/// console-typeable lower-case) but the mechanism shouldn't (review nit).
#[must_use]
pub fn random_code(alphabet: &[u8], len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
        .collect()
}
