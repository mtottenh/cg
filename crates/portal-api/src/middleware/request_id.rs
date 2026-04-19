//! Request-ID middleware.
//!
//! Assigns every request a server-generated identifier, inserts it into the
//! request extensions and headers (overwriting any client-supplied value),
//! includes it in the tracing span, and echoes it back in the response.
//!
//! Rationale: the prior implementation used `tower_http::request_id::
//! SetRequestIdLayer`, which honors an incoming `x-request-id` header if the
//! client provides one. That means logs, DTO echoes, and any downstream
//! correlation were using attacker-controlled input — a log-injection and
//! correlation-poisoning vector. Now the server always generates; the client
//! header is discarded.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

/// Header name used for propagation.
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Server-generated identifier for the current request, available from
/// `Request::extensions()` inside handlers or Tower middleware.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Axum middleware that overwrites any client-supplied `x-request-id` with a
/// fresh server-generated id, stashes it in request extensions, and
/// reflects it on the response.
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let id = format!("req_{}", Uuid::now_v7().simple());

    // HeaderValue::from_str rejects non-ASCII/control characters; our
    // generated id is ASCII-hex so this is effectively infallible.
    match HeaderValue::from_str(&id) {
        Ok(value) => {
            req.headers_mut().insert(REQUEST_ID_HEADER, value.clone());
            req.extensions_mut().insert(RequestId(id));
            let mut resp = next.run(req).await;
            resp.headers_mut().insert(REQUEST_ID_HEADER, value);
            resp
        }
        Err(_) => next.run(req).await,
    }
}

/// Read a server-generated request id from request headers.
///
/// Handlers can use this as a shared alternative to per-file `get_request_id`
/// helpers. Returns "unknown" if the middleware isn't installed (shouldn't
/// happen in a correctly-wired app).
#[must_use]
pub fn request_id_from_headers(headers: &axum::http::HeaderMap) -> &str {
    headers
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
}
