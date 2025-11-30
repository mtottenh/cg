//! Pagination types for list endpoints.

use serde::{Deserialize, Serialize};

/// Default number of items per page.
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// Maximum number of items per page.
pub const MAX_PAGE_SIZE: u32 = 100;

/// Pagination request parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PageRequest {
    /// Page number (1-indexed).
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page.
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

const fn default_page() -> u32 {
    1
}

const fn default_per_page() -> u32 {
    DEFAULT_PAGE_SIZE
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: DEFAULT_PAGE_SIZE,
        }
    }
}

impl PageRequest {
    /// Create a new page request.
    #[must_use]
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, MAX_PAGE_SIZE),
        }
    }

    /// Calculate the offset for database queries.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }

    /// Get the limit for database queries.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.per_page
    }

    /// Normalize the request (ensure valid values).
    #[must_use]
    pub fn normalize(self) -> Self {
        Self::new(self.page, self.per_page)
    }
}

/// Pagination metadata for responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    /// Current page number (1-indexed).
    pub page: u32,

    /// Number of items per page.
    pub per_page: u32,

    /// Total number of items across all pages.
    pub total_items: u64,

    /// Total number of pages.
    pub total_pages: u32,
}

impl Pagination {
    /// Create pagination metadata from a page request and total count.
    #[must_use]
    pub fn new(request: PageRequest, total_items: u64) -> Self {
        let total_pages = if total_items == 0 {
            1
        } else {
            ((total_items as f64) / f64::from(request.per_page)).ceil() as u32
        };

        Self {
            page: request.page,
            per_page: request.per_page,
            total_items,
            total_pages,
        }
    }

    /// Check if there is a next page.
    #[must_use]
    pub const fn has_next(&self) -> bool {
        self.page < self.total_pages
    }

    /// Check if there is a previous page.
    #[must_use]
    pub const fn has_prev(&self) -> bool {
        self.page > 1
    }
}

/// A page of items with pagination metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// The items on this page.
    pub data: Vec<T>,

    /// Pagination metadata.
    pub pagination: Pagination,
}

impl<T> Page<T> {
    /// Create a new page.
    #[must_use]
    pub fn new(data: Vec<T>, request: PageRequest, total_items: u64) -> Self {
        Self {
            data,
            pagination: Pagination::new(request, total_items),
        }
    }

    /// Create an empty page.
    #[must_use]
    pub fn empty(request: PageRequest) -> Self {
        Self::new(Vec::new(), request, 0)
    }

    /// Map the items using a function.
    pub fn map<U, F>(self, f: F) -> Page<U>
    where
        F: FnMut(T) -> U,
    {
        Page {
            data: self.data.into_iter().map(f).collect(),
            pagination: self.pagination,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_request_offset() {
        assert_eq!(PageRequest::new(1, 20).offset(), 0);
        assert_eq!(PageRequest::new(2, 20).offset(), 20);
        assert_eq!(PageRequest::new(3, 10).offset(), 20);
    }

    #[test]
    fn test_page_request_normalize() {
        let req = PageRequest { page: 0, per_page: 200 };
        let normalized = req.normalize();
        assert_eq!(normalized.page, 1);
        assert_eq!(normalized.per_page, MAX_PAGE_SIZE);
    }

    #[test]
    fn test_pagination_calculation() {
        let pagination = Pagination::new(PageRequest::new(1, 20), 55);
        assert_eq!(pagination.total_pages, 3);
        assert!(pagination.has_next());
        assert!(!pagination.has_prev());

        let pagination = Pagination::new(PageRequest::new(3, 20), 55);
        assert!(!pagination.has_next());
        assert!(pagination.has_prev());
    }
}
