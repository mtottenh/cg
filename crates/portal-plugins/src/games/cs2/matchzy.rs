//! MatchZy match-config generation.
//!
//! Builds the JSON that `matchzy_loadmatch_url` fetches, per the verified
//! v0.8.15 contract (docs/matchzy-integration.md §2.1, §6.3):
//!
//! - `maplist` has exactly `num_maps` entries → MatchZy force-skips its
//!   in-server veto.
//! - `map_sides` is always full-length; missing entries would silently
//!   re-enable knife rounds.
//! - `cvars` carries `sv_password` (MatchZy never touches it), the
//!   per-reservation event-webhook URL + auth header, and
//!   `matchzy_kick_when_no_match_loaded` so managed servers stay
//!   match-only. Cvars are restored by MatchZy at series end.
//! - Demo-upload cvars are deliberately NOT set here: a series-end cvar
//!   restore can race the final map's upload (§6.3); those live in the
//!   server's permanent config.

use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A maplist entry resolved against the game's map catalog.
#[derive(Debug, Clone)]
pub struct MatchzyMapRef {
    /// Portal map id (the veto/pool identifier).
    pub portal_id: String,
    /// Engine-level map name when it differs from the portal id.
    pub engine_name: Option<String>,
    /// Steam Workshop item id (decimal digits) for workshop-hosted maps.
    pub workshop_id: Option<String>,
}

/// Extract the numeric workshop item id from a catalog `external_id` —
/// accepts bare digits or a steamcommunity `filedetails/?id=…` URL.
#[must_use]
pub fn workshop_numeric_id(external_id: &str) -> Option<String> {
    let trimmed = external_id.trim();
    if !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let (_, after) = trimmed.split_once("id=")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

/// Resolve maplist entries to the tokens MatchZy understands.
///
/// Verified v0.8.15 `ChangeMap` contract: a token that parses as an
/// integer is loaded via `host_workshop_map <id>` (on-demand Steam CDN
/// download); anything else goes through `changelevel <name>`, which
/// SILENTLY does nothing unless the map is valid on the server. So the
/// token is the workshop id when present, else the engine-level name —
/// and anything unexpressible is an error here rather than a match that
/// hangs on the wrong map.
pub fn matchzy_map_tokens(maps: &[MatchzyMapRef]) -> Result<Vec<String>, String> {
    maps.iter()
        .map(|m| {
            if let Some(ws) = &m.workshop_id {
                if ws.is_empty() || !ws.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(format!(
                        "map \"{}\" has a non-numeric workshop id: {ws:?}",
                        m.portal_id
                    ));
                }
                return Ok(ws.clone());
            }
            let name = m.engine_name.as_deref().unwrap_or(&m.portal_id);
            let name_ok = !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if !name_ok {
                return Err(format!(
                    "map \"{}\" has an invalid engine name: {name:?}",
                    m.portal_id
                ));
            }
            Ok(name.to_string())
        })
        .collect()
}

/// A maplist token MatchZy will treat as a workshop item id.
fn is_workshop_token(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit())
}

/// One team's identity and roster for the config.
#[derive(Debug, Clone)]
pub struct MatchzyTeam {
    /// Echoed back in webhook events (we use the registration UUID).
    pub id: String,
    pub name: String,
    /// `(steamid64, display name)` — every listed player may connect;
    /// everyone else is kicked by MatchZy.
    pub players: Vec<(String, String)>,
}

/// Inputs for building a MatchZy match config.
#[derive(Debug, Clone)]
pub struct MatchzyConfigInput {
    /// The integer `matchid` (reservation `matchzy_id`).
    pub matchzy_id: i64,
    /// Picked maps in play order as MatchZy tokens (see
    /// [`matchzy_map_tokens`]: workshop item id, or engine-level name) —
    /// length defines `num_maps`.
    pub maplist: Vec<String>,
    /// Full-length side assignments (`team1_ct` / `team2_ct` / … / `knife`).
    pub map_sides: Vec<String>,
    pub team1: MatchzyTeam,
    pub team2: MatchzyTeam,
    pub players_per_team: u32,
    /// Ready threshold per team (defaults to `players_per_team`).
    pub min_players_to_ready: u32,
    /// Server hostname shown in the browser (MatchZy format vars allowed).
    pub hostname: String,
    /// `sv_password` for this reservation.
    pub connect_password: String,
    /// GOTV password, when GOTV is enabled.
    pub gotv_password: Option<String>,
    /// Absolute URL of the portal's event-webhook endpoint.
    pub event_url: String,
    /// Absolute URL of the portal's round-backup endpoint (uploads happen
    /// per round DURING the series, so per-match cvars are race-free here).
    pub backup_url: String,
    /// Raw bearer token for the event webhook (single MatchZy header pair).
    pub event_token: String,
    /// Extra per-tournament cvar overrides (applied last).
    pub extra_cvars: BTreeMap<String, String>,
}

/// Build the MatchZy match-config JSON.
///
/// # Panics
/// Never — invariants (equal-length `maplist`/`map_sides`, non-empty teams)
/// are the caller's contract, validated by [`validate_input`].
#[must_use]
pub fn build_matchzy_config(input: &MatchzyConfigInput) -> Value {
    let players = |team: &MatchzyTeam| -> Value {
        Value::Object(
            team.players
                .iter()
                .map(|(steam64, name)| (steam64.clone(), Value::String(name.clone())))
                .collect(),
        )
    };

    let mut cvars = BTreeMap::new();
    cvars.insert("hostname".to_string(), input.hostname.clone());
    cvars.insert("sv_password".to_string(), input.connect_password.clone());
    if let Some(gotv) = &input.gotv_password {
        // §6.3: GOTV on, delayed ≥105s (ghosting mitigation, M6). The
        // delay may be raised (never lowered) via tournament overrides.
        cvars.insert("tv_enable".to_string(), "1".to_string());
        cvars.insert("tv_delay".to_string(), "105".to_string());
        cvars.insert("tv_password".to_string(), gotv.clone());
    }
    cvars.insert(
        "matchzy_remote_log_url".to_string(),
        input.event_url.clone(),
    );
    cvars.insert(
        "matchzy_remote_log_header_key".to_string(),
        "Authorization".to_string(),
    );
    cvars.insert(
        "matchzy_remote_log_header_value".to_string(),
        format!("Bearer {}", input.event_token),
    );
    cvars.insert(
        "matchzy_remote_backup_url".to_string(),
        input.backup_url.clone(),
    );
    cvars.insert(
        "matchzy_remote_backup_header_key".to_string(),
        "Authorization".to_string(),
    );
    cvars.insert(
        "matchzy_remote_backup_header_value".to_string(),
        format!("Bearer {}", input.event_token),
    );
    cvars.insert(
        "matchzy_kick_when_no_match_loaded".to_string(),
        "true".to_string(),
    );
    // Workshop maps download from the Steam CDN at changelevel time; widen
    // the between-maps window so multi-hundred-MB maps arrive before the
    // next map is expected live (Get5 applies the same +20s buffer).
    if input.maplist.iter().any(|t| is_workshop_token(t)) {
        cvars.insert("mp_match_restart_delay".to_string(), "45".to_string());
    }
    // Tournament overrides win over the defaults above — except the
    // security-relevant keys, which are portal-owned.
    for (key, value) in &input.extra_cvars {
        if is_portal_owned_cvar(key) {
            continue;
        }
        // tv_delay may only be raised, never lowered (ghosting, M6).
        if key == "tv_delay" && value.parse::<u32>().is_ok_and(|v| v < 105) {
            continue;
        }
        cvars.insert(key.clone(), value.clone());
    }

    json!({
        "matchid": input.matchzy_id,
        "num_maps": input.maplist.len(),
        "maplist": input.maplist,
        "map_sides": input.map_sides,
        "skip_veto": true,
        "clinch_series": true,
        "players_per_team": input.players_per_team,
        "min_players_to_ready": input.min_players_to_ready,
        "team1": {
            "id": input.team1.id,
            "name": input.team1.name,
            "players": players(&input.team1),
        },
        "team2": {
            "id": input.team2.id,
            "name": input.team2.name,
            "players": players(&input.team2),
        },
        "cvars": cvars,
    })
}

/// Cvars the portal owns: security/webhook/credential surface a tournament
/// override must never touch (M10 — deny by prefix, not by enumerating
/// literals that drift as keys are added).
fn is_portal_owned_cvar(key: &str) -> bool {
    key == "sv_password"
        || key == "rcon_password"
        || key == "tv_password"
        || key.starts_with("matchzy_remote_")
        || key.starts_with("matchzy_demo_upload_")
}

/// Validate builder invariants, returning a user-actionable error message.
pub fn validate_input(input: &MatchzyConfigInput) -> Result<(), String> {
    if input.maplist.is_empty() {
        return Err("no maps selected — veto result is empty".to_string());
    }
    if input.map_sides.len() != input.maplist.len() {
        return Err(format!(
            "map_sides length {} does not match maplist length {}",
            input.map_sides.len(),
            input.maplist.len()
        ));
    }
    for team in [&input.team1, &input.team2] {
        if team.players.is_empty() {
            return Err(format!(
                "team \"{}\" has no players with Steam IDs",
                team.name
            ));
        }
        for (steam64, name) in &team.players {
            if steam64.parse::<u64>().is_err() {
                return Err(format!(
                    "player \"{name}\" on \"{}\" has an invalid SteamID64: {steam64}",
                    team.name
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> MatchzyConfigInput {
        MatchzyConfigInput {
            matchzy_id: 42,
            maplist: vec!["de_mirage".into(), "de_nuke".into(), "de_ancient".into()],
            map_sides: vec!["team2_ct".into(), "team1_ct".into(), "knife".into()],
            team1: MatchzyTeam {
                id: "reg-1".into(),
                name: "Alpha".into(),
                players: vec![("76561198000000001".into(), "a1".into())],
            },
            team2: MatchzyTeam {
                id: "reg-2".into(),
                name: "Bravo".into(),
                players: vec![("76561198000000002".into(), "b1".into())],
            },
            players_per_team: 5,
            min_players_to_ready: 5,
            hostname: "Portal | {TEAM1} vs {TEAM2}".into(),
            connect_password: "pw123".into(),
            gotv_password: Some("gotv".into()),
            event_url: "https://portal.test/v1/gameserver/events".into(),
            backup_url: "https://portal.test/v1/gameserver/backups".into(),
            event_token: "cgm_secret".into(),
            extra_cvars: BTreeMap::new(),
        }
    }

    #[test]
    fn config_matches_the_verified_matchzy_contract() {
        let config = build_matchzy_config(&input());
        assert_eq!(config["matchid"], 42);
        assert_eq!(config["num_maps"], 3);
        assert_eq!(config["skip_veto"], true);
        assert_eq!(config["clinch_series"], true);
        assert_eq!(config["maplist"].as_array().unwrap().len(), 3);
        assert_eq!(config["map_sides"].as_array().unwrap().len(), 3);
        assert_eq!(config["team1"]["players"]["76561198000000001"], "a1");
        let cvars = &config["cvars"];
        assert_eq!(cvars["sv_password"], "pw123");
        assert_eq!(cvars["tv_password"], "gotv");
        assert_eq!(cvars["matchzy_remote_log_header_key"], "Authorization");
        assert_eq!(
            cvars["matchzy_remote_log_header_value"],
            "Bearer cgm_secret"
        );
        assert_eq!(cvars["matchzy_kick_when_no_match_loaded"], "true");
        // Demo upload cvars deliberately absent (§6.3 race)
        assert!(cvars.get("matchzy_demo_upload_url").is_none());
    }

    #[test]
    fn tournament_cvars_cannot_override_security_keys() {
        let mut i = input();
        i.extra_cvars
            .insert("sv_password".into(), "attacker".into());
        i.extra_cvars
            .insert("matchzy_remote_backup_url".into(), "https://evil".into());
        i.extra_cvars
            .insert("matchzy_demo_upload_url".into(), "https://evil".into());
        i.extra_cvars.insert("tv_delay".into(), "0".into());
        i.extra_cvars.insert("mp_freezetime".into(), "10".into());
        let config = build_matchzy_config(&i);
        assert_eq!(config["cvars"]["sv_password"], "pw123");
        assert_ne!(config["cvars"]["matchzy_remote_backup_url"], "https://evil");
        assert!(config["cvars"].get("matchzy_demo_upload_url").is_none());
        assert_eq!(config["cvars"]["tv_delay"], "105");
        assert_eq!(config["cvars"]["tv_enable"], "1");
        assert_eq!(config["cvars"]["mp_freezetime"], "10");
    }

    #[test]
    fn validation_rejects_mismatched_sides_and_bad_steamids() {
        let mut i = input();
        i.map_sides.pop();
        assert!(validate_input(&i).unwrap_err().contains("map_sides"));

        let mut i = input();
        i.team2.players = vec![("not-a-steamid".into(), "b1".into())];
        assert!(validate_input(&i).unwrap_err().contains("SteamID64"));

        let mut i = input();
        i.team1.players.clear();
        assert!(validate_input(&i).unwrap_err().contains("no players"));
    }

    fn map_ref(portal_id: &str) -> MatchzyMapRef {
        MatchzyMapRef {
            portal_id: portal_id.into(),
            engine_name: None,
            workshop_id: None,
        }
    }

    #[test]
    fn map_tokens_prefer_workshop_id_then_engine_name_then_portal_id() {
        let refs = vec![
            MatchzyMapRef {
                workshop_id: Some("3070244462".into()),
                engine_name: Some("de_cache".into()),
                ..map_ref("cache_workshop")
            },
            MatchzyMapRef {
                engine_name: Some("de_mirage".into()),
                ..map_ref("mirage_comp")
            },
            map_ref("de_nuke"),
        ];
        assert_eq!(
            matchzy_map_tokens(&refs).unwrap(),
            vec!["3070244462", "de_mirage", "de_nuke"]
        );
    }

    #[test]
    fn map_tokens_reject_unexpressible_entries() {
        let mut bad_ws = map_ref("m");
        bad_ws.workshop_id = Some("12ab34".into());
        assert!(
            matchzy_map_tokens(&[bad_ws])
                .unwrap_err()
                .contains("non-numeric workshop id")
        );

        let mut bad_name = map_ref("m");
        bad_name.engine_name = Some("de_cache; say pwned".into());
        assert!(
            matchzy_map_tokens(&[bad_name])
                .unwrap_err()
                .contains("invalid engine name")
        );
    }

    #[test]
    fn workshop_numeric_id_accepts_digits_and_filedetails_urls() {
        assert_eq!(
            workshop_numeric_id("3070244462").as_deref(),
            Some("3070244462")
        );
        assert_eq!(
            workshop_numeric_id(" 3070244462 ").as_deref(),
            Some("3070244462")
        );
        assert_eq!(
            workshop_numeric_id(
                "https://steamcommunity.com/sharedfiles/filedetails/?id=3070244462&searchtext=x"
            )
            .as_deref(),
            Some("3070244462")
        );
        assert_eq!(workshop_numeric_id("de_cache"), None);
        assert_eq!(workshop_numeric_id(""), None);
    }

    #[test]
    fn workshop_maplist_widens_restart_delay_unless_overridden() {
        let mut i = input();
        i.maplist = vec!["3070244462".into(), "de_nuke".into(), "de_ancient".into()];
        let config = build_matchzy_config(&i);
        assert_eq!(config["cvars"]["mp_match_restart_delay"], "45");

        // Tournament override wins.
        i.extra_cvars
            .insert("mp_match_restart_delay".into(), "60".into());
        let config = build_matchzy_config(&i);
        assert_eq!(config["cvars"]["mp_match_restart_delay"], "60");

        // No workshop maps → no forced delay.
        let config = build_matchzy_config(&input());
        assert!(config["cvars"].get("mp_match_restart_delay").is_none());
    }
}
