//! Scanner configuration from environment variables.

/// Configuration for the demo scanner daemon.
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// S3 bucket to scan for demo files.
    pub s3_bucket: String,
    /// S3 key prefix to filter (e.g., "demos/").
    pub s3_prefix: String,
    /// S3-compatible endpoint URL.
    pub s3_endpoint: Option<String>,
    /// S3 region.
    pub s3_region: String,
    /// Portal API base URL.
    pub api_url: String,
    /// Portal API key for service authentication.
    pub api_key: String,
    /// CS2 demo stats service URL.
    pub demo_service_url: String,
    /// S3 scan poll interval in seconds.
    pub interval_secs: u64,
    /// Pending-demo processing interval in seconds.
    pub processing_interval_secs: u64,
    /// Game ID for cataloged demos.
    pub game_id: String,
}

impl ScannerConfig {
    /// Load configuration from environment variables.
    ///
    /// # Panics
    ///
    /// Panics if required environment variables are missing.
    pub fn from_env() -> Self {
        Self {
            s3_bucket: std::env::var("SCANNER_S3_BUCKET")
                .unwrap_or_else(|_| "cs2-10mans-demo-files".to_string()),
            s3_prefix: std::env::var("SCANNER_S3_PREFIX").unwrap_or_default(),
            s3_endpoint: Some(
                std::env::var("SCANNER_S3_ENDPOINT")
                    .unwrap_or_else(|_| "https://gb-lon-1.linodeobjects.com".to_string()),
            ),
            s3_region: std::env::var("SCANNER_S3_REGION")
                .unwrap_or_else(|_| "gb-lon-1".to_string()),
            api_url: std::env::var("PORTAL_API_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            api_key: std::env::var("PORTAL_API_KEY").expect("PORTAL_API_KEY is required"),
            demo_service_url: std::env::var("CS2_DEMO_SERVICE_URL")
                .unwrap_or_else(|_| "https://demos.cs210mans.uk".to_string()),
            interval_secs: std::env::var("SCANNER_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            processing_interval_secs: std::env::var("SCANNER_PROCESSING_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            game_id: std::env::var("SCANNER_GAME_ID").expect("SCANNER_GAME_ID is required"),
        }
    }
}
