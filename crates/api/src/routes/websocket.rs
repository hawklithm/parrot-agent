//! Paperclip-compatible company live-events WebSocket.
//!
//! Paperclip keeps this transport separate from the ordinary HTTP/SSE routes:
//! the browser connects to `/api/companies/:companyId/events/ws` and receives
//! live company events until the socket closes.  Reuse Parrot's company-scoped
//! broadcast service here so local-trusted Board actors can connect without a
//! company-bound session token.

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};
use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;
use models::SseSubscription;
use services::auth::AuthorizationActor;
use uuid::Uuid;

use crate::app_state::AppState;

pub fn websocket_routes() -> Router<AppState> {
    Router::new().route(
        "/companies/:companyId/events/ws",
        get(company_events_websocket),
    )
}

async fn company_events_websocket(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    axum::extract::Extension(actor): axum::extract::Extension<AuthorizationActor>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    // Match Paperclip's upgrade authorization: the resource company is the
    // scope, and local-trusted Board actors are allowed across companies.
    crate::routes::assert_company_access(&actor, company_id, true)?;
    let actor_id = actor.principal_id().unwrap_or(Uuid::nil());
    let receiver = state
        .sse_service
        .subscribe(SseSubscription {
            company_id,
            actor_id,
            channel: "events".to_string(),
            last_event_id: None,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(ws.on_upgrade(move |socket| forward_company_events(socket, receiver)))
}

async fn forward_company_events(
    mut socket: WebSocket,
    mut receiver: tokio::sync::broadcast::Receiver<models::SseFrame>,
) {
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Ok(frame) => {
                    if socket.send(Message::Text(frame.data.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            message = socket.next() => match message {
                Some(Ok(Message::Ping(payload))) => {
                    if socket.send(Message::Pong(payload)).await.is_err() { break; }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}
