#!/usr/bin/env python3
"""
Tutorial 1: Basic Debugging - Fibonacci with a bug

This program calculates Fibonacci numbers but has a subtle bug
that causes incorrect results for certain inputs.
"""

def fibonacci(n: int) -> int:
    """Calculate the nth Fibonacci number."""
    if n == 0:
        return 0
    if n == 1:
        return 1

    prev = 0
    curr = 1

    # Bug: Off-by-one error - should iterate n-1 times
    for i in range(n):  # This iterates n times instead of n-1
        next_val = prev + curr
        prev = curr
        curr = next_val

    return curr


def main():
    print("Fibonacci Calculator")
    print("=" * 20)

    for i in range(15):
        result = fibonacci(i)
        print(f"fib({i:2}) = {result}")

    # Expected: fib(10) = 55, but our buggy version gives 89!
    print("\nVerification:")
    test_value = fibonacci(10)
    if test_value == 55:
        print("PASS: fib(10) = 55")
    else:
        print(f"FAIL: fib(10) = {test_value} (expected 55)")


if __name__ == "__main__":
    main()
