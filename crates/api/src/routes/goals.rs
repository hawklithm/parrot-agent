//! Goal routes — CRUD + progress + hierarchy

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::goal::{Goal, GoalLevel, GoalPriority};
use services::GoalService;

pub fn goal_routes() -> Router<AppState> {
    Router::new()
        // Goal CRUD
        .route("/companies/:company_id/goals", get(list_goals).post(create_goal))
        .route("/goals/:goal_id", get(get_goal).patch(update_goal).delete(delete_goal))
        .route("/goals/:goal_id/complete", post(complete_goal))
        .route("/goals/:goal_id/abandon", post(abandon_goal))
        .route("/goals/:goal_id/progress", get(get_goal_progress))
        .route("/goals/:goal_id/hierarchy", get(get_goal_hierarchy))
        // Children
        .route("/goals/:goal_id/children", get(list_child_goals))
}

/// POST /companies/:company_id/goals
async fn create_goal(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Goal>), AppError> {
    let name = body.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = body.get("description").and_then(|v| v.as_str().map(String::from));
    let level_str = body.get("level").and_then(|v| v.as_str()).unwrap_or("task");
    let level = match level_str {
        "company" => GoalLevel::Company,
        "project" => GoalLevel::Project,
        _ => GoalLevel::Task,
    };

    let input = services::CreateGoalInput {
        company_id,
        title: name,
        description,
        level,
        parent_id: body.get("parent_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())),
        owner_agent_id: body.get("owner_agent_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())),
    };

    let goal = state
        .goal_service
        .create(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(goal)))
}

/// GET /companies/:company_id/goals
async fn list_goals(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Goal>>, AppError> {
    let goals = state
        .goal_service
        .list_by_company(company_id, None)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(goals))
}

/// GET /goals/:goal_id
async fn get_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<Goal>, AppError> {
    let goal = state
        .goal_service
        .get_by_id(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(goal))
}

/// PATCH /goals/:goal_id
async fn update_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Goal>, AppError> {
    let title = body.get("title").and_then(|v| v.as_str().map(String::from));
    let description = body.get("description").and_then(|v| v.as_str().map(String::from));
    let status_str = body.get("status").and_then(|v| v.as_str());
    let status = status_str.and_then(|s| match s {
        "planned" => Some(models::goal::GoalStatus::Planned),
        "active" => Some(models::goal::GoalStatus::Active),
        "completed" => Some(models::goal::GoalStatus::Achieved),
        "archived" => Some(models::goal::GoalStatus::Archived),
        _ => None,
    });

    let input = services::UpdateGoalInput {
        title,
        description,
        status,
        owner_agent_id: body.get("owner_agent_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())),
    };

    let goal = state
        .goal_service
        .update(goal_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(goal))
}

/// DELETE /goals/:goal_id
async fn delete_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .goal_service
        .delete(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /goals/:goal_id/complete
async fn complete_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<Goal>, AppError> {
    let goal = state
        .goal_service
        .mark_achieved(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(goal))
}

/// POST /goals/:goal_id/abandon
async fn abandon_goal(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<Goal>, AppError> {
    // Update goal status to archived
    let input = services::UpdateGoalInput {
        title: None,
        description: None,
        status: Some(models::goal::GoalStatus::Archived),
        owner_agent_id: None,
    };
    let goal = state
        .goal_service
        .update(goal_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(goal))
}

/// GET /goals/:goal_id/progress
async fn get_goal_progress(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let progress = state
        .goal_service
        .calculate_progress(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(serde_json::json!({ "progress": progress })))
}

/// GET /goals/:goal_id/hierarchy
async fn get_goal_hierarchy(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<services::GoalHierarchy>, AppError> {
    let hierarchy = state
        .goal_service
        .get_hierarchy(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(hierarchy))
}

/// GET /goals/:goal_id/children
async fn list_child_goals(
    State(state): State<AppState>,
    Path(goal_id): Path<Uuid>,
) -> Result<Json<Vec<Goal>>, AppError> {
    // Use GoalRepository directly via GoalService
    let hierarchy = state
        .goal_service
        .get_hierarchy(goal_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(hierarchy.children))
}
