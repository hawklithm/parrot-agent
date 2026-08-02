use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use models::{CreateEnvironmentInput, UpdateEnvironmentInput};
use services::auth::AuthorizationActor;

pub fn environment_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/environments",
            get(list_environments_v2).post(create_environment_v2),
        )
        .route(
            "/environments/:id",
            get(get_environment_v2)
                .patch(update_environment_v2)
                .delete(delete_environment_v2),
        )
        .route("/environments/:id/probe", post(probe_environment_v2))
        // --- P1: Environment 补齐 (E11-E24) ---
        .route(
            "/companies/:company_id/environments/capabilities",
            get(get_environment_capabilities),
        )
        .route(
            "/companies/:company_id/environments/probe-config",
            post(probe_environment_config),
        )
        .route(
            "/environments/:id/delete-blast-radius",
            get(get_delete_blast_radius),
        )
        .route(
            "/environments/:environment_id/custom-image-template",
            get(get_custom_image_template).delete(delete_custom_image_template),
        )
        .route(
            "/environments/:environment_id/custom-image-template/rollback",
            post(rollback_custom_image_template),
        )
        .route(
            "/environments/:environment_id/custom-image-setup-sessions",
            post(create_custom_image_setup_session),
        )
        .route(
            "/environment-custom-image-setup-sessions/:id/finish",
            post(finish_custom_image_setup_session),
        )
        .route(
            "/environment-custom-image-setup-sessions/:id/cancel",
            post(cancel_custom_image_setup_session),
        )
        .route("/environment-leases/:lease_id", get(get_environment_lease))
}

// ===== V2 Handlers (AppState-based) =====

async fn list_environments_v2(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> impl IntoResponse {
    match state
        .environment_service
        .list_by_status(models::execution_environment::EnvironmentStatus::Active)
        .await
    {
        Ok(environments) => (StatusCode::OK, Json(environments)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn create_environment_v2(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateEnvironmentInput>,
) -> impl IntoResponse {
    match state.environment_service.create(company_id, input).await {
        Ok(env) => (StatusCode::CREATED, Json(env)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn get_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_service.get(id).await {
        Ok(env) => (StatusCode::OK, Json(env)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, Json(json!({"error": msg}))).into_response()
            }
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response(),
        },
    }
}

async fn update_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEnvironmentInput>,
) -> impl IntoResponse {
    match state.environment_service.update(id, input).await {
        Ok(env) => (StatusCode::OK, Json(env)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_service.delete(id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn probe_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_diagnostics_service.probe(id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

// ===== P1: Environment 补齐 Handlers (E11-E24) =====

/// E11: GET /companies/:company_id/environments/capabilities
async fn get_environment_capabilities(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .environment_service
        .get_capabilities(company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E12: POST /companies/:company_id/environments/probe-config
async fn probe_environment_config(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .environment_service
        .probe_config(company_id, payload)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E16: GET /environments/:id/delete-blast-radius
async fn get_delete_blast_radius(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .environment_service
        .get_delete_blast_radius(id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E17: GET /environments/:environment_id/custom-image-template
async fn get_custom_image_template(
    State(state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let active = sqlx::query_scalar::<_, serde_json::Value>("SELECT json_build_object('id', id, 'environmentId', environment_id, 'provider', provider, 'templateKind', template_kind, 'templateRef', template_ref, 'sourceTemplateRef', source_template_ref, 'status', status, 'supersededByTemplateId', superseded_by_template_id, 'createdByUserId', created_by_user_id, 'createdByAgentId', created_by_agent_id, 'capturedAt', captured_at, 'metadata', metadata, 'lastUsedAt', last_used_at, 'createdAt', created_at, 'updatedAt', updated_at) FROM environment_custom_image_templates WHERE environment_id = $1 AND status = 'active'")
        .bind(environment_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let active_session = sqlx::query_scalar::<_, Uuid>("SELECT id FROM environment_custom_image_setup_sessions WHERE environment_id = $1 AND status IN ('pending','running','starting','waiting_for_user','capturing') ORDER BY created_at DESC LIMIT 1")
        .bind(environment_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let latest_session = sqlx::query_scalar::<_, Uuid>("SELECT id FROM environment_custom_image_setup_sessions WHERE environment_id = $1 ORDER BY created_at DESC LIMIT 1")
        .bind(environment_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        json!({"activeTemplate": active, "activeSession": load_session_json(&state, active_session).await?, "latestSession": load_session_json(&state, latest_session).await?}),
    ))
}

/// E18: DELETE /environments/:environment_id/custom-image-template
async fn delete_custom_image_template(
    State(state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("UPDATE environment_custom_image_templates SET status = 'disabled', updated_at = NOW() WHERE environment_id = $1 AND status = 'active'")
        .bind(environment_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// E19: POST /environments/:environment_id/custom-image-template/rollback
async fn rollback_custom_image_template(
    State(state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let active: Option<Uuid> = sqlx::query_scalar("SELECT id FROM environment_custom_image_templates WHERE environment_id = $1 AND status = 'active' LIMIT 1").bind(environment_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let previous: Option<Uuid> = sqlx::query_scalar("SELECT id FROM environment_custom_image_templates WHERE environment_id = $1 AND status = 'superseded' ORDER BY captured_at DESC LIMIT 1").bind(environment_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (Some(active), Some(previous)) = (active, previous) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE environment_custom_image_templates SET status='superseded', updated_at=NOW() WHERE id=$1").bind(active).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE environment_custom_image_templates SET status='active', superseded_by_template_id=NULL, updated_at=NOW() WHERE id=$1").bind(previous).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let active_template = template_json(&state, previous).await?;
    let superseded_template = template_json(&state, active).await?;
    Ok(Json(
        json!({"activeTemplate": active_template, "supersededTemplate": superseded_template}),
    ))
}

/// E20: POST /environments/:environment_id/custom-image-setup-sessions
async fn create_custom_image_setup_session(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(environment_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    };
    let environment = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT driver::text, config FROM environments WHERE id = $1",
    )
    .bind(environment_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let provider = body
        .get("provider")
        .and_then(|v| v.as_str())
        .or_else(|| environment.1.get("provider").and_then(|v| v.as_str()))
        .unwrap_or(&environment.0)
        .to_string();
    let template_id = body
        .get("templateId")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok());
    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(
            body.get("ttlSeconds")
                .and_then(|v| v.as_i64())
                .unwrap_or(7200)
                .clamp(60, 86400),
        );
    let metadata = body.get("metadata").cloned().unwrap_or_else(|| json!({}));
    let connection_payload = body.get("connectionPayload").cloned();
    let id: Uuid = sqlx::query_scalar("INSERT INTO environment_custom_image_setup_sessions (environment_id, template_id, provider, status, started_by_user_id, expires_at, metadata, connection_payload) VALUES ($1,$2,$3,'running',$4,$5,$6,$7) RETURNING id")
        .bind(environment_id).bind(template_id).bind(provider).bind(user_id).bind(expires_at).bind(metadata).bind(connection_payload).fetch_one(&state.pool).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(
        json!({"session": load_session_json(&state, Some(id)).await?, "connectionPayload": body.get("connectionPayload").cloned()}),
    ))
}

/// E21: GET /environment-custom-image-setup-sessions/:id/finish
async fn finish_custom_image_setup_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session = sqlx::query_as::<_, (Uuid, String, Option<String>)>("SELECT environment_id, provider, base_template_ref FROM environment_custom_image_setup_sessions WHERE id = $1 AND status IN ('running','waiting_for_user','starting')")
        .bind(id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let template_ref = body
        .get("templateRef")
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("metadata")
                .and_then(|v| v.get("templateRef"))
                .and_then(|v| v.as_str())
        })
        .ok_or(StatusCode::BAD_REQUEST)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE environment_custom_image_templates SET status='superseded', updated_at=NOW() WHERE environment_id=$1 AND status='active'")
        .bind(session.0).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let template_id: Uuid = sqlx::query_scalar("INSERT INTO environment_custom_image_templates (environment_id, provider, template_ref, source_template_ref, created_by_user_id, metadata) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(session.0).bind(&session.1).bind(template_ref).bind(session.2).bind(body.get("userId").and_then(|v| v.as_str())).bind(body.get("metadata").cloned()).fetch_one(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE environment_custom_image_setup_sessions SET status='completed', promoted_template_id=$2, finished_at=NOW(), updated_at=NOW() WHERE id=$1").bind(id).bind(template_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        json!({"session": load_session_json(&state, Some(id)).await?, "template": template_json(&state, template_id).await?}),
    ))
}

/// E22: POST /environment-custom-image-setup-sessions/:id/cancel
async fn cancel_custom_image_setup_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = sqlx::query("UPDATE environment_custom_image_setup_sessions SET status='cancelled', failure_reason=$2, updated_at=NOW() WHERE id=$1 AND status IN ('pending','running','starting','waiting_for_user','capturing')")
        .bind(id).bind(body.get("reason").and_then(|v| v.as_str()).unwrap_or("cancelled")).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(
        load_session_json(&state, Some(id))
            .await?
            .unwrap_or_else(|| json!({})),
    ))
}

/// E23: GET /environment-leases/:lease_id
async fn get_environment_lease(
    State(_state): State<AppState>,
    Path(_lease_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn load_session_json(
    state: &AppState,
    id: Option<Uuid>,
) -> Result<Option<serde_json::Value>, StatusCode> {
    let Some(id) = id else {
        return Ok(None);
    };
    let session = state
        .custom_image_setup_service
        .get_session(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Some(
        serde_json::to_value(session).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

async fn template_json(state: &AppState, id: Uuid) -> Result<serde_json::Value, StatusCode> {
    sqlx::query_scalar::<_, serde_json::Value>("SELECT json_build_object('id', id, 'environmentId', environment_id, 'provider', provider, 'templateKind', template_kind, 'templateRef', template_ref, 'sourceTemplateRef', source_template_ref, 'status', status, 'supersededByTemplateId', superseded_by_template_id, 'createdByUserId', created_by_user_id, 'createdByAgentId', created_by_agent_id, 'capturedAt', captured_at, 'metadata', metadata, 'lastUsedAt', last_used_at, 'createdAt', created_at, 'updatedAt', updated_at) FROM environment_custom_image_templates WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}
