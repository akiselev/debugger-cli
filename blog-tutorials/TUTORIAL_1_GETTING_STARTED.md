# Tutorial 1: Getting Started with debugger-cli

This tutorial walks you through debugging a buggy Fibonacci implementation using debugger-cli.

## The Bug

We have a simple Fibonacci calculator that's producing wrong results:

```python
def fibonacci(n: int) -> int:
    if n == 0:
        return 0
    if n == 1:
        return 1

    prev = 0
    curr = 1

    for i in range(n):  # Bug: should be range(n-1)
        next_val = prev + curr
        prev = curr
        curr = next_val

    return curr
```

When we run it, we get:
```
fib(10) = 89 (expected 55)
```

Let's use debugger-cli to find and understand the bug!

## Step 1: Start Debugging

First, start the program with `--stop-on-entry` to pause at the beginning:

```bash
$ debugger start example1_fibonacci.py --stop-on-entry
Started debugging: /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py
Stopped at entry point. Use 'debugger continue' to run.
```

## Step 2: Check Context

The `context` command shows where we are and what variables are in scope:

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:2
In function: <module>

      1 | #!/usr/bin/env python3
->    2 | """
      3 | Tutorial 1: Basic Debugging - Fibonacci with a bug
      4 |
      5 | This program calculates Fibonacci numbers but has a subtle bug
      6 | that causes incorrect results for certain inputs.
      7 | """

Locals:
  special variables =  ()
```

## Step 3: Set a Breakpoint

Let's set a breakpoint inside the fibonacci function's loop:

```bash
$ debugger break example1_fibonacci.py:20
Breakpoint 1 set at example1_fibonacci.py:20
```

## Step 4: Continue to Breakpoint

Now run until we hit the breakpoint:

```bash
$ debugger continue
Continuing execution...

$ debugger await --timeout 5
Waiting for program to stop (timeout: 5s)...
Stopped at breakpoint
  Location: example1_fibonacci.py:20
```

## Step 5: Inspect Variables

The `context` command now shows us inside the fibonacci function:

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:20
In function: fibonacci

     15 |
     16 |     prev = 0
     17 |     curr = 1
     18 |
     19 |     # Bug: Off-by-one error - should iterate n-1 times
->   20 |     for i in range(n):  # This iterates n times instead of n-1
     21 |         next_val = prev + curr
     22 |         prev = curr
     23 |         curr = next_val
     24 |
     25 |     return curr

Locals:
  curr = 1 (int)
  n = 2 (int)
  prev = 0 (int)
```

We can see:
- We're calculating `fibonacci(2)`
- `prev = 0`, `curr = 1`
- We're about to enter a loop that runs `n` times (2 times)

## Step 6: Step Through the Loop

Let's step through the loop to see what happens:

```bash
$ debugger step
Stepping into...

$ debugger await --timeout 5
Step completed

$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:21
In function: fibonacci

->   21 |         next_val = prev + curr

Locals:
  curr = 1 (int)
  i = 0 (int)
  n = 2 (int)
  prev = 0 (int)
```

First iteration: `i = 0`, `prev = 0`, `curr = 1`

## Step 7: Evaluate Expressions

We can evaluate expressions to check our logic:

```bash
$ debugger print "prev + curr"
prev + curr = 1 (int)
```

## Step 8: View the Call Stack

The `backtrace` command shows how we got here:

```bash
$ debugger backtrace
#0 fibonacci at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:21
#1 main at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:33
#2 <module> at /home/user/debugger-cli/blog-tutorials/example1_fibonacci.py:46
```

## Step 9: Finding the Bug

By stepping through, we can trace the values:

For `fibonacci(2)`:
- Initial: prev=0, curr=1
- After loop iteration 0: prev=1, curr=1
- After loop iteration 1: prev=1, curr=2

But `fibonacci(2)` should be `1`, not `2`!

The bug is that the loop runs `n` times instead of `n-1` times. For `fibonacci(2)`:
- It should only do 1 iteration (2-1=1)
- But it's doing 2 iterations

## Step 10: Stop the Session

When done debugging:

```bash
$ debugger stop
Debug session stopped
```

## The Fix

Change `range(n)` to `range(n-1)`:

```python
for i in range(n-1):  # Fixed: iterate n-1 times
    next_val = prev + curr
    prev = curr
    curr = next_val
```

## Key Commands Summary

| Command | Description |
|---------|-------------|
| `debugger start <program>` | Start debugging |
| `debugger start <program> --stop-on-entry` | Start and pause at entry |
| `debugger break <file>:<line>` | Set a breakpoint |
| `debugger continue` | Resume execution |
| `debugger await` | Wait for program to stop |
| `debugger context` | Show source and locals |
| `debugger step` | Step into next line |
| `debugger next` | Step over (skip function calls) |
| `debugger print <expr>` | Evaluate expression |
| `debugger backtrace` | Show call stack |
| `debugger stop` | End debug session |

## Next Steps

- Try Tutorial 2: Debugging the Debugger (meta debugging!)
- Try Tutorial 3: Finding and Fixing a Real Bug
- Try Tutorial 4: Advanced Features (conditional breakpoints, etc.)
