//! Test helpers for API integration tests.

pub mod minio;
pub mod ws;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use portal_api::adapters::{EvidenceStorageBackend, S3EvidenceStorageAdapter};
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
    /// Wrap the app so every request carries a `ConnectInfo<SocketAddr>`
    /// extension. Production gets this from
    /// `into_make_service_with_connect_info` in portal-app; the `oneshot`
    /// path used by these tests has no TCP peer, so without it the
    /// `tower_governor` rate limiter on `/auth/*` 500s with
    /// "Unable To Extract Key!" on every request.
    fn with_connect_info(app: Router) -> Router {
        app.layer(axum::Extension(axum::extract::ConnectInfo(
            SocketAddr::from(([127, 0, 0, 1], 0)),
        )))
    }

    /// Initialize tracing (once per process).
    fn init_tracing() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
                )
                .with_test_writer()
                .init();
        });
    }

    /// Create a new test application with an isolated database.
    pub async fn new() -> Self {
        Self::init_tracing();

        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone(), "test-jwt-secret").await;
        let app = Self::with_connect_info(create_app(state));

        Self {
            app,
            db,
            server_addr: None,
        }
    }

    /// Create a test application whose CS2 demo-stats client points at a
    /// local mock server (e.g. `http://127.0.0.1:PORT`), bypassing the
    /// https/non-private-host validation applied to the real env var.
    pub async fn new_with_demo_service(demo_service_url: &str) -> Self {
        Self::init_tracing();

        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone(), "test-jwt-secret")
            .await
            .with_cs2_demo_url_unchecked(demo_service_url.to_string());
        let app = Self::with_connect_info(create_app(state));

        Self {
            app,
            db,
            server_addr: None,
        }
    }

    /// Create a new test application with S3 evidence storage backed by MinIO.
    ///
    /// `minio_endpoint` — e.g. `http://127.0.0.1:32768`
    /// `bucket` — the S3 bucket name for evidence (must be pre-created)
    pub async fn new_with_s3(minio_endpoint: &str, bucket: &str) -> Self {
        Self::init_tracing();

        // Build S3 SDK config with static credentials for MinIO
        let creds =
            aws_sdk_s3::config::Credentials::new("minioadmin", "minioadmin", None, None, "test");
        let sdk_config = aws_config::from_env()
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .endpoint_url(minio_endpoint)
            .credentials_provider(creds)
            .load()
            .await;

        let adapter = S3EvidenceStorageAdapter::from_sdk_config(
            &sdk_config,
            bucket,
            format!("{minio_endpoint}/{bucket}"),
            true, // force_path_style for MinIO
        );
        let storage = EvidenceStorageBackend::S3(adapter);

        let db = TestDb::new().await;
        let state = AppState::new(db.pool.clone(), "test-jwt-secret")
            .await
            .with_evidence_storage(storage, bucket.to_string());
        let app = Self::with_connect_info(create_app(state));

        Self {
            app,
            db,
            server_addr: None,
        }
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

    /// Get the database pool.
    pub fn pool(&self) -> &DbPool {
        &self.db.pool
    }

    /// Make a GET request.
    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
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
    pub async fn post_json_no_auth<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
    ) -> TestResponse {
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
                .header("Authorization", format!("Bearer {token}"))
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
                .header("Authorization", format!("Bearer {token}"))
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
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(json))
                .unwrap(),
        )
        .await
    }

    /// Make a PATCH request with JSON body (without auth).
    pub async fn patch_json_no_auth<T: serde::Serialize>(
        &self,
        uri: &str,
        body: &T,
    ) -> TestResponse {
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
                .header("Authorization", format!("Bearer {token}"))
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
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Make a POST request with multipart form data (with dev-token auth).
    pub async fn post_multipart_auth(
        &self,
        uri: &str,
        field_name: &str,
        file_name: &str,
        content_type: &str,
        data: &[u8],
    ) -> TestResponse {
        self.post_multipart_with_token(uri, field_name, file_name, content_type, data, "dev-token")
            .await
    }

    /// Make a POST request with multipart form data and a specific token.
    pub async fn post_multipart_with_token(
        &self,
        uri: &str,
        field_name: &str,
        file_name: &str,
        content_type: &str,
        data: &[u8],
        token: &str,
    ) -> TestResponse {
        let boundary = "----TestBoundary7MA4YWxkTrZu0gW";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::from(body))
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

        TestResponse {
            status,
            body: body.to_vec(),
        }
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
            self.status,
            expected,
            "Expected status {}, got {}. Body: {}",
            expected,
            self.status,
            self.text()
        );
    }
}
