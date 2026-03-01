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
pub use traits::{EvidencePlugin, GamePlugin, MapInfo, RankTier};
pub use types::{
    DemoMetadata, DiscoveredEvidence, DisplayStat, EvidenceStorage, EvidenceType,
    EvidenceValidation, ExtractedResult, GameResult, LobbyStateMachine, MapPickBanFormat,
    MapVetoAction, MatchConfig, MatchContext, MatchData, MatchFormat, MatchPlayerData,
    MatchTeamData, MatchmakingCriteria, ParticipantContext, PlayerInfo, RankedParticipant,
    RatingChange, TournamentFormatId, VetoActionType,
};

/// Create and initialize the default plugin manager with built-in plugins.
pub fn create_default_plugin_manager() -> PluginManager {
    create_plugin_manager_with_config(None)
}

/// Create a plugin manager with optional configuration.
///
/// When `demo_base_url` is provided, the CS2 plugin is registered with
/// evidence support backed by the external demo service.
pub fn create_plugin_manager_with_config(demo_base_url: Option<String>) -> PluginManager {
    let mut manager = PluginManager::new();

    // Register CS2 plugin with evidence support
    let cs2_plugin: std::sync::Arc<dyn GamePlugin> = match demo_base_url {
        Some(url) => std::sync::Arc::new(Cs2PluginWithEvidence::with_demo_url(url)),
        None => std::sync::Arc::new(Cs2PluginWithEvidence::new()),
    };

    if let Err(e) = manager.register(cs2_plugin) {
        tracing::error!("Failed to register CS2 plugin: {}", e);
    }

    manager
}
