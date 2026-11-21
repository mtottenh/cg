//! Validation of portal-managed MatchZy overrides.
//!
//! The portal stores a per-server map of MatchZy settings (`cvar → value`)
//! and pushes it to the server agent with `set_matchzy_overrides`; the
//! host's update helper writes it into a managed block of MatchZy's
//! `config.cfg`. The rules here are EXACTLY the agent's
//! (`validate_overrides` in portal-server-agent, README "Protocol"), which
//! the helper applies once more to the file. Keep the three in step: a set
//! the portal accepts and the agent refuses can never be applied.

use serde_json::{Map, Value};
use std::fmt;

/// At most this many overrides per server.
pub const MAX_OVERRIDES: usize = 64;
/// A cvar name is at most this many bytes.
pub const MAX_OVERRIDE_NAME_BYTES: usize = 128;
/// A value, rendered as text, is at most this many bytes.
pub const MAX_OVERRIDE_VALUE_BYTES: usize = 256;

/// Why an override set was refused. All-or-nothing: one bad entry rejects
/// the whole set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// More than [`MAX_OVERRIDES`] entries.
    TooMany(usize),
    /// The name is not `^matchzy_[a-z0-9_]+$` within the length limit.
    /// Carries a bounded, printable rendering of the name.
    BadName(String),
    /// The cvar is portal-owned (demo upload, webhooks, match loading).
    PortalOwned(String),
    /// The value is not a string, number or boolean.
    BadValueType(String),
    /// The rendered value is longer than [`MAX_OVERRIDE_VALUE_BYTES`].
    ValueTooLong(String),
    /// The rendered value is not printable ASCII, or contains `"`, `;`, `\`.
    BadValueChars(String),
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany(n) => write!(f, "too many overrides ({n} > {MAX_OVERRIDES})"),
            Self::BadName(name) => write!(
                f,
                "override {name:?} refused: name must match ^matchzy_[a-z0-9_]+$ \
                 (at most {MAX_OVERRIDE_NAME_BYTES} characters)"
            ),
            Self::PortalOwned(name) => {
                write!(f, "override {name:?} refused: cvar is portal-owned")
            }
            Self::BadValueType(name) => write!(
                f,
                "override {name}: value must be a string, number or boolean"
            ),
            Self::ValueTooLong(name) => write!(
                f,
                "override {name}: value longer than {MAX_OVERRIDE_VALUE_BYTES} characters"
            ),
            Self::BadValueChars(name) => write!(
                f,
                "override {name}: value must be printable ASCII without quotes, \
                 semicolons or backslashes"
            ),
        }
    }
}

impl std::error::Error for OverrideError {}

/// Names are admin-typed and echoed into error messages: bound them.
fn shown(name: &str) -> String {
    name.chars()
        .take(64)
        .filter(char::is_ascii_graphic)
        .collect()
}

fn check_name(name: &str) -> Result<(), OverrideError> {
    let Some(rest) = name.strip_prefix("matchzy_").filter(|r| !r.is_empty()) else {
        return Err(OverrideError::BadName(shown(name)));
    };
    if name.len() > MAX_OVERRIDE_NAME_BYTES
        || !rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(OverrideError::BadName(shown(name)));
    }
    // The portal owns these: demo upload and webhooks carry its tokens, the
    // load commands are the match lifecycle.
    if name.starts_with("matchzy_demo_upload_")
        || name.starts_with("matchzy_remote_")
        || name.starts_with("matchzy_loadmatch")
        || name == "matchzy_loadbackup_url"
    {
        return Err(OverrideError::PortalOwned(shown(name)));
    }
    Ok(())
}

/// The text the agent writes for a value, or why it is refused.
fn render_value(name: &str, value: &Value) -> Result<String, OverrideError> {
    let text = match value {
        Value::String(s) => s.trim_matches(' ').to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return Err(OverrideError::BadValueType(name.to_string())),
    };
    if text.len() > MAX_OVERRIDE_VALUE_BYTES {
        return Err(OverrideError::ValueTooLong(name.to_string()));
    }
    if !text
        .bytes()
        .all(|b| (0x20..=0x7e).contains(&b) && !matches!(b, b'"' | b';' | b'\\'))
    {
        return Err(OverrideError::BadValueChars(name.to_string()));
    }
    Ok(text)
}

/// Validate an override set and return it normalised: the same entries,
/// strings with their leading and trailing spaces dropped (as the agent
/// drops them), numbers and booleans untouched. An empty map is valid and
/// clears the overrides.
///
/// # Errors
///
/// The first rule an entry breaks, in name order.
pub fn validate_matchzy_overrides(
    overrides: &Map<String, Value>,
) -> Result<Map<String, Value>, OverrideError> {
    if overrides.len() > MAX_OVERRIDES {
        return Err(OverrideError::TooMany(overrides.len()));
    }
    let mut out = Map::new();
    for (name, value) in overrides {
        check_name(name)?;
        let text = render_value(name, value)?;
        let stored = match value {
            Value::String(_) => Value::String(text),
            other => other.clone(),
        };
        out.insert(name.clone(), stored);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(v: Value) -> Result<Map<String, Value>, OverrideError> {
        validate_matchzy_overrides(v.as_object().expect("object"))
    }

    #[test]
    fn accepts_plain_matchzy_cvars_and_normalises_strings() {
        let out = check(json!({
            "matchzy_kick_when_no_match_loaded": false,
            "matchzy_max_tech_pauses": 2,
            "matchzy_tech_pause_time": 1.5,
            "matchzy_chat_prefix": "  [10 Mans]  ",
            "matchzy_empty": "",
        }))
        .unwrap();
        assert_eq!(out["matchzy_kick_when_no_match_loaded"], json!(false));
        assert_eq!(out["matchzy_max_tech_pauses"], json!(2));
        assert_eq!(out["matchzy_tech_pause_time"], json!(1.5));
        assert_eq!(out["matchzy_chat_prefix"], json!("[10 Mans]"));
        assert_eq!(out["matchzy_empty"], json!(""));
    }

    #[test]
    fn empty_map_is_valid() {
        assert!(check(json!({})).unwrap().is_empty());
    }

    #[test]
    fn refuses_portal_owned_cvars() {
        for name in [
            "matchzy_demo_upload_url",
            "matchzy_demo_upload_header_value",
            "matchzy_remote_log_url",
            "matchzy_remote_backup_header_key",
            "matchzy_loadmatch",
            "matchzy_loadmatch_url",
            "matchzy_loadbackup_url",
        ] {
            let err = check(json!({ name: "x" })).unwrap_err();
            assert!(
                matches!(err, OverrideError::PortalOwned(_)),
                "{name}: {err}"
            );
        }
        // Near misses are ordinary cvars.
        assert!(check(json!({ "matchzy_loadbackup": "x" })).is_ok());
        assert!(check(json!({ "matchzy_demo_path": "demos" })).is_ok());
        assert!(check(json!({ "matchzy_demo_upload": "x" })).is_ok());
    }

    #[test]
    fn refuses_names_outside_the_pattern() {
        let long = format!("matchzy_{}", "a".repeat(MAX_OVERRIDE_NAME_BYTES));
        for name in [
            "",
            "matchzy_",
            "sv_cheats",
            "MATCHZY_foo",
            "matchzy_Foo",
            "matchzy_foo bar",
            "matchzy_foo;quit",
            "matchzy_foo\n",
            "matchzy_f\u{00f6}o",
            " matchzy_foo",
            long.as_str(),
        ] {
            let err = check(json!({ name: "x" })).unwrap_err();
            assert!(matches!(err, OverrideError::BadName(_)), "{name:?}: {err}");
        }
        // Exactly at the limit passes.
        let at_limit = format!("matchzy_{}", "a".repeat(MAX_OVERRIDE_NAME_BYTES - 8));
        assert_eq!(at_limit.len(), MAX_OVERRIDE_NAME_BYTES);
        assert!(check(json!({ at_limit: "x" })).is_ok());
    }

    #[test]
    fn refuses_values_that_are_not_scalars() {
        for value in [json!(null), json!([1]), json!({"a": 1})] {
            let err = check(json!({ "matchzy_x": value })).unwrap_err();
            assert!(matches!(err, OverrideError::BadValueType(_)), "{err}");
        }
    }

    #[test]
    fn refuses_anything_but_printable_ascii_in_values() {
        for value in [
            "a\"b",
            "a;b",
            "a\\b",
            "a\nb",
            "a\tb",
            "a\rb",
            "a\u{0}b",
            "a\u{7f}b",
            "caf\u{e9}",
            "a\u{2028}b",
            "a\u{85}b",
            "a\u{a0}b",
        ] {
            let err = check(json!({ "matchzy_x": value })).unwrap_err();
            assert!(
                matches!(err, OverrideError::BadValueChars(_)),
                "{value:?}: {err}"
            );
        }
        // Inner spaces and the rest of printable ASCII are fine.
        assert!(check(json!({ "matchzy_x": "a b ~!@#$%^&*()'<>{}[]|/?.," })).is_ok());
    }

    #[test]
    fn values_are_bounded_after_trimming() {
        let ok = "a".repeat(MAX_OVERRIDE_VALUE_BYTES);
        assert!(check(json!({ "matchzy_x": ok })).is_ok());
        // Padding does not count: the agent trims before measuring.
        let padded = format!("  {}  ", "a".repeat(MAX_OVERRIDE_VALUE_BYTES));
        assert!(check(json!({ "matchzy_x": padded })).is_ok());
        let long = "a".repeat(MAX_OVERRIDE_VALUE_BYTES + 1);
        let err = check(json!({ "matchzy_x": long })).unwrap_err();
        assert!(matches!(err, OverrideError::ValueTooLong(_)));
    }

    #[test]
    fn the_set_is_bounded_and_all_or_nothing() {
        let mut map = Map::new();
        for i in 0..MAX_OVERRIDES {
            map.insert(format!("matchzy_cvar_{i}"), json!(i));
        }
        assert_eq!(
            validate_matchzy_overrides(&map).unwrap().len(),
            MAX_OVERRIDES
        );
        map.insert("matchzy_one_more".into(), json!(1));
        assert_eq!(
            validate_matchzy_overrides(&map).unwrap_err(),
            OverrideError::TooMany(MAX_OVERRIDES + 1)
        );

        // One bad entry rejects the lot.
        let err = check(json!({ "matchzy_good": 1, "matchzy_remote_log_url": "x" })).unwrap_err();
        assert!(matches!(err, OverrideError::PortalOwned(_)));
    }

    #[test]
    fn error_messages_bound_and_clean_the_name() {
        let name = format!("bad\n{}", "x".repeat(500));
        let msg = check(json!({ name: "x" })).unwrap_err().to_string();
        assert!(msg.len() < 250, "{}", msg.len());
        assert!(!msg.contains('\n'));
    }
}
