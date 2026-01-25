#!/bin/bash
# Run E2E tests locally with Docker
# Usage:
#   ./scripts/run-e2e-tests.sh          - Run all tests
#   ./scripts/run-e2e-tests.sh lldb     - Run only LLDB tests
#   ./scripts/run-e2e-tests.sh delve    - Run only Delve tests
#   ./scripts/run-e2e-tests.sh debugpy  - Run only debugpy tests
#   ./scripts/run-e2e-tests.sh js-debug - Run only js-debug tests
#   ./scripts/run-e2e-tests.sh gdb      - Run only GDB tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Building debugger CLI ===${NC}"
cargo build --release

# Build base image first
echo -e "${YELLOW}=== Building base Docker image ===${NC}"
docker build -t debugger-cli:base -f docker/base/Dockerfile .

# Function to run tests for a specific adapter
run_tests() {
    local adapter="$1"
    echo -e "${YELLOW}=== Building ${adapter} Docker image ===${NC}"
    docker build -t "debugger-cli:${adapter}" -f "docker/${adapter}/Dockerfile" .

    echo -e "${YELLOW}=== Running ${adapter} tests ===${NC}"
    if docker run --rm "debugger-cli:${adapter}"; then
        echo -e "${GREEN}=== ${adapter} tests PASSED ===${NC}"
        return 0
    else
        echo -e "${RED}=== ${adapter} tests FAILED ===${NC}"
        return 1
    fi
}

# If specific adapter is requested, run only those tests
if [ -n "$1" ]; then
    run_tests "$1"
    exit $?
fi

# Run all tests
FAILED=0
TOTAL=0

for adapter in lldb delve debugpy js-debug gdb; do
    ((TOTAL++))
    if ! run_tests "$adapter"; then
        ((FAILED++))
    fi
done

echo ""
echo "=== Summary ==="
PASSED=$((TOTAL - FAILED))
echo -e "Passed: ${GREEN}${PASSED}${NC}/${TOTAL}"
if [ $FAILED -gt 0 ]; then
    echo -e "Failed: ${RED}${FAILED}${NC}/${TOTAL}"
    exit 1
fi

echo -e "${GREEN}All tests passed!${NC}"
