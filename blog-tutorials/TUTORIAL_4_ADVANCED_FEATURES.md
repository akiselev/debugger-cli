# Tutorial 4: Advanced Debugging Features

This tutorial demonstrates advanced debugging features:
- Conditional breakpoints
- Hit-count breakpoints
- Output capture
- Expression evaluation
- Breakpoint management

## The Example: Server Request Simulation

We have a simulated web server handling 10 requests. We want to debug specific scenarios without stepping through every request.

## Conditional Breakpoints

Break only when a specific condition is true:

```bash
$ debugger start example4_server_simulation.py --stop-on-entry
Started debugging
Stopped at entry point.

$ debugger break example4_server_simulation.py:52 --condition "request.path.startswith('/admin/')"
Breakpoint 1 set at example4_server_simulation.py:52
```

This breakpoint only triggers when we're handling an admin route!

## Hit-Count Breakpoints

Break after N occurrences. Use `breakpoint add` for full options:

```bash
$ debugger breakpoint add example4_server_simulation.py:46 --hit-count 5
Breakpoint 2 set at example4_server_simulation.py:46
```

This breaks on the 5th time we hit line 46 (5th request).

## Viewing All Breakpoints

```bash
$ debugger breakpoint list
Breakpoints:
  ✓ 1 example4_server_simulation.py:52 (if request.path.startswith('/admin/'))
  ✓ 2 example4_server_simulation.py:46 (hits: 5)
```

The list shows:
- Breakpoint IDs
- Locations
- Conditions (if set)
- Hit counts (if set)

## Continue and Hit the Breakpoints

```bash
$ debugger continue
Continuing execution...

$ debugger await --timeout 10
Stopped at breakpoint
  Location: example4_server_simulation.py:46
```

## Examining State

```bash
$ debugger context
Thread 1 stopped at /home/user/debugger-cli/blog-tutorials/example4_server_simulation.py:46
In function: handle

     43 |     def handle(self, request):
     44 |         """Handle an incoming request."""
     45 |         self.request_count += 1
->   46 |         print(f"[{self.request_count}] Handling {request}")

Locals:
  request = Request(GET /admin/dashboard) (Request)
  self = <__main__.RequestHandler object at 0x7ed69dc8df50> (RequestHandler)
```

We stopped on the 5th request, which is the admin dashboard request!

## Check Request Details

```bash
$ debugger print "self.request_count"
self.request_count = 5 (int)

$ debugger print "request.user_id"
request.user_id = 'bob' (str)

$ debugger print "request.path"
request.path = '/admin/dashboard' (str)
```

## Capturing Program Output

The `output` command shows stdout/stderr captured from the program:

```bash
$ debugger output
Server Request Simulation
==================================================
[1] Handling Request(GET /index.html)
  → Response(200)

[2] Handling Request(GET /api/users)
  → Response(200)

[3] Handling Request(GET /api/user/alice)
  → Response(200)

[4] Handling Request(GET /api/user/eve)
  → Response(404)
```

This shows the first 4 requests that completed before we hit the breakpoint!

## Output Options

```bash
# Get last N lines
$ debugger output --tail 10

# Follow output continuously (like tail -f)
$ debugger output --follow

# Get all output since session started
$ debugger output
```

## Complex Expression Evaluation

Evaluate complex expressions, not just simple variables:

```bash
# Dictionary access
$ debugger print "self.users.get(request.user_id)"
self.users.get(request.user_id) = 'user' (str)

# Comparison
$ debugger print "self.users.get(request.user_id) == 'admin'"
self.users.get(request.user_id) == 'admin' = False (bool)

# List comprehension
$ debugger print "[u for u in self.users if self.users[u] == 'admin']"
[u for u in self.users if self.users[u] == 'admin'] = ['alice'] (list)
```

## Breakpoint Management

### Disable/Enable Breakpoints

```bash
$ debugger breakpoint disable 1
Breakpoint 1 disabled

$ debugger breakpoint enable 1
Breakpoint 1 enabled
```

### Remove Breakpoints

```bash
$ debugger breakpoint remove 2
Breakpoint 2 removed

$ debugger breakpoint list
Breakpoints:
  ✓ 1 example4_server_simulation.py:52 (if request.path.startswith('/admin/'))
```

## Practical Workflow: Debugging Authorization

Let's find the auth bug. Set a conditional breakpoint for failed auth:

```bash
$ debugger break example4_server_simulation.py:76 --condition "self.users.get(request.user_id) != 'admin'"
Breakpoint 3 set
```

This breaks only when a non-admin tries to access admin routes!

## Status Command

Check overall session state:

```bash
$ debugger status
Daemon: running
Session: active
Program: /home/user/debugger-cli/blog-tutorials/example4_server_simulation.py
Adapter: debugpy
State: stopped
```

## Advanced Command Summary

| Command | Description |
|---------|-------------|
| `debugger break <loc> --condition "<expr>"` | Break when expression is true |
| `debugger breakpoint add <loc> --hit-count N` | Break on Nth hit |
| `debugger breakpoint list` | Show all breakpoints |
| `debugger breakpoint disable <id>` | Temporarily disable |
| `debugger breakpoint enable <id>` | Re-enable |
| `debugger breakpoint remove <id>` | Delete breakpoint |
| `debugger output` | View program stdout/stderr |
| `debugger output --tail N` | Last N lines |
| `debugger output --follow` | Stream output live |
| `debugger print "<expr>"` | Evaluate any Python expression |
| `debugger status` | Show session state |

## When to Use Advanced Features

1. **Conditional breakpoints**: Debugging loops, filtering specific cases
2. **Hit-count breakpoints**: Skip initial iterations, catch Nth occurrence
3. **Output capture**: Debug print-heavy programs, verify logging
4. **Complex expressions**: Inspect computed values, check conditions

## Tips for LLM Agents

These advanced features are especially useful for AI coding agents:

1. **Conditional breakpoints** let agents focus on specific failure cases
2. **Hit-count breakpoints** help reproduce intermittent bugs
3. **Output capture** provides context about program state
4. **Expression evaluation** enables sophisticated state inspection

The debugger-cli's design makes it perfect for programmatic debugging workflows!
