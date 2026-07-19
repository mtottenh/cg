//! Application builder.

use crate::handlers::evidence::local_evidence_upload;
use crate::middleware::{REQUEST_ID_HEADER, request_id_middleware};
use crate::openapi::swagger_routes;
use crate::routes::api_routes;
use crate::state::AppState;
use axum::Router;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::{HeaderValue, Request, header};
use axum::middleware;
use axum::response::IntoResponse;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info_span;

/// Liveness + database probe. A static "OK" cannot distinguish a healthy
/// server from one whose database vanished, so this pings the pool; 503
/// on failure lets systemd/Caddy/orchestrators act on it.
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db_pool).await {
        Ok(_) => (StatusCode::OK, "OK"),
        Err(e) => {
            tracing::error!(error = %e, "health check: database unreachable");
            (StatusCode::SERVICE_UNAVAILABLE, "database unreachable")
        }
    }
}

/// Readiness with dependency detail. The demo-stats service being down is
/// reported as degraded but does NOT fail readiness — the portal serves
/// everything except CS2 stats without it, and taking the whole API out of
/// rotation over an optional dependency would be worse than degrading.
async fn health_ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db_pool)
        .await
        .is_ok();

    let demo_service = match state.cs2_demo_base_url.as_deref() {
        None => "unconfigured",
        Some(base) => {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build();
            match client {
                // Any HTTP response (even 404) proves reachability.
                Ok(c) => match c.get(format!("{base}/health")).send().await {
                    Ok(_) => "ok",
                    Err(_) => "unreachable",
                },
                Err(_) => "unreachable",
            }
        }
    };

    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        axum::Json(serde_json::json!({
            "status": if db_ok { "ok" } else { "unavailable" },
            "db": if db_ok { "ok" } else { "unreachable" },
            "demo_service": demo_service,
        })),
    )
}

/// Global body-size cap for ordinary API requests (JSON, small forms).
///
/// Multipart upload handlers run their own per-image limits (`ImageConfig`)
/// and the local evidence PUT handler enforces its own 64 MiB cap, so this
/// only needs to bound JSON payloads. 16 MiB is comfortably above any
/// legitimate API request and small enough that a malformed body can't be
/// used to exhaust memory.
const DEFAULT_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Body cap for the local evidence PUT route. Matches the per-handler limit
/// in [`crate::handlers::evidence::local_evidence_upload`].
const LOCAL_UPLOADS_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// Build the CORS layer.
///
/// `PORTAL_CORS_ORIGINS` (comma-separated origins) configures the allow-list.
/// If unset, defaults to wildcard `*` — appropriate for local dev only.
/// **In production, always set this env var explicitly** to a finite origin
/// list; wildcard CORS combined with credentialed requests is a CSRF
/// foot-gun.
fn build_cors_layer() -> CorsLayer {
    let raw = std::env::var("PORTAL_CORS_ORIGINS").ok();
    // Enumerate allowed headers explicitly rather than using `Any` (wildcard).
    // Per the CORS spec, a wildcard `Access-Control-Allow-Headers: *` does
    // NOT match the `Authorization` header — it's special-cased and must be
    // listed by name. Browsers (Firefox first) already emit a warning and
    // will start blocking outright. Our frontend always sends Bearer tokens
    // via `Authorization`, so wildcard was silently on borrowed time.
    let base = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::CACHE_CONTROL,
            header::HeaderName::from_static("x-request-id"),
        ])
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

/// Tracing span builder that attaches the server-generated request id.
///
/// `request_id_middleware` has already overwritten `x-request-id` by the
/// time `TraceLayer` inspects the request, so the header read here is always
/// the server-generated value.
fn make_http_span(req: &Request<Body>) -> tracing::Span {
    let request_id = req
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    info_span!(
        "http_request",
        method = %req.method(),
        uri = %req.uri(),
        request_id = %request_id,
    )
}

/// Create the Axum application.
pub fn create_app(state: AppState) -> Router {
    let cors = build_cors_layer();

    // Uploads sub-router: PUT writes files, everything else served by ServeDir.
    // Override the global body limit because file uploads are larger than
    // ordinary API requests.
    let uploads_router = Router::new()
        .route("/{*path}", axum::routing::put(local_evidence_upload))
        .fallback_service(ServeDir::new(&state.uploads_path))
        .layer(DefaultBodyLimit::max(LOCAL_UPLOADS_BODY_LIMIT_BYTES));

    Router::new()
        // API routes under /v1
        .nest("/v1", api_routes())
        // Swagger UI at /swagger-ui (also serves /api-docs/openapi.json)
        .merge(swagger_routes())
        // Uploads: PUT for evidence, GET served statically
        .nest("/uploads", uploads_router)
        // Health checks: /health is the liveness+DB probe (an orchestrator
        // must be able to tell a healthy server from one whose database
        // vanished); /health/ready adds dependency detail.
        .route("/health", axum::routing::get(health))
        .route("/health/ready", axum::routing::get(health_ready))
        // Middleware. Layer application order is bottom-up: the last .layer
        // added wraps all inner work, so request_id runs before TraceLayer
        // gets to make its span — giving the span access to the id.
        .layer(DefaultBodyLimit::max(DEFAULT_BODY_LIMIT_BYTES))
        .layer(TraceLayer::new_for_http().make_span_with(make_http_span))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors)
        // State
        .with_state(state)
}
