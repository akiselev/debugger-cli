//! CUDA-GDB native DAP adapter installer
//!
//! Installs CUDA-GDB with native DAP support (based on GDB 14.2).

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
    description: "NVIDIA CUDA debugger with DAP support",
    primary: true,
};

pub struct CudaGdbInstaller;

#[async_trait]
impl Installer for CudaGdbInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        if Platform::current() != Platform::Linux {
            return Ok(InstallStatus::NotInstalled);
        }

        if let Some(path) = find_cuda_gdb() {
            match get_gdb_version(&path).await {
                Some(version) if is_gdb_version_sufficient(&version) => {
                    return Ok(InstallStatus::Installed {
                        path,
                        version: Some(version),
                    });
                }
                Some(version) => {
                    return Ok(InstallStatus::Broken {
                        path,
                        reason: format!(
                            "CUDA-GDB version {} found, but ≥14.1 required for native DAP support",
                            version
                        ),
                    });
                }
                None => {
                    return Ok(InstallStatus::Broken {
                        path,
                        reason: "Could not determine CUDA-GDB version".to_string(),
                    });
                }
            }
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        if Platform::current() != Platform::Linux {
            return Ok(InstallMethod::NotSupported {
                reason: "CUDA-GDB GPU debugging is only supported on Linux".to_string(),
            });
        }

        if let Some(path) = find_cuda_gdb() {
            if let Some(version) = get_gdb_version(&path).await {
                if is_gdb_version_sufficient(&version) {
                    return Ok(InstallMethod::AlreadyInstalled { path });
                }
            }
        }

        Ok(InstallMethod::NotSupported {
            reason: "CUDA-GDB not found. Install NVIDIA CUDA Toolkit from https://developer.nvidia.com/cuda-downloads".to_string(),
        })
    }

    async fn install(&self, _opts: InstallOptions) -> Result<InstallResult> {
        let method = self.best_method().await?;

        match method {
            InstallMethod::AlreadyInstalled { path } => {
                let version = get_gdb_version(&path).await;
                Ok(InstallResult {
                    path,
                    version,
                    args: vec!["-i=dap".to_string()],
                })
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
                verify_dap_adapter(&path, &["-i=dap".to_string()]).await
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
/// Search order prioritizes official NVIDIA install over custom paths
fn find_cuda_gdb() -> Option<PathBuf> {
    let default_path = PathBuf::from("/usr/local/cuda/bin/cuda-gdb");
    if default_path.exists() {
        return Some(default_path);
    }

    if let Ok(cuda_home) = std::env::var("CUDA_HOME") {
        let cuda_home_path = PathBuf::from(cuda_home).join("bin/cuda-gdb");
        if cuda_home_path.exists() {
            return Some(cuda_home_path);
        }
    }

    which::which("cuda-gdb").ok()
}
