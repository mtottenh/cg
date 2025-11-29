//! Common DTO types used across endpoints.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Pagination query parameters.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct PaginationParams {
    /// Page number (1-indexed).
    #[param(default = 1, minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page.
    #[param(default = 20, minimum = 1, maximum = 100)]
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl PaginationParams {
    /// Calculate the offset for database queries.
    #[must_use]
    pub fn offset(&self) -> i64 {
        i64::from((self.page.saturating_sub(1)) * self.per_page)
    }

    /// Get the limit for database queries.
    #[must_use]
    pub fn limit(&self) -> i64 {
        i64::from(self.per_page.min(100))
    }
}

/// Pagination metadata in responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PaginationMeta {
    /// Current page number.
    #[schema(example = 1)]
    pub page: u32,

    /// Items per page.
    #[schema(example = 20)]
    pub per_page: u32,

    /// Total number of items.
    #[schema(example = 150)]
    pub total_items: u64,

    /// Total number of pages.
    #[schema(example = 8)]
    pub total_pages: u32,
}

impl PaginationMeta {
    /// Create pagination metadata.
    #[must_use]
    pub fn new(params: &PaginationParams, total_items: u64) -> Self {
        let total_pages = if total_items == 0 {
            1
        } else {
            ((total_items as f64) / (params.per_page as f64)).ceil() as u32
        };

        Self {
            page: params.page,
            per_page: params.per_page,
            total_items,
            total_pages,
        }
    }
}

/// Response metadata.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Meta {
    /// Unique request identifier.
    #[schema(example = "req_abc123")]
    pub request_id: String,

    /// Response timestamp.
    #[schema(example = "2024-01-15T10:30:00Z")]
    pub timestamp: String,
}

impl Meta {
    /// Create new metadata with the given request ID.
    #[must_use]
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Wrapper for single-item responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DataResponse<T: ToSchema> {
    /// The response data.
    pub data: T,

    /// Response metadata.
    pub meta: Meta,
}

impl<T: ToSchema> DataResponse<T> {
    /// Create a new data response.
    #[must_use]
    pub fn new(data: T, request_id: impl Into<String>) -> Self {
        Self {
            data,
            meta: Meta::new(request_id),
        }
    }
}

/// Wrapper for paginated list responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PaginatedResponse<T: ToSchema> {
    /// The list of items.
    pub data: Vec<T>,

    /// Pagination metadata.
    pub pagination: PaginationMeta,

    /// Response metadata.
    pub meta: Meta,
}

impl<T: ToSchema> PaginatedResponse<T> {
    /// Create a new paginated response.
    #[must_use]
    pub fn new(
        data: Vec<T>,
        params: &PaginationParams,
        total_items: u64,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            data,
            pagination: PaginationMeta::new(params, total_items),
            meta: Meta::new(request_id),
        }
    }
}
