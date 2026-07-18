use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use models::{CreateIssueTreeHoldInput, IssueTreeHold, IssueTreeControlPreview};
use services::TreeControlService;

/// POST /issues/:id/tree-control/preview - Preview tree control impact
async fn preview_tree_control(
    State(service): State<Arc<dyn TreeControlService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateIssueTreeHoldInput>,
) -> Result<Json<IssueTreeControlPreview>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .preview(id, company_id, &input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/tree-holds - Create tree hold
async fn create_tree_hold(
    State(service): State<Arc<dyn TreeControlService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<CreateIssueTreeHoldInput>,
) -> Result<Json<IssueTreeHold>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .create_hold(id, company_id, input, None, None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/:id/tree-control/state - Get tree hold state
async fn get_tree_hold_state(
    State(service): State<Arc<dyn TreeControlService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<IssueTreeHold>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get_hold_state(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/:id/tree-holds - List tree holds
async fn list_tree_holds(
    State(service): State<Arc<dyn TreeControlService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IssueTreeHold>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_holds(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/tree-holds/:holdId/release - Release tree hold
async fn release_tree_hold(
    State(service): State<Arc<dyn TreeControlService>>,
    Path((id, hold_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<IssueTreeHold>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .release_hold(id, hold_id, company_id, None, None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create tree control routes
pub fn tree_control_routes(service: Arc<dyn TreeControlService>) -> Router {
    Router::new()
        .route("/issues/:id/tree-control/preview", post(preview_tree_control))
        .route("/issues/:id/tree-holds", post(create_tree_hold).get(list_tree_holds))
        .route("/issues/:id/tree-control/state", get(get_tree_hold_state))
        .route("/issues/:id/tree-holds/:holdId/release", post(release_tree_hold))
        .with_state(service)
}
