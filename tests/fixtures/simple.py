#!/usr/bin/env python3
"""Simple test program for debugger integration tests"""

import sys

def add(a: int, b: int) -> int:
    # BREAKPOINT_MARKER: add_body
    result = a + b
    return result

def factorial(n: int) -> int:
    # BREAKPOINT_MARKER: factorial_body
    if n <= 1:
        return 1
    return n * factorial(n - 1)

def main():
    # BREAKPOINT_MARKER: main_start
    x = 10
    y = 20

    # BREAKPOINT_MARKER: before_add
    sum_result = add(x, y)
    print(f"Sum: {sum_result}")

    # BREAKPOINT_MARKER: before_factorial
    fact = factorial(5)
    print(f"Factorial: {fact}")

    # BREAKPOINT_MARKER: before_exit
    return 0

if __name__ == "__main__":
    # BREAKPOINT_MARKER: entry_point
    sys.exit(main())
