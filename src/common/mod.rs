//! Common utilities shared between CLI and daemon modes

pub mod config;
pub mod error;
pub mod logging;
pub mod paths;

pub use error::{Error, Result};
