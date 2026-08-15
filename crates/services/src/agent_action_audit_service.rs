/// Agent Action Audit Service
/// 
/// Agent行为审计服务

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("audit record not found: {0}")]
    NotFound(Uuid),
}

pub type AuditResult<T> = Result<T, AuditError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub action_type: String,
    pub action_details: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub user_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub result: Option<String>,
}

impl AuditRecord {
    pub fn new(
        agent_id: Uuid,
        action_type: String,
        action_details: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            action_type,
            action_details,
            timestamp: chrono::Utc::now(),
            user_id: None,
            workspace_id: None,
            result: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditQuery {
    pub agent_id: Option<Uuid>,
    pub action_type: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

pub struct AgentActionAuditService {
    pool: PgPool,
}

impl AgentActionAuditService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 记录agent操作
    pub async fn log_action(&self, record: AuditRecord) -> AuditResult<Uuid> {
        let id = sqlx::query_scalar(
            r#"
            INSERT INTO agent_action_audits 
            (id, agent_id, action_type, action_details, timestamp, user_id, workspace_id, result)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#
        )
        .bind(&record.id)
        .bind(&record.agent_id)
        .bind(&record.action_type)
        .bind(&record.action_details)
        .bind(&record.timestamp)
        .bind(&record.user_id)
        .bind(&record.workspace_id)
        .bind(&record.result)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 查询审计日志
    pub async fn query_audits(&self, query: AuditQuery) -> AuditResult<Vec<AuditRecord>> {
        let mut sql = String::from(
            "SELECT id, agent_id, action_type, action_details, timestamp, user_id, workspace_id, result FROM agent_action_audits WHERE 1=1"
        );
        
        if query.agent_id.is_some() {
            sql.push_str(" AND agent_id = $1");
        }
        if query.action_type.is_some() {
            sql.push_str(" AND action_type = $2");
        }
        if query.start_time.is_some() {
            sql.push_str(" AND timestamp >= $3");
        }
        if query.end_time.is_some() {
            sql.push_str(" AND timestamp <= $4");
        }
        
        sql.push_str(" ORDER BY timestamp DESC");
        
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        
        // 简化实现 - 实际应该使用参数化查询
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await?;
        
        let records = rows.into_iter().map(|row| AuditRecord {
            id: row.get("id"),
            agent_id: row.get("agent_id"),
            action_type: row.get("action_type"),
            action_details: row.get("action_details"),
            timestamp: row.get("timestamp"),
            user_id: row.get("user_id"),
            workspace_id: row.get("workspace_id"),
            result: row.get("result"),
        }).collect();
        
        Ok(records)
    }
    
    /// 生成审计报告
    pub async fn generate_report(
        &self,
        agent_id: Uuid,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> AuditResult<AuditReport> {
        let query = AuditQuery {
            agent_id: Some(agent_id),
            action_type: None,
            start_time: Some(start),
            end_time: Some(end),
            limit: None,
        };
        
        let records = self.query_audits(query).await?;
        
        // 统计
        let total_actions = records.len();
        let mut action_counts = std::collections::HashMap::new();
        
        for record in &records {
            *action_counts.entry(record.action_type.clone()).or_insert(0) += 1;
        }
        
        Ok(AuditReport {
            agent_id,
            period_start: start,
            period_end: end,
            total_actions,
            action_breakdown: action_counts,
            sample_records: records.into_iter().take(100).collect(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditReport {
    pub agent_id: Uuid,
    pub period_start: chrono::DateTime<chrono::Utc>,
    pub period_end: chrono::DateTime<chrono::Utc>,
    pub total_actions: usize,
    pub action_breakdown: std::collections::HashMap<String, usize>,
    pub sample_records: Vec<AuditRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audit_record_creation() {
        let agent_id = Uuid::new_v4();
        let record = AuditRecord::new(
            agent_id,
            "tool_execute".to_string(),
            serde_json::json!({"tool": "github:search"}),
        );
        
        assert_eq!(record.agent_id, agent_id);
        assert_eq!(record.action_type, "tool_execute");
    }
}
