//! Cloud Upstream routes — P4 收尾域 (CU1-CU8)

use axum::extract::Query;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;

#[derive(Debug, Deserialize)]
struct CompanyQuery {
    company_id: Uuid,
}

fn bad_request() -> StatusCode {
    StatusCode::BAD_REQUEST
}

async fn ensure_cloud_sync(state: &AppState) -> Result<(), StatusCode> {
    let settings = state
        .instance_settings_service
        .get_experimental_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !settings.enable_cloud_sync {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(())
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
            crate::routes::require_authenticated,
        ))
}

async fn list_cloud_upstreams(
    State(state): State<AppState>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let rows = sqlx::query("SELECT id, company_id, remote_url, status, created_at, updated_at FROM cloud_upstream_connections WHERE company_id = $1 ORDER BY updated_at DESC")
        .bind(query.company_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({
        "id": r.get::<Uuid, _>("id"), "companyId": r.get::<Uuid, _>("company_id"),
        "remoteUrl": r.get::<String, _>("remote_url"), "status": r.get::<String, _>("status"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"), "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    })).collect()))
}

async fn start_cloud_connect(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(bad_request)?;
    let remote_url = body
        .get("remoteUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(bad_request)?;
    let redirect_uri = body
        .get("redirectUri")
        .and_then(|v| v.as_str())
        .ok_or_else(bad_request)?;
    let id = Uuid::new_v4();
    let state_token = Uuid::new_v4().to_string();
    let verifier = Uuid::new_v4().to_string();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    sqlx::query("INSERT INTO cloud_upstream_connections (id, company_id, remote_url, status, pending_state, pending_code_verifier, pending_redirect_uri) VALUES ($1, $2, $3, 'pending', $4, $5, $6)")
        .bind(id).bind(company_id).bind(remote_url).bind(&state_token).bind(&verifier).bind(redirect_uri)
        .execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let authorization_url = format!(
        "{}/oauth/authorize?state={}&code_challenge={}&code_challenge_method=S256&redirect_uri={}",
        remote_url.trim_end_matches('/'),
        state_token,
        challenge,
        urlencoding::encode(redirect_uri)
    );
    Ok(Json(
        serde_json::json!({"connectionId": id, "status": "pending", "authorizationUrl": authorization_url}),
    ))
}

async fn finish_cloud_connect(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let id = body
        .get("pendingConnectionId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(bad_request)?;
    let returned_state = body
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or_else(bad_request)?;
    let expected_state: Option<String> =
        sqlx::query_scalar("SELECT pending_state FROM cloud_upstream_connections WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if expected_state.as_deref() != Some(returned_state) {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query("UPDATE cloud_upstream_connections SET status = 'connected', token_status = 'connected', pending_state = NULL, pending_code_verifier = NULL, updated_at = NOW() WHERE id = $1")
        .bind(id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({"connectionId": id, "status": "connected"}),
    ))
}

async fn preview_push_run(
    State(state): State<AppState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM cloud_upstream_connections WHERE id = $1")
            .bind(connection_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(
        serde_json::json!({"connectionId": connection_id, "preview": {"ready": true}}),
    ))
}

async fn execute_push_run(
    State(state): State<AppState>,
    Path(connection_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(bad_request)?;
    let connection_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM cloud_upstream_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if connection_status.as_deref() != Some("connected") {
        return Err(StatusCode::CONFLICT);
    }
    let running: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cloud_upstream_runs WHERE connection_id = $1 AND status = 'running')")
        .bind(connection_id).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if running {
        return Err(StatusCode::CONFLICT);
    }
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO cloud_upstream_runs (id, connection_id, company_id, status, active_step, progress_percent) VALUES ($1, $2, $3, 'running', 'preview', 0)",
    )
    .bind(run_id)
    .bind(connection_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({"connectionId": connection_id, "runId": run_id, "status": "running"}),
    ))
}

async fn get_push_run(
    State(state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM cloud_upstream_runs WHERE id = $1 AND connection_id = $2 AND company_id = $3",
    )
    .bind(run_id)
    .bind(connection_id)
    .bind(query.company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({"connectionId": connection_id, "runId": run_id, "status": status.ok_or(StatusCode::NOT_FOUND)?}),
    ))
}

async fn cancel_push_run(
    State(state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let result = sqlx::query("UPDATE cloud_upstream_runs SET status = 'cancelled', updated_at = NOW() WHERE id = $1 AND connection_id = $2")
        .bind(run_id).bind(connection_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(
        serde_json::json!({"connectionId": connection_id, "runId": run_id, "status": "cancelled"}),
    ))
}

async fn activate_push_run(
    State(state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    ensure_cloud_sync(&state).await?;
    let entity_type = body
        .get("entityType")
        .and_then(|value| value.as_str())
        .filter(|value| matches!(*value, "agents" | "routines" | "monitors"))
        .ok_or_else(bad_request)?;
    let report: Option<serde_json::Value> = sqlx::query_scalar(
        "UPDATE cloud_upstream_runs SET report = jsonb_set(report, ARRAY['activationChecklist', $3], 'true'::jsonb, true), updated_at = NOW() WHERE id = $1 AND connection_id = $2 AND status = 'succeeded' RETURNING report",
    )
    .bind(run_id)
    .bind(connection_id)
    .bind(entity_type)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(report) = report else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(
        serde_json::json!({"connectionId": connection_id, "runId": run_id, "report": report}),
    ))
}
