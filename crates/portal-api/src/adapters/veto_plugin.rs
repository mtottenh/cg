//! Adapters that bridge plugin-provided veto/side behavior into domain service traits.

use std::sync::Arc;

use portal_core::VetoFormatConfig;
use portal_domain::services::tournament::{SideSelectionProvider, VetoFormatProvider};
use portal_plugins::PluginManager;
use rand::Rng;

/// Resolves veto format IDs by scanning all registered game plugins.
pub struct PluginVetoFormatProvider {
    plugin_manager: Arc<PluginManager>,
}

impl PluginVetoFormatProvider {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        Self { plugin_manager }
    }
}

impl VetoFormatProvider for PluginVetoFormatProvider {
    fn get_format(&self, format_id: &str) -> Option<VetoFormatConfig> {
        for plugin in self.plugin_manager.list_plugins() {
            if let Some(tp) = plugin.as_tournament_plugin() {
                if let Some(f) = tp.veto_formats().into_iter().find(|f| f.id == format_id) {
                    return Some(f);
                }
            }
        }
        None
    }
}

/// Provides game-specific random side selection by delegating to the
/// plugin's `get_available_sides()`.
pub struct PluginSideSelectionProvider {
    plugin_manager: Arc<PluginManager>,
}

impl PluginSideSelectionProvider {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        Self { plugin_manager }
    }
}

impl SideSelectionProvider for PluginSideSelectionProvider {
    fn random_side(&self, _format_id: &str) -> Option<String> {
        // Iterate all plugins and pick a random side from the first that has sides.
        // In practice there's only one plugin (CS2) registered at a time.
        for plugin in self.plugin_manager.list_plugins() {
            if let Some(tp) = plugin.as_tournament_plugin() {
                let sides = tp.get_available_sides("");
                if !sides.is_empty() {
                    let idx = rand::rng().random_range(0..sides.len());
                    return Some(sides[idx].id.clone());
                }
            }
        }
        None
    }
}
