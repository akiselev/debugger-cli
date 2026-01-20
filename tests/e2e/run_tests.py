import subprocess
import os
import sys
import time
import shutil
import tempfile

# Configuration
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEBUGGER_BIN = os.path.join(PROJECT_ROOT, "target", "release", "debugger")
TEST_DIR = os.path.join(PROJECT_ROOT, "tests", "e2e")
GCC = "gcc"
RUSTC = "rustc"
GO = "go"
PYTHON = sys.executable

def log(msg):
    print(f"[TEST] {msg}")

def compile_c(source, output):
    cmd = [GCC, "-g", "-o", output, source]
    log(f"Compiling C: {' '.join(cmd)}")
    subprocess.check_call(cmd)

def compile_rust(source, output):
    cmd = [RUSTC, "-g", "-o", output, source]
    log(f"Compiling Rust: {' '.join(cmd)}")
    subprocess.check_call(cmd)

def compile_go(source, output):
    # Disable optimizations and inlining for better debugging
    cmd = [GO, "build", "-gcflags=all=-N -l", "-o", output, source]
    log(f"Compiling Go: {' '.join(cmd)}")
    subprocess.check_call(cmd)

def setup_config():
    """Create a temporary config directory and config file"""
    config_dir = tempfile.mkdtemp(prefix="debugger-test-config-")
    app_config_dir = os.path.join(config_dir, "debugger-cli")
    os.makedirs(app_config_dir, exist_ok=True)
    
    config_path = os.path.join(app_config_dir, "config.toml")
    
    # Check where python is
    python_path = sys.executable
        
    config_content = f"""
[adapters.debugpy]
path = "{python_path}"
args = ["-m", "debugpy.adapter"]

[adapters.go]
path = "dlv"
args = ["dap"]

[defaults]
adapter = "lldb-dap"

[timeouts]
dap_initialize_secs = 10
dap_request_secs = 30
await_default_secs = 60
"""
    
    with open(config_path, "w") as f:
        f.write(config_content)
        
    return config_dir

def run_debugger_command(cmd, config_dir, input_cmds=None):
    full_cmd = [DEBUGGER_BIN] + cmd
    log(f"Running: {' '.join(full_cmd)}")
    
    env = os.environ.copy()
    if config_dir:
        env["XDG_CONFIG_HOME"] = config_dir
        # Also use a temp runtime dir to avoid conflicts
        env["XDG_RUNTIME_DIR"] = os.path.join(config_dir, "runtime")
        os.makedirs(env["XDG_RUNTIME_DIR"], exist_ok=True)
    
    process = subprocess.Popen(
        full_cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd=TEST_DIR,
        env=env
    )
    
    stdout, stderr = process.communicate(input=input_cmds)
    
    if process.returncode != 0:
        log(f"Command failed with code {process.returncode}")
        log(f"STDOUT: {stdout}")
        log(f"STDERR: {stderr}")
        return False, stdout, stderr
    
    return True, stdout, stderr

def test_program(name, source_file, compiler_func, expected_output_substr, config_dir, adapter_args=None):
    log(f"=== Testing {name} ===")
    binary_path = os.path.join(TEST_DIR, name)
    source_path = os.path.join(TEST_DIR, source_file)
    
    # 1. Compile (if compiler provided)
    if compiler_func:
        try:
            compiler_func(source_path, binary_path)
        except Exception as e:
            log(f"Compilation failed: {e}")
            return False
    else:
        # interpreted language, binary_path is just source_path
        binary_path = source_path

    # 2. Start Debugger
    
    # Stop any existing daemon to ensure clean state
    run_debugger_command(["stop"], config_dir)
    
    # Start debugging with stop-on-entry
    log("Starting debugger...")
    start_cmd = ["start", binary_path, "--stop-on-entry"]
    if adapter_args:
        start_cmd.extend(adapter_args)
        
    ok, out, err = run_debugger_command(start_cmd, config_dir)
    if not ok:
        log("Failed to start debugger")
        return False

    # Set breakpoint at main
    log("Setting breakpoint...")
    ok, out, err = run_debugger_command(["breakpoint", "add", "main"], config_dir)
    if not ok: 
        log("Failed to set breakpoint")
        return False
        
    # Continue (from entry point)
    log("Continuing execution from entry...")
    ok, out, err = run_debugger_command(["continue"], config_dir)
    if not ok:
        log(f"Failed to continue from entry: {err}")
        return False
    
    # Wait for breakpoint hit
    log("Waiting for breakpoint stop...")
    ok, out, err = run_debugger_command(["await"], config_dir)
    if not ok: return False
    log(f"Await output:\n{out}")
    
    # Check threads
    log("Checking threads...")
    ok, out_threads, err = run_debugger_command(["threads"], config_dir)
    log(f"Threads:\n{out_threads}")

    # Inspect variables
    log("Inspecting locals...")
    ok, out_locals, err = run_debugger_command(["locals"], config_dir)
    if not ok: return False
    log(f"Locals:\n{out_locals}")
    
    # Verify variable 'sum' or 'x' exists
    if "x" not in out_locals and "y" not in out_locals:
         log("WARNING: Locals x/y not found")

    # Continue to finish
    log("Continuing to finish...")
    ok, out, err = run_debugger_command(["continue"], config_dir)
    if not ok: return False

    # Wait for exit
    log("Waiting for program exit...")
    ok, out, err = run_debugger_command(["await"], config_dir)
    # Note: await might fail if session is already terminated, or return exited event.

    # Check output
    log("Checking output...")
    # Give a moment for output buffer to flush
    time.sleep(1.0)
    ok, out_prog, err = run_debugger_command(["output"], config_dir)
    log(f"Program Output:\n{out_prog}")
    
    if expected_output_substr not in out_prog:
        log(f"FAILED: Expected output '{expected_output_substr}' not found.")
        return False
        
    log(f"SUCCESS: {name} passed.")
    return True

def test_complex_python(config_dir):
    log("=== Testing Complex Python App ===")
    
    app_dir = os.path.join(PROJECT_ROOT, "tests", "complex_app")
    main_py = os.path.join(app_dir, "main.py")
    
    if not os.path.exists(main_py):
        log(f"Complex app not found at {main_py}")
        return False
        
    # Start Debugger
    run_debugger_command(["stop"], config_dir)
    log("Starting debugger on complex app...")
    
    cmd = ["start", main_py, "--adapter", "debugpy", "--stop-on-entry"]
    ok, out, err = run_debugger_command(cmd, config_dir)
    if not ok: return False
    
    # We will run a script of commands to exercise various features
    commands = [
        "break scenarios.py:7",  # recursion_step
        "break scenarios.py:34", # thread_work
        "break scenarios.py:59", # catch_exception
        "continue",              # Hit stop-on-entry
        "continue",              # Should hit recursion_step
        "bt --limit 5",          # Check backtrace
        "locals",                # Check locals
        "break remove --all",    # Clear breakpoints
        "continue",              # Finish recursion, should hit catch_exception (exception test runs after recursion)
        "continue",              # Finish exception, run large data
        "continue",              # Finish large data, run threads (should hit thread_work)
        "threads",               # List threads
        "continue",              # Continue thread 1
        "continue",              # Continue thread 2 (if it hits) or finish
        "await"                  # Wait for exit
    ]
    
    # We need to send these commands interactively or batch them. 
    # The current run_debugger_command helper sends input all at once if provided, 
    # but the debugger might not be ready for all of them.
    # However, since we are using `subprocess.communicate`, it sends all input and close stdin.
    # The debugger CLI reads from stdin. If it processes commands sequentially, this might work 
    # provided it doesn't exit early.
    
    # A better approach for this test helper might be to just run the sequence.
    # But since we need to verify output at steps, implementing a full interactive drive is complex 
    # in this simple runner.
    # Let's try to just run it and see if we get the expected "Complex App Finished" output
    # and maybe some intermediate logs in stdout.
    
    # For now, let's just set breakpoints and continue until finish, checking final output.
    
    input_script = "\n".join([
        "break scenarios.py:8",  # recursion_step (verify line numbers match file)
        "continue", # from entry
        "continue", # hit recursion
        "bt 5",
        "locals",
        "break remove --all",
        "continue", # finish all
        "await",
        "output"
    ])
    
    ok, out, err = run_debugger_command([], config_dir, input_cmds=input_script)
    
    # Check if we see expected things in the output
    if "Complex App Finished" not in out:
        log("FAILED: Did not see 'Complex App Finished'")
        log(f"Output: {out}")
        return False
        
    log("SUCCESS: Complex Python App passed basic run.")
    return True

def check_debugpy():
    try:
        subprocess.check_call([PYTHON, "-c", "import debugpy"])
        return True
    except subprocess.CalledProcessError:
        return False

def check_delve():
    """Check if Delve (dlv) is available"""
    try:
        subprocess.check_call(["dlv", "version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False

def main():
    if not os.path.exists(DEBUGGER_BIN):
        log(f"Debugger binary not found at {DEBUGGER_BIN}. Please build release first.")
        sys.exit(1)

    config_dir = setup_config()
    log(f"Created temp config dir at {config_dir}")

    failed = False
    
    try:
        # Test C
        if not test_program("test_c", "hello_world.c", compile_c, "Hello from C! Sum is 30", config_dir):
            failed = True
            
        # Test Rust
        if not test_program("test_rs", "hello_world.rs", compile_rust, "Hello from Rust! Sum is 30", config_dir):
            failed = True
            
        # Test Python
        if check_debugpy():
            # For Python, we use the source file directly and specify debugpy adapter
            # Note: hello_world.py needs to exist. We can reuse simple.py or copy it.
            # Using fixtures/simple.py for now as hello_world.py might not be there.
            
            # First, check if hello_world.py exists, if not create it
            py_test_file = os.path.join(TEST_DIR, "hello_world.py")
            if not os.path.exists(py_test_file):
                with open(py_test_file, "w") as f:
                    f.write("""
import sys

def main():
    x = 10
    y = 20
    print(f"Hello from Python! Sum is {x+y}")
    return 0

if __name__ == "__main__":
    sys.exit(main())
""")
            
            if not test_program("test_py", "hello_world.py", None, "Hello from Python! Sum is 30", config_dir, ["--adapter", "debugpy"]):
                failed = True
        else:
            log("Skipping Python test (debugpy not found)")

        # Test Go
        if check_delve():
            # First, check if hello_world.go exists
            go_test_file = os.path.join(TEST_DIR, "hello_world.go")
            if os.path.exists(go_test_file):
                if not test_program("test_go", "hello_world.go", compile_go, "Hello from Go! Sum is 30", config_dir, ["--adapter", "go"]):
                    failed = True
            else:
                log("Skipping Go test (hello_world.go not found)")
        else:
            log("Skipping Go test (dlv not found)")

        # Test Complex Python
        if check_debugpy():
             if not test_complex_python(config_dir):
                 failed = True

    finally:
        # Cleanup
        shutil.rmtree(config_dir)

    if failed:
        sys.exit(1)
    else:
        log("All tests passed!")
        sys.exit(0)

if __name__ == "__main__":
    main()
