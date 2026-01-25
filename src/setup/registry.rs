//! Debugger registry and metadata
//!
//! Contains information about all supported debuggers and their installation methods.

use super::installer::Installer;
use std::fmt;
use std::sync::Arc;

/// Supported platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOS,
    Windows,
}

impl Platform {
    /// Get the current platform
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        return Platform::Linux;

        #[cfg(target_os = "macos")]
        return Platform::MacOS;

        #[cfg(target_os = "windows")]
        return Platform::Windows;

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        return Platform::Linux; // Default fallback
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Linux => write!(f, "linux"),
            Platform::MacOS => write!(f, "macos"),
            Platform::Windows => write!(f, "windows"),
        }
    }
}

/// Information about a debugger
#[derive(Debug, Clone)]
pub struct DebuggerInfo {
    /// Unique identifier (e.g., "lldb", "codelldb")
    pub id: &'static str,
    /// Display name
    pub name: &'static str,
    /// Supported languages
    pub languages: &'static [&'static str],
    /// Supported platforms
    pub platforms: &'static [Platform],
    /// Brief description
    pub description: &'static str,
    /// Whether this is the primary adapter for its languages
    pub primary: bool,
}

/// All available debuggers
static DEBUGGERS: &[DebuggerInfo] = &[
    DebuggerInfo {
        id: "gdb",
        name: "GDB",
        languages: &["c", "cpp"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "GDB native DAP adapter",
        primary: true,
    },
    DebuggerInfo {
        id: "cuda-gdb",
        name: "CUDA-GDB",
        languages: &["cuda", "c", "cpp"],
        platforms: &[Platform::Linux],
        description: "NVIDIA CUDA debugger with DAP support",
        primary: true,
    },
    DebuggerInfo {
        id: "lldb",
        name: "lldb-dap",
        languages: &["c", "cpp", "rust", "swift"],
        platforms: &[Platform::Linux, Platform::MacOS],
        description: "LLVM's native DAP adapter",
        primary: true,
    },
    DebuggerInfo {
        id: "codelldb",
        name: "CodeLLDB",
        languages: &["c", "cpp", "rust"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "Feature-rich LLDB-based debugger",
        primary: false,
    },
    DebuggerInfo {
        id: "python",
        name: "debugpy",
        languages: &["python"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "Microsoft's Python debugger",
        primary: true,
    },
    DebuggerInfo {
        id: "go",
        name: "Delve",
        languages: &["go"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "Go debugger with DAP support",
        primary: true,
    },
    DebuggerInfo {
        id: "js-debug",
        name: "js-debug",
        languages: &["javascript", "typescript"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "Microsoft's JavaScript/TypeScript debugger",
        primary: true,
    },
];

/// Get all registered debuggers
pub fn all_debuggers() -> &'static [DebuggerInfo] {
    DEBUGGERS
}

/// Get debugger info by ID
pub fn get_debugger(id: &str) -> Option<&'static DebuggerInfo> {
    DEBUGGERS.iter().find(|d| d.id == id)
}

/// Get debuggers for a specific language
pub fn debuggers_for_language(language: &str) -> Vec<&'static DebuggerInfo> {
    DEBUGGERS
        .iter()
        .filter(|d| d.languages.contains(&language))
        .collect()
}

/// Get the primary debugger for a language
pub fn primary_debugger_for_language(language: &str) -> Option<&'static DebuggerInfo> {
    DEBUGGERS
        .iter()
        .find(|d| d.languages.contains(&language) && d.primary)
}

/// Get an installer for a debugger
pub fn get_installer(id: &str) -> Option<Arc<dyn Installer>> {
    use super::adapters;

    match id {
        "gdb" => Some(Arc::new(adapters::gdb::GdbInstaller)),
        "cuda-gdb" => Some(Arc::new(adapters::cuda_gdb::CudaGdbInstaller)),
        "lldb" => Some(Arc::new(adapters::lldb::LldbInstaller)),
        "codelldb" => Some(Arc::new(adapters::codelldb::CodeLldbInstaller)),
        "python" => Some(Arc::new(adapters::debugpy::DebugpyInstaller)),
        "go" => Some(Arc::new(adapters::delve::DelveInstaller)),
        "js-debug" => Some(Arc::new(adapters::js_debug::JsDebugInstaller)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_debuggers_has_entries() {
        assert!(!all_debuggers().is_empty());
    }

    #[test]
    fn test_get_debugger() {
        assert!(get_debugger("lldb").is_some());
        assert!(get_debugger("nonexistent").is_none());
    }

    #[test]
    fn test_debuggers_for_language() {
        let rust_debuggers = debuggers_for_language("rust");
        assert!(!rust_debuggers.is_empty());
    }
}
