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
pub mod background;
pub mod dto;
pub mod error;
pub mod extractors;
pub mod handlers;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod steam_openid;
pub mod websocket;

pub use app::create_app;
pub use background::spawn_lifecycle_task;
pub use state::{
    AdminState, AppState, AuthState, AvailabilityState, BanState, DemoState, DisputeState,
    EvidenceState, ForfeitState, GamesState, InternalState, LeagueTeamState, LeaguesState,
    PlayerState, ProgressionState, ResultReviewState, ResultState, RolesState, SteamTrackingState,
    TokenConfig, TournamentState, UploadsState, UsersState, VetoDelegatesState, VetoState,
    VetoWsState,
};
pub use websocket::spawn_timeout_warning_task;
