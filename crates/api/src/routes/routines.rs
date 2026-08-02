//! Routine routes — CRUD + trigger + run management

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use services::auth::AuthorizationActor;
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::routine::{Routine, RoutineRun, RoutineTriggerConfig};

pub fn routine_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/folders", get(list_company_folders))
        // Routine CRUD
        .route(
            "/companies/:company_id/routines",
            get(list_routines).post(create_routine),
        )
        .route(
            "/routines/:routine_id",
            get(get_routine)
                .patch(update_routine)
                .delete(delete_routine),
        )
        .route("/routines/:routine_id/pause", post(pause_routine))
        .route("/routines/:routine_id/resume", post(resume_routine))
        .route("/routines/:routine_id/trigger", post(trigger_routine))
        // Runs
        .route("/routines/:routine_id/runs", get(list_runs))
        .route("/runs/:run_id", get(get_run))
        // --- P3: Routines 补齐 (GR1-GR9) ---
        .route(
            "/routines/:routine_id/revisions",
            get(list_routine_revisions),
        )
        .route(
            "/routines/:routine_id/revisions/:revision_id/restore",
            post(restore_routine_revision),
        )
        .route(
            "/routines/:routine_id/triggers",
            get(list_routine_triggers).post(create_routine_trigger),
        )
        .route(
            "/routine-triggers/:trigger_id",
            patch(update_routine_trigger).delete(delete_routine_trigger),
        )
        .route(
            "/routine-triggers/:trigger_id/rotate-secret",
            post(rotate_trigger_secret),
        )
        .route(
            "/routine-triggers/public/:public_id/fire",
            post(fire_public_trigger),
        )
        .route("/routines/:routine_id/run", post(trigger_routine_run))
}

#[derive(Debug, Deserialize)]
struct FolderQuery {
    kind: String,
}

/// Paperclip GET /companies/:companyId/folders.
/// Folder rows and item counts come from the database; no folder names are synthesized.
async fn list_company_folders(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<FolderQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    if query.kind != "routine" && query.kind != "skill" {
        return Err(AppError::BadRequest(
            "Folder kind query parameter is required".into(),
        ));
    }

    let rows = sqlx::query(
        r#"WITH RECURSIVE folder_tree AS (
             SELECT id, company_id, kind, parent_id, name, slug, system_key, color,
                    position, created_at, updated_at, 0::int AS depth, name::text AS path
             FROM folders
             WHERE company_id = $1 AND kind = $2 AND parent_id IS NULL
             UNION ALL
             SELECT f.id, f.company_id, f.kind, f.parent_id, f.name, f.slug, f.system_key,
                    f.color, f.position, f.created_at, f.updated_at, t.depth + 1,
                    (t.path || ' / ' || f.name)::text
             FROM folders f JOIN folder_tree t ON f.parent_id = t.id
             WHERE f.company_id = $1 AND f.kind = $2
           )
           SELECT t.*, CASE WHEN $2 = 'routine'
             THEN (SELECT COUNT(*) FROM routines r WHERE r.company_id = $1 AND r.folder_id = t.id)
             ELSE (SELECT COUNT(*) FROM company_skills s WHERE s.company_id = $1 AND s.folder_id = t.id)
             END::bigint AS item_count
           FROM folder_tree t
           ORDER BY t.position, t.name, t.id"#,
    )
    .bind(company_id)
    .bind(&query.kind)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to list folders: {e}")))?;

    let folders: Vec<_> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "companyId": r.get::<Uuid, _>("company_id"),
                "kind": r.get::<String, _>("kind"),
                "parentId": r.get::<Option<Uuid>, _>("parent_id"),
                "name": r.get::<String, _>("name"),
                "slug": r.get::<String, _>("slug"),
                "systemKey": r.get::<Option<String>, _>("system_key"),
                "path": r.get::<String, _>("path"),
                "depth": r.get::<i32, _>("depth"),
                "color": r.get::<Option<String>, _>("color"),
                "position": r.get::<i32, _>("position"),
                "itemCount": r.get::<i64, _>("item_count"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
            })
        })
        .collect();
    let all_count: i64 = if query.kind == "routine" {
        sqlx::query_scalar("SELECT COUNT(*) FROM routines WHERE company_id = $1")
            .bind(company_id)
            .fetch_one(&state.pool)
            .await
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM company_skills WHERE company_id = $1")
            .bind(company_id)
            .fetch_one(&state.pool)
            .await
    }
    .map_err(|e| AppError::InternalServerError(format!("Failed to count folder items: {e}")))?;
    let unfiled_count: i64 = if query.kind == "routine" {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM routines WHERE company_id = $1 AND folder_id IS NULL",
        )
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM company_skills WHERE company_id = $1 AND folder_id IS NULL",
        )
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
    }
    .map_err(|e| AppError::InternalServerError(format!("Failed to count unfiled items: {e}")))?;

    Ok(Json(
        serde_json::json!({ "kind": query.kind, "folders": folders, "allCount": all_count, "unfiledCount": unfiled_count }),
    ))
}

/// POST /companies/:company_id/routines
async fn create_routine(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Routine>), AppError> {
    let agent_id: Uuid = body
        .get("agent_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("agent_id is required".to_string()))?;
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = body
        .get("description")
        .and_then(|v| v.as_str().map(String::from));
    let trigger_config: RoutineTriggerConfig = serde_json::from_value(
        body.get("trigger_config")
            .cloned()
            .unwrap_or(serde_json::json!({})),
    )
    .map_err(|e| AppError::BadRequest(format!("Invalid trigger_config: {}", e)))?;

    let creator_user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        AuthorizationActor::Agent { .. } | AuthorizationActor::None => {
            return Err(AppError::Forbidden(
                "A board user is required to create a routine".to_string(),
            ));
        }
    };
    let routine = state
        .routine_service
        .create_routine(
            company_id,
            agent_id,
            name,
            description,
            trigger_config,
            creator_user_id,
        )
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(routine)))
}

/// GET /companies/:company_id/routines
async fn list_routines(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Routine>>, AppError> {
    let routines = state
        .routine_service
        .list_routines(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routines))
}

/// GET /routines/:routine_id
async fn get_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .get_by_id(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// PATCH /routines/:routine_id
async fn update_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Routine>, AppError> {
    let name = body.get("name").and_then(|v| v.as_str().map(String::from));
    let description = body
        .get("description")
        .and_then(|v| v.as_str().map(String::from));
    let routine = state
        .routine_service
        .update_routine(routine_id, name, description)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// DELETE /routines/:routine_id
async fn delete_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .routine_service
        .delete_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /routines/:routine_id/pause
async fn pause_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .pause_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// POST /routines/:routine_id/resume
async fn resume_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .resume_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// POST /routines/:routine_id/trigger
async fn trigger_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<RoutineRun>, AppError> {
    let run = state
        .routine_service
        .trigger_routine(routine_id, "manual".to_string())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(run))
}

/// GET /routines/:routine_id/runs
async fn list_runs(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<RoutineRun>>, AppError> {
    let runs = state
        .routine_service
        .list_runs(routine_id, 50)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(runs))
}

/// GET /runs/:run_id
async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RoutineRun>, AppError> {
    let run = state
        .routine_service
        .get_run(run_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Run {} not found", run_id)))?;
    Ok(Json(run))
}

// ============================================================================
// P3: Routines 补齐 Handlers (GR1-GR9)
// ============================================================================

/// GR1: GET /routines/:routine_id/revisions
async fn list_routine_revisions(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let revisions = sqlx::query_as::<_, (Uuid, Uuid, i32, String, Option<String>, serde_json::Value, chrono::DateTime<chrono::Utc>)>("SELECT id, routine_id, revision_number, title, description, snapshot, created_at FROM routine_revisions WHERE routine_id = $1 ORDER BY revision_number DESC")
        .bind(routine_id).fetch_all(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(revisions.into_iter().map(|(id, routine_id, version, title, description, snapshot, created_at)| serde_json::json!({"id": id, "routineId": routine_id, "version": version, "title": title, "description": description, "snapshot": snapshot, "createdAt": created_at})).collect()))
}

/// GR2: POST /routines/:routine_id/revisions/:revision_id/restore
async fn restore_routine_revision(
    State(state): State<AppState>,
    Path((routine_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let snapshot: serde_json::Value = sqlx::query_scalar(
        "SELECT snapshot FROM routine_revisions WHERE id = $1 AND routine_id = $2",
    )
    .bind(revision_id)
    .bind(routine_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("Routine revision not found".to_string()))?;
    sqlx::query("UPDATE routines SET latest_revision_id=$2, latest_revision_number=(SELECT revision_number FROM routine_revisions WHERE id=$2), title=COALESCE($3,title), description=COALESCE($4,description), updated_at=NOW() WHERE id=$1")
        .bind(routine_id).bind(revision_id).bind(snapshot.get("title").and_then(|v| v.as_str())).bind(snapshot.get("description").and_then(|v| v.as_str())).execute(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(
        serde_json::json!({"routineId": routine_id, "revisionId": revision_id, "restored": true}),
    ))
}

/// GR3: GET /routines/:routine_id/triggers
async fn list_routine_triggers(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let triggers = sqlx::query_as::<_, (Uuid, Uuid, String, Option<String>, bool, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>("SELECT id, routine_id, kind::text, label, enabled, cron_expression, timezone, public_id, next_run_at, last_fired_at FROM routine_triggers WHERE routine_id=$1 ORDER BY created_at ASC")
        .bind(routine_id).fetch_all(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(triggers.into_iter().map(|(id, routine_id, kind, label, enabled, cron, timezone, public_id, next_run_at, last_fired_at)| serde_json::json!({"id": id, "routineId": routine_id, "triggerType": kind, "label": label, "enabled": enabled, "cronExpression": cron, "timezone": timezone, "publicId": public_id, "nextRunAt": next_run_at, "lastFiredAt": last_fired_at})).collect()))
}

/// GR4: POST /routines/:routine_id/triggers
async fn create_routine_trigger(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM routines WHERE id=$1")
        .bind(routine_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Routine not found".to_string()))?;
    let kind = payload
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("manual");
    let trigger_id: Uuid = sqlx::query_scalar("INSERT INTO routine_triggers (company_id, routine_id, kind, label, enabled, cron_expression, timezone, public_id, secret_id) VALUES ($1,$2,$3::trigger_kind,$4,$5,$6,$7,$8,$9) RETURNING id")
        .bind(company_id).bind(routine_id).bind(kind).bind(payload.get("label").and_then(|v| v.as_str())).bind(payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)).bind(payload.get("cronExpression").and_then(|v| v.as_str())).bind(payload.get("timezone").and_then(|v| v.as_str())).bind(if kind == "webhook" { Some(format!("rt_{}", Uuid::new_v4().simple())) } else { None::<String> }).bind(None::<String>).fetch_one(&state.pool).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"id": trigger_id, "routineId": routine_id, "trigger": payload})),
    ))
}

/// GR5: PATCH /routine-triggers/:trigger_id
async fn update_routine_trigger(
    State(state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, bool, Option<String>, Option<String>, Option<String>)>("UPDATE routine_triggers SET enabled=COALESCE($2,enabled), label=COALESCE($3,label), cron_expression=COALESCE($4,cron_expression), timezone=COALESCE($5,timezone), updated_at=NOW() WHERE id=$1 RETURNING id, routine_id, enabled, label, cron_expression, timezone")
        .bind(trigger_id).bind(payload.get("enabled").and_then(|v| v.as_bool())).bind(payload.get("label").and_then(|v| v.as_str())).bind(payload.get("cronExpression").and_then(|v| v.as_str())).bind(payload.get("timezone").and_then(|v| v.as_str())).fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("Routine trigger not found".to_string()))?;
    Ok(Json(
        serde_json::json!({"id": row.0, "routineId": row.1, "enabled": row.2, "label": row.3, "cronExpression": row.4, "timezone": row.5}),
    ))
}

/// GR6: DELETE /routine-triggers/:trigger_id
async fn delete_routine_trigger(
    State(state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM routine_triggers WHERE id=$1")
        .bind(trigger_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Routine trigger not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GR7: POST /routine-triggers/:trigger_id/rotate-secret
async fn rotate_trigger_secret(
    State(state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let secret = format!("rts_{}", Uuid::new_v4().simple());
    let row = sqlx::query_as::<_, (Uuid, String)>("UPDATE routine_triggers SET secret_id=$2, last_rotated_at=NOW(), updated_at=NOW() WHERE id=$1 RETURNING id, secret_id").bind(trigger_id).bind(&secret).fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("Routine trigger not found".to_string()))?;
    Ok(Json(
        serde_json::json!({"id": row.0, "secret": row.1, "rotated": true}),
    ))
}

/// GR8: POST /routine-triggers/public/:public_id/fire
async fn fire_public_trigger(
    State(state): State<AppState>,
    Path(public_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let routine_id: Uuid = sqlx::query_scalar("UPDATE routine_triggers SET last_fired_at=NOW(), updated_at=NOW() WHERE public_id=$1 AND enabled=true RETURNING routine_id").bind(public_id).fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("Public routine trigger not found".to_string()))?;
    let run = state
        .routine_service
        .trigger_routine(routine_id, "webhook".to_string())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(
        serde_json::json!({"publicId": public_id, "fired": true, "run": run}),
    ))
}

/// GR9: POST /routines/:routine_id/run
async fn trigger_routine_run(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<RoutineRun>, AppError> {
    let run = state
        .routine_service
        .trigger_routine(routine_id, "manual".to_string())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(run))
}
