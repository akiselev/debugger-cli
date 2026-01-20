# E2E Test Runner Implementation Notes

## Overview

Implemented a robust end-to-end test executor that reads YAML test scenarios and uses the `DaemonClient` to communicate directly with the debug daemon. This approach ensures assertions are made against structured data (JSON/Structs) rather than fragile string matching.

## Implementation Date
January 20, 2026

## Files Created

### New Module: `src/testing/`

1. **`src/testing/mod.rs`** - Module entry point
   - Exports config types and `run_scenario` function

2. **`src/testing/config.rs`** - Test scenario configuration types
   - `TestScenario` - Root structure for YAML test files
   - `SetupStep` - Shell commands to run before tests (e.g., compilation)
   - `TargetConfig` - Debug target configuration (program, args, adapter)
   - `TestStep` - Enum of supported test actions:
     - `Command` - Execute debugger commands
     - `Await` - Wait for stop events
     - `InspectLocals` - Check local variables
     - `InspectStack` - Check stack frames
     - `CheckOutput` - Verify program output
     - `Evaluate` - Evaluate expressions

3. **`src/testing/runner.rs`** - Test execution logic
   - `run_scenario()` - Main entry point, loads YAML and executes test
   - Command parsing for common debugger commands
   - Assertion checking for variables, stack frames, stop events
   - Colored output for test progress and results

### Sample Test Scenarios: `tests/scenarios/`

1. **`hello_world_c.yml`** - Basic C debugging test
2. **`hello_world_python.yml`** - Python debugging with debugpy
3. **`hello_world_rust.yml`** - Rust debugging test
4. **`complex_verification.yml`** - Variable inspection and expression evaluation

## Files Modified

1. **`Cargo.toml`** - Added dependencies:
   - `serde_yaml = "0.9"` - YAML parsing
   - `colored = "2"` - Colored terminal output

2. **`src/lib.rs`** - Added `pub mod testing;`

3. **`src/commands.rs`** - Added `Test` subcommand:
   ```rust
   Test {
       path: PathBuf,
       #[arg(long, short)]
       verbose: bool,
   }
   ```

4. **`src/cli/mod.rs`** - Changes:
   - Made `spawn` module public (`pub mod spawn`)
   - Added `use crate::testing;`
   - Added dispatch handler for `Commands::Test`

5. **`src/common/error.rs`** - Added error variant:
   ```rust
   #[error("Test assertion failed: {0}")]
   TestAssertion(String),
   ```

## YAML Test Format

```yaml
name: "Test Name"
description: "Optional description"

# Optional setup commands (e.g., compilation)
setup:
  - shell: "gcc -g program.c -o program"

# Debug target configuration
target:
  program: "path/to/program"
  args: ["arg1", "arg2"]
  adapter: "lldb-dap"  # optional
  stop_on_entry: true

# Test steps
steps:
  - action: command
    command: "break main"
    expect:
      success: true

  - action: await
    timeout: 10
    expect:
      reason: "breakpoint"
      file: "program.c"
      line: 15

  - action: inspect_locals
    asserts:
      - name: "x"
        value: "10"
        type: "int"

  - action: evaluate
    expression: "x + y"
    expect:
      result: "30"

  - action: check_output
    contains: "Hello"
```

## Supported Test Actions

| Action | Description |
|--------|-------------|
| `command` | Execute a debugger command (break, continue, next, step, etc.) |
| `await` | Wait for stop event with optional assertions on reason/location |
| `inspect_locals` | Check local variable names, values, and types |
| `inspect_stack` | Check stack frame function names, files, and lines |
| `check_output` | Verify program stdout/stderr contains expected text |
| `evaluate` | Evaluate expression and check result |

## Supported Commands (in test steps)

- Execution: `continue`, `c`, `next`, `n`, `step`, `s`, `finish`, `out`, `pause`
- Breakpoints: `break <location>`, `breakpoint add/remove/list/enable/disable`
- Inspection: `locals`, `backtrace`, `bt`, `print <expr>`, `eval <expr>`
- Navigation: `frame <n>`, `up`, `down`, `thread <id>`, `threads`
- Session: `stop`, `detach`, `restart`

## Usage

```bash
# Run a single test scenario
cargo run -- test tests/scenarios/hello_world_c.yml

# With verbose output
cargo run -- test tests/scenarios/hello_world_c.yml --verbose
```

## Exit Codes

- `0` - Test passed
- `1` - Test failed (assertion failure or error)

## Benefits

1. **Direct API Access** - Tests the logic, not string formatting
2. **Platform Independent** - YAML definitions are cleaner than Python scripts
3. **CI/CD Friendly** - Returns standard exit codes for GitHub Actions
4. **Extensible** - Easy to add new assertion types

## Future Enhancements

- [ ] Add `--all` flag to run all scenarios in a directory
- [ ] Add test report generation (JUnit XML, JSON)
- [ ] Add parallel test execution
- [ ] Add retry logic for flaky tests
- [ ] Add screenshot/source snapshot on failure
- [ ] Add conditional steps based on platform/adapter
