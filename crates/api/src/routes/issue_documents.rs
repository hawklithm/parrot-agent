use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{Document, IssueDocument, UpsertDocumentInput};
use services::DocumentServiceError;
use crate::{errors::ApiError, app_state::AppState};

/// Upsert Document request
#[derive(Debug, Deserialize)]
pub struct UpsertDocumentRequest {
    pub content: String,
    pub content_type: Option<String>,
}

/// Lock Document request
#[derive(Debug, Deserialize)]
pub struct LockDocumentRequest {
    pub locked_by_type: String, // "agent", "user", "system"
    pub locked_by_id: Uuid,
    pub run_id: Option<Uuid>,
}

/// Unlock Document request
#[derive(Debug, Deserialize)]
pub struct UnlockDocumentRequest {
    pub actor_id: Uuid,
}

/// Document response
#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    pub link: IssueDocument,
    pub document: Document,
}

/// Upsert response
#[derive(Debug, Serialize)]
pub struct UpsertDocumentResponse {
    pub link: IssueDocument,
    pub document: Document,
    pub created: bool,
}

/// Documents list response
#[derive(Debug, Serialize)]
pub struct DocumentsListResponse {
    pub documents: Vec<DocumentWithLink>,
}

#[derive(Debug, Serialize)]
pub struct DocumentWithLink {
    pub link: IssueDocument,
    pub document: Document,
}

// Convert service errors to API errors
impl From<DocumentServiceError> for ApiError {
    fn from(err: DocumentServiceError) -> Self {
        match err {
            DocumentServiceError::NotFound(msg) => {
                ApiError::NotFound(msg)
            }
            DocumentServiceError::IssueNotFound(id) => {
                ApiError::NotFound(format!("Issue not found: {}", id))
            }
            DocumentServiceError::DocumentLocked { locked_by_type, locked_by_id } => {
                ApiError::Conflict(format!("Document is locked by {} {}", locked_by_type, locked_by_id))
            }
            DocumentServiceError::PermissionDenied(msg) => {
                ApiError::Forbidden(msg)
            }
            DocumentServiceError::Validation(msg) => {
                ApiError::BadRequest(msg)
            }
            DocumentServiceError::Repository(repo_err) => {
                ApiError::InternalServerError(format!("Database error: {}", repo_err))
            }
        }
    }
}

/// PUT /issues/:issue_id/documents/:key - Create or update a document
pub async fn upsert_document(
    State(state): State<AppState>,
    Path((issue_id, key)): Path<(Uuid, String)>,
    Json(req): Json<UpsertDocumentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = UpsertDocumentInput {
        key,
        content: req.content,
        content_type: req.content_type,
    };

    // Get issue to determine company_id
    let issue = state.issue_service.get(issue_id, Uuid::nil()).await
        .map_err(|_| ApiError::NotFound(format!("Issue not found: {}", issue_id)))?;
    let company_id = issue
        .map(|i| i.company_id)
        .ok_or_else(|| ApiError::NotFound(format!("Issue not found: {}", issue_id)))?;

    let (link, document, created) = state.issue_document_service
        .upsert_document(issue_id, company_id, input)
        .await?;

    let status_code = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status_code, Json(UpsertDocumentResponse { link, document, created })))
}

/// GET /issues/:issue_id/documents/:key - Get a document
pub async fn get_document(
    State(state): State<AppState>,
    Path((issue_id, key)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let (link, document) = state.issue_document_service
        .get_document(issue_id, &key)
        .await?;

    Ok(Json(DocumentResponse { link, document }))
}

/// GET /issues/:issue_id/documents - List all documents for an issue
pub async fn list_documents(
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let docs = state.issue_document_service
        .list_documents(issue_id)
        .await?;

    let documents: Vec<DocumentWithLink> = docs.into_iter()
        .map(|(link, document)| DocumentWithLink { link, document })
        .collect();

    Ok(Json(DocumentsListResponse { documents }))
}

/// DELETE /issues/:issue_id/documents/:key - Delete a document
pub async fn delete_document(
    State(state): State<AppState>,
    Path((issue_id, key)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    state.issue_document_service
        .delete_document(issue_id, &key)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /issues/:issue_id/documents/:key/lock - Lock a document
pub async fn lock_document(
    State(state): State<AppState>,
    Path((issue_id, key)): Path<(Uuid, String)>,
    Json(req): Json<LockDocumentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let document = state.issue_document_service
        .lock_document(
            issue_id,
            &key,
            req.locked_by_type,
            req.locked_by_id,
            req.run_id,
        )
        .await?;

    Ok(Json(document))
}

/// POST /issues/:issue_id/documents/:key/unlock - Unlock a document
pub async fn unlock_document(
    State(state): State<AppState>,
    Path((issue_id, key)): Path<(Uuid, String)>,
    Json(req): Json<UnlockDocumentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let document = state.issue_document_service
        .unlock_document(issue_id, &key, req.actor_id)
        .await?;

    Ok(Json(document))
}

/// Create Issue Document routes
pub fn issue_document_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:issue_id/documents", get(list_documents))
        .route("/issues/:issue_id/documents/:key", put(upsert_document))
        .route("/issues/:issue_id/documents/:key", get(get_document))
        .route("/issues/:issue_id/documents/:key", delete(delete_document))
        .route("/issues/:issue_id/documents/:key/lock", post(lock_document))
        .route("/issues/:issue_id/documents/:key/unlock", post(unlock_document))
}
