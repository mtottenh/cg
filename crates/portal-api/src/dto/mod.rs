//! Data Transfer Objects for API requests and responses.
//!
//! DTOs are separate from domain entities and database rows.
//! They handle serialization, validation, and OpenAPI schema generation.

pub mod common;
pub mod requests;
pub mod responses;

pub use common::{Meta, PaginatedResponse, PaginationParams};
