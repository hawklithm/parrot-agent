use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::models::{CreateDocumentInput, IssueDocument, LockDocumentInput};
use crate::services::{DocumentParentType, DocumentService};

/// GET /issues/:id/documents - List issue documents
async fn list_issue_documents(
    State(service): State<Arc<dyn DocumentService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IssueDocument>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_documents(DocumentParentType::Issue, id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/:id/documents/:key - Get issue document by key
async fn get_issue_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get_document(DocumentParentType::Issue, id, &key, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// PUT /issues/:id/documents/:key - Upsert issue document
async fn upsert_issue_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(input): Json<CreateDocumentInput>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .upsert_document(DocumentParentType::Issue, id, &key, input, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/documents/:key/lock - Lock issue document
async fn lock_issue_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(input): Json<LockDocumentInput>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .lock_document(DocumentParentType::Issue, id, &key, input, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/documents/:key/unlock - Unlock issue document
async fn unlock_issue_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .unlock_document(DocumentParentType::Issue, id, &key, company_id, None, None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /cases/:id/documents - List case documents
async fn list_case_documents(
    State(service): State<Arc<dyn DocumentService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IssueDocument>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_documents(DocumentParentType::Case, id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /cases/:id/documents/:key - Get case document by key
async fn get_case_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get_document(DocumentParentType::Case, id, &key, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// PUT /cases/:id/documents/:key - Upsert case document
async fn upsert_case_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(input): Json<CreateDocumentInput>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .upsert_document(DocumentParentType::Case, id, &key, input, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /cases/:id/documents/:key/lock - Lock case document
async fn lock_case_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(input): Json<LockDocumentInput>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .lock_document(DocumentParentType::Case, id, &key, input, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /cases/:id/documents/:key/unlock - Unlock case document
async fn unlock_case_document(
    State(service): State<Arc<dyn DocumentService>>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<IssueDocument>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .unlock_document(DocumentParentType::Case, id, &key, company_id, None, None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create document routes
pub fn document_routes(service: Arc<dyn DocumentService>) -> Router {
    Router::new()
        .route("/api/issues/:id/documents", get(list_issue_documents))
        .route("/api/issues/:id/documents/:key", get(get_issue_document).put(upsert_issue_document))
        .route("/api/issues/:id/documents/:key/lock", post(lock_issue_document))
        .route("/api/issues/:id/documents/:key/unlock", post(unlock_issue_document))
        .route("/api/cases/:id/documents", get(list_case_documents))
        .route("/api/cases/:id/documents/:key", get(get_case_document).put(upsert_case_document))
        .route("/api/cases/:id/documents/:key/lock", post(lock_case_document))
        .route("/api/cases/:id/documents/:key/unlock", post(unlock_case_document))
        .with_state(service)
}
