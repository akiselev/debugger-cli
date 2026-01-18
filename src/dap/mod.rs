//! Debug Adapter Protocol (DAP) implementation
//!
//! This module implements the client side of DAP for communicating
//! with debug adapters like lldb-dap.

pub mod client;
pub mod codec;
pub mod types;

pub use client::DapClient;
pub use types::*;
