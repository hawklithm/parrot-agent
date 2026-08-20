//! Service-facing boundaries for CLI commands.
//!
//! Server calls belong here as the command set grows; commands must not depend
//! on server implementation modules directly.

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Unknown,
    Healthy,
    Unavailable,
}
