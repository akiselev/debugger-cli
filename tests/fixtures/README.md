# Test Fixtures

Test fixtures are minimal programs designed for debugging integration tests. Each fixture contains BREAKPOINT_MARKERs - semantic locations where tests can reliably set breakpoints.

## Fixture Files

### simple.c / simple.go / simple.js / simple.py

Single-threaded programs with basic computation. Used for testing breakpoints, stepping, variable inspection, and output capture.

**Functions:**
- `add(a, b)` - Simple addition
- `factorial(n)` - Recursive factorial (n=5)
- `main` - Calls both functions, prints output

**BREAKPOINT_MARKERs:**
- `main_start` - Entry point of main function
- `before_add` - Immediately before add() call
- `add_body` - Inside add() function
- `before_factorial` - Immediately before factorial() call
- `factorial_body` - Inside factorial() function (recursive)
- `before_exit` - Final marker before program exits

**Output:**
```
Sum: 30
Factorial: 120
```

### threaded.c / threaded.go

Multithreaded programs with synchronization. Used for testing thread listing and thread-safe debugging.

**C (threaded.c):**
- 2 worker threads using pthreads
- Portable barrier (mutex + condvar) synchronizes main + workers (3 threads total)
- Works on both Linux and macOS (macOS lacks pthread_barrier_t)
- Shared counter protected by mutex
- `worker_body(thread_id)` - Helper function called AFTER barrier (safe breakpoint target)

**Go (threaded.go):**
- 2 worker goroutines
- Buffered channel provides deterministic start ordering
- Shared counter protected by sync.Mutex

**BREAKPOINT_MARKERs:**
- `main_start` - Entry point of main function
- `main_wait` - Main thread waiting at barrier/channel
- `thread_entry` - Worker thread entry (C: BEFORE barrier, Go: before channel receive)
- `after_barrier` - C only: SAFE breakpoint after barrier synchronization
- `worker_body` - C only: Helper function after barrier (recommended breakpoint)
- `worker_start` - Worker begins critical section
- `worker_end` - Worker exits

**C Threading Deadlock Warning:**

Breaking at `thread_entry` or `thread_func` (before barrier) causes deadlock. The debugger stops one thread while the barrier waits for all 3 threads (main + 2 workers) to synchronize. Use `worker_body` function (recommended) or `after_barrier` line marker instead.

**Output:**
```
Starting 2 worker threads
Thread 0 incremented counter to 1
Thread 1 incremented counter to 2
Final counter value: 2
```

(Note: Thread output order is non-deterministic)

## BREAKPOINT_MARKER Convention

BREAKPOINT_MARKERs are comments marking semantic locations:

```c
// BREAKPOINT_MARKER: main_start
int x = 10;
```

```go
// BREAKPOINT_MARKER: add_body
result := a + b
```

```javascript
// BREAKPOINT_MARKER: before_factorial
const fact = factorial(5);
```

Tests reference these markers by function name or line number. Markers ensure breakpoints hit meaningful locations even if code changes slightly.

## Compilation

Fixtures compile with debug symbols:

```bash
# C
gcc -g tests/fixtures/simple.c -o tests/fixtures/test_simple_c
gcc -g -pthread tests/fixtures/threaded.c -o tests/fixtures/test_threaded_c

# Go
go build -gcflags='all=-N -l' -o tests/fixtures/test_simple_go tests/fixtures/simple.go
go build -gcflags='all=-N -l' -o tests/fixtures/test_threaded_go tests/fixtures/threaded.go

# JavaScript/TypeScript (no compilation needed)
node tests/fixtures/simple.js

# Python (no compilation needed)
python3 tests/fixtures/simple.py
```

Compilation commands are included in scenario `setup:` steps.
