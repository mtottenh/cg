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
    /// Picked maps in play order — length defines `num_maps`.
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
    // Tournament overrides win over the defaults above — except the
    // security-relevant keys, which are portal-owned.
    for (key, value) in &input.extra_cvars {
        if matches!(
            key.as_str(),
            "sv_password"
                | "matchzy_remote_log_url"
                | "matchzy_remote_log_header_key"
                | "matchzy_remote_log_header_value"
        ) {
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
        i.extra_cvars.insert("mp_freezetime".into(), "10".into());
        let config = build_matchzy_config(&i);
        assert_eq!(config["cvars"]["sv_password"], "pw123");
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
}
