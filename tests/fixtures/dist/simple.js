#!/usr/bin/env node
function add(a, b) {
    // BREAKPOINT_MARKER: add_body
    var result = a + b;
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
    var x = 10;
    var y = 20;
    // BREAKPOINT_MARKER: before_add
    var sumResult = add(x, y);
    console.log("Sum: ".concat(sumResult));
    // BREAKPOINT_MARKER: before_factorial
    var fact = factorial(5);
    console.log("Factorial: ".concat(fact));
    // BREAKPOINT_MARKER: before_exit
    return 0;
}
// BREAKPOINT_MARKER: entry_point
process.exit(main());
//# sourceMappingURL=simple.js.map