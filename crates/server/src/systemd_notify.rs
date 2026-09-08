//! Systemd readiness and stopping notifications.
//!
//! Implements `NotifyAccess=all` compliance: the server sends READY when the
//! HTTP listener is bound, and STOPPING before initiating drain.

use sd_notify::NotifyState;

/// Send a systemd READY notification. No-op if NOTIFY_SOCKET is unset.
pub fn notify_ready() {
    // `sd-notify` 0.3 returns `io::Result<()>`; `WouldBlock` means NOTIFY_SOCKET
    // is unset (running outside systemd), which is not an error.
    match sd_notify::notify(true, &[NotifyState::Ready]) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
            tracing::debug!("systemd notification socket not configured — running outside systemd");
        }
        Err(e) => tracing::warn!("failed to send systemd READY: {e}"),
        Ok(()) => tracing::info!("systemd READY notification sent"),
    }
}

/// Send a systemd STOPPING notification before draining connections.
pub fn notify_stopping(reason: &str) {
    match sd_notify::notify(
        true,
        &[NotifyState::Stopping, NotifyState::Status(reason.to_string())],
    ) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(e) => tracing::warn!("failed to send systemd STOPPING: {e}"),
        Ok(()) => tracing::info!("systemd STOPPING notification sent"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_ready_no_panic_outside_systemd() {
        // NOTIFY_SOCKET is not set in test env; should log debug, not panic.
        notify_ready();
    }
}
