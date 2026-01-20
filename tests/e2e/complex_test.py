#!/usr/bin/env python3
"""Complex test script for debugger testing."""

import sys
from dataclasses import dataclass
from typing import List, Optional


@dataclass
class Person:
    name: str
    age: int
    email: Optional[str] = None

    def greet(self) -> str:
        return f"Hello, I'm {self.name}!"

    def is_adult(self) -> bool:
        return self.age >= 18


class Calculator:
    def __init__(self):
        self.history: List[str] = []

    def add(self, a: int, b: int) -> int:
        result = a + b
        self.history.append(f"{a} + {b} = {result}")
        return result

    def multiply(self, a: int, b: int) -> int:
        result = a * b
        self.history.append(f"{a} * {b} = {result}")
        return result

    def factorial(self, n: int) -> int:
        """Recursive factorial for testing step-into."""
        if n <= 1:
            return 1
        return n * self.factorial(n - 1)


def fibonacci(n: int) -> int:
    """Iterative fibonacci for testing loops."""
    if n <= 1:
        return n
    a, b = 0, 1
    for i in range(2, n + 1):
        a, b = b, a + b
    return b


def process_people(people: List[Person]) -> dict:
    """Process a list of people."""
    adults = []
    minors = []
    
    for person in people:
        if person.is_adult():
            adults.append(person.name)
        else:
            minors.append(person.name)
    
    return {
        "adults": adults,
        "minors": minors,
        "total": len(people)
    }


def main():
    print("=== Complex Debug Test ===")
    
    # Test 1: Simple variables
    x = 42
    y = 3.14159
    message = "Hello, debugger!"
    items = [1, 2, 3, 4, 5]
    
    print(f"x={x}, y={y}")
    print(f"message: {message}")
    print(f"items: {items}")
    
    # Test 2: Calculator with history
    calc = Calculator()
    sum_result = calc.add(10, 20)
    product = calc.multiply(5, 6)
    fact_5 = calc.factorial(5)
    
    print(f"10 + 20 = {sum_result}")
    print(f"5 * 6 = {product}")
    print(f"5! = {fact_5}")
    print(f"Calculator history: {calc.history}")
    
    # Test 3: Fibonacci sequence
    fib_results = []
    for i in range(10):
        fib_results.append(fibonacci(i))
    print(f"Fibonacci(0-9): {fib_results}")
    
    # Test 4: People processing
    people = [
        Person("Alice", 30, "alice@example.com"),
        Person("Bob", 17),
        Person("Charlie", 25, "charlie@example.com"),
        Person("Diana", 15),
        Person("Eve", 42),
    ]
    
    result = process_people(people)
    print(f"Adults: {result['adults']}")
    print(f"Minors: {result['minors']}")
    print(f"Total: {result['total']}")
    
    # Test 5: Nested data structure
    nested = {
        "level1": {
            "level2": {
                "level3": {
                    "value": 42,
                    "list": [1, 2, 3]
                }
            }
        }
    }
    deep_value = nested["level1"]["level2"]["level3"]["value"]
    print(f"Deep value: {deep_value}")
    
    print("=== Test Complete ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
