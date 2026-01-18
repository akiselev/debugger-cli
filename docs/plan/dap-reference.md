# DAP Protocol Quick Reference

## Wire Protocol

All DAP messages use this format:

```
Content-Length: <byte-length>\r\n
\r\n
<JSON body>
```

Example:
```
Content-Length: 119\r\n
\r\n
{"seq":1,"type":"request","command":"initialize","arguments":{"clientId":"debugger-cli","adapterId":"lldb-dap"}}
```

## Message Structure

### Base Message

```json
{
  "seq": 1,           // Sequence number (incrementing)
  "type": "request"   // "request", "response", or "event"
}
```

### Request

```json
{
  "seq": 1,
  "type": "request",
  "command": "initialize",
  "arguments": { /* command-specific */ }
}
```

### Response

```json
{
  "seq": 2,
  "type": "response",
  "request_seq": 1,        // References the request
  "success": true,
  "command": "initialize",
  "body": { /* command-specific */ }
}
```

### Error Response

```json
{
  "seq": 2,
  "type": "response",
  "request_seq": 1,
  "success": false,
  "command": "initialize",
  "message": "Error message",
  "body": {
    "error": {
      "id": 1,
      "format": "Detailed error: {reason}",
      "variables": { "reason": "..." }
    }
  }
}
```

### Event

```json
{
  "seq": 3,
  "type": "event",
  "event": "stopped",
  "body": { /* event-specific */ }
}
```

## Initialization Sequence

### 1. Initialize Request

```json
{
  "command": "initialize",
  "arguments": {
    "clientId": "debugger-cli",
    "clientName": "LLM Debugger CLI",
    "adapterId": "lldb-dap",
    "linesStartAt1": true,
    "columnsStartAt1": true,
    "pathFormat": "path",
    "supportsVariableType": true,
    "supportsVariablePaging": true,
    "supportsRunInTerminalRequest": false,
    "supportsMemoryReferences": true,
    "supportsProgressReporting": true
  }
}
```

### Initialize Response (Capabilities)

```json
{
  "body": {
    "supportsConfigurationDoneRequest": true,
    "supportsFunctionBreakpoints": true,
    "supportsConditionalBreakpoints": true,
    "supportsHitConditionalBreakpoints": true,
    "supportsEvaluateForHovers": true,
    "supportsStepBack": false,
    "supportsSetVariable": true,
    "supportsRestartFrame": false,
    "supportsGotoTargetsRequest": false,
    "supportsStepInTargetsRequest": false,
    "supportsCompletionsRequest": true,
    "supportsModulesRequest": true,
    "supportsDataBreakpoints": true,
    "supportsReadMemoryRequest": true,
    "supportsDisassembleRequest": true,
    "supportsInstructionBreakpoints": true
  }
}
```

### 2. Initialized Event (Adapter → Client)

```json
{
  "type": "event",
  "event": "initialized"
}
```

### 3. Set Breakpoints

```json
{
  "command": "setBreakpoints",
  "arguments": {
    "source": {
      "path": "/path/to/src/main.rs"
    },
    "breakpoints": [
      { "line": 10 },
      { "line": 25, "condition": "x > 5" },
      { "line": 30, "hitCondition": "3" }
    ]
  }
}
```

Response:
```json
{
  "body": {
    "breakpoints": [
      { "id": 1, "verified": true, "line": 10 },
      { "id": 2, "verified": true, "line": 25 },
      { "id": 3, "verified": false, "message": "No code at line 30" }
    ]
  }
}
```

### 4. Configuration Done

```json
{
  "command": "configurationDone"
}
```

### 5. Launch

```json
{
  "command": "launch",
  "arguments": {
    "program": "/path/to/executable",
    "args": ["arg1", "arg2"],
    "cwd": "/working/directory",
    "env": { "KEY": "value" },
    "stopOnEntry": false
  }
}
```

### 5b. Attach (Alternative)

```json
{
  "command": "attach",
  "arguments": {
    "pid": 12345
  }
}
```

## Execution Control

### Continue

```json
{
  "command": "continue",
  "arguments": {
    "threadId": 1,
    "singleThread": false
  }
}
```

Response:
```json
{
  "body": {
    "allThreadsContinued": true
  }
}
```

### Next (Step Over)

```json
{
  "command": "next",
  "arguments": {
    "threadId": 1,
    "granularity": "statement"  // "statement", "line", or "instruction"
  }
}
```

### Step In

```json
{
  "command": "stepIn",
  "arguments": {
    "threadId": 1,
    "granularity": "statement"
  }
}
```

### Step Out

```json
{
  "command": "stepOut",
  "arguments": {
    "threadId": 1,
    "granularity": "statement"
  }
}
```

### Pause

```json
{
  "command": "pause",
  "arguments": {
    "threadId": 1
  }
}
```

## Stopped Event

When execution stops:

```json
{
  "type": "event",
  "event": "stopped",
  "body": {
    "reason": "breakpoint",  // "step", "breakpoint", "exception", "pause", "entry", etc.
    "description": "Paused on breakpoint",
    "threadId": 1,
    "allThreadsStopped": true,
    "hitBreakpointIds": [1]
  }
}
```

## State Inspection

### Threads

```json
{
  "command": "threads"
}
```

Response:
```json
{
  "body": {
    "threads": [
      { "id": 1, "name": "main" },
      { "id": 2, "name": "worker-1" }
    ]
  }
}
```

### Stack Trace

```json
{
  "command": "stackTrace",
  "arguments": {
    "threadId": 1,
    "startFrame": 0,
    "levels": 20
  }
}
```

Response:
```json
{
  "body": {
    "stackFrames": [
      {
        "id": 1000,
        "name": "main",
        "source": { "path": "/path/to/main.rs" },
        "line": 42,
        "column": 5
      },
      {
        "id": 1001,
        "name": "process_data",
        "source": { "path": "/path/to/lib.rs" },
        "line": 100,
        "column": 1
      }
    ],
    "totalFrames": 5
  }
}
```

### Scopes

```json
{
  "command": "scopes",
  "arguments": {
    "frameId": 1000
  }
}
```

Response:
```json
{
  "body": {
    "scopes": [
      {
        "name": "Locals",
        "variablesReference": 100,
        "expensive": false
      },
      {
        "name": "Arguments",
        "variablesReference": 101,
        "expensive": false
      },
      {
        "name": "Registers",
        "variablesReference": 102,
        "expensive": true
      }
    ]
  }
}
```

### Variables

```json
{
  "command": "variables",
  "arguments": {
    "variablesReference": 100,
    "start": 0,
    "count": 50
  }
}
```

Response:
```json
{
  "body": {
    "variables": [
      {
        "name": "x",
        "value": "42",
        "type": "i32",
        "variablesReference": 0  // 0 = no children
      },
      {
        "name": "data",
        "value": "Vec<u8> { len: 10 }",
        "type": "Vec<u8>",
        "variablesReference": 200  // Has children, fetch with this ref
      }
    ]
  }
}
```

### Evaluate Expression

```json
{
  "command": "evaluate",
  "arguments": {
    "expression": "x + y * 2",
    "frameId": 1000,
    "context": "watch"  // "watch", "repl", "hover", or "clipboard"
  }
}
```

Response:
```json
{
  "body": {
    "result": "52",
    "type": "i32",
    "variablesReference": 0
  }
}
```

## Breakpoint Management

### Function Breakpoints

```json
{
  "command": "setFunctionBreakpoints",
  "arguments": {
    "breakpoints": [
      { "name": "main" },
      { "name": "mymodule::process", "condition": "count > 0" }
    ]
  }
}
```

### Data Breakpoints (Watchpoints)

First, check if supported:
```json
{
  "command": "dataBreakpointInfo",
  "arguments": {
    "variablesReference": 100,
    "name": "x"
  }
}
```

Response:
```json
{
  "body": {
    "dataId": "0x7ffc12345678",
    "description": "x (i32)",
    "accessTypes": ["read", "write", "readWrite"]
  }
}
```

Then set:
```json
{
  "command": "setDataBreakpoints",
  "arguments": {
    "breakpoints": [
      {
        "dataId": "0x7ffc12345678",
        "accessType": "write"
      }
    ]
  }
}
```

## Session Termination

### Terminate (Stop debuggee)

```json
{
  "command": "terminate",
  "arguments": {
    "restart": false
  }
}
```

### Disconnect

```json
{
  "command": "disconnect",
  "arguments": {
    "restart": false,
    "terminateDebuggee": true  // For launch
    // or "terminateDebuggee": false for attach
  }
}
```

## Important Events

### Output

```json
{
  "type": "event",
  "event": "output",
  "body": {
    "category": "stdout",  // "console", "stdout", "stderr", "telemetry"
    "output": "Hello, world!\n"
  }
}
```

### Thread

```json
{
  "type": "event",
  "event": "thread",
  "body": {
    "reason": "started",  // "started" or "exited"
    "threadId": 2
  }
}
```

### Exited

```json
{
  "type": "event",
  "event": "exited",
  "body": {
    "exitCode": 0
  }
}
```

### Terminated

```json
{
  "type": "event",
  "event": "terminated",
  "body": {
    "restart": false
  }
}
```

## lldb-dap Specific Extensions

### Launch Arguments

```json
{
  "command": "launch",
  "arguments": {
    "program": "/path/to/exe",
    "args": [],
    "cwd": ".",
    "env": {},
    "stopOnEntry": false,
    "runInTerminal": false,
    "initCommands": ["settings set target.x86-disassembly-flavor intel"],
    "preRunCommands": [],
    "postRunCommands": [],
    "exitCommands": []
  }
}
```

### Custom Commands

lldb-dap supports running LLDB commands directly via evaluate:

```json
{
  "command": "evaluate",
  "arguments": {
    "expression": "`thread backtrace all",
    "context": "repl"
  }
}
```

The backtick prefix runs raw LLDB commands.

## Error Codes

Common error scenarios:

| Error ID | Meaning |
|----------|---------|
| 1 | Generic error |
| 2 | Invalid request |
| 3 | Operation not supported |
| 4 | Resource not found |
| 5 | Timeout |

## Object Reference Lifetime

**Important**: All `variablesReference` and similar integer IDs are only valid while the debuggee is stopped. When execution resumes (continue, step), all references become invalid and must be re-fetched after the next stop event.
