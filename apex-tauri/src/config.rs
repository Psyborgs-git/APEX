use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::commands::python_runtime::RuntimePaths;

const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../config/apex.example.toml");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub market_data: MarketDataConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub risk: RiskSection,
    #[serde(default)]
    pub storage: StorageConfig,
}

impl AppConfig {
    pub fn load(runtime_paths: &RuntimePaths) -> Result<Self> {
        ensure_config_file(runtime_paths)?;

        let config_path = runtime_paths.config_file();
        let raw = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file {}", config_path.display()))?;

        toml::from_str(&raw)
            .with_context(|| format!("Failed to parse config file {}", config_path.display()))
    }

    pub fn resolved_data_dir(&self, runtime_paths: &RuntimePaths) -> Result<PathBuf> {
        resolve_runtime_dir(
            runtime_paths,
            self.general.data_dir.trim(),
            runtime_paths.data_dir(),
        )
    }

    pub fn resolved_sqlite_path(&self, runtime_paths: &RuntimePaths) -> Result<PathBuf> {
        let data_dir = self.resolved_data_dir(runtime_paths)?;
        let configured = self.storage.sqlite_path.trim();
        let sqlite_path = if configured.is_empty() {
            data_dir.join("apex.db")
        } else {
            let path = PathBuf::from(configured);
            if path.is_absolute() {
                path
            } else {
                data_dir.join(path)
            }
        };

        if let Some(parent) = sqlite_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create SQLite parent directory {}",
                    parent.display()
                )
            })?;
        }

        Ok(sqlite_path)
    }

    pub fn risk_config(&self) -> apex_core::application::risk_engine::RiskConfig {
        let defaults = apex_core::application::risk_engine::RiskConfig::default();
        apex_core::application::risk_engine::RiskConfig {
            max_daily_loss: self.risk.max_daily_loss,
            max_order_value: self.risk.max_order_value,
            ..defaults
        }
    }
}

fn ensure_config_file(runtime_paths: &RuntimePaths) -> Result<()> {
    let config_path = runtime_paths.config_file();
    if config_path.exists() {
        return Ok(());
    }

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create config directory {}", parent.display())
        })?;
    }

    fs::write(config_path, DEFAULT_CONFIG_TEMPLATE).with_context(|| {
        format!(
            "Failed to create default config file at {}",
            config_path.display()
        )
    })?;

    Ok(())
}

fn resolve_runtime_dir(
    runtime_paths: &RuntimePaths,
    configured: &str,
    fallback: &Path,
) -> Result<PathBuf> {
    let path = if configured.is_empty() {
        fallback.to_path_buf()
    } else {
        runtime_paths.resolve_user_relative_path(configured)
    };

    fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create runtime directory {}", path.display()))?;

    Ok(path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataConfig {
    #[serde(default = "default_market_data_adapter")]
    pub adapter: String,
}

impl Default for MarketDataConfig {
    fn default() -> Self {
        Self {
            adapter: default_market_data_adapter(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_execution_adapter")]
    pub adapter: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            adapter: default_execution_adapter(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSection {
    #[serde(default = "default_max_daily_loss")]
    pub max_daily_loss: f64,
    #[serde(default = "default_max_order_value")]
    pub max_order_value: f64,
}

impl Default for RiskSection {
    fn default() -> Self {
        Self {
            max_daily_loss: default_max_daily_loss(),
            max_order_value: default_max_order_value(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_backend")]
    pub backend: String,
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
    #[serde(default)]
    pub postgres_url: Option<String>,
    #[serde(default = "default_wal_mode")]
    pub wal_mode: bool,
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: default_storage_backend(),
            sqlite_path: default_sqlite_path(),
            postgres_url: None,
            wal_mode: default_wal_mode(),
            pool_size: default_pool_size(),
        }
    }
}

impl StorageConfig {
    pub fn backend_kind(&self) -> Result<StorageBackendKind> {
        let normalized = self.backend.trim().to_ascii_lowercase();

        match normalized.as_str() {
            "" | "sqlite" => Ok(StorageBackendKind::Sqlite),
            "timescale" | "timescaledb" | "postgres" | "postgresql" => {
                Ok(StorageBackendKind::Timescale)
            }
            other => Err(anyhow::anyhow!(
                "Unsupported storage backend `{other}`. Expected `sqlite` or `timescale`."
            )),
        }
    }

    pub fn postgres_url(&self) -> Option<String> {
        preferred_env_var("APEX_POSTGRES_URL")
            .or_else(|| preferred_env_var("DATABASE_URL"))
            .or_else(|| self.postgres_url.clone().and_then(normalize_optional_string))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackendKind {
    Sqlite,
    Timescale,
}

fn preferred_env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .and_then(|value| normalize_optional_string(value))
}

fn normalize_optional_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn default_data_dir() -> String {
    "data".to_string()
}

fn default_max_daily_loss() -> f64 {
    50_000.0
}

fn default_market_data_adapter() -> String {
    "yahoo_finance".to_string()
}

fn default_execution_adapter() -> String {
    "paper".to_string()
}

fn default_max_order_value() -> f64 {
    500_000.0
}

fn default_storage_backend() -> String {
    "sqlite".to_string()
}

fn default_sqlite_path() -> String {
    "apex.db".to_string()
}

fn default_wal_mode() -> bool {
    true
}

fn default_pool_size() -> usize {
    4
}