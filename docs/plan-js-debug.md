# Plan: js-debug Support for JavaScript/TypeScript Debugging

## Overview

This plan implements support for the VS Code JavaScript Debugger (js-debug) in debugger-cli, enabling debugging of JavaScript and TypeScript code. The implementation follows **Approach A (Minimal Change)**: extending the existing `spawn_tcp()` with adapter-aware spawn style detection rather than creating separate spawn methods.

Key decisions:
- **Installation**: npm package (`@vscode/js-debug`) first, GitHub clone fallback
- **Transport**: TCP with port-as-argument pattern (vs Delve's `--listen` flag)
- **Tests**: Real adapter integration tests with pre-compiled TypeScript + sourcemaps

## Planning Context

### Decision Log

| Decision | Reasoning Chain |
|----------|-----------------|
| Extend spawn_tcp vs new method | js-debug is only second TCP adapter -> creating spawn_tcp_port_arg() adds maintenance burden for one adapter -> extending spawn_tcp with spawn_style config minimizes code paths while supporting both patterns |
| npm package first, GitHub fallback | npm package @vscode/js-debug exists and is maintained -> faster install than clone+build -> fallback to GitHub ensures installation works even if npm fails |
| spawn_style in AdapterConfig | Port-argument pattern differs from --listen flag -> need adapter-specific spawn behavior -> config field is cleaner than adapter name detection |
| Add 6 js-debug fields to LaunchArguments | js-debug needs type, sourceMaps, outFiles, runtimeExecutable, runtimeArgs, skipFiles -> existing pattern uses optional fields with skip_serializing_if -> maintains single LaunchArguments struct |
| Pre-compiled TS with sourcemaps for tests | Direct ts-node execution differs from production -> pre-compiled .js with .map files matches real debugging workflow -> tests validate actual sourcemap resolution |
| Real adapter integration tests | Existing tests use real adapters (lldb-dap, GDB) -> mock tests wouldn't catch DAP protocol issues -> integration tests catch real js-debug behavior |
| Port allocation before spawn | js-debug needs port as argument -> must allocate port before spawning -> use TcpListener::bind("127.0.0.1:0") then extract port |
| Node.js runtime only for v1 | js-debug supports multiple runtimes (pwa-node, pwa-chrome, pwa-extensionHost) -> user confirmed Node.js focus for v1 -> set type_attr to "pwa-node" for all JS/TS files -> browser debugging deferred to future release |
| sourceMaps always enabled for .ts | TypeScript requires sourcemaps for line mapping -> user confirmed auto-enable is preferred experience -> most TS projects use sourcemaps -> auto-enable provides best default -> edge cases can disable via future --no-sourcemaps flag |
| debugServerMain.js path discovery | npm installs to node_modules/@vscode/js-debug/ -> internal structure may vary -> hardcode standard path out/src/debugServerMain.js -> fallback: search for debugServerMain.js in package directory -> verified at installation time |
| type_attr serde rename to "type" | DAP requires field named "type" but Rust keyword collision -> use type_attr with #[serde(rename = "type")] -> overrides camelCase rename_all -> ensures correct DAP field name "type" not "typeAttr" -> tested in M1 serialization test |

### Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| Separate spawn_tcp_port_arg() method | Creates two TCP spawn paths to maintain; only one adapter currently needs port-arg pattern |
| Wrapper script normalizing output | Adds Node.js file to maintain; extra process layer complicates debugging; not idiomatic Rust |
| Hardcode js-debug detection in spawn_tcp | Adapter name detection is fragile; config-based spawn_style is explicit and extensible |
| ts-node for TypeScript tests | Different execution model than production; wouldn't test sourcemap resolution which is critical for TS debugging |

### Constraints & Assumptions

- **Node.js required**: js-debug is Node.js-based; installer must verify node/npm availability
- **Existing pattern preservation**: Follow delve.rs/debugpy.rs installer structure exactly
- **TransportMode::Tcp already exists**: No enum changes needed, only spawn behavior differs
- **LaunchArguments extensible**: Existing optional fields pattern with skip_serializing_if

### Known Risks

| Risk | Mitigation | Anchor |
|------|------------|--------|
| js-debug stdout format differs from Delve | Parse multiple patterns: "Listening on port X" or silent startup with timeout-based detection | N/A - runtime behavior |
| npm package may not exist or be outdated | GitHub clone fallback with npm install && npm run build | N/A - external dependency |
| Port allocation race condition | Allocate port, immediately spawn adapter, connect with retry loop | src/dap/client.rs:156-264 has similar pattern |
| TypeScript compilation in tests | Include pre-compiled fixtures with .map files; regenerate in test setup if missing | N/A - test infrastructure |

## Invisible Knowledge

### Architecture

```
User Command: debugger start app.js
    |
    v
Session::launch() [session.rs]
    |
    +-- Check adapter_config.transport == Tcp
    |
    +-- Check adapter_config.spawn_style
    |       |
    |       +-- TcpListen -> spawn_tcp() with --listen flag (Delve)
    |       |
    |       +-- TcpPortArg -> spawn_tcp() with port argument (js-debug)
    |               |
    |               +-- Allocate random port via TcpListener
    |               +-- Spawn: node debugServerMain.js <port>
    |               +-- Connect via TcpStream
    |
    v
DapClient::initialize()
    |
    +-- LaunchArguments with js-debug fields:
    |     type: "pwa-node"
    |     sourceMaps: true
    |     outFiles: ["/dist/**/*.js"]
    |
    v
DAP Protocol over TCP
```

### Data Flow

```
Installation:
  npm install @vscode/js-debug -> ~/.local/share/debugger-cli/adapters/js-debug/
      |
      +-- Fallback: git clone + npm install + npm run build
      |
      v
  Result: /path/to/out/src/debugServerMain.js

Launch:
  AdapterConfig {
    path: "node",
    args: ["/path/to/debugServerMain.js"],
    transport: Tcp,
    spawn_style: TcpPortArg
  }
      |
      v
  spawn_tcp() allocates port -> spawns -> connects
```

### Why This Structure

- **spawn_style in config vs adapter detection**: Config is explicit; adapter name matching is fragile when aliases exist
- **Single spawn_tcp with branching**: Avoids code duplication while handling both patterns
- **LaunchArguments union type**: DAP spec allows adapter-specific fields; skip_serializing_if keeps JSON clean

### Invariants

1. Port must be allocated BEFORE spawning adapter (js-debug needs it as argument)
2. spawn_style defaults to TcpListen for backward compatibility with Delve. **New TCP adapters MUST specify spawn_style explicitly in config.** Default is NOT safe for all TCP adapters.
3. js-debug tests must use real adapter (mocks wouldn't catch protocol issues)
4. TypeScript test fixtures must have accompanying .map files
5. type_attr field MUST use `#[serde(rename = "type")]` to produce correct DAP field name

### Tradeoffs

- **Config complexity vs code simplicity**: spawn_style field adds config option but simplifies spawn_tcp logic
- **npm dependency**: Requires Node.js ecosystem but avoids reimplementing js-debug
- **Integration test speed**: Real adapter tests are slower but catch real issues

## Milestones

### Milestone 1: Core Infrastructure (spawn_style + LaunchArguments)

**Files**:
- `src/common/config.rs`
- `src/dap/types.rs`
- `src/dap/client.rs`

**Flags**:
- `conformance`: Must match existing adapter patterns

**Requirements**:
- Add `TcpSpawnStyle` enum with `TcpListen` (default) and `TcpPortArg` variants
- Add `spawn_style` field to `AdapterConfig` with default `TcpListen`
- Add js-debug fields to `LaunchArguments`: `type_attr`, `source_maps`, `out_files`, `runtime_executable`, `runtime_args`, `skip_files`
- Modify `spawn_tcp()` to handle `TcpPortArg` pattern: allocate port first, pass as argument

**Acceptance Criteria**:
- `TcpSpawnStyle::TcpListen` produces `--listen=127.0.0.1:PORT` behavior (Delve unchanged)
- `TcpSpawnStyle::TcpPortArg` allocates port, appends to args, spawns, connects
- LaunchArguments serializes js-debug fields only when set
- LaunchArguments with type_attr="pwa-node" serializes to `{"type": "pwa-node"}` (not `{"typeAttr": "pwa-node"}`)

**Tests**:
- **Test files**: `tests/integration.rs` (unit test section)
- **Test type**: unit
- **Backing**: default-derived
- **Scenarios**:
  - Normal: TcpPortArg allocates port and includes in spawn args
  - Normal: TcpListen includes --listen flag (regression)
  - Edge: LaunchArguments with js-debug fields serializes correctly

**Code Intent**:
- `src/common/config.rs`: Add `TcpSpawnStyle` enum after `TransportMode`. Add `spawn_style: TcpSpawnStyle` field to `AdapterConfig` with `#[serde(default)]`
- `src/dap/types.rs`: Add 6 optional fields to `LaunchArguments` after Delve fields: `type_attr` (with `#[serde(rename = "type")]` to produce DAP "type" field), `source_maps`, `out_files`, `runtime_executable`, `runtime_args`, `skip_files`. Use `#[serde(skip_serializing_if = "Option::is_none")]`. Decision: "type_attr serde rename to type"
- `src/dap/client.rs`: In `spawn_tcp()`, add branch at line ~160 to check spawn_style. For TcpPortArg: allocate port via `TcpListener::bind`, extract port, append to args, spawn without --listen flag. Decision: "Port allocation before spawn"

**Code Changes**:

```diff
--- a/src/common/config.rs
+++ b/src/common/config.rs
@@ -43,6 +43,19 @@ pub enum TransportMode {
     Tcp,
 }

+/// TCP adapter spawn style
+#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
+#[serde(rename_all = "lowercase")]
+pub enum TcpSpawnStyle {
+    /// Adapter accepts --listen flag and waits for connection (Delve)
+    #[default]
+    TcpListen,
+    /// Adapter receives port as positional argument (js-debug)
+    TcpPortArg,
+}
+
 /// Configuration for a debug adapter
 #[derive(Debug, Deserialize, Clone)]
 pub struct AdapterConfig {
@@ -55,6 +68,10 @@ pub struct AdapterConfig {
     /// Transport mode for DAP communication
     #[serde(default)]
     pub transport: TransportMode,
+
+    /// TCP spawn style (only used when transport is Tcp)
+    #[serde(default)]
+    pub spawn_style: TcpSpawnStyle,
 }

 /// Default settings
```

```diff
--- a/src/dap/types.rs
+++ b/src/dap/types.rs
@@ -159,6 +159,30 @@ pub struct LaunchArguments {
     /// Stop at beginning of main (GDB uses stopAtBeginningOfMainSubprogram instead of stopOnEntry)
     #[serde(skip_serializing_if = "Option::is_none")]
     pub stop_at_beginning_of_main_subprogram: Option<bool>,
+
+    // === js-debug (JavaScript/TypeScript) specific ===
+    /// Runtime type: "pwa-node", "pwa-chrome", "pwa-extensionHost"
+    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
+    pub type_attr: Option<String>,
+    /// Enable source maps for TypeScript debugging
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub source_maps: Option<bool>,
+    /// Glob patterns for compiled output files
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub out_files: Option<Vec<String>>,
+    /// Path to runtime executable (node)
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub runtime_executable: Option<String>,
+    /// Arguments passed to runtime executable
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub runtime_args: Option<Vec<String>>,
+    /// Glob patterns for files to skip when debugging
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub skip_files: Option<Vec<String>>,
 }

 /// Attach request arguments
```

```diff
--- a/src/dap/client.rs
+++ b/src/dap/client.rs
@@ -150,16 +150,44 @@ impl DapClient {
     }

     /// Spawn a new DAP adapter that uses TCP for communication (e.g., Delve)
-    ///
-    /// This spawns the adapter with a --listen flag, waits for it to output
-    /// the port it's listening on, then connects via TCP.
-    pub async fn spawn_tcp(adapter_path: &Path, args: &[String]) -> Result<Self> {
+    pub async fn spawn_tcp(
+        adapter_path: &Path,
+        args: &[String],
+        spawn_style: &crate::common::config::TcpSpawnStyle,
+    ) -> Result<Self> {
         use crate::common::parse_listen_address;
         use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};

-        // Build command with --listen=127.0.0.1:0 to get a random available port
+        let (mut adapter, addr) = match spawn_style {
+            crate::common::config::TcpSpawnStyle::TcpListen => {
+                let mut cmd = Command::new(adapter_path);
+                cmd.args(args)
+                    .arg("--listen=127.0.0.1:0")
+                    .stdin(Stdio::null())
+                    .stdout(Stdio::piped())
+                    .stderr(Stdio::piped());
+
+                let mut adapter = cmd.spawn().map_err(|e| {
+                    Error::AdapterStartFailed(format!(
+                        "Failed to start {}: {}",
+                        adapter_path.display(),
+                        e
+                    ))
+                })?;
+
+                let stdout = adapter.stdout.take().ok_or_else(|| {
+                    let _ = adapter.start_kill();
+                    Error::AdapterStartFailed("Failed to get adapter stdout".to_string())
+                })?;
+
+                let mut stdout_reader = TokioBufReader::new(stdout);
+                let mut line = String::new();
+
+                let addr_result = tokio::time::timeout(Duration::from_secs(10), async {
+                    loop {
+                        line.clear();
+                        let bytes_read = stdout_reader.read_line(&mut line).await.map_err(|e| {
+                            Error::AdapterStartFailed(format!("Failed to read adapter output: {}", e))
+                        })?;
+
+                        if bytes_read == 0 {
+                            return Err(Error::AdapterStartFailed(
+                                "Adapter exited before outputting listen address".to_string(),
+                            ));
+                        }
+
+                        tracing::debug!("Adapter output: {}", line.trim());
+
+                        if let Some(addr) = parse_listen_address(&line) {
+                            return Ok(addr);
+                        }
+                    }
+                })
+                .await;
+
+                let addr = match addr_result {
+                    Ok(Ok(addr)) => addr,
+                    Ok(Err(e)) => {
+                        let _ = adapter.start_kill();
+                        return Err(e);
+                    }
+                    Err(_) => {
+                        let _ = adapter.start_kill();
+                        return Err(Error::AdapterStartFailed(
+                            "Timeout waiting for adapter to start listening".to_string(),
+                        ));
+                    }
+                };
+
+                (adapter, addr)
+            }
+            crate::common::config::TcpSpawnStyle::TcpPortArg => {
+                use std::net::TcpListener as StdTcpListener;
+
+                let listener = StdTcpListener::bind("127.0.0.1:0").map_err(|e| {
+                    Error::AdapterStartFailed(format!("Failed to allocate port: {}", e))
+                })?;
+                let port = listener.local_addr().map_err(|e| {
+                    Error::AdapterStartFailed(format!("Failed to get port: {}", e))
+                })?.port();
+                drop(listener);
+
+                let addr = format!("127.0.0.1:{}", port);
+
+                let mut cmd = Command::new(adapter_path);
+                let mut full_args = args.to_vec();
+                full_args.push(port.to_string());
+
+                cmd.args(&full_args)
+                    .stdin(Stdio::null())
+                    .stdout(Stdio::piped())
+                    .stderr(Stdio::piped());
+
+                let adapter = cmd.spawn().map_err(|e| {
+                    Error::AdapterStartFailed(format!(
+                        "Failed to start {}: {}",
+                        adapter_path.display(),
+                        e
+                    ))
+                })?;
+
+                tokio::time::sleep(Duration::from_millis(500)).await;
+
+                (adapter, addr)
+            }
+        };
+
+        tracing::info!("Connecting to DAP adapter at {}", addr);
+
         let mut cmd = Command::new(adapter_path);
         cmd.args(args)
-            .arg("--listen=127.0.0.1:0")
-            .stdin(Stdio::null())
-            .stdout(Stdio::piped())
-            .stderr(Stdio::piped());
-
-        let mut adapter = cmd.spawn().map_err(|e| {
-            Error::AdapterStartFailed(format!(
-                "Failed to start {}: {}",
-                adapter_path.display(),
-                e
-            ))
-        })?;
-
-        // Read stdout to find the listening address
-        // Delve outputs: "DAP server listening at: 127.0.0.1:PORT"
-        let stdout = adapter.stdout.take().ok_or_else(|| {
-            let _ = adapter.start_kill();
-            Error::AdapterStartFailed("Failed to get adapter stdout".to_string())
-        })?;
-
-        let mut stdout_reader = TokioBufReader::new(stdout);
-        let mut line = String::new();
-
-        // Wait for the "listening at" message with timeout
-        let addr_result = tokio::time::timeout(Duration::from_secs(10), async {
-            loop {
-                line.clear();
-                let bytes_read = stdout_reader.read_line(&mut line).await.map_err(|e| {
-                    Error::AdapterStartFailed(format!("Failed to read adapter output: {}", e))
-                })?;
-
-                if bytes_read == 0 {
-                    return Err(Error::AdapterStartFailed(
-                        "Adapter exited before outputting listen address".to_string(),
-                    ));
-                }
-
-                tracing::debug!("Delve output: {}", line.trim());
-
-                // Look for the listening address in the output
-                if let Some(addr) = parse_listen_address(&line) {
-                    return Ok(addr);
-                }
-            }
-        })
-        .await;
-
-        // Handle timeout or error - cleanup adapter before returning
-        let addr = match addr_result {
-            Ok(Ok(addr)) => addr,
-            Ok(Err(e)) => {
-                let _ = adapter.start_kill();
-                return Err(e);
-            }
-            Err(_) => {
-                let _ = adapter.start_kill();
-                return Err(Error::AdapterStartFailed(
-                    "Timeout waiting for Delve to start listening".to_string(),
-                ));
-            }
-        };
-
-        tracing::info!("Connecting to Delve DAP server at {}", addr);
-
-        // Connect to the TCP port - cleanup adapter on failure
         let stream = match TcpStream::connect(&addr).await {
             Ok(s) => s,
             Err(e) => {
                 let _ = adapter.start_kill();
                 return Err(Error::AdapterStartFailed(format!(
-                    "Failed to connect to Delve at {}: {}",
+                    "Failed to connect to adapter at {}: {}",
                     addr, e
                 )));
             }
         };

         let (read_half, write_half) = tokio::io::split(stream);
```


---

### Milestone 2: js-debug Installer

**Files**:
- `src/setup/adapters/js_debug.rs` (new)
- `src/setup/adapters/mod.rs`
- `src/setup/registry.rs`
- `src/setup/detector.rs`

**Flags**:
- `conformance`: Must match delve.rs/debugpy.rs patterns exactly

**Requirements**:
- Create `JsDebugInstaller` implementing `Installer` trait
- Support npm package installation with GitHub clone fallback
- Verify Node.js availability before installation
- Register in adapter registry with languages: `["javascript", "typescript"]`
- Update project detector to return `"js-debug"` for JS/TS projects

**Acceptance Criteria**:
- `debugger setup list` shows js-debug adapter
- `debugger setup install js-debug` installs via npm or GitHub
- `debugger setup verify js-debug` confirms DAP communication works
- Project detection in JS/TS directories suggests js-debug

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: user-specified (real adapter)
- **Scenarios**:
  - Normal: Fresh install via npm succeeds
  - Normal: Verify confirms adapter responds to DAP initialize
  - Edge: npm failure triggers GitHub fallback
  - Error: Missing Node.js returns clear error

**Code Intent**:
- New `src/setup/adapters/js_debug.rs`: Create `JsDebugInstaller` struct. Implement `Installer` trait with: `info()` returning DebuggerInfo for js-debug, `status()` checking adapters_dir/js-debug for out/src/debugServerMain.js (hardcoded path, fallback search for debugServerMain.js), `best_method()` trying npm then GitHub, `install()` running npm install or git clone, `verify()` using `verify_dap_adapter_tcp` variant. Decision: "debugServerMain.js path discovery"
- `src/setup/adapters/mod.rs`: Add `pub mod js_debug;`
- `src/setup/registry.rs`: Add DebuggerInfo entry in DEBUGGERS array. Add match arm in `get_installer()` for "js-debug"
- `src/setup/detector.rs`: Change line ~55 from `vec![]` to `vec!["js-debug"]` for JS/TS project types

**Code Changes**:

New file `src/setup/adapters/js_debug.rs`:

```rust
//! js-debug installer
//!
//! Installs the VS Code JavaScript Debugger (js-debug) with DAP support.

use crate::common::config::TcpSpawnStyle;
use crate::common::{Error, Result};
use crate::setup::installer::{
    adapters_dir, download_file, ensure_adapters_dir, extract_tar_gz, get_github_release,
    run_command_args, write_version_file, InstallMethod, InstallOptions, InstallResult,
    InstallStatus, Installer, PackageManager,
};
use crate::setup::registry::{DebuggerInfo, Platform};
use crate::setup::verifier::{verify_dap_adapter_tcp, VerifyResult};
use async_trait::async_trait;
use std::path::PathBuf;

static INFO: DebuggerInfo = DebuggerInfo {
    id: "js-debug",
    name: "js-debug",
    languages: &["javascript", "typescript"],
    platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
    description: "VS Code JavaScript Debugger with DAP support",
    primary: true,
};

const GITHUB_REPO: &str = "microsoft/vscode-js-debug";
const NPM_PACKAGE: &str = "@vscode/js-debug";

pub struct JsDebugInstaller;

#[async_trait]
impl Installer for JsDebugInstaller {
    fn info(&self) -> &DebuggerInfo {
        &INFO
    }

    async fn status(&self) -> Result<InstallStatus> {
        let adapter_dir = adapters_dir().join("js-debug");

        let main_path = adapter_dir.join("out").join("src").join("debugServerMain.js");
        if main_path.exists() {
            let version = read_version_file(&adapter_dir);
            return Ok(InstallStatus::Installed {
                path: main_path,
                version,
            });
        }

        if adapter_dir.exists() {
            if let Some(path) = find_debug_server_main(&adapter_dir) {
                let version = read_version_file(&adapter_dir);
                return Ok(InstallStatus::Installed {
                    path,
                    version,
                });
            }
        }

        Ok(InstallStatus::NotInstalled)
    }

    async fn best_method(&self) -> Result<InstallMethod> {
        let managers = PackageManager::detect();

        if managers.contains(&PackageManager::Npm) {
            return Ok(InstallMethod::LanguagePackage {
                tool: "npm".to_string(),
                package: NPM_PACKAGE.to_string(),
            });
        }

        Ok(InstallMethod::GitHubRelease {
            repo: GITHUB_REPO.to_string(),
            asset_pattern: String::new(),
        })
    }

    async fn install(&self, opts: InstallOptions) -> Result<InstallResult> {
        let method = self.best_method().await?;

        match method {
            InstallMethod::LanguagePackage { tool, package } => {
                install_via_npm(&tool, &package, &opts).await
            }
            InstallMethod::GitHubRelease { .. } => install_from_github(&opts).await,
            _ => Err(Error::Internal("Unexpected installation method".to_string())),
        }
    }

    async fn verify(&self) -> Result<VerifyResult> {
        let status = self.status().await?;

        let path = match status {
            InstallStatus::Installed { path, .. } => path,
            InstallStatus::NotInstalled => {
                return Ok(VerifyResult::NotInstalled);
            }
        };

        let node_path = which::which("node").map_err(|_| {
            Error::VerificationFailed("Node.js not found in PATH".to_string())
        })?;

        verify_dap_adapter_tcp(&node_path, &[path.to_string_lossy().to_string()], TcpSpawnStyle::TcpPortArg).await
    }
}

async fn install_via_npm(
    _tool: &str,
    package: &str,
    opts: &InstallOptions,
) -> Result<InstallResult> {
    ensure_adapters_dir()?;
    let adapter_dir = adapters_dir().join("js-debug");
    std::fs::create_dir_all(&adapter_dir).map_err(|e| {
        Error::Internal(format!("Failed to create adapter directory: {}", e))
    })?;

    if !opts.quiet {
        println!("Installing {} via npm...", package);
    }

    run_command_args(
        "npm",
        &["install", "--prefix", adapter_dir.to_str().unwrap(), package],
        opts,
    )
    .await?;

    let main_path = adapter_dir
        .join("node_modules")
        .join(package)
        .join("out")
        .join("src")
        .join("debugServerMain.js");

    if !main_path.exists() {
        if let Some(fallback_path) = find_debug_server_main(&adapter_dir) {
            let version = Some("npm".to_string());
            write_version_file(&adapter_dir, &version);
            return Ok(InstallResult {
                path: fallback_path,
                version,
                args: vec![],
            });
        }
        return Err(Error::Internal(
            "debugServerMain.js not found after npm install".to_string(),
        ));
    }

    let version = Some("npm".to_string());
    write_version_file(&adapter_dir, &version);

    Ok(InstallResult {
        path: main_path,
        version,
        args: vec![],
    })
}

async fn install_from_github(opts: &InstallOptions) -> Result<InstallResult> {
    ensure_adapters_dir()?;
    let adapter_dir = adapters_dir().join("js-debug");

    if !opts.quiet {
        println!("Installing js-debug from GitHub...");
    }

    run_command_args(
        "git",
        &[
            "clone",
            "--depth",
            "1",
            "https://github.com/microsoft/vscode-js-debug.git",
            adapter_dir.to_str().unwrap(),
        ],
        opts,
    )
    .await?;

    if !opts.quiet {
        println!("Installing dependencies...");
    }

    run_command_args("npm", &["install", "--prefix", adapter_dir.to_str().unwrap()], opts).await?;

    if !opts.quiet {
        println!("Building js-debug...");
    }

    run_command_args(
        "npm",
        &["run", "build", "--prefix", adapter_dir.to_str().unwrap()],
        opts,
    )
    .await?;

    let main_path = adapter_dir.join("out").join("src").join("debugServerMain.js");

    if !main_path.exists() {
        if let Some(fallback_path) = find_debug_server_main(&adapter_dir) {
            let version = Some("git".to_string());
            write_version_file(&adapter_dir, &version);
            return Ok(InstallResult {
                path: fallback_path,
                version,
                args: vec![],
            });
        }
        return Err(Error::Internal(
            "debugServerMain.js not found after build".to_string(),
        ));
    }

    let version = Some("git".to_string());
    write_version_file(&adapter_dir, &version);

    Ok(InstallResult {
        path: main_path,
        version,
        args: vec![],
    })
}

fn find_debug_server_main(base_dir: &std::path::Path) -> Option<PathBuf> {
    let patterns = [
        "out/src/debugServerMain.js",
        "dist/src/debugServerMain.js",
        "debugServerMain.js",
    ];

    for pattern in &patterns {
        let path = base_dir.join(pattern);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn read_version_file(adapter_dir: &std::path::Path) -> Option<String> {
    let version_file = adapter_dir.join(".version");
    std::fs::read_to_string(version_file).ok()
}
```

```diff
--- a/src/setup/adapters/mod.rs
+++ b/src/setup/adapters/mod.rs
@@ -9,3 +9,4 @@ pub mod debugpy;
 pub mod delve;
 pub mod gdb_common;
 pub mod gdb;
 pub mod lldb;
+pub mod js_debug;
```

```diff
--- a/src/setup/registry.rs
+++ b/src/setup/registry.rs
@@ -109,6 +109,13 @@ static DEBUGGERS: &[DebuggerInfo] = &[
         description: "Go debugger with DAP support",
         primary: true,
     },
+    DebuggerInfo {
+        id: "js-debug",
+        name: "js-debug",
+        languages: &["javascript", "typescript"],
+        platforms: &[Platform::Linux, Platform::MacOS, Platform::Windows],
+        description: "VS Code JavaScript Debugger with DAP support",
+        primary: true,
+    },
 ];

 /// Get all registered debuggers
@@ -147,6 +154,7 @@ pub fn get_installer(id: &str) -> Option<Arc<dyn Installer>> {
         "codelldb" => Some(Arc::new(adapters::codelldb::CodeLldbInstaller)),
         "python" => Some(Arc::new(adapters::debugpy::DebugpyInstaller)),
         "go" => Some(Arc::new(adapters::delve::DelveInstaller)),
+        "js-debug" => Some(Arc::new(adapters::js_debug::JsDebugInstaller)),
         _ => None,
     }
 }
```

```diff
--- a/src/setup/detector.rs
+++ b/src/setup/detector.rs
@@ -97,7 +97,7 @@ pub fn debuggers_for_project(project: &ProjectType) -> Vec<&'static str> {
         ProjectType::Cuda => vec!["cuda-gdb"],
         ProjectType::Go => vec!["go"],
         ProjectType::Python => vec!["python"],
-        ProjectType::JavaScript | ProjectType::TypeScript => vec![],
+        ProjectType::JavaScript | ProjectType::TypeScript => vec!["js-debug"],
         ProjectType::C | ProjectType::Cpp => vec!["lldb", "codelldb"],
         ProjectType::CSharp => vec![],
         ProjectType::Java => vec![],
```


---

### Milestone 3: Session Launch Integration

**Files**:
- `src/daemon/session.rs`

**Flags**:
- `needs-rationale`: js-debug field population logic needs WHY comments

**Requirements**:
- Detect js-debug adapter and populate js-debug-specific LaunchArguments
- Set `type_attr: "pwa-node"` for Node.js debugging
- Enable `source_maps: true` by default for .ts files
- Pass spawn_style to DapClient spawn

**Acceptance Criteria**:
- `debugger start app.js --adapter js-debug` launches with correct LaunchArguments
- TypeScript files get `source_maps: true` automatically
- spawn_tcp receives correct spawn_style from config

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: user-specified
- **Scenarios**:
  - Normal: JavaScript file launches with type: pwa-node
  - Normal: TypeScript file gets sourceMaps: true
  - Edge: Custom outFiles passed through

**Code Intent**:
- `src/daemon/session.rs`: Near line 178, add detection block for js-debug (similar to is_python, is_go). When adapter is js-debug or file extension is .js/.ts: set `launch_args.type_attr = Some("pwa-node".to_string())` (Decision: "Node.js runtime only for v1"). For .ts files: set `launch_args.source_maps = Some(true)` (Decision: "sourceMaps always enabled for .ts"). Near line 154, pass `adapter_config.spawn_style` to spawn decision.

**Code Changes**:

```diff
--- a/src/daemon/session.rs
+++ b/src/daemon/session.rs
@@ -155,7 +155,7 @@ impl Session {
             TransportMode::Stdio => {
                 DapClient::spawn(&adapter_config.path, &adapter_config.args).await?
             }
             TransportMode::Tcp => {
-                DapClient::spawn_tcp(&adapter_config.path, &adapter_config.args).await?
+                DapClient::spawn_tcp(&adapter_config.path, &adapter_config.args, &adapter_config.spawn_style).await?
             }
         };

@@ -182,6 +182,11 @@ impl Session {
         let is_go = adapter_name == "go"
             || adapter_name == "delve"
             || adapter_name == "dlv";
+        let is_javascript = adapter_name == "js-debug"
+            || program.extension().map(|e| e == "js").unwrap_or(false);
+        let is_typescript = adapter_name == "js-debug"
+            || program.extension().map(|e| e == "ts").unwrap_or(false);

         let launch_args = LaunchArguments {
             program: program.to_string_lossy().into_owned(),
@@ -203,6 +208,11 @@ impl Session {
             stop_at_entry: if is_go && stop_on_entry { Some(true) } else { None },
             // GDB-based adapters (gdb, cuda-gdb) use stopAtBeginningOfMainSubprogram
             stop_at_beginning_of_main_subprogram: if (adapter_name == "gdb" || adapter_name == "cuda-gdb") && stop_on_entry { Some(true) } else { None },
+            // js-debug specific
+            type_attr: if is_javascript || is_typescript { Some("pwa-node".to_string()) } else { None },
+            source_maps: if is_typescript { Some(true) } else { None },
+            out_files: None,
+            runtime_executable: None,
+            runtime_args: None,
+            skip_files: None,
         };

         tracing::debug!(
```


---

### Milestone 4: Test Fixtures

**Files**:
- `tests/fixtures/simple.js` (new)
- `tests/fixtures/simple.ts` (new)
- `tests/fixtures/tsconfig.json` (new)
- `tests/fixtures/dist/simple.js` (new, compiled)
- `tests/fixtures/dist/simple.js.map` (new, sourcemap)

**Requirements**:
- JavaScript fixture with BREAKPOINT_MARKER comments matching C/Python fixtures
- TypeScript fixture with same structure, compiled to dist/
- Sourcemap enabling TypeScript line mapping

**Acceptance Criteria**:
- `simple.js` has markers: main_start, before_add, before_factorial, add_body
- `simple.ts` has same markers, compiles to `dist/simple.js` with `.map`
- Sourcemap correctly maps dist/simple.js lines to simple.ts lines

**Tests**:
- Skip: Fixtures are test infrastructure, validated by integration tests

**Code Intent**:
- New `tests/fixtures/simple.js`: JavaScript file matching structure of simple.c/simple.py with add() and factorial() functions, BREAKPOINT_MARKER comments
- New `tests/fixtures/simple.ts`: TypeScript version with type annotations
- New `tests/fixtures/tsconfig.json`: Compiler config targeting dist/, generating sourcemaps
- Pre-compiled `tests/fixtures/dist/simple.js` and `.map`: Committed to repo so tests don't require tsc

**Code Changes**:

New file `tests/fixtures/simple.js`:

```javascript
#!/usr/bin/env node
// Simple test program for debugger integration tests

function add(a, b) {
    // BREAKPOINT_MARKER: add_body
    const result = a + b;
    return result;
}

function factorial(n) {
    // BREAKPOINT_MARKER: factorial_body
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

function main() {
    // BREAKPOINT_MARKER: main_start
    const x = 10;
    const y = 20;

    // BREAKPOINT_MARKER: before_add
    const sumResult = add(x, y);
    console.log(`Sum: ${sumResult}`);

    // BREAKPOINT_MARKER: before_factorial
    const fact = factorial(5);
    console.log(`Factorial: ${fact}`);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}

// BREAKPOINT_MARKER: entry_point
process.exit(main());
```

New file `tests/fixtures/simple.ts`:

```typescript
#!/usr/bin/env node
// Simple test program for debugger integration tests

function add(a: number, b: number): number {
    // BREAKPOINT_MARKER: add_body
    const result: number = a + b;
    return result;
}

function factorial(n: number): number {
    // BREAKPOINT_MARKER: factorial_body
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

function main(): number {
    // BREAKPOINT_MARKER: main_start
    const x: number = 10;
    const y: number = 20;

    // BREAKPOINT_MARKER: before_add
    const sumResult: number = add(x, y);
    console.log(`Sum: ${sumResult}`);

    // BREAKPOINT_MARKER: before_factorial
    const fact: number = factorial(5);
    console.log(`Factorial: ${fact}`);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}

// BREAKPOINT_MARKER: entry_point
process.exit(main());
```

New file `tests/fixtures/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "outDir": "./dist",
    "rootDir": "./",
    "sourceMap": true,
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["simple.ts"],
  "exclude": ["node_modules", "dist"]
}
```

New file `tests/fixtures/dist/simple.js` (pre-compiled):

```javascript
#!/usr/bin/env node
"use strict";
function add(a, b) {
    const result = a + b;
    return result;
}
function factorial(n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}
function main() {
    const x = 10;
    const y = 20;
    const sumResult = add(x, y);
    console.log(`Sum: ${sumResult}`);
    const fact = factorial(5);
    console.log(`Factorial: ${fact}`);
    return 0;
}
process.exit(main());
//# sourceMappingURL=simple.js.map
```

New file `tests/fixtures/dist/simple.js.map`:

```json
{"version":3,"file":"simple.js","sourceRoot":"","sources":["../simple.ts"],"names":[],"mappings":";;AAEA,SAAS,GAAG,CAAC,CAAS,EAAE,CAAS;IAE7B,MAAM,MAAM,GAAW,CAAC,GAAG,CAAC,CAAC;IAC7B,OAAO,MAAM,CAAC;AAClB,CAAC;AAED,SAAS,SAAS,CAAC,CAAS;IAExB,IAAI,CAAC,IAAI,CAAC,EAAE,CAAC;QACT,OAAO,CAAC,CAAC;IACb,CAAC;IACD,OAAO,CAAC,GAAG,SAAS,CAAC,CAAC,GAAG,CAAC,CAAC,CAAC;AAChC,CAAC;AAED,SAAS,IAAI;IAET,MAAM,CAAC,GAAW,EAAE,CAAC;IACrB,MAAM,CAAC,GAAW,EAAE,CAAC;IAGrB,MAAM,SAAS,GAAW,GAAG,CAAC,CAAC,EAAE,CAAC,CAAC,CAAC;IACpC,OAAO,CAAC,GAAG,CAAC,QAAQ,SAAS,EAAE,CAAC,CAAC;IAGjC,MAAM,IAAI,GAAW,SAAS,CAAC,CAAC,CAAC,CAAC;IAClC,OAAO,CAAC,GAAG,CAAC,cAAc,IAAI,EAAE,CAAC,CAAC;IAGlC,OAAO,CAAC,CAAC;AACb,CAAC;AAGD,OAAO,CAAC,IAAI,CAAC,IAAI,EAAE,CAAC,CAAC"}
```


---

### Milestone 5: Integration Tests

**Files**:
- `tests/integration.rs`

**Flags**:
- `conformance`: Must match existing test patterns (lldb-dap tests)

**Requirements**:
- `js_debug_available()` helper function checking for js-debug installation
- `test_basic_debugging_workflow_js()` - start, breakpoint, continue, stop
- `test_basic_debugging_workflow_ts()` - TypeScript with sourcemap verification
- `test_stepping_js()` - step in, step out, step over
- `test_expression_evaluation_js()` - evaluate expressions at breakpoint
- `test_sourcemap_resolution_ts()` - verify breakpoint hits in .ts not .js

**Acceptance Criteria**:
- All js-debug tests pass when adapter is installed
- Tests skip gracefully when js-debug not available
- TypeScript test verifies sourcemap: breakpoint in .ts, stops at correct line

**Tests**:
- **Test files**: `tests/integration.rs`
- **Test type**: integration
- **Backing**: user-specified (real adapter, pre-compiled TS)
- **Scenarios**:
  - Normal: JavaScript breakpoint hit and variable inspection
  - Normal: TypeScript breakpoint with sourcemap resolution
  - Normal: Step into function, step out
  - Edge: Async function debugging (if supported)
  - Skip: Tests auto-skip if js-debug not installed

**Code Intent**:
- `tests/integration.rs`: Add `js_debug_available()` helper similar to `lldb_dap_available()`. Add 5 test functions with `#[ignore = "requires js-debug"]` attribute. Use `TestContext::create_config_with_args()` for js-debug config with transport=tcp. Follow existing test patterns: cleanup_daemon, start, break, continue, await, assertions, stop

**Code Changes**:

```diff
--- a/tests/integration.rs
+++ b/tests/integration.rs
@@ -340,6 +340,26 @@ fn lldb_dap_available() -> Option<PathBuf> {
     }
 }

+/// Check if js-debug is available
+fn js_debug_available() -> Option<PathBuf> {
+    if let Ok(node_path) = which::which("node") {
+        let adapters_dir = dirs::data_local_dir()?.join("debugger-cli").join("adapters");
+        let js_debug_dir = adapters_dir.join("js-debug");
+        let main_path = js_debug_dir.join("out").join("src").join("debugServerMain.js");
+
+        if main_path.exists() {
+            return Some(main_path);
+        }
+
+        let npm_path = js_debug_dir.join("node_modules").join("@vscode").join("js-debug").join("out").join("src").join("debugServerMain.js");
+        if npm_path.exists() {
+            return Some(npm_path);
+        }
+    }
+
+    None
+}
+
 /// Check if GDB with DAP support is available (requires GDB ≥14.1)
 fn gdb_available() -> Option<PathBuf> {
     if let Ok(gdb_path) = which::which("gdb") {
@@ -1192,3 +1212,233 @@ fn test_cuda_gdb_kernel_debugging() {
     let _ = ctx.run_debugger(&["stop"]);
 }
+
+#[test]
+#[ignore = "requires js-debug"]
+fn test_basic_debugging_workflow_js() {
+    let js_debug_path = match js_debug_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: js-debug not available");
+            return;
+        }
+    };
+
+    let node_path = match which::which("node") {
+        Ok(path) => path,
+        Err(_) => {
+            eprintln!("Skipping test: Node.js not available");
+            return;
+        }
+    };
+
+    let mut ctx = TestContext::new("basic_workflow_js");
+    ctx.create_config_with_tcp(
+        "js-debug",
+        node_path.to_str().unwrap(),
+        &[js_debug_path.to_str().unwrap()],
+        "tcpportarg",
+    );
+
+    let js_file = ctx.fixtures_dir.join("simple.js");
+    let markers = ctx.find_breakpoint_markers(&js_file);
+    let main_start_line = markers.get("main_start").expect("Missing main_start marker");
+
+    ctx.cleanup_daemon();
+
+    let output = ctx.run_debugger_ok(&[
+        "start",
+        js_file.to_str().unwrap(),
+    ]);
+    assert!(output.contains("Started debugging") || output.contains("session"));
+
+    let bp_location = format!("simple.js:{}", main_start_line);
+    let output = ctx.run_debugger_ok(&["break", &bp_location]);
+    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));
+
+    let output = ctx.run_debugger_ok(&["continue"]);
+    assert!(output.contains("Continuing") || output.contains("running"));
+
+    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
+    assert!(
+        output.contains("Stopped") || output.contains("breakpoint"),
+        "Expected stop at breakpoint: {}",
+        output
+    );
+
+    let output = ctx.run_debugger_ok(&["locals"]);
+    assert!(
+        output.contains("x") || output.contains("Local"),
+        "Expected locals output: {}",
+        output
+    );
+
+    let _ = ctx.run_debugger(&["continue"]);
+    let _ = ctx.run_debugger(&["await", "--timeout", "10"]);
+    let _ = ctx.run_debugger(&["stop"]);
+}
+
+#[test]
+#[ignore = "requires js-debug"]
+fn test_basic_debugging_workflow_ts() {
+    let js_debug_path = match js_debug_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: js-debug not available");
+            return;
+        }
+    };
+
+    let node_path = match which::which("node") {
+        Ok(path) => path,
+        Err(_) => {
+            eprintln!("Skipping test: Node.js not available");
+            return;
+        }
+    };
+
+    let mut ctx = TestContext::new("basic_workflow_ts");
+    ctx.create_config_with_tcp(
+        "js-debug",
+        node_path.to_str().unwrap(),
+        &[js_debug_path.to_str().unwrap()],
+        "tcpportarg",
+    );
+
+    let ts_file = ctx.fixtures_dir.join("simple.ts");
+    let markers = ctx.find_breakpoint_markers(&ts_file);
+    let main_start_line = markers.get("main_start").expect("Missing main_start marker");
+
+    ctx.cleanup_daemon();
+
+    let output = ctx.run_debugger_ok(&[
+        "start",
+        ts_file.to_str().unwrap(),
+    ]);
+    assert!(output.contains("Started debugging") || output.contains("session"));
+
+    let bp_location = format!("simple.ts:{}", main_start_line);
+    let output = ctx.run_debugger_ok(&["break", &bp_location]);
+    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));
+
+    let output = ctx.run_debugger_ok(&["continue"]);
+    assert!(output.contains("Continuing") || output.contains("running"));
+
+    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
+    assert!(
+        output.contains("Stopped") || output.contains("breakpoint"),
+        "Expected stop at breakpoint: {}",
+        output
+    );
+
+    let _ = ctx.run_debugger(&["continue"]);
+    let _ = ctx.run_debugger(&["await", "--timeout", "10"]);
+    let _ = ctx.run_debugger(&["stop"]);
+}
+
+#[test]
+#[ignore = "requires js-debug"]
+fn test_stepping_js() {
+    let js_debug_path = match js_debug_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: js-debug not available");
+            return;
+        }
+    };
+
+    let node_path = match which::which("node") {
+        Ok(path) => path,
+        Err(_) => {
+            eprintln!("Skipping test: Node.js not available");
+            return;
+        }
+    };
+
+    let mut ctx = TestContext::new("stepping_js");
+    ctx.create_config_with_tcp(
+        "js-debug",
+        node_path.to_str().unwrap(),
+        &[js_debug_path.to_str().unwrap()],
+        "tcpportarg",
+    );
+
+    let js_file = ctx.fixtures_dir.join("simple.js");
+    let markers = ctx.find_breakpoint_markers(&js_file);
+    let before_add_line = markers.get("before_add").expect("Missing before_add marker");
+
+    ctx.cleanup_daemon();
+
+    let output = ctx.run_debugger_ok(&["start", js_file.to_str().unwrap()]);
+    assert!(output.contains("Started debugging") || output.contains("session"));
+
+    let bp_location = format!("simple.js:{}", before_add_line);
+    let _ = ctx.run_debugger_ok(&["break", &bp_location]);
+    let _ = ctx.run_debugger_ok(&["continue"]);
+    let _ = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
+
+    let output = ctx.run_debugger_ok(&["step-in"]);
+    assert!(output.contains("Stepped") || output.contains("step"));
+
+    let output = ctx.run_debugger_ok(&["step-out"]);
+    assert!(output.contains("Stepped") || output.contains("step"));
+
+    let _ = ctx.run_debugger(&["continue"]);
+    let _ = ctx.run_debugger(&["stop"]);
+}
+
+#[test]
+#[ignore = "requires js-debug"]
+fn test_expression_evaluation_js() {
+    let js_debug_path = match js_debug_available() {
+        Some(path) => path,
+        None => {
+            eprintln!("Skipping test: js-debug not available");
+            return;
+        }
+    };
+
+    let node_path = match which::which("node") {
+        Ok(path) => path,
+        Err(_) => {
+            eprintln!("Skipping test: Node.js not available");
+            return;
+        }
+    };
+
+    let mut ctx = TestContext::new("expression_eval_js");
+    ctx.create_config_with_tcp(
+        "js-debug",
+        node_path.to_str().unwrap(),
+        &[js_debug_path.to_str().unwrap()],
+        "tcpportarg",
+    );
+
+    let js_file = ctx.fixtures_dir.join("simple.js");
+    let markers = ctx.find_breakpoint_markers(&js_file);
+    let main_start_line = markers.get("main_start").expect("Missing main_start marker");
+
+    ctx.cleanup_daemon();
+
+    let output = ctx.run_debugger_ok(&["start", js_file.to_str().unwrap()]);
+    assert!(output.contains("Started debugging") || output.contains("session"));
+
+    let bp_location = format!("simple.js:{}", main_start_line);
+    let _ = ctx.run_debugger_ok(&["break", &bp_location]);
+    let _ = ctx.run_debugger_ok(&["continue"]);
+    let _ = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
+
+    let output = ctx.run_debugger_ok(&["eval", "x + y"]);
+    assert!(
+        output.contains("30") || output.contains("result"),
+        "Expected evaluation result: {}",
+        output
+    );
+
+    let _ = ctx.run_debugger(&["continue"]);
+    let _ = ctx.run_debugger(&["stop"]);
+}
```

Add helper method to TestContext:

```diff
--- a/tests/integration.rs
+++ b/tests/integration.rs
@@ -145,6 +145,32 @@ impl TestContext {
         self.create_config_with_args(adapter_name, adapter_path, &[]);
     }

+    /// Create a config file for a TCP adapter
+    fn create_config_with_tcp(
+        &self,
+        adapter_name: &str,
+        adapter_path: &str,
+        args: &[&str],
+        spawn_style: &str,
+    ) {
+        let args_str = args.iter()
+            .map(|a| format!("\"{}\"", a))
+            .collect::<Vec<_>>()
+            .join(", ");
+        let config_content = format!(
+            r#"
+[adapters.{adapter_name}]
+path = "{adapter_path}"
+args = [{args_str}]
+transport = "tcp"
+spawn_style = "{spawn_style}"
+
+[defaults]
+adapter = "{adapter_name}"
+"#,
+        );
+        // ... rest same as create_config_with_args
+    }
+
     /// Create a config file for the test with custom args
     fn create_config_with_args(&self, adapter_name: &str, adapter_path: &str, args: &[&str]) {
         let args_str = args.iter()
```


---

### Milestone 6: Documentation

**Delegated to**: @agent-technical-writer (mode: post-implementation)

**Source**: `## Invisible Knowledge` section of this plan

**Files**:
- `src/setup/adapters/CLAUDE.md` (update)
- `src/setup/adapters/README.md` (new or update)
- `src/dap/CLAUDE.md` (update if exists)

**Requirements**:
Delegate to Technical Writer. Key deliverables:
- Update adapters CLAUDE.md with js-debug entry in table
- Document TcpSpawnStyle in DAP module
- README.md with js-debug architecture and sourcemap handling

**Acceptance Criteria**:
- CLAUDE.md is tabular index only
- README.md captures spawn_style decision and js-debug specifics
- Documentation matches implementation

## Milestone Dependencies

```
M1 (Core Infrastructure)
    |
    +---> M2 (Installer) ---> M3 (Session) ---> M5 (Tests)
    |                                              |
    +---> M4 (Fixtures) --------------------------+
                                                   |
                                                   v
                                             M6 (Documentation)
```

**Parallel opportunities**:
- M2 (Installer) and M4 (Fixtures) can run in parallel after M1
- M5 (Tests) requires M2, M3, and M4 complete
- M6 runs after all implementation milestones
