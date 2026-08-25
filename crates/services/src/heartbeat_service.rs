use async_trait::async_trait;
use crate::text_utils::truncate_suffix_chars;
use crate::sse_service::{InMemorySseService, SseService};
use chrono::{DateTime, Utc};
use models::{Agent, AgentStatus, SseEvent, SseEventType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

const CLAUDE_PROVIDER_ENV_KEYS: &[&str] = &[
    "CLAUDE_CODE_USE_OPENAI",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_MODEL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "LLM_API_KEY",
    "LLM_BASE_URL",
    "LLM_MODEL",
];

fn isolate_claude_provider_environment(
    cmd: &mut Command,
    explicit_env: Option<&serde_json::Map<String, Value>>,
) {
    for key in CLAUDE_PROVIDER_ENV_KEYS {
        if explicit_env.map_or(true, |env| !env.contains_key(*key)) {
            cmd.env_remove(key);
        }
    }
}

/// 智能解析环境变量值：
/// 1. 如果value看起来像环境变量引用（纯大写字母数字下划线，或带$前缀），先尝试从环境读取
/// 2. 如果环境变量存在且非空，使用环境变量的值
/// 3. 否则，使用value本身作为实际值
fn resolve_env_value(configured_value: &str) -> String {
    // 去除可能的 $ 前缀和 ${} 包裹
    let trimmed = configured_value.trim();
    let key = trimmed
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix("}"))
        .or_else(|| trimmed.strip_prefix("$"))
        .unwrap_or(trimmed);
    
    // 检查是否看起来像环境变量名（纯大写字母、数字、下划线）
    let looks_like_env_var = !key.is_empty() 
        && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    
    if looks_like_env_var {
        // 尝试从当前环境读取
        if let Ok(env_value) = std::env::var(key) {
            if !env_value.is_empty() {
                tracing::debug!(
                    key = %key,
                    "resolved env var reference from host environment"
                );
                return env_value;
            }
        }
    }
    
    // 回退到使用配置值本身（保持原样，不trim）
    configured_value.to_string()
}

/// 从 adapters 目录加载默认配置
/// 文件名规则：adapter_type 的下划线转横线，如 "claude_local" → "claude-local.json"
fn load_default_adapter_config(adapter_type: &str) -> Option<serde_json::Value> {
    let file_name = adapter_type.replace('_', "-");
    let config_path = format!("adapters/{}.json", file_name);
    
    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(config) => {
                    tracing::debug!(
                        adapter_type = %adapter_type,
                        config_path = %config_path,
                        "loaded default adapter config from file"
                    );
                    Some(config)
                }
                Err(e) => {
                    tracing::warn!(
                        adapter_type = %adapter_type,
                        config_path = %config_path,
                        error = %e,
                        "failed to parse default adapter config"
                    );
                    None
                }
            }
        }
        Err(e) => {
            tracing::debug!(
                adapter_type = %adapter_type,
                config_path = %config_path,
                error = %e,
                "no default adapter config file found"
            );
            None
        }
    }
}

/// 合并配置：数据库配置优先，默认配置填充缺失字段
/// 
/// 合并规则：
/// - 如果数据库配置中某个字段存在，使用数据库的值
/// - 如果数据库配置中某个字段不存在，使用默认配置的值
/// - 特别处理 "env" 字段：如果数据库没有，从默认配置补充
fn merge_adapter_config(
    db_config: serde_json::Value,
    default_config: Option<serde_json::Value>,
) -> serde_json::Value {
    let Some(default) = default_config else {
        return db_config;
    };
    
    // 如果数据库配置不是对象，直接返回
    let Some(db_obj) = db_config.as_object() else {
        return db_config;
    };
    
    // 如果默认配置不是对象，返回数据库配置
    let Some(default_obj) = default.as_object() else {
        return db_config;
    };
    
    // 合并：从默认配置开始，用数据库配置覆盖
    let mut merged = default_obj.clone();
    for (key, value) in db_obj {
        merged.insert(key.clone(), value.clone());
    }
    
    tracing::debug!(
        db_keys = ?db_obj.keys().collect::<Vec<_>>(),
        default_keys = ?default_obj.keys().collect::<Vec<_>>(),
        merged_keys = ?merged.keys().collect::<Vec<_>>(),
        "merged adapter config: db + default"
    );
    
    serde_json::Value::Object(merged)
}


/// Heartbeat service for managing agent wake/sleep lifecycle
#[async_trait]
pub trait HeartbeatService: Send + Sync {
    /// Wake up an agent to work on an issue
    /// Called after checkout to notify the assignee
    async fn wakeup(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<(), HeartbeatError>;

    /// Wake an agent with Paperclip-compatible event context. Existing callers
    /// can keep using `wakeup`; context-aware callers opt into rewake policy.
    async fn wakeup_with_options(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: HeartbeatWakeupOptions,
    ) -> Result<(), HeartbeatError> {
        let _ = options;
        self.wakeup(agent_id, issue_id, company_id).await
    }

    /// Cancel an active run for an issue
    /// Called after force_release to stop ongoing execution
    async fn cancel_run(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<(), HeartbeatError>;

    /// Cancel only a pending scheduled retry for an issue.
    async fn cancel_scheduled_retry(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<bool, HeartbeatError>;

    /// Get heartbeat context for an issue (diagnostics/monitoring)
    async fn get_heartbeat_context(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<HeartbeatContext, HeartbeatError>;
}

#[derive(Debug, Clone, Default)]
pub struct HeartbeatWakeupOptions {
    pub source: Option<String>,
    pub trigger_detail: Option<String>,
    pub reason: Option<String>,
    pub requested_by_actor_type: Option<String>,
    pub requested_by_actor_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub payload: Option<Value>,
    pub context_snapshot: Option<Value>,
}

/// Heartbeat context information for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatContext {
    pub issue_id: Uuid,
    pub company_id: Uuid,
    pub active_agents: Vec<AgentHeartbeatInfo>,
    pub last_wakeup_at: Option<DateTime<Utc>>,
    pub wakeup_count: i64,
}

/// Agent heartbeat information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHeartbeatInfo {
    pub agent_id: Uuid,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub status: HeartbeatStatus,
}

#[derive(Debug, Default)]
struct AdapterOutcome {
    explicit_failure: bool,
    failure_reason: Option<String>,
    error_code: Option<String>,
    error_family: Option<String>,
    result_summary: Option<String>,
    tool_call_count: usize,
    handoff: Option<Value>,
    result_event: Option<Value>,
    // Token usage and cost tracking
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cost_usd: Option<f64>,
    model: Option<String>,
    provider: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug)]
struct AdapterCommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// Read the structured result records emitted by Claude/Codex JSONL modes.
/// Exit status remains the process-level fallback, but an adapter can emit an
/// explicit error/result record before exiting zero; treating that as success
/// would incorrectly complete the Issue.
fn parse_adapter_outcome(output: &str) -> AdapterOutcome {
    let mut outcome = AdapterOutcome::default();
    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        visit_adapter_event(&value, &mut outcome, true);
    }
    outcome
}

fn valid_claude_resume_session(session_id: Option<&str>) -> Option<String> {
    let session_id = session_id?.trim();
    if uuid::Uuid::parse_str(session_id).is_ok() { Some(session_id.to_string()) } else { None }
}

fn classify_claude_error(message: &str) -> (Option<String>, Option<String>) {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("not logged in")
        || normalized.contains("please log in")
        || normalized.contains("login required")
        || normalized.contains("authentication required")
        || normalized.contains("unauthorized")
        || normalized.contains("invalid api key")
    {
        return (Some("claude_auth_required".to_string()), Some("authentication".to_string()));
    }
    if normalized.contains("rate limit")
        || normalized.contains("rate_limit_error")
        || normalized.contains("too many requests")
        || normalized.contains("overloaded")
        || normalized.contains("service unavailable")
        || normalized.contains("429")
        || normalized.contains("503")
        || normalized.contains("529")
        || normalized.contains("usage limit")
        || normalized.contains("out of extra usage")
    {
        return (Some("claude_transient_upstream".to_string()), Some("transient_upstream".to_string()));
    }
    if normalized.contains("empty or malformed response") {
        return (Some("claude_malformed_response".to_string()), Some("upstream_protocol".to_string()));
    }
    (None, None)
}

/// Human-readable reason recorded on a scheduled retry, derived from the
/// adapter outcome. Falls back to the explicit failure reason, then to the
/// classified error code, then a generic message.
fn retry_reason(outcome: &AdapterOutcome) -> String {
    if let Some(reason) = outcome.failure_reason.as_deref() {
        if !reason.trim().is_empty() {
            return reason.to_string();
        }
    }
    if let Some(code) = outcome.error_code.as_deref() {
        return format!("recoverable failure: {code}");
    }
    "recoverable failure".to_string()
}

fn visit_adapter_event(value: &Value, outcome: &mut AdapterOutcome, top_level: bool) {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "tool_use" | "tool_call") || value.get("tool_name").is_some() {
        outcome.tool_call_count += 1;
    }
    if kind == "handoff" || value.get("handoff").is_some() {
        outcome.handoff = value.get("handoff").cloned().or_else(|| Some(value.clone()));
    }

    if outcome.session_id.is_none() {
        outcome.session_id = value
            .get("session_id")
            .or_else(|| value.get("sessionId"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }

    // Parse usage and cost information (Claude emits snake_case JSONL fields;
    // retain the camelCase aliases used by older Parrot adapters).
    if let Some(usage) = value.get("usage") {
        if let Some(input) = usage.get("input_tokens").or_else(|| usage.get("inputTokens")).and_then(Value::as_i64) {
            outcome.input_tokens = outcome.input_tokens.max(input);
        }
        if let Some(output) = usage.get("output_tokens").or_else(|| usage.get("outputTokens")).and_then(Value::as_i64) {
            outcome.output_tokens = outcome.output_tokens.max(output);
        }
        if let Some(cached) = usage.get("cache_read_input_tokens").or_else(|| usage.get("cached_input_tokens")).or_else(|| usage.get("cachedInputTokens")).and_then(Value::as_i64) {
            outcome.cached_input_tokens = outcome.cached_input_tokens.max(cached);
        }
    }
    if let Some(cost) = value.get("total_cost_usd").or_else(|| value.get("costUsd")).and_then(Value::as_f64) {
        outcome.cost_usd = Some(cost);
    }
    if outcome.model.is_none() {
        if let Some(model) = value.get("model").and_then(Value::as_str) {
            outcome.model = Some(model.to_string());
        }
    }
    if outcome.provider.is_none() {
        if let Some(provider) = value.get("provider").and_then(Value::as_str) {
            outcome.provider = Some(provider.to_string());
        }
    }

    // Claude Code nests tool_use records inside assistant.message.content. Only
    // top-level adapter/result records can determine the process outcome: a
    // recoverable tool_result error must not turn an otherwise successful run
    // into a failed heartbeat.
    let is_error = top_level
        && (value.get("is_error").and_then(Value::as_bool).unwrap_or(false)
            || value.get("isError").and_then(Value::as_bool).unwrap_or(false)
            || matches!(value.get("subtype").and_then(Value::as_str), Some("error" | "failed")));
    if is_error {
        outcome.explicit_failure = true;
        let reason = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
            .or_else(|| value.get("result").and_then(Value::as_str))
            .map(ToOwned::to_owned);
        if let Some(reason) = reason {
            let (error_code, error_family) = classify_claude_error(&reason);
            outcome.failure_reason = Some(reason);
            outcome.error_code = error_code;
            outcome.error_family = error_family;
        }
        outcome.result_event = Some(value.clone());
    }
    if let Some(result) = value.get("result").and_then(Value::as_str) {
        outcome.result_summary = Some(result.to_owned());
    }
    match value {
        Value::Array(values) => values.iter().for_each(|item| visit_adapter_event(item, outcome, false)),
        Value::Object(values) => values.values().for_each(|item| visit_adapter_event(item, outcome, false)),
        _ => {}
    }
}

/// Heartbeat status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatStatus {
    Active,
    Idle,
    Sleeping,
    Unknown,
}

/// Heartbeat error
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),

    #[error("Issue not found: {0}")]
    IssueNotFound(Uuid),

    #[error("Wakeup failed: {0}")]
    WakeupFailed(String),

    #[error("Cancel run failed: {0}")]
    CancelRunFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Production heartbeat coordinator.
///
/// A wake is durable before execution starts: the wake request and heartbeat
/// run are inserted first, then the adapter is launched asynchronously. This
/// keeps issue liveness correct across request failures and makes cancellation
/// addressable by run id.
pub struct DefaultHeartbeatService {
    pool: PgPool,
    children: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Child>>>>>,
    sse_service: Arc<dyn SseService>,
    cost_service: Option<Arc<dyn crate::CostService>>,
    budget_service: Option<Arc<dyn crate::BudgetService>>,
}

async fn publish_live_event(
    service: &Arc<dyn SseService>,
    company_id: Uuid,
    event_type: &str,
    payload: Value,
) {
    let event = serde_json::json!({
        "id": Uuid::new_v4(),
        "companyId": company_id,
        "type": event_type,
        "createdAt": Utc::now(),
        "payload": payload,
    });
    let _ = service
        .publish(
            company_id,
            "events",
            SseEvent {
                event_type: SseEventType::Message,
                channel: "events".to_string(),
                payload: event,
                timestamp: Utc::now(),
            },
        )
        .await;
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:@%+=,-".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn shell_command(
    command: &str,
    args: &[String],
    cwd: Option<&str>,
    stdin_prompt: Option<&str>,
) -> String {
    let command_line = std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let invocation = match stdin_prompt {
        Some(prompt) => format!("printf '%s' {} | {}", shell_quote(prompt), command_line),
        None => command_line,
    };
    match cwd {
        Some(cwd) => format!("cd {} && {{ {}; }}", shell_quote(cwd), invocation),
        None => invocation,
    }
}

fn redact_gateway_token(value: &str, token: &str) -> String {
    value.replace(token, "[PAPERCLIP_TOOL_GATEWAY_TOKEN]")
}

impl DefaultHeartbeatService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            children: Arc::new(Mutex::new(HashMap::new())),
            sse_service: InMemorySseService::new(),
            cost_service: None,
            budget_service: None,
        }
    }

    pub fn with_sse_service(mut self, sse_service: Arc<dyn SseService>) -> Self {
        self.sse_service = sse_service;
        self
    }

    pub fn with_cost_service(mut self, cost_service: Arc<dyn crate::CostService>) -> Self {
        self.cost_service = Some(cost_service);
        self
    }

    pub fn with_budget_service(mut self, budget_service: Arc<dyn crate::BudgetService>) -> Self {
        self.budget_service = Some(budget_service);
        self
    }

    /// 克隆 service 用于后台任务
    fn clone_for_background(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            children: Arc::clone(&self.children),
            sse_service: Arc::clone(&self.sse_service),
            cost_service: self.cost_service.clone(),
            budget_service: self.budget_service.clone(),
        }
    }

    /// 优雅终止进程：先发送 SIGTERM，等待 grace period，然后 SIGKILL
    async fn terminate_process_gracefully(
        &self,
        child: Arc<Mutex<Child>>,
        grace_ms: u64,
    ) -> Result<(), String> {
        let pid = {
            let child_guard = child.lock().await;
            child_guard.id()
        };
        
        if let Some(pid) = pid {
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                
                // 1. 发送 SIGTERM
                let unix_pid = Pid::from_raw(pid as i32);
                if let Err(e) = kill(unix_pid, Signal::SIGTERM) {
                    tracing::warn!(pid = %pid, error = %e, "failed to send SIGTERM, trying SIGKILL");
                    let _ = kill(unix_pid, Signal::SIGKILL);
                    return Ok(());
                }
                
                tracing::debug!(pid = %pid, grace_ms = %grace_ms, "sent SIGTERM, waiting for graceful shutdown");
                
                // 2. 等待 grace period，检查进程是否已退出
                let deadline = tokio::time::Instant::now() + Duration::from_millis(grace_ms);
                let check_interval = Duration::from_millis(100);
                
                while tokio::time::Instant::now() < deadline {
                    // 检查进程是否还活着
                    match kill(unix_pid, None) {
                        Err(nix::errno::Errno::ESRCH) => {
                            // 进程已退出
                            tracing::debug!(pid = %pid, "process exited gracefully");
                            return Ok(());
                        }
                        Ok(_) => {
                            // 进程还活着，继续等待
                            tokio::time::sleep(check_interval).await;
                        }
                        Err(e) => {
                            tracing::warn!(pid = %pid, error = %e, "error checking process liveness");
                            break;
                        }
                    }
                }
                
                // 3. Grace period 超时，发送 SIGKILL
                tracing::warn!(pid = %pid, "grace period expired, sending SIGKILL");
                let _ = kill(unix_pid, Signal::SIGKILL);
            }
            
            #[cfg(not(unix))]
            {
                // Windows: 直接 kill
                let mut child_guard = child.lock().await;
                let _ = child_guard.kill().await;
            }
        }
        
        Ok(())
    }

    /// 启动队列中的下一个 run（从 paperclip 完整迁移）
    /// Phase 2: 依赖就绪检查 + 4级排序
    /// Phase 3: Claim验证（简化版，不包括预算和组织结构检查）
    async fn start_next_queued_run_for_agent(&self, agent_id: Uuid) -> Result<Vec<Uuid>, String> {
        // 1. 检查 agent 是否存在
        let agent = self.load_agent(agent_id).await.map_err(|e| e.to_string())?;
        let company_id = agent.company_id;
        
        // 2. 检查 agent 是否可调用（不在暂停/删除状态）
        if matches!(agent.status, AgentStatus::Paused | AgentStatus::Terminated) {
            tracing::debug!(%agent_id, status = ?agent.status, "agent not invokable, skipping queue");
            return Ok(vec![]);
        }
        
        // 3. 获取 maxConcurrentRuns 配置（默认 1）
        let max_concurrent_runs: i32 = agent
            .adapter_config
            .0
            .get("maxConcurrentRuns")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(1)
            .max(1)
            .min(50);
        
        // 4. 查询当前正在运行的 run 数量
        let running_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM heartbeat_runs WHERE agent_id = $1 AND status IN ('running', 'queued')"
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        
        let available_slots = (max_concurrent_runs as i64 - running_count).max(0);
        if available_slots <= 0 {
            tracing::debug!(%agent_id, running_count, max_concurrent_runs, "no available slots for new runs");
            return Ok(vec![]);
        }
        
        // 5. 查询队列中的所有 runs（不做 SQL 排序，在内存中排序）
        #[derive(sqlx::FromRow, Debug)]
        struct QueuedRun {
            id: Uuid,
            issue_id: Option<String>,
            priority: Option<i32>,
            created_at: DateTime<Utc>,
        }
        
        let queued_runs: Vec<QueuedRun> = sqlx::query_as(
            "SELECT r.id, r.context_snapshot->>'issueId' as issue_id, i.priority, r.created_at
             FROM heartbeat_runs r
             LEFT JOIN issues i ON i.id = (r.context_snapshot->>'issueId')::uuid AND i.company_id = r.company_id
             WHERE r.agent_id = $1 AND r.status = 'queued'
             ORDER BY r.created_at ASC"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        
        if queued_runs.is_empty() {
            tracing::debug!(%agent_id, "no queued runs to start");
            return Ok(vec![]);
        }
        
        tracing::debug!(
            %agent_id,
            queued_count = %queued_runs.len(),
            available_slots = %available_slots,
            "processing queued runs with dependency check"
        );
        
        // 6. Phase 2: 依赖就绪检查（从 paperclip 迁移）
        let issue_ids: Vec<Uuid> = queued_runs
            .iter()
            .filter_map(|run| run.issue_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()))
            .collect();
        
        // 6.1 查询所有相关 issues 的状态
        #[derive(sqlx::FromRow)]
        #[allow(dead_code)]
        struct IssueInfo {
            id: Uuid,
            status: String,
            priority: Option<i32>,
        }
        
        let issues: Vec<IssueInfo> = if !issue_ids.is_empty() {
            sqlx::query_as(
                "SELECT id, status::text, priority FROM issues WHERE id = ANY($1)"
            )
            .bind(&issue_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        
        // 6.2 查询依赖关系（blocker issues）
        #[derive(sqlx::FromRow)]
        struct BlockerRelation {
            blocked_issue_id: Uuid,
            blocker_issue_id: Uuid,
        }
        
        let blocker_relations: Vec<BlockerRelation> = if !issue_ids.is_empty() {
            sqlx::query_as(
                "SELECT related_issue_id as blocked_issue_id, issue_id as blocker_issue_id
                 FROM issue_relations 
                 WHERE company_id = $1 
                   AND related_issue_id = ANY($2) 
                   AND type = 'blocks'"
            )
            .bind(company_id)
            .bind(&issue_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        
        // 6.3 查询 blocker issues 的状态
        let blocker_ids: Vec<Uuid> = blocker_relations.iter()
            .map(|r| r.blocker_issue_id)
            .collect();
        
        #[derive(sqlx::FromRow)]
        struct BlockerStatus {
            id: Uuid,
            status: String,
        }
        
        let blocker_statuses: Vec<BlockerStatus> = if !blocker_ids.is_empty() {
            sqlx::query_as(
                "SELECT id, status::text FROM issues WHERE id = ANY($1)"
            )
            .bind(&blocker_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        
        // 6.4 构建依赖就绪映射
        use std::collections::HashMap;
        
        let issue_map: HashMap<Uuid, &IssueInfo> = 
            issues.iter().map(|i| (i.id, i)).collect();
        
        let blocker_status_map: HashMap<Uuid, String> = 
            blocker_statuses.into_iter().map(|b| (b.id, b.status)).collect();
        
        // 计算每个 issue 的依赖就绪状态
        let mut issue_readiness: HashMap<Uuid, bool> = HashMap::new();
        let mut issue_unresolved_blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        
        for issue_id in &issue_ids {
            let blockers_for_issue: Vec<Uuid> = blocker_relations
                .iter()
                .filter(|r| r.blocked_issue_id == *issue_id)
                .map(|r| r.blocker_issue_id)
                .collect();
            
            if blockers_for_issue.is_empty() {
                // 没有依赖，就绪
                issue_readiness.insert(*issue_id, true);
            } else {
                // 检查所有 blocker 是否都是 done 状态
                let unresolved: Vec<Uuid> = blockers_for_issue
                    .iter()
                    .filter(|blocker_id| {
                        blocker_status_map.get(blocker_id)
                            .map(|status| status != "done")
                            .unwrap_or(true) // 找不到状态视为未解决
                    })
                    .copied()
                    .collect();
                
                let is_ready = unresolved.is_empty();
                issue_readiness.insert(*issue_id, is_ready);
                
                if !is_ready {
                    issue_unresolved_blockers.insert(*issue_id, unresolved);
                    
                    tracing::debug!(
                        %issue_id,
                        unresolved_blockers = ?issue_unresolved_blockers.get(issue_id).unwrap(),
                        "issue has unresolved blockers, not ready"
                    );
                }
            }
        }
        
        tracing::debug!(
            %agent_id,
            total_issues = %issue_ids.len(),
            ready_count = %issue_readiness.values().filter(|&ready| *ready).count(),
            blocked_count = %issue_readiness.values().filter(|&ready| !*ready).count(),
            "dependency readiness check complete"
        );
        
        // 7. Phase 2: 智能排序（从 paperclip 迁移的 4 级排序逻辑）
        // Rank 0: 依赖就绪 + in_progress
        // Rank 1: 依赖就绪 + 其他状态
        // Rank 2: 非 issue 任务（heartbeat等）
        // Rank 3: 依赖未就绪（blocked）
        let mut sorted_runs = queued_runs;
        sorted_runs.sort_by(|left, right| {
            let left_issue_id = left.issue_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());
            let right_issue_id = right.issue_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());
            
            let left_ready = left_issue_id
                .and_then(|id| issue_readiness.get(&id).copied())
                .unwrap_or(true); // 非 issue 任务视为就绪
            let right_ready = right_issue_id
                .and_then(|id| issue_readiness.get(&id).copied())
                .unwrap_or(true);
            
            let left_issue = left_issue_id.and_then(|id| issue_map.get(&id));
            let right_issue = right_issue_id.and_then(|id| issue_map.get(&id));
            
            let left_rank = if let Some(issue) = left_issue {
                if left_ready {
                    if issue.status == "in_progress" { 0 } else { 1 }
                } else {
                    3 // blocked
                }
            } else {
                2 // non-issue task
            };
            
            let right_rank = if let Some(issue) = right_issue {
                if right_ready {
                    if issue.status == "in_progress" { 0 } else { 1 }
                } else {
                    3 // blocked
                }
            } else {
                2 // non-issue task
            };
            
            // 首先按 rank 排序
            if left_rank != right_rank {
                return left_rank.cmp(&right_rank);
            }
            
            // 然后按 priority 排序（数字越小优先级越高）
            let left_priority = left.priority.unwrap_or(3);
            let right_priority = right.priority.unwrap_or(3);
            if left_priority != right_priority {
                return left_priority.cmp(&right_priority);
            }
            
            // 最后按创建时间排序
            left.created_at.cmp(&right.created_at)
        });
        
        // 8. Phase 3: Claim 验证并启动（简化版）
        let mut started_runs = Vec::new();
        let mut claimed_count = 0;
        
        for queued_run in sorted_runs.iter() {
            if claimed_count >= available_slots {
                break;
            }
            
            // 8.1 Phase 3: Claim 前验证
            let issue_id_opt = queued_run.issue_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());
            
            // 验证：依赖未就绪的 issue 不应启动
            if let Some(issue_id) = issue_id_opt {
                if let Some(&is_ready) = issue_readiness.get(&issue_id) {
                    if !is_ready {
                        let unresolved = issue_unresolved_blockers.get(&issue_id)
                            .map(|v| v.len())
                            .unwrap_or(0);
                        
                        tracing::info!(
                            run_id = %queued_run.id,
                            %issue_id,
                            unresolved_blockers = %unresolved,
                            "skipping run: issue has unresolved blockers"
                        );
                        continue; // 跳过 blocked issue
                    }
                }
            }
            
            // 8.2 更新状态为 running（atomic claim）
            let result = sqlx::query(
                "UPDATE heartbeat_runs 
                 SET status = 'running', started_at = NOW(), updated_at = NOW() 
                 WHERE id = $1 AND status = 'queued'"
            )
            .bind(queued_run.id)
            .execute(&self.pool)
            .await;
            
            match result {
                Ok(result) if result.rows_affected() > 0 => {
                    claimed_count += 1;
                    
                    // 8.3 启动执行
                    if let Some(issue_id) = issue_id_opt {
                        // 复制所有需要的值以满足 'static 生命周期
                        let run_id = queued_run.id;
                        let service = self.clone_for_background();
                        tokio::spawn(async move {
                            service.execute_run(run_id, agent_id, issue_id, company_id).await;
                        });
                        
                        started_runs.push(run_id);
                        
                        tracing::info!(
                            %run_id,
                            %agent_id,
                            %issue_id,
                            claimed_count = %claimed_count,
                            available_slots = %available_slots,
                            "claimed and started queued run"
                        );
                    }
                }
                Ok(_) => {
                    tracing::warn!(
                        run_id = %queued_run.id,
                        "failed to claim run: already claimed by another process"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        run_id = %queued_run.id,
                        error = %e,
                        "failed to claim run: database error"
                    );
                }
            }
        }
        
        tracing::info!(
            %agent_id,
            started_count = %started_runs.len(),
            available_slots = %available_slots,
            total_queued = %sorted_runs.len(),
            "queue processing complete"
        );
        
        Ok(started_runs)
    }

    async fn load_agent(&self, id: Uuid) -> Result<Agent, HeartbeatError> {
        sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?
            .ok_or(HeartbeatError::AgentNotFound(id))
    }

    async fn refresh_continuation_summary(
        &self,
        issue_id: Uuid,
        run_id: Uuid,
        agent_id: Uuid,
        run_status: &str,
        run_error: Option<&str>,
        output: &str,
    ) {
        let issue = sqlx::query("SELECT company_id, identifier, title, description, status::text AS status, priority::text AS priority FROM issues WHERE id=$1")
            .bind(issue_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        let Some(issue) = issue else { return };
        let agent = sqlx::query("SELECT name, adapter_type FROM agents WHERE id=$1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        let identifier: Option<String> = issue.try_get("identifier").ok();
        let title: String = issue.try_get("title").unwrap_or_default();
        let description: Option<String> = issue.try_get("description").ok();
        let status: String = issue.try_get("status").unwrap_or_else(|_| run_status.to_string());
        let priority: String = issue.try_get("priority").unwrap_or_else(|_| "medium".to_string());
        let agent_name: String = agent.as_ref().and_then(|row| row.try_get("name").ok()).unwrap_or_else(|| "unknown".to_string());
        let adapter_type: String = agent.as_ref().and_then(|row| row.try_get("adapter_type").ok()).unwrap_or_else(|| "unknown".to_string());
        let output_excerpt = output.trim();
        // 使用 text_utils 安全地截取最后 1200 个字符（而不是字节）
        let output_excerpt = truncate_suffix_chars(output_excerpt, 1_200);
        let objective = description.as_deref().unwrap_or("No objective captured.").trim();
        let next_action = if status == "done" {
            "Review the completed issue output and close any remaining follow-up comments."
        } else if status == "in_review" {
            "Wait for reviewer feedback or approval before continuing executor work."
        } else if matches!(run_status, "failed" | "timed_out" | "cancelled") {
            "Inspect the failed run, fix the cause, and resume from the latest concrete action."
        } else {
            "Resume implementation from the acceptance criteria, latest comments, and this summary."
        };
        let error_section = run_error.map(|error| format!("\n\nLatest run error:\n- {}", error.trim())).unwrap_or_default();
        let body = format!(
            "# Continuation Summary\n\n- Issue: {} — {}\n- Status: {}\n- Priority: {}\n- Last updated by run: {}\n- Agent: {} ({})\n\n## Objective\n\n{}\n\n## Recent Concrete Actions\n\n- Run `{}` finished with status `{}`.\n- Adapter output excerpt:\n\n```text\n{}\n```{}\n\n## Commands Run\n\n- Detailed shell command and tool events remain in the heartbeat run log.\n\n## Next Action\n\n- {}",
            identifier.as_deref().unwrap_or(&issue_id.to_string()), title, status, priority, run_id,
            agent_name, adapter_type, objective, run_id, run_status, output_excerpt, error_section, next_action
        );
        let body = if body.len() > 8_000 { format!("{}\n[truncated]", &body[..7_980]) } else { body };
        let existing = sqlx::query("SELECT d.id FROM issue_documents l JOIN documents d ON d.id=l.document_id WHERE l.issue_id=$1 AND l.key='continuation-summary' FOR UPDATE")
            .bind(issue_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        let mut tx = match self.pool.begin().await { Ok(tx) => tx, Err(_) => return };
        let document_id = if let Some(row) = existing {
            let document_id: Uuid = row.try_get("id").unwrap_or_else(|_| Uuid::new_v4());
            let revision: Option<i32> = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1")
                .bind(document_id).fetch_optional(&mut *tx).await.ok().flatten();
            let revision = revision.unwrap_or(1);
            if sqlx::query("UPDATE documents SET content=$2, content_type='text/markdown', updated_at=NOW() WHERE id=$1")
                .bind(document_id).bind(&body).execute(&mut *tx).await.is_err() { return; }
            if sqlx::query("INSERT INTO document_revisions (document_id, revision_number, content) VALUES ($1,$2,$3)")
                .bind(document_id).bind(revision).bind(&body).execute(&mut *tx).await.is_err() { return; }
            document_id
        } else {
            let company_id: Uuid = issue.try_get("company_id").unwrap_or_default();
            let document_id: Uuid = match sqlx::query_scalar("INSERT INTO documents (company_id, content, content_type) VALUES ($1,$2,'text/markdown') RETURNING id")
                .bind(company_id).bind(&body).fetch_one(&mut *tx).await { Ok(id) => id, Err(_) => return };
            if sqlx::query("INSERT INTO issue_documents (company_id, issue_id, document_id, key) VALUES ($1,$2,$3,'continuation-summary')")
                .bind(company_id).bind(issue_id).bind(document_id).execute(&mut *tx).await.is_err() { return; }
            if sqlx::query("INSERT INTO document_revisions (document_id, revision_number, content) VALUES ($1,1,$2)")
                .bind(document_id).bind(&body).execute(&mut *tx).await.is_err() { return; }
            document_id
        };
        if tx.commit().await.is_err() { return; }
        tracing::debug!(%issue_id, %run_id, %document_id, "refreshed issue continuation summary");
    }

    async fn execute_run(&self, run_id: Uuid, agent_id: Uuid, issue_id: Uuid, company_id: Uuid) {
        let pause_blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1
               FROM issue_tree_holds h
               JOIN issue_tree_hold_members m ON m.hold_id = h.id
              WHERE h.company_id = $1 AND m.issue_id = $2
                AND h.mode = 'pause' AND h.status = 'active'
            )",
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false);
        if pause_blocked {
            let interaction_wake: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM heartbeat_runs
                   WHERE id = $1 AND company_id = $2
                     AND context_snapshot->>'wakeReason' = 'issue_reopened_via_comment'
                     AND context_snapshot->>'source' = 'issue.comment.reopen'
                     AND context_snapshot ? 'commentId'
                     AND context_snapshot ? 'requestedByActorId'
                )",
            )
            .bind(run_id)
            .bind(company_id)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(false);
            if !interaction_wake {
                let _ = sqlx::query(
                    "UPDATE heartbeat_runs
                     SET status = 'cancelled', error = 'cancelled by active issue pause hold',
                         finished_at = NOW(), updated_at = NOW()
                     WHERE id = $1 AND status IN ('queued','running')",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE agent_wakeup_requests
                     SET status = 'cancelled', reason = 'issue_pause_hold',
                         finished_at = NOW(), updated_at = NOW()
                     WHERE company_id = $1 AND agent_id = $2
                       AND status IN ('queued','dispatched','running')
                       AND payload->>'runId' = $3",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(run_id.to_string())
                .execute(&self.pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE agents SET status = 'idle', updated_at = NOW() WHERE id = $1",
                )
                .bind(agent_id)
                .execute(&self.pool)
                .await;
                return;
            }
        }
        let result = self.run_command(run_id, agent_id, issue_id, company_id).await;
        let (status, exit_code, error, output, outcome) = match result {
            Ok(command_output) => {
                let combined = format!("{}{}", command_output.stdout, command_output.stderr);
                let outcome = parse_adapter_outcome(&combined);
                if command_output.exit_code == 0 && !outcome.explicit_failure {
                    ("succeeded", Some(command_output.exit_code), None, command_output, outcome)
                } else if outcome.explicit_failure {
                    let reason = outcome
                        .failure_reason
                        .clone()
                        .unwrap_or_else(|| "adapter reported an explicit failure".to_string());
                    (
                        "failed",
                        Some(command_output.exit_code),
                        Some(reason),
                        command_output,
                        outcome,
                    )
                } else {
                    let reason = command_output
                        .stderr
                        .lines()
                        .chain(command_output.stdout.lines())
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or("no adapter output")
                        .to_string();
                    let (error_code, error_family) = classify_claude_error(&reason);
                    let mut outcome = outcome;
                    outcome.error_code = error_code;
                    outcome.error_family = error_family;
                    (
                        "failed",
                        Some(command_output.exit_code),
                        Some(reason),
                        command_output,
                        outcome,
                    )
                }
            }
            Err(error) => {
                let outcome = AdapterOutcome {
                    explicit_failure: true,
                    failure_reason: Some(error.clone()),
                    error_code: Some("adapter_failed".to_string()),
                    error_family: Some("adapter".to_string()),
                    ..Default::default()
                };
                (
                    "failed",
                    None,
                    Some(error),
                    AdapterCommandOutput {
                        exit_code: -1,
                        stdout: String::new(),
                        stderr: String::new(),
                    },
                    outcome,
                )
            }
        };
        let result_json = serde_json::json!({
            "toolCallCount": outcome.tool_call_count,
            "resultSummary": outcome.result_summary,
            "handoff": outcome.handoff,
            "explicitFailure": outcome.explicit_failure,
            "errorCode": outcome.error_code,
            "errorFamily": outcome.error_family,
            "resultEvent": outcome.result_event,
            "sessionId": outcome.session_id,
            "stdout": output.stdout,
            "stderr": output.stderr,
        });
        // Self-healing: a recoverable failure is rescheduled instead of left
        // terminal. maybe_schedule_retry transitions the run to `scheduled_retry`
        // (finished_at cleared); the finalize UPDATE below is guarded by
        // `status IN ('queued','running')` so it is a harmless no-op for the
        // rescheduled run, while token-usage tracking still runs below.
        if status == "failed" {
            let _ = self
                .maybe_schedule_retry(
                    run_id,
                    agent_id,
                    issue_id,
                    company_id,
                    outcome.error_code.as_deref(),
                    outcome.error_family.as_deref(),
                    &retry_reason(&outcome),
                )
                .await;
        }
        if let Err(error) = sqlx::query(
            "UPDATE heartbeat_runs SET status = $2::heartbeat_run_status, exit_code = $3, error = $4, output = $5, result_json = $6, error_code = $7, error_family = $8, finished_at = NOW(), updated_at = NOW() WHERE id = $1 AND status IN ('queued','running')")
            .bind(run_id).bind(status).bind(exit_code).bind(&error).bind(&output.stdout).bind(&result_json).bind(outcome.error_code).bind(outcome.error_family).execute(&self.pool).await
        {
            tracing::error!(%run_id, %error, "failed to persist heartbeat run final status");
        }

        if let Some(session_id) = outcome.session_id.as_deref().filter(|value| !value.trim().is_empty()) {
            if let Err(error) = sqlx::query(
                "UPDATE agent_runtime_states SET session_id = $2, last_run_id = $3, updated_at = NOW() WHERE agent_id = $1",
            )
            .bind(agent_id)
            .bind(session_id)
            .bind(run_id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%run_id, %agent_id, %error, "failed to persist adapter session id");
            }
        }

        // Update agent runtime state with token usage and cost (incremental)
        let has_token_usage = outcome.input_tokens > 0 || outcome.output_tokens > 0 || outcome.cached_input_tokens > 0;
        if has_token_usage || outcome.cost_usd.is_some() {
            let cost_cents = outcome.cost_usd.map(|v| (v * 100.0) as i64).unwrap_or(0);
            
            let update_result = sqlx::query(
                "UPDATE agent_runtime_states 
                 SET total_input_tokens = total_input_tokens + $2,
                     total_output_tokens = total_output_tokens + $3,
                     total_cached_input_tokens = total_cached_input_tokens + $4,
                     total_cost_cents = total_cost_cents + $5,
                     last_run_id = $6,
                     last_run_status = $7,
                     updated_at = NOW()
                 WHERE agent_id = $1"
            )
            .bind(agent_id)
            .bind(outcome.input_tokens)
            .bind(outcome.output_tokens)
            .bind(outcome.cached_input_tokens)
            .bind(cost_cents)
            .bind(run_id)
            .bind(status)
            .execute(&self.pool)
            .await;
            
            if let Err(error) = update_result {
                tracing::warn!(%run_id, %agent_id, %error, "failed to update agent runtime state with token usage");
            } else {
                tracing::debug!(
                    %run_id, 
                    %agent_id, 
                    input_tokens = outcome.input_tokens,
                    output_tokens = outcome.output_tokens,
                    cached_input_tokens = outcome.cached_input_tokens,
                cost_cents,
                    "updated agent runtime state with token usage and cost"
                );
            }
            
            
            // Create cost event via CostService for ledger tracking
            if let Some(cost_service) = &self.cost_service {
                if cost_cents > 0 || has_token_usage {
                    let create_event_input = crate::CreateCostEventInput {
                        agent_id,
                        heartbeat_run_id: Some(run_id),
                        issue_id: Some(issue_id),
                        project_id: None,
                        goal_id: None,
                        billing_code: None,
                        provider: outcome.provider.clone().unwrap_or_else(|| "unknown".to_string()),
                        model: outcome.model.clone().unwrap_or_else(|| "unknown".to_string()),
                        biller: "anthropic".to_string(), // TODO: resolve biller from adapter config
                        billing_type: "api".to_string(), // TODO: resolve billing_type from adapter config
                        input_tokens: outcome.input_tokens as i32,
                        cached_input_tokens: outcome.cached_input_tokens as i32,
                        output_tokens: outcome.output_tokens as i32,
                        cost_cents: cost_cents as i32,
                        occurred_at: None, // Use current time
                    };
                    
                    match cost_service.create_event(company_id, create_event_input).await {
                        Ok(event) => {
                            tracing::debug!(
                                %run_id,
                                event_id = %event.id,
                                cost_cents,
                                "created cost event for heartbeat run"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(
                                %run_id,
                                %agent_id,
                                ?error,
                                "failed to create cost event"
                            );
                        }
                    }
                }
            }
        }

        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.status",
            serde_json::json!({
                "runId": run_id,
                "agentId": agent_id,
                "issueId": issue_id,
                "status": status,
                "exitCode": exit_code,
                "error": error,
            }),
        ).await;
        let issue_status = if status == "succeeded" { "done" } else { "todo" };
        let _ = sqlx::query(
            "UPDATE issues SET status = $2::issue_status, checkout_run_id = NULL, execution_run_id = NULL, execution_locked_at = NULL, execution_agent_name_key = NULL, completed_at = CASE WHEN $2 = 'done' THEN NOW() ELSE NULL END, updated_at = NOW() WHERE id = $1 AND company_id = $3 AND execution_run_id = $4",
        )
        .bind(issue_id)
        .bind(issue_status)
        .bind(company_id)
        .bind(run_id)
            .execute(&self.pool)
            .await;
        self.refresh_continuation_summary(issue_id, run_id, agent_id, status, error.as_deref(), &output.stdout).await;
        let _ = sqlx::query("UPDATE agent_wakeup_requests SET status = 'completed', updated_at = NOW() WHERE company_id = $1 AND agent_id = $2 AND status IN ('queued','dispatched','running') AND payload->>'issueId' = $3")
            .bind(company_id).bind(agent_id).bind(issue_id.to_string()).execute(&self.pool).await;
        let _ = sqlx::query("UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW() WHERE run_id = $1 AND revoked_at IS NULL")
            .bind(run_id).execute(&self.pool).await;
        let _ = sqlx::query("UPDATE agents SET status = 'idle', updated_at = NOW() WHERE id = $1 AND status = 'running'")
            .bind(agent_id).execute(&self.pool).await;
        self.children.lock().await.remove(&run_id);
    }

    async fn run_command(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<AdapterCommandOutput, String> {
        let agent = self.load_agent(agent_id).await.map_err(|e| e.to_string())?;
        let issue = sqlx::query("SELECT title, description FROM issues WHERE id = $1")
            .bind(issue_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        let title: String = issue
            .as_ref()
            .and_then(|r| r.try_get("title").ok())
            .unwrap_or_default();
        let description: Option<String> =
            issue.as_ref().and_then(|r| r.try_get("description").ok());
        let default_prompt = format!(
            "Task: {title}\n{}\n\nReport the work performed and final result.",
            description.as_deref().unwrap_or_default()
        );
        // 获取数据库配置
        let db_config = agent.adapter_config.0.clone();
        let adapter = agent.adapter_type.as_str();
        
        // 加载默认配置并合并
        let default_config = load_default_adapter_config(adapter);
        let cfg = merge_adapter_config(db_config, default_config);
        let prompt = cfg
            .get("promptTemplate")
            .or_else(|| cfg.get("prompt_template"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|template| {
                template
                    .replace("{{issue.title}}", &title)
                    .replace("{{issue.description}}", description.as_deref().unwrap_or(""))
                    .replace("{{issueId}}", &issue_id.to_string())
            })
            .unwrap_or(default_prompt);
        let configured_model = cfg
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty());
        if let Some(api_key) = cfg
            .get("apiKey")
            .or_else(|| cfg.get("api_key"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
        {
            let endpoint = cfg
                .get("endpoint")
                .or_else(|| cfg.get("baseUrl"))
                .and_then(|v| v.as_str());
            let model = if adapter == "claude_local" {
                configured_model.ok_or_else(|| "claude_local API execution requires adapter config model".to_string())?
            } else {
                cfg.get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("gpt-4o-mini")
            };
            let url = endpoint.unwrap_or(if adapter == "claude_local" {
                "https://api.anthropic.com/v1/messages"
            } else {
                "https://api.openai.com/v1/chat/completions"
            });
            let client = reqwest::Client::new();
            let response = if adapter == "claude_local" {
                client.post(url).header("x-api-key", api_key).header("anthropic-version", "2023-06-01")
                    .json(&serde_json::json!({"model": model, "max_tokens": cfg.get("maxTokens").and_then(|v| v.as_u64()).unwrap_or(4096), "messages": [{"role":"user","content":prompt}]})).send().await
            } else {
                client.post(url).bearer_auth(api_key)
                    .json(&serde_json::json!({"model": model, "messages": [{"role":"user","content":prompt}]})).send().await
            }.map_err(|e| e.to_string())?;
            let status = response.status();
            let body = response.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                return Err(format!("LLM request failed with HTTP {status}: {body}"));
            }
            return Ok(AdapterCommandOutput { exit_code: 0, stdout: body, stderr: String::new() });
        }
        let command = cfg
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or(match adapter {
                "claude_local" => "claude",
                "codex_local" => "codex",
                "opencode" => "opencode",
                _ => "sh",
            });
        let mut args: Vec<String> = cfg
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let custom_args = !args.is_empty();
        if args.is_empty() {
            args = match adapter {
                "process" => vec![
                    "-c".into(),
                    format!("printf '%s' '{}'", prompt.replace('\'', "'\\''")),
                ],
                "codex_local" => vec!["exec".into(), prompt.clone()],
                "claude_local" => vec![
                    "--print".into(),
                    "-".into(),
                    "--output-format".into(),
                    "stream-json".into(),
                    "--verbose".into(),
                ],
                _ => vec!["-p".into(), prompt.clone()],
            };
            if adapter == "codex_local" {
                let model = configured_model.unwrap_or("deepseek-v4-flash");
                args.splice(1..1, ["--model".to_string(), model.to_string()]);
            }
        }
        if adapter == "claude_local" {
            let skip_permissions = cfg
                .get("dangerouslySkipPermissions")
                .or_else(|| cfg.get("dangerously_skip_permissions"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if skip_permissions && !args.iter().any(|arg| arg == "--dangerously-skip-permissions") {
                args.push("--dangerously-skip-permissions".into());
            }
            if let Some(max_turns) = cfg
                .get("maxTurnsPerRun")
                .or_else(|| cfg.get("max_turns_per_run"))
                .and_then(|v| v.as_u64())
                .filter(|value| *value > 0)
            {
                args.extend(["--max-turns".into(), max_turns.to_string()]);
            }
            if let Some(effort) = cfg
                .get("effort")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                args.extend(["--effort".into(), effort.to_owned()]);
            }
            if cfg.get("chrome").and_then(|value| value.as_bool()).unwrap_or(false) {
                args.push("--chrome".into());
            }
            if let Some(instructions_path) = cfg
                .get("instructionsFilePath")
                .or_else(|| cfg.get("instructions_file_path"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                args.extend(["--append-system-prompt-file".into(), instructions_path.to_owned()]);
            }
            if let Some(system_prompt) = cfg
                .get("systemPrompt")
                .or_else(|| cfg.get("system_prompt"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                args.extend(["--system-prompt".into(), system_prompt.to_owned()]);
            }
            if let Some(append_system_prompt) = cfg
                .get("appendSystemPrompt")
                .or_else(|| cfg.get("append_system_prompt"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
            {
                args.extend(["--append-system-prompt".into(), append_system_prompt.to_owned()]);
            }
            if cfg
                .get("excludeDynamicSystemPromptSections")
                .or_else(|| cfg.get("exclude_dynamic_system_prompt_sections"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                args.push("--exclude-dynamic-system-prompt-sections".into());
            }
            if cfg
                .get("strictMcpConfig")
                .or_else(|| cfg.get("strict_mcp_config"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                args.push("--strict-mcp-config".into());
            }
            if let Some(extra_args) = cfg.get("extraArgs").or_else(|| cfg.get("extra_args")) {
                if let Some(extra_args) = extra_args.as_array() {
                    args.extend(extra_args.iter().filter_map(|value| value.as_str().map(str::to_owned)));
                }
            }
        }
        if adapter == "codex_local" {
            let skip_permissions = cfg
                .get("dangerouslySkipPermissions")
                .or_else(|| cfg.get("dangerously_skip_permissions"))
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            if skip_permissions
                && !args
                    .iter()
                    .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
            {
                args.push("--dangerously-bypass-approvals-and-sandbox".into());
            }
        }
        if adapter == "claude_local" && !custom_args {
            let persisted_session: Option<String> = sqlx::query_scalar(
                "SELECT session_id FROM agent_runtime_states WHERE agent_id = $1",
            )
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| format!("failed to load Claude session: {error}"))?;
            if let Some(session_id) = valid_claude_resume_session(persisted_session.as_deref()) {
                args.extend(["--resume".to_string(), session_id]);
            }
        }
        let mut cmd = Command::new(command);
        let gateway_token = format!("ptg_{}", Uuid::new_v4().simple());
        let mut token_hasher = Sha256::new();
        token_hasher.update(gateway_token.as_bytes());
        let gateway_token_hash = hex::encode(token_hasher.finalize());
        let gateway_url = cfg
            .get("toolGatewayUrl")
            .or_else(|| cfg.get("tool_gateway_url"))
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .or_else(|| std::env::var("PAPERCLIP_TOOL_GATEWAY_URL").ok())
            .or_else(|| {
                std::env::var("PAPERCLIP_API_URL").ok().map(|value| {
                    let base = value.trim_end_matches('/');
                    if base.ends_with("/api") {
                        format!("{base}/tool-gateway")
                    } else {
                        format!("{base}/api/tool-gateway")
                    }
                })
            })
            .unwrap_or_else(|| "http://127.0.0.1:3100/api/tool-gateway".to_string());
        let _ = sqlx::query(
            "INSERT INTO tool_gateway_sessions (company_id, agent_id, run_id, issue_id, token_hash, expires_at)
             VALUES ($1,$2,$3,$4,$5,NOW() + INTERVAL '30 minutes')",
        )
        .bind(agent.company_id)
        .bind(agent_id)
        .bind(run_id)
        .bind(issue_id)
        .bind(gateway_token_hash)
        .execute(&self.pool)
        .await;
        // Make the per-run gateway discoverable by the local CLIs. Environment
        // variables alone are not consumed by Codex/Claude as MCP servers.
        let mcp_url = format!("{}/mcp", gateway_url.trim_end_matches('/'));
        match adapter {
            "claude_local" => {
                let config = serde_json::json!({
                    "mcpServers": {
                        "paperclip": {
                            "type": "http",
                            "url": mcp_url,
                            "headers": {"Authorization": format!("Bearer {gateway_token}")}
                        }
                    }
                });
                args.splice(0..0, ["--mcp-config".to_string(), config.to_string()]);
            }
            "codex_local" => {
                args.splice(0..0, [
                    "-c".to_string(),
                    format!("mcp_servers.paperclip.url={mcp_url:?}"),
                    "-c".to_string(),
                    // Codex 0.144.x reads `env_http_headers` for Streamable
                    // HTTP bearer auth. `bearer_token_env_var` is accepted by
                    // its config printer but is not emitted on requests in
                    // this CLI version. Keep the value in a dedicated env var
                    // whose value already contains the required scheme.
                    "mcp_servers.paperclip.env_http_headers.Authorization=\"PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION\"".to_string(),
                ]);
            }
            _ => {}
        }
        // Do not accidentally inherit Claude Code's OpenAI compatibility mode
        // or another provider override from the shell that launched
        // parrot-server. Explicit per-agent env values remain authoritative
        // below. This is important for local Claude runs: otherwise a
        // developer shell's ANTHROPIC_BASE_URL/LLM_* silently changes the
        // provider used by every agent.
        if adapter == "claude_local" {
            let explicit_env = cfg.get("env").and_then(|v| v.as_object());
            isolate_claude_provider_environment(&mut cmd, explicit_env);
        }
        let stdin_prompt = adapter == "claude_local" && !custom_args;
        let timeout_sec = cfg
            .get("timeoutSec")
            .or_else(|| cfg.get("timeout_sec"))
            .and_then(|v| v.as_u64())
            .filter(|value| *value > 0);
        // 处理工作目录：如果未配置，则创建默认目录
        let working_dir = cfg.get("cwd").and_then(|value| value.as_str());
        let effective_cwd = if let Some(cwd) = working_dir {
            cwd.to_string()
        } else {
            // 查询 company name
            let company_name: String = sqlx::query_scalar("SELECT name FROM companies WHERE id = $1")
                .bind(company_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or_else(|_| company_id.to_string());
            
            // 创建默认工作目录: ~/.parrot-agent/<company_name>/
            let home_dir = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            
            // 规范化 company name 作为目录名（移除特殊字符）
            let safe_company_name = company_name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect::<String>();
            
            let default_dir = format!("{}/.parrot-agent/{}", home_dir, safe_company_name);
            
            // 创建目录（如果不存在）
            if let Err(e) = std::fs::create_dir_all(&default_dir) {
                tracing::error!(
                    company_id = %company_id,
                    company_name = %company_name,
                    default_dir = %default_dir,
                    error = %e,
                    "failed to create default working directory"
                );
                return Err(format!("failed to create default working directory: {}", e));
            }
            
            tracing::warn!(
                run_id = %run_id,
                agent_id = %agent_id,
                company_id = %company_id,
                company_name = %company_name,
                default_dir = %default_dir,
                "no working directory configured, using default directory ~/.parrot-agent/{}/",
                safe_company_name
            );
            
            // 更新 agent 配置中的 cwd（持久化到数据库）
            let mut updated_config = agent.adapter_config.0.clone();
            updated_config.as_object_mut().map(|obj| {
                obj.insert("cwd".to_string(), serde_json::Value::String(default_dir.clone()));
            });
            
            let _ = sqlx::query("UPDATE agents SET adapter_config = $1, updated_at = NOW() WHERE id = $2")
                .bind(serde_json::to_value(&updated_config).unwrap_or(serde_json::json!({})))
                .bind(agent_id)
                .execute(&self.pool)
                .await;
            
            default_dir
        };
        
        let shell_command_text = shell_command(
            command,
            &args,
            Some(&effective_cwd),
            stdin_prompt.then_some(prompt.as_str()),
        );
        let configured_env_keys = cfg
            .get("env")
            .and_then(|value| value.as_object())
            .map(|env| env.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let logged_shell_command = redact_gateway_token(&shell_command_text, &gateway_token);
        let logged_argv = std::iter::once(command.to_owned())
            .chain(args.iter().cloned())
            .map(|value| redact_gateway_token(&value, &gateway_token))
            .collect::<Vec<_>>();
        // 构造包含所有环境变量的完整 shell 命令
        let mut full_cmd_with_env = String::new();
        full_cmd_with_env.push_str(&format!("PAPERCLIP_RUN_ID={} ", shell_quote(&run_id.to_string())));
        full_cmd_with_env.push_str(&format!("PAPERCLIP_AGENT_ID={} ", shell_quote(&agent_id.to_string())));
        full_cmd_with_env.push_str(&format!("PAPERCLIP_TOOL_GATEWAY_URL={} ", shell_quote(&gateway_url)));
        full_cmd_with_env.push_str(&format!("PAPERCLIP_TOOL_GATEWAY_TOKEN={} ", shell_quote(&gateway_token)));
        full_cmd_with_env.push_str(&format!("PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION={} ", shell_quote(&format!("Bearer {}", gateway_token))));
        if let Some(env) = cfg.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env {
                if let Some(s) = v.as_str() {
                    let resolved_value = resolve_env_value(s);
                    full_cmd_with_env.push_str(&format!("{}={} ", k, shell_quote(&resolved_value)));
                }
            }
        }
        full_cmd_with_env.push_str(&shell_command_text);
        
        tracing::info!(
            run_id = %run_id,
            agent_id = %agent_id,
            issue_id = %issue_id,
            adapter,
            shell_command = %logged_shell_command,
            argv = ?logged_argv,
            working_dir = %effective_cwd,
            configured_env_keys = ?configured_env_keys,
            full_command_with_env = %full_cmd_with_env,
            stdin_prompt,
            prompt_bytes = prompt.len(),
            "starting local adapter process"
        );
        cmd.args(args)
            .stdin(if stdin_prompt {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .env("PAPERCLIP_RUN_ID", run_id.to_string())
            .env("PAPERCLIP_AGENT_ID", agent_id.to_string())
            .env("PAPERCLIP_TOOL_GATEWAY_URL", gateway_url)
            .env("PAPERCLIP_TOOL_GATEWAY_TOKEN", &gateway_token)
            .env(
                "PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION",
                format!("Bearer {gateway_token}"),
            )
            .current_dir(&effective_cwd);
        if let Some(env) = cfg.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env {
                if let Some(s) = v.as_str() {
                    let resolved_value = resolve_env_value(s);
                    cmd.env(k, resolved_value);
                }
            }
        }
        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        sqlx::query("UPDATE heartbeat_runs SET status = 'running', started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND status = 'queued'").bind(run_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.status",
            serde_json::json!({
                "runId": run_id,
                "agentId": agent_id,
                "issueId": issue_id,
                "status": "running",
            }),
        ).await;
        let child_ref = Arc::new(Mutex::new(child));
        self.children.lock().await.insert(run_id, child_ref.clone());
        let mut child = child_ref.lock().await;
        if stdin_prompt {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| format!("failed to write Claude prompt: {e}"))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|e| format!("failed to close Claude stdin: {e}"))?;
            }
        }
        let mut stdout = child.stdout.take().ok_or("stdout unavailable")?;
        let mut stderr = child.stderr.take().ok_or("stderr unavailable")?;
        let sequence = Arc::new(AtomicU64::new(0));
        let stdout_service = self.sse_service.clone();
        let stderr_service = self.sse_service.clone();
        let stdout_sequence = sequence.clone();
        let stderr_sequence = sequence.clone();
        let stdout_reader = async move {
            let mut captured = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let read = stdout.read(&mut buffer).await.map_err(|e| e.to_string())?;
                if read == 0 { break; }
                captured.extend_from_slice(&buffer[..read]);
                let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
                let seq = stdout_sequence.fetch_add(1, Ordering::Relaxed) + 1;
                publish_live_event(
                    &stdout_service,
                    company_id,
                    "heartbeat.run.log",
                    serde_json::json!({
                        "runId": run_id,
                        "agentId": agent_id,
                        "issueId": issue_id,
                        "seq": seq,
                        "stream": "stdout",
                        "chunk": chunk,
                        "ts": Utc::now(),
                    }),
                ).await;
            }
            Ok::<Vec<u8>, String>(captured)
        };
        let stderr_reader = async move {
            let mut captured = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let read = stderr.read(&mut buffer).await.map_err(|e| e.to_string())?;
                if read == 0 { break; }
                captured.extend_from_slice(&buffer[..read]);
                let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
                let seq = stderr_sequence.fetch_add(1, Ordering::Relaxed) + 1;
                publish_live_event(
                    &stderr_service,
                    company_id,
                    "heartbeat.run.log",
                    serde_json::json!({
                        "runId": run_id,
                        "agentId": agent_id,
                        "issueId": issue_id,
                        "seq": seq,
                        "stream": "stderr",
                        "chunk": chunk,
                        "ts": Utc::now(),
                    }),
                ).await;
            }
            Ok::<Vec<u8>, String>(captured)
        };
        let wait_result = timeout(
            timeout_sec.map(Duration::from_secs).unwrap_or(Duration::from_secs(u64::MAX)),
            async {
                let (stdout_result, stderr_result, status) =
                    tokio::join!(stdout_reader, stderr_reader, child.wait());
                Ok::<(Vec<u8>, Vec<u8>, std::process::ExitStatus), String>(
                    (stdout_result?, stderr_result?, status.map_err(|e| e.to_string())?),
                )
            },
        )
        .await;
        let status = match wait_result {
            Ok(status) => status?,
            Err(_) => {
                let _ = child.kill().await;
                return Err(format!(
                    "adapter timed out after {} seconds",
                    timeout_sec.unwrap_or(0),
                ));
            }
        };
        let (out, err, status) = status;
        Ok(AdapterCommandOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out).to_string(),
            stderr: String::from_utf8_lossy(&err).to_string(),
        })
    }
}

impl DefaultHeartbeatService {
    async fn wakeup_with_context(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: HeartbeatWakeupOptions,
        idempotency_row_id: Option<Uuid>,
    ) -> Result<(), HeartbeatError> {
        let _agent = self.load_agent(agent_id).await?;
        let active_run: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2 AND status IN ('queued','running') AND (context_snapshot->>'issueId' = $3 OR context_snapshot->>'taskId' = $3) ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        if active_run.is_some() {
            return Ok(());
        }

        let mut context = options
            .context_snapshot
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = context.as_object_mut() {
            object.insert("issueId".to_string(), serde_json::json!(issue_id));
        } else {
            context = serde_json::json!({ "issueId": issue_id });
        }
        let run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot, responsible_user_id)
             SELECT $1, $2, $3, 'queued'::heartbeat_run_status, $4, i.responsible_user_id
             FROM issues i WHERE i.id = $5
             UNION ALL
             SELECT $1, $2, $3, 'queued'::heartbeat_run_status, $4, NULL::uuid
             WHERE NOT EXISTS (SELECT 1 FROM issues WHERE id = $5)
             LIMIT 1
             RETURNING id",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind("on_demand")
        .bind(&context)
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.queued",
            serde_json::json!({
                "runId": run_id,
                "agentId": agent_id,
                "issueId": issue_id,
                "status": "queued",
                "invocationSource": "on_demand",
            }),
        )
        .await;

        let mut payload = options
            .payload
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = payload.as_object_mut() {
            object.insert("issueId".to_string(), serde_json::json!(issue_id));
            object.insert("runId".to_string(), serde_json::json!(run_id));
        } else {
            payload = serde_json::json!({ "issueId": issue_id, "runId": run_id });
        }
        if let Some(request_id) = idempotency_row_id {
            sqlx::query(
                "UPDATE agent_wakeup_requests
                 SET status = 'dispatched', payload = $2, source = $3, trigger_detail = $4,
                     reason = $5, requested_by_actor_type = $6, requested_by_actor_id = $7,
                     updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(request_id)
            .bind(&payload)
            .bind(options.source.as_deref())
            .bind(options.trigger_detail.as_deref())
            .bind(options.reason.as_deref())
            .bind(options.requested_by_actor_type.as_deref())
            .bind(options.requested_by_actor_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        } else {
            sqlx::query(
                "INSERT INTO agent_wakeup_requests
                 (company_id, agent_id, status, payload, source, trigger_detail, reason,
                  requested_by_actor_type, requested_by_actor_id, updated_at)
                 VALUES ($1,$2,'dispatched',$3,$4,$5,$6,$7,$8,NOW())",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(&payload)
            .bind(options.source.as_deref())
            .bind(options.trigger_detail.as_deref())
            .bind(options.reason.as_deref())
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        }
        sqlx::query("UPDATE issues SET assignee_agent_id = $2, assignee_user_id = NULL, status = CASE WHEN status IN ('todo','backlog') THEN 'in_progress'::issue_status ELSE status END, checkout_run_id = $3, execution_run_id = $3, started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND company_id = $4 AND (assignee_agent_id IS NULL OR assignee_agent_id = $2) AND status NOT IN ('done','cancelled')")
            .bind(issue_id).bind(agent_id).bind(run_id).bind(company_id)
            .execute(&self.pool).await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        sqlx::query("UPDATE agents SET status = 'running', updated_at = NOW() WHERE id = $1")
            .bind(agent_id).execute(&self.pool).await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let service = self.clone_for_task();
        tokio::spawn(async move { service.execute_run(run_id, agent_id, issue_id, company_id).await; });
        Ok(())
    }

    /// Mark an agent wakeup as `skipped` for an external gate (budget hard-stop),
    /// reusing the idempotency row when present and otherwise inserting a fresh one.
    /// Mirrors the throttle-skip path in `wakeup_with_options` so a blocked wakeup is
    /// recorded consistently and is not retried by idempotent callers.
    async fn mark_wakeup_skipped(
        &self,
        idempotency_row_id: Option<Uuid>,
        company_id: Uuid,
        agent_id: Uuid,
        payload: &Value,
        options: &HeartbeatWakeupOptions,
        reason: &str,
    ) -> Result<(), HeartbeatError> {
        if let Some(row_id) = idempotency_row_id {
            sqlx::query(
                "UPDATE agent_wakeup_requests
                 SET status = 'skipped', payload = $2, reason = $3,
                     finished_at = NOW(), updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(row_id)
            .bind(payload)
            .bind(reason)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        } else {
            sqlx::query(
                "INSERT INTO agent_wakeup_requests
                 (company_id, agent_id, status, payload, source, trigger_detail,
                  reason, requested_by_actor_type, requested_by_actor_id,
                  idempotency_key, finished_at, updated_at)
                 VALUES ($1, $2, 'skipped', $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(payload)
            .bind(options.source.as_deref())
            .bind(options.trigger_detail.as_deref())
            .bind(reason)
            .bind(options.requested_by_actor_type.as_deref())
            .bind(options.requested_by_actor_id)
            .bind(options.idempotency_key.as_deref())
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait]
impl HeartbeatService for DefaultHeartbeatService {
    async fn wakeup_with_options(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: HeartbeatWakeupOptions,
    ) -> Result<(), HeartbeatError> {
        let idempotency_row_id = if let Some(idempotency_key) = options.idempotency_key.as_deref() {
            let row = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO agent_wakeup_requests
                 (company_id, agent_id, status, payload, source, trigger_detail, reason,
                  requested_by_actor_type, requested_by_actor_id, idempotency_key, updated_at)
                 VALUES ($1, $2, 'queued', $3, $4, $5, $6, $7, $8, $9, NOW())
                 ON CONFLICT (company_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING
                 RETURNING id",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(serde_json::json!({ "issueId": issue_id }))
            .bind(options.source.as_deref())
            .bind(options.trigger_detail.as_deref())
            .bind(options.reason.as_deref())
            .bind(options.requested_by_actor_type.as_deref())
            .bind(options.requested_by_actor_id)
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            row
        } else {
            None
        };

        // ── Budget hard-stop enforcement ──────────────────────────────────────
        // Hard-stop is the verifiable "硬停止" contract: when a company/agent budget
        // hard-stop is reached (open hard-stop incident / scope paused for budget /
        // observed spend >= policy amount with hard_stop_enabled), no new work may
        // start. get_invocation_block already implements the full scoped check
        // (company → agent → project). It was previously only reachable via the
        // read-only GET /budgets/invocation-block route and never consulted here, so
        // a hard-stop incident did NOT actually block runs. Wire it through the single
        // wakeup funnel so every wakeup source (heartbeat, issue assignment, scheduled-
        // retry promotion, recovery) honors the stop. Fail OPEN on billing-service
        // errors: a billing DB hiccup must not deadlock every agent wakeup.
        if let Some(budget_service) = &self.budget_service {
            match budget_service
                .get_invocation_block(company_id, agent_id, None)
                .await
            {
                Ok(Some(block)) => {
                    let payload = serde_json::json!({
                        "issueId": issue_id,
                        "heartbeatSkip": {
                            "reason": "budget_hard_stop",
                            "scopeType": block.scope_type,
                            "scopeId": block.scope_id,
                            "scopeName": block.scope_name,
                            "detail": block.reason,
                            "requestedReason": options.reason,
                        }
                    });
                    self.mark_wakeup_skipped(idempotency_row_id, company_id, agent_id, &payload, &options, "budget_hard_stop")
                        .await?;
                    return Ok(());
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        company_id = %company_id,
                        agent_id = %agent_id,
                        error = %e,
                        "budget invocation-block check failed; allowing wakeup (fail-open)"
                    );
                }
            }
        }


        let throttle_candidate = matches!(
            options.reason.as_deref(),
            None
                | Some(
                    "issue_assigned"
                        | "issue_continuation_needed"
                        | "issue_assignment_recovery"
                        | "issue_graph_liveness_backstop"
                )
        );

        if throttle_candidate {
            const LOOKBACK_HOURS: i64 = 6;
            const SAMPLE_LIMIT: i64 = 8;
            const THRESHOLD: i64 = 2;
            const BASE_COOLDOWN_SECONDS: i64 = 120;
            const MAX_COOLDOWN_SECONDS: i64 = 30 * 60;
            const PROGRESS_EVENTS: &[&str] = &[
                "issue.updated",
                "issue.comment_added",
                "issue.created",
                "issue.child_created",
                "issue.assigned",
                "issue.released",
                "issue.blockers_updated",
                "issue.document_upserted",
                "issue.document_updated",
                "issue.work_product_created",
                "issue.work_product_updated",
                "issue.thread_interaction_created",
                "issue.monitor_scheduled",
                "issue.approval_linked",
            ];
            let recent_runs = sqlx::query(
                "SELECT id, status::text AS status, finished_at
                 FROM heartbeat_runs
                 WHERE company_id = $1 AND agent_id = $2
                   AND finished_at IS NOT NULL
                   AND finished_at >= NOW() - ($3 * INTERVAL '1 hour')
                   AND (context_snapshot->>'issueId' = $4 OR context_snapshot->>'taskId' = $4)
                 ORDER BY finished_at DESC
                 LIMIT $5",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(LOOKBACK_HOURS)
            .bind(issue_id.to_string())
            .bind(SAMPLE_LIMIT)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;

            let mut no_progress_streak = 0i64;
            let mut latest_finished_at = None;
            for run in recent_runs {
                let status: String = run
                    .try_get("status")
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                let finished_at: DateTime<Utc> = run
                    .try_get("finished_at")
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                if status != "succeeded" {
                    break;
                }
                if latest_finished_at.is_none() {
                    latest_finished_at = Some(finished_at);
                }
                let run_id: Uuid = run
                    .try_get("id")
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                let made_progress: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                       SELECT 1 FROM activity_logs
                       WHERE company_id = $1 AND resource_type = 'issue'
                         AND resource_id = $2 AND run_id = $3
                         AND event_type = ANY($4)
                     )",
                )
                .bind(company_id)
                .bind(issue_id)
                .bind(run_id)
                .bind(PROGRESS_EVENTS)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                if made_progress {
                    break;
                }
                no_progress_streak += 1;
            }

            if no_progress_streak >= THRESHOLD {
                if let Some(last_finished_at) = latest_finished_at {
                    let doublings = (no_progress_streak - THRESHOLD).min(16) as u32;
                    let cooldown_seconds = (BASE_COOLDOWN_SECONDS * 2_i64.pow(doublings))
                        .min(MAX_COOLDOWN_SECONDS);
                    let next_allowed_at = last_finished_at
                        + chrono::Duration::seconds(cooldown_seconds);
                    if Utc::now() < next_allowed_at {
                        let payload = serde_json::json!({
                            "issueId": issue_id,
                            "heartbeatSkip": {
                                "reason": "issue_rewake_throttled",
                                "requestedReason": options.reason,
                                "noProgressStreak": no_progress_streak,
                                "cooldownSeconds": cooldown_seconds,
                                "lastRunFinishedAt": last_finished_at,
                                "nextAllowedAt": next_allowed_at,
                            }
                        });
                        if let Some(idempotency_row_id) = idempotency_row_id.as_ref() {
                            sqlx::query(
                                "UPDATE agent_wakeup_requests
                                 SET status = 'skipped', payload = $2, reason = 'issue_rewake_throttled',
                                     finished_at = NOW(), updated_at = NOW()
                                 WHERE id = $1",
                            )
                            .bind(idempotency_row_id)
                            .bind(&payload)
                            .execute(&self.pool)
                            .await
                            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                        } else {
                            sqlx::query(
                            "INSERT INTO agent_wakeup_requests
                             (company_id, agent_id, status, payload, source, trigger_detail,
                              reason, requested_by_actor_type, requested_by_actor_id,
                              idempotency_key, finished_at, updated_at)
                             VALUES ($1, $2, 'skipped', $3, $4, $5, 'issue_rewake_throttled',
                                     $6, $7, $8, NOW(), NOW())",
                        )
                        .bind(company_id)
                        .bind(agent_id)
                        .bind(payload)
                        .bind(options.source.as_deref())
                        .bind(options.trigger_detail.as_deref())
                        .bind(options.requested_by_actor_type.as_deref())
                        .bind(options.requested_by_actor_id)
                        .bind(options.idempotency_key.as_deref())
                        .execute(&self.pool)
                        .await
                        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                        }
                        return Ok(());
                    }
                }
            }
        }

        let result = self
            .wakeup_with_context(agent_id, issue_id, company_id, options, idempotency_row_id)
            .await;
        if let (Some(idempotency_row_id), Err(error)) = (idempotency_row_id, &result) {
            let _ = sqlx::query(
                "UPDATE agent_wakeup_requests
                 SET status = 'failed', error = $2, finished_at = NOW(), updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(idempotency_row_id)
            .bind(error.to_string())
            .execute(&self.pool)
            .await;
        }
        result
    }

    async fn wakeup(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<(), HeartbeatError> {
        self.wakeup_with_context(
            agent_id,
            issue_id,
            company_id,
            HeartbeatWakeupOptions::default(),
            None,
        )
        .await
    }


    async fn cancel_run(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<(), HeartbeatError> {
        let run: Option<Uuid> = sqlx::query_scalar("SELECT id FROM heartbeat_runs WHERE company_id=$1 AND agent_id=$2 AND status IN ('queued','running','scheduled_retry') AND (context_snapshot->>'issueId'=$3 OR context_snapshot->>'taskId'=$3) ORDER BY created_at DESC LIMIT 1")
            .bind(company_id).bind(agent_id).bind(issue_id.to_string()).fetch_optional(&self.pool).await.map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
        
        if let Some(run_id) = run {
            // 1. 终止子进程（优雅终止）
            if let Some(child) = self.children.lock().await.remove(&run_id) {
                // 从 agent 配置读取 grace period（默认 2 秒）
                let grace_sec = {
                    let agent_result = self.load_agent(agent_id).await;
                    agent_result.ok().and_then(|agent| {
                        agent.adapter_config.0.get("graceSec")
                            .and_then(|v| v.as_u64())
                            .map(|v| v.max(1).min(30)) // 1-30秒范围
                    }).unwrap_or(2) // 默认 2 秒
                };
                let grace_ms = grace_sec * 1000;
                
                tracing::debug!(
                    run_id = %run_id,
                    agent_id = %agent_id,
                    grace_sec = %grace_sec,
                    "terminating process with grace period"
                );
                
                if let Err(e) = self.terminate_process_gracefully(child, grace_ms).await {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %e,
                        "failed to terminate process gracefully"
                    );
                }
            }
            
            // 2. 更新 run 状态为 cancelled
            sqlx::query("UPDATE heartbeat_runs SET status='cancelled', error=$2, finished_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(run_id)
                .bind(reason)
                .execute(&self.pool)
                .await
                .map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
            
            // 3. 释放 issue execution lock (关键修复！)
            sqlx::query(
                "UPDATE issues 
                 SET checkout_run_id = NULL, 
                     execution_run_id = NULL, 
                     execution_locked_at = NULL, 
                     execution_agent_name_key = NULL, 
                     updated_at = NOW() 
                 WHERE id = $1 
                   AND company_id = $2 
                   AND execution_run_id = $3"
            )
            .bind(issue_id)
            .bind(company_id)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
            
            // 4. 撤销 tool gateway session
            let _ = sqlx::query("UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW() WHERE run_id = $1 AND revoked_at IS NULL")
                .bind(run_id)
                .execute(&self.pool)
                .await;
            
            tracing::info!(
                run_id = %run_id,
                agent_id = %agent_id,
                issue_id = %issue_id,
                company_id = %company_id,
                reason = %reason,
                "cancelled heartbeat run and released issue execution lock"
            );
        }
        
        // 5. 取消相关的 wakeup requests
        sqlx::query("UPDATE agent_wakeup_requests SET status='cancelled', updated_at=NOW() WHERE company_id=$1 AND agent_id=$2 AND status IN ('queued','dispatched','running') AND payload->>'issueId'=$3")
            .bind(company_id)
            .bind(agent_id)
            .bind(issue_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
        
        // 6. 启动队列中的下一个 run
        if let Err(e) = self.start_next_queued_run_for_agent(agent_id).await {
            tracing::error!(%agent_id, error = %e, "failed to start next queued run after cancel");
        }
        
        Ok(())
    }

    async fn cancel_scheduled_retry(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<bool, HeartbeatError> {
        let result = sqlx::query(
            "UPDATE heartbeat_runs
             SET status = 'cancelled', error = $4, finished_at = NOW(), updated_at = NOW()
             WHERE id = (
                 SELECT id FROM heartbeat_runs
                 WHERE company_id = $1
                   AND agent_id = $2
                   AND status = 'scheduled_retry'
                   AND (context_snapshot->>'issueId' = $3 OR context_snapshot->>'taskId' = $3)
                 ORDER BY scheduled_retry_at ASC NULLS LAST, created_at ASC
                 LIMIT 1
             )",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id.to_string())
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(|error| HeartbeatError::CancelRunFailed(error.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_heartbeat_context(
        &self,
        issue_id: Uuid,
        _company_id: Uuid,
    ) -> Result<HeartbeatContext, HeartbeatError> {
        let active_agents = sqlx::query("SELECT agent_id, status, started_at FROM heartbeat_runs WHERE company_id=$1 AND (context_snapshot->>'issueId'=$2 OR context_snapshot->>'taskId'=$2) AND status IN ('queued','running')")
            .bind(_company_id).bind(issue_id.to_string()).fetch_all(&self.pool).await.map_err(|e| HeartbeatError::Internal(e.to_string()))?.into_iter().filter_map(|row| Some(AgentHeartbeatInfo { agent_id: row.try_get("agent_id").ok()?, last_heartbeat_at: row.try_get("started_at").ok(), status: HeartbeatStatus::Active })).collect::<Vec<_>>();
        let wakeup_count = active_agents.len() as i64;
        Ok(HeartbeatContext {
            issue_id,
            company_id: _company_id,
            active_agents,
            last_wakeup_at: None,
            wakeup_count,
        })
    }
}

impl DefaultHeartbeatService {
    /// Reconcile heartbeat runs that survived a server restart without an
    /// in-memory child process. Paperclip treats these as process-lost runs
    /// instead of leaving them live forever. The age threshold avoids racing
    /// the small window between spawning the child and registering its handle.
    pub async fn reconcile_orphaned_runs(&self, stale_after_secs: i64) -> Result<usize, HeartbeatError> {
        let rows = sqlx::query(
            "SELECT id, agent_id, company_id, context_snapshot, updated_at
             FROM heartbeat_runs
             WHERE status = 'running' AND updated_at < NOW() - ($1 * INTERVAL '1 second')",
        )
        .bind(stale_after_secs.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut reconciled = 0;
        for row in rows {
            let run_id: Uuid = row
                .try_get("id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

            // A live in-process run is owned by execute_run; do not interfere
            // with it. Runs absent from this map are the restart/orphan case.
            if self.children.lock().await.contains_key(&run_id) {
                continue;
            }

            let agent_id: Uuid = row
                .try_get("agent_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let issue_id = row
                .try_get::<Option<serde_json::Value>, _>("context_snapshot")
                .ok()
                .flatten()
                .and_then(|snapshot| snapshot.get("issueId").and_then(|v| v.as_str()).map(str::to_owned))
                .and_then(|value| Uuid::parse_str(&value).ok());
            let error = "Process lost -- server may have restarted while the run was active";

            let updated = sqlx::query(
                "UPDATE heartbeat_runs
                 SET status = 'failed', error = $2, finished_at = NOW(), updated_at = NOW(),
                     result_json = COALESCE(result_json, '{}'::jsonb) || '{\"processLost\":true}'::jsonb
                 WHERE id = $1 AND status = 'running'",
            )
            .bind(run_id)
            .bind(error)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            if updated.rows_affected() == 0 {
                continue;
            }

            if let Some(issue_id) = issue_id {
                sqlx::query(
                    "UPDATE issues
                     SET status = 'todo'::issue_status, checkout_run_id = NULL,
                         execution_run_id = NULL, execution_locked_at = NULL,
                         execution_agent_name_key = NULL, updated_at = NOW()
                     WHERE id = $1 AND company_id = $2 AND execution_run_id = $3",
                )
                .bind(issue_id)
                .bind(company_id)
                .bind(run_id)
                .execute(&self.pool)
                .await
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

                sqlx::query(
                    "UPDATE agent_wakeup_requests
                     SET status = 'failed', error = $4, finished_at = NOW(), updated_at = NOW()
                     WHERE company_id = $1 AND agent_id = $2
                       AND status IN ('queued','dispatched','running')
                       AND payload->>'issueId' = $3",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(issue_id.to_string())
                .bind(error)
                .execute(&self.pool)
                .await
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            }

            sqlx::query("UPDATE agents SET status = 'idle', updated_at = NOW() WHERE id = $1 AND status = 'running'")
                .bind(agent_id)
                .execute(&self.pool)
                .await
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

            publish_live_event(
                &self.sse_service,
                company_id,
                "heartbeat.run.status",
                serde_json::json!({
                    "runId": run_id,
                    "agentId": agent_id,
                    "issueId": issue_id,
                    "status": "failed",
                    "error": error,
                    "errorCode": "process_lost",
                }),
            )
            .await;
            reconciled += 1;
        }

        Ok(reconciled)
    }

    /// Requeue assigned todo issues that were created before assignment wakeups
    /// were wired into the issue API.
    pub async fn reconcile_pending_issues(&self) -> Result<usize, HeartbeatError> {
        let rows = sqlx::query(
            "SELECT i.id, i.assignee_agent_id, i.company_id FROM issues i WHERE i.status = 'todo' AND i.assignee_agent_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM heartbeat_runs r WHERE r.company_id = i.company_id AND r.agent_id = i.assignee_agent_id AND r.status IN ('queued','running') AND (r.context_snapshot->>'issueId' = i.id::text OR r.context_snapshot->>'taskId' = i.id::text)) AND NOT EXISTS (SELECT 1 FROM agent_wakeup_requests w WHERE w.company_id = i.company_id AND w.agent_id = i.assignee_agent_id AND w.status IN ('queued','dispatched','running') AND w.payload->>'issueId' = i.id::text)",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut reconciled = 0;
        for row in rows {
            let issue_id: Uuid = row
                .try_get("id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let agent_id: Uuid = row
                .try_get("assignee_agent_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            self.wakeup_with_options(
                agent_id,
                issue_id,
                company_id,
                HeartbeatWakeupOptions {
                    source: Some("recovery".to_string()),
                    trigger_detail: Some("heartbeat_recovery".to_string()),
                    reason: Some("issue_assignment_recovery".to_string()),
                    idempotency_key: Some(format!("issue_assignment_recovery:{issue_id}")),
                    payload: Some(serde_json::json!({
                        "issueId": issue_id,
                        "recovery": "assigned_issue_without_live_wakeup",
                    })),
                    context_snapshot: Some(serde_json::json!({
                        "issueId": issue_id,
                        "source": "heartbeat.recovery",
                        "reason": "issue_assignment_recovery",
                    })),
                    ..Default::default()
                },
            )
            .await?;
            reconciled += 1;
        }
        Ok(reconciled)
    }

    /// Heal blocked Issues whose dependency graph is ready but whose wake was
    /// lost between the blocker transition and the normal fan-out path.
    pub async fn reconcile_dependency_wakeups(&self) -> Result<usize, HeartbeatError> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (dependent.id) dependent.id, dependent.assignee_agent_id,
                    dependent.company_id, relation.issue_id AS blocker_issue_id
             FROM issue_relations relation
             JOIN issues dependent ON dependent.id = relation.related_issue_id
             JOIN issues blocker ON blocker.id = relation.issue_id
             WHERE relation.type = 'blocks'
               AND blocker.status = 'done'
               AND dependent.status = 'blocked'
               AND dependent.assignee_agent_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1
                   FROM issue_relations remaining
                   JOIN issues unresolved ON unresolved.id = remaining.issue_id
                   WHERE remaining.company_id = relation.company_id
                     AND remaining.related_issue_id = dependent.id
                     AND remaining.type = 'blocks'
                     AND unresolved.status <> 'done'
               )
               AND NOT EXISTS (
                   SELECT 1
                   FROM heartbeat_runs live
                   WHERE live.company_id = dependent.company_id
                     AND live.agent_id = dependent.assignee_agent_id
                     AND live.status IN ('queued', 'running')
                     AND (live.context_snapshot->>'issueId' = dependent.id::text
                          OR live.context_snapshot->>'taskId' = dependent.id::text)
               )
               AND NOT EXISTS (
                   SELECT 1
                   FROM agent_wakeup_requests wake
                   WHERE wake.company_id = dependent.company_id
                     AND wake.agent_id = dependent.assignee_agent_id
                     AND wake.idempotency_key =
                         'issue_graph_liveness_backstop:' || dependent.id::text || ':' || relation.issue_id::text
                     AND wake.status IN ('queued', 'dispatched', 'running', 'completed')
               )
             ORDER BY dependent.id, relation.issue_id
             LIMIT 500",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut healed = 0;
        for row in rows {
            let issue_id: Uuid = row
                .try_get("id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let agent_id: Uuid = row
                .try_get("assignee_agent_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let blocker_issue_id: Uuid = row
                .try_get("blocker_issue_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let idempotency_key = format!(
                "issue_graph_liveness_backstop:{}:{}",
                issue_id, blocker_issue_id
            );
            self.wakeup_with_options(
                agent_id,
                issue_id,
                company_id,
                HeartbeatWakeupOptions {
                    source: Some("automation".to_string()),
                    trigger_detail: Some("system".to_string()),
                    reason: Some("issue_graph_liveness_backstop".to_string()),
                    idempotency_key: Some(idempotency_key),
                    payload: Some(serde_json::json!({
                        "issueId": issue_id,
                        "resolvedBlockerIssueId": blocker_issue_id,
                        "backstop": "issue_graph_liveness_reconciliation",
                    })),
                    context_snapshot: Some(serde_json::json!({
                        "issueId": issue_id,
                        "taskId": issue_id,
                        "source": "issue_graph_liveness.backstop",
                        "resolvedBlockerIssueId": blocker_issue_id,
                    })),
                    ..Default::default()
                },
            )
            .await?;
            healed += 1;
        }
        Ok(healed)
    }

    /// Maximum automatic scheduled-retry attempts before a failed run is left
    /// as a terminal `failed` (mirrors Paperclip's recoverable-run cap).
    const MAX_SCHEDULED_RETRY_ATTEMPTS: i32 = 3;
    /// Backoff (seconds) for the Nth scheduled retry (exponential, capped).
    fn scheduled_retry_backoff_secs(attempt: i32) -> i64 {
        let base: i64 = 60;
        let doublings = (attempt.max(1) - 1).min(6) as u32;
        (base * 2_i64.pow(doublings)).min(60 * 60)
    }

    /// A run is a self-healing candidate when it failed with a recoverable
    /// error family: transient upstream (rate limits/overload), auth, or an
    /// upstream protocol glitch. Permanent failures (explicit business logic
    /// failure with no error_code) are not retried.
    fn is_recoverable_failure(error_code: Option<&str>) -> bool {
        matches!(
            error_code,
            Some("claude_auth_required")
                | Some("claude_transient_upstream")
                | Some("claude_malformed_response")
                | Some("adapter_failed")
        )
    }

    /// Decide whether a just-failed run should be auto-rescheduled instead of
    /// left terminal, and if so transition it to `scheduled_retry`. Returns
    /// `true` when the run was rescheduled.
    pub async fn maybe_schedule_retry(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        error_code: Option<&str>,
        error_family: Option<&str>,
        reason: &str,
    ) -> Result<bool, HeartbeatError> {
        if !Self::is_recoverable_failure(error_code) {
            return Ok(false);
        }
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT COALESCE(scheduled_retry_attempt, 0) FROM heartbeat_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
        let attempt = row.map(|r| (r.0 + 1).min(Self::MAX_SCHEDULED_RETRY_ATTEMPTS)).unwrap_or(1);
        if attempt > Self::MAX_SCHEDULED_RETRY_ATTEMPTS {
            return Ok(false);
        }
        let backoff = Self::scheduled_retry_backoff_secs(attempt);
        let updated = sqlx::query(
            "UPDATE heartbeat_runs
             SET status = 'scheduled_retry'::heartbeat_run_status,
                 scheduled_retry_at = NOW() + ($2 || ' seconds')::interval,
                 scheduled_retry_attempt = $3,
                 scheduled_retry_reason = $4,
                 retry_of_run_id = $1,
                 error_code = COALESCE($5, error_code),
                 error_family = COALESCE($6, error_family),
                 finished_at = NULL,
                 result_json = COALESCE(result_json, '{}'::jsonb)
                     || jsonb_build_object('scheduledRetry', true, 'scheduledRetryAttempt', $3),
                 updated_at = NOW()
             WHERE id = $1 AND status = 'failed'",
        )
        .bind(run_id)
        .bind(backoff)
        .bind(attempt)
        .bind(reason)
        .bind(error_code)
        .bind(error_family)
        .execute(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
        if updated.rows_affected() > 0 {
            tracing::info!(
                run_id = %run_id,
                attempt = %attempt,
                backoff_secs = %backoff,
                "scheduled recoverable run for retry"
            );
            let _ = publish_live_event(
                &self.sse_service,
                company_id,
                "heartbeat.run.status",
                serde_json::json!({
                    "runId": run_id,
                    "agentId": agent_id,
                    "issueId": issue_id,
                    "status": "scheduled_retry",
                    "scheduledRetryAttempt": attempt,
                    "scheduledRetryAt": (chrono::Utc::now() + chrono::Duration::seconds(backoff)),
                    "errorCode": error_code,
                }),
            )
            .await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Self-healing promotion: find `scheduled_retry` runs whose
    /// `scheduled_retry_at` is due and re-wake them via the normal wakeup path
    /// (which resets the run back to a queued wakeup request). Idempotent per
    /// run; safe to call from the heartbeat_recovery scheduler job.
    pub async fn promote_due_scheduled_retries(&self) -> Result<usize, HeartbeatError> {
        let rows = sqlx::query(
            "SELECT id, agent_id, company_id, context_snapshot, scheduled_retry_attempt
             FROM heartbeat_runs
             WHERE status = 'scheduled_retry'
               AND scheduled_retry_at IS NOT NULL
               AND scheduled_retry_at <= NOW()
             ORDER BY scheduled_retry_at ASC
             LIMIT 200",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut promoted = 0;
        for row in rows {
            let run_id: Uuid = row
                .try_get("id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let agent_id: Uuid = row
                .try_get("agent_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let attempt: i32 = row
                .try_get("scheduled_retry_attempt")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let issue_id = row
                .try_get::<Option<serde_json::Value>, _>("context_snapshot")
                .ok()
                .flatten()
                .and_then(|snapshot| {
                    snapshot
                        .get("issueId")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .and_then(|value| Uuid::parse_str(&value).ok());
            let Some(issue_id) = issue_id else {
                continue;
            };

            if let Err(e) = self
                .wakeup_with_options(
                    agent_id,
                    issue_id,
                    company_id,
                    HeartbeatWakeupOptions {
                        source: Some("recovery".to_string()),
                        trigger_detail: Some("scheduled_retry".to_string()),
                        reason: Some("scheduled_retry_promotion".to_string()),
                        idempotency_key: Some(format!(
                            "scheduled_retry_promotion:{}:{}",
                            issue_id, attempt
                        )),
                        payload: Some(serde_json::json!({
                            "issueId": issue_id,
                            "scheduledRetry": true,
                            "scheduledRetryAttempt": attempt,
                        })),
                        context_snapshot: Some(serde_json::json!({
                            "issueId": issue_id,
                            "source": "heartbeat.scheduled_retry",
                            "reason": "scheduled_retry_promotion",
                        })),
                        ..Default::default()
                    },
                )
                .await
            {
                tracing::error!(
                    run_id = %run_id,
                    error = %e,
                    "failed to promote scheduled retry"
                );
                continue;
            }
            // Clear the scheduled_retry marker now that a fresh wakeup exists.
            let _ = sqlx::query(
                "UPDATE heartbeat_runs
                 SET scheduled_retry_at = NULL, updated_at = NOW()
                 WHERE id = $1 AND status = 'scheduled_retry'",
            )
            .bind(run_id)
            .execute(&self.pool)
            .await;
            promoted += 1;
        }
        Ok(promoted)
    }

    fn clone_for_task(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            children: self.children.clone(),
            sse_service: self.sse_service.clone(),
            cost_service: self.cost_service.clone(),
            budget_service: self.budget_service.clone(),
        }
    }
}

#[cfg(test)]
mod adapter_outcome_tests {
    use super::{parse_adapter_outcome, valid_claude_resume_session};

    #[test]
    fn explicit_structured_error_overrides_zero_exit() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"result","subtype":"error","is_error":true,"result":"tool failed"}"#,
        );
        assert!(outcome.explicit_failure);
        assert_eq!(outcome.failure_reason.as_deref(), Some("tool failed"));
    }

    #[test]
    fn classifies_claude_malformed_response() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"result","subtype":"error","is_error":true,"result":"API Error: API returned an empty or malformed response (HTTP 200)"}"#,
        );
        assert_eq!(outcome.error_code.as_deref(), Some("claude_malformed_response"));
        assert_eq!(outcome.error_family.as_deref(), Some("upstream_protocol"));
        assert_eq!(
            outcome.failure_reason.as_deref(),
            Some("API Error: API returned an empty or malformed response (HTTP 200)")
        );
    }

    #[test]
    fn parses_tool_calls_and_handoff_metadata() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"tool_use","name":"paperclipGetIssue"}
{"type":"handoff","handoff":{"issueId":"ABC-1"}}
{"type":"result","subtype":"success","result":"done"}"#,
        );
        assert!(!outcome.explicit_failure);
        assert_eq!(outcome.tool_call_count, 1);
        assert_eq!(outcome.result_summary.as_deref(), Some("done"));
        assert!(outcome.handoff.is_some());
    }

    #[test]
    fn parses_nested_claude_tool_use_without_promoting_tool_error_to_run_failure() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"paperclipGetIssue"}]}}
{"type":"user","message":{"content":[{"type":"tool_result","is_error":true}]}}
{"type":"result","subtype":"success","result":"done"}"#,
        );
        assert_eq!(outcome.tool_call_count, 1);
        assert!(!outcome.explicit_failure);
        assert_eq!(outcome.result_summary.as_deref(), Some("done"));
    }

    #[test]
    fn parses_native_claude_session_usage_and_cost_fields() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"system","session_id":"sess-42","model":"claude-sonnet","usage":{"input_tokens":120,"output_tokens":45,"cache_read_input_tokens":10},"total_cost_usd":0.0123}
{"type":"result","subtype":"success","result":"done"}"#,
        );
        assert_eq!(outcome.session_id.as_deref(), Some("sess-42"));
        assert_eq!(outcome.input_tokens, 120);
        assert_eq!(outcome.output_tokens, 45);
        assert_eq!(outcome.cached_input_tokens, 10);
        assert_eq!(outcome.cost_usd, Some(0.0123));
        assert_eq!(outcome.model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn only_uuid_claude_sessions_are_eligible_for_resume() {
        assert!(valid_claude_resume_session(Some("550e8400-e29b-41d4-a716-446655440000")).is_some());
        assert!(valid_claude_resume_session(Some("not-a-session")).is_none());
        assert!(valid_claude_resume_session(None).is_none());
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    pub struct MockHeartbeatService {
        wakeup_count: AtomicI64,
        cancel_count: AtomicI64,
        should_fail: std::sync::atomic::AtomicBool,
    }

    impl MockHeartbeatService {
        pub fn new() -> Self {
            Self {
                wakeup_count: AtomicI64::new(0),
                cancel_count: AtomicI64::new(0),
                should_fail: std::sync::atomic::AtomicBool::new(false),
            }
        }

        pub fn wakeup_call_count(&self) -> i64 {
            self.wakeup_count.load(Ordering::Relaxed)
        }

        pub fn cancel_call_count(&self) -> i64 {
            self.cancel_count.load(Ordering::Relaxed)
        }

        pub fn wakeup_count(&self) -> i64 {
            self.wakeup_call_count()
        }

        pub fn set_should_fail(&self, should_fail: bool) {
            self.should_fail.store(should_fail, Ordering::Relaxed);
        }
    }

    #[async_trait]
    impl HeartbeatService for MockHeartbeatService {
        async fn wakeup(
            &self,
            _agent_id: Uuid,
            _issue_id: Uuid,
            _company_id: Uuid,
        ) -> Result<(), HeartbeatError> {
            if self.should_fail.load(Ordering::Relaxed) {
                return Err(HeartbeatError::Internal("Mock failure".to_string()));
            }
            self.wakeup_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn cancel_run(
            &self,
            _agent_id: Uuid,
            _issue_id: Uuid,
            _company_id: Uuid,
            _reason: &str,
        ) -> Result<(), HeartbeatError> {
            self.cancel_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn cancel_scheduled_retry(
            &self,
            _agent_id: Uuid,
            _issue_id: Uuid,
            _company_id: Uuid,
            _reason: &str,
        ) -> Result<bool, HeartbeatError> {
            Ok(true)
        }

        async fn get_heartbeat_context(
            &self,
            issue_id: Uuid,
            _company_id: Uuid,
        ) -> Result<HeartbeatContext, HeartbeatError> {
            Ok(HeartbeatContext {
                issue_id,
                company_id: _company_id,
                active_agents: vec![],
                last_wakeup_at: None,
                wakeup_count: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_command_logs_redact_gateway_token_without_redacting_the_real_argv() {
        let token = "ptg_test_secret";
        let command = shell_command(
            "claude",
            &[format!("--header=Bearer {token}"), "--print".to_string()],
            None,
            Some("hello"),
        );
        let logged = redact_gateway_token(&command, token);
        assert!(!logged.contains(token));
        assert!(logged.contains("[PAPERCLIP_TOOL_GATEWAY_TOKEN]"));
        assert!(command.contains(token));
    }

    #[test]
    fn resolve_env_value_uses_host_env_when_looks_like_var() {
        // 设置测试环境变量
        std::env::set_var("TEST_AUTH_TOKEN", "secret_from_env");
        std::env::set_var("MY_API_KEY", "key_from_env");
        
        // 测试1: 直接使用环境变量名
        assert_eq!(
            resolve_env_value("TEST_AUTH_TOKEN"),
            "secret_from_env",
            "应该从环境变量读取值"
        );
        
        // 测试2: 使用 $ 前缀
        assert_eq!(
            resolve_env_value("$MY_API_KEY"),
            "key_from_env",
            "应该支持 $VAR 格式"
        );
        
        // 测试3: 使用花括号包裹
        assert_eq!(
            resolve_env_value("${TEST_AUTH_TOKEN}"),
            "secret_from_env",
            "应该支持 dollar-brace-VAR-brace 格式"
        );
        
        // 测试4: 环境变量不存在时，使用配置值本身
        assert_eq!(
            resolve_env_value("NONEXISTENT_VAR"),
            "NONEXISTENT_VAR",
            "环境变量不存在时应该使用配置值本身"
        );
        
        // 测试5: 不像环境变量的值（包含小写字母或特殊字符），直接使用
        assert_eq!(
            resolve_env_value("sk-real-api-key-123"),
            "sk-real-api-key-123",
            "不像环境变量名的值应该直接使用"
        );
        
        assert_eq!(
            resolve_env_value("http://localhost:8787"),
            "http://localhost:8787",
            "URL应该直接使用"
        );
        
        assert_eq!(
            resolve_env_value("claude-3-opus"),
            "claude-3-opus",
            "包含小写和横线的值应该直接使用"
        );
        
        // 测试6: 空值
        std::env::set_var("EMPTY_VAR", "");
        assert_eq!(
            resolve_env_value("EMPTY_VAR"),
            "EMPTY_VAR",
            "环境变量为空时应该使用配置值本身"
        );
        
        // 清理测试环境变量
        std::env::remove_var("TEST_AUTH_TOKEN");
        std::env::remove_var("MY_API_KEY");
        std::env::remove_var("EMPTY_VAR");
    }
    
    #[test]
    fn resolve_env_value_preserves_whitespace_in_direct_values() {
        // 直接值应该保留原样（包括空格）
        assert_eq!(
            resolve_env_value("  some value with spaces  "),
            "  some value with spaces  ",
            "非环境变量的值应该完全保留原样"
        );
    }

    #[test]
    fn test_merge_adapter_config() {
        // 测试1: 数据库配置覆盖默认配置
        let db_config = serde_json::json!({
            "command": "claude",
            "maxTurnsPerRun": 10
        });
        let default_config = Some(serde_json::json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"
            },
            "command": "claude",
            "maxTurnsPerRun": 20,
            "effort": "high"
        }));
        
        let merged = merge_adapter_config(db_config, default_config);
        
        assert_eq!(merged["command"], "claude");
        assert_eq!(merged["maxTurnsPerRun"], 10); // 数据库值优先
        assert_eq!(merged["effort"], "high"); // 从默认配置补充
        assert!(merged.get("env").is_some()); // env 从默认配置补充
        
        // 测试2: 数据库配置为空对象，使用默认配置
        let db_config = serde_json::json!({});
        let default_config = Some(serde_json::json!({
            "env": {"ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"},
            "command": "claude"
        }));
        
        let merged = merge_adapter_config(db_config, default_config);
        assert!(merged.get("env").is_some());
        assert_eq!(merged["command"], "claude");
        
        // 测试3: 没有默认配置，返回数据库配置
        let db_config = serde_json::json!({"command": "claude"});
        let merged = merge_adapter_config(db_config.clone(), None);
        assert_eq!(merged, db_config);
    }
    
    #[test]
    fn test_load_default_adapter_config() {
        // 测试加载不存在的配置
        let config = load_default_adapter_config("nonexistent_adapter");
        assert!(config.is_none());
        
        // 测试文件名转换：下划线转横线
        // claude_local → claude-local.json
        let config = load_default_adapter_config("claude_local");
        if config.is_some() {
            let cfg = config.unwrap();
            assert!(cfg.get("env").is_some());
        }
    }

    #[tokio::test]
    async fn test_mock_heartbeat_service() {
        let service = mock::MockHeartbeatService::new();
        let agent_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        assert_eq!(service.wakeup_call_count(), 0);
        assert_eq!(service.cancel_call_count(), 0);

        service
            .wakeup(agent_id, issue_id, company_id)
            .await
            .unwrap();
        assert_eq!(service.wakeup_call_count(), 1);

        service
            .cancel_run(agent_id, issue_id, company_id, "test")
            .await
            .unwrap();
        assert_eq!(service.cancel_call_count(), 1);
    }
}
