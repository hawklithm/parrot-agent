use async_trait::async_trait;
use sqlx::PgPool;
use models::{
    Issue, IssueQueryFilter, Pagination, CreateIssueInput, UpdateIssueInput, IssueStatus,
};
use uuid::Uuid;
use crate::{issue_repository::IssueRepository, RepositoryError};

pub struct PgIssueRepository {
    pool: PgPool,
}

impl PgIssueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueRepository for PgIssueRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Issue>, RepositoryError> {
        let issue = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issue)
    }

    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let mut query = String::from("SELECT * FROM issues WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status = ANY(${})", param_count));
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND priority = ANY(${})", param_count));
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_agent_id = ${}", param_count));
        }

        if let Some(assignee_user_id) = filter.assignee_user_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_user_id = ${}", param_count));
        }

        if let Some(project_id) = filter.project_id {
            param_count += 1;
            query.push_str(&format!(" AND project_id = ${}", param_count));
        }

        if let Some(goal_id) = filter.goal_id {
            param_count += 1;
            query.push_str(&format!(" AND goal_id = ${}", param_count));
        }

        if let Some(parent_id) = filter.parent_id {
            param_count += 1;
            query.push_str(&format!(" AND parent_id = ${}", param_count));
        }

        if let Some(work_mode) = filter.work_mode {
            param_count += 1;
            query.push_str(&format!(" AND work_mode = ${}", param_count));
        }

        // Add ordering and pagination
        query.push_str(" ORDER BY updated_at DESC");
        param_count += 1;
        query.push_str(&format!(" LIMIT ${}", param_count));
        param_count += 1;
        query.push_str(&format!(" OFFSET ${}", param_count));

        // Build query with all parameters
        let mut q = sqlx::query_as::<_, Issue>(&query).bind(company_id);

        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                let status_strs: Vec<String> = statuses.iter().map(|s| format!("{:?}", s).to_lowercase()).collect();
                q = q.bind(status_strs);
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                let priority_strs: Vec<String> = priorities.iter().map(|p| format!("{:?}", p).to_lowercase()).collect();
                q = q.bind(priority_strs);
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }

        if let Some(assignee_user_id) = filter.assignee_user_id {
            q = q.bind(assignee_user_id);
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        if let Some(goal_id) = filter.goal_id {
            q = q.bind(goal_id);
        }

        if let Some(parent_id) = filter.parent_id {
            q = q.bind(parent_id);
        }

        if let Some(work_mode) = filter.work_mode {
            let mode_str = format!("{:?}", work_mode).to_lowercase();
            q = q.bind(mode_str);
        }

        q = q.bind(pagination.limit).bind(pagination.offset);

        let issues = q.fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(issues)
    }

    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
    ) -> Result<i64, RepositoryError> {
        let mut query = String::from("SELECT COUNT(*) as count FROM issues WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters (same logic as list_by_company)
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status = ANY(${})", param_count));
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND priority = ANY(${})", param_count));
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            param_count += 1;
            query.push_str(&format!(" AND assignee_agent_id = ${}", param_count));
        }

        if let Some(project_id) = filter.project_id {
            param_count += 1;
            query.push_str(&format!(" AND project_id = ${}", param_count));
        }

        let mut q = sqlx::query_scalar::<_, i64>(&query).bind(company_id);

        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                let status_strs: Vec<String> = statuses.iter().map(|s| format!("{:?}", s).to_lowercase()).collect();
                q = q.bind(status_strs);
            }
        }

        if let Some(priorities) = &filter.priority {
            if !priorities.is_empty() {
                let priority_strs: Vec<String> = priorities.iter().map(|p| format!("{:?}", p).to_lowercase()).collect();
                q = q.bind(priority_strs);
            }
        }

        if let Some(assignee_agent_id) = filter.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        let count = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(count)
    }

    async fn create(&self, input: CreateIssueInput) -> Result<Issue, RepositoryError> {
        let issue = sqlx::query_as::<_, Issue>(
            r#"
            INSERT INTO issues (
                company_id, project_id, project_workspace_id, goal_id, parent_id,
                title, description, status, work_mode, priority,
                assignee_agent_id, assignee_user_id,
                created_by_agent_id, created_by_user_id, responsible_user_id,
                origin_kind, origin_id, origin_run_id, request_depth,
                billing_code, assignee_adapter_overrides,
                execution_workspace_id, execution_workspace_preference
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12,
                $13, $14, $15,
                $16, $17, $18, $19,
                $20, $21,
                $22, $23
            )
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.project_id)
        .bind(input.project_workspace_id)
        .bind(input.goal_id)
        .bind(input.parent_id)
        .bind(&input.title)
        .bind(input.description.as_ref())
        .bind(input.status)
        .bind(input.work_mode.unwrap_or(models::IssueWorkMode::Standard))
        .bind(input.priority.unwrap_or(models::IssuePriority::Medium))
        .bind(input.assignee_agent_id)
        .bind(input.assignee_user_id)
        .bind(input.created_by_agent_id)
        .bind(input.created_by_user_id)
        .bind(input.responsible_user_id)
        .bind(input.origin_kind.as_ref())
        .bind(input.origin_id.as_ref())
        .bind(input.origin_run_id)
        .bind(input.request_depth.unwrap_or(0))
        .bind(input.billing_code.as_ref())
        .bind(&input.assignee_adapter_overrides)
        .bind(input.execution_workspace_id)
        .bind(input.execution_workspace_preference.as_ref())
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issue)
    }

    async fn update(&self, id: Uuid, input: UpdateIssueInput) -> Result<Issue, RepositoryError> {
        // Build dynamic UPDATE query
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.title.is_some() {
            param_count += 1;
            updates.push(format!("title = ${}", param_count));
        }
        if input.description.is_some() {
            param_count += 1;
            updates.push(format!("description = ${}", param_count));
        }
        if input.status.is_some() {
            param_count += 1;
            updates.push(format!("status = ${}", param_count));
        }
        if input.priority.is_some() {
            param_count += 1;
            updates.push(format!("priority = ${}", param_count));
        }
        if input.work_mode.is_some() {
            param_count += 1;
            updates.push(format!("work_mode = ${}", param_count));
        }
        if input.assignee_agent_id.is_some() {
            param_count += 1;
            updates.push(format!("assignee_agent_id = ${}", param_count));
        }
        if input.assignee_user_id.is_some() {
            param_count += 1;
            updates.push(format!("assignee_user_id = ${}", param_count));
        }
        if input.responsible_user_id.is_some() {
            param_count += 1;
            updates.push(format!("responsible_user_id = ${}", param_count));
        }
        if input.execution_policy.is_some() {
            param_count += 1;
            updates.push(format!("execution_policy = ${}", param_count));
        }
        if input.execution_state.is_some() {
            param_count += 1;
            updates.push(format!("execution_state = ${}", param_count));
        }
        if input.monitor_notes.is_some() {
            param_count += 1;
            updates.push(format!("monitor_notes = ${}", param_count));
        }
        if input.monitor_scheduled_by.is_some() {
            param_count += 1;
            updates.push(format!("monitor_scheduled_by = ${}", param_count));
        }
        if input.execution_workspace_preference.is_some() {
            param_count += 1;
            updates.push(format!("execution_workspace_preference = ${}", param_count));
        }
        if input.execution_workspace_settings.is_some() {
            param_count += 1;
            updates.push(format!("execution_workspace_settings = ${}", param_count));
        }
        if input.hidden_at.is_some() {
            param_count += 1;
            updates.push(format!("hidden_at = ${}", param_count));
        }
        if input.source_trust.is_some() {
            param_count += 1;
            updates.push(format!("source_trust = ${}", param_count));
        }

        if updates.is_empty() {
            // No fields to update, just return the existing issue
            return self.get_by_id(id).await?.ok_or_else(|| RepositoryError::NotFound(id));
        }

        updates.push("updated_at = NOW()".to_string());

        let query = format!(
            "UPDATE issues SET {} WHERE id = $1 RETURNING *",
            updates.join(", ")
        );

        let mut q = sqlx::query_as::<_, Issue>(&query).bind(id);

        // Bind all parameters in the same order as updates
        if let Some(ref title) = input.title {
            q = q.bind(title);
        }
        if let Some(ref description) = input.description {
            q = q.bind(description);
        }
        if let Some(status) = input.status {
            q = q.bind(status);
        }
        if let Some(priority) = input.priority {
            q = q.bind(priority);
        }
        if let Some(work_mode) = input.work_mode {
            q = q.bind(work_mode);
        }
        if let Some(assignee_agent_id) = input.assignee_agent_id {
            q = q.bind(assignee_agent_id);
        }
        if let Some(assignee_user_id) = input.assignee_user_id {
            q = q.bind(assignee_user_id);
        }
        if let Some(responsible_user_id) = input.responsible_user_id {
            q = q.bind(responsible_user_id);
        }
        if let Some(ref execution_policy) = input.execution_policy {
            q = q.bind(execution_policy);
        }
        if let Some(ref execution_state) = input.execution_state {
            q = q.bind(execution_state);
        }
        if let Some(ref monitor_notes) = input.monitor_notes {
            q = q.bind(monitor_notes);
        }
        if let Some(monitor_scheduled_by) = input.monitor_scheduled_by {
            q = q.bind(monitor_scheduled_by);
        }
        if let Some(ref execution_workspace_preference) = input.execution_workspace_preference {
            q = q.bind(execution_workspace_preference);
        }
        if let Some(ref execution_workspace_settings) = input.execution_workspace_settings {
            q = q.bind(execution_workspace_settings);
        }
        if let Some(hidden_at) = input.hidden_at {
            q = q.bind(hidden_at);
        }
        if let Some(ref source_trust) = input.source_trust {
            q = q.bind(source_trust);
        }

        let issue = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(issue)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Soft delete by setting status to cancelled
        sqlx::query(
            r#"
            UPDATE issues SET status = 'cancelled', cancelled_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }

    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues
            WHERE company_id = $1
              AND (
                title ILIKE $2
                OR description ILIKE $2
                OR identifier ILIKE $2
              )
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(company_id)
        .bind(format!("%{}%", query))
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issues)
    }

    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Issue>, RepositoryError> {
        let issue = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE identifier = $1
            "#,
        )
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issue)
    }

    async fn list_by_parent(
        &self,
        parent_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        let issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues
            WHERE parent_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(parent_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issues)
    }

    async fn get_by_ids(&self, ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let issues = sqlx::query_as::<_, Issue>(
            r#"
            SELECT * FROM issues WHERE id = ANY($1)
            "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(issues)
    }
}
