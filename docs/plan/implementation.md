# LLM Debugger CLI - Implementation Plan

## Phase 1: Project Setup & Core Infrastructure

### Step 1.1: Initialize Cargo Project

Create a single crate (not a workspace):

```bash
cargo init --name debugger
```

### Step 1.2: Define Core Dependencies

```toml
# Cargo.toml
[package]
name = "debugger"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
interprocess = { version = "2", features = ["tokio"] }
which = "7"
directories = "5"
toml = "0.8"
```

### Step 1.3: Implement IPC Protocol Types (`src/ipc/protocol.rs`)

```rust
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub command: Command,
}

#[derive(Serialize, Deserialize)]
pub enum Command {
    Start { program: PathBuf, args: Vec<String>, adapter: Option<String> },
    Attach { pid: u32, adapter: Option<String> },
    Detach,
    Stop,
    Status,

    BreakpointAdd { location: BreakpointLocation, condition: Option<String> },
    BreakpointRemove { id: u32 },
    BreakpointList,

    Continue,
    Next,
    StepIn,
    StepOut,
    Pause,

    StackTrace { thread_id: Option<i64> },
    Threads,
    Scopes { frame_id: i64 },
    Variables { reference: i64 },
    Evaluate { expression: String, frame_id: Option<i64> },

    Await { timeout_secs: Option<u64> },
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub result: Result<Value, Error>,
}
```

### Step 1.4: Implement Common Utilities (`src/common/`)

```rust
// paths.rs - Cross-platform socket/pipe paths
pub fn socket_path() -> PathBuf;
pub fn config_path() -> PathBuf;
pub fn ensure_socket_dir() -> io::Result<PathBuf>;

// error.rs - Shared error types
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Daemon not running. Start a session with 'debugger start <program>'")]
    DaemonNotRunning,

    #[error("Failed to spawn daemon: timed out waiting for socket")]
    DaemonSpawnTimeout,

    #[error("Failed to connect to daemon: {0}")]
    DaemonConnectionFailed(#[from] std::io::Error),

    #[error("No debug session active. Use 'debugger start <program>' first")]
    SessionNotActive,

    #[error("Debug adapter '{0}' not found")]
    AdapterNotFound(String),

    #[error("Operation timed out")]
    Timeout,

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Invalid breakpoint location: {0}")]
    InvalidLocation(String),
}
```

## Phase 2: Daemon Mode Implementation

### Step 2.1: Entry Point with Mode Detection (`src/main.rs`)

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "debugger", about = "LLM-friendly debugger CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start debugging a program
    Start { /* ... */ },

    /// [Hidden] Run in daemon mode - spawned automatically
    #[command(hide = true)]
    Daemon,

    // ... other commands
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon => daemon::run().await,
        other => cli::dispatch(other).await,
    }
}
```

### Step 2.2: Daemon Server (`src/daemon/server.rs`)

```rust
pub struct Daemon {
    listener: LocalSocketListener,
    session: Option<DebugSession>,
    config: Config,
    output_buffer: VecDeque<OutputEvent>,  // Buffer output when no client connected
}

impl Daemon {
    pub async fn run() -> Result<()> {
        let mut daemon = Self::new().await?;

        loop {
            tokio::select! {
                conn = daemon.listener.accept() => {
                    daemon.handle_client(conn?).await?;
                }
                event = daemon.poll_dap_events(), if daemon.session.is_some() => {
                    daemon.handle_dap_event(event?).await?;
                }
            }
        }
    }
}
```

### Step 2.2: Debug Session Management

```rust
// session.rs
pub struct DebugSession {
    adapter: Child,           // DAP adapter subprocess
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    state: SessionState,
    seq: AtomicU64,           // DAP sequence counter (u64 for cross-platform atomicity)
    pending_requests: HashMap<u64, oneshot::Sender<Response>>,
    breakpoints: HashMap<PathBuf, Vec<Breakpoint>>,
    threads: Vec<Thread>,
    stopped_thread: Option<i64>,
}

impl DebugSession {
    pub async fn launch(config: &LaunchConfig) -> Result<Self>;
    pub async fn attach(pid: u32, config: &AttachConfig) -> Result<Self>;

    // DAP message handling
    async fn send_request<R: DapRequest>(&mut self, req: R) -> Result<R::Response>;
    async fn poll_event(&mut self) -> Result<Event>;

    // High-level operations
    pub async fn set_breakpoint(&mut self, loc: &BreakpointLocation) -> Result<Breakpoint>;
    pub async fn continue_execution(&mut self) -> Result<()>;
    pub async fn step_next(&mut self) -> Result<()>;
    pub async fn get_stack_trace(&mut self, thread_id: i64) -> Result<Vec<StackFrame>>;
}
```

### Step 2.3: DAP Protocol Implementation

```rust
// adapter.rs - DAP wire protocol
pub struct DapConnection {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    seq: u64,
}

impl DapConnection {
    pub async fn send_request(&mut self, request: Request) -> Result<()> {
        let json = serde_json::to_string(&request)?;
        let header = format!("Content-Length: {}\r\n\r\n", json.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn read_message(&mut self) -> Result<ProtocolMessage> {
        // Read headers line by line (more efficient than byte-by-byte)
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).await?;

            if line == "\r\n" {
                break; // Empty line signals end of headers
            }

            if let Some(value) = line.strip_prefix("Content-Length: ") {
                content_length = Some(
                    value.trim().parse::<usize>()
                        .map_err(|e| Error::Protocol(format!("invalid Content-Length: {e}")))?
                );
            }
        }

        let len = content_length
            .ok_or_else(|| Error::Protocol("missing Content-Length header".to_string()))?;

        // Read JSON body
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body).await?;
        Ok(serde_json::from_slice(&body)?)
    }
}
```

### Step 2.4: Initialization Sequence

```rust
impl DebugSession {
    async fn initialize(&mut self) -> Result<Capabilities> {
        // 1. Send initialize request
        let caps = self.send_request(InitializeRequest {
            adapter_id: "debugger-cli".into(),
            client_id: Some("debugger-cli".into()),
            client_name: Some("LLM Debugger CLI".into()),
            lines_start_at_1: true,
            columns_start_at_1: true,
            path_format: Some("path".into()),
            supports_variable_type: true,
            supports_variable_paging: true,
            // ... other capabilities we support
        }).await?;

        // 2. Wait for initialized event
        loop {
            match self.poll_event().await? {
                Event::Initialized => break,
                other => self.queue_event(other),
            }
        }

        Ok(caps)
    }

    async fn configure_and_launch(&mut self, program: &Path, args: &[String]) -> Result<()> {
        // 3. Set initial breakpoints (if any queued)
        for (path, bps) in &self.pending_breakpoints {
            self.send_request(SetBreakpointsRequest {
                source: Source { path: Some(path.clone()), .. },
                breakpoints: bps.clone(),
            }).await?;
        }

        // 4. Signal configuration done
        self.send_request(ConfigurationDoneRequest {}).await?;

        // 5. Launch the program
        self.send_request(LaunchRequest {
            program: program.to_string_lossy().into(),
            args: args.to_vec(),
            cwd: std::env::current_dir()?.to_string_lossy().into(),
            // ... other launch args
        }).await?;

        Ok(())
    }
}
```

## Phase 3: CLI Implementation

### Step 3.1: Command Structure with Clap

```rust
// main.rs
#[derive(Parser)]
#[command(name = "debugger", about = "LLM-friendly debugger CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start debugging a program
    Start {
        program: PathBuf,
        #[arg(last = true)]
        args: Vec<String>,
        #[arg(long)]
        adapter: Option<String>,
    },

    /// Attach to a running process
    Attach {
        pid: u32,
        #[arg(long)]
        adapter: Option<String>,
    },

    /// Breakpoint management
    #[command(subcommand, alias = "break", alias = "b")]
    Breakpoint(BreakpointCommands),

    /// Continue execution
    #[command(alias = "c")]
    Continue,

    /// Step over
    #[command(alias = "n")]
    Next,

    /// Step into
    #[command(alias = "s")]
    Step,

    /// Step out
    #[command(alias = "out")]
    Finish,

    /// Print stack trace
    #[command(alias = "bt")]
    Backtrace,

    /// Evaluate expression
    #[command(alias = "p")]
    Print { expression: String },

    /// Wait for stop event
    Await {
        #[arg(long, default_value = "300")]
        timeout: u64,
    },

    /// Get status
    Status,

    /// Stop debugging
    Stop,

    /// Detach from process
    Detach,
}
```

### Step 3.2: Daemon Connection

```rust
// client.rs
pub struct DaemonClient {
    stream: LocalSocketStream,
    next_id: u64,
}

impl DaemonClient {
    pub async fn connect() -> Result<Self> {
        let path = socket_path();
        let stream = LocalSocketStream::connect(path).await
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    Error::DaemonNotRunning
                } else {
                    Error::DaemonConnectionFailed(e)
                }
            })?;
        Ok(Self { stream, next_id: 1 })
    }

    pub async fn send_command(&mut self, cmd: Command) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = Request { id, command: cmd };
        let json = serde_json::to_vec(&request)?;

        // Send length-prefixed message
        self.stream.write_all(&(json.len() as u32).to_le_bytes()).await?;
        self.stream.write_all(&json).await?;

        // Read response
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut body = vec![0u8; len];
        self.stream.read_exact(&mut body).await?;

        let response: Response = serde_json::from_slice(&body)?;
        response.result.map_err(Into::into)
    }
}
```

### Step 3.3: Daemon Spawning (Same Binary)

```rust
// src/cli/spawn.rs
pub async fn ensure_daemon_running() -> Result<()> {
    match DaemonClient::connect().await {
        Ok(_) => Ok(()), // Already running
        Err(Error::DaemonNotRunning) => spawn_daemon().await,
        Err(e) => Err(e),
    }
}

async fn spawn_daemon() -> Result<()> {
    // Use the same binary with "daemon" subcommand
    let exe_path = std::env::current_exe()?;

    // Spawn detached
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std::process::Command::new(&exe_path)
            .arg("daemon")  // Run in daemon mode
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0) // New process group (detach from terminal)
            .spawn()?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        std::process::Command::new(&exe_path)
            .arg("daemon")  // Run in daemon mode
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()?;
    }

    // Wait for socket to appear
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if DaemonClient::connect().await.is_ok() {
            return Ok(());
        }
    }

    Err(Error::DaemonSpawnTimeout)
}
```

## Phase 4: Essential Commands

### Step 4.1: Start Command

```rust
// commands/start.rs
pub async fn run(program: PathBuf, args: Vec<String>, adapter: Option<String>) -> Result<()> {
    ensure_daemon_running().await?;

    let mut client = DaemonClient::connect().await?;

    let result = client.send_command(Command::Start {
        program: program.canonicalize()?,
        args,
        adapter,
    }).await?;

    println!("Started debugging: {}", program.display());
    println!("Session ID: {}", result["session_id"]);
    Ok(())
}
```

### Step 4.2: Breakpoint Commands

```rust
// commands/breakpoint.rs
pub async fn add(location: &str, condition: Option<String>) -> Result<()> {
    let loc = parse_location(location)?;

    let mut client = DaemonClient::connect().await?;
    let result = client.send_command(Command::BreakpointAdd {
        location: loc,
        condition,
    }).await?;

    let bp: BreakpointResult = serde_json::from_value(result)?;

    if bp.verified {
        println!("Breakpoint {} set at {}:{}", bp.id, bp.source, bp.line);
    } else {
        println!("Breakpoint {} pending (not yet verified)", bp.id);
    }

    Ok(())
}

fn parse_location(s: &str) -> Result<BreakpointLocation> {
    // Handle file:line format, being careful with Windows paths like "C:\path\file.rs:10"
    // Strategy: find the last ':' that's followed by digits only
    if let Some(colon_idx) = s.rfind(':') {
        let (file_part, line_part) = s.split_at(colon_idx);
        let line_str = &line_part[1..]; // Skip the ':'

        // Only treat as file:line if the part after ':' is a valid line number
        if !line_str.is_empty() && line_str.chars().all(|c| c.is_ascii_digit()) {
            let line: u32 = line_str.parse()
                .map_err(|_| Error::InvalidLocation(format!("invalid line number: {}", line_str)))?;
            return Ok(BreakpointLocation::Line {
                file: PathBuf::from(file_part),
                line,
            });
        }
    }
    // No valid file:line pattern, treat as function name
    Ok(BreakpointLocation::Function { name: s.to_string() })
}
```

### Step 4.3: Await Command

```rust
// commands/await.rs
pub async fn run(timeout_secs: u64) -> Result<()> {
    let mut client = DaemonClient::connect().await?;

    let result = client.send_command(Command::Await {
        timeout_secs: Some(timeout_secs),
    }).await?;

    let stop: StopResult = serde_json::from_value(result)?;

    match stop.reason.as_str() {
        "breakpoint" => {
            println!("Stopped at breakpoint");
            print_location(&stop.location);
        }
        "step" => {
            println!("Step completed");
            print_location(&stop.location);
        }
        "exception" => {
            println!("Exception: {}", stop.description.unwrap_or_default());
            print_location(&stop.location);
        }
        "exited" => {
            println!("Program exited with code {}", stop.exit_code.unwrap_or(0));
        }
        reason => {
            println!("Stopped: {}", reason);
        }
    }

    Ok(())
}
```

### Step 4.4: Inspection Commands

```rust
// commands/inspect.rs
pub async fn backtrace(thread_id: Option<i64>) -> Result<()> {
    let mut client = DaemonClient::connect().await?;

    let result = client.send_command(Command::StackTrace { thread_id }).await?;
    let frames: Vec<StackFrame> = serde_json::from_value(result)?;

    for (i, frame) in frames.iter().enumerate() {
        println!("#{} {} at {}:{}",
            i,
            frame.name,
            frame.source.as_ref().map(|s| s.path.as_deref().unwrap_or("?")).unwrap_or("?"),
            frame.line
        );
    }

    Ok(())
}

pub async fn print_expr(expression: &str) -> Result<()> {
    let mut client = DaemonClient::connect().await?;

    let result = client.send_command(Command::Evaluate {
        expression: expression.to_string(),
        frame_id: None, // Current frame
    }).await?;

    let eval: EvaluateResult = serde_json::from_value(result)?;
    println!("{} = {}", expression, eval.result);

    if let Some(ty) = eval.type_name {
        println!("  type: {}", ty);
    }

    Ok(())
}
```

## Phase 5: Advanced Features

### Step 5.1: Watch Expressions (Data Breakpoints)

```rust
// commands/watch.rs
pub async fn add(expression: &str, access_type: AccessType) -> Result<()> {
    let mut client = DaemonClient::connect().await?;

    // First evaluate to get the address
    let eval = client.send_command(Command::Evaluate {
        expression: format!("&({})", expression),
        frame_id: None,
    }).await?;

    // Then set data breakpoint
    let data_id = eval.get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Protocol("missing 'result' in evaluate response".to_string()))?;

    let result = client.send_command(Command::DataBreakpointAdd {
        data_id: data_id.to_string(),
        access_type,
        condition: None,
    }).await?;

    let watch_result: WatchResult = serde_json::from_value(result)?;
    println!("Watch {} set on {}", watch_result.id, expression);
    Ok(())
}
```

### Step 5.2: Output Streaming

```rust
// commands/output.rs
pub async fn stream(follow: bool) -> Result<()> {
    let mut client = DaemonClient::connect().await?;

    if follow {
        // Subscribe to output events
        client.send_command(Command::SubscribeOutput).await?;

        loop {
            let event = client.receive_event().await?;
            match event {
                Event::Output { category, output } => {
                    let prefix = match category.as_str() {
                        "stdout" => "",
                        "stderr" => "[stderr] ",
                        "console" => "[debugger] ",
                        _ => "[?] ",
                    };
                    print!("{}{}", prefix, output);
                }
                Event::SessionEnded => break,
                _ => {}
            }
        }
    } else {
        let result = client.send_command(Command::GetOutput).await?;
        // Safely extract output field with proper error handling
        if let Some(output) = result.get("output").and_then(|v| v.as_str()) {
            print!("{}", output);
        }
    }

    Ok(())
}
```

### Step 5.3: Configuration File

```rust
// config.rs
#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub adapters: HashMap<String, AdapterConfig>,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Deserialize)]
pub struct AdapterConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Deserialize, Default)]
pub struct Defaults {
    #[serde(default = "default_adapter")]
    pub adapter: String,
}

fn default_adapter() -> String { "lldb-dap".into() }

pub fn load_config() -> Result<Config> {
    let path = config_path().join("config.toml");
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}
```

## Phase 6: Testing & Polish

### Step 6.1: Mock DAP Adapter

Create a mock adapter for testing that simulates DAP responses without a real debugger.

### Step 6.2: Integration Tests

```rust
#[tokio::test]
async fn test_basic_debug_session() {
    // Compile test program
    let test_program = compile_test_program("simple_loop");

    // Start daemon
    let daemon = TestDaemon::spawn().await;

    // Run through basic workflow
    let mut client = daemon.connect().await;

    client.start(&test_program).await.unwrap();
    client.breakpoint_add("test.rs:10").await.unwrap();
    client.continue_().await.unwrap();

    let stop = client.await_stop(10).await.unwrap();
    assert_eq!(stop.reason, "breakpoint");

    let stack_frames = client.backtrace().await.unwrap();
    assert!(!stack_frames.is_empty());

    client.stop().await.unwrap();
}
```

### Step 6.3: Error Messages

Ensure all errors are clear and actionable for LLM agents:

```
Error: Daemon not running
  Hint: Start a debug session first with 'debugger start <program>'

Error: No debug session active
  Hint: Use 'debugger start <program>' or 'debugger attach <pid>'

Error: Breakpoint location not found
  The file 'src/foo.rs' exists but line 999 is beyond end of file (file has 50 lines)

Error: Adapter 'lldb-dap' not found
  Searched: /usr/bin/lldb-dap, /usr/local/bin/lldb-dap
  Hint: Install LLVM or specify adapter path with --adapter
```

## Implementation Order

### Phase 1: Foundation
1. Project structure, Cargo.toml, module stubs
2. IPC protocol types and transport (interprocess)
3. Common utilities (paths, error types)

### Phase 2: Daemon Core
4. Daemon mode entry point and server loop
5. DAP wire protocol (Content-Length framing)
6. Debug session state machine
7. Adapter spawning (lldb-dap)

### Phase 3: CLI Commands
8. CLI parsing with clap
9. Daemon spawning and connection
10. Start/stop/status commands
11. Basic session lifecycle

### Phase 4: Debugging Features
12. Breakpoint commands (add, remove, list)
13. Execution control (continue, next, step, finish)
14. Initialize/launch DAP sequence

### Phase 5: Inspection
15. Stack trace (backtrace)
16. Variables and scopes (locals, print)
17. Expression evaluation (eval)
18. Thread management

### Phase 6: Async & Polish
19. Await command (wait for stop event)
20. Output buffering and retrieval
21. Error messages for LLM agents
22. Configuration file support

### Phase 7: Advanced
23. Watch expressions (data breakpoints)
24. Memory read
25. CodeLLDB adapter support
26. Integration tests with mock adapter

## Success Criteria

1. LLM agent can start/stop debug sessions reliably
2. Breakpoints can be set and hit
3. Program state can be inspected at breakpoints
4. Works on Linux, macOS, and Windows
5. Clear error messages guide the agent to correct usage
6. Daemon survives CLI exit and can be reconnected
