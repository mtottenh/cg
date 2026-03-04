//! HTTP client for the portal admin API.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Client for calling portal admin API endpoints.
#[derive(Clone)]
pub struct PortalApiClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

/// A single demo entry for batch cataloging.
#[derive(Debug, Serialize)]
pub struct CatalogDemoEntry {
    pub file_name: String,
    pub s3_bucket: String,
    pub s3_key: String,
    pub file_size_bytes: Option<i64>,
}

/// Batch catalog request.
#[derive(Debug, Serialize)]
pub struct BatchCatalogRequest {
    pub game_id: String,
    pub demos: Vec<CatalogDemoEntry>,
}

/// Response from batch catalog.
#[derive(Debug, Deserialize)]
pub struct BatchCatalogResponse {
    pub data: BatchCatalogData,
}

/// Inner data of the batch catalog response.
#[derive(Debug, Deserialize)]
pub struct BatchCatalogData {
    pub created: Vec<CatalogDemoResponse>,
    pub existing: Vec<CatalogDemoResponse>,
    pub errors: Vec<CatalogErrorResponse>,
}

/// A cataloged demo from the API response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CatalogDemoResponse {
    pub id: String,
    pub file_name: String,
    pub status: String,
}

/// An error from catalog.
#[derive(Debug, Deserialize)]
pub struct CatalogErrorResponse {
    pub s3_key: String,
    pub error: String,
}

/// A pending demo from the API.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PendingDemoResponse {
    pub id: String,
    pub file_name: String,
    pub status: String,
}

/// Wrapper for data responses.
#[derive(Debug, Deserialize)]
pub struct DataResponse<T> {
    pub data: T,
}

/// Submit demo stats request.
#[derive(Debug, Serialize)]
pub struct SubmitStatsRequest {
    pub map_name: Option<String>,
    pub match_date: Option<String>,
    pub team1_name: Option<String>,
    pub team2_name: Option<String>,
    pub team1_score: Option<i32>,
    pub team2_score: Option<i32>,
    pub total_rounds: Option<i32>,
    pub duration_seconds: Option<i64>,
    pub players: Vec<SubmitPlayerEntry>,
    pub raw_stats: serde_json::Value,
}

/// A player entry for stats submission.
#[derive(Debug, Serialize)]
pub struct SubmitPlayerEntry {
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub stats: serde_json::Value,
}

/// Mark demo as failed request.
#[derive(Debug, Serialize)]
pub struct MarkFailedRequest {
    pub error: String,
}

impl PortalApiClient {
    /// Create a new API client.
    pub fn new(base_url: String, token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// Batch catalog demos.
    pub async fn batch_catalog(&self, request: &BatchCatalogRequest) -> Result<BatchCatalogData> {
        let url = format!("{}/v1/admin/demos/batch", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(request)
            .send()
            .await
            .context("batch catalog request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("batch catalog failed ({status}): {body}");
        }

        let parsed: BatchCatalogResponse = resp.json().await.context("parse batch catalog response")?;
        Ok(parsed.data)
    }

    /// Get pending demos.
    pub async fn get_pending_demos(&self, limit: i64) -> Result<Vec<PendingDemoResponse>> {
        let url = format!("{}/v1/admin/demos/pending?limit={limit}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
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
        let url = format!("{}/v1/admin/demos/{demo_id}/stats", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
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
        let url = format!("{}/v1/admin/demos/{demo_id}/stats-failed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
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
