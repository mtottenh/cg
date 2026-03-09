//! Orchestration: scan S3 → catalog → fetch and submit stats.

use anyhow::Result;
use tracing::{debug, error, info, warn};

use crate::api_client::{BatchCatalogRequest, CatalogDemoEntry, PortalApiClient};
use crate::config::ScannerConfig;
use crate::s3_scanner;
use crate::stats_converter;

/// Scan S3 for new demo files, catalog them, and fetch stats.
pub async fn scan_and_process(
    s3_client: &aws_sdk_s3::Client,
    api_client: &PortalApiClient,
    demo_client: &portal_plugins::Cs2DemoClient,
    config: &ScannerConfig,
) -> Result<()> {
    // 1. List demo files from S3
    let objects = s3_scanner::list_demo_files(s3_client, &config.s3_bucket, &config.s3_prefix).await?;

    if objects.is_empty() {
        info!("No demo files found in S3");
        return Ok(());
    }

    info!(count = objects.len(), "Found demo files in S3");

    // 2. Batch catalog (up to 500 at a time)
    for chunk in objects.chunks(500) {
        let demos: Vec<CatalogDemoEntry> = chunk
            .iter()
            .map(|obj| {
                let file_name = obj
                    .key
                    .rsplit('/')
                    .next()
                    .unwrap_or(&obj.key)
                    .to_string();
                CatalogDemoEntry {
                    file_name,
                    s3_bucket: config.s3_bucket.clone(),
                    s3_key: obj.key.clone(),
                    file_size_bytes: Some(obj.size),
                }
            })
            .collect();

        let request = BatchCatalogRequest {
            game_id: config.game_id.clone(),
            demos,
        };

        match api_client.batch_catalog(&request).await {
            Ok(result) => {
                info!(
                    created = result.created.len(),
                    existing = result.existing.len(),
                    errors = result.errors.len(),
                    "Batch catalog complete"
                );

                // 3. Fetch stats for newly created demos
                for demo in &result.created {
                    if let Err(e) = fetch_and_submit_stats(api_client, demo_client, &demo.id, &demo.file_name).await
                    {
                        warn!(
                            demo_id = %demo.id,
                            file_name = %demo.file_name,
                            error = %e,
                            "Failed to fetch stats for new demo"
                        );
                    }
                }

                for err in &result.errors {
                    warn!(s3_key = %err.s3_key, error = %err.error, "Catalog error");
                }
            }
            Err(e) => {
                error!(error = %e, "Batch catalog failed");
            }
        }
    }

    Ok(())
}

/// Process pending demos that previously failed stats fetching.
pub async fn process_pending(
    api_client: &PortalApiClient,
    demo_client: &portal_plugins::Cs2DemoClient,
) -> Result<()> {
    let pending = api_client.get_pending_demos(50).await?;

    if pending.is_empty() {
        return Ok(());
    }

    info!(count = pending.len(), "Processing pending demos");

    for demo in &pending {
        if let Err(e) = fetch_and_submit_stats(api_client, demo_client, &demo.id, &demo.file_name).await {
            warn!(
                demo_id = %demo.id,
                file_name = %demo.file_name,
                error = %e,
                "Failed to fetch stats for pending demo"
            );
        }
    }

    Ok(())
}

/// Fetch demo stats from the external service and submit to the portal API.
async fn fetch_and_submit_stats(
    api_client: &PortalApiClient,
    demo_client: &portal_plugins::Cs2DemoClient,
    demo_id: &str,
    file_name: &str,
) -> Result<()> {
    // Strip .dem.bz2 or .dem extension for the stats lookup
    let stats_name = file_name
        .trim_end_matches(".bz2")
        .trim_end_matches(".dem");

    let stats_url = demo_client.get_stats_url(stats_name);
    debug!(demo_id, file_name, url = %stats_url, "Fetching demo stats");

    match demo_client.get_demo_stats(stats_name).await {
        Ok(stats) => {
            let request = stats_converter::convert_stats(&stats);
            api_client.submit_stats(demo_id, &request).await?;
            info!(demo_id, file_name, map = %stats.map, "Stats submitted");
        }
        Err(portal_plugins::PluginError::NotFound(_)) => {
            info!(demo_id, file_name, "Demo stats not yet available (not parsed)");
            // Don't mark as failed — stats service may not have parsed it yet
        }
        Err(e) => {
            let err_msg = format!("Stats fetch failed: {e}");
            warn!(demo_id, file_name, error = %e, "Stats fetch failed");
            api_client.mark_stats_failed(demo_id, &err_msg).await?;
        }
    }

    Ok(())
}
