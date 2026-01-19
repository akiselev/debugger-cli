// Simple test program for debugger integration tests
// BREAKPOINT_MARKER: main_start (line after this)
#include <stdio.h>

int add(int a, int b) {
    // BREAKPOINT_MARKER: add_body
    int result = a + b;
    return result;
}

int factorial(int n) {
    // BREAKPOINT_MARKER: factorial_body
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

int main(int argc, char *argv[]) {
    // BREAKPOINT_MARKER: main_start
    int x = 10;
    int y = 20;

    // BREAKPOINT_MARKER: before_add
    int sum = add(x, y);
    printf("Sum: %d\n", sum);

    // BREAKPOINT_MARKER: before_factorial
    int fact = factorial(5);
    printf("Factorial: %d\n", fact);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}
