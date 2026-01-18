i want to make my own CLI tool (in rust) that allows LLM coding agents to debug executables.

Let's use the Debug Adapter Protocol (DAP)

This is how VS Code talks to debuggers. Instead of writing a debugger host, you would write a DAP Client.

    Strategy: Your tool spawns an adapter (like lldb-dap or OpenDebugAD7) and sends JSON packets to it.

    Pros: You instantly support every debugger that has an adapter (Python, Go, Rust/C++, etc.).

    Cons: Writing a full DAP client is complex. It's "async heavy" and verbose.


the agent should be able to run commands like:

debugger start <path to binary> # Start the daemon

debugger attach <pid> # Attach to an existing process

debugger breakpoint add src/rust_file.rs:93 # or more specific if it needs to be a breakpoint on an inner expression

debugger breakpoint await --timeout 60 # wait for the next break point or timeout in 60 seconds

debugger stop # Stop the process and the daemon

debugger detach # Detach from running process and stop the daemon



and so on. We'll want watch commands and anything else the debugger supports. this should be cross platform. The start/attach commands might not make sense entirely since im not familiar with DAP. figure out an ergonomic start command, hopefully one that start the debugger/binary (like lldb-dap or whatever, make it configurable)


You cannot run the debugger inside the ephemeral CLI process. If the CLI exits, the debugger session dies.

You must build a Client-Server (Daemon) architecture.
The Flow

    Agent runs: debugger start ./my_app

    CLI Tool:

        Checks if a Daemon is running. If not, spawns debugger_daemon in the background (detached).

        Sends a command to the Daemon via IPC (Unix Socket on Linux/Mac, Named Pipe on Windows).

        Daemon spawns the lldb instance and holds the handle.

    Agent runs: debugger break main.rs:20

    CLI Tool: Connects to Daemon socket -> sends instruction -> Daemon tells lldb to set breakpoint -> Daemon returns "OK".

Come up with a plan and put it in docs/plan/

Make sure to look up online documentation on the DAP and any rust crates you use!