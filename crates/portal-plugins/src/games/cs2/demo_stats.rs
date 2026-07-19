//! CS2 demo statistics types.
//!
//! Types for pre-parsed demo stats from the external demo service.
//! Fetched from: `https://demos.cs210mans.uk/stats/{demo_name}.stats.json`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pre-parsed demo stats from the external demo service.
///
/// Fetched from: `https://demos.cs210mans.uk/stats/{demo_name}.stats.json`
///
/// Example: `https://demos.cs210mans.uk/stats/2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem.stats.json`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cs2DemoStats {
    /// Schema version for forward compatibility (optional, defaults to 3).
    #[serde(default = "default_schema_version")]
    pub schema_version: i32,

    /// Map name (e.g., "de_inferno").
    pub map: String,

    /// Match date as ISO 8601 string.
    pub match_date: String,

    /// Demo file name.
    pub demo_file: String,

    /// Unique match identifier.
    pub match_id: String,

    /// Teams keyed by team name (e.g., "team_Maxymimi" -> TeamInfo).
    pub teams: HashMap<String, TeamInfo>,

    /// Final scores keyed by team name (e.g., "team_Maxymimi" -> 13).
    pub final_score: HashMap<String, i32>,

    /// Aggregated player stats keyed by Steam ID (may be absent in some versions).
    #[serde(default)]
    pub player_summaries: HashMap<String, PlayerSummary>,

    /// Round-by-round data.
    pub rounds: Vec<RoundData>,
}

fn default_schema_version() -> i32 {
    3
}

/// Team information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TeamInfo {
    /// Team ID (2 for T, 3 for CT typically).
    pub team_id: i32,

    /// Team name.
    pub team_name: String,

    /// Side: "T" or "CT".
    pub team_side: String,
}

/// Round-by-round data.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundData {
    /// Round number (1-indexed).
    pub round_number: i32,

    /// Winning team name.
    pub winner_team: String,

    /// Winning side ("T" or "CT").
    pub winner_side: String,

    /// Score after this round, keyed by team name.
    pub round_score: HashMap<String, i32>,

    /// Player states at round start, keyed by Steam ID.
    pub player_states: HashMap<String, PlayerState>,

    /// Events during the round.
    pub events: Vec<RoundEvent>,

    /// Player stats for this round, keyed by Steam ID.
    pub player_stats: HashMap<String, RoundPlayerStats>,
}

/// Player state at round start.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerState {
    /// Steam ID (64-bit as number).
    pub player_id: u64,

    /// Player name during match.
    pub player_name: String,

    /// Team affiliation.
    pub team: TeamInfo,

    /// Starting money for the round.
    pub starting_money: i32,
}

/// Player stats for a single round.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundPlayerStats {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub damage: i32,
}

/// Aggregated player stats for the entire match.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PlayerSummary {
    pub player_id: u64,
    pub player_name: String,
    pub team: Option<TeamInfo>,
    #[serde(default)]
    pub kills: i32,
    #[serde(default)]
    pub deaths: i32,
    #[serde(default)]
    pub assists: i32,
    #[serde(default)]
    pub headshot_kills: i32,
    #[serde(default)]
    pub flash_assists: i32,
    #[serde(default)]
    pub damage_dealt: i32,
    #[serde(default)]
    pub utility_damage: i32,
    #[serde(default)]
    pub adr: f64,
    #[serde(default)]
    pub hs_percentage: f64,
    #[serde(default)]
    pub wallbangs: i32,
    #[serde(default)]
    pub smoke_kills: i32,
    #[serde(default)]
    pub blind_kills: i32,
    #[serde(default)]
    pub blinded_kills: i32,
    #[serde(default)]
    pub flash_duration: f64,
    #[serde(default)]
    pub enemies_flashed: i32,
    #[serde(default)]
    pub bomb_plants: i32,
    #[serde(default)]
    pub bomb_defuses: i32,
    /// Outgoing interactions keyed by target Steam ID.
    #[serde(default)]
    pub outgoing_interactions: HashMap<String, PlayerInteraction>,
    /// Incoming interactions keyed by source Steam ID.
    #[serde(default)]
    pub incoming_interactions: HashMap<String, PlayerInteraction>,
    /// Kills per weapon, keyed by weapon ID (as string).
    #[serde(default)]
    pub weapon_kills: HashMap<String, i32>,
}

/// Interaction between players (kills/assists).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PlayerInteraction {
    #[serde(default)]
    pub killed: Option<i32>,
    #[serde(default)]
    pub assisted: Option<i32>,
}

/// An event that occurred during a round.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoundEvent {
    pub event_type: String,
    pub event_time: f64,
    #[serde(default)]
    pub source_player_id: Option<u64>,
    #[serde(default)]
    pub target_player_id: Option<u64>,
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default)]
    pub weapon_type: Option<i32>,
    #[serde(default)]
    pub is_headshot: Option<bool>,
    #[serde(default)]
    pub attributes: Option<serde_json::Value>,
}

impl Cs2DemoStats {
    /// Parse match_date to DateTime.
    pub fn match_datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.match_date)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Check if this demo likely matches a tournament match by timeframe.
    pub fn matches_timeframe(&self, match_start: DateTime<Utc>, tolerance_minutes: i64) -> bool {
        self.match_datetime()
            .is_some_and(|dt| (dt - match_start).num_minutes().abs() <= tolerance_minutes)
    }

    /// Get team names.
    pub fn team_names(&self) -> Vec<String> {
        self.teams.keys().cloned().collect()
    }

    /// Get score for a team by name.
    pub fn score_for_team(&self, team_name: &str) -> Option<i32> {
        self.final_score.get(team_name).copied()
    }

    /// Get all Steam IDs that participated in the match.
    ///
    /// If `player_summaries` is populated, use it.
    /// Otherwise, extract from round data.
    pub fn all_steam_ids(&self) -> Vec<String> {
        if !self.player_summaries.is_empty() {
            return self.player_summaries.keys().cloned().collect();
        }

        // Extract from rounds
        let mut steam_ids: Vec<String> = self
            .rounds
            .iter()
            .flat_map(|r| r.player_states.keys())
            .cloned()
            .collect();
        steam_ids.sort();
        steam_ids.dedup();
        steam_ids
    }

    /// Get Steam IDs for a specific team.
    pub fn steam_ids_for_team(&self, team_name: &str) -> Vec<String> {
        if !self.player_summaries.is_empty() {
            return self
                .player_summaries
                .iter()
                .filter(|(_, ps)| ps.team.as_ref().is_some_and(|t| t.team_name == team_name))
                .map(|(steam_id, _)| steam_id.clone())
                .collect();
        }

        // Extract from first round's player states
        if let Some(first_round) = self.rounds.first() {
            return first_round
                .player_states
                .iter()
                .filter(|(_, ps)| ps.team.team_name == team_name)
                .map(|(steam_id, _)| steam_id.clone())
                .collect();
        }

        Vec::new()
    }

    /// Check if specific Steam IDs participated in the match.
    pub fn has_players(&self, steam_ids: &[String]) -> bool {
        let all_ids = self.all_steam_ids();
        steam_ids.iter().all(|id| all_ids.contains(id))
    }

    /// Get a player summary by Steam ID.
    ///
    /// If player_summaries is empty, aggregates from round data.
    pub fn get_player(&self, steam_id: &str) -> Option<PlayerSummary> {
        if let Some(summary) = self.player_summaries.get(steam_id) {
            return Some(summary.clone());
        }

        // Aggregate from rounds
        self.aggregate_player_stats(steam_id)
    }

    /// Aggregate player stats from round data.
    fn aggregate_player_stats(&self, steam_id: &str) -> Option<PlayerSummary> {
        let mut kills = 0;
        let mut deaths = 0;
        let mut assists = 0;
        let mut damage = 0;
        let mut player_name = String::new();
        let mut team: Option<TeamInfo> = None;
        let mut found = false;

        for round in &self.rounds {
            if let Some(stats) = round.player_stats.get(steam_id) {
                kills += stats.kills;
                deaths += stats.deaths;
                assists += stats.assists;
                damage += stats.damage;
                found = true;
            }
            if let Some(state) = round.player_states.get(steam_id) {
                if player_name.is_empty() {
                    player_name.clone_from(&state.player_name);
                }
                if team.is_none() {
                    team = Some(state.team.clone());
                }
            }
        }

        if !found {
            return None;
        }

        let rounds_played = self.rounds.len() as f64;
        let adr = if rounds_played > 0.0 {
            f64::from(damage) / rounds_played
        } else {
            0.0
        };

        Some(PlayerSummary {
            player_id: steam_id.parse().unwrap_or(0),
            player_name,
            team,
            kills,
            deaths,
            assists,
            damage_dealt: damage,
            adr,
            ..Default::default()
        })
    }

    /// Get all player summaries as a vector.
    ///
    /// If player_summaries is empty, aggregates from round data.
    pub fn all_player_summaries(&self) -> Vec<PlayerSummary> {
        if !self.player_summaries.is_empty() {
            return self.player_summaries.values().cloned().collect();
        }

        // Aggregate from rounds
        self.all_steam_ids()
            .iter()
            .filter_map(|id| self.aggregate_player_stats(id))
            .collect()
    }

    /// Determine the winning team name.
    pub fn winner_team_name(&self) -> Option<String> {
        self.final_score
            .iter()
            .max_by_key(|(_, score)| *score)
            .map(|(name, _)| name.clone())
    }

    /// Get total rounds played.
    pub fn total_rounds(&self) -> i32 {
        self.rounds.len() as i32
    }

    /// Get team info by name.
    pub fn get_team(&self, team_name: &str) -> Option<&TeamInfo> {
        self.teams.get(team_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn create_test_stats() -> Cs2DemoStats {
        let team_alpha = TeamInfo {
            team_id: 2,
            team_name: "team_Alpha".to_string(),
            team_side: "T".to_string(),
        };
        let team_beta = TeamInfo {
            team_id: 3,
            team_name: "team_Beta".to_string(),
            team_side: "CT".to_string(),
        };

        let mut teams = HashMap::new();
        teams.insert("team_Alpha".to_string(), team_alpha.clone());
        teams.insert("team_Beta".to_string(), team_beta.clone());

        let mut final_score = HashMap::new();
        final_score.insert("team_Alpha".to_string(), 16);
        final_score.insert("team_Beta".to_string(), 10);

        let mut player_states = HashMap::new();
        player_states.insert(
            "76561198000000001".to_string(),
            PlayerState {
                player_id: 76561198000000001,
                player_name: "Player1".to_string(),
                team: team_alpha.clone(),
                starting_money: 800,
            },
        );
        player_states.insert(
            "76561198000000002".to_string(),
            PlayerState {
                player_id: 76561198000000002,
                player_name: "Player2".to_string(),
                team: team_beta.clone(),
                starting_money: 800,
            },
        );

        let mut player_stats = HashMap::new();
        player_stats.insert(
            "76561198000000001".to_string(),
            RoundPlayerStats {
                kills: 2,
                deaths: 1,
                assists: 0,
                damage: 200,
            },
        );
        player_stats.insert(
            "76561198000000002".to_string(),
            RoundPlayerStats {
                kills: 1,
                deaths: 2,
                assists: 0,
                damage: 150,
            },
        );

        let mut round_score = HashMap::new();
        round_score.insert("team_Alpha".to_string(), 1);
        round_score.insert("team_Beta".to_string(), 0);

        Cs2DemoStats {
            schema_version: 3,
            map: "de_dust2".to_string(),
            match_date: "2024-09-14T20:17:30Z".to_string(),
            demo_file: "test_match.dem".to_string(),
            match_id: "test-match-123".to_string(),
            teams,
            final_score,
            player_summaries: HashMap::new(), // Test aggregation from rounds
            rounds: vec![RoundData {
                round_number: 1,
                winner_team: "team_Alpha".to_string(),
                winner_side: "T".to_string(),
                round_score,
                player_states,
                events: vec![],
                player_stats,
            }],
        }
    }

    #[test]
    fn test_team_names() {
        let stats = create_test_stats();
        let names = stats.team_names();
        assert!(names.contains(&"team_Alpha".to_string()));
        assert!(names.contains(&"team_Beta".to_string()));
    }

    #[test]
    fn test_score_for_team() {
        let stats = create_test_stats();
        assert_eq!(stats.score_for_team("team_Alpha"), Some(16));
        assert_eq!(stats.score_for_team("team_Beta"), Some(10));
        assert_eq!(stats.score_for_team("team_Unknown"), None);
    }

    #[test]
    fn test_all_steam_ids_from_rounds() {
        let stats = create_test_stats();
        let ids = stats.all_steam_ids();
        assert!(ids.contains(&"76561198000000001".to_string()));
        assert!(ids.contains(&"76561198000000002".to_string()));
    }

    #[test]
    fn test_steam_ids_for_team() {
        let stats = create_test_stats();
        let alpha_ids = stats.steam_ids_for_team("team_Alpha");
        assert!(alpha_ids.contains(&"76561198000000001".to_string()));

        let beta_ids = stats.steam_ids_for_team("team_Beta");
        assert!(beta_ids.contains(&"76561198000000002".to_string()));
    }

    #[test]
    fn test_winner_team_name() {
        let stats = create_test_stats();
        assert_eq!(stats.winner_team_name(), Some("team_Alpha".to_string()));
    }

    #[test]
    fn test_aggregate_player_stats() {
        let stats = create_test_stats();
        let summary = stats.get_player("76561198000000001").unwrap();
        assert_eq!(summary.kills, 2);
        assert_eq!(summary.deaths, 1);
        assert_eq!(summary.damage_dealt, 200);
        assert_eq!(summary.player_name, "Player1");
    }

    #[test]
    fn test_match_datetime() {
        let stats = create_test_stats();
        let dt = stats.match_datetime().unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 9);
        assert_eq!(dt.day(), 14);
    }

    #[test]
    fn test_total_rounds() {
        let stats = create_test_stats();
        assert_eq!(stats.total_rounds(), 1);
    }
}
