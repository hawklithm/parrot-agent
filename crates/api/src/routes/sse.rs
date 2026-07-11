use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
};
use futures::stream::{Stream, StreamExt};
use models::{SseEvent, SseEventType, SseFrame, SseSubscription};
use services::sse_service::SseService;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

/// GET /companies/:companyId/events/:channel
/// SSE endpoint for real-time event streaming
pub async fn sse_stream(
    Path((company_id, channel)): Path<(Uuid, String)>,
    State(service): State<Arc<dyn SseService>>,
) -> Response {
    // TODO: Extract actor_id from auth context
    let actor_id = Uuid::new_v4(); // Mock for now

    let subscription = SseSubscription {
        company_id,
        actor_id,
        channel: channel.clone(),
        last_event_id: None,
    };

    match service.subscribe(subscription).await {
        Ok(receiver) => {
            let stream = BroadcastStream::new(receiver)
                .filter_map(|result| async move {
                    match result {
                        Ok(frame) => Some(Ok::<_, Infallible>(
                            Event::default()
                                .event(frame.event.unwrap_or_else(|| "message".to_string()))
                                .data(frame.data),
                        )),
                        Err(_) => None, // Skip lagged messages
                    }
                });

            Sse::new(stream)
                .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}


/// POST /companies/:companyId/events/:channel
/// Publish event to SSE channel (for testing/admin)
pub async fn publish_event(
    Path((company_id, channel)): Path<(Uuid, String)>,
    State(service): State<Arc<dyn SseService>>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> Response {
    let event = SseEvent {
        event_type: SseEventType::Message,
        channel: channel.clone(),
        payload,
        timestamp: chrono::Utc::now(),
    };

    match service.publish(company_id, &channel, event).await {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /companies/:companyId/events/:channel/stats
/// Get channel statistics
pub async fn channel_stats(
    Path((company_id, channel)): Path<(Uuid, String)>,
    State(service): State<Arc<dyn SseService>>,
) -> Response {
    let count = service.subscriber_count(company_id, &channel).await;

    let stats = serde_json::json!({
        "channel": channel,
        "subscriberCount": count,
    });

    (StatusCode::OK, axum::Json(stats)).into_response()
}

/// Router setup for SSE endpoints
pub fn sse_routes(service: Arc<dyn SseService>) -> axum::Router {
    axum::Router::new()
        .route(
            "/companies/:companyId/events/:channel",
            axum::routing::get(sse_stream).post(publish_event),
        )
        .route(
            "/companies/:companyId/events/:channel/stats",
            axum::routing::get(channel_stats),
        )
        .with_state(service)
}
