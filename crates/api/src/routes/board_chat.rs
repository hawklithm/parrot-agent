//! Board Chat routes — P4 收尾域 (BC1)

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};

use crate::app_state::AppState;

pub fn board_chat_routes() -> Router<AppState> {
    Router::new()
        .route("/api/board/chat/stream", post(stream_board_chat))
}

async fn stream_board_chat(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // In production: this would establish an SSE stream
    (StatusCode::OK, "data: {\"message\": \"Chat stream started\"}\n\n")
}
