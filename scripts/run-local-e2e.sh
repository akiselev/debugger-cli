#!/bin/bash
# Run E2E tests locally without Docker
# Requires: the debug adapters to be installed on your system
#
# Usage:
#   ./scripts/run-local-e2e.sh          - Run all available tests
#   ./scripts/run-local-e2e.sh lldb     - Run only LLDB tests
#   ./scripts/run-local-e2e.sh delve    - Run only Delve tests
#   ./scripts/run-local-e2e.sh debugpy  - Run only debugpy tests
#   ./scripts/run-local-e2e.sh js-debug - Run only js-debug tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Build the debugger
echo -e "${YELLOW}=== Building debugger CLI ===${NC}"
cargo build --release
DEBUGGER="./target/release/debugger"

# Check which adapters are available
check_adapter() {
    local name="$1"
    case "$name" in
        lldb)
            which lldb-dap >/dev/null 2>&1 || which lldb-vscode >/dev/null 2>&1
            ;;
        delve)
            which dlv >/dev/null 2>&1
            ;;
        debugpy)
            python3 -c "import debugpy" 2>/dev/null
            ;;
        js-debug)
            # Check if js-debug is installed via our setup
            [ -d ~/.local/share/debugger/adapters/js-debug ] || \
            $DEBUGGER setup js-debug --check >/dev/null 2>&1
            ;;
        gdb)
            which gdb >/dev/null 2>&1
            ;;
    esac
}

# Compile test fixtures
compile_fixtures() {
    echo -e "${BLUE}=== Compiling test fixtures ===${NC}"

    # C
    if which gcc >/dev/null 2>&1; then
        gcc -g tests/fixtures/simple.c -o tests/fixtures/test_simple_c 2>/dev/null || true
        gcc -g tests/e2e/hello_world.c -o tests/e2e/test_c 2>/dev/null || true
    fi

    # Rust
    if which rustc >/dev/null 2>&1; then
        rustc -g tests/fixtures/simple.rs -o tests/fixtures/test_simple_rs 2>/dev/null || true
        rustc -g tests/e2e/hello_world.rs -o tests/e2e/test_rs 2>/dev/null || true
    fi

    # Go
    if which go >/dev/null 2>&1; then
        go build -gcflags='all=-N -l' -o tests/e2e/test_go tests/e2e/hello_world.go 2>/dev/null || true
        go build -gcflags='all=-N -l' -o tests/fixtures/test_simple_go tests/fixtures/simple.go 2>/dev/null || true
    fi

    # TypeScript
    if which npx >/dev/null 2>&1; then
        (cd tests/fixtures && npm install 2>/dev/null && npx tsc 2>/dev/null) || true
    fi
}

# Run tests for an adapter
run_adapter_tests() {
    local adapter="$1"
    local passed=0
    local failed=0

    echo -e "${YELLOW}=== Running $adapter tests ===${NC}"

    case "$adapter" in
        lldb)
            for scenario in hello_world_c hello_world_rust complex_verification; do
                if [ -f "tests/scenarios/${scenario}.yml" ]; then
                    echo -e "${BLUE}  Running ${scenario}...${NC}"
                    if $DEBUGGER test "tests/scenarios/${scenario}.yml" --verbose; then
                        ((passed++))
                        echo -e "${GREEN}  ✓ ${scenario}${NC}"
                    else
                        ((failed++))
                        echo -e "${RED}  ✗ ${scenario}${NC}"
                    fi
                fi
            done
            ;;
        delve)
            for scenario in hello_world_go complex_go; do
                if [ -f "tests/scenarios/${scenario}.yml" ]; then
                    echo -e "${BLUE}  Running ${scenario}...${NC}"
                    if $DEBUGGER test "tests/scenarios/${scenario}.yml" --verbose; then
                        ((passed++))
                        echo -e "${GREEN}  ✓ ${scenario}${NC}"
                    else
                        ((failed++))
                        echo -e "${RED}  ✗ ${scenario}${NC}"
                    fi
                fi
            done
            ;;
        debugpy)
            for scenario in hello_world_python; do
                if [ -f "tests/scenarios/${scenario}.yml" ]; then
                    echo -e "${BLUE}  Running ${scenario}...${NC}"
                    if $DEBUGGER test "tests/scenarios/${scenario}.yml" --verbose; then
                        ((passed++))
                        echo -e "${GREEN}  ✓ ${scenario}${NC}"
                    else
                        ((failed++))
                        echo -e "${RED}  ✗ ${scenario}${NC}"
                    fi
                fi
            done
            ;;
        js-debug)
            # Setup js-debug if not installed
            $DEBUGGER setup js-debug 2>/dev/null || true

            for scenario in hello_world_js hello_world_ts stepping_js expression_eval_js; do
                if [ -f "tests/scenarios/${scenario}.yml" ]; then
                    echo -e "${BLUE}  Running ${scenario}...${NC}"
                    if $DEBUGGER test "tests/scenarios/${scenario}.yml" --verbose; then
                        ((passed++))
                        echo -e "${GREEN}  ✓ ${scenario}${NC}"
                    else
                        ((failed++))
                        echo -e "${RED}  ✗ ${scenario}${NC}"
                    fi
                fi
            done
            ;;
        gdb)
            echo -e "${BLUE}  Running hello_world_c with GDB...${NC}"
            if $DEBUGGER test tests/scenarios/hello_world_c.yml --adapter gdb --verbose; then
                ((passed++))
                echo -e "${GREEN}  ✓ hello_world_c (gdb)${NC}"
            else
                ((failed++))
                echo -e "${RED}  ✗ hello_world_c (gdb)${NC}"
            fi
            ;;
    esac

    # Cleanup daemon
    pkill -f "debugger daemon" 2>/dev/null || true

    echo -e "  ${adapter}: ${GREEN}${passed} passed${NC}, ${RED}${failed} failed${NC}"
    return $failed
}

# Main
compile_fixtures

if [ -n "$1" ]; then
    # Run specific adapter
    if check_adapter "$1"; then
        run_adapter_tests "$1"
        exit $?
    else
        echo -e "${RED}Adapter $1 is not available on this system${NC}"
        exit 1
    fi
fi

# Run all available adapters
TOTAL_PASSED=0
TOTAL_FAILED=0
ADAPTERS_RUN=0

for adapter in lldb delve debugpy js-debug gdb; do
    if check_adapter "$adapter"; then
        ((ADAPTERS_RUN++))
        if run_adapter_tests "$adapter"; then
            ((TOTAL_PASSED++))
        else
            ((TOTAL_FAILED++))
        fi
        echo ""
    else
        echo -e "${YELLOW}Skipping $adapter (not available)${NC}"
    fi
done

echo ""
echo "=== Summary ==="
echo -e "Adapters tested: ${ADAPTERS_RUN}"
echo -e "Passed: ${GREEN}${TOTAL_PASSED}${NC}"
echo -e "Failed: ${RED}${TOTAL_FAILED}${NC}"

if [ $TOTAL_FAILED -gt 0 ]; then
    exit 1
fi
