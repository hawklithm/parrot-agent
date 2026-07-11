use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent_repository::{RepositoryError, RepositoryResult};
use serde::{Deserialize, Serialize};

/// Activity action type
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ActivityAction {
    Create,
    Update,
    Delete,
    View,
    Execute,
}

/// Actor type
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
    System,
}

/// Resource type
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Issue,
    Case,
    Agent,
    Project,
    Environment,
}

/// Activity log entry
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub company_id: Uuid,
    pub actor_type: ActorType,
    pub actor_id: Uuid,
    pub action: ActivityAction,
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// ActivityLog查询过滤器
#[derive(Debug, Clone)]
pub struct ActivityLogFilter {
    pub company_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub resource_type: Option<ResourceType>,
    pub resource_id: Option<Uuid>,
    pub action: Option<ActivityAction>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ActivityLogFilter {
    fn default() -> Self {
        Self {
            company_id: Uuid::nil(),
            actor_id: None,
            resource_type: None,
            resource_id: None,
            action: None,
            start_time: None,
            end_time: None,
            limit: 50,
            offset: 0,
        }
    }
}

#[async_trait]
pub trait ActivityLogRepository: Send + Sync {
    /// 记录活动
    async fn log_activity(&self, activity: &Activity) -> RepositoryResult<()>;

    /// 列出最近的活动（按时间倒序）
    async fn list_recent(
        &self,
        company_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> RepositoryResult<Vec<Activity>>;

    /// 按资源查询活动
    async fn list_by_resource(
        &self,
        company_id: Uuid,
        resource_type: ResourceType,
        resource_id: Uuid,
    ) -> RepositoryResult<Vec<Activity>>;

    /// 按actor查询活动
    async fn list_by_actor(
        &self,
        company_id: Uuid,
        actor_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> RepositoryResult<Vec<Activity>>;

    /// 按时间范围查询活动
    async fn list_by_time_range(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<Activity>>;

    /// 高级过滤查询
    async fn list_with_filter(&self, filter: ActivityLogFilter) -> RepositoryResult<Vec<Activity>>;

    /// 删除旧活动记录（用于归档）
    async fn delete_before(&self, company_id: Uuid, before: DateTime<Utc>) -> RepositoryResult<u64>;
}

/// PeSQL implementation
pub struct PgActivityLogRepository {
    pool: PgPool,
}

impl PgActivityLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 过滤敏感信息（密码、token、API密钥）
    fn sanitize_metadata(metadata: &serde_json::Value) -> serde_json::Value {
        let mut sanitized = metadata.clone();

        if let Some(obj) = sanitized.as_object_mut() {
            for (key, value) in obj.iter_mut() {
                let key_lower = key.to_lowercase();
                if key_lower.contains("password")
                    || key_lower.contains("token")
                    || key_lower.contains("api_key")
                    || key_lower.contains("secret")
                {
                    *value = serde_json::json!("[REDACTED]");
                }
            }
        }

        sanitized
    }
}

#[async_trait]
impl ActivityLogRepository for PgActivityLogRepository {
    async fn log_activity(&self, activity: &Activity) -> RepositoryResult<()> {
        let sanitized_metadata = Self::sanitize_metadata(
            &serde_json::to_value(&activity.metadata)
                .map_err(|e| RepositoryError::InvalidData(e.to_string()))?,
        );

        sqlx::query(
            r#"
            INSERT INTO activity_logs
            (id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(&activity.id)
        .bind(&activity.company_id)
        .bind(&activity.actor_type)
        .bind(&activity.actor_id)
        .bind(&activity.action)
        .bind(&activity.resource_type)
        .bind(&activity.resource_id)
        .bind(&sanitized_metadata)
        .bind(&activity.created_at)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(())
    }

    async fn list_recent(
        &self,
        company_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> RepositoryResult<Vec<Activity>> {
        let rows = sqlx::query_as::<_, Activity>(
            r#"
            SELECT id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at
            FROM activity_logs
            WHERE company_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(&company_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn list_by_resource(
        &self,
        company_id: Uuid,
        resource_type: ResourceType,
        resource_id: Uuid,
    ) -> RepositoryResult<Vec<Activity>> {
        let rows = sqlx::query_as::<_, Activity>(
            r#"
            SELECT id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at
            FROM activity_logs
            WHERE company_id = $1 AND resource_type = $2 AND resource_id = $3
            ORDER BY created_at DESC
            "#,
        )
        .bind(&company_id)
        .bind(&resource_type)
        .bind(&resource_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn list_by_actor(
        &self,
        company_id: Uuid,
        actor_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> RepositoryResult<Vec<Activity>> {
        let rows = sqlx::query_as::<_, Activity>(
            r#"
            SELECT id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at
            FROM activity_logs
            WHERE company_id = $1 AND actor_id = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(&company_id)
        .bind(&actor_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn list_by_time_range(
        &self,
        company_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> RepositoryResult<Vec<Activity>> {
        let rows = sqlx::query_as::<_, Activity>(
            r#"
            SELECT id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at
            FROM activity_logs
            WHERE company_id = $1 AND created_at >= $2 AND created_at <= $3
            ORDER BY created_at DESC
            "#,
        )
        .bind(&company_id)
        .bind(&start_time)
        .bind(&end_time)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn list_with_filter(&self, filter: ActivityLogFilter) -> RepositoryResult<Vec<Activity>> {
        let mut query = String::from(
            "SELECT id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at FROM activity_logs WHERE company_id = $1"
        );
        let mut bind_index = 2;

        if filter.actor_id.is_some() {
            query.push_str(&format!(" AND actor_id = ${}", bind_index));
            bind_index += 1;
        }
        if filter.resource_type.is_some() {
            query.push_str(&format!(" AND resource_type = ${}", bind_index));
            bind_index += 1;
        }
        if filter.resource_id.is_some() {
            query.push_str(&format!(" AND resource_id = ${}", bind_index));
            bind_index += 1;
        }
        if filter.action.is_some() {
            query.push_str(&format!(" AND action = ${}", bind_index));
            bind_index += 1;
        }
        if filter.start_time.is_some() {
            query.push_str(&format!(" AND created_at >= ${}", bind_index));
            bind_index += 1;
        }
        if filter.end_time.is_some() {
            query.push_str(&format!(" AND created_at <= ${}", bind_index));
            bind_index += 1;
        }

        query.push_str(&format!(" ORDER BY created_at DESC LIMIT ${} OFFSET ${}", bind_index, bind_index + 1));

        let mut q = sqlx::query_as::<_, Activity>(&query).bind(&filter.company_id);

        if let Some(actor_id) = filter.actor_id {
            q = q.bind(actor_id);
        }
        if let Some(resource_type) = filter.resource_type {
            q = q.bind(resource_type);
        }
        if let Some(resource_id) = filter.resource_id {
            q = q.bind(resource_id);
        }
        if let Some(action) = filter.action {
            q = q.bind(action);
        }
        if let Some(start_time) = filter.start_time {
            q = q.bind(start_time);
        }
        if let Some(end_time) = filter.end_time {
            q = q.bind(end_time);
        }

        q = q.bind(filter.limit).bind(filter.offset);

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(RepositoryError::DatabaseError)?;

        Ok(rows)
    }

    async fn delete_before(&self, company_id: Uuid, before: DateTime<Utc>) -> RepositoryResult<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM activity_logs
            WHERE company_id = $1 AND created_at < $2
            "#,
        )
        .bind(&company_id)
        .bind(&before)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::DatabaseError)?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sanitize_metadata() {
        let metadata = json!({
            "user": "admin",
            "password": "secret123",
            "api_key": "sk-1234",
            "reason": "test"
        });

        let sanitized = PgActivityLogRepository::sanitize_metadata(&metadata);

        assert_eq!(sanitized["user"], "admin");
        assert_eq!(sanitized["password"], "[REDACTED]");
        assert_eq!(sanitized["api_key"], "[REDACTED]");
        assert_eq!(sanitized["reason"], "test");
    }
}
