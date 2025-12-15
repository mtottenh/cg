//! CS2 demo stats HTTP client.
//!
//! Client for fetching pre-parsed demo stats from the external demo service.

use super::demo_stats::Cs2DemoStats;
use crate::error::PluginError;
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
    filename
        .to_ascii_lowercase()
        .ends_with(".stats.json")
}

const DEMO_BASE_URL: &str = "https://demos.cs210mans.uk";
const STATS_PATH: &str = "/stats";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ExternalService(format!("Failed to fetch demo stats: {e}")))?;

        if !response.status().is_success() {
            return Err(PluginError::NotFound(format!(
                "Demo stats not found: {} (status: {})",
                demo_name,
                response.status()
            )));
        }

        let stats: Cs2DemoStats = response
            .json()
            .await
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

    #[tokio::test]
    #[ignore] // Requires external service
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
