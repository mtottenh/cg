//! Output formatting utilities.

use anyhow::Result;
use clap::ValueEnum;
use colored::Colorize;
use serde::Serialize;
use std::fmt::Display;
use tabled::{settings::Style, Table, Tabled};

/// Output format for CLI results.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format
    #[default]
    Table,
    /// JSON format for scripting
    Json,
    /// YAML format
    Yaml,
}

/// Trait for types that can be displayed in a table.
pub trait TableDisplay: Tabled + Serialize {}
impl<T: Tabled + Serialize> TableDisplay for T {}

/// Output a single item.
pub fn output_item<T: Serialize + Display>(item: &T, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            println!("{item}");
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(item)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(item)?);
        }
    }
    Ok(())
}

/// Output a list of items.
pub fn output_list<T: TableDisplay>(items: &[T], format: OutputFormat) -> Result<()> {
    if items.is_empty() {
        println!("{}", "No results found.".dimmed());
        return Ok(());
    }

    match format {
        OutputFormat::Table => {
            let table = Table::new(items).with(Style::rounded()).to_string();
            println!("{table}");
            println!("\n{} item(s)", items.len().to_string().cyan());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(items)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(items)?);
        }
    }
    Ok(())
}

/// Print a success message.
pub fn success(message: &str) {
    println!("{} {}", "✓".green().bold(), message);
}

/// Print an error message.
pub fn error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message);
}

/// Print a warning message.
pub fn warn(message: &str) {
    println!("{} {}", "⚠".yellow().bold(), message);
}

/// Print an info message.
pub fn info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message);
}

/// Format a UUID for display (shortened).
pub fn format_uuid(uuid: &uuid::Uuid) -> String {
    let s = uuid.to_string();
    format!("{}...{}", &s[..8], &s[s.len() - 4..])
}

/// Format a timestamp for display.
pub fn format_timestamp(ts: &chrono::DateTime<chrono::Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format an optional value.
pub fn format_optional<T: Display>(value: &Option<T>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "-".dimmed().to_string(),
    }
}

/// Table row for user display.
#[derive(Tabled, Serialize)]
pub struct UserTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Username")]
    pub username: String,
    #[tabled(rename = "Email")]
    pub email: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "2FA")]
    pub two_factor: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

/// Table row for role display.
#[derive(Tabled, Serialize)]
pub struct RoleTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Display Name")]
    pub display_name: String,
    #[tabled(rename = "Category")]
    pub category: String,
    #[tabled(rename = "Priority")]
    pub priority: i32,
    #[tabled(rename = "System")]
    pub is_system: String,
}

/// Table row for player display.
#[derive(Tabled, Serialize)]
pub struct PlayerTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Display Name")]
    pub display_name: String,
    #[tabled(rename = "Country")]
    pub country: String,
    #[tabled(rename = "Steam ID")]
    pub steam_id: String,
    #[tabled(rename = "Created")]
    pub created_at: String,
}

/// Table row for team display.
#[derive(Tabled, Serialize)]
pub struct TeamTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Tag")]
    pub tag: String,
    #[tabled(rename = "Game")]
    pub game: String,
    #[tabled(rename = "Status")]
    pub status: String,
    #[tabled(rename = "Members")]
    pub member_count: i32,
}

/// Table row for game display.
#[derive(Tabled, Serialize)]
pub struct GameTableRow {
    #[tabled(rename = "ID")]
    pub id: String,
    #[tabled(rename = "Name")]
    pub display_name: String,
    #[tabled(rename = "Team Size")]
    pub team_size: i32,
    #[tabled(rename = "Status")]
    pub status: String,
}
