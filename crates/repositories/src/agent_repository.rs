use async_trait::async_trait;
use models::{Agent, AgentStatus};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Agent not found: {0}")]
    NotFound(Uuid),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

/// Agent Repository trait
#[async_trait]
pub trait AgentRepository: Send + Sync {
    /// Create a new agent
    async fn create(&self, agent: Agent) -> RepositoryResult<Agent>;

    /// Get agent by ID
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Agent>;

    /// List all agents for a company
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Agent>>;

    /// Update an existing agent
    async fn update(&self, agent: Agent) -> RepositoryResult<Agent>;

    /// Delete an agent (soft delete by setting status to terminated)
    async fn delete(&self, id: Uuid) -> RepositoryResult<()>;

    /// Get agents by status
    async fn list_by_status(&self, company_id: Uuid, status: AgentStatus) -> RepositoryResult<Vec<Agent>>;
}
