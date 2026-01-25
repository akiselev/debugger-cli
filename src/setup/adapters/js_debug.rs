//! js-debug installer
//!
//! Installs Microsoft's JavaScript/TypeScript debugger via npm.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, ensure_adapters_dir, run_command_args, write_version_file,
    InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter_tcp, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "js-debug",
    name: "js-debug",
    languages: &["javascript", "typescript"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "Microsoft's JavaScript/TypeScript debugger",
    primary: true,
};

pub struct JsDebugInstaller;

#[async_trait]
impl Installer for JsDebugInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        let adapter_dir = adapters_dir().join("js-debug");
        let dap_path = get_dap_executable(&adapter_dir);

        if dap_path.exists() {
            let version = read_package_version(&adapter_dir);
            return Ok(InstallStatus::Installed {
                path: dap_path,
                version,
            });
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        if which::which("npm").is_err() {
            return Err(Error::Internal(
                "npm not found. Please install Node.js and npm first.".to_string(),
            ));
        }

        Ok(InstallMethod::LanguagePackage {
            tool: "npm".to_string(),
            package: "@vscode/js-debug".to_string(),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        install_js_debug(&opts).await
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("js-debug");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("js-debug managed installation not found");
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                // js-debug's dapDebugServer.js must be run via node
                let node_path = which::which("node").map_err(|_| {
                    Error::Internal("node not found in PATH".to_string())
                })?;
                // TcpPortArg appends port as positional argument, no extra args needed
                verify_dap_adapter_tcp(&node_path, &[path.to_string_lossy().to_string()], crate::common::config::TcpSpawnStyle::TcpPortArg).await
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

fn get_dap_executable(adapter_dir: &PathBuf) -> PathBuf {
    // @vscode/js-debug installs to node_modules/@vscode/js-debug
    let js_path = adapter_dir.join("node_modules/@vscode/js-debug/src/dapDebugServer.js");
    if js_path.exists() {
        return js_path;
    }
    adapter_dir.join("node_modules/@vscode/js-debug/dist/src/dapDebugServer.js")
}

fn read_package_version(adapter_dir: &PathBuf) -> Option<String> {
    let package_json = adapter_dir.join("node_modules/@vscode/js-debug/package.json");
    if !package_json.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&package_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

async fn install_js_debug(opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");

    let npm_path = which::which("npm").map_err(|_| {
        Error::Internal("npm not found in PATH".to_string())
    })?;
    let node_path = which::which("node").map_err(|_| {
        Error::Internal("node not found in PATH".to_string())
    })?;
    println!("Using npm: {}", npm_path.display());

    let adapter_dir = ensure_adapters_dir()?.join("js-debug");

    if opts.force && adapter_dir.exists() {
        std::fs::remove_dir_all(&adapter_dir)?;
    }

    std::fs::create_dir_all(&adapter_dir)?;

    let package = if let Some(version) = &opts.version {
        format!("@vscode/js-debug@{}", version)
    } else {
        "@vscode/js-debug".to_string()
    };

    println!("Installing {}...", package);
    run_command_args(
        &npm_path,
        &["install", "--prefix", adapter_dir.to_str().unwrap_or("."), &package]
    ).await?;

    let dap_path = get_dap_executable(&adapter_dir);
    if !dap_path.exists() {
        return Err(Error::Internal(
            "@vscode/js-debug installation succeeded but dapDebugServer.js not found".to_string(),
        ));
    }

    let version = read_package_version(&adapter_dir);

    if let Some(v) = &version {
        write_version_file(&adapter_dir, v)?;
    }

    println!("js-debug installation completed.");

    // Return node as the executable with the JS file as an argument
    // TcpPortArg will append the port as a positional argument
    Ok(InstallResult {
        path: node_path,
        version,
        args: vec![dap_path.to_string_lossy().to_string()],
    })
}
