//! LLM Debugger CLI - A debugger interface for LLM coding agents
//!
//! This library provides debugging capabilities through the Debug Adapter
//! Protocol (DAP), optimized for LLM agents.

pub mod cli;
pub mod commands;
pub mod common;
pub mod daemon;
pub mod dap;
pub mod ipc;
pub mod setup;

// Re-export commonly used types for tests
pub use common::{Error, Result};
pub use ipc::protocol::{BreakpointLocation, Command};
