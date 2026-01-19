// Simple test program for debugger integration tests

fn add(a: i32, b: i32) -> i32 {
    // BREAKPOINT_MARKER: add_body
    let result = a + b;
    result
}

fn factorial(n: i32) -> i32 {
    // BREAKPOINT_MARKER: factorial_body
    if n <= 1 {
        return 1;
    }
    n * factorial(n - 1)
}

fn main() {
    // BREAKPOINT_MARKER: main_start
    let x = 10;
    let y = 20;

    // BREAKPOINT_MARKER: before_add
    let sum = add(x, y);
    println!("Sum: {}", sum);

    // BREAKPOINT_MARKER: before_factorial
    let fact = factorial(5);
    println!("Factorial: {}", fact);

    // BREAKPOINT_MARKER: before_exit
}
