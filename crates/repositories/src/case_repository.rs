use async_trait::async_trait;
use models::{Case, CaseQueryFilter, Pagination, CreateCaseInput, UpdateCaseInput, CaseEvent, CaseEventKind};
use uuid::Uuid;
use crate::RepositoryError;

#[async_trait]
pub trait CaseRepository: Send + Sync {
    /// Get a single case by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Case>, RepositoryError>;

    /// Get case by identifier
    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Case>, RepositoryError>;

    /// List cases by company with optional filtering and pagination
    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Case>, RepositoryError>;

    /// Count cases by company with optional filtering
    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
    ) -> Result<i64, RepositoryError>;

    /// Create a new case
    async fn create(&self, input: CreateCaseInput) -> Result<Case, RepositoryError>;

    /// Update an existing case
    async fn update(&self, id: Uuid, input: UpdateCaseInput) -> Result<Case, RepositoryError>;

    /// Upsert a case (update if exists by key, create otherwise)
    async fn upsert(&self, input: models::UpsertCaseInput) -> Result<(Case, bool), RepositoryError>; // Returns (case, created)

    /// Find case by company, type, and key
    async fn find_by_key(
        &self,
        company_id: Uuid,
        case_type: &str,
        key: &str,
    ) -> Result<Option<Case>, RepositoryError>;

    /// List cases by parent
    async fn list_by_parent(
        &self,
        parent_case_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Case>, RepositoryError>;

    /// Get cases by multiple IDs
    async fn get_by_ids(&self, ids: Vec<Uuid>) -> Result<Vec<Case>, RepositoryError>;

    /// Generate next case identity (case_number and identifier)
    async fn next_case_identity(&self, company_id: Uuid) -> Result<(i32, String), RepositoryError>;
}

#[async_trait]
pub trait CaseEventRepository: Send + Sync {
    /// Create a case event
    async fn create_event(&self, event: CaseEvent) -> Result<CaseEvent, RepositoryError>;

    /// List events for a case
    async fn list_by_case(
        &self,
        case_id: Uuid,
        limit: i64,
    ) -> Result<Vec<CaseEvent>, RepositoryError>;

    /// List events by kind
    async fn list_by_case_and_kind(
        &self,
        case_id: Uuid,
        kind: CaseEventKind,
        limit: i64,
    ) -> Result<Vec<CaseEvent>, RepositoryError>;
}
