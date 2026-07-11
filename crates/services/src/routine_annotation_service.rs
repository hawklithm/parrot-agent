use async_trait::async_trait;
use models::{
    AnnotationAnchorConfidence, AnnotationAnchorSelector, AnnotationAnchorState,
    AnnotationTextPositionSelector, AnnotationTextQuoteSelector, AnnotationThreadStatus,
    CreateRoutineAnnotationCommentRequest, CreateRoutineAnnotationThreadRequest,
    RoutineAnnotationComment, RoutineAnnotationThread, RoutineAnnotationThreadWithComments,
    UpdateRoutineAnnotationThreadRequest,
};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait RoutineAnnotationService: Send + Sync {
    /// GET /routines/:id/description/annotations - 获取routine的所有annotations
    async fn list_annotations(
        &self,
        routine_id: Uuid,
        include_comments: bool,
    ) -> Result<Vec<RoutineAnnotationThreadWithComments>, String>;

    /// POST /routines/:id/description/annotations - 创建新annotation thread
    async fn create_annotation_thread(
        &self,
        routine_id: Uuid,
        request: CreateRoutineAnnotationThreadRequest,
    ) -> Result<RoutineAnnotationThreadWithComments, String>;

    /// POST /routines/:id/description/annotations/:threadId/comments - 添加评论到thread
    async fn add_comment(
        &self,
        routine_id: Uuid,
        thread_id: Uuid,
        request: CreateRoutineAnnotationCommentRequest,
    ) -> Result<RoutineAnnotationComment, String>;

    /// PATCH /routines/:id/description/annotations/:threadId - 更新thread状态
    async fn update_thread(
        &self,
        routine_id: Uuid,
        thread_id: Uuid,
        request: UpdateRoutineAnnotationThreadRequest,
    ) -> Result<RoutineAnnotationThread, String>;
}

pub struct MockRoutineAnnotationService;

#[async_trait]
impl RoutineAnnotationService for MockRoutineAnnotationService {
    async fn list_annotations(
        &self,
        routine_id: Uuid,
        include_comments: bool,
    ) -> Result<Vec<RoutineAnnotationThreadWithComments>, String> {
        let company_id = Uuid::new_v4();
        let document_id = Uuid::new_v4();
        let thread_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let thread = RoutineAnnotationThread {
            id: thread_id,
            company_id,
            routine_id,
            document_id,
            document_key: "description".to_string(),
            status: AnnotationThreadStatus::Open,
            anchor_state: AnnotationAnchorState::Active,
            anchor_confidence: AnnotationAnchorConfidence::Exact,
            original_revision_id: Some(Uuid::new_v4()),
            original_revision_number: 1,
            current_revision_id: Some(Uuid::new_v4()),
            current_revision_number: 1,
            selected_text: "This is the selected text for annotation".to_string(),
            prefix_text: "Context before...".to_string(),
            suffix_text: "...context after".to_string(),
            normalized_start: 100,
            normalized_end: 141,
            markdown_start: 105,
            markdown_end: 146,
            anchor_selector: AnnotationAnchorSelector {
                quote: AnnotationTextQuoteSelector {
                    exact: "This is the selected text for annotation".to_string(),
                    prefix: "Context before...".to_string(),
                    suffix: "Context after".to_string(),
                },
                position: AnnotationTextPositionSelector {
                    normalized_start: 100,
                    normalized_end: 141,
                    markdown_start: 105,
                    markdown_end: 146,
                },
            },
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            resolved_by_agent_id: None,
            resolved_by_user_id: None,
            resolved_at: None,
            created_at: now,
            updated_at: now,
        };

        let comments = if include_comments {
            vec![RoutineAnnotationComment {
                id: Uuid::new_v4(),
                company_id,
                thread_id,
                routine_id,
                document_id,
                body: "This section needs clarification".to_string(),
                author_type: "user".to_string(),
                author_agent_id: None,
                author_user_id: Some(Uuid::new_v4()),
                created_by_run_id: None,
                created_at: now,
                updated_at: now,
            }]
        } else {
            vec![]
        };

        Ok(vec![RoutineAnnotationThreadWithComments { thread, comments }])
    }

    async fn create_annotation_thread(
        &self,
        routine_id: Uuid,
        request: CreateRoutineAnnotationThreadRequest,
    ) -> Result<RoutineAnnotationThreadWithComments, String> {
        let company_id = Uuid::new_v4();
        let document_id = Uuid::new_v4();
        let thread_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let thread = RoutineAnnotationThread {
            id: thread_id,
            company_id,
            routine_id,
            document_id,
            document_key: "description".to_string(),
            status: AnnotationThreadStatus::Open,
            anchor_state: AnnotationAnchorState::Active,
            anchor_confidence: AnnotationAnchorConfidence::Exact,
            original_revision_id: Some(request.base_revision_id),
            original_revision_number: request.base_revision_number,
            current_revision_id: Some(request.base_revision_id),
            current_revision_number: request.base_revision_number,
            selected_text: request.selector.quote.exact.clone(),
            prefix_text: request.selector.quote.prefix.clone(),
            suffix_text: request.selector.quote.suffix.clone(),
            normalized_start: request.selector.position.normalized_start,
            normalized_end: request.selector.position.normalized_end,
            markdown_start: request.selector.position.markdown_start,
            markdown_end: request.selector.position.markdown_end,
            anchor_selector: request.selector,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            resolved_by_agent_id: None,
            resolved_by_user_id: None,
            resolved_at: None,
            created_at: now,
            updated_at: now,
        };

        let comment = RoutineAnnotationComment {
            id: Uuid::new_v4(),
            company_id,
            thread_id,
            routine_id,
            document_id,
            body: request.body,
            author_type: "user".to_string(),
            author_agent_id: None,
            author_user_id: Some(Uuid::new_v4()),
            created_by_run_id: None,
            created_at: now,
            updated_at: now,
        };

        Ok(RoutineAnnotationThreadWithComments {
            thread,
            comments: vec![comment],
        })
    }

    async fn add_comment(
        &self,
        routine_id: Uuid,
        thread_id: Uuid,
        request: CreateRoutineAnnotationCommentRequest,
    ) -> Result<RoutineAnnotationComment, String> {
        let now = chrono::Utc::now();

        Ok(RoutineAnnotationComment {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            thread_id,
            routine_id,
            document_id: Uuid::new_v4(),
            body: request.body,
            author_type: "user".to_string(),
            author_agent_id: None,
            author_user_id: Some(Uuid::new_v4()),
            created_by_run_id: None,
            created_at: now,
            updated_at: now,
        })
    }

    async fn update_thread(
        &self,
        routine_id: Uuid,
        thread_id: Uuid,
        request: UpdateRoutineAnnotationThreadRequest,
    ) -> Result<RoutineAnnotationThread, String> {
        let now = chrono::Utc::now();
        let company_id = Uuid::new_v4();
        let document_id = Uuid::new_v4();

        let status = request.status.unwrap_or(AnnotationThreadStatus::Open);
        let (resolved_by_user_id, resolved_at) = if status == AnnotationThreadStatus::Resolved {
            (Some(Uuid::new_v4()), Some(now))
        } else {
            (None, None)
        };

        Ok(RoutineAnnotationThread {
            id: thread_id,
            company_id,
            routine_id,
            document_id,
            document_key: "description".to_string(),
            status,
            anchor_state: AnnotationAnchorState::Active,
            anchor_confidence: AnnotationAnchorConfidence::Exact,
            original_revision_id: Some(Uuid::new_v4()),
            original_revision_number: 1,
            current_revision_id: Some(Uuid::new_v4()),
            current_revision_number: 1,
            selected_text: "This is the selected text for annotation".to_string(),
            prefix_text: "Context before...".to_string(),
            suffix_text: "...context after".to_string(),
            normalized_start: 100,
            normalized_end: 141,
            markdown_start: 105,
            markdown_end: 146,
            anchor_selector: AnnotationAnchorSelector {
                quote: AnnotationTextQuoteSelector {
                    exact: "This is the selected text for annotation".to_string(),
                    prefix: "Context before...".to_string(),
                    suffix: "...context after".to_string(),
                },
                position: AnnotationTextPositionSelector {
                    normalized_start: 100,
                    normalized_end: 141,
                    markdown_start: 105,
                    markdown_end: 146,
                },
            },
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            resolved_by_agent_id: None,
            resolved_by_user_id,
            resolved_at,
            created_at: now - chrono::Duration::hours(1),
            updated_at: now,
        })
    }
}
