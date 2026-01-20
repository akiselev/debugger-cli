// Simple test program for debugger integration tests
package main

import "fmt"

func add(a, b int) int {
	// BREAKPOINT_MARKER: add_body
	result := a + b
	return result
}

func factorial(n int) int {
	// BREAKPOINT_MARKER: factorial_body
	if n <= 1 {
		return 1
	}
	return n * factorial(n-1)
}

func main() {
	// BREAKPOINT_MARKER: main_start
	x := 10
	y := 20

	// BREAKPOINT_MARKER: before_add
	sum := add(x, y)
	fmt.Printf("Sum: %d\n", sum)

	// BREAKPOINT_MARKER: before_factorial
	fact := factorial(5)
	fmt.Printf("Factorial: %d\n", fact)

	// BREAKPOINT_MARKER: before_exit
}
