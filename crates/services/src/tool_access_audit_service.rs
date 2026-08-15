use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ToolAccessAuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("audit entry not found: {0}")]
    NotFound(Uuid),
}

pub type AuditResult<T> = Result<T, ToolAccessAuditError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ToolAccessAuditEntry {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub tool_name: String,
    pub action: AccessAction,
    pub result: AccessResult,
    pub request_data: Option<serde_json::Value>,
    pub response_data: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum AccessAction {
    Invoke,
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum AccessResult {
    Success,
    Denied,
    Error,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct RecordAccessRequest {
    pub agent_id: Uuid,
    pub tool_name: String,
    pub action: AccessAction,
    pub result: AccessResult,
    pub request_data: Option<serde_json::Value>,
    pub response_data: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

#[async_trait]
pub trait ToolAccessAuditService: Send + Sync {
    async fn record_access(&self, req: RecordAccessRequest) -> AuditResult<Uuid>;
    async fn get_audit_entry(&self, entry_id: Uuid) -> AuditResult<Option<ToolAccessAuditEntry>>;
    async fn list_agent_access(&self, agent_id: Uuid, limit: i32) -> AuditResult<Vec<ToolAccessAuditEntry>>;
    async fn list_tool_access(&self, tool_name: &str, limit: i32) -> AuditResult<Vec<ToolAccessAuditEntry>>;
    async fn get_access_stats(&self, agent_id: Uuid) -> AuditResult<AccessStats>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessStats {
    pub total_accesses: i64,
    pub successful_accesses: i64,
    pub denied_accesses: i64,
    pub error_accesses: i64,
    pub average_duration_ms: f64,
}

pub struct ToolAccessAuditServiceImpl {
    pool: PgPool,
}

impl ToolAccessAuditServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ToolAccessAuditService for ToolAccessAuditServiceImpl {
    async fn record_access(&self, req: RecordAccessRequest) -> AuditResult<Uuid> {
        let entry_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO tool_access_audit (
                id, agent_id, tool_name, action, result, 
                request_data, response_data, error_message, duration_ms, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(entry_id)
        .bind(req.agent_id)
        .bind(&req.tool_name)
        .bind(serde_json::to_value(&req.action).unwrap())
        .bind(serde_json::to_value(&req.result).unwrap())
        .bind(&req.request_data)
        .bind(&req.response_data)
        .bind(&req.error_message)
        .bind(req.duration_ms)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(entry_id)
    }
    
    async fn get_audit_entry(&self, entry_id: Uuid) -> AuditResult<Option<ToolAccessAuditEntry>> {
        let row = sqlx::query_as::<_, ToolAccessAuditEntry>(
            r#"
            SELECT id, agent_id, tool_name, action, result, 
                   request_data, response_data, error_message, duration_ms, created_at
            FROM tool_access_audit
            WHERE id = $1
            "#
        )
        .bind(entry_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn list_agent_access(&self, agent_id: Uuid, limit: i32) -> AuditResult<Vec<ToolAccessAuditEntry>> {
        let rows = sqlx::query_as::<_, ToolAccessAuditEntry>(
            r#"
            SELECT id, agent_id, tool_name, action, result, 
                   request_data, response_data, error_message, duration_ms, created_at
            FROM tool_access_audit
            WHERE agent_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn list_tool_access(&self, tool_name: &str, limit: i32) -> AuditResult<Vec<ToolAccessAuditEntry>> {
        let rows = sqlx::query_as::<_, ToolAccessAuditEntry>(
            r#"
            SELECT id, agent_id, tool_name, action, result, 
                   request_data, response_data, error_message, duration_ms, created_at
            FROM tool_access_audit
            WHERE tool_name = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#
        )
        .bind(tool_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn get_access_stats(&self, agent_id: Uuid) -> AuditResult<AccessStats> {
        let row: (i64, i64, i64, i64, Option<f64>) = sqlx::query_as(
            r#"
            SELECT 
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE result = 'success') as successful,
                COUNT(*) FILTER (WHERE result = 'denied') as denied,
                COUNT(*) FILTER (WHERE result = 'error') as errors,
                AVG(duration_ms) as avg_duration
            FROM tool_access_audit
            WHERE agent_id = $1
            "#
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(AccessStats {
            total_accesses: row.0,
            successful_accesses: row.1,
            denied_accesses: row.2,
            error_accesses: row.3,
            average_duration_ms: row.4.unwrap_or(0.0),
        })
    }
}
