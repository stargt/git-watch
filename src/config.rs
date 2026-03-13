use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub repos: Vec<String>,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Deserialize)]
pub struct WatchConfig {
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_reconcile_interval_sec")]
    pub reconcile_interval_sec: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            reconcile_interval_sec: default_reconcile_interval_sec(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_command_timeout_sec")]
    pub command_timeout_sec: u64,
    #[serde(default = "default_max_concurrent_checks")]
    pub max_concurrent_checks: usize,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            command_timeout_sec: default_command_timeout_sec(),
            max_concurrent_checks: default_max_concurrent_checks(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_width")]
    pub width: usize,
    #[serde(default = "default_true")]
    pub color: bool,
    #[serde(default = "default_true")]
    pub show_clean: bool,
    #[serde(default = "default_true")]
    pub blank_line_between_repos: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            color: true,
            show_clean: true,
            blank_line_between_repos: true,
        }
    }
}

fn default_debounce_ms() -> u64 {
    200
}
fn default_reconcile_interval_sec() -> u64 {
    60
}
fn default_command_timeout_sec() -> u64 {
    3
}
fn default_max_concurrent_checks() -> usize {
    4
}
fn default_width() -> usize {
    30
}
fn default_true() -> bool {
    true
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read config file: {}", e))?;
        let mut config: Config =
            serde_yml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;

        config.repos = config
            .repos
            .iter()
            .map(|p| {
                let expanded = expand_tilde(p);
                std::fs::canonicalize(&expanded)
                    .unwrap_or(expanded)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        Ok(config)
    }

    pub fn all_repos(&self) -> Vec<(String, PathBuf, bool)> {
        self.repos
            .iter()
            .map(|p| {
                let path = PathBuf::from(p);
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.clone());
                let valid = path.join(".git").exists();
                (name, path, valid)
            })
            .collect()
    }
}
