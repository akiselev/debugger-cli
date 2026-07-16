# Architecture Review — 2026-07

## Current Shape

`debugger-cli` is one binary in two modes. Short-lived CLI invocations send a
length-prefixed JSON request to a local daemon, which owns one `DebugSession`.
The session owns the DAP adapter process and a background reader task; DAP
events are buffered until the daemon processes them.

This is the right product-level split. It makes an otherwise interactive DAP
connection usable from independent agent commands, and it prevents output and
stop events from being lost between invocations.

## What Is Working Well

- The DAP reader task keeps adapter I/O independent from CLI process lifetime.
- `DebugSession` centralizes adapter capability checks and selected
  thread/frame state.
- The IPC boundary keeps CLI presentation code out of DAP mechanics.
- Adapter setup is isolated from the debugging path.

## Risks and Friction

1. **The daemon is single-client and serial.** `handle_client` owns the daemon
   until that connection closes. A long `await`, or any future true streaming
   command, blocks other CLI clients. Concretely: while one terminal runs
   `await --timeout 30`, a second terminal cannot send `pause` or `continue` —
   the command that would cause the stop is blocked behind the command waiting
   for it. The current `output --follow` avoids this by reconnecting for each
   poll, but that is a containment measure rather than a general concurrency
   model.

   A related defect: DAP events are only reduced on the accept loop's 1-second
   tick, and that tick does not run at all while a client connection is open,
   because `handle_client` is awaited inline. Handlers that need fresh state
   (`output`, `await`) each call `process_events()` manually as a workaround.

2. **`DebugSession` has too many responsibilities.** It owns DAP transport,
   session lifecycle, event reduction, breakpoint persistence, output storage,
   selected-view state, and inspection helpers. This makes a feature such as
   multi-session support or a richer output API expensive to add safely.

3. **DAP request lifecycle is split across several paths.** Normal requests,
   deferred launch, and fire-and-forget disconnect have subtly different
   response handling. Adapters may legally delay `launch` until after
   `configurationDone`; this was the root cause of the GDB startup deadlock
   fixed in this pass.

4. **The IPC protocol is typed at the command edge but loosely typed at the
   result edge.** Most responses are `serde_json::Value`, so the daemon and CLI
   can silently drift in field names and error handling.

5. **Validation is adapter-sparse.** Unit tests are useful, but many core
   workflows were ignored when their adapter was absent. A real GDB smoke test
   now runs opportunistically on GDB 14.1+, but the other supported adapters
   still need reproducible coverage.

## Recommended Evolution

### Phase 1 — Make command handling concurrent

Keep exactly one debug session, but move it behind a session actor:

```text
IPC clients ──> request tasks ──> session command channel ──> DebugSession
                                           │
DAP reader ──> event reducer ─────────────┴──> broadcast/watch state updates
```

Each IPC connection can then be handled independently. The actor serializes
DAP requests (which preserves adapter ordering), while `await`, status, and
future subscriptions can wait on state updates without holding the listener.
This is the highest-value architecture change.

Two constraints on the actor design:

- The actor must own the session *lifecycle*, not just command dispatch.
  `start`/`stop`/`detach` create and destroy the session, so session
  creation and teardown belong in the same serialized command domain.
- Decision 3 below (destructive vs. cursor-based output) should be resolved
  before this phase, not before a larger rewrite: the actor's broadcast/watch
  design bakes in one answer or the other.

### Phase 2 — Split session state from DAP orchestration

Extract three focused components:

- `DapTransport`: request IDs, timeouts, deferred-response handling, and raw
  adapter events.
- `SessionModel`: reducer-owned state for lifecycle, threads, frames,
  breakpoints, and exit status.
- `OutputBuffer`: bounded output with a monotonic cursor. A cursor lets clients
  follow output without clearing data another client has not read.

`DebugSession` can remain the façade initially, so this is an internal,
incremental refactor rather than a CLI redesign.

### Phase 3 — Stabilize the public automation contract

Add a `--json` output mode for every inspection and control command, with
versioned response structs rather than ad-hoc JSON values. Keep human output as
the default. This is especially valuable for LLM agents and scenario tests.

At the same time, version the local IPC envelope. The Unix socket is already
restricted to the owning user (`0o600` in `transport.rs`); the remaining gap is
the Windows named-pipe security descriptor. The daemon is a local control plane
for arbitrary debuggee processes, so it should never become a casually exposed
network service.

### Phase 4 — Broaden confidence deliberately

- Keep the native GDB test as a no-install smoke test where supported.
- Use containers or a scheduled CI matrix for LLDB, debugpy, Delve, and
  js-debug rather than claiming an unprovisioned matrix runs on every PR.
- Add mock-DAP tests for request ordering, delayed launch responses, output
  cursor semantics, and adapter failures. These cover protocol behavior without
  making every unit test depend on a toolchain.

## Decisions to Make Before a Larger Rewrite

1. Should one daemon support multiple named sessions, or should session
   isolation remain one daemon per target?
2. Is JSON output a supported public contract, or only an internal testing
   convenience?
3. Should output follow be destructive (current polling behavior) or
   cursor-based and independent per client?
   **Resolved 2026-07-09: cursor-based.** `GetOutput` takes an optional
   cursor and returns `next_cursor`; reads are non-destructive and
   per-client, with `--clear` retained as an explicit destructive op.
4. Do we want to preserve a zero-install default, or accept containerized
   adapter CI as a required release gate?

The recommended default is: one session for now, a versioned JSON contract,
cursor-based non-destructive output, and containerized adapter coverage before
advertising a feature as fully supported.
