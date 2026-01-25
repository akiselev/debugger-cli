// TypeScript Hello World for E2E debugging tests

function main(): number {
    const x: number = 10;  // BREAKPOINT_MARKER: after_x_init
    const y: number = 20;  // BREAKPOINT_MARKER: after_y_init
    const sum: number = x + y;  // BREAKPOINT_MARKER: after_sum

    console.log(`Hello from TypeScript! Sum is ${sum}`);

    return 0;
}

// Call main and exit with its return code
const exitCode: number = main();
process.exit(exitCode);
