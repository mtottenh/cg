//! Audit log viewing commands.
//!
//! Provides commands for viewing entity changes and audit trails:
//! - View changes by entity
//! - View changes by user
//! - Search audit logs

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use portal_db::entities::EntityChangeRow;
use portal_db::PgPool;
use serde::Serialize;
use tabled::Tabled;
use uuid::Uuid;

use crate::output::{format_optional, format_timestamp, format_uuid, output_list, OutputFormat};

/// Audit log viewing commands.
#[derive(Args)]
pub struct AuditCommand {
    #[command(subcommand)]
    command: AuditSubcommand,
}

#[derive(Subcommand)]
enum AuditSubcommand {
    /// List recent audit log entries
    List {
        /// Filter by entity type (user, player, league, team, etc.)
        #[arg(long)]
        entity_type: Option<String>,

        /// Filter by change type (create, update, delete)
        #[arg(long)]
        change_type: Option<String>,

        /// Filter by user who made the change
        #[arg(long)]
        changed_by: Option<Uuid>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get details of a specific audit entry
    Get {
        /// Audit entry ID
        id: Uuid,
    },

    /// View audit history for a specific entity
    Entity {
        /// Entity type (user, player, league, team, etc.)
        entity_type: String,

        /// Entity ID
        entity_id: Uuid,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// View changes made by a specific user
    User {
        /// User ID who made changes
        user_id: Uuid,

        /// Filter by entity type
        #[arg(long)]
        entity_type: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Search audit logs
    Search {
        /// Search term (searches entity type, field names)
        query: String,

        /// Filter by date range start
        #[arg(long)]
        from: Option<DateTime<Utc>>,

        /// Filter by date range end
        #[arg(long)]
        to: Option<DateTime<Utc>>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get summary statistics for audit logs
    Stats {
        /// Number of days to look back
        #[arg(long, default_value = "30")]
        days: i32,
    },
}

impl AuditCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            AuditSubcommand::List {
                entity_type,
                change_type,
                changed_by,
                limit,
            } => {
                list_audit_entries(
                    pool,
                    entity_type.as_deref(),
                    change_type.as_deref(),
                    *changed_by,
                    *limit,
                    format,
                )
                .await
            }
            AuditSubcommand::Get { id } => get_audit_entry(pool, *id, format).await,
            AuditSubcommand::Entity {
                entity_type,
                entity_id,
                limit,
            } => get_entity_history(pool, entity_type, *entity_id, *limit, format).await,
            AuditSubcommand::User {
                user_id,
                entity_type,
                limit,
            } => get_user_changes(pool, *user_id, entity_type.as_deref(), *limit, format).await,
            AuditSubcommand::Search {
                query,
                from,
                to,
                limit,
            } => search_audit_logs(pool, query, *from, *to, *limit, format).await,
            AuditSubcommand::Stats { days } => get_audit_stats(pool, *days, format).await,
        }
    }
}

/// Table row for audit log display.
#[derive(Tabled, Serialize)]
pub struct AuditTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Entity")]
    pub entity_type: String,
    #[tabled(rename = "Entity ID")]
    pub entity_id: String,
    #[tabled(rename = "Change")]
    pub change_type: String,
    #[tabled(rename = "Field")]
    pub field_name: String,
    #[tabled(rename = "Changed By")]
    pub changed_by: String,
    #[tabled(rename = "When")]
    pub created_at: String,
}

fn row_to_table_row(row: &EntityChangeRow) -> AuditTableRow {
    AuditTableRow {
        id: format_uuid(&row.id),
        entity_type: row.entity_type.clone(),
        entity_id: format_uuid(&row.entity_id),
        change_type: row.change_type.clone(),
        field_name: row.field_name.clone().unwrap_or_else(|| "-".to_string()),
        changed_by: format_uuid(&row.changed_by),
        created_at: format_timestamp(&row.created_at),
    }
}

async fn list_audit_entries(
    pool: &PgPool,
    entity_type: Option<&str>,
    change_type: Option<&str>,
    changed_by: Option<Uuid>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let rows = sqlx::query_as::<_, EntityChangeRow>(
        r"
        SELECT * FROM entity_changes
        WHERE ($1::text IS NULL OR entity_type = $1)
          AND ($2::text IS NULL OR change_type = $2)
          AND ($3::uuid IS NULL OR changed_by = $3)
        ORDER BY created_at DESC
        LIMIT $4
        ",
    )
    .bind(entity_type)
    .bind(change_type)
    .bind(changed_by)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch audit entries")?;

    let table_rows: Vec<AuditTableRow> = rows.iter().map(row_to_table_row).collect();
    output_list(&table_rows, format)
}

async fn get_audit_entry(pool: &PgPool, id: Uuid, format: OutputFormat) -> Result<()> {
    let row = sqlx::query_as::<_, EntityChangeRow>(
        "SELECT * FROM entity_changes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch audit entry")?;

    if let Some(entry) = row {
        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": entry.id,
                    "entity_type": entry.entity_type,
                    "entity_id": entry.entity_id,
                    "change_type": entry.change_type,
                    "field_name": entry.field_name,
                    "old_value": entry.old_value,
                    "new_value": entry.new_value,
                    "changed_by": entry.changed_by,
                    "reverted_at": entry.reverted_at,
                    "reverted_by": entry.reverted_by,
                    "revert_reason": entry.revert_reason,
                    "request_id": entry.request_id,
                    "ip_address": entry.ip_address,
                    "user_agent": entry.user_agent,
                    "created_at": entry.created_at,
                }))?
            );
        } else {
            println!("Audit Entry Details:");
            println!("  ID:           {}", entry.id);
            println!("  Entity Type:  {}", entry.entity_type);
            println!("  Entity ID:    {}", entry.entity_id);
            println!("  Change Type:  {}", entry.change_type);
            println!("  Field Name:   {}", format_optional(&entry.field_name));
            println!(
                "  Old Value:    {}",
                entry
                    .old_value
                    .as_ref().map_or_else(|| "-".to_string(), std::string::ToString::to_string)
            );
            println!(
                "  New Value:    {}",
                entry
                    .new_value
                    .as_ref().map_or_else(|| "-".to_string(), std::string::ToString::to_string)
            );
            println!("  Changed By:   {}", entry.changed_by);
            println!("  Created At:   {}", format_timestamp(&entry.created_at));
            println!("  Request ID:   {}", format_optional(&entry.request_id));
            println!("  IP Address:   {}", format_optional(&entry.ip_address));
            println!("  User Agent:   {}", format_optional(&entry.user_agent));
            if let Some(reverted) = entry.reverted_at {
                println!("  Reverted At:  {}", format_timestamp(&reverted));
                println!(
                    "  Reverted By:  {}",
                    entry
                        .reverted_by.map_or_else(|| "-".to_string(), |id| id.to_string())
                );
                println!(
                    "  Revert Reason: {}",
                    format_optional(&entry.revert_reason)
                );
            }
        }
        Ok(())
    } else {
        anyhow::bail!("Audit entry not found: {id}")
    }
}

async fn get_entity_history(
    pool: &PgPool,
    entity_type: &str,
    entity_id: Uuid,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let rows = sqlx::query_as::<_, EntityChangeRow>(
        r"
        SELECT * FROM entity_changes
        WHERE entity_type = $1 AND entity_id = $2
        ORDER BY created_at DESC
        LIMIT $3
        ",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch entity history")?;

    if rows.is_empty() {
        println!("No audit history found for {entity_type} {entity_id}");
        return Ok(());
    }

    let table_rows: Vec<AuditTableRow> = rows.iter().map(row_to_table_row).collect();
    output_list(&table_rows, format)
}

async fn get_user_changes(
    pool: &PgPool,
    user_id: Uuid,
    entity_type: Option<&str>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let rows = sqlx::query_as::<_, EntityChangeRow>(
        r"
        SELECT * FROM entity_changes
        WHERE changed_by = $1
          AND ($2::text IS NULL OR entity_type = $2)
        ORDER BY created_at DESC
        LIMIT $3
        ",
    )
    .bind(user_id)
    .bind(entity_type)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch user changes")?;

    if rows.is_empty() {
        println!("No changes found for user {user_id}");
        return Ok(());
    }

    let table_rows: Vec<AuditTableRow> = rows.iter().map(row_to_table_row).collect();
    output_list(&table_rows, format)
}

async fn search_audit_logs(
    pool: &PgPool,
    query: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let search_pattern = format!("%{query}%");

    let rows = sqlx::query_as::<_, EntityChangeRow>(
        r"
        SELECT * FROM entity_changes
        WHERE (entity_type ILIKE $1 OR field_name ILIKE $1 OR change_type ILIKE $1)
          AND ($2::timestamptz IS NULL OR created_at >= $2)
          AND ($3::timestamptz IS NULL OR created_at <= $3)
        ORDER BY created_at DESC
        LIMIT $4
        ",
    )
    .bind(&search_pattern)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to search audit logs")?;

    if rows.is_empty() {
        println!("No audit entries found matching '{query}'");
        return Ok(());
    }

    let table_rows: Vec<AuditTableRow> = rows.iter().map(row_to_table_row).collect();
    output_list(&table_rows, format)
}

/// Statistics row for audit summary.
#[derive(Debug, sqlx::FromRow)]
struct AuditStatRow {
    entity_type: String,
    change_type: String,
    count: i64,
}

/// Statistics table row.
#[derive(Tabled, Serialize)]
struct StatsTableRow {
    #[tabled(rename = "Entity Type")]
    entity_type: String,
    #[tabled(rename = "Change Type")]
    change_type: String,
    #[tabled(rename = "Count")]
    count: i64,
}

async fn get_audit_stats(pool: &PgPool, days: i32, format: OutputFormat) -> Result<()> {
    // Get counts by entity type and change type
    let rows = sqlx::query_as::<_, AuditStatRow>(
        r"
        SELECT entity_type, change_type, COUNT(*) as count
        FROM entity_changes
        WHERE created_at >= NOW() - INTERVAL '1 day' * $1
        GROUP BY entity_type, change_type
        ORDER BY count DESC
        ",
    )
    .bind(days)
    .fetch_all(pool)
    .await
    .context("Failed to fetch audit statistics")?;

    // Get total count
    let total: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM entity_changes
        WHERE created_at >= NOW() - INTERVAL '1 day' * $1
        ",
    )
    .bind(days)
    .fetch_one(pool)
    .await
    .context("Failed to fetch total count")?;

    // Get unique users count
    let unique_users: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(DISTINCT changed_by) FROM entity_changes
        WHERE created_at >= NOW() - INTERVAL '1 day' * $1
        ",
    )
    .bind(days)
    .fetch_one(pool)
    .await
    .context("Failed to fetch unique users count")?;

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "period_days": days,
                "total_changes": total.0,
                "unique_users": unique_users.0,
                "breakdown": rows.iter().map(|r| {
                    serde_json::json!({
                        "entity_type": r.entity_type,
                        "change_type": r.change_type,
                        "count": r.count,
                    })
                }).collect::<Vec<_>>(),
            }))?
        );
    } else {
        println!("Audit Statistics (last {days} days)");
        println!();
        println!("  Total Changes:  {}", total.0);
        println!("  Unique Users:   {}", unique_users.0);
        println!();

        if !rows.is_empty() {
            println!("Breakdown by Entity Type and Change Type:");
            let table_rows: Vec<StatsTableRow> = rows
                .into_iter()
                .map(|r| StatsTableRow {
                    entity_type: r.entity_type,
                    change_type: r.change_type,
                    count: r.count,
                })
                .collect();
            output_list(&table_rows, format)?;
        }
    }

    Ok(())
}
