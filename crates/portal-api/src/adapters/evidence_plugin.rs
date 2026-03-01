//! Adapter bridging `portal-plugins` evidence types to `portal-domain` evidence types.
//!
//! The domain layer defines [`EvidencePluginClient`] with domain-level types while
//! game plugins implement [`portal_plugins::EvidencePlugin`] with their own types.
//! This adapter translates between the two.

use std::sync::Arc;

use portal_core::DomainError;
use portal_domain::entities::evidence as domain;
use portal_domain::entities::result_claim::GameResult as DomainGameResult;
use portal_domain::services::tournament::EvidencePluginClient;
use portal_plugins::{self as plugin, GamePlugin};

/// Adapter wrapping an `Arc<dyn GamePlugin>` that supports evidence.
///
/// Implements the domain-level [`EvidencePluginClient`] trait by converting
/// between domain and plugin type representations.
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
        context: &domain::MatchEvidenceContext,
    ) -> Result<Vec<domain::DiscoveredEvidence>, DomainError> {
        let ep = self
            .plugin
            .as_evidence_plugin()
            .ok_or_else(|| DomainError::Internal("Plugin does not support evidence".into()))?;

        let plugin_ctx = domain_context_to_plugin(context);
        let discovered = ep
            .discover_evidence(&plugin_ctx)
            .await
            .map_err(|e| DomainError::Internal(format!("Plugin discovery error: {e}")))?;

        Ok(discovered
            .into_iter()
            .map(plugin_evidence_to_domain)
            .collect())
    }

    async fn validate_evidence(
        &self,
        evidence: &domain::Evidence,
        claimed_result: &DomainGameResult,
    ) -> Result<domain::EvidenceValidation, DomainError> {
        let ep = self
            .plugin
            .as_evidence_plugin()
            .ok_or_else(|| DomainError::Internal("Plugin does not support evidence".into()))?;

        let plugin_storage = domain_storage_to_plugin(&evidence.storage);
        let plugin_result = domain_result_to_plugin(claimed_result);

        let validation = ep
            .validate_evidence(&plugin_storage, &plugin_result)
            .await
            .map_err(|e| DomainError::Internal(format!("Plugin validation error: {e}")))?;

        Ok(plugin_validation_to_domain(validation))
    }
}

// =============================================================================
// TYPE CONVERSION: domain → plugin
// =============================================================================

fn domain_context_to_plugin(ctx: &domain::MatchEvidenceContext) -> plugin::MatchContext {
    plugin::MatchContext {
        tournament_id: ctx.tournament_id.as_uuid(),
        match_id: ctx.match_id.as_uuid(),
        game_id: ctx.game_id.to_string(),
        participants: ctx.participants.iter().map(domain_participant_to_plugin).collect(),
        scheduled_at: ctx.scheduled_at,
        started_at: ctx.started_at,
        completed_at: ctx.completed_at,
    }
}

fn domain_participant_to_plugin(p: &domain::ParticipantContext) -> plugin::ParticipantContext {
    plugin::ParticipantContext {
        registration_id: p.registration_id.as_uuid(),
        name: p.name.clone(),
        player_ids: p.player_ids.iter().map(portal_core::PlayerId::as_uuid).collect(),
        steam_ids: p.steam_ids.clone(),
    }
}

fn domain_storage_to_plugin(storage: &domain::EvidenceStorage) -> plugin::EvidenceStorage {
    match storage {
        domain::EvidenceStorage::S3 { bucket, key } => plugin::EvidenceStorage::S3 {
            bucket: bucket.clone(),
            key: key.clone(),
        },
        domain::EvidenceStorage::Url { url } => plugin::EvidenceStorage::Url { url: url.clone() },
        domain::EvidenceStorage::Inline { content } => plugin::EvidenceStorage::Inline {
            content: content.clone(),
        },
    }
}

fn domain_result_to_plugin(result: &DomainGameResult) -> plugin::GameResult {
    plugin::GameResult {
        game_number: result.game_number,
        map_id: Some(result.map_id.clone()),
        participant1_score: result.participant1_score,
        participant2_score: result.participant2_score,
    }
}

// =============================================================================
// TYPE CONVERSION: plugin → domain
// =============================================================================

fn plugin_evidence_to_domain(e: plugin::DiscoveredEvidence) -> domain::DiscoveredEvidence {
    domain::DiscoveredEvidence {
        external_id: e.external_id,
        evidence_type: plugin_evidence_type_to_domain(e.evidence_type),
        name: e.name,
        storage: plugin_storage_to_domain(e.storage),
        file_size_bytes: e.file_size_bytes,
        metadata: e.metadata,
        discovered_at: e.discovered_at,
        relevance_score: e.relevance_score,
    }
}

fn plugin_validation_to_domain(v: plugin::EvidenceValidation) -> domain::EvidenceValidation {
    domain::EvidenceValidation {
        is_valid: v.is_valid,
        confidence: v.confidence,
        extracted_result: v.extracted_result.map(|r| domain::ExtractedResult {
            map_id: r.map_id,
            participant1_score: r.participant1_score,
            participant2_score: r.participant2_score,
            duration_seconds: r.duration_seconds,
            player_stats: r.player_stats,
        }),
        warnings: v.warnings,
        errors: v.errors,
    }
}

fn plugin_storage_to_domain(s: plugin::EvidenceStorage) -> domain::EvidenceStorage {
    match s {
        plugin::EvidenceStorage::S3 { bucket, key } => {
            domain::EvidenceStorage::S3 { bucket, key }
        }
        plugin::EvidenceStorage::Url { url } => domain::EvidenceStorage::Url { url },
        plugin::EvidenceStorage::Inline { content } => {
            domain::EvidenceStorage::Inline { content }
        }
    }
}

fn plugin_evidence_type_to_domain(et: plugin::EvidenceType) -> domain::EvidenceType {
    match et {
        plugin::EvidenceType::Demo => domain::EvidenceType::Demo,
        plugin::EvidenceType::Screenshot => domain::EvidenceType::Screenshot,
        plugin::EvidenceType::Video => domain::EvidenceType::Video,
        plugin::EvidenceType::Link => domain::EvidenceType::Link,
        plugin::EvidenceType::ServerLog => domain::EvidenceType::ServerLog,
    }
}
