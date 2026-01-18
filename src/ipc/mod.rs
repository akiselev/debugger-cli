//! IPC communication between CLI and daemon
//!
//! Uses Unix domain sockets on Unix/macOS and named pipes on Windows
//! via the interprocess crate.

pub mod client;
pub mod protocol;
pub mod transport;

pub use client::DaemonClient;
