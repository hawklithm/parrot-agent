//! Process-wide live event publisher.
//!
//! Mirrors Paperclip's `server/src/services/live-events.ts`: a module-level
//! singleton that any writer can publish through without threading the SSE
//! service down every call chain. [`init`] is called once while the server
//! boots; writers that run outside a server process (unit tests, CLI tools)
//! simply no-op.

use std::sync::{Arc, OnceLock};

use chrono::Utc;
use models::{SseEvent, SseEventType};
use uuid::Uuid;

use crate::sse_service::SseService;

/// The channel every browser SSE/WS subscription listens on.
pub const LIVE_EVENTS_CHANNEL: &str = "events";

static SSE_SERVICE: OnceLock<Arc<dyn SseService>> = OnceLock::new();

/// Registers the process-wide publisher. Called once at server construction;
/// later calls are ignored, so the first publisher wins.
pub fn init(service: Arc<dyn SseService>) {
    let _ = SSE_SERVICE.set(service);
}

/// Publishes a `{id, companyId, type, createdAt, payload}` frame on the
/// [`LIVE_EVENTS_CHANNEL`] channel.
///
/// No-op when the publisher was never initialised. Publish failures are
/// swallowed for the same reason audit writes are: live delivery is best
/// effort and must never fail the caller's transaction.
pub async fn publish_live_event(company_id: Uuid, event_type: &str, payload: serde_json::Value) {
    let Some(service) = SSE_SERVICE.get() else {
        return;
    };
    let event = serde_json::json!({
        "id": Uuid::new_v4(),
        "companyId": company_id,
        "type": event_type,
        "createdAt": Utc::now(),
        "payload": payload,
    });
    let _ = service
        .publish(
            company_id,
            LIVE_EVENTS_CHANNEL,
            SseEvent {
                event_type: SseEventType::Message,
                channel: LIVE_EVENTS_CHANNEL.to_string(),
                payload: event,
                timestamp: Utc::now(),
            },
        )
        .await;
}
