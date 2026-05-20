use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
}

#[derive(Debug, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WatchConfig {
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_reconcile_interval_sec")]
    pub reconcile_interval_sec: u64,
    #[serde(default = "default_fetch_interval_sec")]
    pub fetch_interval_sec: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            reconcile_interval_sec: default_reconcile_interval_sec(),
            fetch_interval_sec: default_fetch_interval_sec(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_command_timeout_sec")]
    pub command_timeout_sec: u64,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            command_timeout_sec: default_command_timeout_sec(),
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
    #[serde(default = "default_compact_threshold")]
    pub compact_threshold: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            color: true,
            show_clean: true,
            blank_line_between_repos: true,
            compact_threshold: default_compact_threshold(),
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
fn default_width() -> usize {
    30
}
fn default_compact_threshold() -> usize {
    40
}
fn default_fetch_interval_sec() -> u64 {
    300
}
fn default_max_depth() -> usize {
    2
}
fn default_true() -> bool {
    true
}

fn discover_repos_in_cwd(max_depth: usize) -> Vec<String> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let mut repos: Vec<String> = Vec::new();
    collect_repos(&cwd, max_depth, &mut repos);
    repos.sort();
    repos
}

fn collect_repos(dir: &Path, remaining_depth: usize, out: &mut Vec<String>) {
    if dir.join(".git").exists() {
        out.push(dir.to_string_lossy().to_string());
        return;
    }
    if remaining_depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        collect_repos(&path, remaining_depth - 1, out);
    }
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
    pub fn load(path: &Path, depth_override: Option<usize>) -> Result<Self, String> {
        let mut config: Config = match fs::read_to_string(path) {
            Ok(content) => serde_yml::from_str(&content)
                .map_err(|e| format!("Failed to parse config: {}", e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(format!("Failed to read config file: {}", e)),
        };

        if config.repos.is_empty() {
            let depth = depth_override.unwrap_or(config.discovery.max_depth);
            config.repos = discover_repos_in_cwd(depth);
        }

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
