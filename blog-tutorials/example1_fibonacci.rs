// Tutorial 1: Basic Debugging - Fibonacci with a bug
//
// This program calculates Fibonacci numbers but has a subtle bug
// that causes incorrect results for certain inputs.

fn fibonacci(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }

    let mut prev = 0u64;
    let mut curr = 1u64;

    // Bug: Off-by-one error - should iterate n-1 times
    for _ in 0..n {  // This iterates n times instead of n-1
        let next = prev + curr;
        prev = curr;
        curr = next;
    }

    curr
}

fn main() {
    println!("Fibonacci Calculator");
    println!("====================");

    for i in 0..15 {
        let result = fibonacci(i);
        println!("fib({:2}) = {}", i, result);
    }

    // Expected: fib(10) = 55, but our buggy version gives 89!
    println!("\nVerification:");
    let test_value = fibonacci(10);
    if test_value == 55 {
        println!("PASS: fib(10) = 55");
    } else {
        println!("FAIL: fib(10) = {} (expected 55)", test_value);
    }
}
