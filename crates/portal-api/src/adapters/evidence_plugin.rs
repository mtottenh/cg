//! Adapter bridging `portal-plugins` evidence types to `portal-domain` evidence types.
//!
//! After the portal-core type unification, both plugin and domain share the same
//! evidence types. This adapter is now a thin wrapper that just unwraps the
//! `EvidencePlugin` extension from a `GamePlugin`.

use std::collections::HashMap;
use std::sync::Arc;

use portal_core::DomainError;
use portal_core::types::evidence::{
    DiscoveredEvidenceData, EvidenceValidationResult, GameMatchResult, MatchEvidenceContext,
};
use portal_domain::entities::evidence::Evidence;
use portal_domain::entities::result_claim::GameResult as DomainGameResult;
use portal_domain::services::tournament::EvidencePluginClient;
use portal_plugins::GamePlugin;

/// Per-map catalog facts the validator needs beyond the portal map id.
#[derive(Debug, Clone, Default)]
pub struct MapValidationInfo {
    /// Engine-level name override (`None` = same as the portal id).
    pub engine_name: Option<String>,
    /// Whether the map is externally sourced (Steam Workshop) — demo
    /// headers then carry the author's in-VPK name, so a name mismatch is
    /// advisory rather than fatal.
    pub is_workshop: bool,
}

/// Adapter wrapping an `Arc<dyn GamePlugin>` that supports evidence.
///
/// Implements the domain-level [`EvidencePluginClient`] trait.
/// Since plugin and domain now share the same portal-core evidence types,
/// no type conversion is needed — this is just a thin delegation layer.
pub struct EvidencePluginAdapter {
    plugin: Arc<dyn GamePlugin>,
    /// portal map id → validation facts, from the game's map catalog.
    /// Empty (the default) preserves the legacy exact-name check.
    map_catalog: HashMap<String, MapValidationInfo>,
}

impl EvidencePluginAdapter {
    /// Create a new adapter from a plugin that supports evidence.
    ///
    /// Returns `None` if the plugin does not support the `EvidencePlugin` extension.
    pub fn new(plugin: Arc<dyn GamePlugin>) -> Option<Self> {
        if plugin.as_evidence_plugin().is_some() {
            Some(Self {
                plugin,
                map_catalog: HashMap::new(),
            })
        } else {
            None
        }
    }

    /// Attach the game's map catalog so validation can resolve engine-level
    /// map names (workshop maps) instead of comparing raw portal ids.
    #[must_use]
    pub fn with_map_catalog(mut self, map_catalog: HashMap<String, MapValidationInfo>) -> Self {
        self.map_catalog = map_catalog;
        self
    }
}

#[async_trait::async_trait]
impl EvidencePluginClient for EvidencePluginAdapter {
    async fn discover_evidence(
        &self,
        context: &MatchEvidenceContext,
    ) -> Result<Vec<DiscoveredEvidenceData>, DomainError> {
        let ep = self
            .plugin
            .as_evidence_plugin()
            .ok_or_else(|| DomainError::Internal("Plugin does not support evidence".into()))?;

        // Same types — direct pass-through
        ep.discover_evidence(context)
            .await
            .map_err(|e| DomainError::Internal(format!("Plugin discovery error: {e}")))
    }

    async fn validate_evidence(
        &self,
        evidence: &Evidence,
        claimed_result: &DomainGameResult,
    ) -> Result<EvidenceValidationResult, DomainError> {
        let ep = self
            .plugin
            .as_evidence_plugin()
            .ok_or_else(|| DomainError::Internal("Plugin does not support evidence".into()))?;

        // Convert the domain GameResult to the shared GameMatchResult.
        // An empty claimed map id means "no map claimed" — pass None so the
        // validator skips the map check instead of fatally comparing "".
        let map_id = Some(claimed_result.map_id.clone()).filter(|m| !m.is_empty());
        let map_info = map_id.as_deref().and_then(|m| self.map_catalog.get(m));
        let plugin_result = GameMatchResult {
            game_number: claimed_result.game_number,
            map_id,
            participant1_score: claimed_result.participant1_score,
            participant2_score: claimed_result.participant2_score,
            expected_map_names: map_info
                .and_then(|i| i.engine_name.clone())
                .into_iter()
                .collect(),
            map_name_advisory: map_info.is_some_and(|i| i.is_workshop),
        };

        // Storage type is now shared — direct pass-through. P-183: a game
        // with no validator now refuses instead of fabricating a pass; that
        // refusal is the caller's mistake (validating an unsupported game),
        // not an internal fault, so it maps to a 400, and crucially the
        // error path means no verdict is ever written for it.
        ep.validate_evidence(&evidence.storage, &plugin_result)
            .await
            .map_err(|e| match e {
                portal_plugins::PluginError::NotSupported(msg) => DomainError::InvalidState(msg),
                e => DomainError::Internal(format!("Plugin validation error: {e}")),
            })
    }
}
