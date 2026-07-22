//! CS2 stat catalog and per-demo fact extraction for tournament awards.
//!
//! The catalog ([`stat_catalog`]) lists the stat keys award templates can
//! reference and drives UI pickers. Extraction ([`extract_facts`]) turns a
//! demo's stats JSON ([`Cs2DemoStats`]) into EAV facts keyed by Steam ID.
//!
//! Weapon kills are an open set: the catalog lists a representative set of
//! `kills.weapon.{name}` entries for common weapons, but extraction emits a
//! `kills.weapon.{name}` fact for whatever weapon names appear in the data.

use serde_json::Value;

use super::demo_stats::Cs2DemoStats;
use crate::stats::{StatDefinition, StatFact, StatValueType};

/// Representative weapons listed in the catalog (key suffix, display name).
///
/// Extraction is data-driven and not limited to this list.
const CATALOG_WEAPONS: &[(&str, &str)] = &[
    ("ak47", "AK-47"),
    ("m4a1", "M4A1"),
    ("awp", "AWP"),
    ("deagle", "Desert Eagle"),
    ("mag7", "MAG-7"),
    ("knife", "Knife"),
    ("glock", "Glock-18"),
    ("usp_silencer", "USP-S"),
    ("nova", "Nova"),
    ("xm1014", "XM1014"),
    ("sawedoff", "Sawed-Off"),
    ("mp9", "MP9"),
    ("mac10", "MAC-10"),
    ("p90", "P90"),
    ("galilar", "Galil AR"),
    ("famas", "FAMAS"),
    ("ssg08", "SSG 08"),
    ("scar20", "SCAR-20"),
    ("g3sg1", "G3SG1"),
];

fn def(
    key: &str,
    label: &str,
    category: &str,
    value_type: StatValueType,
    description: &str,
) -> StatDefinition {
    StatDefinition {
        key: key.to_string(),
        label: label.to_string(),
        category: category.to_string(),
        value_type,
        description: description.to_string(),
    }
}

/// The CS2 stat catalog.
pub fn stat_catalog() -> Vec<StatDefinition> {
    let mut catalog = vec![
        // Combat — counts
        def(
            "kills",
            "Kills",
            "Combat",
            StatValueType::Count,
            "Total enemy kills.",
        ),
        def(
            "deaths",
            "Deaths",
            "Combat",
            StatValueType::Count,
            "Total deaths.",
        ),
        def(
            "assists",
            "Assists",
            "Combat",
            StatValueType::Count,
            "Total kill assists.",
        ),
        def(
            "headshot_kills",
            "Headshot Kills",
            "Combat",
            StatValueType::Count,
            "Kills landed as headshots.",
        ),
        def(
            "damage_dealt",
            "Damage Dealt",
            "Combat",
            StatValueType::Count,
            "Total damage dealt to enemies.",
        ),
        def(
            "wallbangs",
            "Wallbang Kills",
            "Combat",
            StatValueType::Count,
            "Kills through walls or other penetrable surfaces.",
        ),
        def(
            "smoke_kills",
            "Smoke Kills",
            "Combat",
            StatValueType::Count,
            "Kills on enemies through smoke.",
        ),
        def(
            "kills.while_blind",
            "Kills While Flashed",
            "Combat",
            StatValueType::Count,
            "Kills scored while the killer was flashed.",
        ),
        def(
            "kills.on_blinded",
            "Kills on Flashed Enemies",
            "Combat",
            StatValueType::Count,
            "Kills on enemies who were flashed.",
        ),
        // Combat — per-demo ratios
        def(
            "adr",
            "ADR",
            "Combat",
            StatValueType::Ratio,
            "Average damage per round for the demo.",
        ),
        def(
            "hs_percentage",
            "Headshot %",
            "Combat",
            StatValueType::Ratio,
            "Percentage of kills that were headshots for the demo.",
        ),
        // Utility
        def(
            "flash_assists",
            "Flash Assists",
            "Utility",
            StatValueType::Count,
            "Kills assisted by flashing the victim.",
        ),
        def(
            "utility_damage",
            "Utility Damage",
            "Utility",
            StatValueType::Count,
            "Damage dealt with grenades and other utility.",
        ),
        // Objective
        def(
            "bomb_plants",
            "Bomb Plants",
            "Objective",
            StatValueType::Count,
            "Bombs planted.",
        ),
        def(
            "bomb_defuses",
            "Bomb Defuses",
            "Objective",
            StatValueType::Count,
            "Bombs defused.",
        ),
    ];

    catalog.extend(CATALOG_WEAPONS.iter().map(|(key, name)| {
        def(
            &format!("kills.weapon.{key}"),
            &format!("{name} Kills"),
            "Weapons",
            StatValueType::Count,
            &format!("Kills with the {name}."),
        )
    }));

    catalog
}

/// Normalize a raw weapon name from demo data into a stat-key suffix.
///
/// Rules: lowercase, strip a `weapon_` prefix. Purely numeric names (legacy
/// demos used opaque weapon IDs) become `id_{n}` so the data is preserved but
/// clearly marked legacy. Returns `None` for empty names.
fn normalize_weapon_key(raw: &str) -> Option<String> {
    let lowered = raw.trim().to_lowercase();
    let name = lowered.strip_prefix("weapon_").unwrap_or(&lowered);
    if name.is_empty() {
        return None;
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("id_{name}"))
    } else {
        Some(name.to_string())
    }
}

/// Extract per-player stat facts from a demo's stats JSON.
///
/// Every scalar in the catalog is emitted for every player, **including
/// zeros**: zero rows make AVG-style aggregations correct (a player who never
/// defused still weighs into an average) and let qualifiers count
/// participation. `rounds_played` (the demo's round count) is also emitted per
/// player for per-round qualifiers, plus one `kills.weapon.{name}` fact per
/// weapon that appears in the player's data (open set).
///
/// `adr` and `hs_percentage` are per-demo ratios; the aggregation layer is
/// responsible for averaging or recomputing them across demos.
///
/// Returns an empty list if the JSON does not parse as [`Cs2DemoStats`].
pub fn extract_facts(stats_json: &Value) -> Vec<StatFact> {
    let stats: Cs2DemoStats = match serde_json::from_value(stats_json.clone()) {
        Ok(stats) => stats,
        Err(error) => {
            tracing::warn!(%error, "CS2 stat fact extraction: stats JSON did not parse");
            return Vec::new();
        }
    };

    let rounds_played = stats.rounds.len() as f64;
    let mut facts = Vec::new();

    for steam_id in stats.all_steam_ids() {
        // `get_player` falls back to aggregating round data for legacy demos
        // without `player_summaries`.
        let Some(summary) = stats.get_player(&steam_id) else {
            continue;
        };

        let mut push = |stat_key: String, value: f64| {
            facts.push(StatFact {
                steam_id: steam_id.clone(),
                stat_key,
                value,
            });
        };

        let scalars: [(&str, f64); 16] = [
            ("kills", f64::from(summary.kills)),
            ("deaths", f64::from(summary.deaths)),
            ("assists", f64::from(summary.assists)),
            ("headshot_kills", f64::from(summary.headshot_kills)),
            ("damage_dealt", f64::from(summary.damage_dealt)),
            ("wallbangs", f64::from(summary.wallbangs)),
            ("smoke_kills", f64::from(summary.smoke_kills)),
            ("kills.while_blind", f64::from(summary.blind_kills)),
            ("kills.on_blinded", f64::from(summary.blinded_kills)),
            ("flash_assists", f64::from(summary.flash_assists)),
            ("utility_damage", f64::from(summary.utility_damage)),
            ("bomb_plants", f64::from(summary.bomb_plants)),
            ("bomb_defuses", f64::from(summary.bomb_defuses)),
            ("adr", summary.adr),
            ("hs_percentage", summary.hs_percentage),
            ("rounds_played", rounds_played),
        ];
        for (key, value) in scalars {
            push(key.to_string(), value);
        }

        for (raw_name, count) in &summary.weapon_kills {
            if let Some(name) = normalize_weapon_key(raw_name) {
                push(format!("kills.weapon.{name}"), f64::from(*count));
            }
        }
    }

    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Find a single fact's value for (steam_id, stat_key).
    fn fact_value(facts: &[StatFact], steam_id: &str, stat_key: &str) -> Option<f64> {
        facts
            .iter()
            .find(|f| f.steam_id == steam_id && f.stat_key == stat_key)
            .map(|f| f.value)
    }

    #[test]
    fn catalog_contains_seeded_template_keys() {
        // These keys are seeded into award templates by migration; the catalog
        // must keep listing them so the two cannot drift.
        let catalog = stat_catalog();
        for key in [
            "headshot_kills",
            "kills.weapon.mag7",
            "kills.while_blind",
            "kills.weapon.knife",
            "bomb_defuses",
            "utility_damage",
            "wallbangs",
            "flash_assists",
        ] {
            assert!(
                catalog.iter().any(|d| d.key == key),
                "catalog is missing seeded template key {key}"
            );
        }
    }

    #[test]
    fn catalog_uses_known_categories_and_ratio_types() {
        let catalog = stat_catalog();
        for d in &catalog {
            assert!(
                ["Combat", "Utility", "Objective", "Weapons"].contains(&d.category.as_str()),
                "unexpected category {} for {}",
                d.category,
                d.key
            );
        }
        let ratios: Vec<&str> = catalog
            .iter()
            .filter(|d| d.value_type == StatValueType::Ratio)
            .map(|d| d.key.as_str())
            .collect();
        assert_eq!(ratios, ["adr", "hs_percentage"]);
        // Every weapon entry is a Count in the Weapons category.
        assert!(
            catalog
                .iter()
                .filter(|d| d.key.starts_with("kills.weapon."))
                .all(|d| d.category == "Weapons" && d.value_type == StatValueType::Count)
        );
    }

    #[test]
    fn normalize_weapon_key_rules() {
        assert_eq!(normalize_weapon_key("mag7"), Some("mag7".to_string()));
        assert_eq!(
            normalize_weapon_key("weapon_knife"),
            Some("knife".to_string())
        );
        assert_eq!(normalize_weapon_key("AK47"), Some("ak47".to_string()));
        assert_eq!(
            normalize_weapon_key("Weapon_AWP").as_deref(),
            Some("awp"),
            "prefix strip happens after lowercasing"
        );
        assert_eq!(normalize_weapon_key("402"), Some("id_402".to_string()));
        assert_eq!(
            normalize_weapon_key("weapon_402"),
            Some("id_402".to_string())
        );
        assert_eq!(normalize_weapon_key(""), None);
        assert_eq!(normalize_weapon_key("weapon_"), None);
    }

    fn minimal_round(n: i32) -> Value {
        json!({
            "round_number": n,
            "winner_team": "team_A",
            "winner_side": "T",
            "round_score": {},
            "player_states": {},
            "events": [],
            "player_stats": {}
        })
    }

    #[test]
    fn extracts_facts_from_inline_stats() {
        let p1 = "76561198000000001";
        let p2 = "76561198000000002";
        let stats_json = json!({
            "map": "de_dust2",
            "match_date": "2024-09-14T20:17:30Z",
            "demo_file": "test.dem",
            "match_id": "m-1",
            "teams": {
                "team_A": { "team_id": 2, "team_name": "team_A", "team_side": "T" },
                "team_B": { "team_id": 3, "team_name": "team_B", "team_side": "CT" }
            },
            "final_score": { "team_A": 13, "team_B": 7 },
            "rounds": [minimal_round(1), minimal_round(2)],
            "player_summaries": {
                p1: {
                    "player_id": 76_561_198_000_000_001_u64,
                    "player_name": "P1",
                    "kills": 20, "deaths": 10, "assists": 4,
                    "headshot_kills": 9, "flash_assists": 2,
                    "damage_dealt": 2100, "utility_damage": 150,
                    "adr": 105.0, "hs_percentage": 45.0,
                    "wallbangs": 1, "smoke_kills": 0,
                    "blind_kills": 2, "blinded_kills": 3,
                    "bomb_plants": 3, "bomb_defuses": 0,
                    "weapon_kills": { "mag7": 3, "weapon_knife": 1, "402": 2 }
                },
                p2: {
                    "player_id": 76_561_198_000_000_002_u64,
                    "player_name": "P2",
                    "kills": 0, "deaths": 15, "assists": 1
                }
            }
        });

        let facts = extract_facts(&stats_json);

        // 16 scalar facts per player; p1 additionally has 3 weapon facts.
        assert_eq!(facts.iter().filter(|f| f.steam_id == p1).count(), 19);
        assert_eq!(facts.iter().filter(|f| f.steam_id == p2).count(), 16);

        // Weapon-name normalization, including the legacy numeric-ID form.
        assert_eq!(fact_value(&facts, p1, "kills.weapon.mag7"), Some(3.0));
        assert_eq!(fact_value(&facts, p1, "kills.weapon.knife"), Some(1.0));
        assert_eq!(fact_value(&facts, p1, "kills.weapon.id_402"), Some(2.0));

        // Renamed blind-kill keys.
        assert_eq!(fact_value(&facts, p1, "kills.while_blind"), Some(2.0));
        assert_eq!(fact_value(&facts, p1, "kills.on_blinded"), Some(3.0));

        // rounds_played comes from the demo's round count.
        assert_eq!(fact_value(&facts, p1, "rounds_played"), Some(2.0));
        assert_eq!(fact_value(&facts, p2, "rounds_played"), Some(2.0));

        // Zeros are emitted (participation rows for AVG-style aggregation).
        assert_eq!(fact_value(&facts, p1, "smoke_kills"), Some(0.0));
        assert_eq!(fact_value(&facts, p2, "kills"), Some(0.0));
        assert_eq!(fact_value(&facts, p2, "bomb_defuses"), Some(0.0));

        // Ratios pass through per-demo values.
        assert_eq!(fact_value(&facts, p1, "adr"), Some(105.0));
        assert_eq!(fact_value(&facts, p1, "hs_percentage"), Some(45.0));
    }

    #[test]
    fn extract_from_unparseable_json_yields_no_facts() {
        assert!(extract_facts(&json!({ "not": "a demo" })).is_empty());
        assert!(extract_facts(&json!(null)).is_empty());
    }

    #[test]
    fn extracts_facts_from_committed_fixture() {
        let raw = include_str!("../../../../portal-api/tests/fixtures/demo_stats.json");
        let stats_json: Value = serde_json::from_str(raw).expect("fixture parses as JSON");

        let facts = extract_facts(&stats_json);
        assert!(!facts.is_empty(), "fixture should produce facts");

        // 10 players, 16 scalar facts each (fixture has no weapon_kills data).
        let mut steam_ids: Vec<&str> = facts.iter().map(|f| f.steam_id.as_str()).collect();
        steam_ids.sort_unstable();
        steam_ids.dedup();
        assert_eq!(steam_ids.len(), 10);
        assert_eq!(facts.len(), 10 * 16);

        // Spot-check a known player (dewsy).
        let dewsy = "76561197962015608";
        assert_eq!(fact_value(&facts, dewsy, "kills"), Some(18.0));
        assert_eq!(fact_value(&facts, dewsy, "headshot_kills"), Some(5.0));
        assert_eq!(fact_value(&facts, dewsy, "kills.on_blinded"), Some(1.0));
        assert_eq!(fact_value(&facts, dewsy, "bomb_defuses"), Some(1.0));
        assert_eq!(fact_value(&facts, dewsy, "adr"), Some(77.96));

        // Every player gets participation rows even when zero.
        for id in &steam_ids {
            assert!(fact_value(&facts, id, "kills").is_some());
            assert!(fact_value(&facts, id, "deaths").is_some());
            assert!(fact_value(&facts, id, "rounds_played").is_some());
        }
    }
}
