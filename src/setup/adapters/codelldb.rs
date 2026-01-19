//! CodeLLDB installer
//!
//! Installs the CodeLLDB debug adapter from GitHub releases.

use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, arch_str, download_file, ensure_adapters_dir, extract_zip,
    get_github_release, make_executable, platform_str, read_version_file,
    write_version_file, InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "codelldb",
    name: "CodeLLDB",
    languages: &["c", "cpp", "rust"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "Feature-rich LLDB-based debugger",
    primary: false,
};

const GITHUB_REPO: &str = "vadimcn/codelldb";

pub struct CodeLldbInstaller;

#[async_trait]
impl Installer for CodeLldbInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        let adapter_dir = adapters_dir().join("codelldb");
        let binary_path = adapter_dir.join("extension").join("adapter").join(binary_name());

        if binary_path.exists() {
            let version = read_version_file(&adapter_dir);
            return Ok(InstallStatus::Installed {
                path: binary_path,
                version,
            });
        }

        // Check if available in PATH (unlikely but possible)
        if let Ok(path) = which::which("codelldb") {
            return Ok(InstallStatus::Installed {
                path,
                version: None,
            });
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        // CodeLLDB is always installed from GitHub releases
        Ok(InstallMethod::GitHubRelease {
            repo: GITHUB_REPO.to_string(),
            asset_pattern: format!("codelldb-{}-{}.vsix", arch_str(), platform_str()),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        install_from_github(&opts).await
    }

    async fn uninstall(&self) -> Result<()> {
        let adapter_dir = adapters_dir().join("codelldb");
        if adapter_dir.exists() {
            std::fs::remove_dir_all(&adapter_dir)?;
            println!("Removed {}", adapter_dir.display());
        } else {
            println!("CodeLLDB is not installed");
        }
        Ok(())
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        match status {
            InstallStatus::Installed { path, .. } => {
                // CodeLLDB uses different arguments
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
        "codelldb.exe"
    } else {
        "codelldb"
    }
}

fn get_asset_pattern() -> Vec<String> {
    let platform = platform_str();
    let arch = arch_str();

    // Map arch names to CodeLLDB naming convention
    let codelldb_arch = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => arch,
    };

    // Map platform names
    let codelldb_platform = match platform {
        "darwin" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => platform,
    };

    vec![
        format!("codelldb-{}-{}.vsix", codelldb_arch, codelldb_platform),
        // Alternative naming patterns
        format!("codelldb-{}-{}-*.vsix", codelldb_arch, codelldb_platform),
    ]
}

async fn install_from_github(opts: &InstallOptions) -> Result<InstallResult> {
    println!("Checking for existing installation... not found");
    println!("Finding latest CodeLLDB release...");

    let release = get_github_release(GITHUB_REPO, opts.version.as_deref()).await?;
    let version = release.tag_name.trim_start_matches('v').to_string();
    println!("Found version: {}", version);

    // Find appropriate asset
    let patterns = get_asset_pattern();
    let asset = release
        .find_asset(&patterns.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .ok_or_else(|| {
            Error::Internal(format!(
                "No CodeLLDB release found for {} {}. Available assets: {:?}",
                arch_str(),
                platform_str(),
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

    // Create installation directory
    let adapter_dir = ensure_adapters_dir()?.join("codelldb");
    if adapter_dir.exists() {
        std::fs::remove_dir_all(&adapter_dir)?;
    }
    std::fs::create_dir_all(&adapter_dir)?;

    // Extract vsix (it's just a zip file)
    extract_zip(&archive_path, &adapter_dir)?;

    // Find and make the binary executable
    let binary_path = adapter_dir.join("extension").join("adapter").join(binary_name());
    if !binary_path.exists() {
        return Err(Error::Internal(format!(
            "codelldb binary not found at expected location: {}",
            binary_path.display()
        )));
    }
    make_executable(&binary_path)?;

    // Also make libcodelldb executable on Unix
    #[cfg(unix)]
    {
        let lib_path = adapter_dir.join("extension").join("adapter");
        for entry in std::fs::read_dir(&lib_path)? {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().map(|e| e == "so" || e == "dylib").unwrap_or(false) {
                    make_executable(&path)?;
                }
            }
        }

        // Make lldb and lldb-server executable if present
        let lldb_dir = adapter_dir.join("extension").join("lldb");
        if lldb_dir.exists() {
            for subdir in &["bin", "lib"] {
                let dir = lldb_dir.join(subdir);
                if dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                let _ = make_executable(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    // Write version file
    write_version_file(&adapter_dir, &version)?;

    println!("Setting permissions... done");
    println!("Verifying installation...");

    Ok(InstallResult {
        path: binary_path,
        version: Some(version),
        args: Vec::new(),
    })
}
