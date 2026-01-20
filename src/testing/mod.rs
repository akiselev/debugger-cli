//! E2E Test Runner
//!
//! Provides a robust test executor that reads YAML test scenarios
//! and uses the DaemonClient to communicate with the debug daemon.
//! This ensures assertions are made against structured data rather
//! than fragile string matching.

mod config;
mod runner;

pub use config::*;
pub use runner::run_scenario;
