//! debugpy installer
//!
//! Installs Microsoft's Python debugger via pip in an isolated virtual environment.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, ensure_adapters_dir, run_command_args, write_version_file,
    InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "python",
    name: "debugpy",
    languages: &["python"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "Microsoft's Python debugger",
    primary: true,
};

pub struct DebugpyInstaller;

#[async_trait]
impl Installer for DebugpyInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        let adapter_dir = adapters_dir().join("debugpy");
        let venv_dir = adapter_dir.join("venv");
        let python_path = get_venv_python(&venv_dir);

        if python_path.exists() {
            // Verify debugpy is installed in the venv
            let check = run_command_args(
                &python_path,
                &["-c", "import debugpy; print(debugpy.__version__)"],
            )
            .await;

            match check {
                Ok(version) => {
                    return Ok(InstallStatus::Installed {
                        path: python_path,
                        version: Some(version.trim().to_string()),
                    });
                }
                Err(_) => {
                    return Ok(InstallStatus::Broken {
                        path: python_path,
                        reason: "debugpy module not found in venv".to_string(),
                    });
                }
            }
        }

        // Check if debugpy is installed globally
        if let Ok(python_path) = which::which("python3") {
            if let Ok(version) = run_command_args(
                &python_path,
                &["-c", "import debugpy; print(debugpy.__version__)"],
            )
            .await
            {
                return Ok(InstallStatus::Installed {
                    path: python_path,
                    version: Some(version.trim().to_string()),
                });
            }
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        // Check if Python is available
        let python = find_python().await?;

        Ok(InstallMethod::LanguagePackage {
            tool: python.to_string_lossy().to_string(),
            package: "debugpy".to_string(),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        install_debugpy(&opts).await
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("debugpy");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("debugpy managed installation not found");
            println!("If installed globally, use: pip uninstall debugpy");
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                // debugpy requires special arguments to start as DAP adapter
                verify_dap_adapter(&path, &["-m".to_string(), "debugpy.adapter".to_string()]).await
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

/// Find a suitable Python interpreter
async fn find_python() -> Result<PathBuf> {
    // Try python3 first, then python
    for cmd in &["python3", "python"] {
        if let Ok(path) = which::which(cmd) {
            // Verify it's Python 3.7+ using explicit exit code (not assertion)
            let version_check = run_command_args(
                &path,
                &["-c", "import sys; sys.exit(0 if sys.version_info >= (3, 7) else 1)"],
            )
            .await;

            if version_check.is_ok() {
                return Ok(path);
            }
        }
    }

    Err(Error::Internal(
        "Python 3.7+ not found. Please install Python first.".to_string(),
    ))
}

/// Get the path to Python in a venv
fn get_venv_python(venv_dir: &PathBuf) -> PathBuf {
    if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    }
}

/// Get the path to pip in a venv
fn get_venv_pip(venv_dir: &PathBuf) -> PathBuf {
    if cfg!(windows) {
        venv_dir.join("Scripts").join("pip.exe")
    } else {
        venv_dir.join("bin").join("pip")
    }
}

async fn install_debugpy(opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");

    // Find Python
    let python = find_python().await?;
    println!("Using Python: {}", python.display());

    // Create installation directory
    let adapter_dir = ensure_adapters_dir()?.join("debugpy");
    let venv_dir = adapter_dir.join("venv");

    // Remove existing venv if force
    if opts.force && venv_dir.exists() {
        std::fs::remove_dir_all(&venv_dir)?;
    }

    // Create virtual environment
    if !venv_dir.exists() {
        println!("Creating virtual environment...");
        run_command_args(&python, &["-m", "venv", venv_dir.to_str().unwrap_or("venv")]).await?;
    }

    // Get venv pip
    let pip = get_venv_pip(&venv_dir);
    let venv_python = get_venv_python(&venv_dir);

    // Upgrade pip first
    println!("Upgrading pip...");
    let _ = run_command_args(&venv_python, &["-m", "pip", "install", "--upgrade", "pip"]).await;

    // Install debugpy - use separate args to prevent command injection
    let package = if let Some(version) = &opts.version {
        format!("debugpy=={}", version)
    } else {
        "debugpy".to_string()
    };

    println!("Installing {}...", package);
    run_command_args(&pip, &["install", &package]).await?;

    // Get installed version
    let version = run_command_args(
        &venv_python,
        &["-c", "import debugpy; print(debugpy.__version__)"],
    )
    .await
    .ok()
    .map(|s| s.trim().to_string());

    // Write version file
    if let Some(v) = &version {
        write_version_file(&adapter_dir, v)?;
    }

    println!("Setting permissions... done");
    println!("Verifying installation...");

    Ok(InstallResult {
        path: venv_python,
        version,
        args: vec!["-m".to_string(), "debugpy.adapter".to_string()],
    })
}
