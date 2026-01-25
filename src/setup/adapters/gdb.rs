//! GDB native DAP adapter installer
//!
//! Installs GDB with native DAP support (GDB ≥14.1).

use crate::common::{Error, Result};
use crate::setup::installer::{InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;

use super::gdb_common::{get_gdb_version, is_gdb_version_sufficient};

static INFO: DebuggerInfo = DebuggerInfo {
    id: "gdb",
    name: "GDB",
    languages: &["c", "cpp"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "GDB native DAP adapter",
    primary: true,
};

pub struct GdbInstaller;

#[async_trait]
impl Installer for GdbInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        if let Ok(path) = which::which("gdb") {
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
                            "GDB version {} found, but ≥14.1 required for native DAP support",
                            version
                        ),
                    });
                }
                None => {
                    return Ok(InstallStatus::Broken {
                        path,
                        reason: "Could not determine GDB version".to_string(),
                    });
                }
            }
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        if let Ok(path) = which::which("gdb") {
            if let Some(version) = get_gdb_version(&path).await {
                if is_gdb_version_sufficient(&version) {
                    return Ok(InstallMethod::AlreadyInstalled { path });
                }
            }
        }

        Ok(InstallMethod::NotSupported {
            reason: "GDB ≥14.1 not found. Install via your system package manager.".to_string(),
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
                Err(Error::Internal(format!("Cannot install GDB: {}", reason)))
            }
            _ => Err(Error::Internal("Unexpected installation method".to_string())),
        }
    }

    async fn uninstall(&self) -> Result<()> {
        println!("GDB is a system package. Use your package manager to uninstall.");
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
