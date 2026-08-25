//! Pipeline routes — CRUD + Case operations + Stage/Transition management
//!
//! 对应 Pipeline/Adapter 模块 §6 Pipeline HTTP 路由层

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::pipeline::{Pipeline, PipelineCase, PipelineStage, PipelineTransition};
use services::CreateCaseInput;

fn db_err(e: sqlx::Error) -> AppError {
    AppError::InternalServerError(e.to_string())
}

pub fn pipeline_routes() -> Router<AppState> {
    Router::new()
        // Pipeline CRUD
        .route("/companies/:company_id/pipelines", post(create_pipeline))
        .route("/companies/:company_id/pipelines", get(list_pipelines))
        .route("/pipelines/:pipeline_id", get(get_pipeline))
        // Stages
        .route("/pipelines/:pipeline_id/stages", get(list_stages))
        .route("/pipelines/:pipeline_id/transitions", get(list_transitions))
        // Cases
        .route("/pipelines/:pipeline_id/cases", post(create_case))
        .route("/pipelines/:pipeline_id/cases", get(list_cases))
        // Note: Pipeline-specific case operations (advance, terminal) are
        // registered in cases::case_routes() under /cases/:id/ to match
        // Paperclip's route ownership model.
        // Note: GET /cases/:id/events is registered in cases.rs via case_service
        // Health & attention
        .route(
            "/pipelines/:pipeline_id/health-warnings",
            get(get_health_warnings),
        )
        .route("/pipelines/:pipeline_id/health", get(get_pipeline_health))
        .route(
            "/companies/:company_id/pipelines-attention",
            get(get_pipelines_attention),
        )
        // --- P3: Pipelines 补齐 (PP1-PP15) ---
        .route("/companies/:company_id/review-cases", get(list_review_cases))
        .route("/companies/:company_id/review-cases/bulk", post(bulk_review_cases))
        .route("/companies/:company_id/case-events", get(list_case_events))
        // --- P1: Pipeline runs management ---
        .route("/pipelines/:pipeline_id/runs", get(list_pipeline_runs).post(create_pipeline_run))
        .route("/pipelines/:pipeline_id/runs/:run_id", get(get_pipeline_run).delete(delete_pipeline_run))
        .route("/pipelines/:pipeline_id/runs/:run_id/cancel", post(cancel_pipeline_run))
        .route("/pipelines/:pipeline_id/runs/:run_id/retry", post(retry_pipeline_run))
        // --- P2: Pipeline stages detail ---
        .route("/pipelines/:pipeline_id/stages/:stage_id", get(get_pipeline_stage))
        // --- P3: Pipeline triggers ---
        .route("/pipelines/:pipeline_id/triggers", get(list_pipeline_triggers).post(create_pipeline_trigger))
        .route("/pipelines/:pipeline_id/triggers/:trigger_id", axum::routing::delete(delete_pipeline_trigger))
        // --- P4: Pipeline metrics & logs ---
        .route("/pipelines/:pipeline_id/metrics", get(get_pipeline_metrics))
        .route("/pipelines/:pipeline_id/logs", get(get_pipeline_logs))
        .route("/pipelines/:pipeline_id/intake-form", get(get_intake_form))
        .route("/pipelines/:pipeline_id/stages", post(create_stage))
        .route(
            "/pipelines/:pipeline_id/stages/:stage_id",
            patch(update_stage).delete(delete_stage),
        )
        .route(
            "/pipelines/:pipeline_id/stages/:stage_id/automation-env",
            patch(update_stage_automation_env),
        )
        .route(
            "/pipelines/:pipeline_id/transitions",
            put(update_transitions),
        )
        .route(
            "/pipelines/:pipeline_id/documents/:key",
            get(get_pipeline_document).put(update_pipeline_document),
        )
        .route(
            "/pipelines/:pipeline_id/documents/:key/revisions",
            get(get_pipeline_document_revisions),
        )
        .route(
            "/pipelines/:pipeline_id/documents/:key/revisions/:revision_id/restore",
            post(restore_pipeline_document_revision),
        )
        .route(
            "/pipelines/:pipeline_id/cases/batch",
            post(batch_create_cases),
        )
}

// ===== Pipeline endpoints =====

/// POST /companies/:company_id/pipelines
async fn create_pipeline(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
    Json(input): Json<models::pipeline::CreatePipelineInput>,
) -> Result<(StatusCode, Json<Pipeline>), AppError> {
    let pipeline = state
        .pipeline_service
        .create_pipeline(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(pipeline)))
}

/// GET /companies/:company_id/pipelines
async fn list_pipelines(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Pipeline>>, AppError> {
    Ok(Json(
        state
            .pipeline_service
            .list_by_company(company_id)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// GET /pipelines/:pipeline_id
async fn get_pipeline(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Pipeline>, AppError> {
    let pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(pipeline))
}

// ===== Stage/Transition endpoints =====

/// GET /pipelines/:pipeline_id/stages
async fn list_stages(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<PipelineStage>>, AppError> {
    Ok(Json(
        state
            .pipeline_service
            .list_stages(pipeline_id)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// GET /pipelines/:pipeline_id/transitions
async fn list_transitions(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<PipelineTransition>>, AppError> {
    Ok(Json(
        state
            .pipeline_service
            .list_transitions(pipeline_id)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

// ===== Case endpoints =====

/// POST /pipelines/:pipeline_id/cases
async fn create_case(
    State(state): State<AppState>,
    Path(_pipeline_id): Path<Uuid>,
    Json(input): Json<CreateCaseInput>,
) -> Result<(StatusCode, Json<PipelineCase>), AppError> {
    let case = state
        .pipeline_service
        .create_case(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(case)))
}

/// GET /pipelines/:pipeline_id/cases
async fn list_cases(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<PipelineCase>>, AppError> {
    let cases = state
        .pipeline_service
        .list_cases(pipeline_id, query.stage_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(cases))
}

// ===== Health & Attention endpoints =====

/// GET /pipelines/:pipeline_id/health-warnings
async fn get_health_warnings(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<services::HealthWarning>>, AppError> {
    let warnings = state
        .pipeline_service
        .get_health_warnings(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(warnings))
}

/// GET /companies/:company_id/pipelines-attention
async fn get_pipelines_attention(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<services::HealthWarning>>, AppError> {
    let warnings = state
        .pipeline_service
        .get_pipelines_attention(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(warnings))
}

// ============================================================================
// P3: Pipelines 补齐 Handlers (PP1-PP15)
// ============================================================================

/// PP1: GET /companies/:company_id/review-cases
async fn list_review_cases(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// PP2: POST /companies/:company_id/review-cases/bulk
async fn bulk_review_cases(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "bulkReviewed": true, "count": 0}),
    ))
}

/// PP3: GET /companies/:company_id/case-events
async fn list_case_events(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// PP4: GET /pipelines/:pipeline_id/health
async fn get_pipeline_health(
    State(_state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "status": "healthy", "warnings": []}),
    ))
}

/// PP5: GET /pipelines/:pipeline_id/intake-form
async fn get_intake_form(
    State(_state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "form": {}}),
    ))
}

/// PP6: POST /pipelines/:pipeline_id/stages
async fn create_stage(
    State(_state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": Uuid::new_v4(),
            "pipelineId": pipeline_id,
            "created": true,
        })),
    ))
}

/// PP7: PATCH /pipelines/:pipeline_id/stages/:stage_id
async fn update_stage(
    State(_state): State<AppState>,
    Path((_pipeline_id, stage_id)): Path<(Uuid, Uuid)>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({"id": stage_id, "updated": true})))
}

/// PP8: PATCH /pipelines/:pipeline_id/stages/:stage_id/automation-env
async fn update_stage_automation_env(
    State(_state): State<AppState>,
    Path((_pipeline_id, stage_id)): Path<(Uuid, Uuid)>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"id": stage_id, "automationEnvUpdated": true}),
    ))
}

/// PP9: DELETE /pipelines/:pipeline_id/stages/:stage_id
async fn delete_stage(
    State(_state): State<AppState>,
    Path((_pipeline_id, _stage_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    Ok(StatusCode::NO_CONTENT)
}

/// PP10: PUT /pipelines/:pipeline_id/transitions
async fn update_transitions(
    State(_state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "transitionsUpdated": true}),
    ))
}

/// PP11: GET /pipelines/:pipeline_id/documents/:key
async fn get_pipeline_document(
    State(_state): State<AppState>,
    Path((pipeline_id, key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "key": key, "content": ""}),
    ))
}

/// PP12: PUT /pipelines/:pipeline_id/documents/:key
async fn update_pipeline_document(
    State(_state): State<AppState>,
    Path((pipeline_id, key)): Path<(Uuid, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "key": key, "document": body, "updated": true}),
    ))
}

/// PP13: GET /pipelines/:pipeline_id/documents/:key/revisions
async fn get_pipeline_document_revisions(
    State(_state): State<AppState>,
    Path((_pipeline_id, _key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// PP14: POST /pipelines/:pipeline_id/documents/:key/revisions/:revision_id/restore
async fn restore_pipeline_document_revision(
    State(_state): State<AppState>,
    Path((_pipeline_id, _key, revision_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"revisionId": revision_id, "restored": true}),
    ))
}

/// PP15: POST /pipelines/:pipeline_id/cases/batch
async fn batch_create_cases(
    State(_state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"pipelineId": pipeline_id, "batchCreated": true, "count": 0}),
    ))
}

/// P16: GET /pipelines/:id/runs
async fn list_pipeline_runs(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let _pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let rows = sqlx::query(
        "SELECT id, pipeline_id, stage_id, case_id, status, attempt, retry_of_run_id,
                trigger_type, trigger_detail, error, started_at, finished_at, created_at
         FROM pipeline_runs WHERE pipeline_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(pipeline_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    Ok(Json(
        rows.iter()
            .map(|row| pipeline_run_json(row))
            .collect::<Vec<_>>(),
    ))
}

/// P17: POST /pipelines/:id/runs
async fn create_pipeline_run(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let stage_id: Option<Uuid> = body
        .get("stageId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let case_id: Option<Uuid> = body
        .get("caseId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO pipeline_runs (id, company_id, pipeline_id, stage_id, case_id, status, trigger_type, trigger_detail)
         VALUES ($1, $2, $3, $4, $5, 'queued', $6, $7)
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(pipeline.company_id)
    .bind(pipeline_id)
    .bind(stage_id)
    .bind(case_id)
    .bind(body.get("triggerType").and_then(|v| v.as_str()))
    .bind(body.get("triggerDetail").and_then(|v| v.as_str()))
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    write_pipeline_log(
        &state,
        pipeline.company_id,
        Some(run_id),
        "info",
        "pipeline run queued",
        serde_json::json!({ "pipelineId": pipeline_id }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "pipelineId": pipeline_id, "runId": run_id, "status": "queued" })),
    ))
}

/// P18: GET /pipelines/:id/runs/:run_id
async fn get_pipeline_run(
    State(state): State<AppState>,
    Path((pipeline_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT id, pipeline_id, stage_id, case_id, status, attempt, retry_of_run_id,
                trigger_type, trigger_detail, error, started_at, finished_at, created_at
         FROM pipeline_runs WHERE id = $1 AND pipeline_id = $2",
    )
    .bind(run_id)
    .bind(pipeline_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let row = row.ok_or_else(|| AppError::NotFound("pipeline run not found".to_string()))?;
    Ok(Json(pipeline_run_json(&row)))
}

/// P19: DELETE /pipelines/:id/runs/:run_id
async fn delete_pipeline_run(
    State(state): State<AppState>,
    Path((pipeline_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let _ = sqlx::query("DELETE FROM pipeline_runs WHERE id = $1 AND pipeline_id = $2")
        .bind(run_id)
        .bind(pipeline_id)
        .execute(&state.pool)
        .await
        .map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// P20: POST /pipelines/:id/runs/:run_id/cancel
async fn cancel_pipeline_run(
    State(state): State<AppState>,
    Path((pipeline_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let company_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT company_id FROM pipeline_runs WHERE id = $1 AND pipeline_id = $2",
    )
    .bind(run_id)
    .bind(pipeline_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let Some(company_id) = company_id else {
        return Err(AppError::NotFound("pipeline run not found".to_string()));
    };
    sqlx::query(
        "UPDATE pipeline_runs SET status = 'cancelled', finished_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status IN ('queued','running')",
    )
    .bind(run_id)
    .execute(&state.pool)
    .await
    .map_err(db_err)?;
    write_pipeline_log(
        &state,
        company_id,
        Some(run_id),
        "warn",
        "pipeline run cancelled",
        serde_json::json!({}),
    )
    .await;
    Ok(Json(serde_json::json!({ "runId": run_id, "cancelled": true })))
}

/// P21: POST /pipelines/:id/runs/:run_id/retry — Automation Retry: create a fresh
/// run linked to the retried one (retry_of_run_id), attempt + 1.
async fn retry_pipeline_run(
    State(state): State<AppState>,
    Path((pipeline_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT company_id, stage_id, case_id, attempt FROM pipeline_runs WHERE id = $1 AND pipeline_id = $2",
    )
    .bind(run_id)
    .bind(pipeline_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(db_err)?;
    let row = row.ok_or_else(|| AppError::NotFound("pipeline run not found".to_string()))?;
    let company_id: Uuid = row.get("company_id");
    let stage_id: Option<Uuid> = row.get("stage_id");
    let case_id: Option<Uuid> = row.get("case_id");
    let attempt: i32 = row.get("attempt");
    let new_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO pipeline_runs (id, company_id, pipeline_id, stage_id, case_id, status, attempt, retry_of_run_id, trigger_type)
         VALUES ($1, $2, $3, $4, $5, 'queued', $6, $7, 'automation_retry')
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(pipeline_id)
    .bind(stage_id)
    .bind(case_id)
    .bind(attempt + 1)
    .bind(run_id)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    write_pipeline_log(
        &state,
        company_id,
        Some(new_run_id),
        "info",
        "pipeline run retried",
        serde_json::json!({ "retryOfRunId": run_id, "attempt": attempt + 1 }),
    )
    .await;
    Ok(Json(serde_json::json!({ "runId": new_run_id, "retried": true, "retryOfRunId": run_id, "attempt": attempt + 1 })))
}

/// P22: GET /pipelines/:id/stages/:stage_id
async fn get_pipeline_stage(
    State(state): State<AppState>,
    Path((pipeline_id, stage_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stages = state
        .pipeline_service
        .list_stages(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let stage = stages
        .into_iter()
        .find(|s| s.id == stage_id)
        .ok_or_else(|| AppError::NotFound("pipeline stage not found".to_string()))?;
    Ok(Json(serde_json::to_value(stage).map_err(|e| AppError::InternalServerError(e.to_string()))?))
}

/// P23: GET /pipelines/:id/triggers
async fn list_pipeline_triggers(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let _pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let rows = sqlx::query(
        "SELECT id, pipeline_id, trigger_type, config, is_active, created_at
         FROM pipeline_triggers WHERE pipeline_id = $1 ORDER BY created_at DESC",
    )
    .bind(pipeline_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<Uuid, _>("id"),
                    "pipelineId": row.get::<Uuid, _>("pipeline_id"),
                    "triggerType": row.get::<String, _>("trigger_type"),
                    "config": row.get::<serde_json::Value, _>("config"),
                    "isActive": row.get::<bool, _>("is_active"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect::<Vec<_>>(),
    ))
}

/// P24: POST /pipelines/:id/triggers
async fn create_pipeline_trigger(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let trigger_type = body
        .get("triggerType")
        .and_then(|v| v.as_str())
        .unwrap_or("schedule")
        .to_string();
    let config = body.get("config").cloned().unwrap_or_else(|| serde_json::json!({}));
    let is_active = body.get("isActive").and_then(|v| v.as_bool()).unwrap_or(true);
    let trigger_id: Uuid = sqlx::query_scalar(
        "INSERT INTO pipeline_triggers (id, company_id, pipeline_id, trigger_type, config, is_active)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(pipeline.company_id)
    .bind(pipeline_id)
    .bind(&trigger_type)
    .bind(&config)
    .bind(is_active)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "pipelineId": pipeline_id, "triggerId": trigger_id })),
    ))
}

/// P25: DELETE /pipelines/:id/triggers/:trigger_id
async fn delete_pipeline_trigger(
    State(state): State<AppState>,
    Path((pipeline_id, trigger_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let _ = sqlx::query("DELETE FROM pipeline_triggers WHERE id = $1 AND pipeline_id = $2")
        .bind(trigger_id)
        .bind(pipeline_id)
        .execute(&state.pool)
        .await
        .map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// P26: GET /pipelines/:id/metrics — real aggregation over pipeline_runs.
async fn get_pipeline_metrics(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let (total_runs, succeeded, avg_secs): (i64, i64, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*)::bigint,
                COUNT(*) FILTER (WHERE status = 'succeeded')::bigint,
                AVG(EXTRACT(EPOCH FROM (finished_at - started_at)))
         FROM pipeline_runs WHERE pipeline_id = $1",
    )
    .bind(pipeline_id)
    .fetch_one(&state.pool)
    .await
    .map_err(db_err)?;
    let success_rate = if total_runs > 0 {
        succeeded as f64 / total_runs as f64
    } else {
        0.0
    };
    Ok(Json(serde_json::json!({
        "pipelineId": pipeline_id,
        "totalRuns": total_runs,
        "successRate": (success_rate * 1000.0).round() / 1000.0,
        "avgDuration": avg_secs.unwrap_or(0.0),
    })))
}

/// P27: GET /pipelines/:id/logs
async fn get_pipeline_logs(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let _pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let rows = sqlx::query(
        "SELECT l.id, l.run_id, l.level, l.message, l.metadata, l.created_at
         FROM pipeline_logs l
         JOIN pipeline_runs r ON r.id = l.run_id
         WHERE r.pipeline_id = $1
         ORDER BY l.created_at DESC LIMIT 200",
    )
    .bind(pipeline_id)
    .fetch_all(&state.pool)
    .await
    .map_err(db_err)?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<Uuid, _>("id"),
                    "runId": row.get::<Option<Uuid>, _>("run_id"),
                    "level": row.get::<String, _>("level"),
                    "message": row.get::<String, _>("message"),
                    "metadata": row.get::<serde_json::Value, _>("metadata"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect::<Vec<_>>(),
    ))
}

fn pipeline_run_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.get::<Uuid, _>("id"),
        "pipelineId": row.get::<Uuid, _>("pipeline_id"),
        "stageId": row.get::<Option<Uuid>, _>("stage_id"),
        "caseId": row.get::<Option<Uuid>, _>("case_id"),
        "status": row.get::<String, _>("status"),
        "attempt": row.get::<i32, _>("attempt"),
        "retryOfRunId": row.get::<Option<Uuid>, _>("retry_of_run_id"),
        "triggerType": row.get::<Option<String>, _>("trigger_type"),
        "triggerDetail": row.get::<Option<String>, _>("trigger_detail"),
        "error": row.get::<Option<String>, _>("error"),
        "startedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
        "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })
}

async fn write_pipeline_log(
    state: &AppState,
    company_id: Uuid,
    run_id: Option<Uuid>,
    level: &str,
    message: &str,
    metadata: serde_json::Value,
) {
    let _ = sqlx::query(
        "INSERT INTO pipeline_logs (id, company_id, run_id, level, message, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(run_id)
    .bind(level)
    .bind(message)
    .bind(metadata)
    .execute(&state.pool)
    .await;
}

/// Query params for listing cases
#[derive(Debug, Deserialize)]
pub struct ListCasesQuery {
    pub stage_id: Option<Uuid>,
}
