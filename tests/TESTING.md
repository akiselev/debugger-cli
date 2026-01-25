# Testing Guide for Contributors

This guide covers how to write, run, and maintain tests for the debugger CLI.

## Quick Start

```bash
# Run a single test scenario
debugger test tests/scenarios/hello_world_c.yml

# Run with verbose output
debugger test tests/scenarios/hello_world_c.yml --verbose

# Run with a specific adapter
debugger test tests/scenarios/hello_world_c.yml --adapter gdb
```

## Test Architecture

The test framework uses three components:

1. **YAML Scenarios** (`tests/scenarios/*.yml`) - Define test workflows
2. **Test Fixtures** (`tests/fixtures/`) - Programs to debug
3. **Test Runner** (`src/testing/runner.rs`) - Executes scenarios via daemon

```
YAML Scenario
     |
     v
run_scenario() [runner.rs]
     |
     +-- Setup steps (shell commands: compilation)
     |
     +-- Start/Attach debug session
     |
     +-- Execute test steps (commands, assertions)
     |
     v
TestResult (pass/fail)
```

## Adding a New Test Scenario

### Step 1: Choose or Create a Fixture

Use existing fixtures when possible:
- `simple.c` / `simple.go` / `simple.js` / `simple.py` - Basic debugging
- `threaded.c` / `threaded.go` - Multi-threaded programs

If you need a new fixture, add it to `tests/fixtures/` with BREAKPOINT_MARKERs (see below).

### Step 2: Create the YAML Scenario

Create `tests/scenarios/<feature>_<language>.yml`:

```yaml
name: "Feature Test Name"
description: "What this test verifies"

setup:
  # Compile the fixture (if needed)
  - shell: "gcc -g tests/fixtures/simple.c -o tests/fixtures/test_simple_c"

target:
  program: "tests/fixtures/test_simple_c"
  adapter: "lldb"  # lldb, gdb, go, python, js-debug
  stop_on_entry: true

steps:
  # Set a breakpoint
  - action: command
    command: "break simple.c:19"
    expect:
      success: true

  # Continue to breakpoint
  - action: command
    command: "continue"

  # Wait for stop event
  - action: await
    timeout: 10
    expect:
      reason: "breakpoint"
      file: "simple.c"
      line: 19

  # Inspect variables
  - action: inspect_locals
    asserts:
      - name: "x"
        value: "10"

  # Continue to exit
  - action: command
    command: "continue"

  - action: await
    timeout: 10
    expect:
      reason: "exited"
```

### Step 3: Test Locally

```bash
debugger test tests/scenarios/your_new_test.yml --verbose
```

### Step 4: Add to CI

Add the test to `.github/workflows/e2e-tests.yml` under the appropriate adapter job.

## Step Types Reference

| Step Type | Purpose | Key Fields |
|-----------|---------|------------|
| `command` | Execute debugger command | `command`, `expect.success` |
| `await` | Wait for stop event | `timeout`, `expect.reason/file/line` |
| `inspect_locals` | Check local variables | `asserts[].name/value/value_contains/type` |
| `inspect_stack` | Check call stack | `asserts[].index/function/file/line` |
| `check_output` | Check program output | `contains`, `equals` |
| `evaluate` | Evaluate expression | `expression`, `expect.result/result_contains` |

## BREAKPOINT_MARKER Convention

Fixtures use semantic markers for reliable breakpoint locations:

```c
// BREAKPOINT_MARKER: main_start
int x = 10;

// BREAKPOINT_MARKER: before_add
int sum = add(x, y);
```

Tests reference these markers by name or the line number after the marker.

## Common Pitfalls

### 1. Missing Timeout on Await Steps

**Bad:**
```yaml
- action: await
  expect:
    reason: "breakpoint"
```

**Good:**
```yaml
- action: await
  timeout: 10  # Always specify timeout!
  expect:
    reason: "breakpoint"
```

### 2. Breaking Before pthread_barrier_wait

In `threaded.c`, do NOT set breakpoints before `pthread_barrier_wait()` - this causes deadlocks. Use the `worker_body` function marker instead.

### 3. Forgetting Program Termination

Every scenario MUST end with program termination:
```yaml
- action: await
  timeout: 10
  expect:
    reason: "exited"  # or "terminated"
```

### 4. Hardcoded Line Numbers

Prefer semantic locations over hardcoded line numbers:
```yaml
# Fragile - breaks if code changes
command: "break simple.c:42"

# Better - use function names
command: "break main"
command: "break add"
```

## Adding Support for a New Language

1. **Create fixture**: `tests/fixtures/simple.<ext>`
   - Add BREAKPOINT_MARKERs at key locations
   - Include basic functions (add, factorial, main)

2. **Create hello_world scenario**: `tests/scenarios/hello_world_<lang>.yml`
   - Test basic debugging workflow
   - Set breakpoint, continue, inspect locals, exit

3. **Add compilation to CI**: `.github/workflows/e2e-tests.yml`
   - Add fixture compilation step
   - Add adapter-specific test job if needed

4. **Update documentation**:
   - `tests/fixtures/README.md` - Document new fixture
   - `tests/scenarios/README.md` - Add to adapter mapping

## Adapter Compatibility Matrix

Not all features work with all adapters:

| Feature | LLDB | GDB | Delve | debugpy | js-debug |
|---------|------|-----|-------|---------|----------|
| Conditional breakpoints | ✅ | ✅ | ✅ | ✅ | ✅ |
| Hit count breakpoints | ✅ | ✅ | ✅ | ❌ | ❌ |
| Thread listing | ✅ | ✅ | ✅ | ✅ | ✅ |
| Stack navigation | ✅ | ✅ | ✅ | ✅ | ✅ |
| Output capture | ✅ | ✅ | ✅ | ✅ | ✅ |
| Pause/Resume | ✅ | ✅ | ✅ | ✅ | ✅ |

## Running Tests in CI

Tests run automatically on push/PR via GitHub Actions:
- Matrix: 5 adapters × 2 platforms
- Tests have automatic retry (3 attempts) for flaky test handling
- Failed test logs uploaded as artifacts

To debug CI failures:
1. Check the job logs for error messages
2. Download log artifacts from the failed run
3. Reproduce locally with `--verbose` flag
