//! Application builder.

use crate::handlers::evidence::local_evidence_upload;
use crate::middleware::request_id_layer;
use crate::openapi::swagger_routes;
use crate::routes::api_routes;
use crate::state::AppState;
use axum::http::HeaderValue;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Build the CORS layer.
///
/// `PORTAL_CORS_ORIGINS` (comma-separated origins) configures the allow-list.
/// If unset, defaults to wildcard `*` — appropriate for local dev only.
/// **In production, always set this env var explicitly** to a finite origin
/// list; wildcard CORS combined with credentialed requests is a CSRF
/// foot-gun.
fn build_cors_layer() -> CorsLayer {
    let raw = std::env::var("PORTAL_CORS_ORIGINS").ok();
    let base = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    match raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // Explicit "*" is honored as wildcard (still a deliberate signal,
        // not a silent default).
        Some("*") => base.allow_origin(Any),
        Some(list) => {
            let origins: Vec<HeaderValue> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter_map(|o| HeaderValue::from_str(o).ok())
                .collect();
            if origins.is_empty() {
                tracing::warn!(
                    "PORTAL_CORS_ORIGINS set but no valid origins parsed — falling back to wildcard"
                );
                base.allow_origin(Any)
            } else {
                tracing::info!(?origins, "CORS origins configured from PORTAL_CORS_ORIGINS");
                base.allow_origin(origins)
            }
        }
        None => {
            tracing::warn!(
                "PORTAL_CORS_ORIGINS not set — defaulting to wildcard CORS (dev-only; set to a comma-separated origin list in production)"
            );
            base.allow_origin(Any)
        }
    }
}

/// Create the Axum application.
pub fn create_app(state: AppState) -> Router {
    let cors = build_cors_layer();

    // Uploads sub-router: PUT writes files, everything else served by ServeDir
    let uploads_router = Router::new()
        .route("/{*path}", axum::routing::put(local_evidence_upload))
        .fallback_service(ServeDir::new(&state.uploads_path));

    Router::new()
        // API routes under /v1
        .nest("/v1", api_routes())
        // Swagger UI at /swagger-ui (also serves /api-docs/openapi.json)
        .merge(swagger_routes())
        // Uploads: PUT for evidence, GET served statically
        .nest("/uploads", uploads_router)
        // Health check
        .route("/health", axum::routing::get(|| async { "OK" }))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(request_id_layer())
        .layer(cors)
        // State
        .with_state(state)
}
