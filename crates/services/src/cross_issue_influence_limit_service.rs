use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// 跨Issue影响限制常量
pub const CROSS_ISSUE_INFLUENCE_LIMIT: usize = 20;

/// 跨Issue影响类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossIssueInfluenceKind {
    Comment,
    Update,
    InteractionResolution,
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
    pub agent_id: Uuid,
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
#[derive(Clone)]
pub struct DefaultCrossIssueInfluenceLimitService {
    /// 强制执行开始时间（在此之前只记录，不阻止）
    enforce_at: DateTime<Utc>,
    /// 限制上限
    limit: usize,
    /// 持久化 heartbeat run 与 activity log 的数据库连接池
    pool: Option<PgPool>,
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
            pool: None,
        }
    }

    /// 创建自定义配置的实例
    pub fn with_config(enforce_at: DateTime<Utc>, limit: usize) -> Self {
        Self {
            enforce_at,
            limit,
            pool: None,
        }
    }

    /// 为服务启用数据库持久化。默认构造函数保留纯决策测试能力，生产实例必须配置连接池。
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// 检查是否应该强制执行限制
    fn should_enforce(&self, now: DateTime<Utc>) -> bool {
        now >= self.enforce_at
    }

    /// 从run context快照中读取source issue ID
    fn read_run_source_issue_id(context_snapshot: &serde_json::Value) -> Option<Uuid> {
        let candidates = [
            context_snapshot.get("issueId"),
            context_snapshot.get("taskId"),
            context_snapshot
                .get("assignmentSnapshot")
                .and_then(|v| v.get("issueId")),
        ];
        candidates
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .find_map(|value| Uuid::parse_str(value).ok())
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
        let pool = self.pool.as_ref().ok_or_else(|| {
            InfluenceLimitError::DatabaseError(
                "cross-issue influence persistence is not configured".to_string(),
            )
        })?;
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;

        let run = sqlx::query(
            "SELECT company_id, agent_id, responsible_user_id, context_snapshot
             FROM heartbeat_runs
             WHERE id = $1 AND company_id = $2 AND agent_id = $3
             FOR UPDATE",
        )
        .bind(input.heartbeat_run_id)
        .bind(input.company_id)
        .bind(input.agent_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?
        .ok_or(InfluenceLimitError::RunNotFound(input.heartbeat_run_id))?;

        let run_company_id: Uuid = run
            .try_get("company_id")
            .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        let run_agent_id: Uuid = run
            .try_get("agent_id")
            .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        let context_snapshot: Option<serde_json::Value> = run
            .try_get("context_snapshot")
            .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        if run_company_id != input.company_id || run_agent_id != input.agent_id {
            return Err(InfluenceLimitError::RunNotFound(input.heartbeat_run_id));
        }
        let source_issue_id = context_snapshot
            .as_ref()
            .and_then(Self::read_run_source_issue_id)
            .ok_or(InfluenceLimitError::RunContextRequired)?;
        if !self.is_cross_issue_operation(source_issue_id, input.target_issue_id) {
            return Ok(None);
        }

        let prior_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM activity_logs
             WHERE company_id = $1 AND run_id = $2 AND event_type = $3",
        )
        .bind(input.company_id)
        .bind(input.heartbeat_run_id)
        .bind(CROSS_ISSUE_INFLUENCE_ACTIVITY)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        let decision = self.evaluate_limit(prior_count.max(0) as usize, Utc::now());
        let event_type = if decision.allowed {
            CROSS_ISSUE_INFLUENCE_ACTIVITY
        } else {
            CROSS_ISSUE_INFLUENCE_REJECTED_ACTIVITY
        };
        let metadata = serde_json::json!({
            "kind": input.influence_kind,
            "sourceIssueId": source_issue_id,
            "targetIssueId": input.target_issue_id,
            "actorLabel": input.actor_label,
            "assigneeLabel": input.assignee_label,
            "issueIdentifier": input.issue_identifier,
            "count": decision.count,
            "cap": decision.cap,
            "mode": decision.mode,
            "enforceAt": decision.enforce_at,
            "allowed": decision.allowed,
        });
        sqlx::query(
            "INSERT INTO activity_logs
             (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, run_id, agent_id)
             VALUES ($1, $2, 'agent', $3, 'issue', $4, $5, $6, $3)",
        )
        .bind(input.company_id)
        .bind(event_type)
        .bind(input.agent_id)
        .bind(input.target_issue_id)
        .bind(metadata)
        .bind(input.heartbeat_run_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;

        if !decision.allowed && decision.mode == CrossIssueInfluenceMode::Enforce {
            return Err(InfluenceLimitError::LimitExceeded {
                current: decision.count,
                cap: self.limit,
            });
        }
        Ok(Some(decision))
    }

    async fn get_influence_count(
        &self,
        heartbeat_run_id: Uuid,
        company_id: Uuid,
    ) -> Result<usize, InfluenceLimitError> {
        let pool = self.pool.as_ref().ok_or_else(|| {
            InfluenceLimitError::DatabaseError(
                "cross-issue influence persistence is not configured".to_string(),
            )
        })?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM activity_logs
             WHERE company_id = $1 AND run_id = $2 AND event_type = $3",
        )
        .bind(company_id)
        .bind(heartbeat_run_id)
        .bind(CROSS_ISSUE_INFLUENCE_ACTIVITY)
        .fetch_one(pool)
        .await
        .map_err(|error| InfluenceLimitError::DatabaseError(error.to_string()))?;
        Ok(count.max(0) as usize)
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
    use sqlx::postgres::PgPoolOptions;

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
            "issueId": "550e8400-e29b-41d4-a716-446655440000"
        });

        let issue_id = DefaultCrossIssueInfluenceLimitService::read_run_source_issue_id(&context);
        assert!(issue_id.is_some());
    }

    #[tokio::test]
    async fn observe_requires_persistent_storage_instead_of_using_a_fake_counter() {
        let service = DefaultCrossIssueInfluenceLimitService::new();
        let error = service
            .observe_influence(ObserveCrossIssueInfluenceInput {
                heartbeat_run_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                source_issue_id: Uuid::new_v4(),
                target_issue_id: Uuid::new_v4(),
                influence_kind: CrossIssueInfluenceKind::Comment,
                actor_label: None,
                assignee_label: None,
                issue_identifier: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(error, InfluenceLimitError::DatabaseError(_)));
    }

    #[tokio::test]
    async fn persistent_counter_serializes_concurrent_attempts() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&database_url)
            .await
            .unwrap();
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let source_issue_id = Uuid::new_v4();
        let prefix = format!("T{}", &company_id.simple().to_string()[..7]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(company_id)
            .bind("cross-issue-limit-test")
            .bind(prefix)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
            .bind(agent_id)
            .bind(company_id)
            .bind("cross-issue-limit-test-agent")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO heartbeat_runs (id, company_id, agent_id, context_snapshot)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(run_id)
        .bind(company_id)
        .bind(agent_id)
        .bind(serde_json::json!({ "issueId": source_issue_id }))
        .execute(&pool)
        .await
        .unwrap();

        let service = DefaultCrossIssueInfluenceLimitService::new().with_pool(pool.clone());
        let mut tasks = Vec::new();
        for _ in 0..(CROSS_ISSUE_INFLUENCE_LIMIT + 1) {
            let service = service.clone();
            tasks.push(tokio::spawn(async move {
                service
                    .observe_influence(ObserveCrossIssueInfluenceInput {
                        heartbeat_run_id: run_id,
                        company_id,
                        agent_id,
                        source_issue_id,
                        target_issue_id: Uuid::new_v4(),
                        influence_kind: CrossIssueInfluenceKind::Update,
                        actor_label: None,
                        assignee_label: None,
                        issue_identifier: None,
                    })
                    .await
            }));
        }
        let mut rejected = 0;
        for task in tasks {
            if matches!(task.await.unwrap(), Err(InfluenceLimitError::LimitExceeded { .. })) {
                rejected += 1;
            }
        }
        assert_eq!(rejected, 1);
        assert_eq!(service.get_influence_count(run_id, company_id).await.unwrap(), 20);

        sqlx::query("DELETE FROM companies WHERE id = $1")
            .bind(company_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
