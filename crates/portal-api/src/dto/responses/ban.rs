//! Ban response DTOs.

use portal_domain::entities::Ban;
use portal_domain::repositories::PaginatedBans;
use serde::Serialize;
use utoipa::ToSchema;

/// Ban response DTO.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BanResponse {
    /// Unique ban identifier.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub id: String,

    /// The banned user's ID.
    #[schema(example = "550e8400-e29b-41d4-a716-446655440001")]
    pub user_id: String,

    /// Type of ban (platform, matchmaking, chat, league, tournament).
    #[schema(example = "platform")]
    pub ban_type: String,

    /// Reason for the ban.
    #[schema(example = "Cheating violation detected")]
    pub reason: String,

    /// Scope type for context-specific bans.
    #[schema(example = "league")]
    pub scope_type: Option<String>,

    /// Scope ID for context-specific bans.
    pub scope_id: Option<String>,

    /// Who issued the ban (null for system bans).
    pub issued_by: Option<String>,

    /// When the ban takes effect.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub starts_at: String,

    /// When the ban expires (null for permanent bans).
    pub ends_at: Option<String>,

    /// When the ban was lifted (null if not lifted).
    pub lifted_at: Option<String>,

    /// Who lifted the ban.
    pub lifted_by: Option<String>,

    /// Reason for lifting the ban.
    pub lift_reason: Option<String>,

    /// Whether the ban is currently active.
    pub is_active: bool,

    /// Whether this is a permanent ban.
    pub is_permanent: bool,

    /// When the ban record was created.
    pub created_at: String,

    /// When the ban record was last updated.
    pub updated_at: String,
}

impl From<Ban> for BanResponse {
    fn from(ban: Ban) -> Self {
        // Compute derived values before moving ownership
        let is_active = ban.is_active();
        let is_permanent = ban.is_permanent();

        Self {
            id: ban.id.to_string(),
            user_id: ban.user_id.to_string(),
            ban_type: ban.ban_type.to_string(),
            reason: ban.reason,
            scope_type: ban.scope_type,
            scope_id: ban.scope_id.map(|id| id.to_string()),
            issued_by: ban.issued_by.map(|id| id.to_string()),
            starts_at: ban.starts_at.to_rfc3339(),
            ends_at: ban.ends_at.map(|t| t.to_rfc3339()),
            lifted_at: ban.lifted_at.map(|t| t.to_rfc3339()),
            lifted_by: ban.lifted_by.map(|id| id.to_string()),
            lift_reason: ban.lift_reason,
            is_active,
            is_permanent,
            created_at: ban.created_at.to_rfc3339(),
            updated_at: ban.updated_at.to_rfc3339(),
        }
    }
}

/// Paginated list of bans.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BanListResponse {
    /// List of bans.
    pub items: Vec<BanResponse>,

    /// Pagination metadata.
    pub pagination: PaginationMetaResponse,
}

/// Pagination metadata.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PaginationMetaResponse {
    /// Current page (1-indexed).
    pub page: i64,

    /// Items per page.
    pub per_page: i64,

    /// Total number of items.
    pub total_items: i64,

    /// Total number of pages.
    pub total_pages: i64,
}

impl From<PaginatedBans> for BanListResponse {
    fn from(paginated: PaginatedBans) -> Self {
        Self {
            items: paginated.items.into_iter().map(BanResponse::from).collect(),
            pagination: PaginationMetaResponse {
                page: paginated.pagination.page,
                per_page: paginated.pagination.per_page,
                total_items: paginated.pagination.total_items,
                total_pages: paginated.pagination.total_pages,
            },
        }
    }
}
