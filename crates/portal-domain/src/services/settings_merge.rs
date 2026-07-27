//! Settings JSONB update semantics.
//!
//! The repositories persist `settings` with `COALESCE($n, settings)` — a
//! whole-object replace. Every partial writer (a client sending only
//! `{eligibility: ...}`, an old frontend sending only
//! `{side_selection_mode: ...}`) therefore silently erased every other key.
//! Services route incoming settings through [`shallow_merge`] so a PATCH
//! means "change these keys", not "this is the whole object".

/// Merge `incoming` over `stored` at the top level.
///
/// - Keys present in `incoming` replace the stored value wholesale (no deep
///   merge — a nested object is one value).
/// - Keys absent from `incoming` are kept.
/// - Keys set to `null` in `incoming` are removed, so clients can clear a
///   key (e.g. `{"eligibility": null}` drops all entry requirements).
/// - If either side is not a JSON object, `incoming` wins.
#[must_use]
pub fn shallow_merge(stored: &serde_json::Value, incoming: serde_json::Value) -> serde_json::Value {
    match (stored.as_object(), incoming) {
        (Some(stored_obj), serde_json::Value::Object(incoming_obj)) => {
            let mut merged = stored_obj.clone();
            for (key, value) in incoming_obj {
                if value.is_null() {
                    merged.remove(&key);
                } else {
                    merged.insert(key, value);
                }
            }
            serde_json::Value::Object(merged)
        }
        (_, other) => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_preserves_foreign_keys() {
        let stored =
            json!({"side_selection_mode": "knife", "eligibility": {"min_rating_per_player": 1000}});
        let merged = shallow_merge(
            &stored,
            json!({"eligibility": {"min_rating_per_player": 2000}}),
        );
        assert_eq!(
            merged,
            json!({"side_selection_mode": "knife", "eligibility": {"min_rating_per_player": 2000}})
        );
    }

    #[test]
    fn null_removes_a_key() {
        let stored = json!({"a": 1, "eligibility": {"min_rating_per_player": 1000}});
        let merged = shallow_merge(&stored, json!({"eligibility": null}));
        assert_eq!(merged, json!({"a": 1}));
    }

    #[test]
    fn non_object_incoming_replaces() {
        let stored = json!({"a": 1});
        assert_eq!(shallow_merge(&stored, json!([1, 2])), json!([1, 2]));
    }

    #[test]
    fn non_object_stored_is_replaced() {
        assert_eq!(
            shallow_merge(&json!(null), json!({"a": 1})),
            json!({"a": 1})
        );
    }
}
