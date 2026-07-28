use async_trait::async_trait;
use models::{
    AnnotationAnchorConfidence, AnnotationAnchorSelector, AnnotationAnchorState,
    AnnotationTextPositionSelector, AnnotationTextQuoteSelector,
    CreateRoutineAnnotationCommentRequest, CreateRoutineAnnotationThreadRequest,
    RoutineAnnotationComment, RoutineAnnotationThread, RoutineAnnotationThreadWithComments,
    UpdateRoutineAnnotationThreadRequest,
};
use models::routine_annotation::AnnotationThreadStatus;
use uuid::Uuid;
use sqlx::{PgPool, Row};

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

/// PostgreSQL implementation aligned with Paperclip's document annotation
/// service.  A routine owns a stable `description` document and annotations
/// reference that document rather than being regenerated per request.
pub struct PgRoutineAnnotationService { pool: PgPool }

impl PgRoutineAnnotationService {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    async fn document(&self, routine_id: Uuid) -> Result<(Uuid, Uuid), String> {
        if let Some(row) = sqlx::query("SELECT company_id, document_id FROM routine_documents WHERE routine_id = $1 AND document_key = 'description'").bind(routine_id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())? {
            return Ok((row.get("company_id"), row.get("document_id")));
        }
        let row = sqlx::query("SELECT company_id, COALESCE(description, '') AS description FROM routines WHERE id = $1").bind(routine_id).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?.ok_or_else(|| "routine not found".to_string())?;
        let company_id: Uuid = row.get("company_id");
        let document_id: Uuid = sqlx::query_scalar("INSERT INTO documents (company_id, content, content_type) VALUES ($1, $2, 'text/markdown') RETURNING id").bind(company_id).bind(row.get::<String,_>("description")).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO routine_documents (company_id, routine_id, document_key, document_id) VALUES ($1,$2,'description',$3) ON CONFLICT (routine_id, document_key) DO NOTHING").bind(company_id).bind(routine_id).bind(document_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        let id: Uuid = sqlx::query_scalar("SELECT document_id FROM routine_documents WHERE routine_id = $1 AND document_key = 'description'").bind(routine_id).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        Ok((company_id, id))
    }
    fn thread(row: &sqlx::postgres::PgRow) -> Result<RoutineAnnotationThread, String> {
        let selector: AnnotationAnchorSelector = serde_json::from_value(row.get("anchor_selector")).map_err(|e| e.to_string())?;
        let status = match row.get::<String,_>("status").as_str() { "resolved" => AnnotationThreadStatus::Resolved, _ => AnnotationThreadStatus::Open };
        let state = match row.get::<String,_>("anchor_state").as_str() { "stale" => AnnotationAnchorState::Stale, "orphaned" => AnnotationAnchorState::Orphaned, _ => AnnotationAnchorState::Active };
        let confidence = match row.get::<String,_>("anchor_confidence").as_str() { "duplicate" => AnnotationAnchorConfidence::Duplicate, "fuzzy" => AnnotationAnchorConfidence::Fuzzy, "ambiguous" => AnnotationAnchorConfidence::Ambiguous, "missing" => AnnotationAnchorConfidence::Missing, _ => AnnotationAnchorConfidence::Exact };
        Ok(RoutineAnnotationThread { id: row.get("id"), company_id: row.get("company_id"), routine_id: row.get("routine_id"), document_id: row.get("document_id"), document_key: row.get("document_key"), status, anchor_state: state, anchor_confidence: confidence, original_revision_id: row.get("original_revision_id"), original_revision_number: row.get("original_revision_number"), current_revision_id: row.get("current_revision_id"), current_revision_number: row.get("current_revision_number"), selected_text: row.get("selected_text"), prefix_text: row.get("prefix_text"), suffix_text: row.get("suffix_text"), normalized_start: row.get("normalized_start"), normalized_end: row.get("normalized_end"), markdown_start: row.get("markdown_start"), markdown_end: row.get("markdown_end"), anchor_selector: selector, created_by_agent_id: row.get("created_by_agent_id"), created_by_user_id: None, resolved_by_agent_id: row.get("resolved_by_agent_id"), resolved_by_user_id: None, resolved_at: row.get("resolved_at"), created_at: row.get("created_at"), updated_at: row.get("updated_at") })
    }
    async fn comments(&self, routine_id: Uuid, thread_id: Uuid) -> Result<Vec<RoutineAnnotationComment>, String> {
        let rows = sqlx::query("SELECT id, company_id, thread_id, routine_id, document_id, body, author_type, author_agent_id, created_by_run_id, created_at, updated_at FROM document_annotation_comments WHERE routine_id = $1 AND thread_id = $2 ORDER BY created_at ASC").bind(routine_id).bind(thread_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| RoutineAnnotationComment { id:r.get("id"), company_id:r.get("company_id"), thread_id:r.get("thread_id"), routine_id:r.get("routine_id"), document_id:r.get("document_id"), body:r.get("body"), author_type:r.get("author_type"), author_agent_id:r.get("author_agent_id"), author_user_id:None, created_by_run_id:r.get("created_by_run_id"), created_at:r.get("created_at"), updated_at:r.get("updated_at") }).collect())
    }
}

#[async_trait]
impl RoutineAnnotationService for PgRoutineAnnotationService {
    async fn list_annotations(&self, routine_id: Uuid, include_comments: bool) -> Result<Vec<RoutineAnnotationThreadWithComments>, String> {
        let _ = self.document(routine_id).await?;
        let rows = sqlx::query("SELECT * FROM document_annotation_threads WHERE routine_id = $1 AND document_key = 'description' ORDER BY created_at ASC").bind(routine_id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let mut result = Vec::with_capacity(rows.len());
        for row in &rows {
            let thread = Self::thread(row)?;
            let comments = if include_comments { self.comments(routine_id, thread.id).await? } else { vec![] };
            result.push(RoutineAnnotationThreadWithComments { thread, comments });
        }
        Ok(result)
    }
    async fn create_annotation_thread(&self, routine_id: Uuid, request: CreateRoutineAnnotationThreadRequest) -> Result<RoutineAnnotationThreadWithComments, String> {
        let (company_id, document_id) = self.document(routine_id).await?;
        let s = &request.selector;
        let row = sqlx::query("INSERT INTO document_annotation_threads (company_id,routine_id,document_id,document_key,original_revision_id,original_revision_number,current_revision_id,current_revision_number,selected_text,prefix_text,suffix_text,normalized_start,normalized_end,markdown_start,markdown_end,anchor_selector) VALUES ($1,$2,$3,'description',$4,$5,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *").bind(company_id).bind(routine_id).bind(document_id).bind(request.base_revision_id).bind(request.base_revision_number).bind(&s.quote.exact).bind(&s.quote.prefix).bind(&s.quote.suffix).bind(s.position.normalized_start).bind(s.position.normalized_end).bind(s.position.markdown_start).bind(s.position.markdown_end).bind(serde_json::to_value(s).map_err(|e| e.to_string())?).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let thread = Self::thread(&row)?;
        let comment = self.add_comment(routine_id, thread.id, CreateRoutineAnnotationCommentRequest { body: request.body }).await?;
        Ok(RoutineAnnotationThreadWithComments { thread, comments: vec![comment] })
    }
    async fn add_comment(&self, routine_id: Uuid, thread_id: Uuid, request: CreateRoutineAnnotationCommentRequest) -> Result<RoutineAnnotationComment, String> {
        let (company_id, document_id) = self.document(routine_id).await?;
        let row = sqlx::query("INSERT INTO document_annotation_comments (company_id,thread_id,routine_id,document_id,body) VALUES ($1,$2,$3,$4,$5) RETURNING id,company_id,thread_id,routine_id,document_id,body,author_type,author_agent_id,created_by_run_id,created_at,updated_at").bind(company_id).bind(thread_id).bind(routine_id).bind(document_id).bind(request.body).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(RoutineAnnotationComment { id:row.get("id"), company_id:row.get("company_id"), thread_id:row.get("thread_id"), routine_id:row.get("routine_id"), document_id:row.get("document_id"), body:row.get("body"), author_type:row.get("author_type"), author_agent_id:row.get("author_agent_id"), author_user_id:None, created_by_run_id:row.get("created_by_run_id"), created_at:row.get("created_at"), updated_at:row.get("updated_at") })
    }
    async fn update_thread(&self, routine_id: Uuid, thread_id: Uuid, request: UpdateRoutineAnnotationThreadRequest) -> Result<RoutineAnnotationThread, String> {
        let status = request.status.map(|s| match s { AnnotationThreadStatus::Resolved => "resolved", AnnotationThreadStatus::Open => "open" });
        let row = sqlx::query("UPDATE document_annotation_threads SET status = COALESCE($3,status), resolved_at = CASE WHEN $3 = 'resolved' THEN NOW() ELSE NULL END, updated_at = NOW() WHERE id = $1 AND routine_id = $2 RETURNING *").bind(thread_id).bind(routine_id).bind(status).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?.ok_or_else(|| "annotation thread not found".to_string())?;
        Self::thread(&row)
    }
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
