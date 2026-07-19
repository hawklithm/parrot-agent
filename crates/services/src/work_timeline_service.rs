use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

#[derive(Debug, Clone, Default)]
pub struct WorkTimelineQuery {
    pub company_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
}

#[async_trait]
pub trait WorkTimelineService: Send + Sync {
    async fn collect_issue_ids(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<Uuid>>;
    async fn load_events(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<serde_json::Value>>;
}

pub struct DefaultWorkTimelineService { pub pool: PgPool }

#[async_trait]
impl WorkTimelineService for DefaultWorkTimelineService {
    async fn collect_issue_ids(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<Uuid>> {
        if let Some(issue_id) = query.issue_id { return Ok(vec![issue_id]); }
        let rows = sqlx::query("SELECT DISTINCT resource_id FROM activity_logs WHERE company_id=$1 AND resource_type='issue' AND resource_id IS NOT NULL")
            .bind(query.company_id).fetch_all(&self.pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.get::<Uuid,_>("resource_id")).collect())
    }
    async fn load_events(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<serde_json::Value>> {
        let rows = sqlx::query("SELECT id,event_type,actor_id,resource_type,resource_id,metadata,created_at FROM activity_logs WHERE company_id=$1 AND ($2::uuid IS NULL OR resource_id=$2) AND ($3::uuid IS NULL OR actor_id=$3) ORDER BY created_at DESC LIMIT 500")
            .bind(query.company_id).bind(query.issue_id).bind(query.user_id).fetch_all(&self.pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| serde_json::json!({"id":r.get::<Uuid,_>("id"),"eventType":r.get::<String,_>("event_type"),"actorId":r.get::<Uuid,_>("actor_id"),"resourceType":r.get::<String,_>("resource_type"),"resourceId":r.get::<Option<Uuid>,_>("resource_id"),"metadata":r.get::<serde_json::Value,_>("metadata"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")})).collect())
    }
}
