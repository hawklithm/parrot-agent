use models::{Issue, IssuePlanDecomposition};
use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;

/// Service for managing issue plan decompositions
/// Tracks when an agent breaks down a plan into child issues
pub struct IssuePlanDecompositionService {
    pool: PgPool,
}

impl IssuePlanDecompositionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Submit a plan decomposition (agent proposes child tasks)
    /// Creates a decomposition record + confirmation interaction
    pub async fn submit_plan_decomposition(
        &self,
        source_issue: &Issue,
        accepted_plan_revision_id: Option<Uuid>,
        child_issue_ids: Vec<Uuid>,
        owner_agent_id: Option<Uuid>,
        owner_user_id: Option<String>,
        owner_run_id: Option<Uuid>,
    ) -> Result<IssuePlanDecomposition, String> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let decomposition = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            INSERT INTO issue_plan_decompositions (
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status, child_issue_ids, owner_agent_id, owner_user_id, owner_run_id,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 'in_flight'::text, $5, $6, $7, $8, $9, $9)
            RETURNING 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(source_issue.company_id)
        .bind(source_issue.id)
        .bind(accepted_plan_revision_id)
        .bind(serde_json::to_value(&child_issue_ids).unwrap())
        .bind(owner_agent_id)
        .bind(owner_user_id)
        .bind(owner_run_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to create plan decomposition: {}", e))?;

        Ok(decomposition)
    }

    /// Accept a plan decomposition (user confirms the child tasks)
    /// This transitions the decomposition from in_flight → completed
    pub async fn accept_plan_decomposition(
        &self,
        decomposition_id: Uuid,
    ) -> Result<IssuePlanDecomposition, String> {
        let now = Utc::now();

        let decomposition = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            UPDATE issue_plan_decompositions
            SET status = 'completed'::text,
                completed_at = $2,
                updated_at = $2
            WHERE id = $1
            RETURNING 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            "#,
        )
        .bind(decomposition_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to accept decomposition: {}", e))?
        .ok_or_else(|| "Decomposition not found".to_string())?;

        Ok(decomposition)
    }

    /// Cancel a plan decomposition (user rejects the suggested tasks)
    pub async fn cancel_plan_decomposition(
        &self,
        decomposition_id: Uuid,
    ) -> Result<IssuePlanDecomposition, String> {
        let now = Utc::now();

        let decomposition = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            UPDATE issue_plan_decompositions
            SET status = 'cancelled'::text,
                completed_at = $2,
                updated_at = $2
            WHERE id = $1
            RETURNING 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            "#,
        )
        .bind(decomposition_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to cancel decomposition: {}", e))?
        .ok_or_else(|| "Decomposition not found".to_string())?;

        Ok(decomposition)
    }

    /// Get decomposition by ID
    pub async fn get_decomposition(
        &self,
        decomposition_id: Uuid,
    ) -> Result<Option<IssuePlanDecomposition>, String> {
        let decomposition = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            SELECT 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            FROM issue_plan_decompositions
            WHERE id = $1
            "#,
        )
        .bind(decomposition_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to get decomposition: {}", e))?;

        Ok(decomposition)
    }

    /// List all decompositions for a source issue
    pub async fn list_for_issue(
        &self,
        source_issue_id: Uuid,
    ) -> Result<Vec<IssuePlanDecomposition>, String> {
        let decompositions = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            SELECT 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            FROM issue_plan_decompositions
            WHERE source_issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(source_issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to list decompositions: {}", e))?;

        Ok(decompositions)
    }

    /// Find active decompositions owned by an agent
    pub async fn find_active_by_agent(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
    ) -> Result<Vec<IssuePlanDecomposition>, String> {
        let decompositions = sqlx::query_as::<_, IssuePlanDecomposition>(
            r#"
            SELECT 
                id, company_id, source_issue_id, accepted_plan_revision_id,
                status::text as status, child_issue_ids, owner_agent_id, owner_user_id,
                owner_run_id, completed_at, created_at, updated_at
            FROM issue_plan_decompositions
            WHERE company_id = $1 
              AND owner_agent_id = $2 
              AND status = 'in_flight'::text
            ORDER BY created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to find active decompositions: {}", e))?;

        Ok(decompositions)
    }
}
