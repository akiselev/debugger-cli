#!/usr/bin/env node

function add(a: number, b: number): number {
    // BREAKPOINT_MARKER: add_body
    const result: number = a + b;
    return result;
}

function factorial(n: number): number {
    // BREAKPOINT_MARKER: factorial_body
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

function multiply(a: number, b: number): number {
    // BREAKPOINT_MARKER: multiply_body
    return a * b;
}

function main(): number {
    // BREAKPOINT_MARKER: main_start
    const x: number = 10;
    const y: number = 20;
    const message: string = "hello";
    const obj: { name: string; value: number } = { name: "test", value: 42 };
    const arr: number[] = [1, 2, 3, 4, 5];

    // BREAKPOINT_MARKER: before_add
    const sumResult: number = add(x, y);
    console.log(`Sum: ${sumResult}`);

    // BREAKPOINT_MARKER: before_multiply
    const product: number = multiply(x, y);
    console.log(`Product: ${product}`);

    // BREAKPOINT_MARKER: before_factorial
    const fact: number = factorial(5);
    console.log(`Factorial: ${fact}`);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}

// BREAKPOINT_MARKER: entry_point
process.exit(main());
