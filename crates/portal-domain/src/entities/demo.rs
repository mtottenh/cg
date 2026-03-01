//! Demo domain entities.
//!
//! Demos are game replay files that exist independently of matches.
//! They can be browsed, categorized, and optionally linked to tournament matches.

use chrono::{DateTime, Utc};
use portal_core::{
    DemoCategory, DemoId, DemoLinkType, DemoMatchLinkId, DemoPlayerId, DemoStatus, GameId, LeagueId,
    PlayerId, TournamentId, TournamentMatchId, UserId,
};
use serde::{Deserialize, Serialize};

// =============================================================================
// DEMO
// =============================================================================

/// A demo file in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Demo {
    pub id: DemoId,
    pub game_id: GameId,
    pub file_name: String,

    // S3 storage
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,

    // Categorization
    pub category: DemoCategory,
    pub is_hidden: bool,

    // Optional organization linkage
    pub league_id: Option<LeagueId>,
    pub tournament_id: Option<TournamentId>,

    // Parsed metadata (from stats)
    pub metadata: Option<ParsedDemoMetadata>,

    // Full stats JSON
    pub stats_json: Option<serde_json::Value>,

    // Processing status
    pub status: DemoStatus,
    pub stats_fetched_at: Option<DateTime<Utc>>,
    pub stats_fetch_error: Option<String>,

    // Admin actions
    pub categorized_by_user_id: Option<UserId>,
    pub categorized_at: Option<DateTime<Utc>>,
    pub hidden_by_user_id: Option<UserId>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub admin_notes: Option<String>,

    // Timestamps
    pub discovered_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Demo {
    /// Check if stats are available for this demo.
    #[must_use]
    pub fn has_stats(&self) -> bool {
        self.status.has_stats() && self.metadata.is_some()
    }

    /// Check if this demo needs stats processing.
    #[must_use]
    pub fn needs_processing(&self) -> bool {
        self.status.needs_processing()
    }

    /// Check if this demo is visible in public browsing.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        !self.is_hidden && self.category.is_visible()
    }

    /// Get the winner team name if stats are available.
    #[must_use]
    pub fn winner_team(&self) -> Option<&str> {
        self.metadata.as_ref().and_then(|m| {
            match m.team1_score.cmp(&m.team2_score) {
                std::cmp::Ordering::Greater => Some(m.team1_name.as_str()),
                std::cmp::Ordering::Less => Some(m.team2_name.as_str()),
                std::cmp::Ordering::Equal => None, // Draw
            }
        })
    }

    /// Get S3 URL for this demo (for download).
    #[must_use]
    pub fn s3_url(&self, base_url: &str) -> String {
        format!("{}/{}/{}", base_url, self.s3_bucket, self.s3_key)
    }
}

/// Parsed metadata extracted from demo stats for the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDemoMetadata {
    pub map_name: String,
    pub match_date: Option<DateTime<Utc>>,
    pub team1_name: String,
    pub team2_name: String,
    pub team1_score: i32,
    pub team2_score: i32,
    pub total_rounds: i32,
    pub duration_seconds: Option<i64>,
}

// =============================================================================
// DEMO-MATCH LINK
// =============================================================================

/// Link between a demo and a tournament match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoMatchLink {
    pub id: DemoMatchLinkId,
    pub demo_id: DemoId,
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,

    pub link_type: DemoLinkType,
    pub confidence_score: Option<f32>,

    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub validation_result: Option<serde_json::Value>,

    pub linked_by_user_id: Option<UserId>,
    pub linked_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// DEMO PLAYER
// =============================================================================

/// A player's appearance and stats in a demo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoPlayer {
    pub id: DemoPlayerId,
    pub demo_id: DemoId,

    // Player identification
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,

    // Optional link to portal player
    pub player_id: Option<PlayerId>,

    // Stats
    pub stats: DemoPlayerStats,

    pub created_at: DateTime<Utc>,
}

/// Player statistics from a demo.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DemoPlayerStats {
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub damage: i32,
    pub adr: f64,
    pub headshot_kills: i32,
    pub hs_percentage: f64,
}

impl DemoPlayerStats {
    /// Calculate K/D ratio.
    #[must_use]
    pub fn kd_ratio(&self) -> f64 {
        if self.deaths == 0 {
            f64::from(self.kills)
        } else {
            f64::from(self.kills) / f64::from(self.deaths)
        }
    }
}

// =============================================================================
// COMMANDS
// =============================================================================

/// Command to create a demo catalog entry.
#[derive(Debug, Clone)]
pub struct CreateDemoCommand {
    pub game_id: GameId,
    pub file_name: String,
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,
}

/// Command to update demo metadata after stats fetch.
#[derive(Debug, Clone)]
pub struct UpdateDemoStatsCommand {
    pub demo_id: DemoId,
    pub metadata: ParsedDemoMetadata,
    pub stats_json: serde_json::Value,
    pub players: Vec<CreateDemoPlayerCommand>,
}

/// Command to create a demo player entry.
#[derive(Debug, Clone)]
pub struct CreateDemoPlayerCommand {
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub stats: DemoPlayerStats,
}

/// Command to categorize a demo.
#[derive(Debug, Clone)]
pub struct CategorizeDemoCommand {
    pub demo_id: DemoId,
    pub category: DemoCategory,
    pub by_user_id: UserId,
}

/// Command to hide/unhide a demo.
#[derive(Debug, Clone)]
pub struct SetDemoVisibilityCommand {
    pub demo_id: DemoId,
    pub is_hidden: bool,
    pub by_user_id: UserId,
}

/// Command to associate a demo with a league/tournament.
#[derive(Debug, Clone)]
pub struct AssociateDemoCommand {
    pub demo_id: DemoId,
    pub league_id: Option<LeagueId>,
    pub tournament_id: Option<TournamentId>,
}

/// Command to link a demo to a match.
#[derive(Debug, Clone)]
pub struct LinkDemoToMatchCommand {
    pub demo_id: DemoId,
    pub match_id: TournamentMatchId,
    pub game_number: Option<i32>,
    pub link_type: DemoLinkType,
    pub by_user_id: UserId,
}

/// Command to unlink a demo from a match.
#[derive(Debug, Clone)]
pub struct UnlinkDemoFromMatchCommand {
    pub demo_id: DemoId,
    pub match_id: TournamentMatchId,
}

// =============================================================================
// FILTERS
// =============================================================================

/// Filter for listing demos.
#[derive(Debug, Clone, Default)]
pub struct DemoFilter {
    pub game_id: Option<GameId>,
    pub category: Option<DemoCategory>,
    pub status: Option<DemoStatus>,
    pub league_id: Option<LeagueId>,
    pub tournament_id: Option<TournamentId>,
    pub map_name: Option<String>,
    pub team_name_contains: Option<String>,
    pub steam_id: Option<String>,
    pub match_date_from: Option<DateTime<Utc>>,
    pub match_date_to: Option<DateTime<Utc>>,
    pub include_hidden: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Result of listing demos with total count.
#[derive(Debug, Clone)]
pub struct DemoListResult {
    pub demos: Vec<Demo>,
    pub total: i64,
}
