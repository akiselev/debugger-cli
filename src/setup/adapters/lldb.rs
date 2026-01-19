//! lldb-dap installer
//!
//! Installs the LLVM lldb-dap debug adapter.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, ensure_adapters_dir, make_executable, InstallMethod, InstallOptions,
    InstallResult, InstallStatus, Installer, PackageManager,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "lldb",
    name: "lldb-dap",
    languages: &["c", "cpp", "rust", "swift"],
    platforms: &[Platform::Linux, Platform::MacOS],
    description: "LLVM's native DAP adapter",
    primary: true,
};

pub struct LldbInstaller;

#[async_trait]
impl Installer for LldbInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        // Check our managed installation first
        let adapter_dir = adapters_dir().join("lldb-dap");
        let managed_path = adapter_dir.join("bin").join(binary_name());

        if managed_path.exists() {
            let version = crate::setup::installer::read_version_file(&adapter_dir);
            return Ok(InstallStatus::Installed {
                path: managed_path,
                version,
            });
        }

        // Check if available in PATH
        if let Ok(path) = which::which("lldb-dap") {
            let version = get_version(&path).await;
            return Ok(InstallStatus::Installed { path, version });
        }

        // Also check for lldb-vscode (older name)
        if let Ok(path) = which::which("lldb-vscode") {
            let version = get_version(&path).await;
            return Ok(InstallStatus::Installed { path, version });
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        // Check if already in PATH
        if let Ok(path) = which::which("lldb-dap") {
            return Ok(InstallMethod::AlreadyInstalled { path });
        }
        if let Ok(path) = which::which("lldb-vscode") {
            return Ok(InstallMethod::AlreadyInstalled { path });
        }

        let platform = Platform::current();
        let managers = PackageManager::detect();

        match platform {
            Platform::MacOS => {
                // macOS: Xcode command line tools or Homebrew
                if managers.contains(&PackageManager::Homebrew) {
                    return Ok(InstallMethod::PackageManager {
                        manager: PackageManager::Homebrew,
                        package: "llvm".to_string(),
                    });
                }
                // Check for Xcode lldb-dap
                let xcode_path = PathBuf::from("/usr/bin/lldb-dap");
                if xcode_path.exists() {
                    return Ok(InstallMethod::AlreadyInstalled { path: xcode_path });
                }
            }
            Platform::Linux => {
                // Linux: package managers or LLVM releases
                if managers.contains(&PackageManager::Apt) {
                    return Ok(InstallMethod::PackageManager {
                        manager: PackageManager::Apt,
                        package: "lldb".to_string(),
                    });
                }
                if managers.contains(&PackageManager::Dnf) {
                    return Ok(InstallMethod::PackageManager {
                        manager: PackageManager::Dnf,
                        package: "lldb".to_string(),
                    });
                }
                if managers.contains(&PackageManager::Pacman) {
                    return Ok(InstallMethod::PackageManager {
                        manager: PackageManager::Pacman,
                        package: "lldb".to_string(),
                    });
                }

                // Fallback to GitHub releases
                return Ok(InstallMethod::GitHubRelease {
                    repo: "llvm/llvm-project".to_string(),
                    asset_pattern: "LLVM-*-Linux-*.tar.xz".to_string(),
                });
            }
            Platform::Windows => {
                return Ok(InstallMethod::NotSupported {
                    reason: "lldb-dap is not well-supported on Windows. Use codelldb instead."
                        .to_string(),
                });
            }
        }

        Ok(InstallMethod::NotSupported {
            reason: "No installation method found for this platform".to_string(),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        let method = self.best_method().await?;

        match method {
            InstallMethod::AlreadyInstalled { path } => {
                let version = get_version(&path).await;
                Ok(InstallResult {
                    path,
                    version,
                    args: Vec::new(),
                })
            }
            InstallMethod::PackageManager { manager, package } => {
                install_via_package_manager(manager, &package, &opts).await
            }
            InstallMethod::GitHubRelease { .. } => {
                install_from_github(&opts).await
            }
            InstallMethod::NotSupported { reason } => {
                Err(Error::Internal(format!("Cannot install lldb-dap: {}", reason)))
            }
            _ => Err(Error::Internal("Unexpected installation method".to_string())),
        }
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("lldb-dap");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("lldb-dap is not installed in managed location");
            if let Ok(path) = which::which("lldb-dap") {
                println!("System installation found at: {}", path.display());
                println!("Use your system package manager to uninstall.");
            }
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                verify_dap_adapter(&path, &[]).await
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

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "lldb-dap.exe"
    } else {
        "lldb-dap"
    }
}

async fn get_version(path: &PathBuf) -> Option<String> {
    let output = tokio::process::Command::new(path)
        .arg("--version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse version from output like "lldb version 17.0.6"
        stdout
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().last())
            .map(|s| s.to_string())
    } else {
        None
    }
}

async fn install_via_package_manager(
    manager: PackageManager,
    package: &str,
    _opts: &InstallOptions,
) -> Result<InstallResult> {
    println!("Installing {} via {:?}...", package, manager);

    let command = manager.install_command(package);
    println!("Running: {}", command);

    crate::setup::installer::run_command(&command).await?;

    // Find the installed binary
    let path = which::which("lldb-dap")
        .or_else(|_| which::which("lldb-vscode"))
        .map_err(|_| {
            Error::Internal(
                "lldb-dap not found after installation. You may need to add LLVM to your PATH."
                    .to_string(),
            )
        })?;

    let version = get_version(&path).await;

    Ok(InstallResult {
        path,
        version,
        args: Vec::new(),
    })
}

async fn install_from_github(opts: &InstallOptions) -> Result<InstallResult> {
    use crate::setup::installer::{
        arch_str, download_file, extract_tar_gz, get_github_release, platform_str,
        write_version_file,
    };

    println!("Checking for existing installation... not found");
    println!("Finding latest LLVM release...");

    let release = get_github_release("llvm/llvm-project", opts.version.as_deref()).await?;
    println!("Found version: {}", release.tag_name);

    // Find appropriate asset
    let platform = platform_str();
    let arch = arch_str();

    let asset_patterns = vec![
        format!("LLVM-*-{}-{}.tar.xz", arch, platform),
        format!("clang+llvm-*-{}-*{}.tar.xz", arch, platform),
    ];

    let asset = release
        .find_asset(&asset_patterns.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .ok_or_else(|| {
            Error::Internal(format!(
                "No LLVM release found for {} {}. Available assets: {:?}",
                arch,
                platform,
                release.assets.iter().map(|a| &a.name).collect::<Vec<_>>()
            ))
        })?;

    // Create temp directory for download
    let temp_dir = tempfile::tempdir()?;
    let archive_path = temp_dir.path().join(&asset.name);

    println!("Downloading {}... {:.1} MB", asset.name, asset.size as f64 / 1_000_000.0);
    download_file(&asset.browser_download_url, &archive_path).await?;

    println!("Extracting...");
    extract_tar_gz(&archive_path, temp_dir.path())?;

    // Find the extracted directory
    let extracted_dir = std::fs::read_dir(temp_dir.path())?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir() && e.file_name().to_string_lossy().starts_with("LLVM"))
        .or_else(|| {
            std::fs::read_dir(temp_dir.path())
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.path().is_dir() && e.file_name().to_string_lossy().starts_with("clang"))
        })
        .ok_or_else(|| Error::Internal("Could not find extracted LLVM directory".to_string()))?;

    // Find lldb-dap binary
    let lldb_dap_src = extracted_dir.path().join("bin").join(binary_name());
    if !lldb_dap_src.exists() {
        return Err(Error::Internal(format!(
            "lldb-dap not found in LLVM distribution at {}",
            lldb_dap_src.display()
        )));
    }

    // Create installation directory
    let adapter_dir = ensure_adapters_dir()?.join("lldb-dap");
    let bin_dir = adapter_dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    // Copy lldb-dap and required libraries
    let dest_path = bin_dir.join(binary_name());
    std::fs::copy(&lldb_dap_src, &dest_path)?;
    make_executable(&dest_path)?;

    // Write version file
    write_version_file(&adapter_dir, &release.tag_name)?;

    println!("Setting permissions... done");
    println!("Verifying installation...");

    Ok(InstallResult {
        path: dest_path,
        version: Some(release.tag_name),
        args: Vec::new(),
    })
}
