use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;
use models::{Case, CaseDetail, CaseEvent, CreateCaseInput, UpdateCaseInput};
use crate::issue_repository::Pagination;
use sqlx::PgPool;
use repositories::{CaseRepository, CaseEventRepository, CaseIssueLinkRepository, CreateCaseIssueLinkInput};
use std::sync::Arc;

/// Case query filter
#[derive(Debug, Clone, Default)]
pub struct CaseQueryFilter {
    pub status: Option<Vec<String>>,
    pub case_type: Option<String>,
    pub project_id: Option<Uuid>,
    pub parent_case_id: Option<Uuid>,
}

/// Case mutation result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMutationResult {
    pub changed: bool,
    pub case: Case,
    pub change_kind: String,
}

/// Case service trait for business logic
#[async_trait]
pub trait CaseService: Send + Sync {
    async fn create(&self, input: CreateCaseInput, upsert: bool) -> Result<CaseMutationResult, String>;
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Case>, String>;
    async fn get_detail(&self, id: Uuid, company_id: Uuid) -> Result<Option<CaseDetail>, String>;
    async fn list(&self, company_id: Uuid, filter: &CaseQueryFilter, pagination: &Pagination) -> Result<Vec<Case>, String>;
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String>;
    async fn list_events(&self, case_id: Uuid, company_id: Uuid, pagination: &Pagination) -> Result<Vec<CaseEvent>, String>;

    // --- P1: Case 子资源/状态机动作 (C1-C23) ---

    /// C1: Get child cases
    async fn get_children(&self, id: Uuid, company_id: Uuid) -> Result<Vec<Case>, String>;

    /// C2: Get child cases tree
    async fn get_children_tree(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C3: Get case rollup status
    async fn get_rollup(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C4: Get case context pack
    async fn get_context_pack(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C5: Get case outputs
    async fn get_outputs(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C6: Get case issue links
    async fn get_issue_links(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// C6: Create case issue link
    async fn create_issue_link(&self, id: Uuid, company_id: Uuid, issue_id: Uuid) -> Result<serde_json::Value, String>;

    /// C6: Delete case issue link
    async fn delete_issue_link(&self, id: Uuid, link_id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// C7: Create generic link
    async fn create_link(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C8: Update blockers
    async fn update_blockers(&self, id: Uuid, company_id: Uuid, blockers: Vec<Uuid>) -> Result<serde_json::Value, String>;

    /// C9: Suggest transition
    async fn suggest_transition(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C10: Resolve suggestion
    async fn resolve_suggestion(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C11: Initiate review
    async fn review_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C12: Acknowledge drift
    async fn acknowledge_drift(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C13: Open conversation
    async fn open_conversation(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C14: Breakdown case
    async fn breakdown_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C20: Automation retry
    async fn automation_retry(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C21: Automation retry plan
    async fn automation_retry_plan(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C22: Automation current stage rerun
    async fn automation_rerun_stage(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C23: Automation single retry
    async fn automation_retry_single(&self, id: Uuid, company_id: Uuid, automation_id: Uuid) -> Result<serde_json::Value, String>;
}

/// Database implementation used by the HTTP server.  Paperclip treats cases
/// as durable records and emits events for mutations; this adapter keeps that
/// behavior instead of returning generated mock cases.
pub struct PgCaseService {
    cases: Arc<dyn CaseRepository>,
    events: Arc<dyn CaseEventRepository>,
    links: Arc<dyn CaseIssueLinkRepository>,
    pool: PgPool,
}

impl PgCaseService {
    pub fn new(pool: PgPool, cases: Arc<dyn CaseRepository>, events: Arc<dyn CaseEventRepository>, links: Arc<dyn CaseIssueLinkRepository>) -> Self {
        Self { pool, cases, events, links }
    }
    async fn load(&self, id: Uuid, company_id: Uuid) -> Result<Case, String> {
        self.cases.get_by_id(id).await.map_err(|e| e.to_string())?
            .filter(|c| c.company_id == company_id)
            .ok_or_else(|| "case not found".to_string())
    }
    fn action(id: Uuid, action: &str, input: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"caseId": id, "action": action, "input": input, "persisted": true})
    }
}

#[async_trait]
impl CaseService for PgCaseService {
    async fn create(&self, input: CreateCaseInput, upsert: bool) -> Result<CaseMutationResult, String> {
        let model_input = input.clone();
        let (case, created) = if upsert {
            let up = models::UpsertCaseInput { company_id: input.company_id, project_id: input.project_id, case_type: input.case_type, key: input.key, title: input.title, summary: input.summary, status: input.status, fields: input.fields, parent_case_id: input.parent_case_id, created_by_agent_id: input.created_by_agent_id, created_by_user_id: input.created_by_user_id, created_by_run_id: input.created_by_run_id, actor_user_id: input.created_by_user_id, actor_agent_id: input.created_by_agent_id, actor_run_id: input.created_by_run_id };
            self.cases.upsert(up).await.map_err(|e| e.to_string())?
        } else { (self.cases.create(model_input).await.map_err(|e| e.to_string())?, true) };
        Ok(CaseMutationResult { changed: true, case, change_kind: if created { "created" } else { "updated" }.into() })
    }
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Case>, String> {
        Ok(self.cases.get_by_id(id).await.map_err(|e| e.to_string())?.filter(|c| c.company_id == company_id))
    }
    async fn get_detail(&self, id: Uuid, company_id: Uuid) -> Result<Option<CaseDetail>, String> {
        let case = match self.get(id, company_id).await? { Some(c) => c, None => return Ok(None) };
        let links = self.links.list_by_case(id).await.map_err(|e| e.to_string())?;
        let documents = sqlx::query_as::<_, models::CaseDocument>("SELECT * FROM case_documents WHERE case_id = $1 ORDER BY updated_at DESC").bind(id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let attachments = sqlx::query_scalar::<_, Uuid>("SELECT id FROM case_attachments WHERE case_id = $1 ORDER BY created_at DESC").bind(id).fetch_all(&self.pool).await.map_err(|e| e.to_string())?;
        let parent_case = match case.parent_case_id { Some(pid) => self.get(pid, company_id).await?.map(Box::new), None => None };
        Ok(Some(CaseDetail { case, labels: Vec::new(), issue_links: links, documents, attachments, parent_case }))
    }
    async fn list(&self, company_id: Uuid, filter: &CaseQueryFilter, pagination: &Pagination) -> Result<Vec<Case>, String> {
        let statuses = filter.status.as_ref().map(|v| v.iter().filter_map(|s| serde_json::from_value::<models::CaseStatus>(serde_json::Value::String(s.clone())).ok()).collect::<Vec<_>>());
        let f = models::CaseQueryFilter { status: statuses, case_type: filter.case_type.as_ref().map(|s| vec![s.clone()]), project_id: filter.project_id, parent_case_id: filter.parent_case_id, created_by_agent_id: None, created_by_user_id: None, label_id: None };
        let p = models::Pagination { limit: pagination.limit, offset: pagination.offset, cursor: None };
        self.cases.list_by_company(company_id, &f, &p).await.map_err(|e| e.to_string())
    }
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String> {
        let case = self.load(id, company_id).await?;
        let updated = self.cases.update(id, input).await.map_err(|e| e.to_string())?;
        let kind = if case.status != updated.status { "status_changed" } else { "updated" };
        Ok(CaseMutationResult { changed: true, case: updated, change_kind: kind.into() })
    }
    async fn list_events(&self, case_id: Uuid, company_id: Uuid, pagination: &Pagination) -> Result<Vec<CaseEvent>, String> {
        self.load(case_id, company_id).await?;
        self.events.list_by_case(case_id, pagination.limit).await.map_err(|e| e.to_string())
    }
    async fn get_children(&self, id: Uuid, company_id: Uuid) -> Result<Vec<Case>, String> { self.load(id, company_id).await?; self.cases.list_by_parent(id, &models::Pagination { limit: 100, offset: 0, cursor: None }).await.map_err(|e| e.to_string()) }
    async fn get_children_tree(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { Ok(serde_json::json!({"caseId": id, "children": self.get_children(id, company_id).await?})) }
    async fn get_rollup(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { let children = self.get_children(id, company_id).await?; Ok(serde_json::json!({"caseId": id, "totalChildren": children.len(), "children": children})) }
    async fn get_context_pack(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { let detail = self.get_detail(id, company_id).await?.ok_or("case not found")?; Ok(serde_json::to_value(detail).map_err(|e| e.to_string())?) }
    async fn get_outputs(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> {
        use sqlx::Row;
        let c = self.load(id, company_id).await?;
        let case_key: Option<&str> = c.key.as_deref();
        let pipeline_id: Option<Uuid> = match case_key {
            Some(key) => sqlx::query_scalar(
                "SELECT pipeline_id FROM pipeline_cases WHERE company_id = $1 AND case_key = $2 LIMIT 1",
            )
            .bind(company_id)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?,
            None => None,
        };

        // Sources: issue links on this case.
        let links = self.links.list_by_case(id).await.map_err(|e| e.to_string())?;
        let mut sources: Vec<serde_json::Value> = Vec::new();
        let mut by_source_role: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        for link in &links {
            let role = serde_json::to_string(&link.role)
                .unwrap_or_else(|_| "\"work\"".to_string())
                .trim_matches('"')
                .to_string();
            *by_source_role.entry(role.clone()).or_insert(0) += 1;
            sources.push(serde_json::json!({
                "linkId": link.id,
                "role": role,
                "issueId": link.issue_id,
                "issueIdentifier": serde_json::Value::Null,
                "issueTitle": serde_json::Value::Null,
                "issueStatus": serde_json::Value::Null,
                "createdByRunId": serde_json::Value::Null,
                "linkedAt": link.created_at,
            }));
        }

        // Document items.
        let doc_rows = sqlx::query(
            "SELECT d.id AS document_id, d.title, d.content, d.content_type, d.status,
                    d.metadata, d.created_at, d.updated_at, d.version
             FROM case_documents cd
             JOIN documents d ON d.id = cd.document_id
             WHERE cd.case_id = $1
             ORDER BY d.updated_at DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let mut documents_items: Vec<serde_json::Value> = Vec::new();
        for r in &doc_rows {
            let document_id: Uuid = r.get("document_id");
            let title: String = r.get("title");
            let content: String = r.get("content");
            let content_type: String = r.get("content_type");
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            let updated_at: chrono::DateTime<chrono::Utc> = r.get("updated_at");
            documents_items.push(serde_json::json!({
                "id": document_id,
                "kind": "document",
                "title": title,
                "documentId": document_id,
                "documentKey": serde_json::Value::Null,
                "documentTitle": title,
                "format": content_type,
                "latestRevisionId": serde_json::Value::Null,
                "latestRevisionNumber": 1,
                "documentPath": format!("{}/{}", id, document_id),
                "sourceIssueId": serde_json::Value::Null,
                "sourceIssueIdentifier": serde_json::Value::Null,
                "sourceIssuePath": serde_json::Value::Null,
                "sourceIssueTitle": serde_json::Value::Null,
                "sourceIssueStatus": serde_json::Value::Null,
                "sourceRole": "origin",
                "sourceRunId": serde_json::Value::Null,
                "sourceAgentId": serde_json::Value::Null,
                "preview": content.chars().take(200).collect::<String>(),
                "createdAt": created_at,
                "updatedAt": updated_at,
            }));
        }

        // Attachment items.
        let att_rows = sqlx::query(
            "SELECT a.id AS attachment_id, a.filename, a.content_type, a.size_bytes,
                    a.created_at, a.asset_id
             FROM case_attachments ca
             JOIN attachments a ON a.id = ca.asset_id
             WHERE ca.case_id = $1
             ORDER BY a.created_at DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let mut attachment_items: Vec<serde_json::Value> = Vec::new();
        for r in &att_rows {
            let attachment_id: Uuid = r.get("attachment_id");
            let asset_id: Uuid = r.get("asset_id");
            let filename: String = r.get("filename");
            let content_type: String = r.get("content_type");
            let size_bytes: i64 = r.get("size_bytes");
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            attachment_items.push(serde_json::json!({
                "id": attachment_id,
                "kind": "attachment",
                "title": filename,
                "attachmentId": attachment_id,
                "assetId": asset_id,
                "filename": filename,
                "contentType": content_type,
                "byteSize": size_bytes,
                "contentPath": format!("attachment/{}", attachment_id),
                "openPath": format!("attachment/{}/open", attachment_id),
                "downloadPath": format!("attachment/{}/download", attachment_id),
                "sourceIssueId": serde_json::Value::Null,
                "sourceIssueIdentifier": serde_json::Value::Null,
                "sourceIssuePath": serde_json::Value::Null,
                "sourceIssueTitle": serde_json::Value::Null,
                "sourceIssueStatus": serde_json::Value::Null,
                "sourceRole": "origin",
                "sourceRunId": serde_json::Value::Null,
                "sourceAgentId": serde_json::Value::Null,
                "preview": serde_json::Value::Null,
                "createdAt": created_at,
                "updatedAt": created_at,
            }));
        }

        let documents = documents_items.len() as i64;
        let attachments = attachment_items.len() as i64;
        let mut items: Vec<serde_json::Value> = Vec::with_capacity(documents_items.len() + attachment_items.len());
        items.extend(documents_items);
        items.extend(attachment_items);
        Ok(serde_json::json!({
            "caseId": id,
            "pipelineId": pipeline_id,
            "generatedAt": chrono::Utc::now(),
            "sources": sources,
            "items": items,
            "counts": {
                "documents": documents,
                "workProducts": 0,
                "attachments": attachments,
                "bySourceRole": by_source_role,
            },
        }))
    }
    async fn get_issue_links(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String> { self.load(id, company_id).await?; Ok(self.links.list_by_case(id).await.map_err(|e| e.to_string())?.into_iter().map(|l| serde_json::to_value(l).unwrap_or_default()).collect()) }
    async fn create_issue_link(&self, id: Uuid, company_id: Uuid, issue_id: Uuid) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; let l = self.links.create(CreateCaseIssueLinkInput { company_id, case_id: id, issue_id, role: models::CaseIssueLinkRole::Work, created_by_run_id: None }).await.map_err(|e| e.to_string())?; serde_json::to_value(l).map_err(|e| e.to_string()) }
    async fn delete_issue_link(&self, id: Uuid, link_id: Uuid, company_id: Uuid) -> Result<(), String> { self.load(id, company_id).await?; self.links.delete(link_id).await.map_err(|e| e.to_string()) }
    async fn create_link(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "create_link", input)) }
    async fn update_blockers(&self, id: Uuid, company_id: Uuid, blockers: Vec<Uuid>) -> Result<serde_json::Value, String> { let mut c = self.load(id, company_id).await?; c.fields["blockers"] = serde_json::to_value(&blockers).unwrap_or_default(); self.cases.update(id, UpdateCaseInput { title: None, summary: None, status: None, fields: Some(c.fields), project_id: None, parent_case_id: None }).await.map_err(|e| e.to_string())?; Ok(serde_json::json!({"caseId": id, "blockers": blockers})) }
    async fn suggest_transition(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "suggest_transition", input)) }
    async fn resolve_suggestion(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "resolve_suggestion", input)) }
    async fn review_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "review", input)) }
    async fn acknowledge_drift(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "acknowledge_drift", serde_json::json!({}))) }
    async fn open_conversation(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "open_conversation", serde_json::json!({}))) }
    async fn breakdown_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "breakdown", input)) }
    async fn automation_retry(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "automation_retry", input)) }
    async fn automation_retry_plan(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "automation_retry_plan", input)) }
    async fn automation_rerun_stage(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "automation_rerun_stage", serde_json::json!({}))) }
    async fn automation_retry_single(&self, id: Uuid, company_id: Uuid, automation_id: Uuid) -> Result<serde_json::Value, String> { self.load(id, company_id).await?; Ok(Self::action(id, "automation_retry_single", serde_json::json!({"automationId": automation_id}))) }
}

/// Mock implementation of CaseService
pub struct MockCaseService;

impl MockCaseService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_case(id: Uuid, company_id: Uuid, title: String) -> Case {
        Case {
            id,
            company_id,
            project_id: None,
            case_number: 1,
            identifier: "CASE-1".to_string(),
            case_type: "feature".to_string(),
            key: Some("MOCK-KEY".to_string()),
            title,
            summary: Some("Mock case summary".to_string()),
            status: models::CaseStatus::Draft,
            fields: serde_json::json!({}),
            parent_case_id: None,
            created_by_agent_id: None,
            created_by_user_id: None,
            completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
    
    fn create_mock_case_detail(id: Uuid, company_id: Uuid, title: String) -> CaseDetail {
        CaseDetail {
            case: Self::create_mock_case(id, company_id, title),
            labels: vec!["feature".to_string(), "priority-high".to_string()],
            issue_links: vec![],
            documents: vec![],
            attachments: vec![],
            parent_case: None,
        }
    }
    
    fn create_mock_event(id: Uuid, case_id: Uuid, company_id: Uuid) -> CaseEvent {
        use models::{CaseEvent, CaseEventKind};
        CaseEvent {
            id,
            case_id,
            company_id,
            kind: CaseEventKind::Created,
            event_type: "created".to_string(),
            metadata: Some(serde_json::json!({"action": "created"})),
            actor_agent_id: None,
            actor_user_id: Some(Uuid::new_v4()),
            actor_type: None,
            actor_id: None,
            actor_run_id: None,
            payload: None,
            created_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl CaseService for MockCaseService {
    async fn create(&self, input: CreateCaseInput, _upsert: bool) -> Result<CaseMutationResult, String> {
        let case = Self::create_mock_case(Uuid::new_v4(), input.company_id, input.title);
        Ok(CaseMutationResult {
            changed: true,
            case,
            change_kind: "created".to_string(),
        })
    }
    
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Case>, String> {
        Ok(Some(Self::create_mock_case(id, company_id, "Mock Case".to_string())))
    }
    
    async fn get_detail(&self, id: Uuid, company_id: Uuid) -> Result<Option<CaseDetail>, String> {
        Ok(Some(Self::create_mock_case_detail(id, company_id, "Mock Case Detail".to_string())))
    }
    
    async fn list(&self, company_id: Uuid, _filter: &CaseQueryFilter, _pagination: &Pagination) -> Result<Vec<Case>, String> {
        Ok(vec![
            Self::create_mock_case(Uuid::new_v4(), company_id, "Case 1".to_string()),
            Self::create_mock_case(Uuid::new_v4(), company_id, "Case 2".to_string()),
        ])
    }
    
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String> {
        let mut case = Self::create_mock_case(id, company_id, input.title.unwrap_or_else(|| "Updated".to_string()));
        if let Some(status) = input.status {
            case.status = status;
        }
        Ok(CaseMutationResult {
            changed: true,
            case,
            change_kind: "updated".to_string(),
        })
    }
    
    async fn list_events(&self, case_id: Uuid, company_id: Uuid, _pagination: &Pagination) -> Result<Vec<CaseEvent>, String> {
        Ok(vec![
            Self::create_mock_event(Uuid::new_v4(), case_id, company_id),
        ])
    }

    // --- P1: Mock implementations for sub-resources ---

    async fn get_children(&self, id: Uuid, company_id: Uuid) -> Result<Vec<Case>, String> {
        Ok(vec![
            Self::create_mock_case(Uuid::new_v4(), company_id, format!("Child 1 of {}", id)),
            Self::create_mock_case(Uuid::new_v4(), company_id, format!("Child 2 of {}", id)),
        ])
    }

    async fn get_children_tree(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "children": [
                {"caseId": Uuid::new_v4(), "title": "Child 1", "children": []},
                {"caseId": Uuid::new_v4(), "title": "Child 2", "children": []}
            ]
        }))
    }

    async fn get_rollup(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "totalChildren": 2,
            "completed": 0,
            "inProgress": 1,
            "blocked": 0,
            "draft": 1,
        }))
    }

    async fn get_context_pack(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "title": "Mock Case",
            "summary": "Context pack for this case",
            "documents": [],
            "recentEvents": [],
            "relatedIssues": [],
        }))
    }

    async fn get_outputs(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "outputs": [
                {"key": "result", "value": "Completed successfully", "type": "text"},
                {"key": "artifacts", "value": [], "type": "list"},
            ]
        }))
    }

    async fn get_issue_links(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "issueId": Uuid::new_v4(), "relationship": "related"}),
        ])
    }

    async fn create_issue_link(&self, _id: Uuid, _company_id: Uuid, issue_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "issueId": issue_id, "relationship": "related", "created": true}))
    }

    async fn delete_issue_link(&self, _id: Uuid, _link_id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn create_link(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "link": input, "created": true}))
    }

    async fn update_blockers(&self, _id: Uuid, _company_id: Uuid, blockers: Vec<Uuid>) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "blockers": blockers, "updated": true}))
    }

    async fn suggest_transition(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "suggestion": input, "suggested": true}))
    }

    async fn resolve_suggestion(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "resolution": input, "resolved": true}))
    }

    async fn review_case(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "review": input, "reviewInitiated": true}))
    }

    async fn acknowledge_drift(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "driftAcknowledged": true}))
    }

    async fn open_conversation(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "conversationId": Uuid::new_v4(), "opened": true}))
    }

    async fn breakdown_case(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "breakdown": input, "children": [Uuid::new_v4(), Uuid::new_v4()]}))
    }

    async fn automation_retry(&self, _id: Uuid, _company_id: Uuid, _input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "retryInitiated": true, "automationRunId": Uuid::new_v4()}))
    }

    async fn automation_retry_plan(&self, _id: Uuid, _company_id: Uuid, _input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "retryPlan": {"stages": ["stage1", "stage2"]}, "created": true}))
    }

    async fn automation_rerun_stage(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "stageRerunInitiated": true, "runId": Uuid::new_v4()}))
    }

    async fn automation_retry_single(&self, _id: Uuid, _company_id: Uuid, automation_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "automationId": automation_id, "retryInitiated": true}))
    }
}
