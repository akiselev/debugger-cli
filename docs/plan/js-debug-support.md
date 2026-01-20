Based on the project structure and the goal to add `js-debug` (VS Code's built-in JavaScript/TypeScript debugger) support, here is the implementation plan.

The plan involves:

1. **Creating a new adapter installer** (`src/setup/adapters/js_debug.rs`) that downloads the `ms-vscode.js-debug` VSIX from GitHub releases, extracts it, and verifies `node` is available.
2. **Registering the adapter** in `src/setup/registry.rs`.
3. **Updating project detection** in `src/setup/detector.rs` to recommend this debugger for Node.js/TypeScript projects.
4. **Exposing the new module** in `src/setup/adapters/mod.rs`.

### 1. Create `src/setup/adapters/js_debug.rs`

This file implements the `Installer` trait for `js-debug`. It handles finding `node`, downloading the VSIX, and configuring the execution command.

```rust
//! js-debug installer
//!
//! Installs the VS Code JavaScript debugger from GitHub releases.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, download_file, ensure_adapters_dir, extract_zip, get_github_release,
    make_executable, run_command_args, write_version_file, InstallMethod, InstallOptions,
    InstallResult, InstallStatus, Installer,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "js-debug",
    name: "VS Code JavaScript Debugger",
    languages: &["javascript", "typescript"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "Microsoft's JavaScript debugger (aka js-debug)",
    primary: true,
};

const GITHUB_REPO: &str = "microsoft/vscode-js-debug";

pub struct JsDebugInstaller;

#[async_trait]
impl Installer for JsDebugInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        let adapter_dir = adapters_dir().join("js-debug");
        let server_path = find_dap_server(&adapter_dir);

        if server_path.exists() {
             // Check if node is available
            if let Ok(node_path) = find_node().await {
                let version = crate::setup::installer::read_version_file(&adapter_dir);
                return Ok(InstallStatus::Installed {
                    path: node_path, // We run 'node', passing the server script as arg
                    version,
                });
            } else {
                 return Ok(InstallStatus::Broken {
                    path: server_path,
                    reason: "Node.js not found in PATH".to_string(),
                });
            }
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        Ok(InstallMethod::GitHubRelease {
            repo: GITHUB_REPO.to_string(),
            asset_pattern: "ms-vscode.js-debug-*.vsix".to_string(),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        install_from_github(&opts).await
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("js-debug");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("js-debug is not installed");
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path: node_path, .. } => {
                let adapter_dir = adapters_dir().join("js-debug");
                let server_path = find_dap_server(&adapter_dir);
                
                // js-debug DAP server runs via: node <path/to/dapDebugServer.js>
                let args = vec![server_path.to_string_lossy().to_string()];
                
                verify_dap_adapter(&node_path, &args).await
            }
            InstallStatus::Broken { reason, .. } => Ok(VerifyResult {
                success: false,
                capabilities: None,
                error: Some(reason),
            }),
            InstallStatus::NotInstalled => Ok(VerifyResult {
                success: false,
                capabilities: None,
                error: Some("Not installed".to_string()),
            }),
        }
    }
}

async fn find_node() -> Result<PathBuf> {
    which::which("node")
        .map_err(|_| Error::Internal("Node.js not found. Please install Node.js (v14+) first.".to_string()))
}

fn find_dap_server(adapter_dir: &std::path::Path) -> PathBuf {
    // The VSIX extracts to an 'extension' folder usually
    // Server entry point is typically src/dapDebugServer.js within the extension folder
    adapter_dir.join("extension").join("src").join("dapDebugServer.js")
}

async fn install_from_github(opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");
    
    // Ensure Node is present
    let node_path = find_node().await?;
    println!("Using Node.js: {}", node_path.display());

    println!("Finding latest js-debug release...");
    let release = get_github_release(GITHUB_REPO, opts.version.as_deref()).await?;
    let version = release.tag_name.trim_start_matches('v').to_string();
    println!("Found version: {}", version);

    // Find the vsix asset
    let asset = release
        .find_asset(&["ms-vscode.js-debug-*.vsix"])
        .ok_or_else(|| {
            Error::Internal(format!(
                "No js-debug VSIX found in release {}.",
                version
            ))
        })?;

    // Download and extract
    let temp_dir = tempfile::tempdir()?;
    let archive_path = temp_dir.path().join(&asset.name);

    println!(
        "Downloading {}... {:.1} MB",
        asset.name,
        asset.size as f64 / 1_000_000.0
    );
    download_file(&asset.browser_download_url, &archive_path).await?;

    println!("Extracting...");
    let adapter_dir = ensure_adapters_dir()?.join("js-debug");
    if adapter_dir.exists() {
        std::fs::remove_dir_all(&adapter_dir)?;
    }
    std::fs::create_dir_all(&adapter_dir)?;

    // Extract vsix (it's a zip)
    extract_zip(&archive_path, &adapter_dir)?;

    let server_path = find_dap_server(&adapter_dir);
    if !server_path.exists() {
        return Err(Error::Internal(format!(
            "DAP server script not found at expected location: {}",
            server_path.display()
        )));
    }

    // Write version file
    write_version_file(&adapter_dir, &version)?;

    println!("Verifying installation...");

    Ok(InstallResult {
        path: node_path,
        version: Some(version),
        args: vec![server_path.to_string_lossy().to_string()],
    })
}

```

### 2. Update `src/setup/adapters/mod.rs`

Register the new module.

```rust
//! Debug adapter installers
//!
//! Individual installers for each supported debug adapter.

pub mod codelldb;
pub mod debugpy;
pub mod delve;
pub mod lldb;
pub mod js_debug; // Add this line

```

### 3. Update `src/setup/registry.rs`

Register the new adapter in the global list and factory function.

```rust
// ... imports

static DEBUGGERS: &[DebuggerInfo] = &[
    // ... existing adapters ...
    DebuggerInfo {
        id: "js-debug",
        name: "VS Code JavaScript Debugger",
        languages: &["javascript", "typescript"],
        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
        description: "Microsoft's JavaScript debugger (aka js-debug)",
        primary: true,
    },
];

// ...

pub fn get_installer(id: &str) -> Option<Arc<dyn Installer>> {
    use super::adapters;

    match id {
        "lldb" => Some(Arc::new(adapters::lldb::LldbInstaller)),
        "codelldb" => Some(Arc::new(adapters::codelldb::CodeLldbInstaller)),
        "python" => Some(Arc::new(adapters::debugpy::DebugpyInstaller)),
        "go" => Some(Arc::new(adapters::delve::DelveInstaller)),
        "js-debug" => Some(Arc::new(adapters::js_debug::JsDebugInstaller)), // Add this match arm
        _ => None,
    }
}

```

### 4. Update `src/setup/detector.rs`

Update the project detector to recommend `js-debug`.

```rust
pub fn debuggers_for_project(project: &ProjectType) -> Vec<&'static str> {
    match project {
        ProjectType::Rust => vec!["codelldb", "lldb"],
        ProjectType::Go => vec!["go"],
        ProjectType::Python => vec!["python"],
        ProjectType::JavaScript | ProjectType::TypeScript => vec!["js-debug"], // Update this line
        ProjectType::C | ProjectType::Cpp => vec!["lldb", "codelldb"],
        ProjectType::CSharp => vec![],
        ProjectType::Java => vec![],
    }
}
```
