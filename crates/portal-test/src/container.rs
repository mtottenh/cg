//! Shared `PostgreSQL` container management for testing.
//!
//! Uses testcontainers to spin up a single `PostgreSQL` instance
//! that is shared across all tests in the process.

use std::sync::{Arc, OnceLock};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

/// `PostgreSQL` image tag for testing.
const POSTGRES_TAG: &str = "16-alpine";

/// `PostgreSQL` default port.
const POSTGRES_PORT: u16 = 5432;

/// Shared container instance.
///
/// Using `OnceCell` ensures the container is initialized exactly once,
/// even when multiple tests attempt to access it concurrently.
static SHARED_CONTAINER: OnceCell<Arc<SharedPostgresContainer>> = OnceCell::const_new();

/// Docker container ID for cleanup on process exit.
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

/// Cleanup function called by the C runtime on normal process exit.
/// Removes the Docker container to prevent orphaned containers.
extern "C" fn cleanup_container() {
    if let Some(id) = CONTAINER_ID.get() {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", id])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Register the cleanup function via `libc::atexit`.
#[allow(unsafe_code)]
fn register_cleanup() {
    // SAFETY: cleanup_container is an extern "C" fn that accesses only a
    // global OnceLock read-only, does not panic, and handles errors gracefully.
    unsafe {
        libc::atexit(cleanup_container);
    }
}

/// Wrapper holding the container and connection details.
pub struct SharedPostgresContainer {
    /// The running container handle.
    ///
    /// Kept alive to prevent the container from being dropped.
    _container: ContainerAsync<Postgres>,

    /// Host where the container is accessible.
    host: String,

    /// Mapped port on the host.
    port: u16,
}

impl SharedPostgresContainer {
    /// Get the base URL (without database name) for creating new databases.
    pub fn base_url(&self) -> String {
        format!("postgres://postgres:postgres@{}:{}", self.host, self.port)
    }

    /// Get the admin URL (connects to the "postgres" database).
    pub fn admin_url(&self) -> String {
        format!("{}/postgres", self.base_url())
    }
}

/// Initialize or retrieve the shared `PostgreSQL` container.
///
/// This function is idempotent and thread-safe. The first call will
/// start the container; subsequent calls return the existing instance.
///
/// # Errors
///
/// Returns an error if the container fails to start.
pub async fn get_or_init_container() -> Result<Arc<SharedPostgresContainer>, ContainerError> {
    SHARED_CONTAINER
        .get_or_try_init(|| async { start_container().await })
        .await
        .cloned()
}

/// Start a new `PostgreSQL` container.
async fn start_container() -> Result<Arc<SharedPostgresContainer>, ContainerError> {
    tracing::info!(
        "Starting shared PostgreSQL container (tag: {})",
        POSTGRES_TAG
    );

    let postgres = Postgres::default().with_tag(POSTGRES_TAG);

    let container = postgres
        .start()
        .await
        .map_err(ContainerError::StartFailed)?;

    // Store container ID and register atexit cleanup so the container is
    // removed when the test process exits normally.
    CONTAINER_ID.set(container.id().to_string()).ok();
    register_cleanup();

    let host = container
        .get_host()
        .await
        .map_err(ContainerError::HostResolutionFailed)?
        .to_string();

    let port = container
        .get_host_port_ipv4(POSTGRES_PORT)
        .await
        .map_err(ContainerError::PortMappingFailed)?;

    tracing::info!("PostgreSQL container ready at {}:{}", host, port);

    Ok(Arc::new(SharedPostgresContainer {
        _container: container,
        host,
        port,
    }))
}

/// Errors that can occur during container management.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// Failed to start the `PostgreSQL` container.
    #[error("Failed to start PostgreSQL container: {0}")]
    StartFailed(#[source] testcontainers::TestcontainersError),

    /// Failed to resolve the container host.
    #[error("Failed to resolve container host: {0}")]
    HostResolutionFailed(#[source] testcontainers::TestcontainersError),

    /// Failed to get the port mapping.
    #[error("Failed to get port mapping: {0}")]
    PortMappingFailed(#[source] testcontainers::TestcontainersError),
}
