//! 认证授权审计日志（对应任务拆解 §10 阶段一「实现审计日志」）。
//!
//! 提供 `log_auth_event` 函数，在关键认证事件中记录审计日志：
//! - JWT run_id 不匹配
//! - Agent Key 缺少 responsible user
//! - API Key 过期拒绝
//! - 认证失败/拒绝等

use sqlx::PgPool;
use uuid::Uuid;

use repositories::activity_log_repository::{
    Activity, ActivityAction, ActivityLogRepository, ActorType, PgActivityLogRepository,
    ResourceType,
};

/// 认证事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthEventType {
    /// JWT 认证被拒绝（run_id 不匹配、agent 不活跃等）
    JwtRejected,
    /// API Key 认证被拒绝（过期、已撤销等）
    ApiKeyRejected,
    /// Agent Key 缺少 responsible user
    MissingResponsibleUser,
    /// 认证成功
    Authenticated,
    /// 授权决策被拒绝
    AuthorizationDenied,
    /// API Key 轮换
    ApiKeyRotated,
}

impl AuthEventType {
    /// 获取事件类型字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthEventType::JwtRejected => "jwt_rejected",
            AuthEventType::ApiKeyRejected => "api_key_rejected",
            AuthEventType::MissingResponsibleUser => "missing_responsible_user",
            AuthEventType::Authenticated => "authenticated",
            AuthEventType::AuthorizationDenied => "authorization_denied",
            AuthEventType::ApiKeyRotated => "api_key_rotated",
        }
    }
}

/// 记录认证审计事件（最佳努力，不阻塞主流程）。
///
/// # 参数
/// - `pool`: 数据库连接池
/// - `event_type`: 认证事件类型
/// - `company_id`: 公司 ID
/// - `actor_id`: 操作者 ID（用户或 Agent）
/// - `actor_type`: 操作者类型（user / agent / system）
/// - `details`: 事件详情（JSON 对象，会被自动过滤敏感信息）
pub async fn log_auth_event(
    pool: &PgPool,
    event_type: AuthEventType,
    company_id: Uuid,
    actor_id: Uuid,
    actor_type: &str,
    details: serde_json::Value,
) {
    let repo = PgActivityLogRepository::new(pool.clone());

    let actor_type_enum = match actor_type {
        "agent" => ActorType::Agent,
        "user" => ActorType::User,
        _ => ActorType::System,
    };

    let activity = Activity {
        id: Uuid::new_v4(),
        company_id,
        actor_type: actor_type_enum,
        actor_id,
        action: ActivityAction::View,
        resource_type: ResourceType::Environment,
        resource_id: actor_id,
        metadata: Some(serde_json::json!({
            "event": event_type.as_str(),
            "details": details,
        })),
        created_at: chrono::Utc::now(),
    };

    let _ = repo.log_activity(&activity).await;
}

/// 记录 JWT 认证拒绝事件。
pub async fn audit_jwt_rejected(
    pool: &PgPool,
    agent_id: Uuid,
    company_id: Uuid,
    reason: &str,
    run_id: Option<Uuid>,
) {
    log_auth_event(
        pool,
        AuthEventType::JwtRejected,
        company_id,
        agent_id,
        "agent",
        serde_json::json!({
            "reason": reason,
            "run_id": run_id,
        }),
    )
    .await;
}

/// 记录 API Key 拒绝事件（过期/已撤销）。
pub async fn audit_api_key_rejected(
    pool: &PgPool,
    key_id: Uuid,
    company_id: Uuid,
    principal_id: Uuid,
    principal_type: &str,
    reason: &str,
) {
    log_auth_event(
        pool,
        AuthEventType::ApiKeyRejected,
        company_id,
        principal_id,
        principal_type,
        serde_json::json!({
            "key_id": key_id.to_string(),
            "reason": reason,
        }),
    )
    .await;
}

/// 记录缺少 responsible user 事件。
pub async fn audit_missing_responsible_user(
    pool: &PgPool,
    agent_id: Uuid,
    company_id: Uuid,
) {
    log_auth_event(
        pool,
        AuthEventType::MissingResponsibleUser,
        company_id,
        agent_id,
        "agent",
        serde_json::json!({
            "message": "Agent key authenticated but no responsible user found",
        }),
    )
    .await;
}
