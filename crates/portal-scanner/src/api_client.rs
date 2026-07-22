//! HTTP client for the portal admin API.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Client for calling portal internal API endpoints.
#[derive(Clone)]
pub struct PortalApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// A single demo entry for batch cataloging.
#[derive(Debug, Serialize)]
pub struct CatalogDemoEntry {
    /// Demo file name (basename of the S3 key).
    pub file_name: String,
    /// S3 bucket the demo was found in.
    pub s3_bucket: String,
    /// Full S3 key of the demo object.
    pub s3_key: String,
    /// Object size in bytes, if known.
    pub file_size_bytes: Option<i64>,
}

/// Batch catalog request.
#[derive(Debug, Serialize)]
pub struct BatchCatalogRequest {
    /// ID of the game the demos belong to.
    pub game_id: String,
    /// Demos to catalog in this batch.
    pub demos: Vec<CatalogDemoEntry>,
}

/// Response from batch catalog.
#[derive(Debug, Deserialize)]
pub struct BatchCatalogResponse {
    /// Payload of the batch catalog response.
    pub data: BatchCatalogData,
}

/// Inner data of the batch catalog response.
#[derive(Debug, Deserialize)]
pub struct BatchCatalogData {
    /// Demos newly created by this batch.
    pub created: Vec<CatalogDemoResponse>,
    /// Demos that were already cataloged.
    pub existing: Vec<CatalogDemoResponse>,
    /// Entries that failed to catalog.
    pub errors: Vec<CatalogErrorResponse>,
}

/// A cataloged demo from the API response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CatalogDemoResponse {
    /// Demo ID assigned by the portal.
    pub id: String,
    /// Demo file name.
    pub file_name: String,
    /// Processing status of the demo.
    pub status: String,
}

/// An error from catalog.
#[derive(Debug, Deserialize)]
pub struct CatalogErrorResponse {
    /// S3 key of the entry that failed.
    pub s3_key: String,
    /// Error message describing the failure.
    pub error: String,
}

/// A pending demo from the API.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PendingDemoResponse {
    /// Demo ID assigned by the portal.
    pub id: String,
    /// Demo file name.
    pub file_name: String,
    /// Processing status of the demo.
    pub status: String,
}

/// Wrapper for data responses.
#[derive(Debug, Deserialize)]
pub struct DataResponse<T> {
    /// The wrapped response payload.
    pub data: T,
}

/// Submit demo stats request.
#[derive(Debug, Serialize)]
pub struct SubmitStatsRequest {
    /// Map the match was played on.
    pub map_name: Option<String>,
    /// Date the match was played (ISO 8601 string).
    pub match_date: Option<String>,
    /// Name of the first team.
    pub team1_name: Option<String>,
    /// Name of the second team.
    pub team2_name: Option<String>,
    /// Final score of the first team.
    pub team1_score: Option<i32>,
    /// Final score of the second team.
    pub team2_score: Option<i32>,
    /// Total number of rounds played.
    pub total_rounds: Option<i32>,
    /// Match duration in seconds.
    pub duration_seconds: Option<i64>,
    /// Per-player statistics entries.
    pub players: Vec<SubmitPlayerEntry>,
    /// Full raw stats payload from the demo parser.
    pub raw_stats: serde_json::Value,
}

/// A player entry for stats submission.
#[derive(Debug, Serialize)]
pub struct SubmitPlayerEntry {
    /// Player's Steam ID.
    pub steam_id: String,
    /// Player's in-game name.
    pub player_name: String,
    /// Team the player was on, if known.
    pub team_name: Option<String>,
    /// Per-player stats as JSON.
    pub stats: serde_json::Value,
}

/// Mark demo as failed request.
#[derive(Debug, Serialize)]
pub struct MarkFailedRequest {
    /// Error message explaining why processing failed.
    pub error: String,
}

impl PortalApiClient {
    /// Create a new API client.
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    /// Batch catalog demos.
    pub async fn batch_catalog(&self, request: &BatchCatalogRequest) -> Result<BatchCatalogData> {
        let url = format!("{}/v1/internal/demos/batch", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(request)
            .send()
            .await
            .context("batch catalog request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("batch catalog failed ({status}): {body}");
        }

        let parsed: BatchCatalogResponse =
            resp.json().await.context("parse batch catalog response")?;
        Ok(parsed.data)
    }

    /// Get pending demos.
    pub async fn get_pending_demos(&self, limit: i64) -> Result<Vec<PendingDemoResponse>> {
        let url = format!("{}/v1/internal/demos/pending?limit={limit}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .context("get pending demos request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("get pending demos failed ({status}): {body}");
        }

        let parsed: DataResponse<Vec<PendingDemoResponse>> =
            resp.json().await.context("parse pending demos response")?;
        Ok(parsed.data)
    }

    /// Submit parsed stats for a demo.
    pub async fn submit_stats(&self, demo_id: &str, request: &SubmitStatsRequest) -> Result<()> {
        let url = format!("{}/v1/internal/demos/{demo_id}/stats", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(request)
            .send()
            .await
            .context("submit stats request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("submit stats for {demo_id} failed ({status}): {body}");
        }

        debug!(demo_id, "Stats submitted successfully");
        Ok(())
    }

    /// Mark a demo's stats processing as failed.
    pub async fn mark_stats_failed(&self, demo_id: &str, error: &str) -> Result<()> {
        let url = format!("{}/v1/internal/demos/{demo_id}/stats-failed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&MarkFailedRequest {
                error: error.to_string(),
            })
            .send()
            .await
            .context("mark failed request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(demo_id, "mark stats failed returned ({status}): {body}");
        }

        Ok(())
    }
}
