use models::{Company, CompanyMembership, CompanyStats, CreateCompanyInput, UpdateCompanyInput};
use sqlx::{PgPool, Result};
use uuid::Uuid;

#[derive(Clone)]
pub struct CompanyRepository {
    pub pool: PgPool,
}

impl CompanyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, input: CreateCompanyInput, creator_user_id: Uuid) -> Result<Company> {
        let mut tx = self.pool.begin().await?;

        let company = sqlx::query_as::<_, Company>(
            r#"
            INSERT INTO companies (
                name, description, issue_prefix, budget_monthly_cents,
                attachment_max_bytes, default_responsible_user_id,
                require_board_approval_for_new_agents
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.issue_prefix.as_deref().unwrap_or(&input.name.to_uppercase()))
        .bind(input.budget_monthly_cents)
        .bind(input.attachment_max_bytes.unwrap_or(10485760))
        .bind(input.default_responsible_user_id)
        .bind(input.require_board_approval_for_new_agents.unwrap_or(false))
        .fetch_one(&mut *tx)
        .await?;

        // Create owner membership
        sqlx::query(
            r#"
            INSERT INTO company_memberships (
                company_id, principal_type, principal_id, membership_role
            )
            VALUES ($1, 'user', $2, 'owner')
            "#,
        )
        .bind(company.id)
        .bind(creator_user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(company)
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Company>> {
        sqlx::query_as::<_, Company>("SELECT * FROM companies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<Company>> {
        sqlx::query_as::<_, Company>(
            "SELECT * FROM companies ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<Company>> {
        sqlx::query_as::<_, Company>(
            r#"
            SELECT c.* FROM companies c
            INNER JOIN company_memberships cm ON c.id = cm.company_id
            WHERE cm.principal_type = 'user'
              AND cm.principal_id = $1
              AND cm.status = 'active'
            ORDER BY c.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn update(&self, id: Uuid, input: UpdateCompanyInput) -> Result<Company> {
        let mut query = String::from("UPDATE companies SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            query.push_str(&format!(", name = ${}", bind_count));
            bind_count += 1;
        }
        if input.description.is_some() {
            query.push_str(&format!(", description = ${}", bind_count));
            bind_count += 1;
        }
        if input.status.is_some() {
            query.push_str(&format!(", status = ${}", bind_count));
            bind_count += 1;
        }
        if input.pause_reason.is_some() {
            query.push_str(&format!(", pause_reason = ${}", bind_count));
            bind_count += 1;
        }
        if input.budget_monthly_cents.is_some() {
            query.push_str(&format!(", budget_monthly_cents = ${}", bind_count));
            bind_count += 1;
        }
        if input.attachment_max_bytes.is_some() {
            query.push_str(&format!(", attachment_max_bytes = ${}", bind_count));
            bind_count += 1;
        }
        if input.default_responsible_user_id.is_some() {
            query.push_str(&format!(", default_responsible_user_id = ${}", bind_count));
            bind_count += 1;
        }
        if input.require_board_approval_for_new_agents.is_some() {
            query.push_str(&format!(", require_board_approval_for_new_agents = ${}", bind_count));
            bind_count += 1;
        }
        if input.feedback_data_sharing_enabled.is_some() {
            query.push_str(&format!(", feedback_data_sharing_enabled = ${}", bind_count));
            bind_count += 1;
        }
        if input.feedback_data_sharing_consent_at.is_some() {
            query.push_str(&format!(", feedback_data_sharing_consent_at = ${}", bind_count));
            bind_count += 1;
        }
        if input.feedback_data_sharing_consent_by_user_id.is_some() {
            query.push_str(&format!(", feedback_data_sharing_consent_by_user_id = ${}", bind_count));
            bind_count += 1;
        }
        if input.feedback_data_sharing_terms_version.is_some() {
            query.push_str(&format!(", feedback_data_sharing_terms_version = ${}", bind_count));
            bind_count += 1;
        }

        query.push_str(&format!(" WHERE id = ${} RETURNING *", bind_count));

        let mut q = sqlx::query_as::<_, Company>(&query);

        if let Some(name) = input.name {
            q = q.bind(name);
        }
        if let Some(description) = input.description {
            q = q.bind(description);
        }
        if let Some(status) = input.status {
            q = q.bind(status);
        }
        if let Some(pause_reason) = input.pause_reason {
            q = q.bind(pause_reason);
        }
        if let Some(budget) = input.budget_monthly_cents {
            q = q.bind(budget);
        }
        if let Some(max_bytes) = input.attachment_max_bytes {
            q = q.bind(max_bytes);
        }
        if let Some(user_id) = input.default_responsible_user_id {
            q = q.bind(user_id);
        }
        if let Some(require_approval) = input.require_board_approval_for_new_agents {
            q = q.bind(require_approval);
        }
        if let Some(enabled) = input.feedback_data_sharing_enabled {
            q = q.bind(enabled);
        }
        if let Some(consent_at) = input.feedback_data_sharing_consent_at {
            q = q.bind(consent_at);
        }
        if let Some(consent_by) = input.feedback_data_sharing_consent_by_user_id {
            q = q.bind(consent_by);
        }
        if let Some(terms_version) = input.feedback_data_sharing_terms_version {
            q = q.bind(terms_version);
        }

        q.bind(id).fetch_one(&self.pool).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM companies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_stats(&self, company_id: Uuid) -> Result<CompanyStats> {
        sqlx::query_as::<_, CompanyStats>(
            r#"
            SELECT
                $1 as company_id,
                (SELECT COUNT(*) FROM projects WHERE company_id = $1) as total_projects,
                (SELECT COUNT(*) FROM agents WHERE company_id = $1) as total_agents,
                (SELECT COUNT(*) FROM issues WHERE company_id = $1) as total_issues,
                (SELECT spent_monthly_cents FROM companies WHERE id = $1) as spent_monthly_cents
            "#,
        )
        .bind(company_id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn increment_issue_counter(&self, company_id: Uuid) -> Result<i32> {
        let row: (i32,) = sqlx::query_as(
            "UPDATE companies SET issue_counter = issue_counter + 1 WHERE id = $1 RETURNING issue_counter",
        )
        .bind(company_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn get_membership(
        &self,
        company_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<CompanyMembership>> {
        sqlx::query_as::<_, CompanyMembership>(
            "SELECT * FROM company_memberships WHERE company_id = $1 AND principal_type = 'user' AND principal_id = $2",
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }
}
