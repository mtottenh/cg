//! Gaming Portal server entry point.

use anyhow::Result;
use portal_api::{create_app, spawn_timeout_warning_task, AppState, TokenConfig};
use portal_db::{create_pool, PoolConfig};
use std::net::SocketAddr;
use tracing::info;
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
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let pool = create_pool(&database_url, PoolConfig::default()).await?;

    // Run migrations
    info!("Running database migrations...");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await?;
    info!("Migrations complete");

    // JWT secret
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "development-secret-change-in-production".to_string());

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
        token_config.access_token_expiry_minutes,
        token_config.refresh_token_expiry_minutes
    );

    // Create app state
    let state = AppState::new(pool, jwt_secret)
        .with_token_config(token_config);

    // Start background tasks
    spawn_timeout_warning_task(state.clone());

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
    axum::serve(listener, app).await?;

    Ok(())
}
