use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use services::auth::AuthorizationActor;
use services::resource_membership_service::UpdateResourceMembershipInput;
use uuid::Uuid;

pub fn resource_membership_routes() -> Router<AppState> {
    Router::new()
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

/// GET /companies/:company_id/resource-memberships/me
/// List all resource memberships for the current user
async fn list_my_memberships(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Extract user_id from auth context (Board user only)
    let user_id = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => user_id.to_string(),
        _ => return Err(AppError::Forbidden("Board user access required".to_string())),
    };

    let service = services::ResourceMembershipService::new(state.pool.clone());
    let memberships = service.list_for_user(company_id, &user_id).await?;

    Ok(Json(serde_json::to_value(memberships).unwrap_or_default()))
}

/// PUT /companies/:company_id/resource-memberships/me/projects/:project_id
/// Update project membership (join/leave/star/unstar)
async fn update_project_membership(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((company_id, project_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateResourceMembershipInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Extract user_id from auth context (Board user only)
    let user_id = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => user_id.to_string(),
        _ => return Err(AppError::Forbidden("Board user access required".to_string())),
    };

    // Extract actor info for activity logging
    let (actor_type, actor_id, agent_id, run_id) = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => {
            ("user", *user_id, None, None)
        }
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            ("agent", *agent_id, Some(*agent_id), *run_id)
        }
        _ => {
            return Err(AppError::Forbidden("Authentication required".to_string()));
        }
    };

    let service = services::ResourceMembershipService::new(state.pool.clone());
    let result = service
        .update_project(&auth_actor, company_id, &user_id, project_id, input)
        .await?;

    // Log activity if changed
    if result.changed && result.change_kind.is_some() {
        let _ = service
            .log_membership_activity(
                company_id,
                actor_type,
                actor_id,
                agent_id,
                run_id,
                &user_id,
                &result,
            )
            .await;
    }

    // Filter out internal fields (align with paperclip)
    let response = serde_json::json!({
        "resourceType": result.resource_type,
        "resourceId": result.resource_id,
        "state": result.state,
        "starredAt": result.starred_at,
        "updatedAt": result.updated_at,
    });

    Ok((StatusCode::OK, Json(response)))
}

/// PUT /companies/:company_id/resource-memberships/me/agents/:agent_id
/// Update agent membership (join/leave/star/unstar)
async fn update_agent_membership(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((company_id, agent_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateResourceMembershipInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Extract user_id from auth context (Board user only)
    let user_id = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => user_id.to_string(),
        _ => return Err(AppError::Forbidden("Board user access required".to_string())),
    };

    // Extract actor info for activity logging
    let (actor_type, actor_id_val, agent_id_actor, run_id) = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => {
            ("user", *user_id, None, None)
        }
        AuthorizationActor::Agent { agent_id: aid, run_id, .. } => {
            ("agent", *aid, Some(*aid), *run_id)
        }
        _ => {
            return Err(AppError::Forbidden("Authentication required".to_string()));
        }
    };

    let service = services::ResourceMembershipService::new(state.pool.clone());
    let result = service
        .update_agent(&auth_actor, company_id, &user_id, agent_id, input)
        .await?;

    // Log activity if changed
    if result.changed && result.change_kind.is_some() {
        let _ = service
            .log_membership_activity(
                company_id,
                actor_type,
                actor_id_val,
                agent_id_actor,
                run_id,
                &user_id,
                &result,
            )
            .await;
    }

    // Filter out internal fields (align with paperclip)
    let response = serde_json::json!({
        "resourceType": result.resource_type,
        "resourceId": result.resource_id,
        "state": result.state,
        "starredAt": result.starred_at,
        "updatedAt": result.updated_at,
    });

    Ok((StatusCode::OK, Json(response)))
}
