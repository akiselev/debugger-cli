// Multi-source project main program
#include <stdio.h>
#include "utils.h"

int main() {
    // BREAKPOINT_MARKER: main_start
    int x = 5;
    int y = 10;

    // BREAKPOINT_MARKER: before_helper_call
    int sum = helper_add(x, y);
    printf("Sum: %d\n", sum);

    int product = helper_multiply(x, y);
    printf("Product: %d\n", product);

    // BREAKPOINT_MARKER: before_exit
    return 0;
}
