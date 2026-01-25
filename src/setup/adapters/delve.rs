//! Delve installer
//!
//! Installs the Go debugger with DAP support.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, arch_str, download_file, ensure_adapters_dir, extract_tar_gz,
    get_github_release, make_executable, platform_str, read_version_file, run_command_args,
    write_version_file, InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer,
    PackageManager,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter_tcp, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "go",
    name: "Delve",
    languages: &["go"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "Go debugger with DAP support",
    primary: true,
};

const GITHUB_REPO: &str = "go-delve/delve";

pub struct DelveInstaller;

#[async_trait]
impl Installer for DelveInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        // Check our managed installation first
        let adapter_dir = adapters_dir().join("delve");
        let managed_path = adapter_dir.join("bin").join(binary_name());

        if managed_path.exists() {
            let version = read_version_file(&adapter_dir);
            return Ok(InstallStatus::Installed {
                path: managed_path,
                version,
            });
        }

        // Check if dlv is available in PATH
        if let Ok(path) = which::which("dlv") {
            let version = get_version(&path).await;
            return Ok(InstallStatus::Installed { path, version });
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        // Check if already in PATH
        if let Ok(path) = which::which("dlv") {
            return Ok(InstallMethod::AlreadyInstalled { path });
        }

        let managers = PackageManager::detect();

        // Prefer go install if Go is available
        if managers.contains(&PackageManager::Go) {
            return Ok(InstallMethod::LanguagePackage {
                tool: "go".to_string(),
                package: "github.com/go-delve/delve/cmd/dlv@latest".to_string(),
            });
        }

        // Fallback to GitHub releases
        Ok(InstallMethod::GitHubRelease {
            repo: GITHUB_REPO.to_string(),
            asset_pattern: format!("delve_*_{}_*.tar.gz", platform_str()),
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
                    args: vec!["dap".to_string()],
                })
            }
            InstallMethod::LanguagePackage { tool, package } => {
                install_via_go(&tool, &package, &opts).await
            }
            InstallMethod::GitHubRelease { .. } => install_from_github(&opts).await,
            _ => Err(Error::Internal("Unexpected installation method".to_string())),
        }
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("delve");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("Delve is not installed in managed location");
            if let Ok(path) = which::which("dlv") {
                println!("Found dlv at: {}", path.display());
                println!("If installed via 'go install', it's in your GOPATH/bin.");
            }
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                // Delve uses TCP-based DAP mode with 'dap' subcommand
                verify_dap_adapter_tcp(&path, &["dap".to_string()], crate::common::config::TcpSpawnStyle::TcpListen).await
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
        "dlv.exe"
    } else {
        "dlv"
    }
}

async fn get_version(path: &PathBuf) -> Option<String> {
    let output = tokio::process::Command::new(path)
        .arg("version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse version from output like "Delve Debugger\nVersion: 1.22.0"
        stdout
            .lines()
            .find(|line| line.starts_with("Version:"))
            .and_then(|line| line.strip_prefix("Version:"))
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

async fn install_via_go(tool: &str, package: &str, opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");
    println!("Installing via go install...");

    let package = if let Some(version) = &opts.version {
        format!(
            "github.com/go-delve/delve/cmd/dlv@v{}",
            version.trim_start_matches('v')
        )
    } else {
        package.to_string()
    };

    println!("Running: {} install {}", tool, package);

    // Use run_command_args to prevent command injection
    let go_path = which::which(tool).map_err(|_| {
        Error::Internal(format!("{} not found in PATH", tool))
    })?;
    run_command_args(&go_path, &["install", &package]).await?;

    // Find the installed binary
    let path = which::which("dlv").map_err(|_| {
        Error::Internal(
            "dlv not found after installation. Make sure GOPATH/bin is in your PATH.".to_string(),
        )
    })?;

    let version = get_version(&path).await;

    println!("Setting permissions... done");
    println!("Verifying installation...");

    Ok(InstallResult {
        path,
        version,
        args: vec!["dap".to_string()],
    })
}

async fn install_from_github(opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");
    println!("Finding latest Delve release...");

    let release = get_github_release(GITHUB_REPO, opts.version.as_deref()).await?;
    let version = release.tag_name.trim_start_matches('v').to_string();
    println!("Found version: {}", version);

    // Find appropriate asset
    let platform = platform_str();
    let arch = arch_str();

    // Map arch to delve naming convention
    let delve_arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => arch,
    };

    let patterns = vec![
        format!("delve_{}_{}.tar.gz", platform, delve_arch),
        format!("delve_*_{}_{}.tar.gz", platform, delve_arch),
    ];

    let asset = release
        .find_asset(&patterns.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .ok_or_else(|| {
            Error::Internal(format!(
                "No Delve release found for {} {}. Available assets: {:?}",
                arch,
                platform,
                release.assets.iter().map(|a| &a.name).collect::<Vec<_>>()
            ))
        })?;

    // Create temp directory for download
    let temp_dir = tempfile::tempdir()?;
    let archive_path = temp_dir.path().join(&asset.name);

    println!(
        "Downloading {}... {:.1} MB",
        asset.name,
        asset.size as f64 / 1_000_000.0
    );
    download_file(&asset.browser_download_url, &archive_path).await?;

    println!("Extracting...");
    extract_tar_gz(&archive_path, temp_dir.path())?;

    // Find dlv binary in extracted directory (check root first, then subdirectories)
    let dlv_src = temp_dir.path().join(binary_name());
    let dlv_src = if dlv_src.exists() {
        dlv_src
    } else {
        // Try looking in a subdirectory
        std::fs::read_dir(temp_dir.path())?
            .filter_map(|e| e.ok())
            .find(|e| e.path().is_dir())
            .map(|e| e.path().join(binary_name()))
            .filter(|p| p.exists())
            .ok_or_else(|| Error::Internal("dlv binary not found in downloaded archive".to_string()))?
    };

    // Create installation directory
    let adapter_dir = ensure_adapters_dir()?.join("delve");
    let bin_dir = adapter_dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    // Copy dlv binary
    let dest_path = bin_dir.join(binary_name());
    std::fs::copy(&dlv_src, &dest_path)?;
    make_executable(&dest_path)?;

    // Write version file
    write_version_file(&adapter_dir, &version)?;

    println!("Setting permissions... done");
    println!("Verifying installation...");

    Ok(InstallResult {
        path: dest_path,
        version: Some(version),
        args: vec!["dap".to_string()],
    })
}
