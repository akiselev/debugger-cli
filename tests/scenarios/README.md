# Test Scenarios

Test scenarios define end-to-end debugging workflows using a YAML DSL. Each scenario compiles a fixture program, starts a debug session, executes commands, and verifies behavior.

## Naming Convention

Scenarios follow the pattern `<feature>_<language>.yml`:

- `hello_world_c.yml` - Basic C program debugging
- `conditional_breakpoint_go.yml` - Conditional breakpoints in Go
- `thread_list_c.yml` - Thread listing with C pthreads
- `stack_navigation_js.yml` - Frame navigation in JavaScript

## YAML DSL Format

### Structure

```yaml
name: "Human-readable test name"
description: "What this test verifies"

setup:
  - shell: "gcc -g tests/fixtures/simple.c -o tests/fixtures/test_simple_c"

target:
  program: "tests/fixtures/test_simple_c"
  args: []
  adapter: "lldb"  # Optional: defaults to lldb-dap
  stop_on_entry: true

steps:
  - action: command
    command: "break main"
    expect:
      success: true

  - action: await
    timeout: 10
    expect:
      reason: "breakpoint"

  - action: inspect_locals
    asserts:
      - name: "x"
        value_contains: "10"
```

### Step Types

| Step Type | Purpose | Fields |
|-----------|---------|--------|
| `command` | Execute debugger command | `command`, `expect` |
| `await` | Wait for stop event | `timeout`, `expect` |
| `inspect_locals` | Verify local variables | `asserts` |
| `inspect_stack` | Verify stack frames | `asserts` |
| `check_output` | Verify program stdout/stderr | `contains`, `equals` |
| `evaluate` | Evaluate expression | `expression`, `expect` |

### Adapter Field

Adapter names map to debug backends:

- `lldb` or omitted - lldb-dap (C, C++, Rust)
- `go` - Delve (Go)
- `python` - debugpy (Python)
- `js-debug` - js-debug (JavaScript, TypeScript)
- `gdb` - GDB 14.1+ or cdt-gdb-adapter (C, C++)

## Running Tests Locally

```bash
# Run single scenario
debugger test tests/scenarios/hello_world_c.yml

# Verbose output
debugger test tests/scenarios/conditional_breakpoint_go.yml --verbose

# Run with specific adapter
debugger test tests/scenarios/hello_world_c.yml --adapter gdb
```

## Running Tests in CI

GitHub Actions runs all scenarios across adapter/OS matrix:

- **LLDB**: Ubuntu + macOS (C, Rust)
- **GDB**: Ubuntu + macOS (C)
- **Delve**: Ubuntu + macOS (Go)
- **debugpy**: Ubuntu + macOS (Python)
- **js-debug**: Ubuntu + macOS (JavaScript, TypeScript)

Tests run on every push and PR via GitHub Actions. The workflow includes parallel jobs for each adapter (LLDB, GDB, Delve, debugpy, js-debug) on Ubuntu and macOS, with graceful fallback for macOS GDB installation failures.

## Adapter Feature Compatibility

Not all features work with all adapters. Tests are created only for compatible combinations.

| Feature | LLDB | GDB | Delve | debugpy | js-debug |
|---------|------|-----|-------|---------|----------|
| Conditional breakpoints | ✅ | ✅ | ✅ | ✅ | ✅ |
| Hit count breakpoints | ✅ | ✅ | ✅ | ❌ | ❌ |
| Thread listing | ✅ | ✅ | ✅ (goroutines) | ✅ | ✅ |
| Stack navigation | ✅ | ✅ | ✅ | ✅ | ✅ |
| Output capture | ✅ | ✅ | ✅ | ✅ | ✅ |
| Pause | ✅ | ✅ | ✅ | ✅ | ✅ |
| Restart | ✅ | ✅ | ✅ | ✅ | ✅ |

## Writing New Scenarios

1. Use existing fixtures from `tests/fixtures/` when possible
2. All `await` steps MUST specify `timeout: 10` (or other value)
3. Scenarios MUST end with program termination (`exited` or `terminated` reason)
4. Use BREAKPOINT_MARKERs from fixtures to identify semantic locations
5. For language-specific scenarios, set `adapter:` in target config
