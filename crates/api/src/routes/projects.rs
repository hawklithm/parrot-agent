//! Project routes — CRUD + workspaces + memberships
//!
//! 对应 Company/Org 模块任务 §1.2 ~ §1.3 + §10 API 路由层

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::{
    Project, ProjectWorkspace, ProjectMembership, ResourceMemberships,
    CreateProjectInput, UpdateProjectInput, CreateWorkspaceInput,
    MembershipState,
};
use services::ProjectService;

pub fn project_routes() -> Router<AppState> {
    Router::new()
        // Project list + create (scoped to company)
        .route("/companies/:company_id/projects", get(list_projects).post(create_project))
        // Single project
        .route("/projects/:project_id", get(get_project).patch(update_project).delete(delete_project))
        // Workspaces
        .route("/projects/:project_id/workspaces", get(list_workspaces).post(create_workspace))
        .route("/projects/:project_id/workspaces/:workspace_id", patch(update_workspace).delete(delete_workspace))
        // External object summary
        .route("/projects/:project_id/external-object-summary", get(get_external_object_summary))
        // Resource memberships
        .route("/companies/:company_id/resource-memberships/me", get(list_my_memberships))
        .route("/companies/:company_id/resource-memberships/me/projects/:project_id", put(update_project_membership))
}

// ===== Project endpoints =====

/// GET /companies/:company_id/projects
async fn list_projects(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Project>>, AppError> {
    let projects = state
        .project_service
        .list_by_company(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(projects))
}

/// POST /companies/:company_id/projects
async fn create_project(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<CreateProjectInput>,
) -> Result<(StatusCode, Json<Project>), AppError> {
    input.company_id = company_id;
    let project = state
        .project_service
        .create(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// GET /projects/:project_id
async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Project>, AppError> {
    let project = state
        .project_service
        .get_by_id(project_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", project_id)))?;
    Ok(Json(project))
}

/// PATCH /projects/:project_id
async fn update_project(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<UpdateProjectInput>,
) -> Result<Json<Project>, AppError> {
    let project = state
        .project_service
        .update(project_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(project))
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
    Path(project_id): Path<Uuid>,
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
) -> Result<Json<ProjectWorkspace>, AppError> {
    // TODO: Implement workspace update
    Err(AppError::NotImplemented("Workspace update not yet implemented".to_string()))
}

/// DELETE /projects/:project_id/workspaces/:workspace_id
async fn delete_workspace(
    State(state): State<AppState>,
    Path((project_id, workspace_id)): Path<(Uuid, Uuid)>,
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
    State(_state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Implement external object summary query
    Ok(Json(serde_json::json!({
        "project_id": project_id,
        "issues_count": 0,
        "agents_count": 0,
        "workspaces_count": 0,
    })))
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
    let state_val = body.get("state").and_then(|v| v.as_str()).unwrap_or("joined");
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
