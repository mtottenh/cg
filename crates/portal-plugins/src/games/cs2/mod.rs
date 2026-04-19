//! Counter-Strike 2 game plugin.
//!
//! Provides CS2-specific logic for:
//! - 7 competitive maps
//! - CS Rating system (numerical 0-35,000+ with color tiers)
//! - 5v5 team size
//! - Map pick/ban formats
//! - CS2-specific stats (K/D, ADR, HLTV rating, etc.)
//! - Demo evidence discovery and validation

pub mod demo_client;
pub mod demo_stats;
pub mod evidence_validator;

pub use demo_client::Cs2DemoClient;
pub use demo_stats::Cs2DemoStats;
pub use evidence_validator::Cs2EvidenceValidator;

use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{PluginError, RatingError, StatsError};
use crate::traits::{EvidencePlugin, GamePlugin, MapInfo, RankTier, SideOption, TournamentPlugin};
use crate::types::{
    DemoMetadata, DiscoveredEvidence, DisplayStat, EvidenceStorage, EvidenceType,
    EvidenceValidation, ExtractedResult, GameResult, MapPickBanFormat, MapVetoAction, MatchContext,
    MatchData, MatchFormat, MatchmakingCriteria, PlayerStatsContext, RankedParticipant,
    RatingChange, TournamentFormatId, VetoActionType,
};
use portal_core::types::veto::{SideSelectionMode, VetoFormatConfig};
use chrono::Utc;

/// Counter-Strike 2 game plugin.
#[derive(Debug, Clone, Default)]
pub struct Cs2Plugin;

impl Cs2Plugin {
    /// Create a new CS2 plugin instance.
    pub const fn new() -> Self {
        Self
    }
}

impl GamePlugin for Cs2Plugin {
    fn id(&self) -> &'static str {
        "cs2"
    }

    fn as_tournament_plugin(&self) -> Option<&dyn TournamentPlugin> {
        Some(self)
    }

    fn as_evidence_plugin(&self) -> Option<&dyn EvidencePlugin> {
        Some(self)
    }

    fn display_name(&self) -> &'static str {
        "Counter-Strike 2"
    }

    fn short_name(&self) -> &'static str {
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
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_dust2_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_mirage".to_string(),
                display_name: "Mirage".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_mirage_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_inferno".to_string(),
                display_name: "Inferno".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_inferno_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_nuke".to_string(),
                display_name: "Nuke".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_nuke_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_ancient".to_string(),
                display_name: "Ancient".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_ancient_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_anubis".to_string(),
                display_name: "Anubis".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_anubis_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
            },
            MapInfo {
                id: "de_vertigo".to_string(),
                display_name: "Vertigo".to_string(),
                image_url: Some("https://raw.githubusercontent.com/MurkyYT/cs2-map-icons/main/images/thumbs/de_vertigo_1_png.png".to_string()),
                game_modes: vec!["competitive".to_string(), "casual".to_string()],
                external_id: None,
                external_url: None,
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
        let match_rounds = i64::from(match_team.and_then(|t| t.rounds_won).unwrap_or(0));
        let enemy_rounds = i64::from(match_data
            .teams
            .iter()
            .find(|t| t.team_id != player_data.team_id)
            .and_then(|t| t.rounds_won)
            .unwrap_or(0));
        stats["rounds_played"] = json!(current_rounds + match_rounds + enemy_rounds);

        // Update match count
        let current_matches = stats["matches_played"].as_i64().unwrap_or(0);
        stats["matches_played"] = json!(current_matches + 1);

        // Update wins if this player won
        let won = match_data
            .winner_team_id
            .is_some_and(|w| w == player_data.team_id);
        if won {
            let current_wins = stats["matches_won"].as_i64().unwrap_or(0);
            stats["matches_won"] = json!(current_wins + 1);
        }

        Ok(stats)
    }

    fn format_player_stats(&self, stats: &Value, context: &PlayerStatsContext) -> Vec<DisplayStat> {
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

        // Determine rank tier color
        let rank_color = self
            .rating_to_rank_tier(context.rating)
            .and_then(|tier| tier.color);

        let mut result = vec![
            // Rating stats
            DisplayStat {
                key: "elo_current".to_string(),
                label: "CS Rating".to_string(),
                value: context.rating.to_string(),
                category: "Rating".to_string(),
                sort_order: 1,
                color: rank_color.clone(),
            },
            DisplayStat {
                key: "elo_peak".to_string(),
                label: "Peak Rating".to_string(),
                value: context.peak_rating.to_string(),
                category: "Rating".to_string(),
                sort_order: 2,
                color: None,
            },
        ];

        if let Some(avg) = context.average_rating {
            result.push(DisplayStat {
                key: "elo_avg".to_string(),
                label: "Avg Rating".to_string(),
                value: format!("{avg:.0}"),
                category: "Rating".to_string(),
                sort_order: 3,
                color: None,
            });
        }

        if let Some(ref tier_id) = context.rank_tier {
            if let Some(tier) = self.rank_tiers().into_iter().find(|t| t.id == *tier_id) {
                result.push(DisplayStat {
                    key: "rank_tier".to_string(),
                    label: "Rank".to_string(),
                    value: tier.display_name,
                    category: "Rating".to_string(),
                    sort_order: 4,
                    color: tier.color,
                });
            }
        }

        // General stats
        result.extend([
            DisplayStat {
                key: "matches".to_string(),
                label: "Matches".to_string(),
                value: matches_played.to_string(),
                category: "General".to_string(),
                sort_order: 10,
                color: None,
            },
            DisplayStat {
                key: "win_rate".to_string(),
                label: "Win Rate".to_string(),
                value: format!("{win_rate:.1}%"),
                category: "General".to_string(),
                sort_order: 11,
                color: None,
            },
            // Combat stats
            DisplayStat {
                key: "kd_ratio".to_string(),
                label: "K/D Ratio".to_string(),
                value: format!("{kd_ratio:.2}"),
                category: "Combat".to_string(),
                sort_order: 20,
                color: None,
            },
            DisplayStat {
                key: "kills".to_string(),
                label: "Kills".to_string(),
                value: kills.to_string(),
                category: "Combat".to_string(),
                sort_order: 21,
                color: None,
            },
            DisplayStat {
                key: "deaths".to_string(),
                label: "Deaths".to_string(),
                value: deaths.to_string(),
                category: "Combat".to_string(),
                sort_order: 22,
                color: None,
            },
            DisplayStat {
                key: "assists".to_string(),
                label: "Assists".to_string(),
                value: assists.to_string(),
                category: "Combat".to_string(),
                sort_order: 23,
                color: None,
            },
            DisplayStat {
                key: "hs_percent".to_string(),
                label: "HS%".to_string(),
                value: format!("{hs_percent:.1}%"),
                category: "Combat".to_string(),
                sort_order: 24,
                color: None,
            },
            DisplayStat {
                key: "adr".to_string(),
                label: "ADR".to_string(),
                value: format!("{adr:.1}"),
                category: "Combat".to_string(),
                sort_order: 25,
                color: None,
            },
        ]);

        result
    }

    fn build_match_data_from_demo(
        &self,
        demo: &crate::types::DemoData,
    ) -> Result<MatchData, StatsError> {
        use crate::traits::resolve_demo_team_id;
        use crate::types::{MatchPlayerData, MatchTeamData};

        let winner_team_id = match demo.team1_score.cmp(&demo.team2_score) {
            std::cmp::Ordering::Greater => Some(1u32),
            std::cmp::Ordering::Less => Some(2u32),
            std::cmp::Ordering::Equal => None,
        };

        let teams = vec![
            MatchTeamData {
                team_id: 1,
                score: demo.team1_score,
                rounds_won: Some(demo.team1_score as u32),
                side_scores: None,
            },
            MatchTeamData {
                team_id: 2,
                score: demo.team2_score,
                rounds_won: Some(demo.team2_score as u32),
                side_scores: None,
            },
        ];

        let players: Vec<MatchPlayerData> = demo
            .players
            .iter()
            .filter_map(|dp| {
                let player_id = dp.player_id?;
                let team_id = resolve_demo_team_id(dp, &demo.team1_name, &demo.team2_name)?;

                // CS2-specific: remap raw demo stat keys to the keys that
                // calculate_player_stats expects. The raw stats from demo parsing
                // use "headshot_kills" and "damage", but the CS2 stats schema
                // expects "headshots" and "total_damage".
                let raw = &dp.stats;
                let game_stats = json!({
                    "kills": raw.get("kills").and_then(Value::as_i64).unwrap_or(0),
                    "deaths": raw.get("deaths").and_then(Value::as_i64).unwrap_or(0),
                    "assists": raw.get("assists").and_then(Value::as_i64).unwrap_or(0),
                    "headshots": raw.get("headshot_kills").or_else(|| raw.get("headshots")).and_then(Value::as_i64).unwrap_or(0),
                    "total_damage": raw.get("damage").or_else(|| raw.get("total_damage")).and_then(Value::as_i64).unwrap_or(0),
                    "mvps": raw.get("mvps").and_then(Value::as_i64).unwrap_or(0),
                    "clutches_won": raw.get("clutches_won").and_then(Value::as_i64).unwrap_or(0),
                    "clutches_attempted": raw.get("clutches_attempted").and_then(Value::as_i64).unwrap_or(0),
                    "opening_kills": raw.get("opening_kills").and_then(Value::as_i64).unwrap_or(0),
                    "opening_deaths": raw.get("opening_deaths").and_then(Value::as_i64).unwrap_or(0),
                    "flash_assists": raw.get("flash_assists").and_then(Value::as_i64).unwrap_or(0),
                    "utility_damage": raw.get("utility_damage").and_then(Value::as_i64).unwrap_or(0),
                });

                Some(MatchPlayerData {
                    player_id,
                    team_id,
                    game_specific_stats: game_stats,
                })
            })
            .collect();

        Ok(MatchData {
            match_id: demo.match_id,
            game_id: demo.game_id.clone(),
            map_id: demo.map_name.clone(),
            duration_seconds: demo.duration_seconds,
            players,
            teams,
            winner_team_id,
            game_specific_data: demo.raw_stats.clone(),
        })
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
        let avg_rating_1: f64 = team1.iter().map(|p| f64::from(p.rating)).sum::<f64>() / team1.len() as f64;
        let avg_rating_2: f64 = team2.iter().map(|p| f64::from(p.rating)).sum::<f64>() / team2.len() as f64;

        // Expected scores (Elo formula)
        // Using 2000 as divisor instead of 400 due to the larger CS2 Premier scale (0-35,000+)
        let expected_1 = 1.0 / (1.0 + 10.0_f64.powf((avg_rating_2 - avg_rating_1) / 2000.0));
        let expected_2 = 1.0 - expected_1;

        // Determine actual scores
        let team1_won = team1.first().is_some_and(|p| p.is_winner);
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

// ============================================================================
// TournamentPlugin Implementation
// ============================================================================

impl TournamentPlugin for Cs2Plugin {
    fn veto_formats(&self) -> Vec<VetoFormatConfig> {
        vec![
            VetoFormatConfig::bo1(),
            VetoFormatConfig::bo3(),
            VetoFormatConfig::bo5(),
        ]
    }

    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String> {
        match match_format {
            MatchFormat::Bo1 => Some("bo1_standard".to_string()),
            MatchFormat::Bo3 => Some("bo3_standard".to_string()),
            MatchFormat::Bo5 => Some("bo5_standard".to_string()),
            MatchFormat::Bo7 => None, // CS2 doesn't typically do Bo7
        }
    }

    fn get_available_sides(&self, _map_id: &str) -> Vec<SideOption> {
        // CS2 has CT and T sides for all maps
        vec![
            SideOption {
                id: "ct".to_string(),
                display_name: "Counter-Terrorist".to_string(),
                short_name: "CT".to_string(),
            },
            SideOption {
                id: "t".to_string(),
                display_name: "Terrorist".to_string(),
                short_name: "T".to_string(),
            },
        ]
    }

    fn available_side_selection_modes(&self) -> Vec<SideSelectionMode> {
        vec![
            SideSelectionMode::PickerChoice,
            SideSelectionMode::CoinFlip,
            SideSelectionMode::Knife,
        ]
    }

    fn default_side_selection_mode(&self) -> SideSelectionMode {
        SideSelectionMode::PickerChoice
    }
}

// ============================================================================
// EvidencePlugin Implementation
// ============================================================================

#[async_trait::async_trait]
impl EvidencePlugin for Cs2Plugin {
    async fn discover_evidence(
        &self,
        match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError> {
        // This would scan S3 for demo files matching the match timeframe
        // For now, return empty - actual implementation would use S3 client
        let discovered = Vec::new();

        // In a real implementation:
        // 1. List objects in S3 with prefix for this game
        // 2. Filter by timestamp (match start/end time with buffer)
        // 3. Filter by player Steam IDs from participant context
        // 4. Calculate relevance scores based on timing and player matches

        // Example of what a discovered demo would look like:
        if match_context.completed_at.is_some() {
            // Would scan for demos here
            // Placeholder: in production, this calls S3 list_objects
        }

        Ok(discovered)
    }

    async fn validate_evidence(
        &self,
        evidence_storage: &EvidenceStorage,
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, PluginError> {
        // Parse the demo file and extract the actual result
        // For CS2, we'd parse the .dem file format

        // In a real implementation:
        // 1. Download demo from storage (or use presigned URL)
        // 2. Parse demo header for basic info (map, teams, scores)
        // 3. Optionally parse full demo for detailed validation
        // 4. Compare extracted result with claimed result

        match evidence_storage {
            EvidenceStorage::S3 { bucket: _, key: _ } => {
                // Would parse the demo file here
                // For now, return validation result indicating we need real implementation

                // Placeholder validation - just check the result is valid on its own
                let is_valid_score = claimed_result.participant1_score >= 0
                    && claimed_result.participant2_score >= 0
                    && claimed_result.participant1_score != claimed_result.participant2_score;

                if !is_valid_score {
                    return Ok(EvidenceValidation {
                        is_valid: false,
                        confidence: 0.0,
                        extracted_result: None,
                        warnings: Vec::new(),
                        errors: vec!["Invalid game score claimed".to_string()],
                    });
                }

                // In production, we'd actually parse the demo
                Ok(EvidenceValidation {
                    is_valid: true,
                    confidence: 0.5, // Low confidence since we didn't actually parse
                    extracted_result: None,
                    warnings: vec![
                        "Demo parsing not fully implemented - basic validation only".to_string(),
                    ],
                    errors: Vec::new(),
                })
            }
            EvidenceStorage::Url { url: _ } => {
                // External URL - might be a VOD or something
                Ok(EvidenceValidation {
                    is_valid: true,
                    confidence: 0.0,
                    extracted_result: None,
                    warnings: vec!["Cannot validate external URL evidence automatically".to_string()],
                    errors: Vec::new(),
                })
            }
            EvidenceStorage::Inline { content: _ } => {
                Ok(EvidenceValidation {
                    is_valid: false,
                    confidence: 0.0,
                    extracted_result: None,
                    warnings: Vec::new(),
                    errors: vec!["Demo files cannot be stored inline".to_string()],
                })
            }
        }
    }

    async fn get_demo_metadata(
        &self,
        storage: &EvidenceStorage,
    ) -> Result<DemoMetadata, PluginError> {
        match storage {
            EvidenceStorage::S3 { bucket: _, key } => {
                // In a real implementation, we'd download and parse the demo header
                // CS2 demos have a header with map name, duration, player info, etc.

                // For now, try to extract map name from key if it follows naming convention
                let map_name = if key.contains("de_dust2") {
                    "de_dust2"
                } else if key.contains("de_mirage") {
                    "de_mirage"
                } else if key.contains("de_inferno") {
                    "de_inferno"
                } else if key.contains("de_nuke") {
                    "de_nuke"
                } else if key.contains("de_ancient") {
                    "de_ancient"
                } else if key.contains("de_anubis") {
                    "de_anubis"
                } else if key.contains("de_vertigo") {
                    "de_vertigo"
                } else {
                    "unknown"
                };

                // Placeholder - real implementation would parse demo header
                Ok(DemoMetadata {
                    map_name: map_name.to_string(),
                    duration_seconds: 0, // Would be extracted from demo
                    player_count: 10,    // Standard 5v5
                    team1_score: 0,
                    team2_score: 0,
                    recorded_at: Utc::now(), // Would be from demo timestamp
                    server_name: None,
                    demo_version: "cs2".to_string(),
                })
            }
            _ => Err(PluginError::NotSupported(
                "Demo metadata can only be extracted from S3 storage".to_string(),
            )),
        }
    }

    fn supports_evidence_discovery(&self) -> bool {
        true
    }

    fn supports_evidence_validation(&self) -> bool {
        true
    }

    fn supported_evidence_types(&self) -> Vec<EvidenceType> {
        vec![
            EvidenceType::Demo,
            EvidenceType::Screenshot,
            EvidenceType::Video,
        ]
    }

    fn demo_file_extension(&self) -> Option<&str> {
        Some("dem")
    }

    fn demo_storage_prefix(&self) -> Option<&str> {
        Some("demos/cs2")
    }
}

// ============================================================================
// CS2 Plugin with Enhanced Evidence Support
// ============================================================================

/// CS2 plugin with enhanced evidence support using the external demo service.
///
/// This variant fetches demo stats from `https://demos.cs210mans.uk` and
/// validates results against claimed match outcomes.
#[derive(Clone)]
pub struct Cs2PluginWithEvidence {
    /// Inner plugin (reserved for future game-specific operations).
    #[allow(dead_code)]
    inner: Cs2Plugin,
    demo_client: Arc<Cs2DemoClient>,
}

impl Default for Cs2PluginWithEvidence {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Cs2PluginWithEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cs2PluginWithEvidence")
            .field("base_url", &self.demo_client.base_url())
            .finish_non_exhaustive()
    }
}

impl Cs2PluginWithEvidence {
    /// Create a new CS2 plugin with evidence support.
    pub fn new() -> Self {
        Self {
            inner: Cs2Plugin::new(),
            demo_client: Arc::new(Cs2DemoClient::default()),
        }
    }

    /// Create with a custom demo service URL.
    pub fn with_demo_url(base_url: String) -> Self {
        Self {
            inner: Cs2Plugin::new(),
            demo_client: Arc::new(Cs2DemoClient::new(base_url)),
        }
    }

    /// Get the demo client for direct access.
    pub fn demo_client(&self) -> &Cs2DemoClient {
        &self.demo_client
    }

    /// Fetch and validate a demo against a claimed result.
    ///
    /// # Arguments
    /// * `demo_name` - Demo file name (e.g., "match_12345.dem")
    /// * `claimed_result` - The result being claimed
    /// * `team1_steam_ids` - Steam IDs for team 1
    /// * `team2_steam_ids` - Steam IDs for team 2
    pub async fn validate_demo(
        &self,
        demo_name: &str,
        claimed_result: &GameResult,
        team1_steam_ids: &[String],
        team2_steam_ids: &[String],
    ) -> Result<EvidenceValidation, PluginError> {
        // Fetch stats from external service
        let stats = self.demo_client.get_demo_stats(demo_name).await?;

        // Validate against claimed result
        let validation = Cs2EvidenceValidator::validate(
            &stats,
            claimed_result,
            team1_steam_ids,
            team2_steam_ids,
        );

        Ok(validation)
    }

    /// Fetch demo stats without validation.
    pub async fn get_demo_stats(&self, demo_name: &str) -> Result<Cs2DemoStats, PluginError> {
        self.demo_client.get_demo_stats(demo_name).await
    }

    /// Extract result from a demo without comparing to a claim.
    pub async fn extract_demo_result(
        &self,
        demo_name: &str,
        team1_steam_ids: &[String],
        team2_steam_ids: &[String],
    ) -> Result<Option<ExtractedResult>, PluginError> {
        let stats = self.demo_client.get_demo_stats(demo_name).await?;
        Ok(Cs2EvidenceValidator::extract_result(
            &stats,
            team1_steam_ids,
            team2_steam_ids,
        ))
    }

    /// Get the download URL for a demo.
    pub fn get_demo_url(&self, demo_name: &str) -> String {
        self.demo_client.get_demo_url(demo_name)
    }

    /// Get the stats URL for a demo.
    pub fn get_stats_url(&self, demo_name: &str) -> String {
        self.demo_client.get_stats_url(demo_name)
    }
}

// ============================================================================
// GamePlugin / TournamentPlugin / EvidencePlugin for Cs2PluginWithEvidence
// ============================================================================

impl GamePlugin for Cs2PluginWithEvidence {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn display_name(&self) -> &str {
        self.inner.display_name()
    }

    fn short_name(&self) -> &str {
        self.inner.short_name()
    }

    fn description(&self) -> Option<&str> {
        self.inner.description()
    }

    fn icon_url(&self) -> Option<&str> {
        self.inner.icon_url()
    }

    fn available_maps(&self) -> Vec<MapInfo> {
        self.inner.available_maps()
    }

    fn default_map_pool(&self) -> Vec<String> {
        self.inner.default_map_pool()
    }

    fn team_size_min(&self) -> u32 {
        self.inner.team_size_min()
    }

    fn team_size_max(&self) -> u32 {
        self.inner.team_size_max()
    }

    fn team_size_default(&self) -> u32 {
        self.inner.team_size_default()
    }

    fn player_stats_schema(&self) -> serde_json::Value {
        self.inner.player_stats_schema()
    }

    fn calculate_player_stats(
        &self,
        match_data: &MatchData,
        player_id: Uuid,
        existing_stats: &serde_json::Value,
    ) -> Result<serde_json::Value, StatsError> {
        self.inner
            .calculate_player_stats(match_data, player_id, existing_stats)
    }

    fn format_player_stats(&self, stats: &serde_json::Value, context: &PlayerStatsContext) -> Vec<DisplayStat> {
        self.inner.format_player_stats(stats, context)
    }

    fn build_match_data_from_demo(
        &self,
        demo: &crate::types::DemoData,
    ) -> Result<MatchData, StatsError> {
        self.inner.build_match_data_from_demo(demo)
    }

    fn rank_tiers(&self) -> Vec<RankTier> {
        self.inner.rank_tiers()
    }

    fn calculate_rating_change(
        &self,
        participants: &[RankedParticipant],
    ) -> Result<Vec<RatingChange>, RatingError> {
        self.inner.calculate_rating_change(participants)
    }

    fn matchmaking_criteria(&self) -> MatchmakingCriteria {
        self.inner.matchmaking_criteria()
    }

    fn supported_tournament_formats(&self) -> Vec<TournamentFormatId> {
        self.inner.supported_tournament_formats()
    }

    fn map_pick_ban_formats(&self) -> Vec<MapPickBanFormat> {
        self.inner.map_pick_ban_formats()
    }

    fn default_match_format(&self) -> MatchFormat {
        self.inner.default_match_format()
    }

    fn supported_match_formats(&self) -> Vec<MatchFormat> {
        self.inner.supported_match_formats()
    }

    fn as_tournament_plugin(&self) -> Option<&dyn TournamentPlugin> {
        Some(self)
    }

    fn as_evidence_plugin(&self) -> Option<&dyn EvidencePlugin> {
        Some(self)
    }
}

impl TournamentPlugin for Cs2PluginWithEvidence {
    fn veto_formats(&self) -> Vec<VetoFormatConfig> {
        self.inner.veto_formats()
    }

    fn default_veto_format(&self, match_format: MatchFormat) -> Option<String> {
        self.inner.default_veto_format(match_format)
    }

    fn get_available_sides(&self, map_id: &str) -> Vec<SideOption> {
        self.inner.get_available_sides(map_id)
    }

    fn available_side_selection_modes(&self) -> Vec<SideSelectionMode> {
        self.inner.available_side_selection_modes()
    }

    fn default_side_selection_mode(&self) -> SideSelectionMode {
        self.inner.default_side_selection_mode()
    }
}

#[async_trait::async_trait]
impl EvidencePlugin for Cs2PluginWithEvidence {
    async fn discover_evidence(
        &self,
        _match_context: &MatchContext,
    ) -> Result<Vec<DiscoveredEvidence>, PluginError> {
        // S3 scanning not yet implemented — requires bucket config, list permissions,
        // and prefix conventions. Return empty until that infrastructure is available.
        Ok(Vec::new())
    }

    async fn validate_evidence(
        &self,
        evidence_storage: &EvidenceStorage,
        claimed_result: &GameResult,
    ) -> Result<EvidenceValidation, PluginError> {
        let demo_name = match evidence_storage {
            EvidenceStorage::S3 { key, .. } => extract_demo_name(key),
            EvidenceStorage::Url { url } => extract_demo_name(url),
            EvidenceStorage::Inline { .. } => {
                return Ok(EvidenceValidation {
                    is_valid: false,
                    confidence: 0.0,
                    extracted_result: None,
                    warnings: Vec::new(),
                    errors: vec!["Demo files cannot be stored inline".to_string()],
                });
            }
        };

        let stats = self.demo_client.get_demo_stats(&demo_name).await?;

        // Validate using the existing validator (Steam IDs unavailable at this layer)
        let validation =
            Cs2EvidenceValidator::validate(&stats, claimed_result, &[], &[]);

        Ok(validation)
    }

    async fn get_demo_metadata(
        &self,
        storage: &EvidenceStorage,
    ) -> Result<DemoMetadata, PluginError> {
        let demo_name = match storage {
            EvidenceStorage::S3 { key, .. } => extract_demo_name(key),
            EvidenceStorage::Url { url } => extract_demo_name(url),
            EvidenceStorage::Inline { .. } => {
                return Err(PluginError::NotSupported(
                    "Demo metadata cannot be extracted from inline storage".to_string(),
                ));
            }
        };

        let stats = self.demo_client.get_demo_stats(&demo_name).await?;

        let team_names = stats.team_names();
        let team1_score = team_names
            .first()
            .and_then(|n| stats.score_for_team(n))
            .unwrap_or(0);
        let team2_score = team_names
            .get(1)
            .and_then(|n| stats.score_for_team(n))
            .unwrap_or(0);

        let recorded_at = chrono::NaiveDateTime::parse_from_str(
            &stats.match_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .map_or_else(|_| Utc::now(), |ndt| ndt.and_utc());

        Ok(DemoMetadata {
            map_name: stats.map.clone(),
            duration_seconds: 0, // Not available from stats JSON
            player_count: stats.all_steam_ids().len() as u32,
            team1_score,
            team2_score,
            recorded_at,
            server_name: None,
            demo_version: "cs2".to_string(),
        })
    }

    fn supports_evidence_discovery(&self) -> bool {
        false
    }

    fn supports_evidence_validation(&self) -> bool {
        true
    }

    fn supported_evidence_types(&self) -> Vec<EvidenceType> {
        vec![
            EvidenceType::Demo,
            EvidenceType::Screenshot,
            EvidenceType::Video,
        ]
    }

    fn demo_file_extension(&self) -> Option<&str> {
        Some("dem")
    }

    fn demo_storage_prefix(&self) -> Option<&str> {
        Some("demos/cs2")
    }
}

/// Extract demo filename from a path (S3 key or URL).
fn extract_demo_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
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

        let context = crate::types::PlayerStatsContext {
            rating: 0,
            peak_rating: 0,
            peak_rating_at: None,
            rank_tier: None,
            average_rating: None,
        };
        let formatted = plugin.format_player_stats(&stats, &context);
        assert!(!formatted.is_empty());

        // Check K/D ratio is calculated
        let kd = formatted.iter().find(|s| s.key == "kd_ratio").unwrap();
        assert_eq!(kd.value, "1.25");
    }
}
