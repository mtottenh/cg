//! Match-setup helpers: veto → MatchZy `map_sides` derivation and
//! reservation secrets. Design: docs/matchzy-integration.md §6.3.

use portal_core::ids::TournamentRegistrationId;
use rand::Rng;

use crate::entities::VetoAction;

/// Derive the full-length MatchZy `map_sides` array from completed veto
/// actions.
///
/// For each picked map (in `selected_maps` play order) the side chosen via
/// the veto's side-selection step is translated relative to which
/// registration is MatchZy `team1` (always participant 1). A map with no
/// recorded side selection — deciders, or `SideSelectionMode::Knife` —
/// yields `"knife"`.
///
/// The array is always emitted full-length: MatchZy defaults missing
/// entries to `"knife"`, which would silently override portal-veto sides
/// (verified v0.8.15 behavior, §2.1).
#[must_use]
pub fn derive_map_sides(
    selected_maps: &[String],
    actions: &[VetoAction],
    team1_registration: TournamentRegistrationId,
) -> Vec<String> {
    selected_maps
        .iter()
        .map(|map| {
            let side_action = actions.iter().find(|a| {
                a.map_id == *map
                    && a.side_selection.is_some()
                    && a.side_selected_by_registration_id.is_some()
            });
            match side_action {
                Some(action) => {
                    let side = action.side_selection.as_deref().unwrap_or_default();
                    let selector_is_team1 =
                        action.side_selected_by_registration_id == Some(team1_registration);
                    match (selector_is_team1, side) {
                        (true, "ct") => "team1_ct".to_string(),
                        (true, "t") => "team1_t".to_string(),
                        (false, "ct") => "team2_ct".to_string(),
                        (false, "t") => "team2_t".to_string(),
                        // Unknown side string: fail safe to knife rather
                        // than guessing a side.
                        _ => "knife".to_string(),
                    }
                }
                None => "knife".to_string(),
            }
        })
        .collect()
}

/// Generate a player-typeable connect password (`sv_password`).
///
/// Unambiguous lowercase alphanumerics — players read this off a screen and
/// type it into a console.
#[must_use]
pub fn generate_connect_password() -> String {
    const CHARSET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    crate::util::random_code(CHARSET, 10)
}

/// Generate a bearer token for the config/event endpoints (`cgm_` prefix,
/// 32 random bytes hex — same at-rest hashing as enrollment tokens).
#[must_use]
pub fn generate_reservation_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!("cgm_{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use portal_core::ids::{VetoActionId, VetoSessionId};
    use portal_core::types::VetoActionType;

    fn action(
        map: &str,
        side: Option<&str>,
        selected_by: Option<TournamentRegistrationId>,
    ) -> VetoAction {
        VetoAction {
            id: VetoActionId::new(),
            session_id: VetoSessionId::new(),
            action_number: 1,
            action_type: VetoActionType::Pick,
            map_id: map.to_string(),
            performed_by_registration_id: None,
            performed_by_user_id: None,
            side_selection: side.map(String::from),
            side_selected_by_registration_id: selected_by,
            side_selected_at: side.map(|_| Utc::now()),
            was_auto_action: false,
            auto_action_reason: None,
            performed_at: Utc::now(),
        }
    }

    #[test]
    fn sides_translate_relative_to_team1() {
        let team1 = TournamentRegistrationId::new();
        let team2 = TournamentRegistrationId::new();
        let maps = vec![
            "de_mirage".to_string(),
            "de_nuke".to_string(),
            "de_ancient".to_string(),
        ];
        let actions = vec![
            // team1 picked mirage; team2 chose to start CT on it
            action("de_mirage", Some("ct"), Some(team2)),
            // team2 picked nuke; team1 chose to start T on it
            action("de_nuke", Some("t"), Some(team1)),
            // ancient is the decider — no side selection
            action("de_ancient", None, None),
        ];
        assert_eq!(
            derive_map_sides(&maps, &actions, team1),
            vec!["team2_ct", "team1_t", "knife"]
        );
    }

    #[test]
    fn array_is_always_full_length() {
        let team1 = TournamentRegistrationId::new();
        let maps = vec!["de_inferno".to_string(), "de_dust2".to_string()];
        // No actions recorded at all (knife-mode format)
        assert_eq!(derive_map_sides(&maps, &[], team1), vec!["knife", "knife"]);
    }

    #[test]
    fn unknown_side_string_fails_safe_to_knife() {
        let team1 = TournamentRegistrationId::new();
        let maps = vec!["de_train".to_string()];
        let actions = vec![action("de_train", Some("attackers"), Some(team1))];
        assert_eq!(derive_map_sides(&maps, &actions, team1), vec!["knife"]);
    }

    #[test]
    fn connect_password_is_typeable() {
        let pw = generate_connect_password();
        assert_eq!(pw.len(), 10);
        assert!(
            pw.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
        // Ambiguous glyphs excluded
        assert!(!pw.contains(['l', 'o', '0', '1', 'i']));
    }
}
