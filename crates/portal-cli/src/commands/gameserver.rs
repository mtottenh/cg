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

    /// Register a game server (idempotent by name: an existing server with
    /// this name is reused and its id printed — built for Ansible)
    Create {
        /// Display name; doubles as the idempotency key
        #[arg(long)]
        name: String,
        /// Game slug the server hosts (resolved against the games table)
        #[arg(long, default_value = "cs2")]
        game_slug: String,
        /// Public IPv4/IPv6 address players connect to
        #[arg(long)]
        ip: String,
        /// Game port (also the RCON port on the server host)
        #[arg(long, default_value_t = 27015)]
        port: u16,
        /// GOTV port, if GOTV is enabled
        #[arg(long)]
        gotv_port: Option<u16>,
        /// Region label used by allocation
        #[arg(long, default_value = "eu")]
        region: String,
        /// Print only the server id (for scripting)
        #[arg(long)]
        quiet: bool,
    },

    /// Mint a one-time enrollment token for a server (invalidates any previous)
    EnrollToken {
        /// Game server ID (UUID)
        id: String,
        /// Print only the token (for scripting)
        #[arg(long)]
        quiet: bool,
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
            GameServerSubcommand::Create {
                name,
                game_slug,
                ip,
                port,
                gotv_port,
                region,
                quiet,
            } => create(pool, name, game_slug, ip, *port, *gotv_port, region, *quiet).await,
            GameServerSubcommand::EnrollToken { id, quiet } => enroll_token(pool, id, *quiet).await,
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

/// Register a server, reusing an existing row with the same name. Names
/// carry no uniqueness constraint in the schema, so this is a tooling-level
/// convention: our Ansible inventory names are unique, and idempotent
/// converges must not mint duplicate registry rows.
#[allow(clippy::too_many_arguments)]
async fn create(
    pool: &PgPool,
    name: &str,
    game_slug: &str,
    ip: &str,
    port: u16,
    gotv_port: Option<u16>,
    region: &str,
    quiet: bool,
) -> Result<()> {
    if let Some((id,)) =
        sqlx::query_as::<_, (uuid::Uuid,)>("SELECT id FROM game_servers WHERE name = $1")
            .fetch_optional(pool)
            .await
            .context("looking up game server by name")?
    {
        if !quiet {
            info(&format!("server {name:?} already registered"));
        }
        println!("{id}");
        return Ok(());
    }

    let game_id: uuid::Uuid =
        sqlx::query_as::<_, (uuid::Uuid,)>("SELECT id FROM games WHERE slug = $1")
            .fetch_optional(pool)
            .await
            .context("resolving game slug")?
            .map(|(id,)| id)
            .with_context(|| {
                format!("no game with slug {game_slug:?} — seed the games table first")
            })?;

    let id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO game_servers (id, name, game_id, ip_address, port, gotv_port, region) \
         VALUES ($1, $2, $3, $4::inet, $5, $6, $7)",
    )
    .bind(id)
    .bind(name)
    .bind(game_id)
    .bind(ip)
    .bind(i32::from(port))
    .bind(gotv_port.map(i32::from))
    .bind(region)
    .execute(pool)
    .await
    .context("inserting game server")?;

    if !quiet {
        success(&format!("registered game server {name:?}"));
    }
    println!("{id}");
    Ok(())
}

async fn enroll_token(pool: &PgPool, id: &str, quiet: bool) -> Result<()> {
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

    if quiet {
        println!("{token}");
        return Ok(());
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
