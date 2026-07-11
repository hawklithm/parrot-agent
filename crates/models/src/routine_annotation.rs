use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Annotation thread status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationThreadStatus {
    Open,
    Resolved,
}

/// Anchor state (active/stale/orphaned)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationAnchorState {
    Active,
    Stale,
    Orphaned,
}

/// Anchor confidence level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationAnchorConfidence {
    Exact,
    Duplicate,
    Fuzzy,
    Ambiguous,
    Missing,
}

/// Text quote selector (annotation position anchor)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationTextQuoteSelector {
    pub exact: String,
    pub prefix: String,
    pub suffix: String,
}

/// Text position selector (character offsets)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationTextPositionSelector {
    pub normalized_start: i32,
    pub normalized_end: i32,
    pub markdown_start: i32,
    pub markdown_end: i32,
}

/// Combined anchor selector (quote + position)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationAnchorSelector {
    pub quote: AnnotationTextQuoteSelector,
    pub position: AnnotationTextPositionSelector,
}

/// Annotation thread on routine description
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineAnnotationThread {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub document_id: Uuid,
    pub document_key: String, // "description"
    pub status: AnnotationThreadStatus,
    pub anchor_state: AnnotationAnchorState,
    pub anchor_confidence: AnnotationAnchorConfidence,
    pub original_revision_id: Option<Uuid>,
    pub original_revision_number: i32,
    pub current_revision_id: Option<Uuid>,
    pub current_revision_number: i32,
    pub selected_text: String,
    pub prefix_text: String,
    pub suffix_text: String,
    pub normalized_start: i32,
    pub normalized_end: i32,
    pub markdown_start: i32,
    pub markdown_end: i32,
    pub anchor_selector: AnnotationAnchorSelector,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub resolved_by_agent_id: Option<Uuid>,
    pub resolved_by_user_id: Option<Uuid>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Annotation comment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineAnnotationComment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub thread_id: Uuid,
    pub routine_id: Uuid,
    pub document_id: Uuid,
    pub body: String,
    pub author_type: String, // "user" | "agent" | "system"
    pub author_agent_id: Option<Uuid>,
    pub author_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Thread with comments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineAnnotationThreadWithComments {
    #[serde(flatten)]
    pub thread: RoutineAnnotationThread,
    pub comments: Vec<RoutineAnnotationComment>,
}

/// Create annotation thread request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutineAnnotationThreadRequest {
    pub base_revision_id: Uuid,
    pub base_revision_number: i32,
    pub selector: AnnotationAnchorSelector,
    pub body: String,
}

/// Create annotation comment request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutineAnnotationCommentRequest {
    pub body: String,
}

/// Update annotation thread request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoutineAnnotationThreadRequest {
    pub status: Option<AnnotationThreadStatus>,
}
