//! Parrot CLI library — shared code for the parrot binary and tests.
//!
//! Re-exports all modules so they're accessible from both the binary and test code.
//! The `main` function is in `bin/parrot.rs`.

mod backup;
pub mod checks;
pub mod client;
pub mod commands;
pub mod config;
pub mod context;
pub mod env_lab;
pub mod install_store;
pub mod plugin_scaffold;
pub mod services;
pub mod update_notice;
pub mod worktree;

// Re-export key types for external access
pub use client::ApiClient;
pub use commands::run;
pub use config::{default_config_path, resolve_config_path, CliConfig};
