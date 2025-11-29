//! Reusable validation rules and value objects.
//!
//! This module provides validated types that ensure business rules
//! are enforced at construction time.

use crate::errors::{FieldError, ValidationError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated team name.
///
/// Team names must be:
/// - Between 3 and 64 characters
/// - Not contain only whitespace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeamName(String);

impl TeamName {
    /// Minimum length for team names.
    pub const MIN_LENGTH: usize = 3;
    /// Maximum length for team names.
    pub const MAX_LENGTH: usize = 64;

    /// Create a new validated team name.
    ///
    /// # Errors
    /// Returns `ValidationError` if the name is invalid.
    pub fn new(name: &str) -> Result<Self, ValidationError> {
        let trimmed = name.trim();

        if trimmed.len() < Self::MIN_LENGTH || trimmed.len() > Self::MAX_LENGTH {
            return Err(ValidationError::field(FieldError::length(
                "name",
                Self::MIN_LENGTH,
                Self::MAX_LENGTH,
            )));
        }

        Ok(Self(trimmed.to_string()))
    }

    /// Get the inner string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for TeamName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for TeamName {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<TeamName> for String {
    fn from(name: TeamName) -> String {
        name.0
    }
}

/// A validated team tag (short identifier).
///
/// Team tags must be:
/// - Between 2 and 5 characters
/// - Alphanumeric only (letters and numbers)
/// - Stored in uppercase
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeamTag(String);

impl TeamTag {
    /// Minimum length for team tags.
    pub const MIN_LENGTH: usize = 2;
    /// Maximum length for team tags.
    pub const MAX_LENGTH: usize = 5;

    /// Create a new validated team tag.
    ///
    /// # Errors
    /// Returns `ValidationError` if the tag is invalid.
    pub fn new(tag: &str) -> Result<Self, ValidationError> {
        let trimmed = tag.trim();

        if trimmed.len() < Self::MIN_LENGTH || trimmed.len() > Self::MAX_LENGTH {
            return Err(ValidationError::field(FieldError::length(
                "tag",
                Self::MIN_LENGTH,
                Self::MAX_LENGTH,
            )));
        }

        if !trimmed.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ValidationError::field(FieldError::format(
                "tag",
                "alphanumeric characters only",
            )));
        }

        Ok(Self(trimmed.to_uppercase()))
    }

    /// Get the inner string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for TeamTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for TeamTag {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<TeamTag> for String {
    fn from(tag: TeamTag) -> String {
        tag.0
    }
}

/// A validated display name for players.
///
/// Display names must be:
/// - Between 3 and 32 characters
/// - Not contain only whitespace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DisplayName(String);

impl DisplayName {
    /// Minimum length for display names.
    pub const MIN_LENGTH: usize = 3;
    /// Maximum length for display names.
    pub const MAX_LENGTH: usize = 32;

    /// Create a new validated display name.
    ///
    /// # Errors
    /// Returns `ValidationError` if the name is invalid.
    pub fn new(name: &str) -> Result<Self, ValidationError> {
        let trimmed = name.trim();

        if trimmed.len() < Self::MIN_LENGTH || trimmed.len() > Self::MAX_LENGTH {
            return Err(ValidationError::field(FieldError::length(
                "display_name",
                Self::MIN_LENGTH,
                Self::MAX_LENGTH,
            )));
        }

        Ok(Self(trimmed.to_string()))
    }

    /// Get the inner string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for DisplayName {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<DisplayName> for String {
    fn from(name: DisplayName) -> String {
        name.0
    }
}

/// A validated email address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Email(String);

impl Email {
    /// Maximum length for email addresses.
    pub const MAX_LENGTH: usize = 255;

    /// Create a new validated email address.
    ///
    /// # Errors
    /// Returns `ValidationError` if the email is invalid.
    pub fn new(email: &str) -> Result<Self, ValidationError> {
        let trimmed = email.trim().to_lowercase();

        if trimmed.len() > Self::MAX_LENGTH {
            return Err(ValidationError::field(FieldError::new(
                "email",
                format!("email must be at most {} characters", Self::MAX_LENGTH),
                "length",
            )));
        }

        // Basic email validation (contains @ with non-empty local part, and at least one . after @)
        if let Some(at_pos) = trimmed.find('@') {
            let local = &trimmed[..at_pos];
            let domain = &trimmed[at_pos + 1..];
            if !local.is_empty()
                && !domain.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
            {
                return Ok(Self(trimmed));
            }
        }

        Err(ValidationError::field(FieldError::format(
            "email",
            "a valid email address",
        )))
    }

    /// Get the inner string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Get the domain part of the email.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split('@').nth(1).unwrap_or("")
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for Email {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Email> for String {
    fn from(email: Email) -> String {
        email.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_name_valid() {
        assert!(TeamName::new("Cloud9").is_ok());
        assert!(TeamName::new("  Team Liquid  ").is_ok()); // Trims whitespace
        assert!(TeamName::new("ABC").is_ok()); // Minimum length
    }

    #[test]
    fn test_team_name_invalid() {
        assert!(TeamName::new("AB").is_err()); // Too short
        assert!(TeamName::new("   ").is_err()); // Only whitespace
        assert!(TeamName::new(&"x".repeat(65)).is_err()); // Too long
    }

    #[test]
    fn test_team_tag_valid() {
        assert!(TeamTag::new("C9").is_ok());
        assert_eq!(TeamTag::new("c9").unwrap().as_str(), "C9"); // Uppercased
        assert!(TeamTag::new("NAVI").is_ok());
    }

    #[test]
    fn test_team_tag_invalid() {
        assert!(TeamTag::new("A").is_err()); // Too short
        assert!(TeamTag::new("TOOLONG").is_err()); // Too long
        assert!(TeamTag::new("C9!").is_err()); // Non-alphanumeric
    }

    #[test]
    fn test_display_name_valid() {
        assert!(DisplayName::new("Player1").is_ok());
        assert!(DisplayName::new("  Shroud  ").is_ok());
    }

    #[test]
    fn test_email_valid() {
        assert!(Email::new("user@example.com").is_ok());
        assert!(Email::new("USER@EXAMPLE.COM").unwrap().as_str() == "user@example.com"); // Lowercased
        assert!(Email::new("user.name+tag@sub.example.com").is_ok());
    }

    #[test]
    fn test_email_invalid() {
        assert!(Email::new("notanemail").is_err());
        assert!(Email::new("user@").is_err());
        assert!(Email::new("@example.com").is_err());
        assert!(Email::new("user@example").is_err()); // No dot in domain
    }
}
