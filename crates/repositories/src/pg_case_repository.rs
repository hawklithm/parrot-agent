use async_trait::async_trait;
use sqlx::PgPool;
use models::{
    Case, CaseQueryFilter, Pagination, CreateCaseInput, UpdateCaseInput,
    CaseEvent, CaseEventKind, UpsertCaseInput, CaseStatus,
};
use uuid::Uuid;
use crate::{case_repository::{CaseRepository, CaseEventRepository}, RepositoryError};

pub struct PgCaseRepository {
    pool: PgPool,
}

impl PgCaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CaseRepository for PgCaseRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Case>, RepositoryError> {
        let case = sqlx::query_as::<_, Case>(
            r#"
            SELECT * FROM cases WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(case)
    }

    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Case>, RepositoryError> {
        let case = sqlx::query_as::<_, Case>(
            r#"
            SELECT * FROM cases WHERE identifier = $1
            "#,
        )
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(case)
    }

    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Case>, RepositoryError> {
        let mut query = String::from("SELECT * FROM cases WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status = ANY(${})", param_count));
            }
        }

        if let Some(case_types) = &filter.case_type {
            if !case_types.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND case_type = ANY(${})", param_count));
            }
        }

        if let Some(project_id) = filter.project_id {
            param_count += 1;
            query.push_str(&format!(" AND project_id = ${}", param_count));
        }

        if let Some(parent_case_id) = filter.parent_case_id {
            param_count += 1;
            query.push_str(&format!(" AND parent_case_id = ${}", param_count));
        }

        if let Some(label_id) = filter.label_id {
            param_count += 1;
            query.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM case_labels WHERE case_id = cases.id AND label_id = ${})",
                param_count
            ));
        }

        // Add ordering and pagination
        query.push_str(" ORDER BY updated_at DESC");
        param_count += 1;
        query.push_str(&format!(" LIMIT ${}", param_count));
        param_count += 1;
        query.push_str(&format!(" OFFSET ${}", param_count));

        // Build query with all parameters
        let mut q = sqlx::query_as::<_, Case>(&query).bind(company_id);

        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                let status_strs: Vec<String> = statuses.iter().map(|s| format!("{:?}", s).to_lowercase()).collect();
                q = q.bind(status_strs);
            }
        }

        if let Some(case_types) = &filter.case_type {
            if !case_types.is_empty() {
                q = q.bind(case_types);
            }
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        if let Some(parent_case_id) = filter.parent_case_id {
            q = q.bind(parent_case_id);
        }

        if let Some(label_id) = filter.label_id {
            q = q.bind(label_id);
        }

        q = q.bind(pagination.limit).bind(pagination.offset);

        let cases = q.fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(cases)
    }

    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &CaseQueryFilter,
    ) -> Result<i64, RepositoryError> {
        let mut query = String::from("SELECT COUNT(*) as count FROM cases WHERE company_id = $1");
        let mut param_count = 1;

        // Build dynamic query based on filters
        if let Some(statuses) = &filter.status {
            if !statuses.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND status = ANY(${})", param_count));
            }
        }

        if let Some(case_types) = &filter.case_type {
            if !case_types.is_empty() {
                param_count += 1;
                query.push_str(&format!(" AND case_type = ANY(${})", param_count));
            }
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

        if let Some(case_types) = &filter.case_type {
            if !case_types.is_empty() {
                q = q.bind(case_types);
            }
        }

        if let Some(project_id) = filter.project_id {
            q = q.bind(project_id);
        }

        let count = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(count)
    }

    async fn create(&self, input: CreateCaseInput) -> Result<Case, RepositoryError> {
        // Generate case identity first
        let (case_number, identifier) = self.next_case_identity(input.company_id).await?;

        let case = sqlx::query_as::<_, Case>(
            r#"
            INSERT INTO cases (
                company_id, project_id, case_number, identifier, case_type, key,
                title, summary, status, fields, parent_case_id,
                created_by_agent_id, created_by_user_id, created_by_run_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(input.company_id)
        .bind(input.project_id)
        .bind(case_number)
        .bind(&identifier)
        .bind(&input.case_type)
        .bind(input.key.as_ref())
        .bind(&input.title)
        .bind(input.summary.as_ref())
        .bind(input.status.unwrap_or(CaseStatus::Draft))
        .bind(&input.fields.unwrap_or(serde_json::json!({})))
        .bind(input.parent_case_id)
        .bind(input.created_by_agent_id)
        .bind(input.created_by_user_id)
        .bind(input.created_by_run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(case)
    }

    async fn update(&self, id: Uuid, input: UpdateCaseInput) -> Result<Case, RepositoryError> {
        // Build dynamic UPDATE query
        let mut updates = Vec::new();
        let mut param_count = 1;

        if input.project_id.is_some() {
            param_count += 1;
            updates.push(format!("project_id = ${}", param_count));
        }
        if input.title.is_some() {
            param_count += 1;
            updates.push(format!("title = ${}", param_count));
        }
        if input.summary.is_some() {
            param_count += 1;
            updates.push(format!("summary = ${}", param_count));
        }
        if input.status.is_some() {
            param_count += 1;
            updates.push(format!("status = ${}", param_count));
        }
        if input.fields.is_some() {
            param_count += 1;
            updates.push(format!("fields = ${}", param_count));
        }
        if input.parent_case_id.is_some() {
            param_count += 1;
            updates.push(format!("parent_case_id = ${}", param_count));
        }

        if updates.is_empty() {
            return self.get_by_id(id).await?.ok_or_else(|| RepositoryError::NotFound(id));
        }

        updates.push("updated_at = NOW()".to_string());

        // Update completed_at if status is terminal
        if let Some(status) = input.status {
            if status.is_terminal() {
                updates.push("completed_at = COALESCE(completed_at, NOW())".to_string());
            }
        }

        let query = format!(
            "UPDATE cases SET {} WHERE id = $1 RETURNING *",
            updates.join(", ")
        );

        let mut q = sqlx::query_as::<_, Case>(&query).bind(id);

        if let Some(project_id) = input.project_id {
            q = q.bind(project_id);
        }
        if let Some(ref title) = input.title {
            q = q.bind(title);
        }
        if let Some(ref summary) = input.summary {
            q = q.bind(summary);
        }
        if let Some(status) = input.status {
            q = q.bind(status);
        }
        if let Some(ref fields) = input.fields {
            q = q.bind(fields);
        }
        if let Some(parent_case_id) = input.parent_case_id {
            q = q.bind(parent_case_id);
        }

        let case = q.fetch_one(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(case)
    }

    async fn upsert(&self, input: UpsertCaseInput) -> Result<(Case, bool), RepositoryError> {
        // Use PostgreSQL advisory lock to prevent race conditions
        let lock_key = format!("case-upsert:{}:{}:{}", input.company_id, input.case_type, input.key.as_deref().unwrap_or("<null>"));
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&lock_key)
            .execute(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        // Check if case exists
        if let Some(key) = &input.key {
            if let Some(existing) = self.find_by_key(input.company_id, &input.case_type, key).await? {
                // Update existing case
                let update_input = UpdateCaseInput {
                    project_id: input.project_id,
                    title: Some(input.title),
                    summary: input.summary,
                    status: input.status,
                    fields: input.fields,
                    parent_case_id: input.parent_case_id,
                };
                let updated = self.update(existing.id, update_input).await?;
                return Ok((updated, false)); // Not created
            }
        }

        // Create new case
        let create_input = CreateCaseInput {
            company_id: input.company_id,
            project_id: input.project_id,
            case_type: input.case_type,
            key: input.key,
            title: input.title,
            summary: input.summary,
            status: input.status,
            fields: input.fields,
            parent_case_id: input.parent_case_id,
            created_by_agent_id: input.actor_agent_id,
            created_by_user_id: input.actor_user_id,
            created_by_run_id: input.actor_run_id,
        };
        let created = self.create(create_input).await?;
        Ok((created, true)) // Created
    }

    async fn find_by_key(
        &self,
        company_id: Uuid,
        case_type: &str,
        key: &str,
    ) -> Result<Option<Case>, RepositoryError> {
        let case = sqlx::query_as::<_, Case>(
            r#"
            SELECT * FROM cases
            WHERE company_id = $1 AND case_type = $2 AND key = $3
            "#,
        )
        .bind(company_id)
        .bind(case_type)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(case)
    }

    async fn list_by_parent(
        &self,
        parent_case_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Case>, RepositoryError> {
        let cases = sqlx::query_as::<_, Case>(
            r#"
            SELECT * FROM cases
            WHERE parent_case_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(parent_case_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(cases)
    }

    async fn get_by_ids(&self, ids: Vec<Uuid>) -> Result<Vec<Case>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let cases = sqlx::query_as::<_, Case>(
            r#"
            SELECT * FROM cases WHERE id = ANY($1)
            "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(cases)
    }

    async fn next_case_identity(&self, company_id: Uuid) -> Result<(i32, String), RepositoryError> {
        // Use advisory lock to ensure atomic counter increment
        let lock_key = format!("case-identity:{}", company_id);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&lock_key)
            .execute(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        // Get company issue prefix (we'll reuse the companies table)
        let prefix: Option<String> = sqlx::query_scalar(
            r#"
            SELECT issue_prefix FROM companies WHERE id = $1
            "#,
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let prefix = prefix.ok_or_else(|| RepositoryError::NotFound(company_id))?;

        // Get max case number
        let max_number: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(case_number), 0) FROM cases WHERE company_id = $1
            "#,
        )
        .bind(company_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        let case_number = max_number.unwrap_or(0) + 1;
        let identifier = format!("{}-C{}", prefix.to_uppercase(), case_number);

        Ok((case_number, identifier))
    }
}

// CaseEventRepository implementation
pub struct PgCaseEventRepository {
    pool: PgPool,
}

impl PgCaseEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CaseEventRepository for PgCaseEventRepository {
    async fn create_event(&self, event: CaseEvent) -> Result<CaseEvent, RepositoryError> {
        let created = sqlx::query_as::<_, CaseEvent>(
            r#"
            INSERT INTO case_events (
                company_id, case_id, kind, actor_type, actor_id, actor_run_id, payload
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(event.company_id)
        .bind(event.case_id)
        .bind(event.kind)
        .bind(event.actor_type.as_ref())
        .bind(event.actor_id)
        .bind(event.actor_run_id)
        .bind(&event.payload)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(created)
    }

    async fn list_by_case(
        &self,
        case_id: Uuid,
        limit: i64,
    ) -> Result<Vec<CaseEvent>, RepositoryError> {
        let events = sqlx::query_as::<_, CaseEvent>(
            r#"
            SELECT * FROM case_events
            WHERE case_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(case_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(events)
    }

    async fn list_by_case_and_kind(
        &self,
        case_id: Uuid,
        kind: CaseEventKind,
        limit: i64,
    ) -> Result<Vec<CaseEvent>, RepositoryError> {
        let events = sqlx::query_as::<_, CaseEvent>(
            r#"
            SELECT * FROM case_events
            WHERE case_id = $1 AND kind = $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(case_id)
        .bind(kind)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(events)
    }
}
