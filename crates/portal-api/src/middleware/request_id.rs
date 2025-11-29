//! Request ID middleware.

use axum::http::{HeaderName, HeaderValue, Request};
use tower_http::request_id::{MakeRequestId, RequestId};
use uuid::Uuid;

/// Generate unique request IDs.
#[derive(Clone, Copy)]
pub struct RequestIdGenerator;

impl MakeRequestId for RequestIdGenerator {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let id = format!("req_{}", Uuid::now_v7().simple());
        Some(RequestId::new(HeaderValue::from_str(&id).ok()?))
    }
}

/// Create the request ID layer.
pub fn request_id_layer() -> tower_http::request_id::SetRequestIdLayer<RequestIdGenerator> {
    tower_http::request_id::SetRequestIdLayer::new(
        HeaderName::from_static("x-request-id"),
        RequestIdGenerator,
    )
}
