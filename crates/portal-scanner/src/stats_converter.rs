//! Convert typed `Cs2DemoStats` into `SubmitStatsRequest` JSON.
//!
//! This module is the fix for the CLI scanner bug where it treated the
//! `teams` JSON object as an array, producing zero scores and empty
//! player lists. By using the typed `Cs2DemoStats` struct from
//! `portal-plugins`, we access fields via proper HashMap lookups.

use portal_plugins::Cs2DemoStats;

use crate::api_client::{SubmitPlayerEntry, SubmitStatsRequest};

/// Convert typed `Cs2DemoStats` into a `SubmitStatsRequest` for the portal API.
pub fn convert_stats(stats: &Cs2DemoStats) -> SubmitStatsRequest {
    let team_names = stats.team_names();
    let (team1_name, team2_name) = if team_names.len() >= 2 {
        (team_names[0].clone(), team_names[1].clone())
    } else {
        ("Team 1".to_string(), "Team 2".to_string())
    };

    let team1_score = stats.score_for_team(&team1_name).unwrap_or(0);
    let team2_score = stats.score_for_team(&team2_name).unwrap_or(0);

    let players: Vec<SubmitPlayerEntry> = stats
        .all_player_summaries()
        .into_iter()
        .map(|p| {
            let team_name = p.team.as_ref().map(|t| t.team_name.clone());
            SubmitPlayerEntry {
                steam_id: p.player_id.to_string(),
                player_name: p.player_name.clone(),
                team_name,
                stats: serde_json::json!({
                    "kills": p.kills,
                    "deaths": p.deaths,
                    "assists": p.assists,
                    "damage": p.damage_dealt,
                    "adr": p.adr,
                    "headshot_kills": p.headshot_kills,
                    "hs_percentage": p.hs_percentage,
                }),
            }
        })
        .collect();

    let match_date = stats.match_datetime().map(|dt| dt.to_rfc3339());

    SubmitStatsRequest {
        map_name: Some(stats.map.clone()),
        match_date,
        team1_name: Some(team1_name),
        team2_name: Some(team2_name),
        team1_score: Some(team1_score),
        team2_score: Some(team2_score),
        total_rounds: Some(stats.total_rounds()),
        duration_seconds: None,
        players,
        raw_stats: serde_json::to_value(stats).unwrap_or_default(),
    }
}
