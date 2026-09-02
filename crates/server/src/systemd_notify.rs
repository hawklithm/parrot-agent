//! Systemd readiness and stopping notifications.
//!
//! Implements `NotifyAccess=all` compliance: the server sends READY when the
//! HTTP listener is bound, and STOPPING before initiating drain.

use sd_notify::NotifyState;

/// Send a systemd READY notification. No-op if NOTIFY_SOCKET is unset.
pub fn notify_ready() {
    let state = sd_notify::notify(true, &[NotifyState::Ready]);
    match state {
        Ok(got) if got.would_block() => {
            tracing::debug!("systemd notification socket not configured — running outside systemd");
        }
        Ok(_) => tracing::info!("systemd READY notification sent"),
        Err(e) => tracing::warn!("failed to send systemd READY: {e}"),
    }
}

/// Send a systemd STOPPING notification before draining connections.
pub fn notify_stopping(reason: &str) {
    let state = sd_notify::notify(true, &[NotifyState::Stopping, NotifyState::Status(reason)]);
    match state {
        Ok(got) if got.would_block() => {}
        Ok(_) => tracing::info!("systemd STOPPING notification sent"),
        Err(e) => tracing::warn!("failed to send systemd STOPPING: {e}"),
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
