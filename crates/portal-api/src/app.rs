//! Application builder.

use crate::middleware::request_id_layer;
use crate::openapi::swagger_routes;
use crate::routes::api_routes;
use crate::state::AppState;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Create the Axum application.
pub fn create_app(state: AppState) -> Router {
    // CORS configuration - permissive for development
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    Router::new()
        // API routes under /v1
        .nest("/v1", api_routes())
        // Swagger UI at /swagger-ui (also serves /api-docs/openapi.json)
        .merge(swagger_routes())
        // Serve uploaded files (avatars, banners, etc.)
        .nest_service("/uploads", ServeDir::new(&state.uploads_path))
        // Health check
        .route("/health", axum::routing::get(|| async { "OK" }))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(request_id_layer())
        .layer(cors)
        // State
        .with_state(state)
}
