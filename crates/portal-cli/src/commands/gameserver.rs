//! Game-server integration commands (MatchZy).
//!
//! `ca-init` bootstraps the portal CA that signs agent client certificates
//! (docs/matchzy-integration.md §5.3). `enroll-token` mints the one-time
//! token an operator feeds to `portal-server-agent enroll`. Registry rows
//! are normally managed through the admin UI; `list`/`revoke` exist for
//! headless operation.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use portal_db::PgPool;
use portal_domain::services::game_server::{
    CertificateAuthority, ENROLLMENT_TOKEN_TTL_HOURS, generate_enrollment_token, hash_token,
};
use serde::Serialize;
use tabled::Tabled;

use crate::output::{OutputFormat, format_timestamp, info, output_list, success};

/// Game-server integration commands.
#[derive(Args)]
pub struct GameServerCommand {
    #[command(subcommand)]
    command: GameServerSubcommand,
}

#[derive(Subcommand)]
enum GameServerSubcommand {
    /// Generate the portal CA for agent certificates (writes ca.pem + ca.key)
    CaInit {
        /// Directory to write the CA material into (e.g. /etc/portal/agent-ca)
        #[arg(long)]
        dir: String,
        /// CA common name
        #[arg(long, default_value = "portal-agent-ca")]
        common_name: String,
        /// Overwrite existing CA material
        #[arg(long)]
        force: bool,
    },

    /// List registered game servers
    List,

    /// Mint a one-time enrollment token for a server (invalidates any previous)
    EnrollToken {
        /// Game server ID (UUID)
        id: String,
    },

    /// Revoke a server's agent certificates
    Revoke {
        /// Game server ID (UUID)
        id: String,
    },
}

impl GameServerCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            GameServerSubcommand::CaInit {
                dir,
                common_name,
                force,
            } => ca_init(dir, common_name, *force),
            GameServerSubcommand::List => list_servers(pool, format).await,
            GameServerSubcommand::EnrollToken { id } => enroll_token(pool, id).await,
            GameServerSubcommand::Revoke { id } => revoke(pool, id).await,
        }
    }
}

fn ca_init(dir: &str, common_name: &str, force: bool) -> Result<()> {
    let cert_path = format!("{dir}/ca.pem");
    let key_path = format!("{dir}/ca.key");
    if !force
        && (std::path::Path::new(&cert_path).exists() || std::path::Path::new(&key_path).exists())
    {
        bail!("CA material already exists in {dir}; pass --force to overwrite");
    }

    let ca = CertificateAuthority::generate(common_name)
        .map_err(|e| anyhow::anyhow!("CA generation failed: {e}"))?;

    std::fs::create_dir_all(dir).with_context(|| format!("creating {dir}"))?;
    std::fs::write(&cert_path, &ca.cert_pem).with_context(|| format!("writing {cert_path}"))?;
    std::fs::write(&key_path, &ca.key_pem).with_context(|| format!("writing {key_path}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod 600 {key_path}"))?;
    }

    success(&format!("CA written to {cert_path} and {key_path}"));
    info("Point PORTAL_AGENT_CA_DIR at this directory and give ca.pem to Caddy's client_auth.");
    Ok(())
}

/// Row shape for game_servers listing.
type ServerRow = (
    uuid::Uuid,
    String,
    String,
    i32,
    String,
    String,
    bool,
    Option<chrono::DateTime<chrono::Utc>>,
);

/// Table row for server display.
#[derive(Tabled, Serialize)]
struct ServerTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Address")]
    address: String,
    #[tabled(rename = "Region")]
    region: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Enabled")]
    enabled: String,
    #[tabled(rename = "Last Heartbeat")]
    last_heartbeat: String,
}

async fn list_servers(pool: &PgPool, format: OutputFormat) -> Result<()> {
    let rows: Vec<ServerRow> = sqlx::query_as(
        "SELECT id, name, host(ip_address), port, region, status, enabled, last_heartbeat_at \
         FROM game_servers ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .context("listing game servers")?;

    let table_rows: Vec<ServerTableRow> = rows
        .into_iter()
        .map(
            |(id, name, ip, port, region, status, enabled, heartbeat)| ServerTableRow {
                id: format_uuid_short(&id),
                name,
                address: format!("{ip}:{port}"),
                region,
                status,
                enabled: if enabled { "yes".into() } else { "no".into() },
                last_heartbeat: heartbeat
                    .map_or_else(|| "never".into(), |ts| format_timestamp(&ts)),
            },
        )
        .collect();

    output_list(&table_rows, format)?;
    Ok(())
}

fn format_uuid_short(id: &uuid::Uuid) -> String {
    id.to_string()
}

async fn enroll_token(pool: &PgPool, id: &str) -> Result<()> {
    let server_id: uuid::Uuid = id.parse().context("invalid server id")?;

    let token = generate_enrollment_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(ENROLLMENT_TOKEN_TTL_HOURS);

    let updated = sqlx::query(
        "UPDATE game_servers SET enrollment_token_hash = $2, \
         enrollment_token_expires_at = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(server_id)
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(pool)
    .await
    .context("storing enrollment token")?;

    if updated.rows_affected() == 0 {
        bail!("no game server with id {id}");
    }

    success("Enrollment token minted (shown once, valid 24h):");
    println!("{token}");
    info(&format!(
        "On the game host: portal-server-agent enroll --token {token} --url https://<portal>"
    ));
    Ok(())
}

async fn revoke(pool: &PgPool, id: &str) -> Result<()> {
    let server_id: uuid::Uuid = id.parse().context("invalid server id")?;

    let revoked = sqlx::query(
        "UPDATE server_agent_certs SET revoked_at = NOW() \
         WHERE server_id = $1 AND revoked_at IS NULL",
    )
    .bind(server_id)
    .execute(pool)
    .await
    .context("revoking certificates")?;

    sqlx::query(
        "UPDATE game_servers SET agent_cert_serial = NULL, agent_cert_expires_at = NULL, \
         status = 'offline', updated_at = NOW() WHERE id = $1",
    )
    .bind(server_id)
    .execute(pool)
    .await
    .context("clearing server cert fields")?;

    success(&format!(
        "Revoked {} certificate(s); the agent must re-enroll",
        revoked.rows_affected()
    ));
    Ok(())
}
