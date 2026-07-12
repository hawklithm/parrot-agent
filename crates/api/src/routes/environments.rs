use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use models::{CreateEnvironmentInput, UpdateEnvironmentInput};
use services::{
    MockEnvironmentLeaseService, MockEnvironmentService, MockExecutionWorkspaceService,
};

pub fn environment_routes() -> Router {
    let env_service = MockEnvironmentService::new();
    let lease_service = MockEnvironmentLeaseService::new();
    let workspace_service = MockExecutionWorkspaceService::new();

    Router::new()
        .route(
            "/api/companies/:company_id/environments",
            get({
                let service = Arc::clone(&env_service);
                move |Path(company_id): Path<Uuid>| async move {
                    list_environments(company_id, service).await
                }
            })
            .post({
                let service = Arc::clone(&env_service);
                move |Path(company_id): Path<Uuid>, Json(input): Json<CreateEnvironmentInput>| async move {
                    create_environment(company_id, input, service).await
                }
            }),
        )
        .route(
            "/api/environments/:id",
            get({
                let service = Arc::clone(&env_service);
                move |Path(id): Path<Uuid>| async move {
                    get_environment(id, service).await
                }
            })
            .patch({
                let service = Arc::clone(&env_service);
                move |Path(id): Path<Uuid>, Json(input): Json<UpdateEnvironmentInput>| async move {
                    update_environment(id, input, service).await
                }
            })
            .delete({
                let service = Arc::clone(&env_service);
                move |Path(id): Path<Uuid>| async move {
                    delete_environment(id, service).await
                }
            }),
        )
        .route(
            "/api/environments/:id/probe",
            post({
                let service = Arc::clone(&env_service);
                move |Path(id): Path<Uuid>| async move {
                    probe_environment(id, service).await
                }
            }),
        )
        .route(
            "/api/environments/:environment_id/leases",
            post({
                let service = Arc::clone(&lease_service);
                move |Path(environment_id): Path<Uuid>, Json(body): Json<serde_json::Value>| async move {
                    acquire_lease(environment_id, body, service).await
                }
            }),
        )
        .route(
            "/api/companies/:company_id/environment-leases",
            get({
                let service = Arc::clone(&lease_service);
                move |Path(company_id): Path<Uuid>| async move {
                    list_active_leases(company_id, service).await
                }
            }),
        )
        .route(
            "/api/environment-leases/:lease_id/release",
            post({
                let service = Arc::clone(&lease_service);
                move |Path(lease_id): Path<Uuid>| async move {
                    release_lease(lease_id, service).await
                }
            }),
        )
        .route(
            "/api/companies/:company_id/execution-workspaces",
            get({
                let service = Arc::clone(&workspace_service);
                move |Path(company_id): Path<Uuid>| async move {
                    list_workspaces(company_id, service).await
                }
            })
            .post({
                let service = Arc::clone(&workspace_service);
                move |Path(company_id): Path<Uuid>, Json(body): Json<serde_json::Value>| async move {
                    create_workspace(company_id, body, service).await
                }
            }),
        )
        .route(
            "/api/execution-workspaces/:id",
            get({
                let service = Arc::clone(&workspace_service);
                move |Path(id): Path<Uuid>| async move {
                    get_workspace(id, service).await
                }
            })
            .delete({
                let service = Arc::clone(&workspace_service);
                move |Path(id): Path<Uuid>| async move {
                    dispose_workspace(id, service).await
                }
            }),
        )
}

async fn list_environments(
    company_id: Uuid,
    service: Arc<MockEnvironmentService>,
) -> impl IntoResponse {
    match service.list_environments(company_id).await {
        Ok(environments) => (StatusCode::OK, Json(environments)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn get_environment(id: Uuid, service: Arc<MockEnvironmentService>) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.get_environment(id, company_id).await {
        Ok(Some(environment)) => (StatusCode::OK, Json(environment)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Environment not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn create_environment(
    company_id: Uuid,
    input: CreateEnvironmentInput,
    service: Arc<MockEnvironmentService>,
) -> impl IntoResponse {
    match service.create_environment(company_id, input).await {
        Ok(environment) => (StatusCode::CREATED, Json(environment)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn update_environment(
    id: Uuid,
    input: UpdateEnvironmentInput,
    service: Arc<MockEnvironmentService>,
) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.update_environment(id, company_id, input).await {
        Ok(environment) => (StatusCode::OK, Json(environment)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn delete_environment(id: Uuid, service: Arc<MockEnvironmentService>) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.delete_environment(id, company_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn probe_environment(id: Uuid, service: Arc<MockEnvironmentService>) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.probe_environment(id, company_id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn acquire_lease(
    environment_id: Uuid,
    body: serde_json::Value,
    service: Arc<MockEnvironmentLeaseService>,
) -> impl IntoResponse {
    let company_id = body
        .get("companyId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);
    let issue_id = body
        .get("issueId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let heartbeat_run_id = body
        .get("heartbeatRunId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    match service
        .acquire_lease(environment_id, company_id, issue_id, heartbeat_run_id)
        .await
    {
        Ok(lease) => (StatusCode::CREATED, Json(lease)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn release_lease(
    lease_id: Uuid,
    service: Arc<MockEnvironmentLeaseService>,
) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.release_lease(lease_id, company_id).await {
        Ok(lease) => (StatusCode::OK, Json(lease)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn list_active_leases(
    company_id: Uuid,
    service: Arc<MockEnvironmentLeaseService>,
) -> impl IntoResponse {
    match service.list_active_leases(company_id).await {
        Ok(leases) => (StatusCode::OK, Json(leases)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn create_workspace(
    company_id: Uuid,
    body: serde_json::Value,
    service: Arc<MockExecutionWorkspaceService>,
) -> impl IntoResponse {
    let project_id = body
        .get("projectId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Workspace")
        .to_string();

    match service.create_workspace(company_id, project_id, name).await {
        Ok(workspace) => (StatusCode::CREATED, Json(workspace)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn get_workspace(
    id: Uuid,
    service: Arc<MockExecutionWorkspaceService>,
) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.get_workspace(id, company_id).await {
        Ok(Some(workspace)) => (StatusCode::OK, Json(workspace)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Workspace not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn list_workspaces(
    company_id: Uuid,
    service: Arc<MockExecutionWorkspaceService>,
) -> impl IntoResponse {
    match service.list_workspaces(company_id).await {
        Ok(workspaces) => (StatusCode::OK, Json(workspaces)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}

async fn dispose_workspace(
    id: Uuid,
    service: Arc<MockExecutionWorkspaceService>,
) -> impl IntoResponse {
    let company_id = Uuid::new_v4();
    match service.dispose_workspace(id, company_id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e})),
        )
            .into_response(),
    }
}
