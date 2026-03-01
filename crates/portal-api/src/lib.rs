#![allow(missing_docs)]
//! HTTP API layer for the Gaming Portal.
//!
//! This crate provides:
//! - Request/Response DTOs (separate from domain and DB types)
//! - Axum route handlers
//! - Middleware (auth, RBAC, etc.)
//! - `OpenAPI` documentation with utoipa
//!
//! ## Architecture
//!
//! The API layer uses strict type separation:
//!
//! - **Request DTOs**: Input validation via `validator`
//! - **Response DTOs**: Output formatting via `serde` and `ToSchema`
//! - **Mappers**: `From`/`TryFrom` implementations for type conversion
//!
//! All endpoints are documented with `#[utoipa::path]` attributes.

pub mod adapters;
pub mod app;
pub mod dto;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod websocket;

pub use app::create_app;
pub use state::AppState;
pub use websocket::spawn_timeout_warning_task;
