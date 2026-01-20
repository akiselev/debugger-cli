To implement a robust end-to-end test executor, we should move away from parsing CLI text output (like the current Python script does) and instead integrate a **Test Runner** directly into the CLI binary.

This runner will read a **Test Scenario (YAML)** and use the existing `DaemonClient` to communicate directly with the debug daemon. This ensures assertions are made against structured data (JSON/Structs) rather than fragile string matching.

### 1. Test Scenario YAML Format

This file defines the environment, setup steps, and the sequence of actions and assertions.

```yaml
# tests/scenarios/complex_verification.yml
name: "Complex Variable Verification"
description: "Verifies local variables and recursion handling"

# Optional: Setup commands (e.g., compilation)
setup:
  - shell: "gcc -g tests/e2e/hello_world.c -o tests/e2e/hello_world"

# Configuration for the debug session
target:
  program: "tests/e2e/hello_world"
  args: []
  adapter: "lldb-dap" # optional
  stop_on_entry: true

# Execution Flow
steps:
  # 1. Set a breakpoint
  - action: command
    command: "break add main"
    expect:
      success: true
      output_contains: "Breakpoint"

  # 2. Continue to breakpoint
  - action: command
    command: "continue"

  # 3. Wait for the stop event
  - action: await
    timeout: 10
    expect:
      reason: "breakpoint"
      file: "hello_world.c"
      line: 15

  # 4. Inspect Local Variables (Robust Assertion)
  - action: inspect_locals
    asserts:
      - name: "x"
        value: "10"
        type: "int"
      - name: "y"
        value: "20"
  
  # 5. Check Output
  - action: command
    command: "output"
    expect:
      output_contains: "Initializing..."

  # 6. Finish
  - action: command
    command: "continue"
    
  - action: await
    expect:
      reason: "exited"
      exit_code: 0

```

### 2. Rust Implementation Plan

#### A. Add `Test` Subcommand

Update `src/commands.rs` to include the new command.

```rust
// src/commands.rs

#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...

    /// Execute a test scenario defined in a YAML file
    Test {
        /// Path to the YAML test scenario file
        path: PathBuf,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,
    },
}

```

#### B. Data Structures (Config)

Create `src/testing/config.rs` to deserialize the YAML.

```rust
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct TestScenario {
    pub name: String,
    pub description: Option<String>,
    pub setup: Option<Vec<SetupStep>>,
    pub target: TargetConfig,
    pub steps: Vec<TestStep>,
}

#[derive(Deserialize, Debug)]
pub struct SetupStep {
    pub shell: String,
}

#[derive(Deserialize, Debug)]
pub struct TargetConfig {
    pub program: String,
    pub args: Option<Vec<String>>,
    pub adapter: Option<String>,
    pub stop_on_entry: bool,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TestStep {
    Command {
        command: String, // e.g., "break add main" or "next"
        expect: Option<CommandExpectation>,
    },
    Await {
        timeout: Option<u64>,
        expect: Option<StopExpectation>,
    },
    InspectLocals {
        asserts: Vec<VariableAssertion>,
    },
}

#[derive(Deserialize, Debug)]
pub struct CommandExpectation {
    pub success: Option<bool>,
    pub output_contains: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct StopExpectation {
    pub reason: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub exit_code: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct VariableAssertion {
    pub name: String,
    pub value: Option<String>,
    pub type_name: Option<String>,
}

```

#### C. The Test Runner Logic

Create `src/testing/runner.rs`. This uses `DaemonClient` directly, bypassing the CLI text formatting in `src/cli/mod.rs`.

```rust
use crate::ipc::DaemonClient;
use crate::ipc::protocol::{Command, StopResult, VariableInfo};
use crate::common::Result;
use super::config::{TestScenario, TestStep};
use std::path::Path;
use colored::*; // Assuming colored output for test results

pub async fn run_scenario(path: &Path) -> Result<()> {
    // 1. Load YAML
    let content = std::fs::read_to_string(path)?;
    let scenario: TestScenario = serde_yaml::from_str(&content)?;
    
    println!("Running Test: {}", scenario.name.blue().bold());

    // 2. Setup (Shell commands)
    if let Some(setup_steps) = scenario.setup {
        for step in setup_steps {
            // Use std::process::Command to run shell setup
        }
    }

    // 3. Connect to Daemon
    let mut client = DaemonClient::connect().await?;

    // 4. Start Session
    client.send_command(Command::Start {
        program: scenario.target.program.into(),
        args: scenario.target.args.unwrap_or_default(),
        adapter: scenario.target.adapter,
        stop_on_entry: scenario.target.stop_on_entry,
    }).await?;

    // 5. Execute Steps
    for (i, step) in scenario.steps.iter().enumerate() {
        print!("Step {}: ... ", i + 1);
        
        match step {
            TestStep::Command { command, expect } => {
                // We need a parser here to convert string "break add main" 
                // into Protocol Command enum. 
                // Reuse the Clap parser from crate::commands or map manually.
                // For robustness, mapping common test commands manually is often safer.
                
                // Example for "break add":
                // let cmd = Command::BreakpointAdd { ... };
                // let res = client.send_command(cmd).await?;
                
                // Verify `expect` (success/failure)
            },
            TestStep::InspectLocals { asserts } => {
                let res = client.send_command(Command::Locals { frame_id: None }).await?;
                let vars: Vec<VariableInfo> = serde_json::from_value(res["variables"].clone())?;
                
                for assertion in asserts {
                    let found = vars.iter().find(|v| v.name == assertion.name);
                    if let Some(var) = found {
                        if let Some(val) = &assertion.value {
                            if &var.value != val {
                                return Err(format!("Var {} expected {} got {}", assertion.name, val, var.value).into());
                            }
                        }
                    } else {
                         return Err(format!("Variable {} not found", assertion.name).into());
                    }
                }
            },
            TestStep::Await { timeout, expect } => {
                let res = client.send_command(Command::Await { timeout_secs: timeout.unwrap_or(30) }).await?;
                // Parse StopResult and compare with `expect`
            }
        }
        println!("{}", "OK".green());
    }

    println!("{}", "Test Passed".green().bold());
    Ok(())
}

```

### 3. Benefits of this Approach

1. **Direct API Access:** It tests the logic, not the string formatting of the CLI.
2. **Platform Independent:** YAML definitions are cleaner than Python subprocess scripts.
3. **CI/CD Friendly:** Returns standard exit codes; easy to wire into GitHub Actions.
4. **Extensible:** Easy to add new assertion types (e.g., `InspectStack`, `InspectThreads`) without parsing regex.

You would execute this with:

```bash
cargo run -- test tests/scenarios/complex_app.yml

```