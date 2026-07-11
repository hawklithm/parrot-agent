use async_trait::async_trait;
use models::AgentConfigRevision;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{ConfigRevisionRepository, RepositoryError, RepositoryResult};

/// PostgreSQL实现的ConfigRevisionRepository
pub struct PgConfigRevisionRepository {
    pool: PgPool,
}

impl PgConfigRevisionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConfigRevisionRepository for PgConfigRevisionRepository {
    async fn create(&self, revision: AgentConfigRevision) -> RepositoryResult<AgentConfigRevision> {
        let row = sqlx::query(
            r#"
            INSERT INTO agent_config_revisions (id, agent_id, snapshot, created_at)
            VALUES ($1, $2, $3, $4)
            RETURNING id, agent_id, snapshot, created_at
            "#,
        )
        .bind(revision.id)
        .bind(revision.agent_id)
        .bind(&revision.snapshot)
        .bind(revision.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(AgentConfigRevision {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            snapshot: row.get("snapshot"),
            created_at: row.get("created_at"),
        })
    }

    async fn list_by_agent(
        &self,
        agent_id: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> RepositoryResult<Vec<AgentConfigRevision>> {
        let limit = limit.unwrap_or(50).min(100);
        let offset = offset.unwrap_or(0);

        let rows = sqlx::query(
            r#"
            SELECT id, agent_id, snapshot, created_at
            FROM agent_config_revisions
            WHERE agent_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(agent_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows
            .into_iter()
            .map(|row| AgentConfigRevision {
                id: row.get("id"),
                agent_id: row.get("agent_id"),
                snapshot: row.get("snapshot"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<AgentConfigRevision> {
        let row = sqlx::query(
            r#"
            SELECT id, agent_id, snapshot, created_at
            FROM agent_config_revisions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?
        .ok_or_else(|| RepositoryError::NotFound(id))?;

        Ok(AgentConfigRevision {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            snapshot: row.get("snapshot"),
            created_at: row.get("created_at"),
        })
    }

    async fn count_by_agent(&self, agent_id: Uuid) -> RepositoryResult<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM agent_config_revisions
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(row.get("count"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::types::Json;

    #[tokio::test]
    #[ignore]
    async fn test_create_config_revision() {
        let pool = PgPool::connect("postgresql://localhost/test").await.unwrap();
        let repo = PgConfigRevisionRepository::new(pool);

        let revision = AgentConfigRevision {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            snapshot: Json(serde_json::json!({
                "adapter_type": "claude_local",
                "adapter_config": {"model": "claude-opus-4"}
            })),
            created_at: Utc::now(),
        };

        let created = repo.create(revision.clone()).await.unwrap();
        assert_eq!(created.id, revision.id);
        assert_eq!(created.agent_id, revision.agent_id);
    }
}
