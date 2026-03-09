//! Application builder.

use crate::handlers::evidence::local_evidence_upload;
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
