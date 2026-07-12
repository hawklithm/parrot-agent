use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::schemas::{
    derive_agent_url_key, parse_scheduler_heartbeat_policy, InstanceSchedulerHeartbeatAgent,
};

/// GET /instance/scheduler-heartbeats
/// 获取所有配置了调度心跳的 Agent 列表（需要 Instance Admin 权限）
pub async fn list_scheduler_heartbeats(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<InstanceSchedulerHeartbeatAgent>>, HeartbeatError> {
    // 查询所有活跃的 Agent 及其公司信息
    let rows = sqlx::query!(
        r#"
        SELECT
            a.id,
            a.company_id,
            a.name as agent_name,
            a.role,
            a.status,
            a.adapter_type,
            a.runtime_config,
            c.name as company_name,
            c.issue_prefix as company_issue_prefix
        FROM agents a
        INNER JOIN companies c ON a.company_id = c.id
        WHERE a.status NOT IN ('paused', 'terminated', 'pending_approval')
        ORDER BY c.name, a.name
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| HeartbeatError::DatabaseError(e.to_string()))?;

    let agents: Vec<InstanceSchedulerHeartbeatAgent> = rows
        .into_iter()
        .filter_map(|row| {
            // 解析心跳策略
            let policy = parse_scheduler_heartbeat_policy(&row.runtime_config);

            // 状态检查
            let status = row.status.as_str();
            let status_eligible = status != "paused"
                && status != "terminated"
                && status != "pending_approval";

            // 生成 Agent URL key
            let agent_url_key = derive_agent_url_key(&row.agent_name, row.id);

            // 判断调度器是否活跃
            let scheduler_active =
                status_eligible && policy.enabled && policy.interval_sec > 0;

            Some(InstanceSchedulerHeartbeatAgent {
                id: row.id,
                company_id: row.company_id,
                company_name: row.company_name,
                company_issue_prefix: row.company_issue_prefix,
                agent_name: row.agent_name,
                agent_url_key,
                role: row.role,
                title: None,
                status: row.status,
                adapter_type: row.adapter_type,
                interval_sec: policy.interval_sec,
                heartbeat_enabled: policy.enabled,
                scheduler_active,
                last_heartbeat_at: None,
            })
        })
        .collect();

    Ok(Json(agents))
}

/// 心跳相关错误
#[derive(Debug)]
pub enum HeartbeatError {
    DatabaseError(String),
}

impl IntoResponse for HeartbeatError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            HeartbeatError::DatabaseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_error_response() {
        let error = HeartbeatError::DatabaseError("Connection failed".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
