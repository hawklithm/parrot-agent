use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 跨Issue影响限制常量
pub const CROSS_ISSUE_INFLUENCE_LIMIT: usize = 20;

/// 跨Issue影响类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossIssueInfluenceKind {
    Comment,
    Update,
}

/// 跨Issue影响决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossIssueInfluenceDecision {
    /// 是否允许操作
    pub allowed: bool,
    /// 执行模式（仅记录 vs 强制执行）
    pub mode: CrossIssueInfluenceMode,
    /// 当前计数
    pub count: usize,
    /// 限制上限
    pub cap: usize,
    /// 强制执行开始时间
    pub enforce_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossIssueInfluenceMode {
    LogOnly,
    Enforce,
}

/// 观察输入
#[derive(Debug, Clone)]
pub struct ObserveCrossIssueInfluenceInput {
    pub heartbeat_run_id: Uuid,
    pub company_id: Uuid,
    pub source_issue_id: Uuid,
    pub target_issue_id: Uuid,
    pub influence_kind: CrossIssueInfluenceKind,
    pub actor_label: Option<String>,
    pub assignee_label: Option<String>,
    pub issue_identifier: Option<String>,
}

/// 跨Issue影响限制服务
/// 
/// 此服务防止单个agent运行在过多的issue上产生影响，
/// 这是一个重要的安全机制，防止失控的agent造成广泛破坏。
#[async_trait]
pub trait CrossIssueInfluenceLimitService: Send + Sync {
    /// 评估跨Issue影响限制
    /// 
    /// 根据当前计数和时间戳决定是否允许跨Issue操作。
    fn evaluate_limit(
        &self,
        current_count: usize,
        now: DateTime<Utc>,
    ) -> CrossIssueInfluenceDecision;

    /// 原子地观察一次跨Issue影响尝试
    /// 
    /// 此方法会：
    /// 1. 检查是否是跨Issue操作（source != target）
    /// 2. 原子地增加计数器
    /// 3. 记录activity log
    /// 4. 返回是否允许的决策
    async fn observe_influence(
        &self,
        input: ObserveCrossIssueInfluenceInput,
    ) -> Result<Option<CrossIssueInfluenceDecision>, InfluenceLimitError>;

    /// 获取heartbeat run的当前跨Issue影响计数
    async fn get_influence_count(
        &self,
        heartbeat_run_id: Uuid,
        company_id: Uuid,
    ) -> Result<usize, InfluenceLimitError>;

    /// 创建跨Issue影响限制错误消息
    fn create_limit_error(
        &self,
        decision: &CrossIssueInfluenceDecision,
        context: ErrorContext,
    ) -> String;
}

/// 错误上下文
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    pub actor_label: Option<String>,
    pub assignee_label: Option<String>,
    pub issue_identifier: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InfluenceLimitError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Heartbeat run not found: {0}")]
    RunNotFound(Uuid),

    #[error("Cross-issue influence limit exceeded: {current}/{cap}")]
    LimitExceeded { current: usize, cap: usize },

    #[error("Run context required for cross-issue operations")]
    RunContextRequired,
}

/// Activity log常量
const CROSS_ISSUE_INFLUENCE_ACTIVITY: &str = "issue.cross_issue_influence_observed";
const CROSS_ISSUE_INFLUENCE_REJECTED_ACTIVITY: &str = "issue.cross_issue_influence_cap_rejected";

/// 默认跨Issue影响限制服务实现
pub struct DefaultCrossIssueInfluenceLimitService {
    /// 强制执行开始时间（在此之前只记录，不阻止）
    enforce_at: DateTime<Utc>,
    /// 限制上限
    limit: usize,
    // TODO: 添加数据库连接池和activity log服务
}

impl DefaultCrossIssueInfluenceLimitService {
    /// 创建新的服务实例
    pub fn new() -> Self {
        Self {
            // 默认强制执行时间：2026-08-11
            enforce_at: DateTime::parse_from_rfc3339("2026-08-11T00:00:00.000Z")
                .unwrap()
                .with_timezone(&Utc),
            limit: CROSS_ISSUE_INFLUENCE_LIMIT,
        }
    }

    /// 创建自定义配置的实例
    pub fn with_config(enforce_at: DateTime<Utc>, limit: usize) -> Self {
        Self { enforce_at, limit }
    }

    /// 检查是否应该强制执行限制
    fn should_enforce(&self, now: DateTime<Utc>) -> bool {
        now >= self.enforce_at
    }

    /// 从run context快照中读取source issue ID
    fn read_run_source_issue_id(context_snapshot: &serde_json::Value) -> Option<Uuid> {
        context_snapshot
            .get("assignmentSnapshot")
            .and_then(|v| v.get("issueId"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// 检查是否是跨Issue操作
    fn is_cross_issue_operation(&self, source_id: Uuid, target_id: Uuid) -> bool {
        source_id != target_id
    }
}

impl Default for DefaultCrossIssueInfluenceLimitService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CrossIssueInfluenceLimitService for DefaultCrossIssueInfluenceLimitService {
    fn evaluate_limit(
        &self,
        current_count: usize,
        now: DateTime<Utc>,
    ) -> CrossIssueInfluenceDecision {
        let should_enforce = self.should_enforce(now);
        let allowed = current_count < self.limit;

        CrossIssueInfluenceDecision {
            allowed: if should_enforce { allowed } else { true },
            mode: if should_enforce {
                CrossIssueInfluenceMode::Enforce
            } else {
                CrossIssueInfluenceMode::LogOnly
            },
            count: current_count,
            cap: self.limit,
            enforce_at: self.enforce_at,
        }
    }

    async fn observe_influence(
        &self,
        input: ObserveCrossIssueInfluenceInput,
    ) -> Result<Option<CrossIssueInfluenceDecision>, InfluenceLimitError> {
        // 检查是否是跨Issue操作
        if !self.is_cross_issue_operation(input.source_issue_id, input.target_issue_id) {
            return Ok(None);
        }

        // TODO: 实现原子计数逻辑
        // 1. 在数据库中原子地增加计数器
        // 2. 获取更新后的计数
        // 3. 记录activity log
        
        let current_count = 1; // TODO: 从数据库获取实际计数
        let now = Utc::now();
        let decision = self.evaluate_limit(current_count, now);

        // TODO: 记录activity log
        // if decision.allowed || decision.mode == CrossIssueInfluenceMode::LogOnly {
        //     log_activity(CROSS_ISSUE_INFLUENCE_ACTIVITY, ...)
        // } else {
        //     log_activity(CROSS_ISSUE_INFLUENCE_REJECTED_ACTIVITY, ...)
        // }

        tracing::info!(
            run_id = %input.heartbeat_run_id,
            source = %input.source_issue_id,
            target = %input.target_issue_id,
            kind = ?input.influence_kind,
            count = current_count,
            allowed = decision.allowed,
            mode = ?decision.mode,
            "Cross-issue influence observed"
        );

        if !decision.allowed && decision.mode == CrossIssueInfluenceMode::Enforce {
            return Err(InfluenceLimitError::LimitExceeded {
                current: current_count,
                cap: self.limit,
            });
        }

        Ok(Some(decision))
    }

    async fn get_influence_count(
        &self,
        _heartbeat_run_id: Uuid,
        _company_id: Uuid,
    ) -> Result<usize, InfluenceLimitError> {
        // TODO: 查询数据库获取实际计数
        // SELECT COUNT(*) FROM activity_log 
        // WHERE heartbeat_run_id = ? 
        //   AND company_id = ?
        //   AND activity_type = CROSS_ISSUE_INFLUENCE_ACTIVITY
        Ok(0)
    }

    fn create_limit_error(
        &self,
        decision: &CrossIssueInfluenceDecision,
        context: ErrorContext,
    ) -> String {
        let actor = context.actor_label.unwrap_or_else(|| "Agent".to_string());
        let assignee = context.assignee_label.as_deref();
        let identifier = context.issue_identifier.as_deref();

        let issue_ref = match (assignee, identifier) {
            (Some(a), Some(id)) => format!("{} ({})", id, a),
            (Some(a), None) => format!("issue assigned to {}", a),
            (None, Some(id)) => id.to_string(),
            (None, None) => "this issue".to_string(),
        };

        format!(
            "{} attempted to influence {} different issues in this run. \
             Paperclip limits cross-issue influence to {} per run to prevent \
             runaway automation. The current mode is {:?}. \
             Current count: {}/{}. \
             Enforcement started at: {}.",
            actor,
            decision.count,
            decision.cap,
            decision.mode,
            decision.count,
            decision.cap,
            decision.enforce_at.format("%Y-%m-%d")
        )
    }
}

/// 创建run context错误
pub fn cross_issue_influence_run_context_error() -> InfluenceLimitError {
    InfluenceLimitError::RunContextRequired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_limit_before_enforcement() {
        let service = DefaultCrossIssueInfluenceLimitService::with_config(
            DateTime::parse_from_rfc3339("2026-12-01T00:00:00.000Z")
                .unwrap()
                .with_timezone(&Utc),
            20,
        );

        let now = DateTime::parse_from_rfc3339("2026-11-01T00:00:00.000Z")
            .unwrap()
            .with_timezone(&Utc);

        let decision = service.evaluate_limit(25, now);
        assert!(decision.allowed); // log_only模式下总是允许
        assert_eq!(decision.mode, CrossIssueInfluenceMode::LogOnly);
    }

    #[test]
    fn test_evaluate_limit_after_enforcement() {
        let service = DefaultCrossIssueInfluenceLimitService::with_config(
            DateTime::parse_from_rfc3339("2026-08-01T00:00:00.000Z")
                .unwrap()
                .with_timezone(&Utc),
            20,
        );

        let now = DateTime::parse_from_rfc3339("2026-09-01T00:00:00.000Z")
            .unwrap()
            .with_timezone(&Utc);

        // 在限制内
        let decision = service.evaluate_limit(15, now);
        assert!(decision.allowed);
        assert_eq!(decision.mode, CrossIssueInfluenceMode::Enforce);

        // 超过限制
        let decision = service.evaluate_limit(25, now);
        assert!(!decision.allowed);
        assert_eq!(decision.mode, CrossIssueInfluenceMode::Enforce);
    }

    #[test]
    fn test_is_cross_issue_operation() {
        let service = DefaultCrossIssueInfluenceLimitService::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        assert!(!service.is_cross_issue_operation(id1, id1));
        assert!(service.is_cross_issue_operation(id1, id2));
    }

    #[test]
    fn test_create_limit_error() {
        let service = DefaultCrossIssueInfluenceLimitService::new();
        let decision = CrossIssueInfluenceDecision {
            allowed: false,
            mode: CrossIssueInfluenceMode::Enforce,
            count: 25,
            cap: 20,
            enforce_at: Utc::now(),
        };

        let context = ErrorContext {
            actor_label: Some("TestAgent".to_string()),
            assignee_label: Some("user@example.com".to_string()),
            issue_identifier: Some("ISSUE-123".to_string()),
        };

        let error = service.create_limit_error(&decision, context);
        assert!(error.contains("TestAgent"));
        assert!(error.contains("25"));
        assert!(error.contains("20"));
    }

    #[test]
    fn test_read_run_source_issue_id() {
        let context = serde_json::json!({
            "assignmentSnapshot": {
                "issueId": "550e8400-e29b-41d4-a716-446655440000"
            }
        });

        let issue_id = DefaultCrossIssueInfluenceLimitService::read_run_source_issue_id(&context);
        assert!(issue_id.is_some());
    }
}
