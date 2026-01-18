//! Configuration file handling

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use super::paths::config_path;
use super::Result;

/// Main configuration structure
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Debug adapter configurations
    #[serde(default)]
    pub adapters: HashMap<String, AdapterConfig>,

    /// Default settings
    #[serde(default)]
    pub defaults: Defaults,

    /// Timeout settings
    #[serde(default)]
    pub timeouts: Timeouts,

    /// Daemon settings
    #[serde(default)]
    pub daemon: DaemonConfig,

    /// Output buffer settings
    #[serde(default)]
    pub output: OutputConfig,
}

/// Adapter type for specialized launch argument handling
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    /// lldb-dap (LLVM debugger)
    #[default]
    LldbDap,
    /// debugpy (Python debugger)
    Debugpy,
    /// CodeLLDB (VSCode extension)
    Codelldb,
    /// Generic DAP adapter (no special handling)
    Generic,
}

/// Configuration for a debug adapter
#[derive(Debug, Deserialize, Clone)]
pub struct AdapterConfig {
    /// Path to the adapter executable
    pub path: PathBuf,

    /// Additional arguments to pass to the adapter
    #[serde(default)]
    pub args: Vec<String>,

    /// Adapter type for specialized handling
    #[serde(default)]
    pub adapter_type: AdapterType,
}

/// Default settings
#[derive(Debug, Deserialize)]
pub struct Defaults {
    /// Default adapter to use
    #[serde(default = "default_adapter")]
    pub adapter: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            adapter: default_adapter(),
        }
    }
}

fn default_adapter() -> String {
    "lldb-dap".to_string()
}

/// Timeout settings in seconds
#[derive(Debug, Deserialize)]
pub struct Timeouts {
    /// Timeout for DAP initialize request
    #[serde(default = "default_dap_initialize")]
    pub dap_initialize_secs: u64,

    /// Timeout for general DAP requests
    #[serde(default = "default_dap_request")]
    pub dap_request_secs: u64,

    /// Default timeout for await command
    #[serde(default = "default_await")]
    pub await_default_secs: u64,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            dap_initialize_secs: default_dap_initialize(),
            dap_request_secs: default_dap_request(),
            await_default_secs: default_await(),
        }
    }
}

fn default_dap_initialize() -> u64 {
    10
}
fn default_dap_request() -> u64 {
    30
}
fn default_await() -> u64 {
    300
}

/// Daemon configuration
#[derive(Debug, Deserialize)]
pub struct DaemonConfig {
    /// Auto-exit after this many minutes with no active session
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_minutes: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            idle_timeout_minutes: default_idle_timeout(),
        }
    }
}

fn default_idle_timeout() -> u64 {
    30
}

/// Output buffer configuration
#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    /// Maximum number of output events to buffer
    #[serde(default = "default_max_events")]
    pub max_events: usize,

    /// Maximum total bytes to buffer
    #[serde(default = "default_max_bytes")]
    pub max_bytes_mb: usize,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            max_events: default_max_events(),
            max_bytes_mb: default_max_bytes(),
        }
    }
}

fn default_max_events() -> usize {
    10_000
}
fn default_max_bytes() -> usize {
    10
}

impl Config {
    /// Load configuration from the default config file
    ///
    /// Returns default configuration if file doesn't exist
    pub fn load() -> Result<Self> {
        if let Some(path) = config_path() {
            if path.exists() {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    super::Error::FileRead {
                        path: path.display().to_string(),
                        error: e.to_string(),
                    }
                })?;
                return toml::from_str(&content)
                    .map_err(|e| super::Error::ConfigParse(e.to_string()));
            }
        }
        Ok(Self::default())
    }

    /// Get adapter configuration by name
    ///
    /// Falls back to searching PATH if not explicitly configured
    pub fn get_adapter(&self, name: &str) -> Option<AdapterConfig> {
        // Check explicit configuration first
        if let Some(config) = self.adapters.get(name) {
            return Some(config.clone());
        }

        // Try to find in PATH
        which::which(name).ok().map(|path| {
            // Infer adapter type from name
            let adapter_type = match name {
                "lldb-dap" | "lldb-vscode" => AdapterType::LldbDap,
                "debugpy" | "debugpy-adapter" => AdapterType::Debugpy,
                "codelldb" => AdapterType::Codelldb,
                _ => AdapterType::Generic,
            };
            AdapterConfig {
                path,
                args: Vec::new(),
                adapter_type,
            }
        })
    }
}
