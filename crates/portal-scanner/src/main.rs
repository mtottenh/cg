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
        interval_secs = config.interval_secs,
        api_url = %config.api_url,
        game_id = %config.game_id,
        "Starting portal-scanner daemon"
    );

    // Build S3 client
    let mut s3_config_loader = aws_config::from_env().region(
        aws_sdk_s3::config::Region::new(config.s3_region.clone()),
    );
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
    let api_client = PortalApiClient::new(config.api_url.clone(), config.api_token.clone());

    // Build demo stats client
    let demo_client = portal_plugins::Cs2DemoClient::new(config.demo_service_url.clone());

    // Main poll loop
    let mut interval = tokio::time::interval(Duration::from_secs(config.interval_secs));

    info!("Scanner daemon running. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!("Starting scan cycle");

                if let Err(e) = scanner::scan_and_process(
                    &s3_client,
                    &api_client,
                    &demo_client,
                    &config,
                ).await {
                    error!(error = %e, "Scan cycle failed");
                }

                if let Err(e) = scanner::process_pending(&api_client, &demo_client).await {
                    error!(error = %e, "Processing pending demos failed");
                }

                info!("Scan cycle complete");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal, exiting");
                break;
            }
        }
    }

    Ok(())
}
