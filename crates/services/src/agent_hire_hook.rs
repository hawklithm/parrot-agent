use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::server_adapter::AdapterType;
use crate::ServiceError;
use repositories::activity_log_repository::{Activity, ActivityAction, ActorType, ResourceType};

const HIRE_APPROVED_MESSAGE: &str = 
    "Tell your user that your hire was approved, now they should assign you a task in Paperclip or ask you to create issues.";

/// Notify Hire Approved Input - 与 Paperclip 完全对齐
#[derive(Debug, Clone)]
pub struct NotifyHireApprovedInput {
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub source: String,
    pub source_id: Uuid,
    pub approved_at: Option<DateTime<Utc>>,
}

/// Hire Approved Payload - 传递给 Adapter Hook 的数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HireApprovedPayload {
    pub company_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub adapter_type: String,
    pub source: String,
    pub source_id: String,
    pub approved_at: String,
    pub message: String,
}

/// Hire Hook Result - Adapter Hook 的返回结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HireHookResult {
    pub ok: bool,
    pub error: Option<String>,
    pub detail: Option<serde_json::Value>,
}

/// Adapter Hire Hook Trait - Adapter 需要实现此 trait 以支持 hire hook
#[async_trait]
pub trait AdapterHireHook: Send + Sync {
    async fn on_hire_approved(
        &self,
        payload: &HireApprovedPayload,
        adapter_config: &serde_json::Value,
    ) -> Result<HireHookResult, Box<dyn std::error::Error + Send + Sync>>;
}

/// 通知 Agent 被批准（调用 Adapter Hook）
/// 
/// 与 Paperclip 完全对齐的实现：
/// 1. 从数据库查询 agent 信息（adapter_type, adapter_config）
/// 2. 通过 adapter registry 找到对应的 adapter
/// 3. 调用 adapter 的 on_hire_approved hook
/// 4. 记录成功/失败到 activity log
/// 
/// 失败不会阻塞审批流程 - 只记录日志和 activity
pub async fn notify_hire_approved(
    db: Arc<dyn repositories::AgentRepository>,
    activity_repo: Arc<dyn repositories::ActivityLogRepository>,
    adapter_registry: Arc<crate::server_adapter::AdapterRegistry>,
    input: NotifyHireApprovedInput,
) -> Result<(), ServiceError> {
    let approved_at = input.approved_at.unwrap_or_else(Utc::now);

    // 1. 查询 agent 信息
    let agent = match db.get_by_id(input.agent_id).await {
        Ok(agent) => agent,
        Err(e) => {
            tracing::warn!(
                error = ?e,
                company_id = %input.company_id,
                agent_id = %input.agent_id,
                source = %input.source,
                source_id = %input.source_id,
                "hire hook: failed to query agent, skipping"
            );
            return Ok(()); // 非致命错误，不阻塞审批流程
        }
    };

    // 验证 company_id 匹配
    if agent.company_id != input.company_id {
        tracing::warn!(
            agent_company_id = %agent.company_id,
            input_company_id = %input.company_id,
            "hire hook: company_id mismatch, skipping"
        );
        return Ok(());
    }

    let adapter_type = agent.adapter_type.clone();

    // 2. 解析 adapter type 并在 registry 中查找对应 adapter
    let parsed_type = match AdapterType::from_str(&adapter_type) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                adapter_type = %adapter_type,
                error = %e,
                "hire hook: unrecognized adapter type, skipping adapter hook"
            );
            return Ok(());
        }
    };

    let adapter = match adapter_registry.find_adapter(parsed_type) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(
                adapter_type = %adapter_type,
                error = ?e,
                "hire hook: adapter not registered, skipping adapter hook"
            );
            return Ok(());
        }
    };

    // 3. 构造 payload 并调用 adapter 的 on_hire_approved hook
    let payload = HireApprovedPayload {
        company_id: input.company_id.to_string(),
        agent_id: input.agent_id.to_string(),
        agent_name: agent.name.clone(),
        adapter_type: adapter_type.clone(),
        source: input.source.clone(),
        source_id: input.source_id.to_string(),
        approved_at: approved_at.to_rfc3339(),
        message: HIRE_APPROVED_MESSAGE.to_string(),
    };

    let payload_value = match serde_json::to_value(&payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = ?e, "hire hook: failed to serialize payload, skipping");
            return Ok(());
        }
    };

    match adapter
        .on_hire_approved(payload_value, &agent.adapter_config)
        .await
    {
        Ok(result) => {
            tracing::info!(
                agent_id = %input.agent_id,
                result = ?result,
                "hire hook: adapter on_hire_approved succeeded"
            );
            record_hire_hook_activity(
                &activity_repo,
                input.company_id,
                input.agent_id,
                "success",
                Some(&result),
            )
            .await;
        }
        Err(e) => {
            tracing::warn!(
                agent_id = %input.agent_id,
                error = ?e,
                "hire hook: adapter on_hire_approved returned an error"
            );
            record_hire_hook_activity(
                &activity_repo,
                input.company_id,
                input.agent_id,
                "error",
                Some(&serde_json::json!({ "error": e.to_string() })),
            )
            .await;
        }
    }

    Ok(())
}

/// 写入一条 hire-approved hook 的 activity 记录（非致命：失败仅记录日志）。
async fn record_hire_hook_activity(
    activity_repo: &Arc<dyn repositories::ActivityLogRepository>,
    company_id: Uuid,
    agent_id: Uuid,
    status: &str,
    detail: Option<&serde_json::Value>,
) {
    let activity = Activity {
        id: Uuid::new_v4(),
        company_id,
        actor_type: ActorType::System,
        actor_id: agent_id,
        action: ActivityAction::Update,
        resource_type: ResourceType::Agent,
        resource_id: agent_id,
        metadata: Some(serde_json::json!({
            "event": "hire_approved_hook",
            "status": status,
            "detail": detail,
        })),
        created_at: Utc::now(),
    };

    if let Err(e) = activity_repo.log_activity(&activity).await {
        tracing::warn!(error = ?e, agent_id = %agent_id, "hire hook: failed to write activity log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use repositories::activity_log_repository::ActivityLogFilter;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockActivityRepo {
        logged: std::sync::Arc<Mutex<Vec<Activity>>>,
    }

    #[async_trait]
    impl repositories::ActivityLogRepository for MockActivityRepo {
        async fn log_activity(&self, activity: &Activity) -> repositories::RepositoryResult<()> {
            self.logged.lock().unwrap().push(activity.clone());
            Ok(())
        }
        async fn list_recent(
            &self,
            _: Uuid,
            _: i64,
            _: i64,
        ) -> repositories::RepositoryResult<Vec<Activity>> {
            Ok(vec![])
        }
        async fn list_by_resource(
            &self,
            _: Uuid,
            _: ResourceType,
            _: Uuid,
        ) -> repositories::RepositoryResult<Vec<Activity>> {
            Ok(vec![])
        }
        async fn list_by_actor(
            &self,
            _: Uuid,
            _: Uuid,
            _: i64,
            _: i64,
        ) -> repositories::RepositoryResult<Vec<Activity>> {
            Ok(vec![])
        }
        async fn list_by_time_range(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
            _: DateTime<Utc>,
        ) -> repositories::RepositoryResult<Vec<Activity>> {
            Ok(vec![])
        }
        async fn list_with_filter(
            &self,
            _: ActivityLogFilter,
        ) -> repositories::RepositoryResult<Vec<Activity>> {
            Ok(vec![])
        }
        async fn delete_before(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> repositories::RepositoryResult<u64> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn hire_hook_records_activity_with_event_and_status() {
        let mock = std::sync::Arc::new(MockActivityRepo::default());
        let repo: std::sync::Arc<dyn repositories::ActivityLogRepository> = mock.clone();
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        record_hire_hook_activity(
            &repo,
            company_id,
            agent_id,
            "success",
            Some(&serde_json::json!({ "ok": true })),
        )
        .await;

        let logged = mock.logged.lock().unwrap();
        assert_eq!(logged.len(), 1);
        let a = &logged[0];
        assert_eq!(a.company_id, company_id);
        assert_eq!(a.resource_id, agent_id);
        assert!(matches!(a.action, ActivityAction::Update));
        assert!(matches!(a.resource_type, ResourceType::Agent));
        let meta = a.metadata.as_ref().expect("metadata should be set");
        assert_eq!(meta["event"], "hire_approved_hook");
        assert_eq!(meta["status"], "success");
    }
}
