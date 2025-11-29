//! Custom Axum extractors.

mod auth;
mod permission;
mod validated;

pub use auth::{AuthenticatedUser, OptionalAuthenticatedUser};
pub use permission::PermissionChecker;
pub use validated::ValidatedJson;
