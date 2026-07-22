//! API key management commands.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use portal_db::PgPool;
use rand::Rng;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tabled::Tabled;

use crate::output::{
    OutputFormat, error, format_timestamp, format_uuid, info, output_list, success,
};

/// Row shape for a single `api_keys` lookup:
/// (id, service_name, key_prefix, is_active, expires_at, last_used_at, created_at).
type ApiKeyRow = (
    uuid::Uuid,
    String,
    String,
    bool,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    chrono::DateTime<chrono::Utc>,
);

/// API key management commands.
#[derive(Args)]
pub struct ApiKeyCommand {
    #[command(subcommand)]
    command: ApiKeySubcommand,
}

#[derive(Subcommand)]
enum ApiKeySubcommand {
    /// Create a new API key for a service
    Create {
        /// Service name (e.g. cs2-poller, cs2-enricher)
        #[arg(long)]
        service: String,
        /// Comma-separated permissions (e.g. steam_tracking.read,discovered_matches.write)
        #[arg(long)]
        permissions: String,
    },

    /// List all API keys
    List,

    /// Show details for a single API key
    Get {
        /// API key ID (UUID)
        id: String,
    },

    /// Deactivate an API key
    Deactivate {
        /// API key ID (UUID)
        id: String,
    },
}

impl ApiKeyCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            ApiKeySubcommand::Create {
                service,
                permissions,
            } => create_key(pool, service, permissions).await,
            ApiKeySubcommand::List => list_keys(pool, format).await,
            ApiKeySubcommand::Get { id } => get_key(pool, id, format).await,
            ApiKeySubcommand::Deactivate { id } => deactivate_key(pool, id).await,
        }
    }
}

/// Table row for API key display.
#[derive(Tabled, Serialize)]
struct ApiKeyTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Service")]
    service_name: String,
    #[tabled(rename = "Prefix")]
    key_prefix: String,
    #[tabled(rename = "Permissions")]
    permissions: String,
    #[tabled(rename = "Active")]
    is_active: String,
    #[tabled(rename = "Created")]
    created_at: String,
}

/// Generate a random API key with `cgp_` prefix.
fn generate_raw_key() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let random_part: String = (0..48)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect();
    format!("cgp_{random_part}")
}

/// Hash a raw API key with SHA-256 (hex-encoded) — matches portal-api extractor.
fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

async fn create_key(pool: &PgPool, service: &str, permissions_str: &str) -> Result<()> {
    let raw_key = generate_raw_key();
    let key_hash = hash_api_key(&raw_key);
    let key_prefix = &raw_key[..8];
    let permissions: Vec<String> = permissions_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Validate permissions against the DB (single source of truth)
    let valid_perms: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM permissions WHERE category = 'service' AND name = ANY($1)",
    )
    .bind(&permissions)
    .fetch_all(pool)
    .await
    .context("Failed to validate permissions")?;

    let valid_names: Vec<&str> = valid_perms.iter().map(|(n,)| n.as_str()).collect();
    let invalid: Vec<&String> = permissions
        .iter()
        .filter(|p| !valid_names.contains(&p.as_str()))
        .collect();
    if !invalid.is_empty() {
        bail!(
            "Unknown service permission(s): {}. Run `portal role list-permissions` to see valid scopes.",
            invalid
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Use a transaction: insert key row, then link permissions
    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    let (id,): (uuid::Uuid,) = sqlx::query_as(
        r"
        INSERT INTO api_keys (service_name, key_hash, key_prefix)
        VALUES ($1, $2, $3)
        RETURNING id
        ",
    )
    .bind(service)
    .bind(&key_hash)
    .bind(key_prefix)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to insert API key")?;

    sqlx::query(
        r"
        INSERT INTO api_key_permissions (api_key_id, permission_id)
        SELECT $1, p.id FROM permissions p WHERE p.name = ANY($2)
        ",
    )
    .bind(id)
    .bind(&permissions)
    .execute(&mut *tx)
    .await
    .context("Failed to link permissions")?;

    tx.commit().await.context("Failed to commit transaction")?;

    success(&format!("Created API key for service '{service}'"));
    println!();
    println!("  ID:          {id}");
    println!("  Service:     {service}");
    println!("  Prefix:      {key_prefix}");
    println!("  Permissions: {}", permissions.join(", "));
    println!();
    println!("  {}", "=".repeat(60));
    println!("  RAW KEY (save this now — it will NOT be shown again):");
    println!();
    println!("    {raw_key}");
    println!();
    println!("  {}", "=".repeat(60));

    Ok(())
}

async fn list_keys(pool: &PgPool, format: OutputFormat) -> Result<()> {
    let rows: Vec<(
        uuid::Uuid,
        String,
        String,
        bool,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r"
        SELECT ak.id, ak.service_name, ak.key_prefix, ak.is_active, ak.created_at
        FROM api_keys ak
        ORDER BY ak.created_at DESC
        ",
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch API keys")?;

    let mut table_rows = Vec::with_capacity(rows.len());
    for (id, service_name, key_prefix, is_active, created_at) in rows {
        let perms: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT p.name
            FROM api_key_permissions akp
            JOIN permissions p ON p.id = akp.permission_id
            WHERE akp.api_key_id = $1
            ",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .context("Failed to fetch permissions for key")?;

        table_rows.push(ApiKeyTableRow {
            id: format_uuid(&id),
            service_name,
            key_prefix,
            permissions: perms
                .into_iter()
                .map(|(n,)| n)
                .collect::<Vec<_>>()
                .join(", "),
            is_active: if is_active { "Yes" } else { "No" }.to_string(),
            created_at: format_timestamp(&created_at),
        });
    }

    output_list(&table_rows, format)
}

async fn get_key(pool: &PgPool, id: &str, format: OutputFormat) -> Result<()> {
    let key_id: uuid::Uuid = id.parse().context("Invalid API key ID")?;

    let row: Option<ApiKeyRow> = sqlx::query_as(
        r"
        SELECT id, service_name, key_prefix, is_active,
               expires_at, last_used_at, created_at
        FROM api_keys
        WHERE id = $1
        ",
    )
    .bind(key_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch API key")?;

    if let Some(k) = row {
        let perms: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT p.name
            FROM api_key_permissions akp
            JOIN permissions p ON p.id = akp.permission_id
            WHERE akp.api_key_id = $1
            ",
        )
        .bind(k.0)
        .fetch_all(pool)
        .await
        .context("Failed to fetch permissions for key")?;

        let perm_names: Vec<String> = perms.into_iter().map(|(n,)| n).collect();

        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": k.0,
                    "service_name": k.1,
                    "key_prefix": k.2,
                    "permissions": perm_names,
                    "is_active": k.3,
                    "expires_at": k.4,
                    "last_used_at": k.5,
                    "created_at": k.6,
                }))?
            );
        } else {
            println!("API Key: {}", k.0);
            println!("  Service:     {}", k.1);
            println!("  Prefix:      {}", k.2);
            println!("  Permissions: {}", perm_names.join(", "));
            println!("  Active:      {}", if k.3 { "Yes" } else { "No" });
            if let Some(exp) = &k.4 {
                println!("  Expires:     {}", format_timestamp(exp));
            }
            if let Some(used) = &k.5 {
                println!("  Last Used:   {}", format_timestamp(used));
            } else {
                info("  Last Used:   Never");
            }
            println!("  Created:     {}", format_timestamp(&k.6));
        }
        Ok(())
    } else {
        error(&format!("API key not found: {id}"));
        std::process::exit(1);
    }
}

async fn deactivate_key(pool: &PgPool, id: &str) -> Result<()> {
    let key_id: uuid::Uuid = id.parse().context("Invalid API key ID")?;

    let result = sqlx::query(
        r"
        UPDATE api_keys
        SET is_active = false, updated_at = NOW()
        WHERE id = $1 AND is_active = true
        ",
    )
    .bind(key_id)
    .execute(pool)
    .await
    .context("Failed to deactivate API key")?;

    if result.rows_affected() == 0 {
        error(&format!("API key not found or already inactive: {id}"));
        std::process::exit(1);
    }

    success(&format!("Deactivated API key {id}"));
    Ok(())
}
