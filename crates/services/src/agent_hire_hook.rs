use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::ServiceError;

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
    
    // 2. 查找 adapter（TODO: 需要实现 AdapterRegistry::find_adapter 方法）
    // 目前 AdapterRegistry 还没有实现查找逻辑，暂时只记录日志
    tracing::info!(
        company_id = %input.company_id,
        agent_id = %input.agent_id,
        agent_name = %agent.name,
        adapter_type = %adapter_type,
        source = %input.source,
        source_id = %input.source_id,
        "hire hook: would call adapter.on_hire_approved (not yet implemented)"
    );

    // TODO: 完整实现
    // let adapter = adapter_registry.find_adapter(&adapter_type)?;
    // if let Some(hire_hook) = adapter.on_hire_approved {
    //     let payload = HireApprovedPayload {
    //         company_id: input.company_id.to_string(),
    //         agent_id: input.agent_id.to_string(),
    //         agent_name: agent.name.clone(),
    //         adapter_type: adapter_type.clone(),
    //         source: input.source.clone(),
    //         source_id: input.source_id.to_string(),
    //         approved_at: approved_at.to_rfc3339(),
    //         message: HIRE_APPROVED_MESSAGE.to_string(),
    //     };
    //
    //     let adapter_config = agent.adapter_config.unwrap_or_default();
    //     
    //     match hire_hook.on_hire_approved(&payload, &adapter_config).await {
    //         Ok(result) if result.ok => {
    //             // 记录成功
    //             let _ = activity_repo.create(...).await;
    //         }
    //         Ok(result) => {
    //             // 记录 adapter 返回的失败
    //             tracing::warn!(...);
    //             let _ = activity_repo.create(...).await;
    //         }
    //         Err(e) => {
    //             // 记录异常
    //             tracing::error!(...);
    //             let _ = activity_repo.create(...).await;
    //         }
    //     }
    // }

    Ok(())
}
