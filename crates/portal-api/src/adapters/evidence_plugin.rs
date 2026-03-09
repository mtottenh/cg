//! Adapter bridging `portal-plugins` evidence types to `portal-domain` evidence types.
//!
//! After the portal-core type unification, both plugin and domain share the same
//! evidence types. This adapter is now a thin wrapper that just unwraps the
//! `EvidencePlugin` extension from a `GamePlugin`.

use std::sync::Arc;

use portal_core::DomainError;
use portal_core::types::evidence::{
    DiscoveredEvidenceData, EvidenceValidationResult, GameMatchResult, MatchEvidenceContext,
};
use portal_domain::entities::evidence::Evidence;
use portal_domain::entities::result_claim::GameResult as DomainGameResult;
use portal_domain::services::tournament::EvidencePluginClient;
use portal_plugins::GamePlugin;

/// Adapter wrapping an `Arc<dyn GamePlugin>` that supports evidence.
///
/// Implements the domain-level [`EvidencePluginClient`] trait.
/// Since plugin and domain now share the same portal-core evidence types,
/// no type conversion is needed — this is just a thin delegation layer.
pub struct EvidencePluginAdapter {
    plugin: Arc<dyn GamePlugin>,
}

impl EvidencePluginAdapter {
    /// Create a new adapter from a plugin that supports evidence.
    ///
    /// Returns `None` if the plugin does not support the `EvidencePlugin` extension.
    pub fn new(plugin: Arc<dyn GamePlugin>) -> Option<Self> {
        if plugin.as_evidence_plugin().is_some() {
            Some(Self { plugin })
        } else {
            None
        }
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

        // Convert the domain GameResult to the shared GameMatchResult
        let plugin_result = GameMatchResult {
            game_number: claimed_result.game_number,
            map_id: Some(claimed_result.map_id.clone()),
            participant1_score: claimed_result.participant1_score,
            participant2_score: claimed_result.participant2_score,
        };

        // Storage type is now shared — direct pass-through
        ep.validate_evidence(&evidence.storage, &plugin_result)
            .await
            .map_err(|e| DomainError::Internal(format!("Plugin validation error: {e}")))
    }
}
