#!/usr/bin/env node

function add(a, b) {
    // BREAKPOINT_MARKER: add_body
    const result = a + b;
    return result;
}

function factorial(n) {
    // BREAKPOINT_MARKER: factorial_body
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

function main() {
    // BREAKPOINT_MARKER: main_start
    const x = 10;
    const y = 20;

    // BREAKPOINT_MARKER: before_add
    const sumResult = add(x, y);
    console.log(`Sum: ${sumResult}`);

    // BREAKPOINT_MARKER: before_factorial
    const fact = factorial(5);
    console.log(`Factorial: ${fact}`);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}

// BREAKPOINT_MARKER: entry_point
process.exit(main());
