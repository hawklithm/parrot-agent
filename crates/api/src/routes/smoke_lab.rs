//! Smoke Lab 路由 —— 对齐 Paperclip `server/src/routes/smoke-lab.ts`（15 端点）。
//! OAuth 端点为 smoke 测试的 mock OAuth 服务；services 为静态 mock 清单。

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

fn run_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "trigger": row.get::<String, _>("trigger"),
        "status": row.get::<String, _>("status"),
        "startedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
        "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
        "summary": row.get::<Value, _>("summary"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

fn step_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "runId": row.get::<Uuid, _>("run_id"),
        "path": row.get::<String, _>("path"),
        "scenarioStep": row.get::<String, _>("scenario_step"),
        "status": row.get::<String, _>("status"),
        "detail": row.get::<Option<String>, _>("detail"),
        "screenshotArtifactRef": row.get::<Option<Value>, _>("screenshot_artifact_ref"),
        "durationMs": row.get::<Option<i32>, _>("duration_ms"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })
}

/// GET /companies/:cid/smoke-lab/oauth/authorize —— mock OAuth authorize（302 到 redirect_uri）。
async fn oauth_authorize(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<axum::response::Response, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let code = Uuid::new_v4().to_string();
    Ok(axum::response::Redirect::to(&format!(
        "/api/companies/{}/smoke-lab/oauth/authorize?code={}&state=mock",
        company_id, code
    ))
    .into_response())
}

/// POST /companies/:cid/smoke-lab/oauth/token —— mock token 端点。
async fn oauth_token(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "access_token": format!("mock-token-{}", Uuid::new_v4()),
        "token_type": "Bearer",
        "expires_in": 3600,
    })))
}

/// GET /companies/:cid/smoke-lab/oauth/userinfo —— mock userinfo。
async fn oauth_userinfo(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "sub": "smoke-user",
        "name": "Smoke Test User",
        "email": "smoke@example.test",
        "preferred_username": "smoke-user",
    })))
}

/// POST /companies/:cid/smoke-lab/oauth/revoke —— mock revoke（204）。
async fn oauth_revoke(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /companies/:cid/smoke-lab/services —— 静态 mock 服务清单。
async fn list_smoke_services(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![
        json!({ "name": "mock-gitlab", "status": "stopped" }),
        json!({ "name": "mock-slack", "status": "stopped" }),
        json!({ "name": "mock-oauth", "status": "running" }),
    ]))
}

/// POST /companies/:cid/smoke-lab/services/start|stop —— mock（204）。
async fn smoke_service_start(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /companies/:cid/smoke-lab/services/stop —— mock（204）。
async fn smoke_service_stop(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /companies/:cid/smoke-lab/install-fixtures —— mock（204）。
async fn install_smoke_fixtures(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /companies/:cid/smoke-lab/runs —— 列表。
async fn list_smoke_runs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT * FROM smoke_runs WHERE company_id = $1 ORDER BY started_at DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list smoke runs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(run_json).collect()))
}

#[derive(Debug, Deserialize)]
struct CreateSmokeRunRequest {
    trigger: Option<String>,
}

/// POST /companies/:cid/smoke-lab/runs —— 创建 run。
async fn create_smoke_run(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateSmokeRunRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO smoke_runs (id, company_id, trigger) VALUES ($1, $2, COALESCE($3, 'manual'))",
    )
    .bind(id)
    .bind(company_id)
    .bind(request.trigger.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create smoke run: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = sqlx::query("SELECT * FROM smoke_runs WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload smoke run: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(run_json(&row))))
}

/// GET /companies/:cid/smoke-lab/runs/:run_id
async fn get_smoke_run(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    
    let row = sqlx::query("SELECT * FROM smoke_runs WHERE id = $1 AND company_id = $2")
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load smoke run: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let steps = sqlx::query(
        "SELECT * FROM smoke_run_steps WHERE run_id = $1 ORDER BY created_at ASC",
    )
    .bind(run_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load smoke steps: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut v = run_json(&row);
    v["steps"] = json!(steps.iter().map(step_json).collect::<Vec<_>>());
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
struct UpdateSmokeRunRequest {
    status: Option<String>,
    summary: Option<Value>,
}

/// PATCH /companies/:cid/smoke-lab/runs/:run_id
async fn update_smoke_run(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, run_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateSmokeRunRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    
    let row = sqlx::query(
        "UPDATE smoke_runs SET \
         status = COALESCE($3, status), \
         summary = COALESCE($4, summary), \
         finished_at = CASE WHEN $3 IN ('passed','failed','cancelled') THEN COALESCE(finished_at, NOW()) ELSE finished_at END, \
         updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(request.status.as_deref())
    .bind(request.summary.as_ref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update smoke run: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(run_json(&row)))
}

#[derive(Debug, Deserialize)]
struct RecordSmokeStepRequest {
    path: String,
    #[serde(rename = "scenarioStep")]
    scenario_step: String,
    status: String,
    detail: Option<String>,
    #[serde(rename = "durationMs")]
    duration_ms: Option<i32>,
}

/// POST /companies/:cid/smoke-lab/runs/:run_id/steps
async fn record_smoke_step(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, run_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<RecordSmokeStepRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO smoke_run_steps (id, company_id, run_id, path, scenario_step, status, detail, duration_ms) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(company_id)
    .bind(run_id)
    .bind(&request.path)
    .bind(&request.scenario_step)
    .bind(&request.status)
    .bind(request.detail.as_deref())
    .bind(request.duration_ms)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to record smoke step: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let row = sqlx::query("SELECT * FROM smoke_run_steps WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload smoke step: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(step_json(&row))))
}

/// POST /companies/:cid/smoke-lab/reset —— 清空该 company 的 runs/steps。
async fn reset_smoke_lab(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM smoke_runs WHERE company_id = $1")
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reset smoke lab: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn smoke_lab_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/smoke-lab/oauth/authorize",
            get(oauth_authorize).post(oauth_authorize),
        )
        .route(
            "/companies/:company_id/smoke-lab/oauth/token",
            post(oauth_token),
        )
        .route(
            "/companies/:company_id/smoke-lab/oauth/userinfo",
            get(oauth_userinfo),
        )
        .route(
            "/companies/:company_id/smoke-lab/oauth/revoke",
            post(oauth_revoke),
        )
        .route(
            "/companies/:company_id/smoke-lab/services",
            get(list_smoke_services),
        )
        .route(
            "/companies/:company_id/smoke-lab/services/start",
            post(smoke_service_start),
        )
        .route(
            "/companies/:company_id/smoke-lab/services/stop",
            post(smoke_service_stop),
        )
        .route(
            "/companies/:company_id/smoke-lab/install-fixtures",
            post(install_smoke_fixtures),
        )
        .route(
            "/companies/:company_id/smoke-lab/runs",
            get(list_smoke_runs).post(create_smoke_run),
        )
        .route(
            "/companies/:company_id/smoke-lab/runs/:run_id",
            get(get_smoke_run).patch(update_smoke_run),
        )
        .route(
            "/companies/:company_id/smoke-lab/runs/:run_id/steps",
            post(record_smoke_step),
        )
        .route(
            "/companies/:company_id/smoke-lab/reset",
            post(reset_smoke_lab),
        )
}
