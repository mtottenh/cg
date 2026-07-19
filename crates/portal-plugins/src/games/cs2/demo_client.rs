//! CS2 demo stats HTTP client.
//!
//! Client for fetching pre-parsed demo stats from the external demo service.

use super::demo_stats::Cs2DemoStats;
use crate::error::PluginError;
use futures_util::StreamExt;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, instrument};

/// Check if a filename ends with a specific extension (case-insensitive).
fn has_extension(filename: &str, ext: &str) -> bool {
    Path::new(filename)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Check if a filename ends with ".stats.json" (case-insensitive).
fn has_stats_json_extension(filename: &str) -> bool {
    filename.to_ascii_lowercase().ends_with(".stats.json")
}

const DEMO_BASE_URL: &str = "https://demos.cs210mans.uk";
const STATS_PATH: &str = "/stats";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum response body size we'll accept from the demo service.
///
/// `Cs2DemoStats` JSON is typically a few hundred KiB. Capping prevents a
/// compromised or hostile demo service from forcing the portal to buffer
/// arbitrary amounts of memory.
const MAX_STATS_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Validate a demo service base URL and return it normalized.
///
/// Defense-in-depth against SSRF: `CS2_DEMO_SERVICE_URL` is an operator-set
/// env var, so the attacker model is "misconfiguration or supply-chain
/// tampering" rather than a live request. Even so, refusing anything that
/// isn't `https` on a routable host keeps the blast radius bounded if the
/// var is ever wired to user input.
pub fn validate_base_url(raw: &str) -> Result<String, PluginError> {
    let url = reqwest::Url::parse(raw)
        .map_err(|e| PluginError::InvalidConfiguration(format!("invalid demo service URL: {e}")))?;

    if url.scheme() != "https" {
        return Err(PluginError::InvalidConfiguration(format!(
            "demo service URL must use https (got scheme `{}`)",
            url.scheme()
        )));
    }

    // `host_str()` wraps IPv6 addresses in `[...]`, which breaks parse-to-IpAddr.
    // Strip those brackets so IPv4 names, DNS names, and raw IPv6 literals all
    // flow through `is_private_or_loopback_host` the same way.
    let host_raw = url
        .host_str()
        .ok_or_else(|| PluginError::InvalidConfiguration("demo service URL has no host".into()))?;
    let host = host_raw.trim_start_matches('[').trim_end_matches(']');

    if is_private_or_loopback_host(host) {
        return Err(PluginError::InvalidConfiguration(format!(
            "demo service URL host `{host_raw}` is loopback/private/link-local; refusing to pivot to internal network"
        )));
    }

    // Strip any trailing slash so callers can do `format!("{base}/{path}")`
    // without producing `//`.
    let trimmed = raw.trim_end_matches('/').to_string();
    Ok(trimmed)
}

// `.local` here is a hostname suffix (mDNS), not a file extension; the host
// comes from `Url::host_str()`, which already lowercases DNS names.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_private_or_loopback_host(host: &str) -> bool {
    // Reject obvious name-based internal targets.
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".local")
    {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
                    || v4.is_multicast()
                    // CGNAT 100.64.0.0/10
                    || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    // fc00::/7 unique-local and fe80::/10 link-local
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }

    false
}

/// Client for fetching demo stats from the external demo service.
#[derive(Debug, Clone)]
pub struct Cs2DemoClient {
    client: reqwest::Client,
    base_url: String,
}

impl Default for Cs2DemoClient {
    fn default() -> Self {
        Self::new(DEMO_BASE_URL.to_string())
    }
}

impl Cs2DemoClient {
    /// Create a new demo client with the specified base URL.
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Fetch demo stats by demo name.
    ///
    /// # Arguments
    /// * `demo_name` - The demo file name (e.g., "match_12345.dem" or "match_12345")
    ///
    /// # Returns
    /// Parsed demo stats from the external service.
    #[instrument(skip(self))]
    pub async fn get_demo_stats(&self, demo_name: &str) -> Result<Cs2DemoStats, PluginError> {
        // Handle various input formats:
        // - "match_12345.dem" -> "match_12345.dem.stats.json"
        // - "match_12345.dem.stats.json" -> "match_12345.dem.stats.json"
        // - "match_12345" -> "match_12345.stats.json"
        let stats_name = if has_stats_json_extension(demo_name) {
            demo_name.to_string()
        } else if has_extension(demo_name, "dem") {
            format!("{demo_name}.stats.json")
        } else {
            format!("{demo_name}.dem.stats.json")
        };

        let url = format!("{}{}/{}", self.base_url, STATS_PATH, stats_name);

        debug!(url = %url, "Fetching demo stats");

        let response = self.client.get(&url).send().await.map_err(|e| {
            PluginError::ExternalService(format!("Failed to fetch demo stats: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(PluginError::NotFound(format!(
                "Demo stats not found: {} (status: {})",
                demo_name,
                response.status()
            )));
        }

        // Cap the response body so a hostile (or broken) demo service can't
        // force unbounded buffering. `Content-Length` is advisory — we also
        // enforce the cap while streaming chunks.
        if let Some(len) = response.content_length()
            && (len as usize) > MAX_STATS_RESPONSE_BYTES
        {
            return Err(PluginError::ExternalService(format!(
                "Demo stats response too large: {len} bytes (max {MAX_STATS_RESPONSE_BYTES})"
            )));
        }

        let mut body = Vec::with_capacity(
            response
                .content_length()
                .map_or(64 * 1024, |l| (l as usize).min(MAX_STATS_RESPONSE_BYTES)),
        );
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                PluginError::ExternalService(format!("Failed to read demo stats body: {e}"))
            })?;
            if body.len() + chunk.len() > MAX_STATS_RESPONSE_BYTES {
                return Err(PluginError::ExternalService(format!(
                    "Demo stats response exceeded {MAX_STATS_RESPONSE_BYTES} bytes"
                )));
            }
            body.extend_from_slice(&chunk);
        }

        let stats: Cs2DemoStats = serde_json::from_slice(&body)
            .map_err(|e| PluginError::ParseError(format!("Failed to parse demo stats: {e}")))?;

        debug!(
            map = %stats.map,
            teams = ?stats.team_names(),
            "Successfully fetched demo stats"
        );

        Ok(stats)
    }

    /// Get the download URL for a demo.
    pub fn get_demo_url(&self, demo_name: &str) -> String {
        let name = if has_extension(demo_name, "dem") {
            demo_name.to_string()
        } else {
            format!("{demo_name}.dem")
        };
        format!("{}/{}", self.base_url, name)
    }

    /// Get the stats URL for a demo.
    pub fn get_stats_url(&self, demo_name: &str) -> String {
        let stats_name = if has_stats_json_extension(demo_name) {
            demo_name.to_string()
        } else if has_extension(demo_name, "dem") {
            format!("{demo_name}.stats.json")
        } else {
            format!("{demo_name}.dem.stats.json")
        };
        format!("{}{}/{}", self.base_url, STATS_PATH, stats_name)
    }

    /// Get the base URL for this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demo_url_generation() {
        let client = Cs2DemoClient::default();

        // With .dem extension
        assert_eq!(
            client.get_demo_url("match_12345.dem"),
            "https://demos.cs210mans.uk/match_12345.dem"
        );

        // Without extension
        assert_eq!(
            client.get_demo_url("match_12345"),
            "https://demos.cs210mans.uk/match_12345.dem"
        );
    }

    #[test]
    fn test_stats_url_generation() {
        let client = Cs2DemoClient::default();

        // With .dem extension
        assert_eq!(
            client.get_stats_url("match_12345.dem"),
            "https://demos.cs210mans.uk/stats/match_12345.dem.stats.json"
        );

        // Without extension
        assert_eq!(
            client.get_stats_url("match_12345"),
            "https://demos.cs210mans.uk/stats/match_12345.dem.stats.json"
        );

        // Full stats name
        assert_eq!(
            client.get_stats_url("match_12345.dem.stats.json"),
            "https://demos.cs210mans.uk/stats/match_12345.dem.stats.json"
        );
    }

    #[test]
    fn test_custom_base_url() {
        let client = Cs2DemoClient::new("https://custom.example.com".to_string());
        assert_eq!(client.base_url(), "https://custom.example.com");
        assert_eq!(
            client.get_demo_url("test.dem"),
            "https://custom.example.com/test.dem"
        );
    }

    #[test]
    fn test_validate_base_url_accepts_https_public_host() {
        let ok = validate_base_url("https://demos.example.com").unwrap();
        assert_eq!(ok, "https://demos.example.com");

        // Trailing slash is trimmed so callers can `format!("{base}/…")`.
        let trimmed = validate_base_url("https://demos.example.com/").unwrap();
        assert_eq!(trimmed, "https://demos.example.com");
    }

    #[test]
    fn test_validate_base_url_rejects_http() {
        let err = validate_base_url("http://demos.example.com").unwrap_err();
        assert!(
            matches!(err, PluginError::InvalidConfiguration(_)),
            "expected InvalidConfiguration, got {err:?}"
        );
    }

    #[test]
    fn test_validate_base_url_rejects_loopback_and_private() {
        for bad in [
            "https://localhost",
            "https://demos.localhost",
            "https://127.0.0.1",
            "https://10.0.0.5",
            "https://192.168.1.1",
            "https://172.16.0.1",
            "https://169.254.169.254", // AWS/Azure metadata
            "https://[::1]",
            "https://some-service.internal",
        ] {
            let result = validate_base_url(bad);
            assert!(
                matches!(result, Err(PluginError::InvalidConfiguration(_))),
                "expected {bad} to be rejected, got {result:?}"
            );
        }
    }

    #[test]
    fn test_validate_base_url_rejects_garbage() {
        assert!(validate_base_url("not a url").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
        assert!(validate_base_url("https://").is_err());
    }

    #[tokio::test]
    #[ignore = "requires external demo-stats service"]
    async fn test_fetch_demo_stats_integration() {
        let client = Cs2DemoClient::default();
        let demo_name = "2024-09-14_20-17-30_9_de_inferno_team_Zan_vs_team_Maxymimi.dem";
        let result = client.get_demo_stats(demo_name).await;

        match result {
            Ok(stats) => {
                println!("Map: {}", stats.map);
                println!("Teams: {:?}", stats.team_names());
                println!("Final score: {:?}", stats.final_score);
                assert_eq!(stats.map, "de_inferno");
            }
            Err(e) => {
                println!("Error (expected if service unavailable): {e}");
            }
        }
    }
}
