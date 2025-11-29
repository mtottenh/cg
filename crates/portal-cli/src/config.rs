//! CLI configuration management.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// CLI configuration.
#[derive(Debug, Deserialize)]
pub struct CliConfig {
    /// Database connection URL.
    pub database_url: String,
}

impl CliConfig {
    /// Load configuration from file and/or environment.
    pub fn load(config_path: Option<&str>, database_url: Option<&str>) -> Result<Self> {
        // Priority: CLI arg > env var > config file > default locations
        let database_url = if let Some(url) = database_url {
            url.to_string()
        } else if let Ok(url) = std::env::var("DATABASE_URL") {
            url
        } else if let Some(path) = config_path {
            Self::load_from_file(path)?.database_url
        } else {
            // Try default config locations
            Self::try_default_locations()?
                .map(|c| c.database_url)
                .context("DATABASE_URL not set. Use --database-url, DATABASE_URL env var, or config file.")?
        };

        Ok(Self { database_url })
    }

    /// Load configuration from a specific file.
    fn load_from_file(path: &str) -> Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::with_name(path))
            .build()
            .context("Failed to load config file")?;

        config
            .try_deserialize()
            .context("Failed to parse config file")
    }

    /// Try to load from default config locations.
    fn try_default_locations() -> Result<Option<Self>> {
        let mut locations: Vec<std::path::PathBuf> = vec![
            ".portal-cli.toml".into(),
            "portal-cli.toml".into(),
        ];

        if let Some(home) = dirs::home_dir() {
            locations.push(home.join(".portal-cli.toml"));
        }
        if let Some(config) = dirs::config_dir() {
            locations.push(config.join("portal-cli").join("config.toml"));
        }

        for location in &locations {
            if location.exists() {
                if let Some(path_str) = location.to_str() {
                    return Ok(Some(Self::load_from_file(path_str)?));
                }
            }
        }

        Ok(None)
    }
}

/// Helper module for finding home/config directories.
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|p| p.join(".config")))
    }
}
