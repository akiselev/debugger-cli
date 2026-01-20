# Setup Command Design

## Overview

The `setup` command intelligently installs DAP-supported debuggers in a cross-platform, robust way. This enables agents to quickly get up and running with a single command like `debugger setup lldb`.

## Goals

1. **Zero-friction setup** - One command to install a working debugger
2. **Cross-platform** - Works on Linux, macOS, and Windows
3. **Intelligent detection** - Detect what's already installed, suggest alternatives
4. **Robust installation** - Handle failures gracefully, verify installations
5. **Offline capability** - Support pre-downloaded binaries where possible

## Supported Debuggers

### Primary Targets

| Debugger | Languages | Platforms | Installation Method |
|----------|-----------|-----------|---------------------|
| `lldb-dap` | C, C++, Rust, Swift | Linux, macOS, Windows | LLVM releases / package managers |
| `codelldb` | C, C++, Rust | All | VS Code extension extraction / GitHub releases |
| `debugpy` | Python | All | pip install |
| `delve` (dlv-dap) | Go | All | go install / GitHub releases |
| `js-debug` | JS, TS, Node | All | VS Code extension extraction |
| `cpptools` | C, C++ | All | VS Code extension extraction |
| `netcoredbg` | C#, .NET | All | GitHub releases |
| `java-debug` | Java | All | VS Code extension extraction |

### Future Targets

- `gdb` (via gdb-dap wrapper) - C, C++, embedded
- `php-debug` - PHP
- `ruby-debug` - Ruby
- `perl-debug` - Perl

## Command Interface

```bash
# Install a specific debugger
debugger setup lldb
debugger setup codelldb
debugger setup python
debugger setup go

# Install with specific version
debugger setup lldb --version 18.1.0

# List available debuggers and their status
debugger setup --list

# Check what's installed and working
debugger setup --check

# Install all debuggers for detected project types
debugger setup --auto

# Uninstall a debugger
debugger setup --uninstall lldb

# Show installation location
debugger setup --path lldb

# Force reinstall
debugger setup lldb --force

# Dry run (show what would be installed)
debugger setup lldb --dry-run
```

## Architecture

### Module Structure

```
src/
  setup/
    mod.rs           # Main setup command handling
    registry.rs      # Debugger registry and metadata
    installer.rs     # Core installation trait and logic
    detector.rs      # Detect installed debuggers and project types
    verifier.rs      # Verify installations work
    adapters/
      mod.rs
      lldb.rs        # LLVM/lldb-dap installation
      codelldb.rs    # CodeLLDB installation
      debugpy.rs     # Python debugpy installation
      delve.rs       # Go delve installation
      js_debug.rs    # JS/TS debugger
      netcoredbg.rs  # .NET debugger
```

### Core Traits

```rust
/// Information about a debugger
pub struct DebuggerInfo {
    /// Unique identifier (e.g., "lldb", "codelldb")
    pub id: &'static str,
    /// Display name
    pub name: &'static str,
    /// Supported languages
    pub languages: &'static [&'static str],
    /// Supported platforms
    pub platforms: &'static [Platform],
    /// Brief description
    pub description: &'static str,
    /// Whether this is the primary adapter for its languages
    pub primary: bool,
}

pub enum Platform {
    Linux,
    MacOS,
    Windows,
}

/// Installation status
pub enum InstallStatus {
    /// Not installed
    NotInstalled,
    /// Installed at path, with version
    Installed { path: PathBuf, version: Option<String> },
    /// Installed but not working (e.g., missing deps)
    Broken { path: PathBuf, reason: String },
}

/// Installation method for a debugger
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

pub enum InstallMethod {
    /// Use system package manager (apt, brew, winget, etc.)
    PackageManager { manager: PackageManager, package: String },
    /// Download from GitHub releases
    GitHubRelease { repo: String, asset_pattern: String },
    /// Download from direct URL
    DirectDownload { url: String },
    /// Use language-specific package manager (pip, cargo, go install)
    LanguagePackage { tool: String, package: String },
    /// Extract from VS Code extension
    VsCodeExtension { extension_id: String },
    /// Already available in PATH
    AlreadyInstalled { path: PathBuf },
    /// Cannot install on this platform
    NotSupported { reason: String },
}
```

### Installation Directory

Debuggers are installed to a dedicated directory managed by debugger-cli:

```
~/.local/share/debugger-cli/adapters/  (Linux)
~/Library/Application Support/debugger-cli/adapters/  (macOS)
%LOCALAPPDATA%\debugger-cli\adapters\  (Windows)

adapters/
  lldb-dap/
    bin/
      lldb-dap (or lldb-dap.exe)
    version.txt
  codelldb/
    bin/
      codelldb
    extension/
      ...
    version.txt
  debugpy/
    venv/
      ...
    version.txt
```

## Rust Crates for Cross-Platform Installation

### Core Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| [`reqwest`](https://crates.io/crates/reqwest) | HTTP downloads | Async, supports streaming |
| [`zip`](https://crates.io/crates/zip) | Extract zip archives | Most GitHub releases |
| [`tar`](https://crates.io/crates/tar) | Extract tar archives | Linux/macOS releases |
| [`flate2`](https://crates.io/crates/flate2) | Gzip decompression | For .tar.gz |
| [`xz2`](https://crates.io/crates/xz2) | XZ decompression | For .tar.xz |
| [`indicatif`](https://crates.io/crates/indicatif) | Progress bars | Download/extract progress |
| [`semver`](https://crates.io/crates/semver) | Version parsing | Compare versions |
| [`self_update`](https://crates.io/crates/self_update) | Self-update pattern | Can borrow GitHub release logic |
| [`octocrab`](https://crates.io/crates/octocrab) | GitHub API | Find latest releases |

### Platform Detection & Package Managers

| Crate | Purpose | Notes |
|-------|---------|-------|
| [`os_info`](https://crates.io/crates/os_info) | OS detection | Detailed OS info |
| [`which`](https://crates.io/crates/which) | Find executables | Already in deps |
| [`sysinfo`](https://crates.io/crates/sysinfo) | System info | CPU arch, memory |

### File Operations

| Crate | Purpose | Notes |
|-------|---------|-------|
| [`fs_extra`](https://crates.io/crates/fs_extra) | Enhanced file ops | Copy dirs recursively |
| [`tempfile`](https://crates.io/crates/tempfile) | Temp directories | Safe download staging |

### Optional: Shell Script Alternative

If we want to delegate to proven shell scripts:

| Crate | Purpose | Notes |
|-------|---------|-------|
| [`run_script`](https://crates.io/crates/run_script) | Run shell scripts | Cross-platform |
| [`shell-words`](https://crates.io/crates/shell-words) | Parse shell strings | Quote handling |

## Installation Strategies

### 1. GitHub Releases (Primary)

Most debuggers publish releases on GitHub. This is the most reliable cross-platform method.

```rust
async fn install_from_github(
    repo: &str,           // "vadimcn/codelldb"
    version: Option<&str>, // None = latest
    asset_pattern: &str,   // "codelldb-{arch}-{os}.vsix"
) -> Result<PathBuf> {
    // 1. Query GitHub API for release
    // 2. Match asset pattern to current platform
    // 3. Download to temp dir with progress
    // 4. Verify checksum if available
    // 5. Extract to adapters directory
    // 6. Set executable permissions (Unix)
    // 7. Verify binary works
}
```

### 2. Package Managers (Fallback/Preference)

Use system package managers when available:

```rust
pub enum PackageManager {
    // Linux
    Apt,      // Debian/Ubuntu
    Dnf,      // Fedora/RHEL
    Pacman,   // Arch
    Zypper,   // openSUSE
    Apk,      // Alpine
    Nix,      // NixOS / Nix
    
    // macOS
    Homebrew,
    MacPorts,
    
    // Windows
    Winget,
    Scoop,
    Chocolatey,
    
    // Cross-platform
    Cargo,    // Rust
    Pip,      // Python
    Npm,      // Node.js
    Go,       // Go
}

impl PackageManager {
    fn detect() -> Vec<PackageManager> {
        let mut found = Vec::new();
        if which("apt").is_ok() { found.push(Apt); }
        if which("brew").is_ok() { found.push(Homebrew); }
        // ... etc
        found
    }
    
    async fn install(&self, package: &str) -> Result<()> {
        let cmd = match self {
            Apt => format!("sudo apt install -y {}", package),
            Homebrew => format!("brew install {}", package),
            Winget => format!("winget install {}", package),
            // ...
        };
        run_command(&cmd).await
    }
}
```

### 3. VS Code Extension Extraction

Many debuggers are distributed as VS Code extensions. We can extract the DAP adapter:

```rust
async fn install_from_vscode_extension(
    extension_id: &str,  // "vadimcn.vscode-lldb"
    binary_path: &str,   // "adapter/codelldb" within extension
) -> Result<PathBuf> {
    // 1. Query VS Code marketplace API for latest version
    // 2. Download .vsix (it's just a zip file)
    // 3. Extract to adapters directory
    // 4. Locate binary within extension
    // 5. Set executable permissions
    // 6. Verify binary works
}
```

### 4. Language Package Managers

For language-specific debuggers:

```rust
// Python - debugpy
async fn install_debugpy() -> Result<PathBuf> {
    // Create isolated venv
    let venv_path = adapters_dir().join("debugpy/venv");
    run_command(&format!("python3 -m venv {}", venv_path.display())).await?;
    
    // Install debugpy into venv
    let pip = venv_path.join("bin/pip");
    run_command(&format!("{} install debugpy", pip.display())).await?;
    
    Ok(venv_path.join("bin/python"))
}

// Go - delve
async fn install_delve() -> Result<PathBuf> {
    run_command("go install github.com/go-delve/delve/cmd/dlv@latest").await?;
    which("dlv").map_err(|_| Error::InstallFailed("dlv not in PATH after install"))
}
```

## Verification

After installation, verify the debugger works:

```rust
async fn verify_dap_adapter(path: &Path) -> Result<VerifyResult> {
    // 1. Spawn adapter process
    // 2. Send DAP initialize request
    // 3. Check for valid initialize response
    // 4. Send disconnect request
    // 5. Verify clean exit
}
```

## Configuration Integration

After installation, automatically update `config.toml`:

```toml
[adapters.lldb]
path = "/home/user/.local/share/debugger-cli/adapters/lldb-dap/bin/lldb-dap"

[adapters.codelldb]
path = "/home/user/.local/share/debugger-cli/adapters/codelldb/bin/codelldb"

[adapters.python]
path = "/home/user/.local/share/debugger-cli/adapters/debugpy/venv/bin/python"
args = ["-m", "debugpy.adapter"]
```

## Project Type Detection

For `--auto` mode, detect project types:

```rust
fn detect_project_types(dir: &Path) -> Vec<ProjectType> {
    let mut types = Vec::new();
    
    if dir.join("Cargo.toml").exists() {
        types.push(ProjectType::Rust);
    }
    if dir.join("go.mod").exists() {
        types.push(ProjectType::Go);
    }
    if dir.join("package.json").exists() {
        types.push(ProjectType::JavaScript);
    }
    if dir.join("pyproject.toml").exists() || dir.join("setup.py").exists() {
        types.push(ProjectType::Python);
    }
    // ... etc
    
    types
}

fn debuggers_for_project(project: ProjectType) -> Vec<&'static str> {
    match project {
        ProjectType::Rust => vec!["codelldb", "lldb"],
        ProjectType::Go => vec!["delve"],
        ProjectType::Python => vec!["debugpy"],
        ProjectType::JavaScript => vec!["js-debug"],
        ProjectType::CSharp => vec!["netcoredbg"],
        // ...
    }
}
```

## Error Handling

Provide helpful error messages:

```rust
pub enum SetupError {
    /// Network error during download
    NetworkError { url: String, source: reqwest::Error },
    
    /// GitHub API rate limit
    RateLimited { reset_time: DateTime<Utc> },
    
    /// No compatible release for this platform
    NoPlatformRelease { debugger: String, platform: String },
    
    /// Package manager not available
    NoPackageManager { debugger: String },
    
    /// Verification failed after install
    VerificationFailed { debugger: String, reason: String },
    
    /// Missing dependency
    MissingDependency { debugger: String, dependency: String },
    
    /// Insufficient permissions
    PermissionDenied { path: PathBuf },
}

impl SetupError {
    fn suggestion(&self) -> &str {
        match self {
            Self::RateLimited { .. } => 
                "Set GITHUB_TOKEN env var to increase rate limit",
            Self::NoPackageManager { .. } => 
                "Install via GitHub release instead: debugger setup lldb --method github",
            Self::MissingDependency { dependency, .. } =>
                &format!("Install {} first, then retry", dependency),
            // ...
        }
    }
}
```

## Progress Reporting

Show clear progress for agents and humans:

```
$ debugger setup codelldb

Checking for existing installation... not found
Finding latest release... v1.10.0
Downloading codelldb-x86_64-linux.vsix... 45.2 MB
  [████████████████████████████████████████] 100%
Extracting... done
Setting permissions... done
Verifying installation... success

✓ CodeLLDB v1.10.0 installed to ~/.local/share/debugger-cli/adapters/codelldb

Configuration updated. Use 'debugger start --adapter codelldb ./program' to debug.
```

For JSON output (agent-friendly):

```bash
$ debugger setup codelldb --json
```

```json
{
  "status": "success",
  "debugger": "codelldb",
  "version": "1.10.0",
  "path": "/home/user/.local/share/debugger-cli/adapters/codelldb/bin/codelldb",
  "languages": ["c", "cpp", "rust"]
}
```

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Add new dependencies to Cargo.toml
- [ ] Create `src/setup/` module structure
- [ ] Implement `InstallStatus` detection
- [ ] Implement GitHub release download
- [ ] Implement archive extraction (zip, tar.gz)
- [ ] Implement verification framework

### Phase 2: First Debuggers
- [ ] `lldb-dap` installer (GitHub releases + Homebrew)
- [ ] `codelldb` installer (GitHub releases / VS Code)
- [ ] `debugpy` installer (pip)
- [ ] `delve` installer (go install / GitHub)

### Phase 3: Polish
- [ ] Progress bars and colored output
- [ ] JSON output mode
- [ ] `--list` and `--check` commands
- [ ] `--auto` project detection
- [ ] Configuration auto-update

### Phase 4: Extended Debuggers
- [ ] `js-debug` installer
- [ ] `netcoredbg` installer
- [ ] `java-debug` installer

## Security Considerations

1. **Checksum verification** - Verify SHA256 checksums when available
2. **HTTPS only** - All downloads over HTTPS
3. **No sudo by default** - Install to user directories
4. **Signature verification** - Support GPG signatures where available
5. **Permission restrictions** - Set minimal permissions on installed binaries

## Dependencies to Add

```toml
[dependencies]
# For setup command
reqwest = { version = "0.12", features = ["json", "stream"] }
zip = "2.2"
tar = "0.4"
flate2 = "1.0"
indicatif = "0.17"
semver = "1.0"
octocrab = "0.41"
tempfile = "3.14"
os_info = "3.9"

# Optional for xz archives
xz2 = "0.1"
```

## Open Questions

1. **Should we support offline installation from local archives?**
   - Use case: Air-gapped environments, CI caching

2. **Should we integrate with VS Code settings?**
   - Auto-detect adapters installed by VS Code extensions

3. **Should we support multiple versions simultaneously?**
   - Use case: Testing against different LLVM versions

4. **Should we support proxy configuration?**
   - For corporate environments

5. **Should we have a global lock file to prevent concurrent installs?**
   - Prevent race conditions if multiple debugger commands run
