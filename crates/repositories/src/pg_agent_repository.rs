use async_trait::async_trait;
use chrono::Utc;
use models::{Agent, AgentStatus};
use sqlx::PgPool;
use uuid::Uuid;

use super::agent_repository::{AgentRepository, ListAgentsOptions, RepositoryError, RepositoryResult};

/// PostgreSQL implementation of AgentRepository
#[derive(Clone)]
pub struct PgAgentRepository {
    pool: PgPool,
}

impl PgAgentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Helper: map a PgRow to Agent via explicit column access.
/// Uses sqlx::Row::get instead of query_as because sqlx::FromRow cannot
/// directly handle the Json<serde_json::Value> wrapper types' column
/// mapping when column names match struct fields exactly.
fn map_agent_row(row: sqlx::postgres::PgRow) -> Agent {
    use sqlx::Row;
    Agent {
        id: row.get("id"),
        company_id: row.get("company_id"),
        name: row.get("name"),
        role: row.get("role"),
        status: row.get("status"),
        adapter_type: row.get("adapter_type"),
        adapter_config: row.get("adapter_config"),
        runtime_config: row.get("runtime_config"),
        permissions: row.get("permissions"),
        metadata: row.get("metadata"),
        budget_monthly_cents: row.get("budget_monthly_cents"),
        reports_to: row.get("reports_to"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl AgentRepository for PgAgentRepository {
    async fn create(&self, agent: Agent) -> RepositoryResult<Agent> {
        let row = sqlx::query(
            r#"
            INSERT INTO agents (
                id, company_id, name, role, status, adapter_type,
                adapter_config, runtime_config, permissions, metadata,
                budget_monthly_cents, reports_to, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(&agent.id)
        .bind(&agent.company_id)
        .bind(&agent.name)
        .bind(&agent.role)
        .bind(&agent.status)
        .bind(&agent.adapter_type)
        .bind(&agent.adapter_config)
        .bind(&agent.runtime_config)
        .bind(&agent.permissions)
        .bind(&agent.metadata)
        .bind(&agent.budget_monthly_cents)
        .bind(&agent.reports_to)
        .bind(&agent.created_at)
        .bind(&agent.updated_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(map_agent_row(row))
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Agent> {
        let row = sqlx::query("SELECT * FROM agents WHERE id = $1")
            .bind(&id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(map_agent_row(row)),
            None => Err(RepositoryError::NotFound(id)),
        }
    }

    async fn list_by_company(
        &self,
        company_id: Uuid,
        options: ListAgentsOptions,
    ) -> RepositoryResult<Vec<Agent>> {
        let mut query = String::from("SELECT * FROM agents WHERE company_id = $1");
        let mut param_count = 1;

        // Default: exclude terminated agents (mirrors Paperclip behavior)
        if !options.include_terminated {
            query.push_str(" AND status != 'terminated'");
        }

        query.push_str(" ORDER BY created_at DESC");

        // Add pagination using parameterized bindings (not string interpolation)
        if options.limit.is_some() {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
        }
        if options.offset.is_some() {
            param_count += 1;
            query.push_str(&format!(" OFFSET ${}", param_count));
        }

        let mut q = sqlx::query(&query).bind(&company_id);

        if let Some(limit) = options.limit {
            q = q.bind(limit);
        }
        if let Some(offset) = options.offset {
            q = q.bind(offset);
        }

        let rows = q.fetch_all(&self.pool).await?;

        let agents = rows.into_iter().map(map_agent_row).collect();
        Ok(agents)
    }

    async fn update(&self, agent: Agent) -> RepositoryResult<Agent> {
        let now = Utc::now();

        let row = sqlx::query(
            r#"
            UPDATE agents
            SET name = $2, role = $3, status = $4, adapter_type = $5,
                adapter_config = $6, runtime_config = $7, permissions = $8,
                metadata = $9, budget_monthly_cents = $10, reports_to = $11,
                updated_at = $12
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(&agent.id)
        .bind(&agent.name)
        .bind(&agent.role)
        .bind(&agent.status)
        .bind(&agent.adapter_type)
        .bind(&agent.adapter_config)
        .bind(&agent.runtime_config)
        .bind(&agent.permissions)
        .bind(&agent.metadata)
        .bind(&agent.budget_monthly_cents)
        .bind(&agent.reports_to)
        .bind(&now)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(map_agent_row(row)),
            None => Err(RepositoryError::NotFound(agent.id)),
        }
    }

    async fn delete(&self, id: Uuid) -> RepositoryResult<()> {
        let result = sqlx::query("UPDATE agents SET status = 'terminated', updated_at = $2 WHERE id = $1")
            .bind(&id)
            .bind(&Utc::now())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            Err(RepositoryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    async fn list_by_status(&self, company_id: Uuid, status: AgentStatus) -> RepositoryResult<Vec<Agent>> {
        let rows = sqlx::query(
            "SELECT * FROM agents WHERE company_id = $1 AND status = $2 ORDER BY created_at DESC"
        )
        .bind(&company_id)
        .bind(&status)
        .fetch_all(&self.pool)
        .await?;

        let agents = rows.into_iter().map(map_agent_row).collect();
        Ok(agents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::AgentRole;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::types::Json;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/parrot_agent_test".to_string());

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_create_agent() {
        let pool = setup_test_db().await;
        let repo = PgAgentRepository::new(pool);

        let company_id = Uuid::new_v4();
        let agent = Agent {
            id: Uuid::new_v4(),
            company_id,
            name: "Test Agent".to_string(),
            role: AgentRole::General,
            status: AgentStatus::Idle,
            adapter_type: "process".to_string(),
            adapter_config: Json(serde_json::json!({})),
            runtime_config: Json(serde_json::json!({})),
            permissions: Json(AgentPermissions::default()),
            metadata: Json(AgentMetadata { is_built_in: None, built_in_key: None, instructions_path: None, instructions_bundle: None }),
            budget_monthly_cents: 0,
            reports_to: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = repo.create(agent.clone()).await;
        assert!(result.is_ok());

        let created = result.unwrap();
        assert_eq!(created.id, agent.id);
        assert_eq!(created.name, agent.name);
    }
}
