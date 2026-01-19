//! Core installation traits and logic
//!
//! Defines the Installer trait and common installation utilities.

use super::registry::{DebuggerInfo, Platform};
use super::verifier::VerifyResult;
use crate::common::{Error, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

/// Installation status of a debugger
#[derive(Debug, Clone)]
pub enum InstallStatus {
    /// Not installed
    NotInstalled,
    /// Installed at path, with optional version
    Installed {
        path: PathBuf,
        version: Option<String>,
    },
    /// Installed but not working
    Broken { path: PathBuf, reason: String },
}

/// Installation method for a debugger
#[derive(Debug, Clone)]
pub enum InstallMethod {
    /// Use system package manager
    PackageManager {
        manager: PackageManager,
        package: String,
    },
    /// Download from GitHub releases
    GitHubRelease {
        repo: String,
        asset_pattern: String,
    },
    /// Download from direct URL
    DirectDownload { url: String },
    /// Use language-specific package manager
    LanguagePackage { tool: String, package: String },
    /// Extract from VS Code extension
    VsCodeExtension { extension_id: String },
    /// Already available in PATH
    AlreadyInstalled { path: PathBuf },
    /// Cannot install on this platform
    NotSupported { reason: String },
}

/// Package managers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    // Linux
    Apt,
    Dnf,
    Pacman,
    // macOS
    Homebrew,
    // Windows
    Winget,
    Scoop,
    // Cross-platform
    Cargo,
    Pip,
    Go,
}

impl PackageManager {
    /// Detect available package managers
    pub fn detect() -> Vec<PackageManager> {
        let mut found = Vec::new();

        if which::which("apt").is_ok() {
            found.push(PackageManager::Apt);
        }
        if which::which("dnf").is_ok() {
            found.push(PackageManager::Dnf);
        }
        if which::which("pacman").is_ok() {
            found.push(PackageManager::Pacman);
        }
        if which::which("brew").is_ok() {
            found.push(PackageManager::Homebrew);
        }
        if which::which("winget").is_ok() {
            found.push(PackageManager::Winget);
        }
        if which::which("scoop").is_ok() {
            found.push(PackageManager::Scoop);
        }
        if which::which("cargo").is_ok() {
            found.push(PackageManager::Cargo);
        }
        if which::which("pip3").is_ok() || which::which("pip").is_ok() {
            found.push(PackageManager::Pip);
        }
        if which::which("go").is_ok() {
            found.push(PackageManager::Go);
        }

        found
    }

    /// Get install command for a package
    pub fn install_command(&self, package: &str) -> String {
        match self {
            PackageManager::Apt => format!("sudo apt install -y {}", package),
            PackageManager::Dnf => format!("sudo dnf install -y {}", package),
            PackageManager::Pacman => format!("sudo pacman -S --noconfirm {}", package),
            PackageManager::Homebrew => format!("brew install {}", package),
            PackageManager::Winget => format!("winget install {}", package),
            PackageManager::Scoop => format!("scoop install {}", package),
            PackageManager::Cargo => format!("cargo install {}", package),
            PackageManager::Pip => format!("pip3 install {}", package),
            PackageManager::Go => format!("go install {}", package),
        }
    }
}

/// Options for installation
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Specific version to install
    pub version: Option<String>,
    /// Force reinstall
    pub force: bool,
}

/// Result of an installation
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Path to the installed binary
    pub path: PathBuf,
    /// Installed version
    pub version: Option<String>,
    /// Additional arguments needed to run the adapter
    pub args: Vec<String>,
}

/// Trait for debugger installers
#[async_trait]
pub trait Installer: Send + Sync {
    /// Get debugger metadata
    fn info(&self) -> &DebuggerInfo;

    /// Check current installation status
    async fn status(&self) -> Result<InstallStatus>;

    /// Find the best installation method for current platform
    async fn best_method(&self) -> Result<InstallMethod>;

    /// Install the debugger
    async fn install(&self, opts: InstallOptions) -> Result<InstallResult>;

    /// Uninstall the debugger
    async fn uninstall(&self) -> Result<()>;

    /// Verify the installation works
    async fn verify(&self) -> Result<VerifyResult>;
}

/// Get the adapters installation directory
pub fn adapters_dir() -> PathBuf {
    let base = directories::ProjectDirs::from("", "", "debugger-cli")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback to platform-specific paths
            #[cfg(target_os = "linux")]
            let fallback = std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".local/share/debugger-cli");

            #[cfg(target_os = "macos")]
            let fallback = std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("Library/Application Support/debugger-cli");

            #[cfg(target_os = "windows")]
            let fallback = std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("debugger-cli");

            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            let fallback = PathBuf::from(".").join("debugger-cli");

            fallback
        });

    base.join("adapters")
}

/// Ensure the adapters directory exists
pub fn ensure_adapters_dir() -> Result<PathBuf> {
    let dir = adapters_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Download a file with progress reporting
pub async fn download_file(url: &str, dest: &Path) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "debugger-cli")
        .send()
        .await
        .map_err(|e| Error::Internal(format!("Failed to download {}: {}", url, e)))?;

    if !response.status().is_success() {
        return Err(Error::Internal(format!(
            "Download failed with status {}: {}",
            response.status(),
            url
        )));
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = if total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("=> "),
        );
        Some(pb)
    } else {
        println!("  Downloading...");
        None
    };

    let mut file =
        std::fs::File::create(dest).map_err(|e| Error::Internal(format!("Failed to create file: {}", e)))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Internal(format!("Download error: {}", e)))?;
        std::io::Write::write_all(&mut file, &chunk)?;
        downloaded += chunk.len() as u64;
        if let Some(ref pb) = pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    Ok(())
}

/// Extract a zip archive
pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Internal(format!("Failed to open zip: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::Internal(format!("Failed to read zip entry: {}", e)))?;

        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }

        // Set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

/// Extract a tar.gz archive
pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    archive
        .unpack(dest_dir)
        .map_err(|e| Error::Internal(format!("Failed to extract tar.gz: {}", e)))?;

    Ok(())
}

/// Make a file executable on Unix
#[cfg(unix)]
pub fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Run a shell command and return output
pub async fn run_command(command: &str) -> Result<String> {
    let output = if cfg!(windows) {
        tokio::process::Command::new("cmd")
            .args(["/C", command])
            .output()
            .await
    } else {
        tokio::process::Command::new("sh")
            .args(["-c", command])
            .output()
            .await
    };

    let output = output.map_err(|e| Error::Internal(format!("Failed to run command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Internal(format!("Command failed: {}", stderr)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Query GitHub API for latest release
pub async fn get_github_release(repo: &str, version: Option<&str>) -> Result<GitHubRelease> {
    let client = reqwest::Client::new();
    let url = if let Some(v) = version {
        format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo, v
        )
    } else {
        format!("https://api.github.com/repos/{}/releases/latest", repo)
    };

    let response = client
        .get(&url)
        .header("User-Agent", "debugger-cli")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| Error::Internal(format!("GitHub API error: {}", e)))?;

    if !response.status().is_success() {
        return Err(Error::Internal(format!(
            "GitHub API returned status {}",
            response.status()
        )));
    }

    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|e| Error::Internal(format!("Failed to parse GitHub response: {}", e)))?;

    Ok(release)
}

/// GitHub release information
#[derive(Debug, serde::Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

/// GitHub release asset
#[derive(Debug, serde::Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl GitHubRelease {
    /// Find an asset matching a pattern
    pub fn find_asset(&self, patterns: &[&str]) -> Option<&GitHubAsset> {
        for pattern in patterns {
            if let Some(asset) = self.assets.iter().find(|a| {
                let name = a.name.to_lowercase();
                pattern
                    .to_lowercase()
                    .split('*')
                    .all(|part| name.contains(part))
            }) {
                return Some(asset);
            }
        }
        None
    }
}

/// Get current platform string for asset matching
pub fn platform_str() -> &'static str {
    match Platform::current() {
        Platform::Linux => "linux",
        Platform::MacOS => "darwin",
        Platform::Windows => "windows",
    }
}

/// Get current architecture string for asset matching
pub fn arch_str() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    return "x86_64";

    #[cfg(target_arch = "aarch64")]
    return "aarch64";

    #[cfg(target_arch = "x86")]
    return "i686";

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    return "unknown";
}

/// Write version info to a file
pub fn write_version_file(dir: &Path, version: &str) -> Result<()> {
    let version_file = dir.join("version.txt");
    std::fs::write(&version_file, version)?;
    Ok(())
}

/// Read version from a version file
pub fn read_version_file(dir: &Path) -> Option<String> {
    let version_file = dir.join("version.txt");
    std::fs::read_to_string(&version_file)
        .ok()
        .map(|s| s.trim().to_string())
}
