//! Project routes — CRUD + workspaces + memberships
//!
//! 对应 Company/Org 模块任务 §1.2 ~ §1.3 + §10 API 路由层

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, put},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::{
    CreateProjectInput, CreateWorkspaceInput, MembershipState, Project, ProjectMembership,
    ProjectWorkspace, ResourceMemberships, UpdateProjectInput,
};

pub fn project_routes() -> Router<AppState> {
    Router::new()
        // Project list + create (scoped to company)
        .route(
            "/companies/:company_id/projects",
            get(list_projects).post(create_project),
        )
        // Single project
        .route(
            "/projects/:project_id",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        // Workspaces
        .route(
            "/projects/:project_id/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/projects/:project_id/workspaces/:workspace_id",
            patch(update_workspace).delete(delete_workspace),
        )
        // External object summary
        .route(
            "/projects/:project_id/external-object-summary",
            get(get_external_object_summary),
        )
        // Resource memberships
        .route(
            "/companies/:company_id/resource-memberships/me",
            get(list_my_memberships),
        )
        .route(
            "/companies/:company_id/resource-memberships/me/projects/:project_id",
            put(update_project_membership),
        )
        .route(
            "/companies/:company_id/resource-memberships/me/agents/:agent_id",
            put(update_agent_membership),
        )
}

// ===== Project endpoints =====

/// GET /companies/:company_id/projects
async fn list_projects(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let projects = state
        .project_service
        .list_by_company(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let mut result = Vec::with_capacity(projects.len());
    for project in projects {
        result.push(hydrate_project(&state, project).await?);
    }
    Ok(Json(result))
}

async fn hydrate_project(state: &AppState, project: Project) -> Result<serde_json::Value, AppError> {
    let workspaces: Vec<ProjectWorkspace> = sqlx::query_as(
        "SELECT id, project_id, name, config, is_primary, created_at, updated_at FROM project_workspaces WHERE project_id = $1 ORDER BY is_primary DESC, created_at ASC",
    )
    .bind(project.id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load project workspaces: {e}")))?;
    let primary = workspaces.iter().find(|w| w.is_primary).cloned();
    let mut value = serde_json::to_value(project).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("workspaces".into(), serde_json::to_value(&workspaces).unwrap_or_default());
        object.insert("primaryWorkspace".into(), serde_json::to_value(primary).unwrap_or(serde_json::Value::Null));
    }
    Ok(value)
}

/// POST /companies/:company_id/projects
async fn create_project(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<CreateProjectInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    input.company_id = company_id;
    let project = state
        .project_service
        .create(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(hydrate_project(&state, project).await?)))
}

/// GET /projects/:project_id
async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project = state
        .project_service
        .get_by_id(project_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", project_id)))?;
    Ok(Json(hydrate_project(&state, project).await?))
}

/// PATCH /projects/:project_id
async fn update_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<UpdateProjectInput>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project = state
        .project_service
        .update(project_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(hydrate_project(&state, project).await?))
}

/// DELETE /projects/:project_id
async fn delete_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .project_service
        .delete(project_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

// ===== Workspace endpoints =====

/// GET /projects/:project_id/workspaces
async fn list_workspaces(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<ProjectWorkspace>>, AppError> {
    let workspaces = state
        .project_service
        .list_workspaces(project_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(workspaces))
}

/// POST /projects/:project_id/workspaces
async fn create_workspace(
    State(state): State<AppState>,
    Path(_project_id): Path<Uuid>,
    Json(input): Json<CreateWorkspaceInput>,
) -> Result<(StatusCode, Json<ProjectWorkspace>), AppError> {
    let workspace = state
        .project_service
        .create_workspace(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

/// PATCH /projects/:project_id/workspaces/:workspace_id
async fn update_workspace(
    State(state): State<AppState>,
    Path((project_id, workspace_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ProjectWorkspace>, AppError> {
    let name = body.get("name").and_then(|v| v.as_str()).map(str::to_owned);
    let config = body.get("config").cloned();
    let is_primary = body
        .get("isPrimary")
        .and_then(|v| v.as_bool())
        .or_else(|| body.get("is_primary").and_then(|v| v.as_bool()));
    let workspace = state
        .project_service
        .update_workspace(project_id, workspace_id, name, config, is_primary)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| {
            AppError::NotFound(format!("Project workspace {} not found", workspace_id))
        })?;
    Ok(Json(workspace))
}

/// DELETE /projects/:project_id/workspaces/:workspace_id
async fn delete_workspace(
    State(state): State<AppState>,
    Path((_project_id, workspace_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .project_service
        .delete_workspace(workspace_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

// ===== External object summary =====

/// GET /projects/:project_id/external-object-summary
async fn get_external_object_summary(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let summary = state
        .project_service
        .external_object_summary(project_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(summary))
}

// ===== Resource membership endpoints =====

/// GET /companies/:company_id/resource-memberships/me
async fn list_my_memberships(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<ResourceMemberships>, AppError> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();
    let memberships = state
        .project_service
        .list_memberships_for_user(company_id, user_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(memberships))
}

/// PUT /companies/:company_id/resource-memberships/me/projects/:project_id
async fn update_project_membership(
    State(state): State<AppState>,
    Path((company_id, project_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ProjectMembership>, AppError> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();
    let state_val = body
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("joined");
    let membership_state = match state_val {
        "left" => MembershipState::Left,
        _ => MembershipState::Joined,
    };

    let membership = state
        .project_service
        .update_project_membership(company_id, project_id, user_id, membership_state)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(membership))
}

/// PUT /companies/:company_id/resource-memberships/me/agents/:agent_id
///
/// Mirrors Paperclip `resourceMembershipRoutes` -> `svc.updateAgent`. The
/// response shape drops the internal `changed/changeKind/policySource` fields
/// and returns `{ resourceType, resourceId, state, starredAt, updatedAt }`.
async fn update_agent_membership(
    State(state): State<AppState>,
    Path((company_id, agent_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Extract user_id from auth context
    let user_id = Uuid::nil();
    let state_val = body
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("joined");
    let membership_state = match state_val {
        "left" => MembershipState::Left,
        _ => MembershipState::Joined,
    };
    let starred = body.get("starred").and_then(|v| v.as_bool());

    let membership = state
        .project_service
        .update_agent_membership(company_id, agent_id, user_id, membership_state, starred)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    // Project to the Paperclip response shape (camelCase, stripped internals).
    Ok(Json(serde_json::json!({
        "resourceType": "agent",
        "resourceId": membership.agent_id,
        "state": membership.state,
        "starredAt": membership.starred_at,
        "updatedAt": membership.updated_at,
    })))
}
