//! CUDA-GDB adapter installer
//!
//! CUDA-GDB supports two modes:
//! 1. Native DAP mode (-i=dap): Available in cuda-gdb builds based on GDB 14.1+
//!    when DAP Python bindings are included (NVIDIA official installs)
//! 2. cdt-gdb-adapter bridge: For cuda-gdb builds without native DAP (e.g., Arch Linux minimal)
//!
//! Architecture (native DAP):
//!   Client <-> cuda-gdb -i=dap <-> GPU
//!
//! Architecture (cdt-gdb-adapter bridge):
//!   Client <-> cdt-gdb-adapter (DAP) <-> cuda-gdb (MI mode) <-> GPU
//!
//! Requirements:
//! - CUDA Toolkit with cuda-gdb (Linux only)
//! - For bridge mode: Node.js runtime + cdt-gdb-adapter npm package

use crate::common::{Error, Result};
use crate::setup::installer::{InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

use super::gdb_common::{get_gdb_version, is_gdb_version_sufficient};

static INFO: DebuggerInfo = DebuggerInfo {
    id: "cuda-gdb",
    name: "CUDA-GDB",
    languages: &["cuda", "c", "cpp"],
    platforms: &[Platform::Linux],
    description: "NVIDIA CUDA debugger for GPU code",
    primary: true,
};

pub struct CudaGdbInstaller;

/// Check if cuda-gdb supports native DAP mode by testing "-i=dap"
async fn has_native_dap_support(cuda_gdb_path: &PathBuf) -> bool {
    // First check version - needs GDB 14.1+ base
    if let Some(version) = get_gdb_version(cuda_gdb_path).await {
        if !is_gdb_version_sufficient(&version) {
            return false;
        }
    } else {
        return false;
    }

    // Test if DAP interpreter is available
    // cuda-gdb without DAP will fail with "Interpreter `dap' unrecognized"
    let output = tokio::process::Command::new(cuda_gdb_path)
        .args(["-i=dap", "-batch", "-ex", "quit"])
        .output()
        .await;

    match output {
        Ok(result) => {
            // Check stderr for "unrecognized" error
            let stderr = String::from_utf8_lossy(&result.stderr);
            !stderr.contains("unrecognized") && !stderr.contains("Interpreter")
        }
        Err(_) => false,
    }
}

#[async_trait]
impl Installer for CudaGdbInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        if Platform::current() != Platform::Linux {
            return Ok(InstallStatus::NotInstalled);
        }

        // Check for cuda-gdb
        let Some(cuda_gdb_path) = find_cuda_gdb() else {
            return Ok(InstallStatus::NotInstalled);
        };

        let version = get_gdb_version(&cuda_gdb_path).await;

        // Check for native DAP support first (preferred)
        if has_native_dap_support(&cuda_gdb_path).await {
            return Ok(InstallStatus::Installed {
                path: cuda_gdb_path,
                version,
            });
        }

        // Fall back to cdt-gdb-adapter bridge
        if let Some(cdt_adapter) = find_cdt_gdb_adapter() {
            return Ok(InstallStatus::Installed {
                path: cdt_adapter,
                version,
            });
        }

        // cuda-gdb exists but no DAP method available
        Ok(InstallStatus::Broken {
            path: cuda_gdb_path,
            reason: "cuda-gdb found but lacks native DAP support. Install cdt-gdb-adapter: npm install -g cdt-gdb-adapter".to_string(),
        })
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        if Platform::current() != Platform::Linux {
            return Ok(InstallMethod::NotSupported {
                reason: "CUDA-GDB GPU debugging is only supported on Linux".to_string(),
            });
        }

        // Check for cuda-gdb
        let Some(cuda_gdb_path) = find_cuda_gdb() else {
            return Ok(InstallMethod::NotSupported {
                reason: "CUDA-GDB not found. Install NVIDIA CUDA Toolkit from https://developer.nvidia.com/cuda-downloads".to_string(),
            });
        };

        // Check for native DAP support first (preferred)
        if has_native_dap_support(&cuda_gdb_path).await {
            return Ok(InstallMethod::AlreadyInstalled { path: cuda_gdb_path });
        }

        // Fall back to cdt-gdb-adapter bridge
        if let Some(cdt_adapter) = find_cdt_gdb_adapter() {
            return Ok(InstallMethod::AlreadyInstalled { path: cdt_adapter });
        }

        Ok(InstallMethod::NotSupported {
            reason: "cuda-gdb lacks native DAP support. Install cdt-gdb-adapter: npm install -g cdt-gdb-adapter".to_string(),
        })
    }

    async fn install(&self, _opts: InstallOptions) -> Result<InstallResult> {
        let method = self.best_method().await?;

        match method {
            InstallMethod::AlreadyInstalled { path } => {
                let cuda_gdb_path = find_cuda_gdb().ok_or_else(|| {
                    Error::Internal("CUDA-GDB not found".to_string())
                })?;
                let version = get_gdb_version(&cuda_gdb_path).await;

                // Determine if using native DAP or bridge mode
                if has_native_dap_support(&cuda_gdb_path).await {
                    // Native DAP mode
                    Ok(InstallResult {
                        path: cuda_gdb_path,
                        version,
                        args: vec!["-i=dap".to_string()],
                    })
                } else {
                    // cdt-gdb-adapter bridge mode
                    Ok(InstallResult {
                        path,
                        version,
                        args: vec![format!("--config={{\"gdb\":\"{}\"}}", cuda_gdb_path.display())],
                    })
                }
            }
            InstallMethod::NotSupported { reason } => {
                Err(Error::Internal(format!("Cannot install CUDA-GDB: {}", reason)))
            }
            _ => Err(Error::Internal("Unexpected installation method".to_string())),
        }
    }

    async fn uninstall(&self) -> Result<()> {
        println!("CUDA-GDB is part of NVIDIA CUDA Toolkit. Uninstall the toolkit to remove it.");
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                let cuda_gdb_path = find_cuda_gdb().ok_or_else(|| {
                    Error::Internal("CUDA-GDB not found".to_string())
                })?;

                // Determine verification args based on mode
                if has_native_dap_support(&cuda_gdb_path).await {
                    // Native DAP mode
                    verify_dap_adapter(&path, &["-i=dap".to_string()]).await
                } else {
                    // cdt-gdb-adapter bridge mode
                    verify_dap_adapter(
                        &path,
                        &[format!("--config={{\"gdb\":\"{}\"}}", cuda_gdb_path.display())],
                    ).await
                }
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

/// Locates cuda-gdb binary using NVIDIA Toolkit path conventions
///
/// Search order: versioned CUDA installs → /usr/local/cuda → /opt/cuda → CUDA_HOME → PATH
fn find_cuda_gdb() -> Option<PathBuf> {
    // Check versioned CUDA installs (e.g., /usr/local/cuda-13.1)
    // Prefer higher versions which are more likely to have DAP support
    if let Ok(entries) = std::fs::read_dir("/usr/local") {
        let mut cuda_paths: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("cuda-") {
                    let cuda_gdb = e.path().join("bin/cuda-gdb");
                    if cuda_gdb.exists() {
                        // Extract version for sorting (e.g., "13.1" from "cuda-13.1")
                        let version = name.strip_prefix("cuda-").unwrap_or("0.0").to_string();
                        return Some((version, cuda_gdb));
                    }
                }
                None
            })
            .collect();

        // Sort by version descending (higher versions first)
        cuda_paths.sort_by(|a, b| {
            let parse_version = |s: &str| -> (u32, u32) {
                let parts: Vec<&str> = s.split('.').collect();
                let major = parts.first().and_then(|p| p.parse().ok()).unwrap_or(0);
                let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
                (major, minor)
            };
            parse_version(&b.0).cmp(&parse_version(&a.0))
        });

        if let Some((_, path)) = cuda_paths.first() {
            return Some(path.clone());
        }
    }

    // NVIDIA's standard install location (symlink to versioned install)
    let default_path = PathBuf::from("/usr/local/cuda/bin/cuda-gdb");
    if default_path.exists() {
        return Some(default_path);
    }

    // Arch Linux installs to /opt/cuda
    let arch_path = PathBuf::from("/opt/cuda/bin/cuda-gdb");
    if arch_path.exists() {
        return Some(arch_path);
    }

    // CUDA_HOME environment variable
    if let Ok(cuda_home) = std::env::var("CUDA_HOME") {
        let cuda_home_path = PathBuf::from(cuda_home).join("bin/cuda-gdb");
        if cuda_home_path.exists() {
            return Some(cuda_home_path);
        }
    }

    // Fall back to PATH
    which::which("cuda-gdb").ok()
}

/// Locates cdt-gdb-adapter (cdtDebugAdapter) binary
///
/// Searches npm global bin directories and common locations
fn find_cdt_gdb_adapter() -> Option<PathBuf> {
    // Check PATH first
    if let Ok(path) = which::which("cdtDebugAdapter") {
        return Some(path);
    }

    // Check common npm global bin locations
    if let Ok(home) = std::env::var("HOME") {
        // nvm installations
        let nvm_path = PathBuf::from(&home).join(".nvm/versions/node");
        if nvm_path.exists() {
            if let Ok(entries) = std::fs::read_dir(&nvm_path) {
                for entry in entries.flatten() {
                    let bin_path = entry.path().join("bin/cdtDebugAdapter");
                    if bin_path.exists() {
                        return Some(bin_path);
                    }
                }
            }
        }

        // Standard npm global
        let npm_global = PathBuf::from(&home).join(".npm-global/bin/cdtDebugAdapter");
        if npm_global.exists() {
            return Some(npm_global);
        }

        // npm prefix bin
        let npm_prefix = PathBuf::from(&home).join("node_modules/.bin/cdtDebugAdapter");
        if npm_prefix.exists() {
            return Some(npm_prefix);
        }
    }

    // System-wide npm
    let system_path = PathBuf::from("/usr/local/bin/cdtDebugAdapter");
    if system_path.exists() {
        return Some(system_path);
    }

    None
}
