//! Test helpers for API integration tests.

pub mod ws;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use portal_api::app::create_app;
use portal_api::state::AppState;
use portal_db::DbPool;
use portal_test::database::TestDb;
use serde::de::DeserializeOwned;
use std::net::SocketAddr;
use tower::util::ServiceExt;

/// Test application wrapper.
pub struct TestApp {
    pub app: Router,
    pub db: TestDb,
    /// Bound address when server is started for WebSocket tests.
    server_addr: Option<SocketAddr>,
}

impl TestApp {
    /// Create a new test application with an isolated database.
    pub async fn new() -> Self {
        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone(), "test-jwt-secret");
        let app = create_app(state);

        Self { app, db, server_addr: None }
    }

    /// Start the server on a random port and return the bound address.
    ///
    /// This is required for WebSocket tests since they need a real TCP connection.
    pub async fn start_server(&mut self) -> SocketAddr {
        if let Some(addr) = self.server_addr {
            return addr;
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to address");
        let addr = listener.local_addr().expect("Failed to get local address");

        let app = self.app.clone();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        self.server_addr = Some(addr);
        addr
    }

    /// Get the JWT secret used for this test app.
    pub fn jwt_secret(&self) -> &str {
        "test-jwt-secret"
    }

    /// Get the database pool.
    pub fn pool(&self) -> &DbPool {
        &self.db.pool
    }

    /// Make a GET request.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap())
            .await
    }

    /// Make an authenticated GET request.
    pub async fn get_auth(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("Authorization", "Bearer dev-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with JSON body (with auth).
    pub async fn post_json<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer dev-token")
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with JSON body (without auth).
    pub async fn post_json_no_auth<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PATCH request with JSON body.
    pub async fn patch_json<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer dev-token")
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a DELETE request.
    pub async fn delete_auth(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("Authorization", "Bearer dev-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a GET request with a specific token.
    pub async fn get_with_token(&self, uri: &str, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with JSON body and a specific token.
    pub async fn post_json_with_token<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        token: &str,
    ) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PATCH request with JSON body and a specific token.
    pub async fn patch_json_with_token<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        token: &str,
    ) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PATCH request with JSON body (without auth).
    pub async fn patch_json_no_auth<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("Content-Type", "application/json")
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PUT request with JSON body (with auth).
    pub async fn put_json<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer dev-token")
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PUT request with JSON body and a specific token.
    pub async fn put_json_with_token<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
        token: &str,
    ) -> TestResponse {
        let json = serde_json::to_string(body).unwrap();
        self.request(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a POST request (with auth, no body).
    pub async fn post_auth(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Authorization", "Bearer dev-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with a specific token (no body).
    pub async fn post_with_token(&self, uri: &str, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a DELETE request with a specific token.
    pub async fn delete_with_token(&self, uri: &str, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a raw request.
    async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .app
            .clone()
            .oneshot(request)
            .await
            .expect("request failed");

        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();

        TestResponse { status, body: body.to_vec() }
    }
}

/// Test response wrapper.
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Vec<u8>,
}

impl TestResponse {
    /// Get the response body as a string.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Parse the response body as JSON.
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("failed to parse JSON response")
    }

    /// Assert the response status.
    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(
            self.status, expected,
            "Expected status {}, got {}. Body: {}",
            expected,
            self.status,
            self.text()
        );
    }
}
