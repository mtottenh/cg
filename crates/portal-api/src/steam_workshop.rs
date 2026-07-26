//! Steam Workshop metadata lookup.
//!
//! Wraps `ISteamRemoteStorage/GetPublishedFileDetails` — a keyless
//! form-POST Web API (and unlike CS:GO, CS2 servers also need no API key
//! to download the items themselves). The admin map-catalog UI uses this
//! to validate a pasted workshop URL/id and prefill map metadata; it is
//! deliberately read-only — Steam exposes no write API for workshop
//! collections, which is why the portal models workshop maps per-item.

use async_trait::async_trait;
use std::time::Duration;

/// Facts about a published workshop file, as returned by Steam.
#[derive(Debug, Clone)]
pub struct WorkshopFileDetails {
    /// The published file id (decimal digits).
    pub workshop_id: String,
    pub title: Option<String>,
    pub preview_url: Option<String>,
    /// Upload filename inside the item (e.g. `de_cache.vpk`) — its stem is
    /// a HINT for the engine-level map name, not a guarantee.
    pub filename: Option<String>,
    pub file_size_bytes: Option<i64>,
    /// Unix seconds of the author's last update.
    pub time_updated: Option<i64>,
    /// App the item is consumed by (CS2 = 730).
    pub consumer_app_id: Option<i64>,
    /// 0 = public, 1 = friends-only, 2 = private, 3 = unlisted.
    pub visibility: Option<i64>,
    pub banned: bool,
}

/// Read-only source of workshop item metadata.
#[async_trait]
pub trait WorkshopMetadataProvider: Send + Sync {
    /// Fetch details for one published file id.
    ///
    /// `Ok(None)` when Steam reports the item does not exist (per-item
    /// `result` != 1); `Err` for transport/contract failures.
    async fn published_file_details(
        &self,
        file_id: u64,
    ) -> Result<Option<WorkshopFileDetails>, String>;
}

/// Production [`WorkshopMetadataProvider`] backed by `reqwest`.
pub struct HttpWorkshopClient {
    client: reqwest::Client,
    base_url: String,
}

impl HttpWorkshopClient {
    /// Create a client against a Steam Web API host (injectable for tests).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Client against the real Steam Web API.
    #[must_use]
    pub fn steam_default() -> Self {
        Self::new("https://api.steampowered.com")
    }
}

#[async_trait]
impl WorkshopMetadataProvider for HttpWorkshopClient {
    async fn published_file_details(
        &self,
        file_id: u64,
    ) -> Result<Option<WorkshopFileDetails>, String> {
        let url = format!(
            "{}/ISteamRemoteStorage/GetPublishedFileDetails/v1/",
            self.base_url.trim_end_matches('/')
        );
        let form = [
            ("itemcount", "1".to_string()),
            ("publishedfileids[0]", file_id.to_string()),
        ];
        let response = self
            .client
            .post(&url)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("steam workshop api unreachable: {e}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "steam workshop api returned HTTP {}",
                response.status()
            ));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("steam workshop api returned invalid JSON: {e}"))?;

        let Some(detail) = body["response"]["publishedfiledetails"].get(0) else {
            return Ok(None);
        };
        // Per-item result: 1 = ok, 9 = file not found.
        if detail["result"].as_i64() != Some(1) {
            return Ok(None);
        }
        Ok(Some(parse_details(file_id, detail)))
    }
}

/// Map one `publishedfiledetails` entry into [`WorkshopFileDetails`].
///
/// Steam's JSON is loosely typed (`file_size` arrives as a string, flags
/// as 0/1 ints) — parse defensively, field by field.
fn parse_details(file_id: u64, detail: &serde_json::Value) -> WorkshopFileDetails {
    let as_i64 =
        |v: &serde_json::Value| -> Option<i64> { v.as_i64().or_else(|| v.as_str()?.parse().ok()) };
    WorkshopFileDetails {
        workshop_id: file_id.to_string(),
        title: detail["title"].as_str().map(str::to_string),
        preview_url: detail["preview_url"].as_str().map(str::to_string),
        filename: detail["filename"].as_str().map(str::to_string),
        file_size_bytes: as_i64(&detail["file_size"]),
        time_updated: as_i64(&detail["time_updated"]),
        consumer_app_id: as_i64(&detail["consumer_app_id"]),
        visibility: as_i64(&detail["visibility"]),
        banned: as_i64(&detail["banned"]).unwrap_or(0) != 0,
    }
}

/// Derive an engine-level map-name hint from the item's upload filename:
/// path stripped, `.vpk`/`.bsp` extension stripped. `de_cache.vpk` →
/// `de_cache`.
#[must_use]
pub fn engine_name_hint(filename: &str) -> Option<String> {
    let stem = filename
        .rsplit(['/', '\\'])
        .next()?
        .trim_end_matches(".vpk")
        .trim_end_matches(".bsp")
        .trim();
    (!stem.is_empty()).then(|| stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_name_hint_strips_paths_and_extensions() {
        assert_eq!(
            engine_name_hint("de_cache.vpk").as_deref(),
            Some("de_cache")
        );
        assert_eq!(
            engine_name_hint("maps/de_cache_v2.bsp").as_deref(),
            Some("de_cache_v2")
        );
        assert_eq!(
            engine_name_hint("workshop\\aim_botz.vpk").as_deref(),
            Some("aim_botz")
        );
        assert_eq!(engine_name_hint(""), None);
    }

    #[test]
    fn parse_details_handles_steams_loose_types() {
        let detail = serde_json::json!({
            "result": 1,
            "title": "Cache",
            "preview_url": "https://img.example/1.jpg",
            "filename": "de_cache.vpk",
            "file_size": "734003200",
            "time_updated": 1750000000,
            "consumer_app_id": 730,
            "visibility": 0,
            "banned": 0,
        });
        let parsed = parse_details(3437809122, &detail);
        assert_eq!(parsed.workshop_id, "3437809122");
        assert_eq!(parsed.file_size_bytes, Some(734003200));
        assert_eq!(parsed.consumer_app_id, Some(730));
        assert!(!parsed.banned);
    }
}
