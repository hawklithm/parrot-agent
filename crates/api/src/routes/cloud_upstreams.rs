//! Cloud Upstream routes — P4 收尾域 (CU1-CU8)
//!
//! Thin route handlers that delegate business logic to `CloudUpstreamService`.

use axum::extract::Query;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use axum::Extension;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompanyQuery {
    company_id: Uuid,
}

pub fn cloud_upstream_routes() -> Router<AppState> {
    Router::new()
        .route("/cloud-upstreams", get(list_cloud_upstreams))
        .route("/cloud-upstreams/connect/start", post(start_cloud_connect))
        .route(
            "/cloud-upstreams/connect/finish",
            post(finish_cloud_connect),
        )
        .route(
            "/cloud-upstreams/:connection_id/push-runs/preview",
            post(preview_push_run),
        )
        .route(
            "/cloud-upstreams/:connection_id/push-runs",
            post(execute_push_run),
        )
        .route(
            "/cloud-upstreams/:connection_id/push-runs/:run_id",
            get(get_push_run),
        )
        .route(
            "/cloud-upstreams/:connection_id/push-runs/:run_id/cancel",
            post(cancel_push_run),
        )
        .route(
            "/cloud-upstreams/:connection_id/push-runs/:run_id/activation",
            post(activate_push_run),
        )
        .layer(axum::middleware::from_fn(
            crate::routes::require_cloud_company_access,
        ))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_cloud_upstreams(
    State(state): State<AppState>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    state
        .cloud_upstream_service
        .list(query.company_id)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn start_cloud_connect(
    State(state): State<AppState>,
    Extension(actor): Extension<services::auth::AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let remote_url = body
        .get("remoteUrl")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let redirect_uri = body
        .get("redirectUri")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    state
        .cloud_upstream_service
        .start_connect(company_id, remote_url, redirect_uri)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn finish_cloud_connect(
    State(state): State<AppState>,
    Extension(actor): Extension<services::auth::AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let id = body
        .get("pendingConnectionId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    // Resolve company_id for access check
    let company_id: Uuid = sqlx::query_scalar(
        "SELECT company_id FROM cloud_upstream_connections WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let returned_state = body
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let code = body
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    state
        .cloud_upstream_service
        .finish_connect(id, returned_state, code)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn preview_push_run(
    State(state): State<AppState>,
    Path(connection_id): Path<Uuid>,
    Extension(actor): Extension<services::auth::AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    state
        .cloud_upstream_service
        .preview(connection_id, company_id)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn execute_push_run(
    State(state): State<AppState>,
    Path(connection_id): Path<Uuid>,
    Extension(actor): Extension<services::auth::AuthorizationActor>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    state
        .cloud_upstream_service
        .create_run(connection_id, company_id, body)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn get_push_run(
    State(state): State<AppState>,
    Path((_connection_id, run_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .cloud_upstream_service
        .read_run(query.company_id, run_id)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

async fn cancel_push_run(
    State(state): State<AppState>,
    Path((_connection_id, run_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .cloud_upstream_service
        .cancel_run(query.company_id, run_id)
        .await
        .map(|_| Json(serde_json::json!({"status": "cancelled"})))
        .map_err(|e| service_to_status(e))
}

async fn activate_push_run(
    State(state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let entity_type = body
        .get("entityType")
        .and_then(|value| value.as_str())
        .filter(|value| matches!(*value, "agents" | "routines" | "monitors"))
        .ok_or(StatusCode::BAD_REQUEST)?;
    state
        .cloud_upstream_service
        .activate_entity(run_id, connection_id, entity_type)
        .await
        .map(Json)
        .map_err(|e| service_to_status(e))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn service_to_status(e: services::errors::ServiceError) -> StatusCode {
    use services::errors::ServiceError;
    match e {
        ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
        ServiceError::BadRequest(_) => StatusCode::BAD_REQUEST,
        ServiceError::Conflict(_) => StatusCode::CONFLICT,
        ServiceError::Forbidden(_) => StatusCode::FORBIDDEN,
        ServiceError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
