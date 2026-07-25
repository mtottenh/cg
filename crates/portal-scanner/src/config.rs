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
            // P-137, same mechanism as `Cs2DemoClient::default()`: this used to
            // fall back to the live `https://demos.cs210mans.uk`, so a scanner
            // started without the variable pointed itself at a third party
            // rather than refusing to start. Required and validated, like
            // `PORTAL_API_KEY` and `SCANNER_GAME_ID` above.
            demo_service_url: resolve_demo_service_url(
                std::env::var("CS2_DEMO_SERVICE_URL").ok().as_deref(),
            )
            .unwrap_or_else(|e| panic!("{e}")),
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

/// The message a scanner started without a demo-stats service dies with.
///
/// P-160: `docker-compose.yml` passed
/// `CS2_DEMO_SERVICE_URL: ${CS2_DEMO_SERVICE_URL:-https://demos.cs210mans.uk}`,
/// so `docker compose --profile scanner up` with no `.env` wired the scanner to
/// a live third-party host without anybody choosing it. Compose cannot use
/// `${VAR:?err}` here — that fails `docker compose config` for the whole file
/// even when the profile is inactive, which is why the sibling required
/// variables are all `${VAR:-}` — so the empty string has to reach this binary
/// and be rejected here, by name and with the fix in the text.
const MISSING_DEMO_SERVICE_URL: &str = "CS2_DEMO_SERVICE_URL is required and must not be empty. \
     Set it to the base URL of your demo-stats service, e.g. \
     CS2_DEMO_SERVICE_URL=https://demos.your-domain.example in .env. It must be https and \
     must not be a private/loopback host.";

/// Resolve and validate the demo-stats service URL from its raw env value.
///
/// Split out of [`ScannerConfig::from_env`] so the empty/missing/invalid cases
/// are testable without mutating process environment.
fn resolve_demo_service_url(raw: Option<&str>) -> Result<String, String> {
    // `std::env::var` yields `Ok("")` for a variable that is set-but-empty,
    // which is exactly what a compose `${VAR:-}` default produces. Treating it
    // as "provided" got the operator a URL-parse error instead of being told
    // which variable to set.
    match raw.map(str::trim) {
        None | Some("") => Err(MISSING_DEMO_SERVICE_URL.to_string()),
        Some(url) => portal_plugins::validate_demo_service_url(url)
            .map_err(|e| format!("CS2_DEMO_SERVICE_URL is invalid: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P-160. The two ways an operator arrives with no demo service — never set
    /// it, or inherit compose's `${CS2_DEMO_SERVICE_URL:-}` — must produce the
    /// same actionable message, not a URL-parse error and not a silent default.
    #[test]
    fn missing_or_empty_demo_service_url_names_the_variable_and_the_fix() {
        for raw in [None, Some(""), Some("   ")] {
            let err = resolve_demo_service_url(raw).expect_err("expected a refusal");
            assert!(
                err.contains("CS2_DEMO_SERVICE_URL is required"),
                "message must name the variable and say it is required, got: {err}"
            );
            assert!(
                err.contains("CS2_DEMO_SERVICE_URL=https://"),
                "message must show what to set, got: {err}"
            );
        }
    }

    /// The defect this replaced: an unset variable resolving to somebody else's
    /// host. Nothing in the failure path may name a host the operator did not.
    #[test]
    fn refusing_never_suggests_a_third_party_host() {
        let err = resolve_demo_service_url(None).expect_err("expected a refusal");
        assert!(
            !err.contains("cs210mans"),
            "the refusal must not hand the operator a third-party host: {err}"
        );
    }

    #[test]
    fn a_configured_https_url_is_accepted_and_normalised() {
        let url = resolve_demo_service_url(Some("https://demos.example.com/"))
            .expect("a public https URL is valid");
        assert!(url.starts_with("https://demos.example.com"));
    }

    /// A set-but-wrong URL is a different failure from a missing one, and must
    /// not be reported as "required".
    #[test]
    fn an_invalid_url_is_rejected_as_invalid_rather_than_missing() {
        let err = resolve_demo_service_url(Some("http://127.0.0.1:3100"))
            .expect_err("loopback/plain-http must be refused");
        assert!(
            err.contains("CS2_DEMO_SERVICE_URL is invalid"),
            "got: {err}"
        );
    }

    /// P-160, the deployment half. The refusal above only helps if nothing in
    /// the shipped config quietly supplies a value first — which is exactly what
    /// `${CS2_DEMO_SERVICE_URL:-https://demos.cs210mans.uk}` did. Compose
    /// substitution happens before the binary ever runs, so no amount of care in
    /// `from_env` can see through a default baked into the yaml.
    ///
    /// Asserts on the *shape* (`:-` followed by anything) rather than on the one
    /// host that was there, so re-introducing the mechanism with a different
    /// host still fails.
    #[test]
    fn compose_supplies_no_default_demo_service_url() {
        let compose = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docker-compose.yml")
            .canonicalize()
            .expect("docker-compose.yml sits at the repo root");
        let text = std::fs::read_to_string(&compose).expect("read docker-compose.yml");

        for (n, line) in text.lines().enumerate() {
            let Some(rest) = line.split_once("${CS2_DEMO_SERVICE_URL").map(|(_, r)| r) else {
                continue;
            };
            // `${VAR}` and `${VAR:-}` are fine; `${VAR:-anything}` is the defect.
            let default_value = rest
                .strip_prefix(":-")
                .and_then(|r| r.split_once('}'))
                .map(|(v, _)| v.trim());
            assert!(
                default_value.is_none_or(str::is_empty),
                "docker-compose.yml:{} supplies a default demo-stats host ({:?}). \
                 An operator running `docker compose up` with no .env would be silently \
                 pointed at it. Leave it empty and let the binary refuse.",
                n + 1,
                default_value.unwrap_or_default(),
            );
        }
    }
}
