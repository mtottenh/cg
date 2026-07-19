//! Portal Demo Scanner Daemon.
//!
//! Polls an S3 bucket for new `.dem` files, catalogs them via the portal
//! admin API, and fetches+submits parsed stats from the CS2 demo service.

mod api_client;
mod config;
mod s3_scanner;
mod scanner;
mod stats_converter;

use std::time::Duration;

use anyhow::Result;
use tracing::{error, info};

use api_client::PortalApiClient;
use config::ScannerConfig;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env if present
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("portal_scanner=info")),
        )
        .init();

    let config = ScannerConfig::from_env();

    info!(
        bucket = %config.s3_bucket,
        prefix = %config.s3_prefix,
        scan_interval_secs = config.interval_secs,
        processing_interval_secs = config.processing_interval_secs,
        api_url = %config.api_url,
        game_id = %config.game_id,
        "Starting portal-scanner daemon"
    );

    // Build S3 client
    let mut s3_config_loader =
        aws_config::from_env().region(aws_sdk_s3::config::Region::new(config.s3_region.clone()));
    if let Some(endpoint) = &config.s3_endpoint {
        s3_config_loader = s3_config_loader.endpoint_url(endpoint);
    }
    let aws_config = s3_config_loader.load().await;
    let s3_client = aws_sdk_s3::Client::from_conf(
        aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(config.s3_endpoint.is_some())
            .build(),
    );

    // Build API client
    let api_client = PortalApiClient::new(config.api_url.clone(), config.api_key.clone());

    // Build demo stats client
    let demo_client = portal_plugins::Cs2DemoClient::new(config.demo_service_url.clone());

    info!("Scanner daemon running. Press Ctrl+C to stop.");

    // Task 1: S3 scan loop
    let scan_api_client = api_client.clone();
    let scan_demo_client = demo_client.clone();
    let scan_interval_secs = config.interval_secs;
    let scan_config = config.clone();
    let s3_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(scan_interval_secs));
        loop {
            interval.tick().await;
            info!("Starting S3 scan cycle");

            if let Err(e) = scanner::scan_and_process(
                &s3_client,
                &scan_api_client,
                &scan_demo_client,
                &scan_config,
            )
            .await
            {
                error!(error = %e, "S3 scan cycle failed");
            }

            info!("S3 scan cycle complete");
        }
    });

    // Task 2: Pending demo processing loop
    let processing_api_client = api_client.clone();
    let processing_demo_client = demo_client.clone();
    let processing_interval_secs = config.processing_interval_secs;
    let processing_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(processing_interval_secs));
        loop {
            interval.tick().await;

            if let Err(e) =
                scanner::process_pending(&processing_api_client, &processing_demo_client).await
            {
                error!(error = %e, "Processing pending demos failed");
            }
        }
    });

    // Wait for shutdown or task failure
    tokio::select! {
        _ = s3_handle => {
            error!("S3 scan task exited unexpectedly");
        }
        _ = processing_handle => {
            error!("Processing task exited unexpectedly");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, exiting");
        }
    }

    Ok(())
}
