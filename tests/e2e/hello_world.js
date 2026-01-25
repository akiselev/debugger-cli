// JavaScript Hello World for E2E debugging tests

function main() {
    const x = 10;  // BREAKPOINT_MARKER: after_x_init
    const y = 20;  // BREAKPOINT_MARKER: after_y_init
    const sum = x + y;  // BREAKPOINT_MARKER: after_sum

    console.log(`Hello from JavaScript! Sum is ${sum}`);

    return 0;
}

// Call main and exit with its return code
const exitCode = main();
process.exit(exitCode);
