//! Common utilities shared between CLI and daemon modes

pub mod config;
pub mod error;
pub mod logging;
pub mod paths;

pub use error::{Error, Result};

/// Parse a "listening at:" address from adapter output.
/// Handles IPv6 format [::]:PORT by converting to 127.0.0.1:PORT
pub fn parse_listen_address(line: &str) -> Option<String> {
    if let Some(addr_start) = line.find("listening at:") {
        let addr_part = &line[addr_start + "listening at:".len()..];
        let addr = addr_part.trim().to_string();
        // Handle IPv6 format [::]:PORT
        let addr = if addr.starts_with("[::]:") {
            addr.replace("[::]:", "127.0.0.1:")
        } else {
            addr
        };
        Some(addr)
    } else {
        None
    }
}