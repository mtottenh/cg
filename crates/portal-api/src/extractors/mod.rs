//! Custom Axum extractors.

pub mod api_key;
mod auth;
mod permission;
mod validated;

pub use api_key::AuthenticatedService;
pub use auth::{AuthenticatedUser, OptionalAuthenticatedUser};
pub use permission::PermissionChecker;
pub use validated::ValidatedJson;
