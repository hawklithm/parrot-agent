use async_trait::async_trait;
use models::{
    Case, CaseStatus, CaseEvent, CaseEventKind, CaseQueryFilter, Pagination,
    CreateCaseInput, UpdateCaseInput, UpsertCaseInput, CaseDetail, CaseIssueLinkDetail,
    CaseParentRef, CaseDocumentRef, CaseAttachmentRef, Label, CaseIssueLink,
};
use uuid::Uuid;
use std::sync::Arc;
use repositories::{
    CaseRepository, CaseEventRepository, CaseIssueLinkRepository,
    RepositoryError,
};

/// Service-level errors for Case operations
#[derive(Debug, thiserror::Error)]
pub enum CaseServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Case not found: {0}")]
    NotFound(Uuid),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid status transition from {from:?} to {to:?}")]
    InvalidStatusTransition { from: CaseStatus, to: CaseStatus },
}

pub type CaseServiceResult<T> = Result<T, CaseServiceError>;

/// Case Service trait
#[async_trait]
pub trait CaseService: Send + Sync {
    /// Create a new case
    async fn create(&self, input: CreateCaseInput) -> CaseServiceResult<Case>;

    /// Get a case by ID
    async fn get(&self, id: Uuid) -> CaseServiceResult<Case>;

    /// List cases with filtering and pagination
    async fn list(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
        pagination: &Pagination,
    ) -> CaseServiceResult<Vec<Case>>;

    /// Count cases matching filter
    async fn count(&self, company_id: Uuid, filter: &CaseQueryFilter) -> CaseServiceResult<i64>;

    /// Update a case
    async fn update(&self, id: Uuid, input: UpdateCaseInput) -> CaseServiceResult<Case>;

    /// Upsert a case (create or update based on unique key)
    async fn upsert(&self, input: UpsertCaseInput) -> CaseServiceResult<(Case, bool)>;

    /// Get case by identifier
    async fn get_by_identifier(&self, identifier: &str) -> CaseServiceResult<Case>;

    /// Load full case detail with related entities
    async fn load_detail(&self, id: Uuid) -> CaseServiceResult<CaseDetail>;

    /// List child cases
    async fn list_children(&self, parent_case_id: Uuid, pagination: &Pagination) -> CaseServiceResult<Vec<Case>>;
}

/// Case Service implementation
pub struct CaseServiceImpl<CR, CER, CILR>
where
    CR: CaseRepository,
    CER: CaseEventRepository,
    CILR: CaseIssueLinkRepository,
{
    case_repository: Arc<CR>,
    event_repository: Arc<CER>,
    link_repository: Arc<CILR>,
}

impl<CR, CER, CILR> CaseServiceImpl<CR, CER, CILR>
where
    CR: CaseRepository,
    CER: CaseEventRepository,
    CILR: CaseIssueLinkRepository,
{
    pub fn new(
        case_repository: Arc<CR>,
        event_repository: Arc<CER>,
        link_repository: Arc<CILR>,
    ) -> Self {
        Self {
            case_repository,
            event_repository,
            link_repository,
        }
    }

    /// Record a case event
    async fn record_event(
        &self,
        case_id: Uuid,
        company_id: Uuid,
        kind: CaseEventKind,
        actor_type: Option<String>,
        actor_id: Option<Uuid>,
        actor_run_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> CaseServiceResult<()> {
        let event = CaseEvent {
            id: Uuid::new_v4(),
            company_id,
            case_id,
            kind,
            actor_type,
            actor_id,
            actor_run_id,
            payload,
            created_at: chrono::Utc::now(),
        };

        self.event_repository.create_event(event).await?;
        Ok(())
    }

    /// Validate status transition
    fn validate_status_transition(&self, from: CaseStatus, to: CaseStatus) -> CaseServiceResult<()> {
        // Simple validation - can be extended with a state machine
        match (from, to) {
            (CaseStatus::Draft, CaseStatus::InProgress) => Ok(()),
            (CaseStatus::Draft, CaseStatus::Cancelled) => Ok(()),
            (CaseStatus::InProgress, CaseStatus::InReview) => Ok(()),
            (CaseStatus::InProgress, CaseStatus::Cancelled) => Ok(()),
            (CaseStatus::InReview, CaseStatus::Approved) => Ok(()),
            (CaseStatus::InReview, CaseStatus::InProgress) => Ok(()),
            (CaseStatus::InReview, CaseStatus::Cancelled) => Ok(()),
            (CaseStatus::Approved, CaseStatus::Done) => Ok(()),
            (CaseStatus::Approved, CaseStatus::InProgress) => Ok(()),
            _ if from == to => Ok(()),
            _ => Err(CaseServiceError::InvalidStatusTransition { from, to }),
        }
    }
}

#[async_trait]
impl<CR, CER, CILR> CaseService for CaseServiceImpl<CR, CER, CILR>
where
    CR: CaseRepository + 'static,
    CER: CaseEventRepository + 'static,
    CILR: CaseIssueLinkRepository + 'static,
{
    async fn create(&self, input: CreateCaseInput) -> CaseServiceResult<Case> {
        let case = self.case_repository.create(input.clone()).await?;

        // Record created event
        let payload = serde_json::json!({
            "case_type": case.case_type,
            "title": case.title,
            "status": case.status,
        });

        self.record_event(
            case.id,
            case.company_id,
            CaseEventKind::Created,
            input.created_by_agent_id.map(|_| "agent".to_string())
                .or(input.created_by_user_id.map(|_| "user".to_string())),
            input.created_by_agent_id.or(input.created_by_user_id),
            input.created_by_run_id,
            payload,
        ).await?;

        Ok(case)
    }

    async fn get(&self, id: Uuid) -> CaseServiceResult<Case> {
        self.case_repository.get_by_id(id).await?
            .ok_or(CaseServiceError::NotFound(id))
    }

    async fn list(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
        pagination: &Pagination,
    ) -> CaseServiceResult<Vec<Case>> {
        let cases = self.case_repository.list_by_company(company_id, filter, pagination).await?;
        Ok(cases)
    }

    async fn count(&self, company_id: Uuid, filter: &CaseQueryFilter) -> CaseServiceResult<i64> {
        let count = self.case_repository.count_by_company(company_id, filter).await?;
        Ok(count)
    }

    async fn update(&self, id: Uuid, input: UpdateCaseInput) -> CaseServiceResult<Case> {
        // Get current case
        let current = self.get(id).await?;

        // Validate status transition if status is being changed
        if let Some(new_status) = input.status {
            self.validate_status_transition(current.status, new_status)?;
        }

        let updated = self.case_repository.update(id, input.clone()).await?;

        // Record updated event
        let mut changed_fields = Vec::new();
        if input.title.is_some() {
            changed_fields.push("title");
        }
        if input.summary.is_some() {
            changed_fields.push("summary");
        }
        if input.status.is_some() {
            changed_fields.push("status");
        }
        if input.fields.is_some() {
            changed_fields.push("fields");
        }

        let payload = serde_json::json!({
            "changed_fields": changed_fields,
            "new_status": input.status,
        });

        // Determine event kind
        let event_kind = if input.status.is_some() && input.status != Some(current.status) {
            CaseEventKind::StatusChanged
        } else {
            CaseEventKind::Updated
        };

        self.record_event(
            updated.id,
            updated.company_id,
            event_kind,
            None,
            None,
            None,
            payload,
        ).await?;

        Ok(updated)
    }

    async fn upsert(&self, input: UpsertCaseInput) -> CaseServiceResult<(Case, bool)> {
        let (case, created) = self.case_repository.upsert(input.clone()).await?;

        // Record event
        let event_kind = if created {
            CaseEventKind::Created
        } else {
            CaseEventKind::Updated
        };

        let payload = serde_json::json!({
            "case_type": case.case_type,
            "title": case.title,
            "upsert": true,
            "created": created,
        });

        self.record_event(
            case.id,
            case.company_id,
            event_kind,
            input.actor_agent_id.map(|_| "agent".to_string())
                .or(input.actor_user_id.map(|_| "user".to_string())),
            input.actor_agent_id.or(input.actor_user_id),
            input.actor_run_id,
            payload,
        ).await?;

        Ok((case, created))
    }

    async fn get_by_identifier(&self, identifier: &str) -> CaseServiceResult<Case> {
        self.case_repository.get_by_identifier(identifier).await?
            .ok_or_else(|| CaseServiceError::Validation(format!("Case not found: {}", identifier)))
    }

    async fn load_detail(&self, id: Uuid) -> CaseServiceResult<CaseDetail> {
        // Get main case
        let case = self.get(id).await?;

        // Get parent case if exists
        let parent = if let Some(parent_id) = case.parent_case_id {
            let parent_case = self.case_repository.get_by_id(parent_id).await?;
            parent_case.map(|p| CaseParentRef {
                id: p.id,
                identifier: p.identifier,
                title: p.title,
                case_type: p.case_type,
                status: p.status,
            })
        } else {
            None
        };

        // Get issue links
        let links = self.link_repository.list_by_case(case.id).await?;
        let issue_links: Vec<CaseIssueLinkDetail> = links.into_iter().map(|link| {
            CaseIssueLinkDetail {
                id: link.id,
                case_id: link.case_id,
                issue_id: link.issue_id,
                role: link.role,
                created_at: link.created_at,
                issue: models::CaseIssueSummary {
                    id: link.issue_id,
                    identifier: None, // Would need to join with issues table
                    title: String::new(), // Would need to join with issues table
                    status: models::issue::IssueStatus::Todo, // Would need to join
                },
            }
        }).collect();

        // TODO: Load labels, documents, attachments when those repositories are available
        let labels = Vec::new();
        let documents = Vec::new();
        let attachments = Vec::new();

        Ok(CaseDetail {
            case,
            parent,
            labels,
            issue_links,
            documents,
            attachments,
        })
    }

    async fn list_children(&self, parent_case_id: Uuid, pagination: &Pagination) -> CaseServiceResult<Vec<Case>> {
        let children = self.case_repository.list_by_parent(parent_case_id, pagination).await?;
        Ok(children)
    }
}
