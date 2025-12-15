#![allow(missing_docs)]
//! Plugin system for game-specific logic.
//!
//! Game plugins provide:
//! - Map definitions and pools
//! - Stats schemas and calculations
//! - Matchmaking criteria
//! - Lobby state machines
//! - Ranking calculations
//! - Tournament format support
//! - Evidence discovery and validation

pub mod error;
pub mod games;
pub mod manager;
pub mod traits;
pub mod types;

// Re-export main types for convenience
pub use error::{PluginError, RatingError, StatsError};
pub use games::{
    Cs2DemoClient, Cs2DemoStats, Cs2EvidenceValidator, Cs2Plugin, Cs2PluginWithEvidence,
};
pub use manager::PluginManager;
pub use traits::{GamePlugin, MapInfo, RankTier};
pub use types::{
    DisplayStat, EvidenceType, EvidenceValidation, ExtractedResult, GameResult,
    LobbyStateMachine, MapPickBanFormat, MapVetoAction, MatchConfig, MatchContext, MatchData,
    MatchFormat, MatchPlayerData, MatchTeamData, MatchmakingCriteria, PlayerInfo,
    RankedParticipant, RatingChange, TournamentFormatId, VetoActionType,
};

/// Create and initialize the default plugin manager with built-in plugins.
pub fn create_default_plugin_manager() -> PluginManager {
    let mut manager = PluginManager::new();

    // Register built-in plugins
    if let Err(e) = manager.register(std::sync::Arc::new(Cs2Plugin::new())) {
        tracing::error!("Failed to register CS2 plugin: {}", e);
    }

    manager
}
