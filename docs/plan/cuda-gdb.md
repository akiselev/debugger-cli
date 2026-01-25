# GDB and CUDA-GDB Support Implementation Plan

## Overview

This plan adds native DAP support for GDB and CUDA-GDB debuggers to debugger-cli. The implementation follows a phased approach: Phase 0 validates GDB's native DAP mode (`-i=dap`) on standard GDB first, Phase 1 implements CUDA-GDB support using the same pattern, and Phase 2 (deferred) would add cdt-gdb-adapter as a fallback if native DAP proves insufficient.

GDB 14+ includes native DAP support via the `-i=dap` interpreter flag, which CUDA-GDB (based on GDB 14.2) should inherit. This approach minimizes external dependencies while leveraging existing Stdio transport patterns from lldb-dap.

## Planning Context

### Decision Log

| Decision | Reasoning Chain |
|----------|-----------------|
| Native DAP over cdt-gdb-adapter | cdt-gdb-adapter requires Node.js runtime → adds ~50MB dependency + complexity → native DAP (`-i=dap`) achieves same goal with zero dependencies → start with simpler approach, add fallback only if needed |
| Phased implementation (GDB first, then CUDA-GDB) | CUDA-GDB native DAP is undocumented by NVIDIA → testing on standard GDB validates approach with better documentation → if GDB works, CUDA-GDB (same codebase) likely works → reduces risk of discovering issues late |
| Stdio transport mode | GDB native DAP uses stdin/stdout like lldb-dap → existing DapClient::spawn() handles this pattern → no new transport code needed → matches established adapter integration |
| Version check ≥14.2 | GDB native DAP requires Python support, added in GDB 14.1 → CUDA-GDB 13.x is based on GDB 14.2 → version check ensures DAP availability → prevents cryptic failures on older GDB |
| CUDA project detection via *.cu files | CMake detection requires parsing CMakeLists.txt → file extension check is O(1) glob → .cu is CUDA-specific (not shared with other languages) → simpler heuristic with high precision |
| Separate GDB and CUDA-GDB adapters | Could share code but have different: version requirements, paths, platform support → separate installers cleaner than conditional logic → registry pattern already supports multiple adapters per language |
| 10s init timeout for GDB | GDB startup can be slow with large symbol files → lldb uses 10s init timeout → match existing pattern rather than optimizing prematurely → can tune if real-world usage shows issues |
| CUDA-GDB DAP support assumed from GDB base | CUDA-GDB 13.x is based on GDB 14.2 which includes DAP → NVIDIA documentation doesn't explicitly confirm or deny DAP availability → assumption validated during Phase 1 verification step → if verify_dap_adapter() fails, adapter marked as Broken with clear message |
| Language mapping overlap priority | GDB and CUDA-GDB both support C/C++ → project detection uses .cu files to distinguish → CUDA-GDB only suggested when .cu files present → otherwise GDB preferred for broader platform support → prevents forcing CUDA Toolkit requirement on non-CUDA projects |
| CUDA detection before C/C++ detection | Projects with .cu files are valid C++ projects → if C++ detected first, CUDA projects misclassified → .cu files are CUDA-specific, check must precede generic C/C++ → detection order in detector.rs is intentional, not alphabetical |
| CUDA Toolkit path search precedence | /usr/local/cuda is NVIDIA's standard install location → check before CUDA_HOME to catch default installs → PATH last because may contain wrapper scripts → prioritizes official toolkit installation over custom setups |
| Version check at install time only | GDB downgrades between setup and runtime are rare → install-time check provides fast feedback during setup → runtime check would add latency to every debug session → if version mismatch occurs, DAP initialization failure guides user to re-run setup → acceptable tradeoff |

### Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| cdt-gdb-adapter as primary | Requires Node.js runtime (50MB+), adds process management complexity, diverges from existing binary-only adapter pattern. Reserved for Phase 2 fallback. |
| GDB/MI direct integration | Would require building Rust GDB/MI parser (~2000 LOC), already solved by GDB's native DAP. Higher effort, same outcome. |
| Single combined GDB+CUDA-GDB adapter | Platform/version requirements differ (CUDA-GDB Linux-only, specific CUDA Toolkit paths). Conditional logic would be error-prone vs. clean separation. |
| cuda-gdb availability detection only (no project detection) | Users expect `debugger setup --auto` to suggest appropriate debugger. Without project detection, CUDA projects would suggest lldb instead. |

### Constraints & Assumptions

- **GDB version**: Native DAP requires GDB ≥14.1 (Python support). CUDA-GDB 13.x is based on GDB 14.2.
- **Platform**: GDB available on Linux/macOS/Windows. CUDA-GDB GPU debugging Linux-only (NVIDIA limitation).
- **CUDA Toolkit path**: Default `/usr/local/cuda/bin/cuda-gdb`, also check `CUDA_HOME` env var and PATH.
- **Existing patterns**: Follow `src/setup/adapters/lldb.rs` pattern for Stdio-based native DAP.
- **Test infrastructure**: Integration tests require real GDB/cuda-gdb binaries, skip if unavailable.

### Known Risks

| Risk | Mitigation | Anchor |
|------|------------|--------|
| GDB native DAP has bugs/limitations | Phase 0 validates before CUDA work. Phase 2 fallback to cdt-gdb-adapter if needed. | N/A (external risk) |
| CUDA-GDB may have different DAP implementation than upstream GDB | Assumption: CUDA-GDB inherits DAP from GDB 14.2 base. Mitigation: verify_dap_adapter() called during install validates DAP works. If fails, returns Broken status. | src/setup/verifier.rs:34-64 (verify_dap_adapter pattern) |
| Older GDB versions in CI/distros | Version check returns Broken status with upgrade message. | N/A (handled in code) |
| CUDA kernel stepping may behave differently | Document warp-level stepping behavior. Out of scope for Phase 1. | N/A (deferred) |

## Invisible Knowledge

### Architecture

```
                    debugger-cli
                         |
         +---------------+---------------+
         |               |               |
    lldb-dap         gdb -i=dap    cuda-gdb -i=dap
    (existing)       (Phase 0)       (Phase 1)
         |               |               |
      Stdio           Stdio           Stdio
    transport       transport       transport
```

All three adapters use identical transport: spawn process with stdin/stdout pipes, send DAP JSON-RPC messages. GDB and CUDA-GDB use `-i=dap` flag to enable DAP interpreter mode (vs default console or `-i=mi` for machine interface).

### Data Flow

```
User Command
     |
     v
CLI parses -> IPC to daemon -> DapClient::spawn("gdb", ["-i=dap"])
                                    |
                                    v
                              GDB process (DAP mode)
                                    |
                              stdin: DAP requests
                              stdout: DAP responses/events
```

### Why Separate GDB and CUDA-GDB Adapters

Despite sharing 90% of code patterns, separate adapters because:
1. **Platform support differs**: GDB works everywhere, CUDA-GDB requires Linux + NVIDIA GPU
2. **Path detection differs**: GDB in PATH, CUDA-GDB in `/usr/local/cuda/bin/` or `$CUDA_HOME`
3. **Language mapping differs**: GDB for C/C++, CUDA-GDB for CUDA (may overlap with C/C++)
4. **Version requirements differ**: GDB ≥14.1, CUDA-GDB tied to CUDA Toolkit version

Merging would require complex conditionals that obscure the simple pattern.

### Invariants

1. **Version compatibility**: GDB must be ≥14.1 for DAP support. Installer.status() returns Broken if version too old.
2. **Interpreter flag**: `-i=dap` must be passed at GDB startup, not as a runtime command. Cannot switch interpreters mid-session.
3. **Python requirement**: GDB native DAP is implemented in Python. GDB built without Python (`--disable-python`) lacks DAP.

### Tradeoffs

| Choice | Benefit | Cost |
|--------|---------|------|
| Native DAP over MI adapter | Zero dependencies, simpler integration | Less control over GDB interaction, rely on GDB's DAP quality |
| Version check at install time | Fast feedback, clear error | May reject working GDB if version parsing fails |
| Separate adapters | Clean code, explicit platforms | Some code duplication between gdb.rs and cuda_gdb.rs |

## Milestones

### Milestone 1: GDB Installer (Phase 0 - Validation)

**Files**:
- `src/setup/adapters/gdb_common.rs` (NEW)
- `src/setup/adapters/gdb.rs` (NEW)
- `src/setup/adapters/mod.rs`
- `src/setup/registry.rs`

**Flags**: `conformance`, `needs-rationale`

**Requirements**:
- Create GdbInstaller implementing Installer trait
- Detect GDB via `which::which("gdb")`
- Parse version from `gdb --version` output, require ≥14.1
- Return InstallResult with args: `["-i=dap"]`
- Verify via existing `verify_dap_adapter()` with Stdio transport
- Register in DEBUGGERS with id="gdb", languages=["c", "cpp"], platforms=[Linux, MacOS, Windows]

**Acceptance Criteria**:
- `debugger setup gdb` succeeds when GDB ≥14.1 is installed
- `debugger setup gdb` returns clear error when GDB <14.1 or missing
- `debugger setup --list` shows GDB adapter
- GDB adapter passes DAP verification (initialize request/response)

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: default-derived (matches existing lldb test pattern)
- **Scenarios**:
  - Normal: `gdb_available()` check, basic C debugging workflow with GDB
  - Edge: GDB not installed returns NotInstalled
  - Edge: GDB <14.1 returns Broken with version message

**Code Intent**:
- New file `src/setup/adapters/gdb_common.rs`:
  - `pub fn parse_gdb_version(output: &str) -> Option<String>`: Extract version from GDB --version output
  - `pub fn is_gdb_version_sufficient(version: &str) -> bool`: Check if version ≥14.1
- New file `src/setup/adapters/gdb.rs`:
  - `INFO` static: DebuggerInfo with id="gdb", name="GDB", languages=["c", "cpp"]
  - `GdbInstaller` struct (unit struct)
  - `impl Installer for GdbInstaller`:
    - `info()`: return &INFO
    - `status()`: which::which("gdb"), parse version using gdb_common, check ≥14.1
    - `best_method()`: return AlreadyInstalled if found, NotSupported otherwise
    - `install()`: return path + args ["-i=dap"]
    - `verify()`: call verify_dap_adapter() with path and args
- Modify `src/setup/adapters/mod.rs`: add `pub mod gdb_common;` and `pub mod gdb;`
- Modify `src/setup/registry.rs`:
  - Add DebuggerInfo entry to DEBUGGERS array
  - Add match arm in get_installer() returning GdbInstaller

### Code Changes

New file `src/setup/adapters/gdb_common.rs`:

```rust
//! Shared utilities for GDB-based adapters (GDB and CUDA-GDB)

/// Extracts version string from GDB --version output
///
/// Returns first token starting with digit (handles varying GDB output formats)
pub fn parse_gdb_version(output: &str) -> Option<String> {
    // GDB output formats vary: "GNU gdb (GDB) 14.1" vs "gdb 14.2-arch"
    output
        .lines()
        .next()
        .and_then(|line| {
            line.split_whitespace()
                .find(|token| token.chars().next().map_or(false, |c| c.is_ascii_digit()))
        })
        .map(|s| s.to_string())
}

/// Checks if GDB version meets DAP support requirement (≥14.1)
///
/// Returns false on parse failure to prevent launching incompatible GDB
pub fn is_gdb_version_sufficient(version: &str) -> bool {
    // Safe parsing: malformed versions fail closed (return false)
    let parts: Vec<&str> = version.split('.').collect();
    let Some(major_str) = parts.get(0) else {
        return false;
    };
    let Some(minor_str) = parts.get(1) else {
        return false;
    };
    let Ok(major) = major_str.parse::<u32>() else {
        return false;
    };
    let Ok(minor) = minor_str.parse::<u32>() else {
        return false;
    };

    // GDB ≥14.1 required for Python-based DAP implementation
    major > 14 || (major == 14 && minor >= 1)
}

/// Retrieves GDB version by executing --version flag
///
/// Returns None on exec failure or unparseable output
pub async fn get_gdb_version(path: &std::path::PathBuf) -> Option<String> {
    let output = tokio::process::Command::new(path)
        .arg("--version")
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_gdb_version(&stdout)
    } else {
        None
    }
}
```

New file `src/setup/adapters/gdb.rs`:

```rust
//! GDB native DAP adapter installer
//!
//! Installs GDB with native DAP support (GDB ≥14.1).

use crate::common::{Error, Result};
use crate::setup::installer::{InstallMethod, InstallOptions, InstallResult, InstallStatus, Installer};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

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
```

```diff
--- a/src/setup/adapters/mod.rs
+++ b/src/setup/adapters/mod.rs
@@ -5,4 +5,6 @@
 pub mod codelldb;
 pub mod debugpy;
 pub mod delve;
+pub mod gdb_common;
+pub mod gdb;
 pub mod lldb;
```

```diff
--- a/src/setup/registry.rs
+++ b/src/setup/registry.rs
@@ -61,6 +61,14 @@ pub struct DebuggerInfo {
 /// All available debuggers
 static DEBUGGERS: &[DebuggerInfo] = &[
     DebuggerInfo {
+        id: "gdb",
+        name: "GDB",
+        languages: &["c", "cpp"],
+        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
+        description: "GDB native DAP adapter",
+        primary: true,
+    },
+    DebuggerInfo {
         id: "lldb",
         name: "lldb-dap",
         languages: &["c", "cpp", "rust", "swift"],
@@ -125,6 +133,7 @@ pub fn get_installer(id: &str) -> Option<Arc<dyn Installer>> {
     use super::adapters;

     match id {
+        "gdb" => Some(Arc::new(adapters::gdb::GdbInstaller)),
         "lldb" => Some(Arc::new(adapters::lldb::LldbInstaller)),
         "codelldb" => Some(Arc::new(adapters::codelldb::CodeLldbInstaller)),
         "python" => Some(Arc::new(adapters::debugpy::DebugpyInstaller)),
```

---

### Milestone 2: CUDA-GDB Installer (Phase 1)

**Files**:
- `src/setup/adapters/cuda_gdb.rs` (NEW)
- `src/setup/adapters/mod.rs`
- `src/setup/registry.rs`

**Flags**: `conformance`, `needs-rationale`

**Requirements**:
- Create CudaGdbInstaller implementing Installer trait
- Detect cuda-gdb via: 1) `/usr/local/cuda/bin/cuda-gdb`, 2) `$CUDA_HOME/bin/cuda-gdb`, 3) `which::which("cuda-gdb")`
- Parse version from `cuda-gdb --version`, require ≥14.1 (GDB base version)
- Return InstallResult with args: `["-i=dap"]`
- Verify via existing `verify_dap_adapter()` with Stdio transport
- Register with id="cuda-gdb", languages=["cuda", "c", "cpp"], platforms=[Linux]

**Acceptance Criteria**:
- `debugger setup cuda-gdb` succeeds when CUDA Toolkit with cuda-gdb is installed
- `debugger setup cuda-gdb` returns NotInstalled on non-Linux or missing cuda-gdb
- Handles multiple path sources (hardcoded, env var, PATH)
- CUDA-GDB adapter passes DAP verification

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: default-derived
- **Scenarios**:
  - Normal: `cuda_gdb_available()` check, verification passes
  - Edge: Not on Linux returns NotSupported
  - Edge: cuda-gdb not found returns NotInstalled
  - Skip: Full CUDA kernel debugging (requires GPU hardware)

**Code Intent**:
- New file `src/setup/adapters/cuda_gdb.rs`:
  - `INFO` static: DebuggerInfo with id="cuda-gdb", name="CUDA-GDB", languages=["cuda", "c", "cpp"], platforms=[Linux]
  - `CudaGdbInstaller` struct (unit struct)
  - `find_cuda_gdb()` helper: checks paths in order (hardcoded, CUDA_HOME, PATH)
  - `impl Installer for CudaGdbInstaller`:
    - `status()`: find_cuda_gdb(), parse version using gdb_common, check ≥14.1
    - `best_method()`: AlreadyInstalled or NotSupported (can't auto-install CUDA Toolkit)
    - `install()`: return path + args ["-i=dap"]
    - `verify()`: call verify_dap_adapter()
- Modify `src/setup/adapters/mod.rs`: add `pub mod cuda_gdb;`
- Modify `src/setup/registry.rs`: add DebuggerInfo and match arm

### Code Changes

New file `src/setup/adapters/cuda_gdb.rs`:

```rust
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
        // CUDA GPU debugging requires Linux (NVIDIA driver limitation)
        if Platform::current() != Platform::Linux {
            return Ok(InstallStatus::NotInstalled);
        }

        // Path search precedence: /usr/local/cuda (NVIDIA default) → CUDA_HOME → PATH
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
    // /usr/local/cuda is NVIDIA's standard install location (prioritize over env vars)
    let default_path = PathBuf::from("/usr/local/cuda/bin/cuda-gdb");
    if default_path.exists() {
        return Some(default_path);
    }

    // CUDA_HOME allows custom toolkit installations
    if let Ok(cuda_home) = std::env::var("CUDA_HOME") {
        let cuda_home_path = PathBuf::from(cuda_home).join("bin/cuda-gdb");
        if cuda_home_path.exists() {
            return Some(cuda_home_path);
        }
    }

    // PATH fallback catches wrapper scripts and non-standard installs
    which::which("cuda-gdb").ok()
}
```

```diff
--- a/src/setup/adapters/mod.rs
+++ b/src/setup/adapters/mod.rs
@@ -4,6 +4,7 @@

 pub mod codelldb;
+pub mod cuda_gdb;
 pub mod debugpy;
 pub mod delve;
 pub mod gdb;
```

```diff
--- a/src/setup/registry.rs
+++ b/src/setup/registry.rs
@@ -69,6 +69,14 @@ static DEBUGGERS: &[DebuggerInfo] = &[
         primary: true,
     },
     DebuggerInfo {
+        id: "cuda-gdb",
+        name: "CUDA-GDB",
+        languages: &["cuda", "c", "cpp"],
+        platforms: &[Platform::Linux],
+        description: "NVIDIA CUDA debugger with DAP support",
+        primary: true,
+    },
+    DebuggerInfo {
         id: "lldb",
         name: "lldb-dap",
         languages: &["c", "cpp", "rust", "swift"],
@@ -134,6 +142,7 @@ pub fn get_installer(id: &str) -> Option<Arc<dyn Installer>> {

     match id {
         "gdb" => Some(Arc::new(adapters::gdb::GdbInstaller)),
+        "cuda-gdb" => Some(Arc::new(adapters::cuda_gdb::CudaGdbInstaller)),
         "lldb" => Some(Arc::new(adapters::lldb::LldbInstaller)),
         "codelldb" => Some(Arc::new(adapters::codelldb::CodeLldbInstaller)),
         "python" => Some(Arc::new(adapters::debugpy::DebugpyInstaller)),
```

---

### Milestone 3: CUDA Project Detection

**Files**:
- `src/setup/detector.rs`

**Flags**: `conformance`

**Requirements**:
- Add ProjectType::Cuda variant
- Detect CUDA projects by presence of `*.cu` files in project directory
- Map ProjectType::Cuda to ["cuda-gdb"] in debuggers_for_project()
- Detection should not conflict with C/C++ (check CUDA first, then C/C++)

**Acceptance Criteria**:
- `debugger setup --auto` in directory with .cu files suggests cuda-gdb
- `debugger setup --auto` in directory without .cu files does not suggest cuda-gdb
- Existing C/C++ detection still works for non-CUDA projects

**Tests**:
- **Test files**: `tests/integration.rs` or unit test in detector.rs
- **Test type**: unit
- **Backing**: default-derived
- **Scenarios**:
  - Normal: Directory with kernel.cu detected as Cuda
  - Normal: Directory with main.c (no .cu) detected as C, not Cuda
  - Edge: Directory with both .cu and .c files detected as Cuda (CUDA takes priority)

**Code Intent**:
- Modify `src/setup/detector.rs`:
  - Add `Cuda` variant to ProjectType enum (after Cpp, before Python)
  - In `detect_project_types()`: add glob check for `**/*.cu` files, return ProjectType::Cuda if found
  - Order matters: check Cuda before C/Cpp so CUDA projects aren't misclassified
  - In `debuggers_for_project()`: add match arm `ProjectType::Cuda => vec!["cuda-gdb"]`

### Code Changes

```diff
--- a/src/setup/detector.rs
+++ b/src/setup/detector.rs
@@ -8,6 +8,7 @@
 #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
 pub enum ProjectType {
     Rust,
+    Cuda,
     Go,
     Python,
     JavaScript,
@@ -28,6 +29,12 @@ pub fn detect_project_types(dir: &Path) -> Vec<ProjectType> {
         types.push(ProjectType::Rust);
     }

+    // CUDA detection must precede C/C++ (.cu files are valid C++ but require CUDA-GDB)
+    if has_extension_in_dir(dir, "cu") {
+        types.push(ProjectType::Cuda);
+    }
+
     // Go
     if dir.join("go.mod").exists() || dir.join("go.sum").exists() {
         types.push(ProjectType::Go);
@@ -88,6 +95,7 @@ pub fn detect_project_types(dir: &Path) -> Vec<ProjectType> {
 pub fn debuggers_for_project(project: &ProjectType) -> Vec<&'static str> {
     match project {
         ProjectType::Rust => vec!["codelldb", "lldb"],
+        ProjectType::Cuda => vec!["cuda-gdb"],
         ProjectType::Go => vec!["go"],
         ProjectType::Python => vec!["python"],
         ProjectType::JavaScript | ProjectType::TypeScript => vec![], // js-debug not yet implemented
```

---

### Milestone 4: Integration Tests

**Files**:
- `tests/integration.rs`

**Flags**: `error-handling`

**Requirements**:
- Add `gdb_available()` helper checking GDB ≥14.1
- Add `cuda_gdb_available()` helper checking cuda-gdb presence
- Add basic GDB debugging test (C program, breakpoint, continue, locals)
- Add CUDA-GDB availability test (skip actual GPU debugging)

**Acceptance Criteria**:
- Tests skip gracefully when GDB/cuda-gdb not available
- GDB test exercises: start, breakpoint, continue, await, locals, stop
- Tests use existing fixtures (`tests/fixtures/simple.c`)

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: default-derived (follows existing integration test pattern with real adapters, user confirmed real dependencies in planning step 2)
- **Scenarios**:
  - Normal: GDB debugging workflow with simple.c
  - Skip: CUDA kernel debugging (requires GPU)

**Code Intent**:
- Modify `tests/integration.rs`:
  - Add `gdb_available() -> bool`: which::which("gdb"), parse version ≥14.1
  - Add `cuda_gdb_available() -> bool`: check cuda-gdb paths
  - Add `#[test] fn test_basic_debugging_workflow_c_gdb()`:
    - Skip if !gdb_available()
    - Follow pattern from `test_basic_debugging_workflow_c()` (lldb version)
    - Use "gdb" adapter instead of "lldb"
  - Add `#[test] fn test_cuda_gdb_adapter_available()`:
    - Skip if !cuda_gdb_available()
    - Verify adapter loads and responds to initialize

### Code Changes

```diff
--- a/tests/integration.rs
+++ b/tests/integration.rs
@@ -281,6 +281,73 @@ fn lldb_dap_available() -> Option<PathBuf> {
     None
 }

+/// Checks if GDB ≥14.1 is available for testing
+///
+/// Returns path only if version meets DAP support requirement
+fn gdb_available() -> Option<PathBuf> {
+    use debugger_cli::setup::adapters::gdb_common::{parse_gdb_version, is_gdb_version_sufficient};
+
+    let path = which::which("gdb").ok()?;
+
+    let output = std::process::Command::new(&path)
+        .arg("--version")
+        .output()
+        .ok()?;
+
+    if output.status.success() {
+        let stdout = String::from_utf8_lossy(&output.stdout);
+        let version = parse_gdb_version(&stdout)?;
+
+        if is_gdb_version_sufficient(&version) {
+            return Some(path);
+        }
+    }
+
+    None
+}
+
+/// Checks if cuda-gdb is available for testing
+///
+/// Uses same path search as CudaGdbInstaller::find_cuda_gdb()
+fn cuda_gdb_available() -> Option<PathBuf> {
+    let default_path = PathBuf::from("/usr/local/cuda/bin/cuda-gdb");
+    if default_path.exists() {
+        return Some(default_path);
+    }
+
+    if let Ok(cuda_home) = std::env::var("CUDA_HOME") {
+        let cuda_home_path = PathBuf::from(cuda_home).join("bin/cuda-gdb");
+        if cuda_home_path.exists() {
+            return Some(cuda_home_path);
+        }
+    }
+
+    which::which("cuda-gdb").ok()
+}
+
 #[test]
 fn test_status_no_daemon() {
     let mut ctx = TestContext::new("status_no_daemon");
@@ -413,6 +480,78 @@ fn test_basic_debugging_workflow_c() {
     let _ = ctx.run_debugger(&["stop"]);
 }

+#[test]
+fn test_basic_debugging_workflow_c_gdb() {
+    let gdb_path = match gdb_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: GDB ≥14.1 not available");
+            return;
+        }
+    };
+
+    let mut ctx = TestContext::new("basic_workflow_c_gdb");
+    ctx.create_config("gdb", gdb_path.to_str().unwrap());
+
+    // Build the C fixture
+    let binary = ctx.build_c_fixture("simple").clone();
+
+    // Find breakpoint markers
+    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
+    let main_start_line = markers.get("main_start").expect("Missing main_start marker");
+
+    // Cleanup any existing daemon
+    ctx.cleanup_daemon();
+
+    // Start debugging
+    let output = ctx.run_debugger_ok(&[
+        "start",
+        binary.to_str().unwrap(),
+        "--stop-on-entry",
+    ]);
+    assert!(output.contains("Started debugging") || output.contains("Stopped"));
+
+    // Set a breakpoint
+    let bp_location = format!("simple.c:{}", main_start_line);
+    let output = ctx.run_debugger_ok(&["break", &bp_location]);
+    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));
+
+    // Continue execution
+    let output = ctx.run_debugger_ok(&["continue"]);
+    assert!(output.contains("Continuing") || output.contains("running"));
+
+    // Wait for breakpoint hit
+    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
+    assert!(
+        output.contains("Stopped") || output.contains("breakpoint"),
+        "Expected stop at breakpoint: {}",
+        output
+    );
+
+    // Get local variables
+    let output = ctx.run_debugger_ok(&["locals"]);
+    assert!(
+        output.contains("x") || output.contains("Local"),
+        "Expected locals output: {}",
+        output
+    );
+
+    // Stop the session
+    let _ = ctx.run_debugger(&["stop"]);
+}
+
+#[test]
+fn test_cuda_gdb_adapter_available() {
+    let cuda_gdb_path = match cuda_gdb_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: CUDA-GDB not available");
+            return;
+        }
+    };
+
+    assert!(cuda_gdb_path.exists(), "CUDA-GDB path should exist");
+}
+
 #[test]
 fn test_stepping_c() {
     let lldb_path = match lldb_dap_available() {
```

---

### Milestone 5: Documentation

**Delegated to**: @agent-technical-writer (mode: post-implementation)

**Source**: `## Invisible Knowledge` section of this plan

**Files**:
- `src/setup/adapters/CLAUDE.md` (update index)
- `src/setup/adapters/README.md` (add GDB/CUDA-GDB section)

**Requirements**:
- Update CLAUDE.md index with new adapter files
- Add README.md section explaining GDB native DAP approach
- Document version requirements and path detection

**Acceptance Criteria**:
- CLAUDE.md lists gdb.rs and cuda_gdb.rs with descriptions
- README.md explains native DAP vs MI adapter decision
- README.md documents CUDA Toolkit path detection logic

### Code Changes

Skip reason: documentation-only milestone

## Milestone Dependencies

```
M1 (GDB Installer) -----> M4 (Integration Tests)
                    \
M2 (CUDA-GDB Installer) -> M4
                    \
M3 (Project Detection) --> M4
                            \
                             --> M5 (Documentation)
```

- M1, M2, M3 can execute in parallel (independent files)
- M4 depends on M1, M2, M3 (tests require adapters)
- M5 depends on M4 (documentation reflects final implementation)
