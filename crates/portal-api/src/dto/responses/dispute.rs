//! Dispute response DTOs.

use chrono::{DateTime, Utc};
use portal_domain::entities::dispute::{
    Dispute, DisputeMessage, DisputeResolution, DisputeResolutionResult, DisputeWithThread,
};
use serde::Serialize;
use utoipa::ToSchema;

// =============================================================================
// DISPUTE RESPONSES
// =============================================================================

/// Response DTO for a dispute.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeResponse {
    /// Dispute ID.
    pub id: String,
    /// Match ID.
    pub match_id: String,
    /// Result claim ID (if disputing a specific claim).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_claim_id: Option<String>,

    /// Registration ID of who raised the dispute.
    pub disputed_by_registration_id: String,
    /// User ID of who raised the dispute.
    pub disputed_by_user_id: String,

    /// Reason for the dispute.
    pub reason: String,
    /// Detailed description.
    pub description: String,
    /// Evidence IDs.
    pub evidence_ids: Vec<String>,

    /// Original winner (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_winner_registration_id: Option<String>,
    /// Original participant 1 score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_participant1_score: Option<i32>,
    /// Original participant 2 score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_participant2_score: Option<i32>,

    /// Current status.
    pub status: String,
    /// Priority level.
    pub priority: String,

    /// When resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    /// Who resolved it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_user_id: Option<String>,
    /// Resolution details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<DisputeResolutionResponse>,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<Dispute> for DisputeResponse {
    fn from(d: Dispute) -> Self {
        Self {
            id: d.id.to_string(),
            match_id: d.match_id.to_string(),
            result_claim_id: d.result_claim_id.map(|id| id.to_string()),
            disputed_by_registration_id: d.disputed_by_registration_id.to_string(),
            disputed_by_user_id: d.disputed_by_user_id.to_string(),
            reason: d.reason.to_string(),
            description: d.description,
            evidence_ids: d
                .evidence_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            original_winner_registration_id: d
                .original_winner_registration_id
                .map(|id| id.to_string()),
            original_participant1_score: d.original_participant1_score,
            original_participant2_score: d.original_participant2_score,
            status: d.status.to_string(),
            priority: d.priority.to_string(),
            resolved_at: d.resolved_at,
            resolved_by_user_id: d.resolved_by_user_id.map(|id| id.to_string()),
            resolution: d.resolution.map(Into::into),
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

/// Response DTO for dispute resolution details.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeResolutionResponse {
    /// Type of resolution.
    pub resolution_type: String,
    /// Admin notes.
    pub notes: String,
    /// New winner (if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_winner_registration_id: Option<String>,
    /// New participant 1 score (if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_participant1_score: Option<i32>,
    /// New participant 2 score (if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_participant2_score: Option<i32>,
}

impl From<DisputeResolution> for DisputeResolutionResponse {
    fn from(r: DisputeResolution) -> Self {
        Self {
            resolution_type: r.resolution_type.to_string(),
            notes: r.notes,
            new_winner_registration_id: r.new_winner_registration_id.map(|id| id.to_string()),
            new_participant1_score: r.new_participant1_score,
            new_participant2_score: r.new_participant2_score,
        }
    }
}

/// Response DTO for a dispute message.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeMessageResponse {
    /// Message ID.
    pub id: String,
    /// Dispute ID.
    pub dispute_id: String,
    /// Author user ID.
    pub author_user_id: String,
    /// Type of author.
    pub author_type: String,
    /// Message content.
    pub message: String,
    /// Evidence IDs.
    pub evidence_ids: Vec<String>,
    /// Whether this is an internal admin note.
    pub is_internal: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl From<DisputeMessage> for DisputeMessageResponse {
    fn from(m: DisputeMessage) -> Self {
        Self {
            id: m.id.to_string(),
            dispute_id: m.dispute_id.to_string(),
            author_user_id: m.author_user_id.to_string(),
            author_type: m.author_type.to_string(),
            message: m.message,
            evidence_ids: m
                .evidence_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            is_internal: m.is_internal,
            created_at: m.created_at,
        }
    }
}

/// Response for a dispute with its message thread.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeWithThreadResponse {
    /// The dispute.
    pub dispute: DisputeResponse,
    /// Message thread.
    pub messages: Vec<DisputeMessageResponse>,
}

impl From<DisputeWithThread> for DisputeWithThreadResponse {
    fn from(d: DisputeWithThread) -> Self {
        Self {
            dispute: DisputeResponse::from(d.dispute),
            messages: d.messages.into_iter().map(Into::into).collect(),
        }
    }
}

/// Response after resolving a dispute.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeResolutionResultResponse {
    /// The resolved dispute.
    pub dispute: DisputeResponse,
    /// Whether bracket progression was affected.
    pub progression_affected: bool,
    /// Matches that had progression reverted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reverted_matches: Vec<String>,
    /// Matches that were updated.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub updated_matches: Vec<String>,
}

impl From<DisputeResolutionResult> for DisputeResolutionResultResponse {
    fn from(r: DisputeResolutionResult) -> Self {
        let progression_affected = r.progression_changes.is_some();
        let (reverted_matches, updated_matches) = r
            .progression_changes
            .map(|c| {
                (
                    c.reverted_matches
                        .into_iter()
                        .map(|id| id.to_string())
                        .collect(),
                    c.updated_matches
                        .into_iter()
                        .map(|id| id.to_string())
                        .collect(),
                )
            })
            .unwrap_or_default();

        Self {
            dispute: DisputeResponse::from(r.dispute),
            progression_affected,
            reverted_matches,
            updated_matches,
        }
    }
}

/// Paginated list of disputes.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisputeListResponse {
    /// List of disputes.
    pub disputes: Vec<DisputeResponse>,
    /// Total count.
    pub total: u64,
    /// Current page.
    pub page: u32,
    /// Page size.
    pub page_size: u32,
}
