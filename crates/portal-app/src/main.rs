//! Gaming Portal server entry point.

use anyhow::Result;
use portal_api::{AppState, TokenConfig, create_app, spawn_timeout_warning_task};
use portal_db::{PoolConfig, create_pool};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "portal=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Database connection
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = create_pool(&database_url, PoolConfig::default()).await?;

    // Run migrations
    info!("Running database migrations...");
    sqlx::migrate!("../../migrations").run(&pool).await?;
    info!("Migrations complete");

    // JWT secret — no fallback. Missing or weak secrets must hard-fail at startup
    // so a misconfigured deployment cannot serve traffic with a known signing key.
    let jwt_secret =
        std::env::var("JWT_SECRET").map_err(|_| anyhow::anyhow!("JWT_SECRET must be set"))?;
    if jwt_secret.len() < 32 {
        anyhow::bail!(
            "JWT_SECRET must be at least 32 bytes (got {})",
            jwt_secret.len()
        );
    }

    // Token expiry configuration
    let token_config = TokenConfig {
        access_token_expiry_minutes: std::env::var("ACCESS_TOKEN_EXPIRY_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15),
        refresh_token_expiry_minutes: std::env::var("REFRESH_TOKEN_EXPIRY_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10080), // 7 days
    };
    info!(
        "Token config: access={}min, refresh={}min",
        token_config.access_token_expiry_minutes, token_config.refresh_token_expiry_minutes
    );

    // Create app state
    let state = AppState::new(pool, jwt_secret)
        .await
        .with_token_config(token_config);

    // Shutdown signalling shared between the HTTP server's
    // with_graceful_shutdown future and the background timeout task.
    let shutdown = Arc::new(Notify::new());

    // Start background tasks. We hold the JoinHandle so we can await it on
    // shutdown — previously the handle was dropped and a panic in the loop
    // would be silently swallowed.
    let timeout_handle = spawn_timeout_warning_task(state.clone(), Arc::clone(&shutdown));

    // Keep a handle to the pool so we can drain it after the server stops.
    let pool_for_shutdown = state.db_pool.clone();

    // Create app
    let app = create_app(state);

    // Start server
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Wait for either Ctrl-C (SIGINT) or SIGTERM, signal background tasks to
    // stop, drain in-flight requests, then close the pool.
    let shutdown_for_serve = Arc::clone(&shutdown);
    // `into_make_service_with_connect_info::<SocketAddr>()` (vs plain
    // `app` / `into_make_service`) is what populates the `ConnectInfo<SocketAddr>`
    // extension on every request. `tower_governor`'s `SmartIpKeyExtractor` needs
    // that to pull the peer IP out of the connection; without it every
    // rate-limited route returns 500 "Unable To Extract Key!".
    let server_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        wait_for_shutdown_signal().await;
        info!("shutdown signal received; draining in-flight requests");
        shutdown_for_serve.notify_waiters();
    })
    .await;

    if let Err(e) = server_result {
        warn!(error = %e, "axum::serve returned error during shutdown");
    }

    // Make sure the timeout task is signalled even if the server exited some
    // other way (e.g. the listener died).
    shutdown.notify_waiters();

    // Bound the wait for the background task so we don't hang shutdown if it
    // is wedged on a slow DB query.
    match tokio::time::timeout(std::time::Duration::from_secs(10), timeout_handle).await {
        Ok(Ok(())) => info!("timeout warning task exited cleanly"),
        Ok(Err(e)) => warn!(error = %e, "timeout warning task panicked"),
        Err(_) => warn!("timeout warning task did not exit within 10s; abandoning"),
    }

    info!("closing database pool");
    pool_for_shutdown.close().await;

    info!("shutdown complete");
    Ok(())
}

/// Resolves when the process is asked to terminate. SIGINT (Ctrl-C) on all
/// platforms; SIGTERM additionally on Unix. Whichever fires first wins.
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received SIGINT"),
        () = terminate => info!("received SIGTERM"),
    }
}
