//! Counter-Strike 2 game plugin.
//!
//! Provides CS2-specific logic for:
//! - 7 competitive maps
//! - CS Rating system (numerical 0-35,000+ with color tiers)
//! - 5v5 team size
//! - Map pick/ban formats
//! - CS2-specific stats (K/D, ADR, HLTV rating, etc.)

use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{RatingError, StatsError};
use crate::traits::{GamePlugin, MapInfo, RankTier};
use crate::types::{
    DisplayStat, MapPickBanFormat, MapVetoAction, MatchData, MatchFormat, MatchmakingCriteria,
    RankedParticipant, RatingChange, TournamentFormatId, VetoActionType,
};

/// Counter-Strike 2 game plugin.
#[derive(Debug, Clone, Default)]
pub struct Cs2Plugin;

impl Cs2Plugin {
    /// Create a new CS2 plugin instance.
    pub fn new() -> Self {
        Self
    }
}

impl GamePlugin for Cs2Plugin {
    fn id(&self) -> &str {
        "cs2"
    }

    fn display_name(&self) -> &str {
        "Counter-Strike 2"
    }

    fn short_name(&self) -> &str {
        "CS2"
    }

    fn description(&self) -> Option<&str> {
        Some("Valve's tactical FPS, featuring 5v5 bomb defusal and competitive matchmaking.")
    }

    fn icon_url(&self) -> Option<&str> {
        None // Could be set to a CDN URL for CS2 icon
    }

    // ========================================================================
    // Maps
    // ========================================================================

    fn available_maps(&self) -> Vec<MapInfo> {
        vec![
            MapInfo {
                id: "de_dust2".to_string(),
                display_name: "Dust II".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_mirage".to_string(),
                display_name: "Mirage".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_inferno".to_string(),
                display_name: "Inferno".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_nuke".to_string(),
                display_name: "Nuke".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_ancient".to_string(),
                display_name: "Ancient".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_anubis".to_string(),
                display_name: "Anubis".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
            MapInfo {
                id: "de_vertigo".to_string(),
                display_name: "Vertigo".to_string(),
                image_url: None,
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
            },
        ]
    }

    fn default_map_pool(&self) -> Vec<String> {
        vec![
            "de_dust2".to_string(),
            "de_mirage".to_string(),
            "de_inferno".to_string(),
            "de_nuke".to_string(),
            "de_ancient".to_string(),
            "de_anubis".to_string(),
            "de_vertigo".to_string(),
        ]
    }

    // ========================================================================
    // Team Size
    // ========================================================================

    fn team_size_min(&self) -> u32 {
        5
    }

    fn team_size_max(&self) -> u32 {
        5
    }

    fn team_size_default(&self) -> u32 {
        5
    }

    // ========================================================================
    // Rank Tiers
    // ========================================================================

    fn rank_tiers(&self) -> Vec<RankTier> {
        // CS2 Premier uses a numerical CS Rating system (0-35,000+)
        // with color-coded tiers instead of the old CS:GO ranks
        vec![
            RankTier {
                id: "grey".to_string(),
                display_name: "Grey".to_string(),
                min_rating: 0,
                max_rating: Some(4999),
                icon_url: None,
                color: Some("#808080".to_string()), // Grey
                order: 1,
            },
            RankTier {
                id: "light_blue".to_string(),
                display_name: "Light Blue".to_string(),
                min_rating: 5000,
                max_rating: Some(9999),
                icon_url: None,
                color: Some("#87CEEB".to_string()), // Light Blue
                order: 2,
            },
            RankTier {
                id: "blue".to_string(),
                display_name: "Blue".to_string(),
                min_rating: 10000,
                max_rating: Some(14999),
                icon_url: None,
                color: Some("#4169E1".to_string()), // Royal Blue
                order: 3,
            },
            RankTier {
                id: "purple".to_string(),
                display_name: "Purple".to_string(),
                min_rating: 15000,
                max_rating: Some(19999),
                icon_url: None,
                color: Some("#9932CC".to_string()), // Purple
                order: 4,
            },
            RankTier {
                id: "pink".to_string(),
                display_name: "Pink".to_string(),
                min_rating: 20000,
                max_rating: Some(24999),
                icon_url: None,
                color: Some("#FF69B4".to_string()), // Pink
                order: 5,
            },
            RankTier {
                id: "red".to_string(),
                display_name: "Red".to_string(),
                min_rating: 25000,
                max_rating: Some(29999),
                icon_url: None,
                color: Some("#DC143C".to_string()), // Crimson Red
                order: 6,
            },
            RankTier {
                id: "gold".to_string(),
                display_name: "Gold".to_string(),
                min_rating: 30000,
                max_rating: None, // No upper limit
                icon_url: None,
                color: Some("#FFD700".to_string()), // Gold
                order: 7,
            },
        ]
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    fn player_stats_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kills": { "type": "integer", "default": 0 },
                "deaths": { "type": "integer", "default": 0 },
                "assists": { "type": "integer", "default": 0 },
                "headshots": { "type": "integer", "default": 0 },
                "mvps": { "type": "integer", "default": 0 },
                "total_damage": { "type": "integer", "default": 0 },
                "rounds_played": { "type": "integer", "default": 0 },
                "matches_played": { "type": "integer", "default": 0 },
                "matches_won": { "type": "integer", "default": 0 },
                "clutches_won": { "type": "integer", "default": 0 },
                "clutches_attempted": { "type": "integer", "default": 0 },
                "opening_kills": { "type": "integer", "default": 0 },
                "opening_deaths": { "type": "integer", "default": 0 },
                "flash_assists": { "type": "integer", "default": 0 },
                "utility_damage": { "type": "integer", "default": 0 }
            }
        })
    }

    fn calculate_player_stats(
        &self,
        match_data: &MatchData,
        player_id: Uuid,
        existing_stats: &Value,
    ) -> Result<Value, StatsError> {
        // Find this player's data in the match
        let player_data = match_data
            .players
            .iter()
            .find(|p| p.player_id == player_id)
            .ok_or_else(|| StatsError::MissingField("player_id".to_string()))?;

        let game_stats = &player_data.game_specific_stats;

        // Get existing stats or use defaults
        let mut stats = existing_stats.clone();
        if stats.is_null() {
            stats = json!({
                "kills": 0,
                "deaths": 0,
                "assists": 0,
                "headshots": 0,
                "mvps": 0,
                "total_damage": 0,
                "rounds_played": 0,
                "matches_played": 0,
                "matches_won": 0,
                "clutches_won": 0,
                "clutches_attempted": 0,
                "opening_kills": 0,
                "opening_deaths": 0,
                "flash_assists": 0,
                "utility_damage": 0
            });
        }

        // Update cumulative stats
        let add_stat = |stats: &mut Value, key: &str, game_stats: &Value| {
            let current = stats[key].as_i64().unwrap_or(0);
            let new = game_stats[key].as_i64().unwrap_or(0);
            stats[key] = json!(current + new);
        };

        add_stat(&mut stats, "kills", game_stats);
        add_stat(&mut stats, "deaths", game_stats);
        add_stat(&mut stats, "assists", game_stats);
        add_stat(&mut stats, "headshots", game_stats);
        add_stat(&mut stats, "mvps", game_stats);
        add_stat(&mut stats, "total_damage", game_stats);
        add_stat(&mut stats, "clutches_won", game_stats);
        add_stat(&mut stats, "clutches_attempted", game_stats);
        add_stat(&mut stats, "opening_kills", game_stats);
        add_stat(&mut stats, "opening_deaths", game_stats);
        add_stat(&mut stats, "flash_assists", game_stats);
        add_stat(&mut stats, "utility_damage", game_stats);

        // Update rounds played
        let current_rounds = stats["rounds_played"].as_i64().unwrap_or(0);
        let match_team = match_data
            .teams
            .iter()
            .find(|t| t.team_id == player_data.team_id);
        let match_rounds = match_team.and_then(|t| t.rounds_won).unwrap_or(0) as i64;
        let enemy_rounds = match_data
            .teams
            .iter()
            .find(|t| t.team_id != player_data.team_id)
            .and_then(|t| t.rounds_won)
            .unwrap_or(0) as i64;
        stats["rounds_played"] = json!(current_rounds + match_rounds + enemy_rounds);

        // Update match count
        let current_matches = stats["matches_played"].as_i64().unwrap_or(0);
        stats["matches_played"] = json!(current_matches + 1);

        // Update wins if this player won
        let won = match_data
            .winner_team_id
            .map(|w| w == player_data.team_id)
            .unwrap_or(false);
        if won {
            let current_wins = stats["matches_won"].as_i64().unwrap_or(0);
            stats["matches_won"] = json!(current_wins + 1);
        }

        Ok(stats)
    }

    fn format_player_stats(&self, stats: &Value) -> Vec<DisplayStat> {
        let kills = stats["kills"].as_i64().unwrap_or(0);
        let deaths = stats["deaths"].as_i64().unwrap_or(0);
        let assists = stats["assists"].as_i64().unwrap_or(0);
        let headshots = stats["headshots"].as_i64().unwrap_or(0);
        let total_damage = stats["total_damage"].as_i64().unwrap_or(0);
        let rounds_played = stats["rounds_played"].as_i64().unwrap_or(0);
        let matches_played = stats["matches_played"].as_i64().unwrap_or(0);
        let matches_won = stats["matches_won"].as_i64().unwrap_or(0);

        // Calculate derived stats
        let kd_ratio = if deaths > 0 {
            kills as f64 / deaths as f64
        } else {
            kills as f64
        };
        let hs_percent = if kills > 0 {
            (headshots as f64 / kills as f64) * 100.0
        } else {
            0.0
        };
        let adr = if rounds_played > 0 {
            total_damage as f64 / rounds_played as f64
        } else {
            0.0
        };
        let win_rate = if matches_played > 0 {
            (matches_won as f64 / matches_played as f64) * 100.0
        } else {
            0.0
        };

        vec![
            DisplayStat {
                key: "matches".to_string(),
                label: "Matches".to_string(),
                value: matches_played.to_string(),
                category: "General".to_string(),
                sort_order: 1,
            },
            DisplayStat {
                key: "win_rate".to_string(),
                label: "Win Rate".to_string(),
                value: format!("{:.1}%", win_rate),
                category: "General".to_string(),
                sort_order: 2,
            },
            DisplayStat {
                key: "kd_ratio".to_string(),
                label: "K/D Ratio".to_string(),
                value: format!("{:.2}", kd_ratio),
                category: "Combat".to_string(),
                sort_order: 3,
            },
            DisplayStat {
                key: "kills".to_string(),
                label: "Kills".to_string(),
                value: kills.to_string(),
                category: "Combat".to_string(),
                sort_order: 4,
            },
            DisplayStat {
                key: "deaths".to_string(),
                label: "Deaths".to_string(),
                value: deaths.to_string(),
                category: "Combat".to_string(),
                sort_order: 5,
            },
            DisplayStat {
                key: "assists".to_string(),
                label: "Assists".to_string(),
                value: assists.to_string(),
                category: "Combat".to_string(),
                sort_order: 6,
            },
            DisplayStat {
                key: "hs_percent".to_string(),
                label: "HS%".to_string(),
                value: format!("{:.1}%", hs_percent),
                category: "Combat".to_string(),
                sort_order: 7,
            },
            DisplayStat {
                key: "adr".to_string(),
                label: "ADR".to_string(),
                value: format!("{:.1}", adr),
                category: "Combat".to_string(),
                sort_order: 8,
            },
        ]
    }

    // ========================================================================
    // Rating
    // ========================================================================

    fn calculate_rating_change(
        &self,
        participants: &[RankedParticipant],
    ) -> Result<Vec<RatingChange>, RatingError> {
        if participants.is_empty() {
            return Err(RatingError::InsufficientParticipants);
        }

        // Simple Elo-like calculation (can be replaced with Glicko-2 later)
        // K-factor varies by rating - scaled for CS2 Premier (0-35,000+)
        // Higher K for lower ratings means faster progression at lower ranks
        let get_k_factor = |rating: i32| -> f64 {
            if rating < 10000 {
                // Grey and Light Blue: faster progression
                200.0
            } else if rating < 20000 {
                // Blue and Purple: moderate progression
                150.0
            } else {
                // Pink, Red, Gold: slower, more stable ratings
                100.0
            }
        };

        // Group by team
        let team1: Vec<_> = participants.iter().filter(|p| p.team_id == 1).collect();
        let team2: Vec<_> = participants.iter().filter(|p| p.team_id == 2).collect();

        if team1.is_empty() || team2.is_empty() {
            return Err(RatingError::InsufficientParticipants);
        }

        // Calculate average ratings
        let avg_rating_1: f64 = team1.iter().map(|p| p.rating as f64).sum::<f64>() / team1.len() as f64;
        let avg_rating_2: f64 = team2.iter().map(|p| p.rating as f64).sum::<f64>() / team2.len() as f64;

        // Expected scores (Elo formula)
        // Using 2000 as divisor instead of 400 due to the larger CS2 Premier scale (0-35,000+)
        let expected_1 = 1.0 / (1.0 + 10.0_f64.powf((avg_rating_2 - avg_rating_1) / 2000.0));
        let expected_2 = 1.0 - expected_1;

        // Determine actual scores
        let team1_won = team1.first().map(|p| p.is_winner).unwrap_or(false);
        let actual_1 = if team1_won { 1.0 } else { 0.0 };
        let actual_2 = if team1_won { 0.0 } else { 1.0 };

        let mut changes = Vec::new();

        // Calculate changes for team 1
        for p in &team1 {
            let k = get_k_factor(p.rating);
            let change = (k * (actual_1 - expected_1)).round() as i32;
            let new_rating = (p.rating + change).max(0);

            changes.push(RatingChange {
                player_id: p.player_id,
                old_rating: p.rating,
                new_rating,
                old_deviation: p.rating_deviation,
                new_deviation: p.rating_deviation, // Keep same for simple Elo
                old_volatility: p.volatility,
                new_volatility: p.volatility,
            });
        }

        // Calculate changes for team 2
        for p in &team2 {
            let k = get_k_factor(p.rating);
            let change = (k * (actual_2 - expected_2)).round() as i32;
            let new_rating = (p.rating + change).max(0);

            changes.push(RatingChange {
                player_id: p.player_id,
                old_rating: p.rating,
                new_rating,
                old_deviation: p.rating_deviation,
                new_deviation: p.rating_deviation,
                old_volatility: p.volatility,
                new_volatility: p.volatility,
            });
        }

        Ok(changes)
    }

    // ========================================================================
    // Matchmaking
    // ========================================================================

    fn matchmaking_criteria(&self) -> MatchmakingCriteria {
        // CS2 Premier uses ratings from 0-35,000+
        // These values are scaled accordingly
        MatchmakingCriteria {
            max_rating_difference: 3000,      // ~half a tier
            max_team_rating_difference: 1000, // Smaller team avg diff
            max_queue_time_seconds: 300,
            rating_relaxation_per_minute: 500, // Relax faster with wider scale
            min_games_for_strict_matching: 10,
            allow_wide_party_spread: false,
            max_party_rating_spread: 5000, // CS2 allows queuing within ~1 tier
        }
    }

    // ========================================================================
    // Tournament Support
    // ========================================================================

    fn supported_tournament_formats(&self) -> Vec<TournamentFormatId> {
        vec![
            TournamentFormatId::SingleElimination,
            TournamentFormatId::DoubleElimination,
            TournamentFormatId::Swiss,
            TournamentFormatId::GroupStage,
        ]
    }

    fn map_pick_ban_formats(&self) -> Vec<MapPickBanFormat> {
        vec![
            // Standard pick (random map from pool)
            MapPickBanFormat {
                id: "random".to_string(),
                display_name: "Random".to_string(),
                sequence: vec![],
                description: "Random map selected from pool".to_string(),
            },
            // Bo1 veto format (ban-ban-ban-ban-ban-ban, last map played)
            MapPickBanFormat {
                id: "bo1_veto".to_string(),
                display_name: "Best of 1 Veto".to_string(),
                sequence: vec![
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 0, action: VetoActionType::Decider },
                ],
                description: "Teams alternate banning maps until one remains".to_string(),
            },
            // Bo3 veto format
            MapPickBanFormat {
                id: "bo3_veto".to_string(),
                display_name: "Best of 3 Veto".to_string(),
                sequence: vec![
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 1, action: VetoActionType::Pick },
                    MapVetoAction { team: 2, action: VetoActionType::Pick },
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 0, action: VetoActionType::Decider },
                ],
                description: "Ban-Ban-Pick-Pick-Ban-Ban-Decider".to_string(),
            },
            // Bo5 veto format
            MapPickBanFormat {
                id: "bo5_veto".to_string(),
                display_name: "Best of 5 Veto".to_string(),
                sequence: vec![
                    MapVetoAction { team: 1, action: VetoActionType::Ban },
                    MapVetoAction { team: 2, action: VetoActionType::Ban },
                    MapVetoAction { team: 1, action: VetoActionType::Pick },
                    MapVetoAction { team: 2, action: VetoActionType::Pick },
                    MapVetoAction { team: 1, action: VetoActionType::Pick },
                    MapVetoAction { team: 2, action: VetoActionType::Pick },
                    MapVetoAction { team: 0, action: VetoActionType::Decider },
                ],
                description: "Ban-Ban-Pick-Pick-Pick-Pick-Decider".to_string(),
            },
        ]
    }

    fn default_match_format(&self) -> MatchFormat {
        MatchFormat::Bo3
    }

    fn supported_match_formats(&self) -> Vec<MatchFormat> {
        vec![MatchFormat::Bo1, MatchFormat::Bo3, MatchFormat::Bo5]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = Cs2Plugin::new();

        assert_eq!(plugin.id(), "cs2");
        assert_eq!(plugin.display_name(), "Counter-Strike 2");
        assert_eq!(plugin.short_name(), "CS2");
        assert!(plugin.description().is_some());
    }

    #[test]
    fn test_maps() {
        let plugin = Cs2Plugin::new();
        let maps = plugin.available_maps();

        assert_eq!(maps.len(), 7);
        assert!(maps.iter().any(|m| m.id == "de_dust2"));
        assert!(maps.iter().any(|m| m.id == "de_mirage"));
        assert!(maps.iter().any(|m| m.id == "de_inferno"));
    }

    #[test]
    fn test_default_map_pool() {
        let plugin = Cs2Plugin::new();
        let pool = plugin.default_map_pool();

        assert_eq!(pool.len(), 7);
        assert!(pool.contains(&"de_dust2".to_string()));
    }

    #[test]
    fn test_validate_map_pool() {
        let plugin = Cs2Plugin::new();

        // Valid pool
        let valid_pool = vec!["de_dust2".to_string(), "de_mirage".to_string()];
        assert!(plugin.validate_map_pool(&valid_pool).is_ok());

        // Invalid pool
        let invalid_pool = vec!["de_nonexistent".to_string()];
        assert!(plugin.validate_map_pool(&invalid_pool).is_err());
    }

    #[test]
    fn test_rank_tiers() {
        let plugin = Cs2Plugin::new();
        let tiers = plugin.rank_tiers();

        // CS2 Premier has 7 color-coded tiers
        assert_eq!(tiers.len(), 7);

        // Check ordering
        assert_eq!(tiers[0].id, "grey");
        assert_eq!(tiers[6].id, "gold");

        // Check Gold has no max rating (unbounded top tier)
        assert!(tiers[6].max_rating.is_none());

        // Check rating boundaries
        assert_eq!(tiers[0].min_rating, 0);
        assert_eq!(tiers[0].max_rating, Some(4999));
        assert_eq!(tiers[1].min_rating, 5000);
        assert_eq!(tiers[6].min_rating, 30000);
    }

    #[test]
    fn test_rating_to_rank_tier() {
        let plugin = Cs2Plugin::new();

        // Grey tier (0-4,999)
        let grey = plugin.rating_to_rank_tier(2500).unwrap();
        assert_eq!(grey.id, "grey");

        // Light Blue tier (5,000-9,999)
        let light_blue = plugin.rating_to_rank_tier(7500).unwrap();
        assert_eq!(light_blue.id, "light_blue");

        // Purple tier (15,000-19,999)
        let purple = plugin.rating_to_rank_tier(17000).unwrap();
        assert_eq!(purple.id, "purple");

        // Gold tier (30,000+)
        let gold = plugin.rating_to_rank_tier(35000).unwrap();
        assert_eq!(gold.id, "gold");
    }

    #[test]
    fn test_team_size() {
        let plugin = Cs2Plugin::new();

        assert_eq!(plugin.team_size_min(), 5);
        assert_eq!(plugin.team_size_max(), 5);
        assert_eq!(plugin.team_size_default(), 5);
    }

    #[test]
    fn test_map_pick_ban_formats() {
        let plugin = Cs2Plugin::new();
        let formats = plugin.map_pick_ban_formats();

        assert!(formats.len() >= 4);
        assert!(formats.iter().any(|f| f.id == "bo1_veto"));
        assert!(formats.iter().any(|f| f.id == "bo3_veto"));
    }

    #[test]
    fn test_supported_tournament_formats() {
        let plugin = Cs2Plugin::new();
        let formats = plugin.supported_tournament_formats();

        assert!(formats.contains(&TournamentFormatId::SingleElimination));
        assert!(formats.contains(&TournamentFormatId::DoubleElimination));
        assert!(formats.contains(&TournamentFormatId::Swiss));
    }

    #[test]
    fn test_rating_calculation() {
        let plugin = Cs2Plugin::new();

        // Using CS2 Premier scale ratings (0-35,000+)
        // Both players at 15,000 (Purple tier)
        let participants = vec![
            RankedParticipant {
                player_id: Uuid::new_v4(),
                team_id: 1,
                rating: 15000,
                rating_deviation: 50.0,
                volatility: 0.06,
                is_winner: true,
            },
            RankedParticipant {
                player_id: Uuid::new_v4(),
                team_id: 2,
                rating: 15000,
                rating_deviation: 50.0,
                volatility: 0.06,
                is_winner: false,
            },
        ];

        let changes = plugin.calculate_rating_change(&participants).unwrap();
        assert_eq!(changes.len(), 2);

        // Winner should gain rating
        let winner_change = changes.iter().find(|c| c.new_rating > c.old_rating);
        assert!(winner_change.is_some());

        // Loser should lose rating
        let loser_change = changes.iter().find(|c| c.new_rating < c.old_rating);
        assert!(loser_change.is_some());

        // With equal ratings, expected score is 0.5, so winner gains ~K/2 and loser loses ~K/2
        // At Purple tier (15,000), K=150, so change should be ~75
        let winner = winner_change.unwrap();
        assert!(winner.new_rating - winner.old_rating > 50); // Should gain meaningful rating
    }

    #[test]
    fn test_format_player_stats() {
        let plugin = Cs2Plugin::new();

        let stats = json!({
            "kills": 100,
            "deaths": 80,
            "assists": 30,
            "headshots": 45,
            "total_damage": 10000,
            "rounds_played": 100,
            "matches_played": 10,
            "matches_won": 6
        });

        let formatted = plugin.format_player_stats(&stats);
        assert!(!formatted.is_empty());

        // Check K/D ratio is calculated
        let kd = formatted.iter().find(|s| s.key == "kd_ratio").unwrap();
        assert_eq!(kd.value, "1.25");
    }
}
