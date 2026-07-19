//! Plugin manager for registering and accessing game plugins.

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::error::PluginError;
use crate::traits::GamePlugin;

/// Manager for game plugins.
///
/// The plugin manager maintains a registry of game plugins and provides
/// access to them by their game ID.
#[derive(Default)]
pub struct PluginManager {
    plugins: HashMap<String, Arc<dyn GamePlugin>>,
}

impl PluginManager {
    /// Create a new empty plugin manager.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a game plugin.
    ///
    /// Returns an error if a plugin with the same ID is already registered.
    pub fn register(&mut self, plugin: Arc<dyn GamePlugin>) -> Result<(), PluginError> {
        let id = plugin.id().to_string();

        if self.plugins.contains_key(&id) {
            return Err(PluginError::AlreadyRegistered(id));
        }

        info!(
            plugin_id = %id,
            display_name = %plugin.display_name(),
            "Registering game plugin"
        );

        self.plugins.insert(id, plugin);
        Ok(())
    }

    /// Get a plugin by game ID.
    pub fn get(&self, game_id: &str) -> Option<Arc<dyn GamePlugin>> {
        self.plugins.get(game_id).cloned()
    }

    /// Get a plugin by game ID, returning an error if not found.
    pub fn get_or_error(&self, game_id: &str) -> Result<Arc<dyn GamePlugin>, PluginError> {
        self.get(game_id)
            .ok_or_else(|| PluginError::NotFound(game_id.to_string()))
    }

    /// List all registered plugin IDs.
    pub fn list(&self) -> Vec<&str> {
        self.plugins
            .keys()
            .map(std::string::String::as_str)
            .collect()
    }

    /// List all registered plugins.
    pub fn list_plugins(&self) -> Vec<Arc<dyn GamePlugin>> {
        self.plugins.values().cloned().collect()
    }

    /// Get the number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Check if a plugin is registered.
    pub fn has(&self, game_id: &str) -> bool {
        self.plugins.contains_key(game_id)
    }

    /// Unregister a plugin by game ID.
    ///
    /// Returns an error if the plugin is not found.
    pub fn unregister(&mut self, game_id: &str) -> Result<Arc<dyn GamePlugin>, PluginError> {
        match self.plugins.remove(game_id) {
            Some(plugin) => {
                warn!(
                    plugin_id = %game_id,
                    "Unregistered game plugin"
                );
                Ok(plugin)
            }
            None => Err(PluginError::NotFound(game_id.to_string())),
        }
    }

    /// Get a plugin that supports evidence, by game ID.
    ///
    /// Returns `Some` only if the plugin is registered and supports
    /// the `EvidencePlugin` extension (i.e. `as_evidence_plugin()` returns `Some`).
    pub fn get_evidence_plugin(&self, game_id: &str) -> Option<Arc<dyn GamePlugin>> {
        self.get(game_id)
            .filter(|p| p.as_evidence_plugin().is_some())
    }

    /// Clear all registered plugins.
    pub fn clear(&mut self) {
        debug!("Clearing all game plugins");
        self.plugins.clear();
    }
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &self.list())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StatsError;
    use crate::traits::{MapInfo, RankTier};
    use crate::types::*;
    use serde_json::{Value, json};

    /// A simple test plugin for testing the manager.
    struct TestPlugin {
        id: String,
        name: String,
    }

    impl TestPlugin {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
            }
        }
    }

    impl GamePlugin for TestPlugin {
        fn id(&self) -> &str {
            &self.id
        }

        fn display_name(&self) -> &str {
            &self.name
        }

        fn available_maps(&self) -> Vec<MapInfo> {
            vec![]
        }

        fn default_map_pool(&self) -> Vec<String> {
            vec![]
        }

        fn player_stats_schema(&self) -> Value {
            json!({})
        }

        fn calculate_player_stats(
            &self,
            _match_data: &MatchData,
            _player_id: uuid::Uuid,
            _existing_stats: &Value,
        ) -> Result<Value, StatsError> {
            Ok(json!({}))
        }

        fn format_player_stats(
            &self,
            _stats: &Value,
            _context: &PlayerStatsContext,
        ) -> Vec<DisplayStat> {
            vec![]
        }

        fn rank_tiers(&self) -> Vec<RankTier> {
            vec![]
        }

        fn calculate_rating_change(
            &self,
            _participants: &[RankedParticipant],
        ) -> Result<Vec<RatingChange>, crate::error::RatingError> {
            Ok(vec![])
        }

        fn map_pick_ban_formats(&self) -> Vec<MapPickBanFormat> {
            vec![]
        }

        fn team_size_min(&self) -> u32 {
            1
        }

        fn team_size_max(&self) -> u32 {
            5
        }

        fn team_size_default(&self) -> u32 {
            5
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut manager = PluginManager::new();
        let plugin = Arc::new(TestPlugin::new("test", "Test Game"));

        assert!(manager.register(plugin.clone()).is_ok());
        assert!(manager.has("test"));

        let retrieved = manager.get("test").unwrap();
        assert_eq!(retrieved.id(), "test");
        assert_eq!(retrieved.display_name(), "Test Game");
    }

    #[test]
    fn test_duplicate_registration() {
        let mut manager = PluginManager::new();
        let plugin1 = Arc::new(TestPlugin::new("test", "Test Game 1"));
        let plugin2 = Arc::new(TestPlugin::new("test", "Test Game 2"));

        assert!(manager.register(plugin1).is_ok());
        let result = manager.register(plugin2);

        assert!(matches!(result, Err(PluginError::AlreadyRegistered(_))));
    }

    #[test]
    fn test_get_nonexistent() {
        let manager = PluginManager::new();

        assert!(manager.get("nonexistent").is_none());
        assert!(matches!(
            manager.get_or_error("nonexistent"),
            Err(PluginError::NotFound(_))
        ));
    }

    #[test]
    fn test_list_plugins() {
        let mut manager = PluginManager::new();
        manager
            .register(Arc::new(TestPlugin::new("game1", "Game 1")))
            .unwrap();
        manager
            .register(Arc::new(TestPlugin::new("game2", "Game 2")))
            .unwrap();

        let list = manager.list();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"game1"));
        assert!(list.contains(&"game2"));
    }

    #[test]
    fn test_unregister() {
        let mut manager = PluginManager::new();
        manager
            .register(Arc::new(TestPlugin::new("test", "Test")))
            .unwrap();

        assert!(manager.has("test"));
        assert!(manager.unregister("test").is_ok());
        assert!(!manager.has("test"));

        // Unregistering again should fail
        assert!(matches!(
            manager.unregister("test"),
            Err(PluginError::NotFound(_))
        ));
    }

    #[test]
    fn test_clear() {
        let mut manager = PluginManager::new();
        manager
            .register(Arc::new(TestPlugin::new("game1", "Game 1")))
            .unwrap();
        manager
            .register(Arc::new(TestPlugin::new("game2", "Game 2")))
            .unwrap();

        assert_eq!(manager.count(), 2);
        manager.clear();
        assert_eq!(manager.count(), 0);
    }
}
