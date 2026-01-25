//! CLI command definitions
//!
//! Defines the clap commands for the debugger CLI.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Commands {
    /// Start debugging a program
    Start {
        /// Path to the executable to debug
        program: PathBuf,

        /// Arguments to pass to the program
        #[arg(last = true)]
        args: Vec<String>,

        /// Debug adapter to use (default: lldb-dap)
        #[arg(long)]
        adapter: Option<String>,

        /// Stop at program entry point
        #[arg(long)]
        stop_on_entry: bool,

        /// Set initial breakpoint(s) before program starts (file:line or function name)
        /// Can be specified multiple times: --break main --break src/file.c:42
        #[arg(long = "break", short = 'b')]
        initial_breakpoints: Vec<String>,
    },

    /// Attach to a running process
    Attach {
        /// Process ID to attach to
        pid: u32,

        /// Debug adapter to use (default: lldb-dap)
        #[arg(long)]
        adapter: Option<String>,
    },

    /// Breakpoint management
    #[command(subcommand)]
    Breakpoint(BreakpointCommands),

    /// Shorthand for 'breakpoint add'
    #[command(name = "break", alias = "b")]
    Break {
        /// Location: file:line or function name
        location: String,

        /// Condition for the breakpoint
        #[arg(long, short)]
        condition: Option<String>,
    },

    /// Continue execution
    #[command(alias = "c")]
    Continue,

    /// Step over (execute current line, step over function calls)
    #[command(alias = "n")]
    Next,

    /// Step into (execute current line, step into function calls)
    #[command(alias = "s")]
    Step,

    /// Step out (run until current function returns)
    #[command(alias = "out")]
    Finish,

    /// Pause execution
    Pause,

    /// Print stack trace
    #[command(alias = "bt")]
    Backtrace {
        /// Maximum number of frames to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Show local variables for each frame
        #[arg(long)]
        locals: bool,
    },

    /// Show local variables in current frame
    Locals,

    /// Print/evaluate expression
    #[command(alias = "p")]
    Print {
        /// Expression to evaluate
        expression: String,
    },

    /// Evaluate expression (can have side effects)
    Eval {
        /// Expression to evaluate
        expression: String,
    },

    /// Show current position with source context and variables
    #[command(alias = "where")]
    Context {
        /// Number of context lines to show
        #[arg(long, default_value = "5")]
        lines: usize,
    },

    /// List all threads
    Threads,

    /// Switch to a specific thread
    Thread {
        /// Thread ID to switch to
        id: Option<i64>,
    },

    /// Navigate to a specific stack frame
    Frame {
        /// Frame number (0 = innermost/current)
        number: Option<usize>,
    },

    /// Move up the stack (to caller)
    Up,

    /// Move down the stack (toward current frame)
    Down,

    /// Wait for next stop event (breakpoint, step completion, etc.)
    Await {
        /// Timeout in seconds
        #[arg(long, default_value = "300")]
        timeout: u64,
    },

    /// Get debuggee stdout/stderr output
    Output {
        /// Stream output continuously
        #[arg(long)]
        follow: bool,

        /// Get last N lines of output
        #[arg(long)]
        tail: Option<usize>,

        /// Clear output buffer
        #[arg(long)]
        clear: bool,
    },

    /// Get daemon/session status
    Status,

    /// Stop debugging (terminates debuggee and session)
    Stop,

    /// Detach from process (process keeps running)
    Detach,

    /// Restart program (re-launch with same arguments)
    Restart,

    /// View daemon logs (for debugging)
    Logs {
        /// Number of lines to show (default: 50)
        #[arg(long, short = 'n', default_value = "50")]
        lines: usize,

        /// Follow log output (like tail -f)
        #[arg(long, short)]
        follow: bool,

        /// Clear the log file
        #[arg(long)]
        clear: bool,
    },

    /// [Hidden] Run in daemon mode - spawned automatically
    #[command(hide = true)]
    Daemon,

    /// Install and manage debug adapters
    Setup {
        /// Debugger to install (e.g., lldb, codelldb, python, go)
        debugger: Option<String>,

        /// Install specific version
        #[arg(long)]
        version: Option<String>,

        /// List available debuggers and their status
        #[arg(long)]
        list: bool,

        /// Check installed debuggers
        #[arg(long)]
        check: bool,

        /// Auto-install debuggers for detected project types
        #[arg(long, name = "auto")]
        auto_detect: bool,

        /// Uninstall a debugger
        #[arg(long)]
        uninstall: bool,

        /// Show installation path for a debugger
        #[arg(long)]
        path: bool,

        /// Force reinstall even if already installed
        #[arg(long)]
        force: bool,

        /// Show what would be installed without installing
        #[arg(long)]
        dry_run: bool,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Execute a test scenario defined in a YAML file
    Test {
        /// Path to the YAML test scenario file
        path: PathBuf,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
pub enum BreakpointCommands {
    /// Add a breakpoint
    Add {
        /// Location: file:line or function name
        location: String,

        /// Condition for the breakpoint
        #[arg(long, short)]
        condition: Option<String>,

        /// Hit count (break after N hits)
        #[arg(long)]
        hit_count: Option<u32>,
    },

    /// Remove a breakpoint
    Remove {
        /// Breakpoint ID to remove
        id: Option<u32>,

        /// Remove all breakpoints
        #[arg(long)]
        all: bool,
    },

    /// List all breakpoints
    List,

    /// Enable a breakpoint
    Enable {
        /// Breakpoint ID to enable
        id: u32,
    },

    /// Disable a breakpoint
    Disable {
        /// Breakpoint ID to disable
        id: u32,
    },
}
