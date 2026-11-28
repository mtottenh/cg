//! Refresh token domain entity.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

/// Why a refresh token was revoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTokenRevokeReason {
    /// Exchanged for a new token by a refresh.
    Rotated,
    /// Signed out with this token.
    Logout,
    /// Revoked together with all the user's tokens (suspected theft, account
    /// no longer active, sign out everywhere).
    RevokeAll,
    /// Found expired and revoked for hygiene.
    Expired,
}

impl RefreshTokenRevokeReason {
    /// The value stored in `refresh_tokens.revoked_reason`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rotated => "rotated",
            Self::Logout => "logout",
            Self::RevokeAll => "revoke_all",
            Self::Expired => "expired",
        }
    }

    /// Parse a stored value; unknown values read as `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rotated" => Some(Self::Rotated),
            "logout" => Some(Self::Logout),
            "revoke_all" => Some(Self::RevokeAll),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

/// How long after a rotation the rotated token still reads as "another tab
/// got there first" rather than as a replay.
pub const ROTATION_GRACE: Duration = Duration::seconds(60);

/// A refresh token for silent access token renewal.
#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    /// Why it was revoked; `None` while active, and for rows revoked before
    /// the reason was recorded.
    pub revoked_reason: Option<RefreshTokenRevokeReason>,
}

impl RefreshToken {
    /// Check if this refresh token is currently valid (not expired and not revoked).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.revoked_at.is_none() && self.expires_at > Utc::now()
    }

    /// Check if the token has been revoked.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// Whether this token was rotated within [`ROTATION_GRACE`] of `now`.
    ///
    /// Two tabs sharing the session cookie can both refresh with it at once;
    /// the one that loses presents a token rotated a moment earlier. That is
    /// not theft, so it must not revoke the user's other sessions.
    #[must_use]
    pub fn rotated_within_grace(&self, now: DateTime<Utc>) -> bool {
        self.revoked_reason == Some(RefreshTokenRevokeReason::Rotated)
            && self
                .revoked_at
                .is_some_and(|at| now.signed_duration_since(at) <= ROTATION_GRACE)
    }

    /// Check if the token has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }
}
