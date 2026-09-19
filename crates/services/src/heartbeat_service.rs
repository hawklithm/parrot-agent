use async_trait::async_trait;
use crate::issue_comment_service::{CommentAttribution, IssueCommentService};
use crate::text_utils::truncate_suffix_chars;
use crate::sse_service::{InMemorySseService, SseService};
use crate::secret_service::RuntimeSecretManifestEntry;
use crate::adapter_executor::{AdapterExecutionContext, AdapterExecutor, ExecutionStatus, ExecutionTargetConfig, ExecutionTargetType, HttpExecutor};
use crate::mcp_client_config::{
    claude_mcp_server, codex_mcp_overrides, cursor_mcp_server, gemini_mcp_server,
    merge_mcp_server, merge_opencode_mcp_server, RestorableJsonConfig,
    write_private_temp_json,
};
use chrono::{DateTime, Utc};
use models::{Agent, AgentStatus, CommentActorType, SseEvent, SseEventType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::path::PathBuf;
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

fn build_codex_exec_args(model: Option<&str>, is_acp: bool) -> Vec<String> {
    if is_acp {
        let mut args = vec!["acp".to_string()];
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            args.extend(["--model".to_string(), model.to_string()]);
        }
        return args;
    }
    let mut args = vec!["exec".to_string(), "--json".to_string()];
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    args.push("-".to_string());
    args
}

fn resolve_acp_mode(adapter: &str, engine: &str) -> Result<bool, String> {
    if adapter == "claude_local" && engine == "acp" {
        return Err(
            "Claude Code CLI does not support --acp; use engine=cli because ACP is not available in this runtime"
                .to_string(),
        );
    }

    Ok(adapter == "codex_local" && matches!(engine, "acp" | "auto"))
}

fn reject_unsupported_claude_args(adapter: &str, args: &[String]) -> Result<(), String> {
    if adapter == "claude_local" && args.iter().any(|arg| arg == "--acp") {
        return Err(
            "Claude Code CLI does not support --acp; remove it from args/extraArgs or use engine=cli"
                .to_string(),
        );
    }
    Ok(())
}

fn instructions_bundle_entry_content(agent: &Agent) -> Option<String> {
    let bundle = agent.metadata.0.instructions_bundle.as_ref()?;
    let entry_file = bundle
        .get("entryFile")
        .and_then(Value::as_str)
        .unwrap_or("AGENTS.md");
    bundle
        .get("files")
        .and_then(Value::as_object)
        .and_then(|files| files.get(entry_file))
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
        .map(ToOwned::to_owned)
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

/// The values `heartbeat_runs.invocation_source` accepts, mirroring Paperclip's
/// `HEARTBEAT_INVOCATION_SOURCES` (`packages/shared/src/constants.ts`):
/// `timer | assignment | on_demand | automation`.
///
/// This column records *how* a run was invoked, not *why* — the describing
/// string ("issue.comment.reopen") belongs in `context_snapshot.source` and
/// `reason`, which is where Paperclip keeps it. Keep this in sync with the
/// `valid_invocation_source` check constraint.
pub const HEARTBEAT_INVOCATION_SOURCES: [&str; 4] =
    ["timer", "assignment", "on_demand", "automation"];

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
    /// When this wakeup is a scheduled-retry promotion, the run it continues
    /// (the original failed run). Persisted on the created heartbeat run so the
    /// dashboard `recovered` counter can identify retry-succeeded runs, and
    /// read by the funnel to keep a retry wake out of an in-flight run's
    /// coalescing: the caller re-runs a run that has already ended.
    pub retry_of_run_id: Option<Uuid>,
    /// The retry attempt the promoted run starts from when `retry_of_run_id` is
    /// set. Paperclip's retry row owns its own `scheduledRetryAttempt`
    /// (`heartbeat.ts:11582-11600`); Parrot keeps the attempt on the run, so it
    /// has to travel with the promotion — `maybe_schedule_retry` reads the
    /// attempt off the run it disposes, and a promoted run that started at 0
    /// would retry at attempt 1 forever instead of reaching the cap.
    pub scheduled_retry_attempt: Option<i32>,
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
    resumed_session_id: Option<String>,
    billing_type: String,
    runtime_secret_manifest: Vec<RuntimeSecretManifestEntry>,
}

const HEARTBEAT_COMMENT_MAX_CHARS: usize = 1_200;
const WITHHELD_HEARTBEAT_COMMENT: &str =
    "Run completed. Agent did not post a summary comment this run (transcript withheld — see run log).";

/// Convert an adapter's final result into the normal task conversation entry.
///
/// Paperclip deliberately withholds long or obviously narrational adapter
/// output from the issue thread and leaves the full transcript in the run log.
/// Keeping the same boundary prevents tool chatter from becoming a misleading
/// user-facing task reply while still giving successful runs a chat message.
fn build_heartbeat_run_issue_comment(summary: Option<&str>) -> Option<String> {
    let summary = summary?.trim();
    if summary.is_empty() {
        return None;
    }

    let lower = summary.to_ascii_lowercase();
    let starts_with_narration = [
        "let me ",
        "i'll ",
        "i’m ",
        "i'm ",
        "i can see",
        "now i'll ",
        "now i’ll ",
        "next i'll ",
        "next i’ll ",
        "looking at",
        "fetching",
        "checking",
        "first,",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));

    if summary.chars().count() > HEARTBEAT_COMMENT_MAX_CHARS || starts_with_narration {
        return Some(WITHHELD_HEARTBEAT_COMMENT.to_string());
    }

    Some(summary.to_string())
}

/// Read the structured result records emitted by Claude/Codex JSONL modes.
/// Exit status remains the process-level fallback, but an adapter can emit an
/// explicit error/result record before exiting zero; treating that as success
/// would incorrectly complete the Issue.
fn parse_adapter_outcome(output: &str, adapter_type: &str) -> AdapterOutcome {
    let mut outcome = AdapterOutcome::default();
    for line in output.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        visit_adapter_event(&value, &mut outcome, true, adapter_type);
    }
    outcome
}

fn is_codex_unknown_session_output(output: &AdapterCommandOutput) -> bool {
    let combined = format!("{}{}", output.stdout, output.stderr);
    let parsed = parse_adapter_outcome(&combined, "codex_local");
    parsed.error_code.as_deref() == Some("codex_unknown_session")
        || (output.exit_code != 0
            && combined.lines().any(|line| {
                classify_codex_error(line).0.as_deref() == Some("codex_unknown_session")
            }))
}

fn text_from_content(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.to_owned()),
        Value::Array(items) => items.iter().find_map(|item| {
            item.get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .map(ToOwned::to_owned)
        }),
        _ => None,
    }
}

fn resolve_biller(adapter_type: &str, provider: Option<&str>) -> String {
    match provider.unwrap_or(adapter_type).to_ascii_lowercase().as_str() {
        "anthropic" | "claude" | "claude_local" => "anthropic".to_string(),
        "openai" | "codex" | "codex_local" => "openai".to_string(),
        provider => provider.to_string(),
    }
}

fn config_u64(config: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| config.get(*key).and_then(Value::as_u64))
}

fn provider_http_timeout(config: &Value) -> Duration {
    let timeout_ms = config_u64(config, &["timeoutMs", "timeout_ms"])
        .or_else(|| config_u64(config, &["timeoutSec", "timeout_sec"]).map(|seconds| seconds.saturating_mul(1000)))
        .filter(|timeout_ms| *timeout_ms > 0)
        .unwrap_or(120_000)
        .clamp(1_000, 15 * 60 * 1_000);
    Duration::from_millis(timeout_ms)
}

fn provider_http_retries(config: &Value) -> u32 {
    config_u64(config, &["retries", "maxRetries", "max_retries"])
        .unwrap_or(2)
        .min(3) as u32
}

fn provider_retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(3);
    Duration::from_millis((250_u64 * 2_u64.pow(exponent)).min(2_000))
}

const PROVIDER_MAX_RETRY_AFTER_MS: u64 = 15 * 60 * 1_000;

fn provider_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = raw.parse::<f64>() {
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let milliseconds = (seconds * 1_000.0).round();
        return Some(Duration::from_millis(
            milliseconds.min(PROVIDER_MAX_RETRY_AFTER_MS as f64) as u64,
        ));
    }

    let retry_at = httpdate::parse_http_date(raw).ok()?;
    let wait = retry_at.duration_since(std::time::SystemTime::now()).ok()?;
    Some(wait.min(Duration::from_millis(PROVIDER_MAX_RETRY_AFTER_MS)))
}

fn provider_retry_delay_with_hint(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after.unwrap_or_else(|| provider_retry_delay(attempt))
}

fn is_retryable_provider_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn redact_adapter_secret(value: &str, secret: &str) -> String {
    if secret.is_empty() {
        value.to_owned()
    } else {
        value.replace(secret, "[REDACTED]")
    }
}

fn valid_claude_resume_session(session_id: Option<&str>) -> Option<String> {
    let session_id = session_id?.trim();
    if uuid::Uuid::parse_str(session_id).is_ok() { Some(session_id.to_string()) } else { None }
}

fn valid_codex_resume_session(session_id: Option<&str>) -> Option<String> {
    let session_id = session_id?.trim();
    if session_id.is_empty()
        || session_id.len() > 256
        || session_id.starts_with('-')
        || !session_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':'))
    {
        return None;
    }
    Some(session_id.to_string())
}

fn classify_claude_error(message: &str) -> (Option<String>, Option<String>) {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("not logged in")
        || normalized.contains("please log in")
        || normalized.contains("login required")
        || normalized.contains("authentication required")
        || normalized.contains("unauthorized")
        || normalized.contains("invalid api key")
        || normalized.contains("http 401")
        || normalized.contains("status 401")
    {
        return (Some("claude_auth_required".to_string()), Some("authentication".to_string()));
    }
    if normalized.contains("rate limit")
        || normalized.contains("rate_limit_error")
        || normalized.contains("too many requests")
        || normalized.contains("overloaded")
        || normalized.contains("service unavailable")
        || normalized.contains("429")
        || normalized.contains("500")
        || normalized.contains("502")
        || normalized.contains("503")
        || normalized.contains("504")
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

fn classify_codex_error(message: &str) -> (Option<String>, Option<String>) {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("unknown session")
        || normalized.contains("unknown thread")
        || normalized.contains("session not found")
        || normalized.contains("thread not found")
        || normalized.contains("no rollout found")
        || normalized.contains("missing rollout path")
        || normalized.contains("rollout path for thread")
    {
        return (
            Some("codex_unknown_session".to_string()),
            Some("session".to_string()),
        );
    }
    if normalized.contains("refresh_token_reused")
        || normalized.contains("refresh token already been used")
        || normalized.contains("refresh token reused")
    {
        return (
            Some("refresh_token_reused".to_string()),
            Some("refresh_token_reused".to_string()),
        );
    }
    if normalized.contains("refresh_token_expired")
        || normalized.contains("expired refresh token")
        || normalized.contains("refresh token has expired")
    {
        return (
            Some("refresh_token_expired".to_string()),
            Some("refresh_token_expired".to_string()),
        );
    }
    if normalized.contains("refresh_token_invalidated")
        || normalized.contains("invalid refresh token")
        || normalized.contains("revoked refresh token")
        || normalized.contains("invalid_grant")
        || normalized.contains("missing bearer")
    {
        return (
            Some("refresh_token_invalidated".to_string()),
            Some("refresh_token_invalidated".to_string()),
        );
    }
    if normalized.contains("usage limit")
        || normalized.contains("at capacity")
        || normalized.contains("capacity limit")
    {
        return (
            Some("provider_quota".to_string()),
            Some("provider_quota".to_string()),
        );
    }
    if normalized.contains("rate limit")
        || normalized.contains("http 429")
        || normalized.contains("status 429")
        || normalized.contains("too many requests")
        || normalized.contains("server overloaded")
        || normalized.contains("service unavailable")
        || normalized.contains("http 500")
        || normalized.contains("http 502")
        || normalized.contains("http 503")
        || normalized.contains("http 504")
        || normalized.contains("high demand")
        || normalized.contains("temporary errors")
        || normalized.contains("try again later")
    {
        return (
            Some("codex_transient_upstream".to_string()),
            Some("transient_upstream".to_string()),
        );
    }
    if normalized.contains("not logged in")
        || normalized.contains("please log in")
        || normalized.contains("login required")
        || normalized.contains("authentication required")
        || normalized.contains("unauthorized")
        || normalized.contains("invalid api key")
        || normalized.contains("http 401")
        || normalized.contains("status 401")
        || normalized.contains("http 403")
        || normalized.contains("status 403")
    {
        return (
            Some("codex_auth_required".to_string()),
            Some("authentication".to_string()),
        );
    }
    if normalized.contains("empty or malformed response") {
        return (
            Some("codex_malformed_response".to_string()),
            Some("upstream_protocol".to_string()),
        );
    }
    (None, None)
}

fn classify_generic_adapter_error(message: &str) -> (Option<String>, Option<String>) {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("unknown session")
        || normalized.contains("unknown thread")
        || normalized.contains("session not found")
        || normalized.contains("thread not found")
        || normalized.contains("no rollout found")
    {
        return (
            Some("adapter_unknown_session".to_string()),
            Some("session".to_string()),
        );
    }
    if normalized.contains("not logged in")
        || normalized.contains("please log in")
        || normalized.contains("login required")
        || normalized.contains("authentication required")
        || normalized.contains("unauthorized")
        || normalized.contains("http 401")
        || normalized.contains("status 401")
        || normalized.contains("http 403")
        || normalized.contains("status 403")
        || normalized.contains("invalid refresh token")
        || normalized.contains("missing bearer")
    {
        return (
            Some("adapter_auth_required".to_string()),
            Some("authentication".to_string()),
        );
    }
    if normalized.contains("rate limit")
        || normalized.contains("http 429")
        || normalized.contains("status 429")
        || normalized.contains("too many requests")
        || normalized.contains("server overloaded")
        || normalized.contains("service unavailable")
        || normalized.contains("http 500")
        || normalized.contains("http 502")
        || normalized.contains("http 503")
        || normalized.contains("http 504")
        || normalized.contains("high demand")
        || normalized.contains("temporary errors")
    {
        return (
            Some("adapter_transient_upstream".to_string()),
            Some("transient_upstream".to_string()),
        );
    }
    (None, None)
}

fn classify_adapter_error(message: &str, adapter_type: &str) -> (Option<String>, Option<String>) {
    match adapter_type {
        "claude_local" => classify_claude_error(message),
        "codex_local" => classify_codex_error(message),
        _ => classify_generic_adapter_error(message),
    }
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

fn visit_adapter_event(value: &Value, outcome: &mut AdapterOutcome, top_level: bool, adapter_type: &str) {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or_default();

    // ACP event types
    if kind == "acpx.result" {
        // ACP turn completed: acpx.result { stopReason, summary, usage, cost }
        outcome.result_event = Some(value.clone());
        if let Some(summary) = value.get("summary").or_else(|| value.get("stopReason")).and_then(Value::as_str) {
            outcome.result_summary = Some(summary.to_owned());
        }
        return;
    }
    if kind == "acpx.error" {
        // ACP error event: acpx.error { code, message, retryable }
        outcome.explicit_failure = true;
        outcome.failure_reason = value.get("message").and_then(Value::as_str).map(ToOwned::to_owned);
        let code = value.get("code").and_then(Value::as_str).map(ToOwned::to_owned);
        outcome.error_code = code.or_else(|| Some("acpx_error".to_string()));
        outcome.error_family = Some("acp".to_string());
        return;
    }
    if kind == "acpx.session" {
        // ACP session started: acpx.session { sessionId, agent, mode }
        if outcome.session_id.is_none() {
            outcome.session_id = value
                .get("sessionId")
                .or_else(|| value.get("acpSessionId"))
                .or_else(|| value.get("runtimeSessionName"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        return;
    }
    if kind == "acpx.status" {
        // ACP status update: acpx.status { text, cost, contextWindow }
        if outcome.result_summary.is_none() {
            outcome.result_summary = value.get("text").and_then(Value::as_str).map(ToOwned::to_owned);
        }
        return;
    }
    if kind == "acpx.text_delta" {
        // ACP streaming text: capture first non-empty text as summary seed
        if outcome.result_summary.is_none() {
            outcome.result_summary = value.get("text").and_then(Value::as_str).map(ToOwned::to_owned);
        }
        return;
    }
    if kind == "acpx.tool_call" {
        outcome.tool_call_count += 1;
        return;
    }
    if kind == "acpx.tool_result" {
        return;
    }

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
            .or_else(|| (kind == "thread.started").then(|| value.get("thread_id")).flatten())
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }

    // Parse usage and cost information (Claude emits snake_case JSONL fields;
    // retain the camelCase aliases used by older Parrot adapters).
    if let Some(usage) = value.get("usage") {
        if let Some(input) = usage
            .get("input_tokens")
            .or_else(|| usage.get("inputTokens"))
            .or_else(|| usage.get("prompt_tokens"))
            .or_else(|| usage.get("promptTokens"))
            .and_then(Value::as_i64)
        {
            outcome.input_tokens = outcome.input_tokens.max(input);
        }
        if let Some(output) = usage
            .get("output_tokens")
            .or_else(|| usage.get("outputTokens"))
            .or_else(|| usage.get("completion_tokens"))
            .or_else(|| usage.get("completionTokens"))
            .and_then(Value::as_i64)
        {
            outcome.output_tokens = outcome.output_tokens.max(output);
        }
        if let Some(cached) = usage
            .get("cache_read_input_tokens")
            .or_else(|| usage.get("cached_input_tokens"))
            .or_else(|| usage.get("cachedInputTokens"))
            .or_else(|| {
                usage
                    .get("prompt_tokens_details")
                    .and_then(|details| details.get("cached_tokens"))
            })
            .or_else(|| {
                usage
                    .get("input_tokens_details")
                    .and_then(|details| details.get("cached_tokens"))
            })
            .and_then(Value::as_i64)
        {
            outcome.cached_input_tokens = outcome.cached_input_tokens.max(cached);
        }
    }
    if let Some(cost) = value
        .get("total_cost_usd")
        .or_else(|| value.get("costUsd"))
        .or_else(|| value.get("cost_usd"))
        .and_then(Value::as_f64)
    {
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
    if kind == "item.completed" {
        if let Some(item) = value.get("item") {
            if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                if let Some(text) = item.get("text").and_then(Value::as_str).filter(|text| !text.is_empty()) {
                    outcome.result_summary = Some(text.to_owned());
                }
            }
        }
    }
    if outcome.result_summary.is_none()
        && (kind == "message"
            || kind == "chat.completion"
            || value.get("role").and_then(Value::as_str) == Some("assistant")
            || value.get("choices").is_some())
    {
        let content = value
            .get("content")
            .or_else(|| value.get("message").and_then(|message| message.get("content")))
            .or_else(|| {
                value
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|message| message.get("content"))
            });
        if let Some(text) = content.and_then(text_from_content) {
            outcome.result_summary = Some(text);
        }
    }

    let is_error = top_level
        && (value.get("is_error").and_then(Value::as_bool).unwrap_or(false)
            || value.get("isError").and_then(Value::as_bool).unwrap_or(false)
            || matches!(value.get("subtype").and_then(Value::as_str), Some("error" | "failed"))
            || matches!(kind, "error" | "turn.failed"));
    if is_error {
        outcome.explicit_failure = true;
        let reason = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("error").and_then(|error| error.get("message")).and_then(Value::as_str))
            .or_else(|| value.get("message").and_then(Value::as_str))
            .or_else(|| value.get("result").and_then(Value::as_str))
            .map(ToOwned::to_owned);
        if let Some(reason) = reason {
            let (error_code, error_family) = classify_adapter_error(&reason, adapter_type);
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
        Value::Array(values) => values
            .iter()
            .for_each(|item| visit_adapter_event(item, outcome, false, adapter_type)),
        Value::Object(values) => values
            .values()
            .for_each(|item| visit_adapter_event(item, outcome, false, adapter_type)),
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
    http_executor: Arc<HttpExecutor>,
    sse_service: Arc<dyn SseService>,
    cost_service: Option<Arc<dyn crate::CostService>>,
    budget_service: Option<Arc<dyn crate::BudgetService>>,
    runtime_secret_resolver: Option<Arc<dyn crate::AdapterRuntimeSecretResolver>>,
    issue_comment_service: Option<Arc<dyn IssueCommentService>>,
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

/// Persist the structured event stream used by the run detail UI.
///
/// Parrot historically only kept adapter output in `heartbeat_runs.output`
/// and exposed tool calls as a synthetic event list.  Paperclip's contract is
/// a durable, ordered event stream, so every lifecycle/log event now gets a
/// sequence number in its own table.  The transaction-scoped advisory lock
/// makes sequence allocation safe when stdout and stderr are being drained by
/// separate tasks or when more than one server process is active.
async fn persist_heartbeat_run_event(
    pool: &PgPool,
    company_id: Uuid,
    run_id: Uuid,
    agent_id: Uuid,
    event_type: &str,
    stream: Option<&str>,
    level: Option<&str>,
    message: Option<&str>,
    payload: &Value,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(run_id.to_string())
        .execute(&mut *tx)
        .await?;
    let seq: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM heartbeat_run_events WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO heartbeat_run_events
            (company_id, run_id, agent_id, seq, event_type, stream, level, message, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(company_id)
    .bind(run_id)
    .bind(agent_id)
    .bind(seq)
    .bind(event_type)
    .bind(stream)
    .bind(level)
    .bind(message)
    .bind(payload)
    .execute(&mut *tx)
    .await?;
    // Run recovery uses heartbeat_runs.updated_at as its durable activity
    // watermark. Keep that watermark in sync with the event stream so a run
    // that is still producing output is not selected as an orphan after a
    // restart (or when the in-memory child registry is temporarily absent).
    sqlx::query(
        "UPDATE heartbeat_runs
         SET updated_at = NOW()
         WHERE id = $1 AND status IN ('queued', 'running')",
    )
    .bind(run_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

async fn persist_agent_task_session(
    pool: &PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    adapter_type: &str,
    task_key: &str,
    run_id: Uuid,
    session_id: Option<&str>,
    session_params: &Value,
    last_error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO agent_task_sessions
            (company_id, agent_id, adapter_type, task_key, session_params_json,
             session_display_id, last_run_id, last_error)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (company_id, agent_id, adapter_type, task_key)
         DO UPDATE SET
            session_params_json = COALESCE(EXCLUDED.session_params_json, agent_task_sessions.session_params_json),
            session_display_id = COALESCE(EXCLUDED.session_display_id, agent_task_sessions.session_display_id),
            last_run_id = EXCLUDED.last_run_id,
            last_error = EXCLUDED.last_error,
            updated_at = NOW()",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(adapter_type)
    .bind(task_key)
    .bind(session_params)
    .bind(session_id)
    .bind(run_id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
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

fn is_sensitive_env_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "access_token",
        "auth_token",
        "authorization",
        "bearer",
        "secret",
        "password",
        "passwd",
        "credential",
        "private_key",
        "privatekey",
        "cookie",
        "jwt",
        "token",
        "opencode_config_content",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn redact_logged_env_value(value: &str, sensitive: bool, gateway_token: &str) -> String {
    if sensitive {
        return "[REDACTED]".to_string();
    }
    redact_gateway_token(value, gateway_token)
}

fn command_with_env(
    environment: &BTreeMap<String, String>,
    sensitive_env_keys: &HashSet<String>,
    command: &str,
    gateway_token: &str,
) -> String {
    let env_prefix = environment
        .iter()
        .map(|(key, value)| {
            let logged_value = redact_logged_env_value(
                value,
                sensitive_env_keys.contains(key),
                gateway_token,
            );
            format!("{}={}", shell_quote(key), shell_quote(&logged_value))
        })
        .collect::<Vec<_>>()
        .join(" ");

    if env_prefix.is_empty() {
        command.to_string()
    } else {
        format!("{env_prefix} {command}")
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
            http_executor: Arc::new(HttpExecutor::new()),
            sse_service: InMemorySseService::new(),
            cost_service: None,
            budget_service: None,
            runtime_secret_resolver: None,
            issue_comment_service: None,
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

    pub fn with_runtime_secret_resolver(
        mut self,
        resolver: Arc<dyn crate::AdapterRuntimeSecretResolver>,
    ) -> Self {
        self.runtime_secret_resolver = Some(resolver);
        self
    }

    pub fn with_issue_comment_service(
        mut self,
        issue_comment_service: Arc<dyn IssueCommentService>,
    ) -> Self {
        self.issue_comment_service = Some(issue_comment_service);
        self
    }

    /// 克隆 service 用于后台任务
    fn clone_for_background(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            children: Arc::clone(&self.children),
            http_executor: Arc::clone(&self.http_executor),
            sse_service: Arc::clone(&self.sse_service),
            cost_service: self.cost_service.clone(),
            budget_service: self.budget_service.clone(),
            runtime_secret_resolver: self.runtime_secret_resolver.clone(),
            issue_comment_service: self.issue_comment_service.clone(),
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
    ///
    /// The return type is spelled out as an explicitly `Send` boxed future:
    /// the promoter starts queued runs by spawning `execute_run`, so its
    /// inferred future type would otherwise depend on the `Send`-ness of
    /// `execute_run` — which in turn promotes queued runs on completion.
    fn start_next_queued_run_for_agent(
        &self,
        agent_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Uuid>, String>> + Send>> {
        let service = self.clone_for_background();
        Box::pin(async move { service.start_queued_runs_for_agent(agent_id).await })
    }

    async fn start_queued_runs_for_agent(&self, agent_id: Uuid) -> Result<Vec<Uuid>, String> {
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
        // `url_key` is a derived projection with no column, so `query_as` cannot
        // build an `Agent`; go through the canonical row mapper instead.
        let row = sqlx::query("SELECT * FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?
            .ok_or(HeartbeatError::AgentNotFound(id))?;
        Ok(repositories::map_agent_row(row))
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
        let company_id: Uuid = issue.try_get("company_id").unwrap_or_default();
        let mut tx = match self.pool.begin().await { Ok(tx) => tx, Err(_) => return };
        // Keep the lookup, lock, content update, and revision allocation in
        // one transaction so concurrent heartbeat completions cannot reuse a
        // revision number or publish a partial continuation summary.
        let existing = sqlx::query("SELECT d.id FROM issue_documents l JOIN documents d ON d.id=l.document_id WHERE l.issue_id=$1 AND l.key='continuation-summary' FOR UPDATE OF l, d")
            .bind(issue_id)
            .fetch_optional(&mut *tx)
            .await
            .ok()
            .flatten();
        let document_id = if let Some(row) = existing {
            let document_id: Uuid = row.try_get("id").unwrap_or_else(|_| Uuid::new_v4());
            let revision: Option<i32> = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1")
                .bind(document_id).fetch_optional(&mut *tx).await.ok().flatten();
            let revision = revision.unwrap_or(1);
            if sqlx::query("UPDATE documents SET content=$2, content_type='text/markdown', updated_at=NOW() WHERE id=$1")
                .bind(document_id).bind(&body).execute(&mut *tx).await.is_err() { return; }
            if sqlx::query("INSERT INTO document_revisions (document_id, company_id, revision_number, content) VALUES ($1,$2,$3,$4)")
                .bind(document_id).bind(company_id).bind(revision).bind(&body).execute(&mut *tx).await.is_err() { return; }
            document_id
        } else {
            let document_id: Uuid = match sqlx::query_scalar("INSERT INTO documents (company_id, title, content, content_type) VALUES ($1,'Continuation Summary',$2,'text/markdown') RETURNING id")
                .bind(company_id).bind(&body).fetch_one(&mut *tx).await { Ok(id) => id, Err(_) => return };
            if sqlx::query("INSERT INTO issue_documents (company_id, issue_id, document_id, key) VALUES ($1,$2,$3,'continuation-summary')")
                .bind(company_id).bind(issue_id).bind(document_id).execute(&mut *tx).await.is_err() { return; }
            if sqlx::query("INSERT INTO document_revisions (document_id, company_id, revision_number, content) VALUES ($1,$2,1,$3)")
                .bind(document_id).bind(company_id).bind(&body).execute(&mut *tx).await.is_err() { return; }
            document_id
        };
        if tx.commit().await.is_err() { return; }
        tracing::debug!(%issue_id, %run_id, %document_id, "refreshed issue continuation summary");
    }

    /// Publish the successful run's final answer as the normal task comment.
    ///
    /// Run logs are useful execution history, but the Chat tab is driven by
    /// issue comments. Paperclip writes this summary at heartbeat completion;
    /// without it a completed run only appears as a collapsed work item and
    /// the task conversation looks empty.
    async fn post_run_summary_comment(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        status: &str,
        body: Option<&str>,
    ) {
        if status != "succeeded" {
            return;
        }
        let Some(body) = body else {
            return;
        };
        let Some(issue_comment_service) = &self.issue_comment_service else {
            tracing::debug!(%run_id, "issue comment service is not configured; skipping run summary comment");
            return;
        };

        // Match Paperclip's opt-out context flag and make retries/idempotent
        // replays safe when the same terminal run is observed more than once.
        let should_post: bool = sqlx::query_scalar(
            "SELECT COALESCE(context_snapshot->>'skipIssueComment', 'false') <> 'true'
             FROM heartbeat_runs WHERE id = $1 AND company_id = $2",
        )
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(false);
        if !should_post {
            return;
        }

        let already_posted: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM issue_comments
                 WHERE issue_id = $1 AND actor_run_id = $2
            )",
        )
        .bind(issue_id)
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false);
        if already_posted {
            return;
        }

        if let Err(error) = issue_comment_service
            .add_comment_attributed(
                issue_id,
                body.to_string(),
                CommentActorType::Agent,
                Some(agent_id),
                Some(run_id),
                Some(serde_json::json!({ "source": "heartbeat_run" })),
                CommentAttribution::default(),
            )
            .await
        {
            // A comment write must not turn a successfully completed adapter
            // run into a failed run. The run log remains the source of truth
            // for diagnosing a best-effort comment persistence failure.
            tracing::warn!(%run_id, %issue_id, %error, "failed to persist heartbeat run summary comment");
        }
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
        let adapter_metadata = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT adapter_type, adapter_config->>'model'
             FROM agents WHERE id = $1 AND company_id = $2",
        )
        .bind(agent_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| ("unknown".to_string(), None));
        let (adapter_type, configured_model) = adapter_metadata;
        let task_key: String = sqlx::query_scalar(
            "SELECT COALESCE(
                context_snapshot->>'taskKey',
                context_snapshot->>'issueId',
                'agent:' || agent_id::text
             )
             FROM heartbeat_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("issue:{issue_id}"));
        let mut result = self
            .run_command(run_id, agent_id, issue_id, company_id, false)
            .await;
        let mut session_recovery: Option<&'static str> = None;
        let mut session_recovery_session: Option<String> = None;
        if adapter_type == "codex_local" {
            let stale_session = result.as_ref().ok().and_then(|command_output| {
                is_codex_unknown_session_output(command_output)
                    .then(|| command_output.resumed_session_id.clone())
                    .flatten()
            });
            if let Some(session_id) = stale_session {
                session_recovery_session = Some(session_id.clone());
                let task_session_cleared = sqlx::query(
                    "UPDATE agent_task_sessions
                     SET session_display_id = NULL, session_params_json = NULL,
                         last_error = NULL, updated_at = NOW()
                     WHERE company_id = $1 AND agent_id = $2
                       AND adapter_type = $3 AND task_key = $4
                       AND (session_display_id = $5
                            OR session_params_json->>'sessionId' = $5)",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(&adapter_type)
                .bind(&task_key)
                .bind(&session_id)
                .execute(&self.pool)
                .await;
                let runtime_session_cleared = sqlx::query(
                    "UPDATE agent_runtime_states
                     SET session_id = NULL, updated_at = NOW()
                     WHERE agent_id = $1 AND session_id = $2",
                )
                .bind(agent_id)
                .bind(&session_id)
                .execute(&self.pool)
                .await;
                match (task_session_cleared, runtime_session_cleared) {
                    (Ok(task_result), Ok(runtime_result))
                        if task_result.rows_affected() > 0 || runtime_result.rows_affected() > 0 =>
                    {
                        session_recovery = Some("fresh_retry");
                        tracing::warn!(
                            %run_id,
                            %agent_id,
                            %session_id,
                            "codex session is stale; retrying with a fresh session"
                        );
                        result = self
                            .run_command(run_id, agent_id, issue_id, company_id, true)
                            .await;
                    }
                    (Ok(_), Ok(_)) => {
                        session_recovery = Some("cas_conflict");
                        tracing::info!(
                            %run_id,
                            %agent_id,
                            %session_id,
                            "codex stale-session recovery skipped because runtime state changed"
                        );
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        session_recovery = Some("cas_failed");
                        tracing::warn!(
                            %run_id,
                            %agent_id,
                            %session_id,
                            %error,
                            "failed to clear stale Codex session"
                        );
                    }
                }
            }
        }
        // Heartbeat Start Lock contention: a second start for this agent raced in. This is not a
        // run failure and must NOT be retried — cancel this run so exactly one adapter process
        // owns the agent (PAPERCLIP_MIGRATION_PLAN §4B.2 line 324).
        if let Err(error) = &result {
            if error.contains("start_lock_contended") {
                let _ = sqlx::query(
                    "UPDATE heartbeat_runs
                     SET status = 'cancelled', error = 'cancelled: start lock contended',
                         finished_at = NOW(), updated_at = NOW()
                     WHERE id = $1 AND status IN ('queued','running')",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                tracing::warn!(
                    run_id = %run_id,
                    agent_id = %agent_id,
                    "heartbeat run cancelled: another start for this agent already in progress"
                );
                return;
            }
        }
        let (status, exit_code, error, output, mut outcome) = match result {
            Ok(command_output) => {
                let combined = format!("{}{}", command_output.stdout, command_output.stderr);
                let mut outcome = parse_adapter_outcome(&combined, &adapter_type);
                if outcome.model.is_none() {
                    outcome.model = configured_model.clone().filter(|model| !model.trim().is_empty());
                }
                if outcome.provider.is_none() {
                    outcome.provider = match adapter_type.as_str() {
                        "claude_local" => Some("anthropic".to_string()),
                        "codex_local" => Some("openai".to_string()),
                        _ => None,
                    };
                }
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
                    let (error_code, error_family) = classify_adapter_error(&reason, &adapter_type);
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
                let (error_code, error_family) = classify_adapter_error(&error, &adapter_type);
                let outcome = AdapterOutcome {
                    explicit_failure: true,
                    failure_reason: Some(error.clone()),
                    error_code: error_code.or_else(|| Some("adapter_failed".to_string())),
                    error_family: error_family.or_else(|| Some("adapter".to_string())),
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
                        resumed_session_id: None,
                        billing_type: "unknown".to_string(),
                        runtime_secret_manifest: Vec::new(),
                    },
                    outcome,
                )
            }
        };
        // Fill missing cost from official price registry when provider returned
        // token counts but no explicit cost_usd.
        if outcome.cost_usd.is_none() && (outcome.input_tokens > 0 || outcome.output_tokens > 0) {
            outcome.cost_usd = crate::fill_missing_cost(
                None,
                outcome.model.as_deref(),
                outcome.input_tokens,
                outcome.cached_input_tokens,
                outcome.output_tokens,
            );
        }
        let issue_comment_body = build_heartbeat_run_issue_comment(outcome.result_summary.as_deref());
        let result_json = serde_json::json!({
            "toolCallCount": outcome.tool_call_count,
            "resultSummary": outcome.result_summary.as_deref(),
            "summary": outcome.result_summary.as_deref(),
            "handoff": outcome.handoff,
            "explicitFailure": outcome.explicit_failure,
            "errorCode": outcome.error_code,
            "errorFamily": outcome.error_family,
            "resultEvent": outcome.result_event,
            "sessionId": outcome.session_id,
            "inputTokens": outcome.input_tokens,
            "cachedInputTokens": outcome.cached_input_tokens,
            "outputTokens": outcome.output_tokens,
            "costUsd": outcome.cost_usd,
            "model": outcome.model,
            "provider": outcome.provider,
            "adapterType": adapter_type,
            "biller": resolve_biller(&adapter_type, outcome.provider.as_deref()),
            "billingType": output.billing_type.clone(),
            "secretManifest": output.runtime_secret_manifest.clone(),
            "sessionRecovery": session_recovery,
            "sessionRecoverySession": session_recovery_session,
            "stdout": output.stdout,
            "stderr": output.stderr,
        });
        // Persist the terminal result before attempting self-healing. The retry
        // transition intentionally accepts only `failed`, so scheduling before
        // this UPDATE would silently leave recoverable failures terminal.
        if let Err(error) = sqlx::query(
            "UPDATE heartbeat_runs SET status = $2::heartbeat_run_status, exit_code = $3, error = $4, output = $5, result_json = $6, error_code = $7, error_family = $8, finished_at = NOW(), updated_at = NOW() WHERE id = $1 AND status IN ('queued','running')")
            .bind(run_id).bind(status).bind(exit_code).bind(&error).bind(&output.stdout).bind(&result_json).bind(outcome.error_code.clone()).bind(outcome.error_family.clone()).execute(&self.pool).await
        {
            tracing::error!(%run_id, %error, "failed to persist heartbeat run final status");
        }

        self.post_run_summary_comment(
            run_id,
            agent_id,
            issue_id,
            company_id,
            status,
            issue_comment_body.as_deref(),
        )
        .await;

        // Self-healing: a recoverable failure is rescheduled instead of left
        // terminal. maybe_schedule_retry clears finished_at and records the
        // retry metadata after the failure result has been durably captured.
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

        let session_params = serde_json::json!({
            "sessionId": outcome.session_id,
            "adapterType": adapter_type,
            "runId": run_id,
        });
        if let Err(session_error) = persist_agent_task_session(
            &self.pool,
            company_id,
            agent_id,
            &adapter_type,
            &task_key,
            run_id,
            outcome.session_id.as_deref(),
            &session_params,
            (status == "failed").then_some(error.as_deref()).flatten(),
        )
        .await
        {
            tracing::warn!(%run_id, %agent_id, %session_error, "failed to persist agent task session");
        }
        // Correlation chain for Task Chat: `context_snapshot.commentId` on this
        // run identifies the operator comment, and this pair shows which
        // provider session served it. A task key that resumes another task's
        // session is visible as a `session_before` that was never written for
        // this `task_key`.
        tracing::info!(
            %run_id,
            %agent_id,
            %task_key,
            adapter = %adapter_type,
            session_before = output.resumed_session_id.as_deref(),
            session_after = outcome.session_id.as_deref(),
            "heartbeat run provider session"
        );

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
                        biller: resolve_biller(&adapter_type, outcome.provider.as_deref()),
                        billing_type: output.billing_type.clone(),
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

        let final_event = serde_json::json!({
            "runId": run_id,
            "agentId": agent_id,
            "issueId": issue_id,
            "status": status,
            "exitCode": exit_code,
            "error": error,
        });
        if let Err(event_error) = persist_heartbeat_run_event(
            &self.pool,
            company_id,
            run_id,
            agent_id,
            "heartbeat.run.status",
            None,
            (status == "failed").then_some("error"),
            Some(status),
            &final_event,
        )
        .await
        {
            tracing::warn!(%run_id, %event_error, "failed to persist heartbeat terminal event");
        }
        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.status",
            final_event,
        )
        .await;
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
        // A comment that arrived while this run held the agent+issue slot was
        // parked on its own wakeup request. Parked rows carry *this* run's id
        // (that is their "blocked by" marker), so they must be excluded here —
        // otherwise the sweep consumes them and the replay below finds nothing.
        self.complete_run_wakeup_requests(company_id, agent_id, issue_id, run_id)
            .await;
        let _ = sqlx::query("UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW() WHERE run_id = $1 AND revoked_at IS NULL")
            .bind(run_id).execute(&self.pool).await;
        let _ = sqlx::query("UPDATE agents SET status = 'idle', updated_at = NOW() WHERE id = $1 AND status = 'running'")
            .bind(agent_id).execute(&self.pool).await;
        self.children.lock().await.remove(&run_id);
        // A comment that arrived while this run held the agent+issue slot was
        // parked on its wakeup request. Replay it now that the slot is free —
        // synchronously, because the queued row this run occupied is already
        // terminal (updated above), so the replay's own slot check sees it as
        // free. `reconcile_deferred_comment_wakes` starts the follow-up run via
        // `tokio::spawn`, so calling it from here does not recurse.
        if let Err(error) = self
            .reconcile_deferred_comment_wakes(agent_id, company_id)
            .await
        {
            tracing::warn!(%run_id, %agent_id, %error, "failed to replay deferred comment wakes");
        }
    }

    /// Closes the wakeup requests a finished run delivered.
    ///
    /// Parked comment wakes are excluded: they carry the blocking run's id as
    /// their "blocked by" marker, so a plain `run_id = $4` match would consume
    /// them before [`Self::reconcile_deferred_comment_wakes`] can replay them.
    ///
    /// `pub` so the Task Chat integration test can drive the real sweep and
    /// then the real replay in the order `execute_run` uses them; the pair's
    /// ordering is the invariant, and a reimplementation in the test would not
    /// catch a regression here.
    pub async fn complete_run_wakeup_requests(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        issue_id: Uuid,
        run_id: Uuid,
    ) {
        let _ = sqlx::query(
            "UPDATE agent_wakeup_requests SET status = 'completed', updated_at = NOW()
             WHERE company_id = $1 AND agent_id = $2
               AND status IN ('queued','dispatched','running')
               AND payload->>'issueId' = $3
               AND NOT (payload ? 'deferredContext')
               AND (run_id IS NULL OR run_id = $4)",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id.to_string())
        .bind(run_id)
        .execute(&self.pool)
        .await;
    }

    /// Resolves the provider session a run may resume, scoped to its task key.
    ///
    /// A task-scoped run only ever resumes the session recorded for *its*
    /// `task_key`. The agent's global runtime session is reachable only from an
    /// agent-level run (no `taskKey`/`issueId` of its own); letting a task-scoped
    /// run fall back to it is what made three different task keys resume the
    /// same Claude session (`1e5a1e18`), sharing history across unrelated
    /// issues. A task-scoped run with no task session starts fresh instead.
    ///
    /// `pub` so the Task Chat integration test asserts the isolation directly
    /// rather than duplicating this query.
    pub async fn resolve_persisted_session(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        adapter: &str,
        task_key: &str,
        run_id: Uuid,
    ) -> Result<Option<String>, String> {
        let task_session: Option<String> = sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(session_display_id, session_params_json->>'sessionId', '')
             FROM agent_task_sessions
             WHERE company_id = $1 AND agent_id = $2
               AND adapter_type = $3 AND task_key = $4
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(adapter)
        .bind(task_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("failed to load task session: {error}"))?
        .filter(|session| !session.trim().is_empty());
        if task_session.is_some() {
            return Ok(task_session);
        }
        let agent_level_run: bool = sqlx::query_scalar(
            "SELECT COALESCE(context_snapshot->>'taskKey', context_snapshot->>'issueId') IS NULL
             FROM heartbeat_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| format!("failed to load heartbeat task scope: {error}"))?;
        if !agent_level_run {
            tracing::info!(
                %run_id,
                %agent_id,
                %task_key,
                adapter,
                "no task session for task key; starting a fresh provider session"
            );
            return Ok(None);
        }
        sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(session_id, '') FROM agent_runtime_states WHERE agent_id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("failed to load adapter session: {error}"))
        .map(|session| session.filter(|value| !value.trim().is_empty()))
    }

    async fn run_command(
        &self,
        run_id: Uuid,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        force_fresh_session: bool,
    ) -> Result<AdapterCommandOutput, String> {
        // Heartbeat Start Lock (PAPERCLIP_MIGRATION_PLAN §4B.2 line 324): guarantee at most
        // one adapter process starts per agent at a time. Acquire before building/launching the
        // command; release immediately after the child is spawned and registered. The lock rows
        // auto-expire (30s) so a crashed holder cannot wedge the agent.
        let start_lock = crate::agent_start_lock_service::AgentStartLockService::new(self.pool.clone());
        let lock_id = match start_lock.acquire_lock(agent_id, run_id.to_string()).await {
            Ok(id) => id,
            Err(_) => {
                return Err(
                    "another start for this agent is already in progress (start_lock_contended)"
                        .to_string(),
                );
            }
        };

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
        let merged_config = merge_adapter_config(db_config, default_config);
        let mut runtime_secret_paths = HashSet::new();
        let mut runtime_secret_manifest = Vec::new();
        let cfg = if let Some(resolver) = &self.runtime_secret_resolver {
            let responsible_user = sqlx::query_scalar::<_, Option<String>>(
                "SELECT responsible_user_id::text
                 FROM heartbeat_runs
                 WHERE id = $1 AND company_id = $2 AND agent_id = $3",
            )
            .bind(run_id)
            .bind(company_id)
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| format!("failed to load heartbeat responsible user: {error}"))?
            .flatten()
            .map(|value| {
                Uuid::parse_str(&value).map_err(|error| {
                    format!("invalid heartbeat responsible user '{value}': {error}")
                })
            })
            .transpose()?;
            let resolved = resolver
                .resolve_adapter_config(company_id, responsible_user, merged_config)
                .await
                .map_err(|error| format!("runtime credential resolution failed: {error}"))?;
            runtime_secret_paths.extend(resolved.secret_keys);
            runtime_secret_manifest = resolved.manifest;
            resolved.config
        } else {
            merged_config
        };
        let base_prompt = cfg
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
        // The wake context carries comment ids, not bodies. Load them once here
        // so both the argv-based adapters below and the stdin prompt can see the
        // operator's actual feedback instead of only the task template.
        let wake_snapshot: Value = sqlx::query_scalar(
            "SELECT COALESCE(context_snapshot, '{}'::jsonb) FROM heartbeat_runs WHERE id = $1 AND company_id = $2",
        )
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("failed to load heartbeat wake context: {error}"))?
        .unwrap_or_else(|| serde_json::json!({}));
        let wake_comment_ids = crate::wake_prompt_service::extract_wake_comment_ids(&wake_snapshot);
        let wake_reason: Option<String> = wake_snapshot
            .get("wakeReason")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let wake_comments = if wake_comment_ids.is_empty() {
            Vec::new()
        } else {
            crate::wake_prompt_service::load_wake_comments(&self.pool, company_id, &wake_comment_ids)
                .await
                .map_err(|error| format!("failed to load wake comments: {error}"))?
        };
        // Invariant: a comment-shaped wake must name its comment. Without an id
        // the run cannot load the operator's feedback and degrades into a plain
        // assignment turn — the defect this path exists to prevent. No producer
        // currently violates this, so it is a tripwire, not a live branch.
        if wake_comment_ids.is_empty()
            && crate::wake_prompt_service::is_comment_shaped_wake_reason(wake_reason.as_deref())
        {
            tracing::warn!(
                %run_id,
                %agent_id,
                %issue_id,
                wake_reason = wake_reason.as_deref().unwrap_or("unknown"),
                "comment-shaped wake carries no comment id; the operator comment cannot be loaded"
            );
        }
        let issue_identifier: Option<String> =
            sqlx::query_scalar("SELECT identifier FROM issues WHERE id = $1 AND company_id = $2")
                .bind(issue_id)
                .bind(company_id)
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
        let mut rendered_prompt = crate::wake_prompt_service::render_run_prompt(
            base_prompt.clone(),
            false,
            wake_reason.as_deref(),
            issue_identifier.as_deref(),
            &title,
            &wake_comments,
            &wake_comment_ids,
        );
        let mut prompt = rendered_prompt.prompt.clone();
        let configured_model = cfg
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty());
        // Determine execution engine from config (auto/cli/acp). Codex exposes
        // an ACP subcommand; the Claude Code CLI does not expose `--acp`, so
        // Claude stays on its regular print-mode CLI path.
        let engine = cfg
            .get("engine")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| s == "acp" || s == "cli" || s == "auto")
            .unwrap_or_else(|| "auto".to_string());
        let is_acp = resolve_acp_mode(adapter, &engine)?;
        if adapter == "http" {
            let result = self
                .http_executor
                .execute(AdapterExecutionContext {
                    run_id: run_id.to_string(),
                    agent_id: agent_id.to_string(),
                    config: cfg.clone(),
                    working_dir: cfg
                        .get("cwd")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned),
                    execution_target: ExecutionTargetConfig {
                        target_type: ExecutionTargetType::Local,
                        connection_info: None,
                        asset_sync_config: None,
                    },
                    log_sink: None,
                })
                .await;
            let exit_code = result.exit_code.unwrap_or_else(|| {
                if result.status == ExecutionStatus::Ok {
                    0
                } else {
                    -1
                }
            });
            return Ok(AdapterCommandOutput {
                exit_code,
                stdout: result.output,
                stderr: result.error.unwrap_or_default(),
                resumed_session_id: None,
                billing_type: "custom".to_string(),
                runtime_secret_manifest,
            });
        }
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
            let request_body = if adapter == "claude_local" {
                serde_json::json!({
                    "model": model,
                    "max_tokens": cfg.get("maxTokens").and_then(|v| v.as_u64()).unwrap_or(4096),
                    "messages": [{"role":"user","content":prompt}]
                })
            } else {
                serde_json::json!({
                    "model": model,
                    "messages": [{"role":"user","content":prompt}]
                })
            };
            let client = reqwest::Client::builder()
                .timeout(provider_http_timeout(&cfg))
                .build()
                .map_err(|error| format!("failed to build LLM HTTP client: {error}"))?;
            let max_retries = provider_http_retries(&cfg);
            let mut retry_count = 0;
            loop {
                let request = if adapter == "claude_local" {
                    client
                        .post(url)
                        .header("x-api-key", api_key)
                        .header("anthropic-version", "2023-06-01")
                        .json(&request_body)
                } else {
                    client
                        .post(url)
                        .bearer_auth(api_key)
                        .json(&request_body)
                };
                let response = match request.send().await {
                    Ok(response) => response,
                    Err(_error) if retry_count < max_retries => {
                        retry_count += 1;
                        tokio::time::sleep(provider_retry_delay(retry_count)).await;
                        continue;
                    }
                    Err(error) => {
                        return Err(format!(
                            "LLM request failed after {} attempt(s): {error}",
                            retry_count + 1
                        ));
                    }
                };
                let status = response.status();
                let retry_after = provider_retry_after(response.headers());
                let body = match response.text().await {
                    Ok(body) => body,
                    Err(_error) if retry_count < max_retries => {
                        retry_count += 1;
                        tokio::time::sleep(provider_retry_delay_with_hint(
                            retry_count,
                            retry_after,
                        ))
                        .await;
                        continue;
                    }
                    Err(error) => {
                        return Err(format!(
                            "LLM response read failed after {} attempt(s): {error}",
                            retry_count + 1
                        ));
                    }
                };
                if status.is_success() {
                    return Ok(AdapterCommandOutput {
                        exit_code: 0,
                        stdout: body,
                        stderr: String::new(),
                        resumed_session_id: None,
                        billing_type: "api".to_string(),
                        runtime_secret_manifest,
                    });
                }
                if is_retryable_provider_status(status) && retry_count < max_retries {
                    retry_count += 1;
                    tokio::time::sleep(provider_retry_delay_with_hint(
                        retry_count,
                        retry_after,
                    ))
                    .await;
                    continue;
                }
                return Err(format!(
                    "LLM request failed with HTTP {status}: {}",
                    redact_adapter_secret(&body, api_key)
                ));
            }
        }
        let command = cfg
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or(match adapter {
                "claude_local" => "claude",
                "codex_local" => "codex",
                "opencode" | "opencode_local" => "opencode",
                "cursor" => "agent",
                "gemini_local" => "gemini",
                "grok_local" => "grok",
                "pi_local" => "pi",
                "hermes_local" => "hermes",
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
                "codex_local" => build_codex_exec_args(configured_model, is_acp),
                "claude_local" => {
                    if is_acp {
                        vec!["--acp".into(), "--print".into(), "-".into()]
                    } else {
                        vec![
                            "--print".into(),
                            "-".into(),
                            "--output-format".into(),
                            "stream-json".into(),
                            "--verbose".into(),
                        ]
                    }
                }
                _ => vec!["-p".into(), prompt.clone()],
            };
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
            // Catalog-installed agents carry a managed instructions bundle in
            // metadata. When no explicit prompt/file is configured, pass its
            // entry file to Claude directly so the catalog instructions are
            // effective in the runtime rather than only visible in the UI.
            let has_explicit_instruction_arg = args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "--append-system-prompt" | "--append-system-prompt-file" | "--system-prompt"
                )
            });
            let has_explicit_instruction_config = [
                "instructionsFilePath",
                "instructions_file_path",
                "systemPrompt",
                "system_prompt",
                "appendSystemPrompt",
                "append_system_prompt",
            ]
            .iter()
            .any(|key| cfg.get(*key).and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty()));
            if !has_explicit_instruction_arg && !has_explicit_instruction_config {
                if let Some(instructions) = instructions_bundle_entry_content(&agent) {
                    args.extend(["--append-system-prompt".into(), instructions]);
                }
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
                let insert_at = args
                    .iter()
                    .rposition(|arg| arg == "-")
                    .unwrap_or(args.len());
                args.insert(insert_at, "--dangerously-bypass-approvals-and-sandbox".into());
            }
        }
        // A persisted legacy adapter config may still carry `--acp` in its
        // custom args/extraArgs even when `engine` is absent or `auto`. Fail
        // before spawning a process with an actionable message instead of
        // turning the issue into a silent heartbeat failure.
        reject_unsupported_claude_args(adapter, &args)?;
        let task_key: String = sqlx::query_scalar(
            "SELECT COALESCE(
                context_snapshot->>'taskKey',
                context_snapshot->>'issueId',
                'agent:' || agent_id::text
             )
             FROM heartbeat_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| format!("failed to load heartbeat task key: {error}"))?;
        let mut resumed_session_id = None;
        if matches!(adapter, "claude_local" | "codex_local") && !custom_args && !force_fresh_session {
            let persisted_session = self
                .resolve_persisted_session(company_id, agent_id, adapter, &task_key, run_id)
                .await?;
            match adapter {
                "claude_local" => {
                    if let Some(session_id) = valid_claude_resume_session(persisted_session.as_deref()) {
                        resumed_session_id = Some(session_id.clone());
                        args.extend(["--resume".to_string(), session_id]);
                    }
                }
                "codex_local" => {
                    if let Some(session_id) = valid_codex_resume_session(persisted_session.as_deref()) {
                        if args.last().map(String::as_str) == Some("-") {
                            resumed_session_id = Some(session_id.clone());
                            args.pop();
                            args.extend(["resume".to_string(), session_id, "-".to_string()]);
                        }
                    }
                }
                _ => {}
            }
        }
        // A resumed session must not replay the original task template: it
        // already received the brief, so only the new wake delta is sent.
        if resumed_session_id.is_some() {
            rendered_prompt = crate::wake_prompt_service::render_run_prompt(
                base_prompt.clone(),
                true,
                wake_reason.as_deref(),
                issue_identifier.as_deref(),
                &title,
                &wake_comments,
                &wake_comment_ids,
            );
            prompt.clone_from(&rendered_prompt.prompt);
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
        let mut runtime_mcp_env = HashMap::<String, String>::new();
        let mut runtime_mcp_temp_dirs = Vec::new();
        let mut runtime_mcp_workspace_configs = Vec::new();
        match adapter {
            "claude_local" => {
                let config = serde_json::json!({
                    "mcpServers": {
                        "paperclip": claude_mcp_server(&mcp_url, &gateway_token),
                    },
                });
                let (temp_dir, config_path) = write_private_temp_json("parrot-claude-mcp-", &config)?;
                runtime_mcp_temp_dirs.push(temp_dir);
                args.splice(
                    0..0,
                    [
                        "--mcp-config".to_string(),
                        config_path.to_string_lossy().into_owned(),
                        "--strict-mcp-config".to_string(),
                    ],
                );
            }
            "codex_local" => {
                // Codex supports process-scoped `-c` overrides. This avoids
                // mutating a user's persistent config.toml while preserving
                // the same mcp_servers shape used by Paperclip's managed home.
                args.splice(0..0, codex_mcp_overrides("paperclip", &mcp_url));
            }
            "opencode_local" | "opencode" => {
                let existing_config = cfg
                    .get("env")
                    .and_then(|value| value.as_object())
                    .and_then(|env| env.get("OPENCODE_CONFIG_CONTENT"))
                    .and_then(Value::as_str)
                    .map(resolve_env_value)
                    .map(String::into_bytes)
                    .or_else(|| {
                        std::env::var("OPENCODE_CONFIG_CONTENT")
                            .ok()
                            .map(String::into_bytes)
                    });
                let config = merge_opencode_mcp_server(
                    existing_config.as_deref(),
                    "paperclip",
                    &mcp_url,
                    &gateway_token,
                )?;
                runtime_mcp_env.insert(
                    "OPENCODE_CONFIG_CONTENT".to_string(),
                    serde_json::to_string(&config)
                        .map_err(|error| format!("failed to serialize OpenCode MCP config: {error}"))?,
                );
            }
            "gemini_local" => {
                // Gemini exposes a system settings path override. A private
                // run-scoped settings file lets us avoid touching ~/.gemini.
                let config = serde_json::json!({
                    "mcpServers": {
                        "paperclip": gemini_mcp_server(&mcp_url, &gateway_token),
                    },
                });
                let (temp_dir, config_path) = write_private_temp_json("parrot-gemini-mcp-", &config)?;
                runtime_mcp_temp_dirs.push(temp_dir);
                runtime_mcp_env.insert(
                    "GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(),
                    config_path.to_string_lossy().into_owned(),
                );
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
        let stdin_prompt = matches!(adapter, "claude_local" | "codex_local") && !custom_args;
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

        if adapter == "cursor" {
            // Cursor's CLI discovers MCP from the project `.cursor/mcp.json`
            // and has no portable per-process MCP path flag. Preserve the
            // existing file and restore it when this run exits.
            let config_path = PathBuf::from(&effective_cwd).join(".cursor").join("mcp.json");
            let existing = std::fs::read(&config_path).ok();
            let config = merge_mcp_server(
                existing.as_deref(),
                "mcpServers",
                "paperclip",
                cursor_mcp_server(&mcp_url, &gateway_token),
            )?;
            runtime_mcp_workspace_configs.push(
                RestorableJsonConfig::install(config_path, &config).await?,
            );
        }
        
        // Build the exact environment overlay once. The same resolved values
        // are used for both `Command::env` and the diagnostic command string;
        // otherwise the log can describe a different process from the one we
        // actually spawn (especially for values such as `ANTHROPIC_AUTH_TOKEN`
        // configured as `ANTHROPIC_AUTH_TOKEN`).
        let mut effective_env = BTreeMap::<String, String>::new();
        let mut sensitive_env_keys = HashSet::<String>::new();
        effective_env.insert("PAPERCLIP_RUN_ID".to_string(), run_id.to_string());
        effective_env.insert("PAPERCLIP_AGENT_ID".to_string(), agent_id.to_string());
        effective_env.insert("PAPERCLIP_TOOL_GATEWAY_URL".to_string(), gateway_url.clone());
        effective_env.insert(
            "PAPERCLIP_TOOL_GATEWAY_TOKEN".to_string(),
            gateway_token.clone(),
        );
        effective_env.insert(
            "PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION".to_string(),
            format!("Bearer {gateway_token}"),
        );
        for key in [
            "PAPERCLIP_TOOL_GATEWAY_TOKEN",
            "PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION",
        ] {
            sensitive_env_keys.insert(key.to_string());
        }

        if let Some(env) = cfg.get("env").and_then(|value| value.as_object()) {
            for (key, value) in env {
                if let Some(value) = value.as_str() {
                    let resolved_value = if runtime_secret_paths.contains(&format!("env.{key}")) {
                        value.to_owned()
                    } else {
                        resolve_env_value(value)
                    };
                    if runtime_secret_paths.contains(&format!("env.{key}"))
                        || is_sensitive_env_key(key)
                    {
                        sensitive_env_keys.insert(key.clone());
                    }
                    effective_env.insert(key.clone(), resolved_value);
                }
            }
        }
        for (key, value) in &runtime_mcp_env {
            if is_sensitive_env_key(key) {
                sensitive_env_keys.insert(key.clone());
            }
            effective_env.insert(key.clone(), value.clone());
        }

        let shell_command_text = shell_command(
            command,
            &args,
            Some(&effective_cwd),
            stdin_prompt.then_some(prompt.as_str()),
        );
        let mut configured_env_keys = cfg
            .get("env")
            .and_then(|value| value.as_object())
            .map(|env| env.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        configured_env_keys.extend(runtime_mcp_env.keys().cloned());
        configured_env_keys.sort();
        configured_env_keys.dedup();
        let logged_shell_command = redact_gateway_token(&shell_command_text, &gateway_token);
        let logged_argv = std::iter::once(command.to_owned())
            .chain(args.iter().cloned())
            .map(|value| redact_gateway_token(&value, &gateway_token))
            .collect::<Vec<_>>();
        let logged_env = effective_env
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    redact_logged_env_value(
                        value,
                        sensitive_env_keys.contains(key),
                        &gateway_token,
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let full_cmd_with_env = command_with_env(
            &effective_env,
            &sensitive_env_keys,
            &logged_shell_command,
            &gateway_token,
        );
        
        tracing::info!(
            run_id = %run_id,
            agent_id = %agent_id,
            issue_id = %issue_id,
            adapter,
            shell_command = %logged_shell_command,
            argv = ?logged_argv,
            working_dir = %effective_cwd,
            configured_env_keys = ?configured_env_keys,
            resolved_env = ?logged_env,
            final_command = %full_cmd_with_env,
            full_command_with_env = %full_cmd_with_env,
            stdin_prompt,
            prompt_bytes = prompt.len(),
            prompt_source = rendered_prompt.source.as_str(),
            resumed_session = resumed_session_id.as_deref(),
            wake_reason = wake_reason.as_deref(),
            comment_ids = ?rendered_prompt.comment_ids,
            comment_count = rendered_prompt.comment_ids.len(),
            missing_comment_count = rendered_prompt.missing_comment_count,
            "starting local adapter process"
        );
        cmd.args(args)
            .stdin(if stdin_prompt {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .current_dir(&effective_cwd);
        for (key, value) in &effective_env {
            cmd.env(key, value);
        }
        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let child_ref = self.register_child(run_id, child).await;
        sqlx::query("UPDATE heartbeat_runs SET status = 'running', started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND status = 'queued'").bind(run_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        let running_event = serde_json::json!({
            "runId": run_id,
            "agentId": agent_id,
            "issueId": issue_id,
            "status": "running",
        });
        if let Err(event_error) = persist_heartbeat_run_event(
            &self.pool,
            company_id,
            run_id,
            agent_id,
            "heartbeat.run.status",
            None,
            Some("info"),
            Some("running"),
            &running_event,
        )
        .await
        {
            tracing::warn!(%run_id, %event_error, "failed to persist heartbeat running event");
        }
        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.status",
            running_event,
        )
        .await;
        // Release the Heartbeat Start Lock now that the child process is spawned and tracked in
        // self.children (which itself prevents a second concurrent child for this run). The lock
        // only guarded the spawn race; a release failure is non-fatal (rows auto-expire).
        if let Err(e) = start_lock.release_lock(lock_id).await {
            tracing::warn!(run_id = %run_id, agent_id = %agent_id, error = %e, "failed to release agent start lock");
        }
        let mut child = child_ref.lock().await;
        if stdin_prompt {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|e| format!("failed to write adapter prompt: {e}"))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|e| format!("failed to close adapter stdin: {e}"))?;
            }
        }
        let mut stdout = child.stdout.take().ok_or("stdout unavailable")?;
        let mut stderr = child.stderr.take().ok_or("stderr unavailable")?;
        let sequence = Arc::new(AtomicU64::new(0));
        let stdout_service = self.sse_service.clone();
        let stderr_service = self.sse_service.clone();
        let stdout_pool = self.pool.clone();
        let stderr_pool = self.pool.clone();
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
                let event = serde_json::json!({
                    "runId": run_id,
                    "agentId": agent_id,
                    "issueId": issue_id,
                    "seq": seq,
                    "stream": "stdout",
                    "chunk": chunk,
                    "ts": Utc::now(),
                });
                if let Err(event_error) = persist_heartbeat_run_event(
                    &stdout_pool,
                    company_id,
                    run_id,
                    agent_id,
                    "heartbeat.run.log",
                    Some("stdout"),
                    Some("info"),
                    event.get("chunk").and_then(Value::as_str),
                    &event,
                )
                .await
                {
                    tracing::warn!(%run_id, %event_error, "failed to persist heartbeat stdout event");
                }
                publish_live_event(
                    &stdout_service,
                    company_id,
                    "heartbeat.run.log",
                    event,
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
                let event = serde_json::json!({
                    "runId": run_id,
                    "agentId": agent_id,
                    "issueId": issue_id,
                    "seq": seq,
                    "stream": "stderr",
                    "chunk": chunk,
                    "ts": Utc::now(),
                });
                if let Err(event_error) = persist_heartbeat_run_event(
                    &stderr_pool,
                    company_id,
                    run_id,
                    agent_id,
                    "heartbeat.run.log",
                    Some("stderr"),
                    Some("error"),
                    event.get("chunk").and_then(Value::as_str),
                    &event,
                )
                .await
                {
                    tracing::warn!(%run_id, %event_error, "failed to persist heartbeat stderr event");
                }
                publish_live_event(
                    &stderr_service,
                    company_id,
                    "heartbeat.run.log",
                    event,
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
            resumed_session_id,
            billing_type: if matches!(adapter, "claude_local" | "codex_local") {
                "subscription".to_string()
            } else {
                "custom".to_string()
            },
            runtime_secret_manifest,
        })
    }

    /// Keep the spawned process addressable by cancellation and orphan
    /// reconciliation for the whole lifetime of the adapter command.
    async fn register_child(&self, run_id: Uuid, child: Child) -> Arc<Mutex<Child>> {
        let child_ref = Arc::new(Mutex::new(child));
        self.children
            .lock()
            .await
            .insert(run_id, Arc::clone(&child_ref));
        child_ref
    }
}

impl DefaultHeartbeatService {
    /// Enqueue a wakeup from a caller that already owns its request row.
    ///
    /// The internal half of the wakeup funnel: budget gate, throttle gate, and
    /// run creation, without the idempotency claim. The scheduled-retry
    /// promoter claims its own request and calls this directly, so a promoted
    /// retry runs through exactly the same gates as any other wakeup.
    pub(super) async fn wakeup_with_context(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: HeartbeatWakeupOptions,
        idempotency_row_id: Option<Uuid>,
    ) -> Result<Option<Uuid>, HeartbeatError> {
        let _agent = self.load_agent(agent_id).await?;

        // Serialize all enqueue decisions for an agent.  The Paperclip
        // contract is that the durable wake request and its heartbeat run are
        // created as one unit; locking the agent row also closes the race
        // between two callers that both observe no active run.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let locked_agent: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(agent_id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        if locked_agent.is_none() {
            return Err(HeartbeatError::AgentNotFound(agent_id));
        }

        // A second caller may have observed the idempotency row before the
        // first caller finished linking its run.  Re-check the link while the
        // agent lock is held so a very fast first run cannot turn the replay
        // into a second execution.
        if let Some(request_id) = idempotency_row_id {
            let linked_run: Option<Uuid> = sqlx::query_scalar(
                "SELECT run_id FROM agent_wakeup_requests
                  WHERE id = $1 AND run_id IS NOT NULL
                  FOR UPDATE",
            )
            .bind(request_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            if linked_run.is_some() {
                tx.commit()
                    .await
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                return Ok(None);
            }
        }

        let active_run: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, status::text FROM heartbeat_runs
             WHERE company_id = $1 AND agent_id = $2
               AND status IN ('queued','running','scheduled_retry')
               AND (context_snapshot->>'issueId' = $3 OR context_snapshot->>'taskId' = $3)
             ORDER BY created_at DESC
             LIMIT 1
             FOR UPDATE",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        // A scheduled-retry wake re-runs a run that already ended: its snapshot
        // is the retry's own, so it is neither merged into the run in flight nor
        // parked behind it. The promoter holds such a wake back instead
        // (`promote_due_scheduled_retries`) until the agent is free, and leaves
        // the claim in place so the next scan resumes the retry.
        if active_run.is_some() && options.retry_of_run_id.is_some() {
            tx.commit()
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            return Ok(None);
        }
        if let Some((active_run_id, active_status)) = active_run {
            // A wake that arrives while a run is already in flight must not be
            // dropped, or the operator's comment is silently lost.
            //
            //  - `queued` / `scheduled_retry`: the run has not built its prompt
            //    yet, so its context is merged in place and the later run
            //    delivers the comment. A second comment therefore coalesces
            //    into the same pending turn instead of queueing a third.
            //  - `running`: the prompt is already on the child's stdin and the
            //    snapshot it read is gone. The comment cannot reach this turn,
            //    so a follow-up queued run is created and promoted when the
            //    current run terminates.
            let incoming_context = options
                .context_snapshot
                .clone()
                .unwrap_or_else(|| serde_json::json!({}));
            let incoming_comment_ids =
                crate::wake_prompt_service::extract_wake_comment_ids(&incoming_context);
            if incoming_comment_ids.is_empty() {
                if let Some(request_id) = idempotency_row_id {
                    sqlx::query(
                        "UPDATE agent_wakeup_requests
                         SET status = 'dispatched', run_id = $2, updated_at = NOW()
                         WHERE id = $1",
                    )
                    .bind(request_id)
                    .bind(active_run_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                }
                tx.commit()
                    .await
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                return Ok(None);
            }

            let existing_context: Value = sqlx::query_scalar(
                "SELECT COALESCE(context_snapshot, '{}'::jsonb) FROM heartbeat_runs
                 WHERE id = $1 FOR UPDATE",
            )
            .bind(active_run_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;

            if active_status == "running" {
                // A second `queued`/`running` row for the same agent + issue is
                // forbidden by `idx_heartbeat_runs_unique_active_agent_issue`
                // (migration 18, which deleted exactly the duplicates a
                // follow-up row would recreate). The comment instead stays on a
                // wakeup request parked as `queued` and is replayed once this
                // run releases the agent+issue slot.
                //
                // Exactly one row may be parked per blocking run, because the
                // replay turns each parked row into its own run. A comment that
                // arrives after the parked row exists therefore extends it
                // rather than parking a second one.
                let parked = sqlx::query_as::<_, (Uuid, Value)>(
                    "SELECT id, COALESCE(payload, '{}'::jsonb) FROM agent_wakeup_requests
                     WHERE company_id = $1 AND agent_id = $2 AND status = 'queued'
                       AND payload->>'deferredBehindRunId' = $3
                     ORDER BY requested_at DESC
                     LIMIT 1
                     FOR UPDATE",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(active_run_id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;

                // Parked-context seeding, aligned with Paperclip's deferred
                // wake path (`heartbeat.ts:18359-18408`): the row that waits
                // behind a running run carries the context of the wake that is
                // parking, extended by later wakes — never the running run's
                // `contextSnapshot`. Seeding from the blocking run would replay
                // that run's own wake (its template, its assignment marker)
                // as if a human had just sent it.
                //
                // Comments still riding on the parked row are re-emitted so a
                // batch coalesced before the row is replayed is not lost; those
                // that this running run already received are dropped, because
                // Parrot delivers them into the run's prompt
                // (`render_run_prompt`) and re-sending them would duplicate the
                // operator's words. Paperclip has no equivalent filter: its
                // running turn always keeps its own snapshot and never drains
                // from the parked row, so it has nothing to subtract.
                let base_context: Value = match parked.as_ref() {
                    Some((_, parked_payload)) => parked_payload
                        .get("deferredContext")
                        .filter(|value| value.is_object())
                        .cloned()
                        .unwrap_or_else(|| incoming_context.clone()),
                    None => incoming_context.clone(),
                };
                let delivered_ids =
                    crate::wake_prompt_service::extract_wake_comment_ids(&existing_context);
                let undelivered: Vec<Uuid> =
                    crate::wake_prompt_service::extract_wake_comment_ids(&base_context)
                        .into_iter()
                        .filter(|id| !delivered_ids.contains(id))
                        .collect();
                let mut base_context = base_context;
                if let Some(object) = base_context.as_object_mut() {
                    match undelivered.last() {
                        Some(latest) => {
                            object.insert(
                                "wakeCommentIds".to_string(),
                                serde_json::json!(undelivered),
                            );
                            object
                                .insert("wakeCommentId".to_string(), serde_json::json!(latest));
                            object.insert("commentId".to_string(), serde_json::json!(latest));
                        }
                        None => {
                            object.remove("wakeCommentIds");
                            object.remove("wakeCommentId");
                            object.remove("commentId");
                        }
                    }
                    // The parked wake's reason/source describe the wake that is
                    // waiting, not the run it waits behind.
                    if !incoming_comment_ids.is_empty() {
                        if let Some(reason) = options.reason.as_ref() {
                            object.insert("wakeReason".to_string(), serde_json::json!(reason));
                        }
                        if let Some(source) = options.source.as_ref() {
                            object.insert("source".to_string(), serde_json::json!(source));
                        }
                    }
                }
                let parked_context = crate::wake_prompt_service::merge_wake_context(
                    &base_context,
                    &incoming_context,
                );

                match parked {
                    Some((parked_id, mut parked_payload)) => {
                        if let Some(object) = parked_payload.as_object_mut() {
                            object.insert(
                                "deferredContext".to_string(),
                                parked_context,
                            );
                        } else {
                            parked_payload = serde_json::json!({
                                "deferredContext": parked_context,
                            });
                        }
                        sqlx::query(
                            "UPDATE agent_wakeup_requests
                             SET payload = $2, reason = COALESCE($3, reason),
                                 coalesced_count = coalesced_count + 1,
                                 requested_at = NOW(), updated_at = NOW()
                             WHERE id = $1",
                        )
                        .bind(parked_id)
                        .bind(&parked_payload)
                        .bind(options.reason.as_deref())
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                        // The incoming comment now lives on the parked row, so
                        // its own row is closed against this run instead of
                        // being parked a second time.
                        if let Some(request_id) = idempotency_row_id {
                            let mut payload =
                                options.payload.clone().unwrap_or_else(|| serde_json::json!({}));
                            if let Some(object) = payload.as_object_mut() {
                                object.insert(
                                    "issueId".to_string(),
                                    serde_json::json!(issue_id),
                                );
                                object.insert(
                                    "deferredIntoRequestId".to_string(),
                                    serde_json::json!(parked_id),
                                );
                            }
                            sqlx::query(
                                "UPDATE agent_wakeup_requests
                                 SET status = 'dispatched', run_id = $2, payload = $3,
                                     reason = COALESCE($4, reason),
                                     coalesced_count = coalesced_count + 1,
                                     updated_at = NOW()
                                 WHERE id = $1",
                            )
                            .bind(request_id)
                            .bind(active_run_id)
                            .bind(&payload)
                            .bind(options.reason.as_deref())
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                        }
                        tracing::info!(
                            %active_run_id,
                            %parked_id,
                            %agent_id,
                            %issue_id,
                            comment_ids = ?incoming_comment_ids,
                            "extended the parked comment wake behind the running run"
                        );
                        tx.commit()
                            .await
                            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                        return Ok(None);
                    }
                    None => {}
                }

                let mut payload = options.payload.clone().unwrap_or_else(|| serde_json::json!({}));
                if let Some(object) = payload.as_object_mut() {
                    object.insert("issueId".to_string(), serde_json::json!(issue_id));
                    object.insert(
                        "deferredBehindRunId".to_string(),
                        serde_json::json!(active_run_id),
                    );
                    object.insert("deferredContext".to_string(), parked_context);
                }
                match idempotency_row_id {
                    Some(request_id) => {
                        sqlx::query(
                            "UPDATE agent_wakeup_requests
                             SET status = 'queued', run_id = $2, payload = $3,
                                 source = COALESCE($4, source),
                                 trigger_detail = COALESCE($5, trigger_detail),
                                 reason = COALESCE($6, reason),
                                 requested_by_actor_type = COALESCE($7, requested_by_actor_type),
                                 requested_by_actor_id = COALESCE($8, requested_by_actor_id),
                                 requested_at = NOW(), claimed_at = NULL, finished_at = NULL,
                                 updated_at = NOW()
                             WHERE id = $1",
                        )
                        .bind(request_id)
                        .bind(active_run_id)
                        .bind(&payload)
                        .bind(options.source.as_deref())
                        .bind(options.trigger_detail.as_deref())
                        .bind(options.reason.as_deref())
                        .bind(options.requested_by_actor_type.as_deref())
                        .bind(options.requested_by_actor_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                    }
                    None => {
                        sqlx::query(
                            "INSERT INTO agent_wakeup_requests
                             (company_id, agent_id, status, payload, source, trigger_detail,
                              reason, requested_by_actor_type, requested_by_actor_id,
                              run_id, coalesced_count, requested_at, updated_at)
                             VALUES ($1, $2, 'queued', $3, $4, $5, $6, $7, $8, $9, 1, NOW(), NOW())",
                        )
                        .bind(company_id)
                        .bind(agent_id)
                        .bind(&payload)
                        .bind(options.source.as_deref().unwrap_or("on_demand"))
                        .bind(options.trigger_detail.as_deref())
                        .bind(options.reason.as_deref())
                        .bind(options.requested_by_actor_type.as_deref())
                        .bind(options.requested_by_actor_id)
                        .bind(active_run_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                    }
                }
                tracing::info!(
                    %active_run_id,
                    %agent_id,
                    %issue_id,
                    comment_ids = ?incoming_comment_ids,
                    request_id = ?idempotency_row_id,
                    "deferred comment wake behind the running run"
                );
                tx.commit()
                    .await
                    .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                return Ok(None);
            }

            let merged_context = crate::wake_prompt_service::merge_wake_context(
                &existing_context,
                &incoming_context,
            );
            sqlx::query(
                "UPDATE heartbeat_runs SET context_snapshot = $2, updated_at = NOW() WHERE id = $1",
            )
            .bind(active_run_id)
            .bind(&merged_context)
            .execute(&mut *tx)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            tracing::info!(
                %active_run_id,
                %agent_id,
                %issue_id,
                comment_ids = ?incoming_comment_ids,
                active_status = %active_status,
                "coalesced comment wake into pending run"
            );

            // Record the delivery so the comment is auditable as consumed.
            let mut payload = options.payload.clone().unwrap_or_else(|| serde_json::json!({}));
            if let Some(object) = payload.as_object_mut() {
                object.insert("issueId".to_string(), serde_json::json!(issue_id));
                object.insert("runId".to_string(), serde_json::json!(active_run_id));
            }
            if let Some(request_id) = idempotency_row_id {
                sqlx::query(
                    "UPDATE agent_wakeup_requests
                     SET status = 'dispatched', run_id = $2, payload = $3,
                         reason = COALESCE($4, reason),
                         coalesced_count = coalesced_count + 1, updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(request_id)
                .bind(active_run_id)
                .bind(&payload)
                .bind(options.reason.as_deref())
                .execute(&mut *tx)
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            } else {
                sqlx::query(
                    "INSERT INTO agent_wakeup_requests
                     (company_id, agent_id, status, payload, source, trigger_detail,
                      reason, requested_by_actor_type, requested_by_actor_id,
                      run_id, coalesced_count, requested_at, updated_at)
                     VALUES ($1, $2, 'dispatched', $3, $4, $5, $6, $7, $8, $9, 1, NOW(), NOW())",
                )
                .bind(company_id)
                .bind(agent_id)
                .bind(&payload)
                .bind(options.source.as_deref().unwrap_or("on_demand"))
                .bind(options.trigger_detail.as_deref())
                .bind(options.reason.as_deref())
                .bind(options.requested_by_actor_type.as_deref())
                .bind(options.requested_by_actor_id)
                .bind(active_run_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            }
            tx.commit()
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
            return Ok(None);
        }

        let mut context = options
            .context_snapshot
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = context.as_object_mut() {
            object.insert("issueId".to_string(), serde_json::json!(issue_id));
            object
                .entry("taskKey".to_string())
                .or_insert_with(|| serde_json::json!(format!("issue:{issue_id}")));
        } else {
            context = serde_json::json!({
                "issueId": issue_id,
                "taskKey": format!("issue:{issue_id}"),
            });
        }
        // Enrich the wake context with plan-review state, mirroring Paperclip's
        // `heartbeat.ts` call to `buildPlanReviewContext`. Best-effort: a context
        // build failure must never block the wake.
        if let Some(object) = context.as_object_mut() {
            let include_for_issue_comment = object.get("commentId").is_some();
            let include_for_annotation_delta = object.get("annotationDeltas").is_some();
            let interaction_id = object
                .get("interactionId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());
            // Paperclip gates on `issueWorkMode === "planning"` in addition to the
            // comment/annotation/interaction markers, so read it from the issue.
            let issue_work_mode: Option<String> = sqlx::query_scalar(
                "SELECT work_mode::text FROM issues WHERE id = $1 AND company_id = $2",
            )
            .bind(issue_id)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
            let planning = issue_work_mode.as_deref() == Some("planning");
            if planning
                || include_for_issue_comment
                || include_for_annotation_delta
                || interaction_id.is_some()
            {
                let input = crate::plan_review_context_service::BuildPlanReviewContextInput {
                    company_id,
                    issue_id,
                    interaction_id,
                    include_for_issue_comment,
                    include_for_annotation_delta,
                    issue_work_mode,
                };
                match crate::plan_review_context_service::build_plan_review_context(
                    &self.pool,
                    &input,
                )
                .await
                {
                    Ok(Some(plan_review)) => {
                        if let Ok(value) = serde_json::to_value(&plan_review) {
                            object.insert("planReviewContext".to_string(), value);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!("plan review context unavailable for issue {issue_id}: {e}");
                    }
                }
            }
        }
        let run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot, responsible_user_id, retry_of_run_id, scheduled_retry_attempt)
             SELECT $1, $2, $3, 'queued'::heartbeat_run_status, $4, i.responsible_user_id::text, $6, COALESCE($7, 0)
             FROM issues i WHERE i.id = $5
             UNION ALL
             SELECT $1, $2, $3, 'queued'::heartbeat_run_status, $4, NULL::text, $6, COALESCE($7, 0)
             WHERE NOT EXISTS (SELECT 1 FROM issues WHERE id = $5)
             LIMIT 1
             RETURNING id",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(options.source.as_deref().unwrap_or("on_demand"))
        .bind(&context)
        .bind(issue_id)
        .bind(options.retry_of_run_id)
        .bind(options.scheduled_retry_attempt)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;

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
                     run_id = $8, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(request_id)
            .bind(&payload)
            .bind(options.source.as_deref().unwrap_or("on_demand"))
            .bind(options.trigger_detail.as_deref())
            .bind(options.reason.as_deref())
            .bind(options.requested_by_actor_type.as_deref())
            .bind(options.requested_by_actor_id)
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        } else {
            sqlx::query(
                "INSERT INTO agent_wakeup_requests
                 (company_id, agent_id, status, payload, source, trigger_detail, reason,
                  requested_by_actor_type, requested_by_actor_id, run_id, requested_at, updated_at)
                 VALUES ($1,$2,'dispatched',$3,$4,$5,$6,$7,$8,$9,NOW(),NOW())",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(&payload)
            .bind(options.source.as_deref().unwrap_or("on_demand"))
            .bind(options.trigger_detail.as_deref())
            .bind(options.reason.as_deref())
            .bind(options.requested_by_actor_type.as_deref())
            .bind(options.requested_by_actor_id)
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        }
        sqlx::query("UPDATE issues SET assignee_agent_id = $2, assignee_user_id = NULL, status = CASE WHEN status IN ('todo','backlog') THEN 'in_progress'::issue_status ELSE status END, checkout_run_id = $3, execution_run_id = $3, started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND company_id = $4 AND (assignee_agent_id IS NULL OR assignee_agent_id = $2) AND status NOT IN ('done','cancelled')")
            .bind(issue_id).bind(agent_id).bind(run_id).bind(company_id)
            .execute(&mut *tx).await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        sqlx::query("UPDATE agents SET status = 'running', updated_at = NOW() WHERE id = $1")
            .bind(agent_id).execute(&mut *tx).await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let queued_event = serde_json::json!({
            "runId": run_id,
            "agentId": agent_id,
            "issueId": issue_id,
            "status": "queued",
            "invocationSource": "on_demand",
        });
        if let Err(event_error) = persist_heartbeat_run_event(
            &self.pool,
            company_id,
            run_id,
            agent_id,
            "heartbeat.run.queued",
            None,
            Some("info"),
            Some("queued"),
            &queued_event,
        )
        .await
        {
            tracing::warn!(%run_id, %event_error, "failed to persist heartbeat queued event");
        }
        publish_live_event(
            &self.sse_service,
            company_id,
            "heartbeat.run.queued",
            queued_event,
        )
        .await;
        let service = self.clone_for_task();
        tokio::spawn(async move { service.execute_run(run_id, agent_id, issue_id, company_id).await; });
        Ok(Some(run_id))
    }

    /// Claim (or create) the `agent_wakeup_requests` row for a wake, without
    /// dispatching anything.
    ///
    /// The same insert-or-claim logic the wakeup funnel starts with, split out
    /// so a caller can own the request lifecycle around a dispatch it drives
    /// itself — Paperclip likewise keeps a wakeup request as the durable owner
    /// of a wake across enqueue, dispatch, and skip (`heartbeat.ts:10025`).
    ///
    /// `Ok(None)` means an earlier wake under the same idempotency key already
    /// dispatched a run, so this wake is a no-op.
    async fn claim_wakeup_request(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: &HeartbeatWakeupOptions,
    ) -> Result<Option<Uuid>, HeartbeatError> {
        let Some(idempotency_key) = options.idempotency_key.as_deref() else {
            return Ok(None);
        };
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let inserted = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO agent_wakeup_requests
             (company_id, agent_id, status, payload, source, trigger_detail, reason,
              requested_by_actor_type, requested_by_actor_id, idempotency_key,
              requested_at, updated_at)
             VALUES ($1, $2, 'queued', $3, COALESCE($4, 'on_demand'), $5, $6, $7, $8, $9, NOW(), NOW())
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
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let row = match inserted {
            Some(row_id) => Some(row_id),
            None => {
                let existing: Option<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
                    "SELECT id, status::text, run_id
                     FROM agent_wakeup_requests
                     WHERE company_id = $1 AND idempotency_key = $2
                     FOR UPDATE",
                )
                .bind(company_id)
                .bind(idempotency_key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                let Some((existing_id, status, existing_run_id)) = existing else {
                    return Err(HeartbeatError::WakeupFailed(
                        "idempotency conflict did not resolve to a wake request".to_string(),
                    ));
                };
                if matches!(status.as_str(), "dispatched" | "running" | "completed")
                    || (status == "queued" && existing_run_id.is_some())
                {
                    None
                } else {
                    if matches!(status.as_str(), "failed" | "cancelled" | "skipped") {
                        sqlx::query(
                            "UPDATE agent_wakeup_requests
                             SET status = 'queued', payload = $3,
                                 source = COALESCE($4, source), trigger_detail = $5,
                                 reason = $6, requested_by_actor_type = $7,
                                 requested_by_actor_id = $8, run_id = NULL,
                                 error = NULL, finished_at = NULL,
                                 requested_at = NOW(), updated_at = NOW()
                             WHERE id = $1 AND status = $2::agent_wakeup_request_status",
                        )
                        .bind(existing_id)
                        .bind(&status)
                        .bind(serde_json::json!({ "issueId": issue_id }))
                        .bind(options.source.as_deref())
                        .bind(options.trigger_detail.as_deref())
                        .bind(options.reason.as_deref())
                        .bind(options.requested_by_actor_type.as_deref())
                        .bind(options.requested_by_actor_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
                    }
                    Some(existing_id)
                }
            }
        };
        tx.commit()
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        Ok(row)
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
            .bind(options.source.as_deref().unwrap_or("on_demand"))
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
        let has_idempotency_key = options.idempotency_key.is_some();
        let idempotency_row_id = self
            .claim_wakeup_request(agent_id, issue_id, company_id, &options)
            .await?;
        // A request that already dispatched a run under this key owns the wake:
        // re-enqueueing it is a no-op rather than a second run.
        if idempotency_row_id.is_none() && has_idempotency_key {
            return Ok(());
        }

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
                        | "interaction_continuation_backstop"
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
                        .bind(options.source.as_deref().unwrap_or("on_demand"))
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
        result.map(|_: Option<Uuid>| ())
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
        .map(|_: Option<Uuid>| ())
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
            self.http_executor.cancel(&run_id.to_string()).await;
            
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
                 SET status = 'failed', error = $2, error_code = 'process_lost',
                     error_family = 'process', finished_at = NOW(), updated_at = NOW(),
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

            let process_lost_event = serde_json::json!({
                "runId": run_id,
                "agentId": agent_id,
                "issueId": issue_id,
                "status": "failed",
                "error": error,
                "errorCode": "process_lost",
            });
            if let Err(event_error) = persist_heartbeat_run_event(
                &self.pool,
                company_id,
                run_id,
                agent_id,
                "heartbeat.run.status",
                None,
                Some("error"),
                Some("process lost"),
                &process_lost_event,
            )
            .await
            {
                tracing::warn!(%run_id, %event_error, "failed to persist process-lost heartbeat event");
            }
            publish_live_event(
                &self.sse_service,
                company_id,
                "heartbeat.run.status",
                process_lost_event,
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
                    // `invocation_source` records how the run was invoked;
                    // "heartbeat.recovery" stays in context_snapshot.source.
                    source: Some("automation".to_string()),
                    trigger_detail: Some("heartbeat_recovery".to_string()),
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

    /// Heal issues whose interaction continuation wake was lost.
    ///
    /// The interaction resolution path wakes the assignee best-effort: a wakeup
    /// failure is logged and the resolution still succeeds, which is the right
    /// call for the request but leaves the issue parked with no execution path
    /// if the wake was the only thing that would have resumed it. This backstop
    /// finds resolved interactions whose continuation policy requires a wake,
    /// where no live run or pending wake covers the issue, and re-queues the
    /// wake with a stable idempotency key so it runs exactly once.
    pub async fn reconcile_interaction_continuation_wakeups(&self) -> Result<usize, HeartbeatError> {
        let rows = sqlx::query(
            r#"
            SELECT interaction.id AS interaction_id,
                   interaction.issue_id,
                   interaction.kind,
                   interaction.status::text AS interaction_status,
                   interaction.continuation_policy,
                   issue.assignee_agent_id,
                   issue.company_id
              FROM issue_thread_interactions interaction
              JOIN issues issue ON issue.id = interaction.issue_id
             WHERE interaction.status IN ('accepted', 'rejected', 'answered', 'cancelled')
               AND (
                   interaction.continuation_policy = 'wake_assignee'
                   OR (
                       interaction.continuation_policy = 'wake_assignee_on_accept'
                       AND interaction.status = 'accepted'
                   )
               )
               AND issue.assignee_agent_id IS NOT NULL
               AND issue.status NOT IN ('done', 'cancelled')
               AND NOT EXISTS (
                   SELECT 1
                     FROM issue_tree_hold_members held_member
                     JOIN issue_tree_holds hold ON hold.id = held_member.hold_id
                    WHERE held_member.issue_id = issue.id
                      AND hold.company_id = issue.company_id
                      AND hold.status = 'active'
                      AND hold.mode = 'pause'
               )
               AND NOT EXISTS (
                   SELECT 1
                     FROM heartbeat_runs live
                    WHERE live.company_id = issue.company_id
                      AND live.agent_id = issue.assignee_agent_id
                      AND live.status IN ('queued', 'running', 'scheduled_retry')
                      AND (live.context_snapshot->>'issueId' = issue.id::text
                           OR live.context_snapshot->>'taskId' = issue.id::text)
               )
               AND NOT EXISTS (
                   SELECT 1
                     FROM agent_wakeup_requests wake
                    WHERE wake.company_id = issue.company_id
                      AND wake.agent_id = issue.assignee_agent_id
                      AND (
                          wake.payload->>'interactionId' = interaction.id::text
                          OR wake.idempotency_key =
                             'interaction_continuation_backstop:' || interaction.id::text
                      )
                      AND wake.status IN ('queued', 'dispatched', 'running', 'completed')
               )
             ORDER BY interaction.id
             LIMIT 500
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut healed = 0;
        for row in rows {
            let interaction_id: Uuid = row
                .try_get("interaction_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let issue_id: Uuid = row
                .try_get("issue_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let agent_id: Uuid = row
                .try_get("assignee_agent_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let kind: String = row
                .try_get("kind")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let interaction_status: String = row
                .try_get("interaction_status")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let continuation_policy: String = row
                .try_get("continuation_policy")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

            let idempotency_key = format!("interaction_continuation_backstop:{interaction_id}");
            self.wakeup_with_options(
                agent_id,
                issue_id,
                company_id,
                HeartbeatWakeupOptions {
                    source: Some("automation".to_string()),
                    trigger_detail: Some("system".to_string()),
                    reason: Some("interaction_continuation_backstop".to_string()),
                    idempotency_key: Some(idempotency_key.clone()),
                    payload: Some(serde_json::json!({
                        "issueId": issue_id,
                        "interactionId": interaction_id,
                        "interactionKind": kind,
                        "interactionStatus": interaction_status,
                        "continuationPolicy": continuation_policy,
                        "mutation": "interaction",
                        "backstop": "interaction_continuation_reconciliation",
                    })),
                    context_snapshot: Some(serde_json::json!({
                        "issueId": issue_id,
                        "taskId": issue_id,
                        "interactionId": interaction_id,
                        "interactionKind": kind,
                        "interactionStatus": interaction_status,
                        "continuationPolicy": continuation_policy,
                        "wakeReason": "interaction_continuation_backstop",
                        "source": "issue.interaction.backstop",
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
                | Some("codex_auth_required")
                | Some("codex_transient_upstream")
                | Some("codex_malformed_response")
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
        let current_attempt = row.map(|r| r.0).unwrap_or(0);
        if current_attempt >= Self::MAX_SCHEDULED_RETRY_ATTEMPTS {
            return Ok(false);
        }
        let attempt = current_attempt + 1;
        let backoff = Self::scheduled_retry_backoff_secs(attempt);
        let updated = sqlx::query(
            "UPDATE heartbeat_runs
             SET status = 'scheduled_retry'::heartbeat_run_status,
                 scheduled_retry_at = NOW() + ($2 || ' seconds')::interval,
                 scheduled_retry_attempt = $3,
                 scheduled_retry_reason = $4,
                 error_code = COALESCE($5, error_code),
                 error_family = COALESCE($6, error_family),
                 finished_at = NULL,
                 result_json = COALESCE(result_json, '{}'::jsonb)
                     || jsonb_build_object('scheduledRetry', true, 'scheduledRetryAttempt', $3),
                 updated_at = NOW()
             WHERE id = $1 AND status = 'failed'
               AND COALESCE(scheduled_retry_attempt, 0) = $7",
        )
        .bind(run_id)
        .bind(backoff)
        .bind(attempt)
        .bind(reason)
        .bind(error_code)
        .bind(error_family)
        .bind(current_attempt)
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

    /// Replays comment wakes that were parked because their issue already had
    /// an active run.
    ///
    /// A comment arriving on a running turn cannot reach that turn (its prompt
    /// is already on the child's stdin) and cannot get a follow-up run
    /// (`idx_heartbeat_runs_unique_active_agent_issue` forbids a second
    /// `queued`/`running` row for the same agent + issue). It waits on its
    /// `agent_wakeup_requests` row instead, and is replayed here once the
    /// blocking run is gone.
    ///
    /// Idempotent: the parked row is only replayed while its blocking run is
    /// terminal, and `wakeup_with_options` refuses to start a second run while
    /// one is active, so a double scan cannot double-deliver a comment.
    pub async fn reconcile_deferred_comment_wakes(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
    ) -> Result<usize, HeartbeatError> {
        const MAX_DEFERRED_ATTEMPTS: i32 = 5;

        let rows = sqlx::query(
            "SELECT id, payload, source, trigger_detail, reason,
                    requested_by_actor_type, requested_by_actor_id, attempt_count
             FROM agent_wakeup_requests
             WHERE company_id = $1 AND agent_id = $2
               AND status = 'queued' AND run_id IS NOT NULL
               AND EXISTS (
                     SELECT 1 FROM heartbeat_runs r
                     WHERE r.id = agent_wakeup_requests.run_id
                       AND r.status NOT IN ('queued','running')
                   )
             ORDER BY requested_at ASC
             LIMIT 50",
        )
        .bind(company_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut replayed = 0;
        for row in rows {
            let request_id: Uuid = row
                .try_get("id")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let attempt_count: i32 = row
                .try_get("attempt_count")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            let payload: Value = row
                .try_get("payload")
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            if attempt_count >= MAX_DEFERRED_ATTEMPTS {
                sqlx::query(
                    "UPDATE agent_wakeup_requests
                     SET status = 'failed',
                         error = COALESCE(error, 'deferred comment wake exhausted its replay attempts'),
                         finished_at = NOW(), updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(request_id)
                .execute(&self.pool)
                .await
                .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
                tracing::warn!(%request_id, %agent_id, %attempt_count, "gave up replaying a deferred comment wake");
                continue;
            }

            let Some(issue_id) = payload
                .get("issueId")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
            else {
                continue;
            };

            // The parked row carries the snapshot the comment was merged into;
            // the replay builds a brand-new run, so replay it explicitly.
            let context_snapshot = payload.get("deferredContext").cloned();
            let mut replay_payload = payload.clone();
            if let Some(object) = replay_payload.as_object_mut() {
                object.remove("deferredContext");
                object.remove("deferredBehindRunId");
            }

            let options = HeartbeatWakeupOptions {
                source: row.try_get("source").ok(),
                trigger_detail: row.try_get("trigger_detail").ok(),
                reason: row.try_get("reason").ok(),
                requested_by_actor_type: row.try_get("requested_by_actor_type").ok(),
                requested_by_actor_id: row.try_get("requested_by_actor_id").ok(),
                context_snapshot,
                payload: Some(replay_payload),
                // Crash safety: if this process dies after the funnel enqueues
                // but before the parked row is closed, the next scan must not
                // deliver the same comment twice.
                idempotency_key: Some(format!("deferred-comment-wake:{request_id}")),
                ..Default::default()
            };

            // The replay is a fresh wake, so the parked row is released and the
            // funnel records an equivalent one. Bump the attempt count first so
            // a wake that fails to enqueue is still bounded.
            sqlx::query(
                "UPDATE agent_wakeup_requests
                 SET attempt_count = attempt_count + 1, updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(request_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

            if let Err(error) = self
                .wakeup_with_options(agent_id, issue_id, company_id, options)
                .await
            {
                tracing::warn!(%request_id, %agent_id, %issue_id, %error, "failed to replay a deferred comment wake");
                continue;
            }

            sqlx::query(
                "UPDATE agent_wakeup_requests
                 SET status = 'completed', finished_at = NOW(), updated_at = NOW()
                 WHERE id = $1 AND status = 'queued'",
            )
            .bind(request_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

            tracing::info!(%request_id, %agent_id, %issue_id, "replayed a deferred comment wake");
            replayed += 1;
        }

        Ok(replayed)
    }

    /// Scans every agent for parked comment wakes whose blocking run is gone.
    ///
    /// The safety net behind the run-completion replay: if a process dies
    /// between a run finishing and its parked wake being replayed, the periodic
    /// recovery job picks it up. Also the mechanism that retries a replay whose
    /// enqueue attempt failed.
    pub async fn reconcile_deferred_comment_wakes_for_all_agents(
        &self,
    ) -> Result<usize, HeartbeatError> {
        let agents: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT DISTINCT company_id, agent_id FROM agent_wakeup_requests
             WHERE status = 'queued' AND run_id IS NOT NULL
               AND EXISTS (
                     SELECT 1 FROM heartbeat_runs r
                     WHERE r.id = agent_wakeup_requests.run_id
                       AND r.status NOT IN ('queued','running')
                   )
             LIMIT 200",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))?;

        let mut replayed = 0;
        for (company_id, agent_id) in agents {
            match self
                .reconcile_deferred_comment_wakes(agent_id, company_id)
                .await
            {
                Ok(count) => replayed += count,
                Err(error) => {
                    tracing::warn!(%agent_id, %company_id, %error, "deferred comment wake scan failed for agent");
                }
            }
        }
        Ok(replayed)
    }

    /// Self-healing promotion: find `scheduled_retry` runs whose
    /// `scheduled_retry_at` is due and relaunch them through the wakeup funnel.
    ///
    /// Mirrors Paperclip's `promoteDueScheduledRetry` (`heartbeat.ts:11105`):
    /// a due retry is promoted by **re-enqueueing a wakeup for its issue**
    /// (`scheduleBoundedRetryForRun` → `enqueueWakeupForIssue`), not by flipping
    /// the due row in place. Paperclip's promoted wake carries `retryOfRunId`
    /// to the run it continues (`heartbeat.ts:11582-11600`), so the retry is a
    /// new run linked back to the run that failed.
    ///
    /// Parrot's scheduling half diverges: `maybe_schedule_retry` flips the
    /// failed row itself to `scheduled_retry` instead of inserting a pre-linked
    /// retry row, so the link is established here, at promotion time.
    ///
    /// Two invariants keep the promotion from stranding or duplicating a retry:
    ///
    /// 1. The due row is retired to `failed` **before** the funnel call.
    ///    `wakeup_with_context`'s active-run query matches
    ///    `status IN ('queued','running','scheduled_retry')`, so a due row still
    ///    present would match itself, the wake would coalesce into it, and the
    ///    cleared `scheduled_retry_at` would leave the retry unreachable.
    /// 2. The retry is only dispatched while the agent is free, and a wake that
    ///    still ends up held back is put back on the retry clock. Handing the
    ///    retry to the run in flight would dispose its request while
    ///    `scheduled_retry_at` is already cleared — a permanently lost retry.
    pub async fn promote_due_scheduled_retries(&self) -> Result<usize, HeartbeatError> {
        let rows: Vec<(Uuid, Uuid, Uuid, Option<DateTime<Utc>>, i32, Option<String>, Value)> =
            sqlx::query_as(
                "SELECT id, agent_id, company_id, scheduled_retry_at,
                        COALESCE(scheduled_retry_attempt, 0), scheduled_retry_reason,
                        COALESCE(context_snapshot, '{}'::jsonb)
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
        for (run_id, agent_id, company_id, due_at, attempt, retry_reason, run_context) in rows {
            let Some(issue_id) = run_context
                .get("issueId")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
            else {
                // Nothing to re-wake: retire the row so the scan stops
                // reconsidering it on every tick.
                let _ = sqlx::query(
                    "UPDATE heartbeat_runs
                     SET status = 'failed'::heartbeat_run_status,
                         scheduled_retry_at = NULL,
                         result_json = COALESCE(result_json, '{}'::jsonb)
                             || jsonb_build_object('retryPromoted', false,
                                                   'retryPromotionSkipped', 'missing_issue_id'),
                         updated_at = NOW()
                     WHERE id = $1 AND status = 'scheduled_retry'",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                continue;
            };

            // Invariant 2: never take the retry's wake while the agent is busy.
            // Leaving the row untouched keeps `scheduled_retry_at` due, so the
            // next scan (or the run-completion replay) promotes it.
            if self
                .run_other_than(agent_id, company_id, run_id)
                .await?
                .is_some()
            {
                continue;
            }

            // Invariant 1: retire the due row before waking, so the funnel does
            // not coalesce the retry into the row being promoted.
            let updated = sqlx::query(
                "UPDATE heartbeat_runs
                 SET status = 'failed'::heartbeat_run_status,
                     scheduled_retry_at = NULL,
                     result_json = COALESCE(result_json, '{}'::jsonb)
                         || jsonb_build_object('retryPromoted', true,
                                               'retryPromotedAt', NOW(),
                                               'retryPromotedAttempt', $2),
                     updated_at = NOW()
                 WHERE id = $1 AND status = 'scheduled_retry'",
            )
            .bind(run_id)
            .bind(attempt)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?;
            if updated.rows_affected() == 0 {
                continue;
            }

            // A stable per-attempt key makes the promotion idempotent: a crash
            // between waking and linking leaves the request claimed, and the
            // resumed attempt finds it already linked rather than waking twice.
            let idempotency_key = format!("scheduled-retry-promotion:{run_id}:{attempt}");
            let options = HeartbeatWakeupOptions {
                source: Some("automation".to_string()),
                trigger_detail: Some("system".to_string()),
                reason: Some("scheduled_retry".to_string()),
                payload: Some(serde_json::json!({
                    "issueId": issue_id,
                    "scheduledRetry": true,
                    "scheduledRetryAttempt": attempt,
                })),
                context_snapshot: Some(run_context.clone()),
                retry_of_run_id: Some(run_id),
                scheduled_retry_attempt: Some(attempt),
                idempotency_key: Some(idempotency_key.clone()),
                ..Default::default()
            };
            if let Err(error) = self
                .wakeup_with_options(agent_id, issue_id, company_id, options)
                .await
            {
                tracing::warn!(
                    %run_id, %agent_id, %issue_id, %attempt, %error,
                    "failed to promote a due scheduled retry"
                );
                continue;
            }

            // The funnel records the run it created on the claiming request
            // row, so the retry link is read back from there rather than from
            // the wakeup signature every other caller shares.
            let new_run_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT run_id FROM agent_wakeup_requests
                 WHERE company_id = $1 AND idempotency_key = $2",
            )
            .bind(company_id)
            .bind(&idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?
            .flatten();
            let Some(new_run_id) = new_run_id else {
                // The wake was held back (another run took the agent between the
                // free check and the dispatch, or a gate recorded it). The due
                // row was already retired, so put it back on the retry clock —
                // otherwise the retry would be consumed without ever running.
                let restored = sqlx::query(
                    "UPDATE heartbeat_runs
                     SET status = 'scheduled_retry'::heartbeat_run_status,
                         scheduled_retry_at = NOW() + INTERVAL '30 seconds',
                         result_json = COALESCE(result_json, '{}'::jsonb) - 'retryPromoted',
                         updated_at = NOW()
                     WHERE id = $1 AND status = 'failed'",
                )
                .bind(run_id)
                .execute(&self.pool)
                .await;
                match restored {
                    Ok(restored) if restored.rows_affected() > 0 => tracing::info!(
                        %run_id, %agent_id, %issue_id, %attempt,
                        "scheduled retry held back; re-armed for the next scan"
                    ),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(
                        %run_id, %error,
                        "failed to re-arm a held-back scheduled retry"
                    ),
                }
                continue;
            };

            // The claiming `agent_wakeup_requests` row already records the run
            // that owns the wake (the funnel sets it on dispatch), so the retry
            // chain is auditable from either side.
            if new_run_id != run_id {
                let _ = crate::run_continuations_service::RunContinuationsService::new(
                    self.pool.clone(),
                )
                .create_continuation(
                    new_run_id,
                    Some(run_id),
                    "scheduled_retry".to_string(),
                    serde_json::json!({ "issueId": issue_id, "attempt": attempt }),
                    format!("scheduled_retry_promotion attempt={attempt}"),
                )
                .await;
            }

            let _ = publish_live_event(
                &self.sse_service,
                company_id,
                "heartbeat.run.queued",
                serde_json::json!({
                    "runId": new_run_id,
                    "agentId": agent_id,
                    "issueId": issue_id,
                    "status": "queued",
                    "invocationSource": "automation",
                    "scheduledRetryAttempt": attempt,
                    "scheduledRetryAt": due_at,
                    "scheduledRetryReason": retry_reason,
                    "idempotencyKey": idempotency_key,
                }),
            )
            .await;

            tracing::info!(
                %run_id,
                %new_run_id,
                %agent_id,
                attempt = %attempt,
                "promoted a due scheduled retry into a fresh linked run"
            );
            promoted += 1;
        }
        Ok(promoted)
    }

    /// A run that would hold this agent back if a wake were enqueued now,
    /// ignoring `excluded_run_id` (the caller's own row).
    ///
    /// The same predicate `wakeup_with_context` uses to decide whether a wake
    /// can start a run, so the promoter's "is the agent free" check cannot
    /// disagree with the funnel it hands the retry to.
    async fn run_other_than(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
        excluded_run_id: Uuid,
    ) -> Result<Option<Uuid>, HeartbeatError> {
        sqlx::query_scalar(
            "SELECT id FROM heartbeat_runs
             WHERE company_id = $1 AND agent_id = $2 AND id <> $3
               AND status IN ('queued','running','scheduled_retry')
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(excluded_run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HeartbeatError::Internal(e.to_string()))
    }

    fn clone_for_task(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            children: self.children.clone(),
            http_executor: self.http_executor.clone(),
            sse_service: self.sse_service.clone(),
            cost_service: self.cost_service.clone(),
            budget_service: self.budget_service.clone(),
            runtime_secret_resolver: self.runtime_secret_resolver.clone(),
            issue_comment_service: self.issue_comment_service.clone(),
        }
    }
}

#[cfg(test)]
mod adapter_outcome_tests {
    use tokio::time::Duration;

    use super::{
        build_codex_exec_args, build_heartbeat_run_issue_comment, classify_adapter_error,
        is_codex_unknown_session_output,
        is_retryable_provider_status, parse_adapter_outcome, provider_http_retries,
        provider_http_timeout, provider_retry_after, provider_retry_delay,
        provider_retry_delay_with_hint, redact_adapter_secret, resolve_acp_mode, resolve_biller,
        reject_unsupported_claude_args, valid_claude_resume_session, valid_codex_resume_session,
        AdapterCommandOutput,
    };

    #[test]
    fn builds_normal_heartbeat_summary_comment() {
        assert_eq!(
            build_heartbeat_run_issue_comment(Some("已完成软件外包团队的任务拆解。")),
            Some("已完成软件外包团队的任务拆解。".to_string())
        );
    }

    #[test]
    fn withholds_narration_and_overlong_heartbeat_summaries() {
        assert_eq!(
            build_heartbeat_run_issue_comment(Some("I'll inspect the task before making changes.")),
            Some(super::WITHHELD_HEARTBEAT_COMMENT.to_string())
        );
        assert_eq!(
            build_heartbeat_run_issue_comment(Some(&"x".repeat(1_201))),
            Some(super::WITHHELD_HEARTBEAT_COMMENT.to_string())
        );
        assert!(build_heartbeat_run_issue_comment(Some("  ")).is_none());
    }

    #[test]
    fn explicit_structured_error_overrides_zero_exit() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"result","subtype":"error","is_error":true,"result":"tool failed"}"#,
            "claude_local",
        );
        assert!(outcome.explicit_failure);
        assert_eq!(outcome.failure_reason.as_deref(), Some("tool failed"));
    }

    #[test]
    fn classifies_claude_malformed_response() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"result","subtype":"error","is_error":true,"result":"API Error: API returned an empty or malformed response (HTTP 200)"}"#,
            "claude_local",
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
            "claude_local",
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
            "claude_local",
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
            "claude_local",
        );
        assert_eq!(outcome.session_id.as_deref(), Some("sess-42"));
        assert_eq!(outcome.input_tokens, 120);
        assert_eq!(outcome.output_tokens, 45);
        assert_eq!(outcome.cached_input_tokens, 10);
        assert_eq!(outcome.cost_usd, Some(0.0123));
        assert_eq!(outcome.model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn parses_anthropic_http_message_usage_and_text() {
        let outcome = parse_adapter_outcome(
            r#"{"id":"msg-1","type":"message","role":"assistant","model":"claude-sonnet","content":[{"type":"text","text":"hello from Claude"}],"usage":{"input_tokens":12,"output_tokens":7,"cache_read_input_tokens":3}}"#,
            "claude_local",
        );
        assert_eq!(outcome.result_summary.as_deref(), Some("hello from Claude"));
        assert_eq!(outcome.input_tokens, 12);
        assert_eq!(outcome.output_tokens, 7);
        assert_eq!(outcome.cached_input_tokens, 3);
        assert_eq!(outcome.model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn parses_openai_http_completion_usage_and_text() {
        let outcome = parse_adapter_outcome(
            r#"{"id":"chat-1","object":"chat.completion","model":"gpt-4o-mini","choices":[{"message":{"role":"assistant","content":"hello from OpenAI"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":4}}}"#,
            "codex_local",
        );
        assert_eq!(outcome.result_summary.as_deref(), Some("hello from OpenAI"));
        assert_eq!(outcome.input_tokens, 20);
        assert_eq!(outcome.output_tokens, 5);
        assert_eq!(outcome.cached_input_tokens, 4);
        assert_eq!(outcome.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn resolves_biller_and_http_error_classification_by_adapter() {
        assert_eq!(resolve_biller("claude_local", Some("anthropic")), "anthropic");
        assert_eq!(resolve_biller("codex_local", Some("openai")), "openai");
        assert_eq!(resolve_biller("process", None), "process");

        let (auth_code, auth_family) = classify_adapter_error(
            "LLM request failed with HTTP 401: invalid api key",
            "codex_local",
        );
        assert_eq!(auth_code.as_deref(), Some("codex_auth_required"));
        assert_eq!(auth_family.as_deref(), Some("authentication"));

        let (transient_code, transient_family) = classify_adapter_error(
            "LLM request failed with HTTP 429: rate limit exceeded",
            "codex_local",
        );
        assert_eq!(transient_code.as_deref(), Some("codex_transient_upstream"));
        assert_eq!(transient_family.as_deref(), Some("transient_upstream"));
    }

    #[test]
    fn bounds_provider_http_timeout_retries_and_redacts_errors() {
        assert_eq!(provider_http_timeout(&serde_json::json!({})), Duration::from_secs(120));
        assert_eq!(
            provider_http_timeout(&serde_json::json!({"timeoutMs": 1_500})),
            Duration::from_millis(1_500)
        );
        assert_eq!(
            provider_http_timeout(&serde_json::json!({"timeoutSec": 2})),
            Duration::from_secs(2)
        );
        assert_eq!(provider_http_retries(&serde_json::json!({})), 2);
        assert_eq!(provider_http_retries(&serde_json::json!({"retries": 9})), 3);
        assert_eq!(provider_http_retries(&serde_json::json!({"maxRetries": 0})), 0);
        assert!(is_retryable_provider_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_provider_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!is_retryable_provider_status(reqwest::StatusCode::UNAUTHORIZED));
        assert_eq!(
            redact_adapter_secret("provider echoed sk-secret in the response", "sk-secret"),
            "provider echoed [REDACTED] in the response"
        );
    }

    #[test]
    fn honors_bounded_provider_retry_after_hints() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("1.5"),
        );
        assert_eq!(
            provider_retry_after(&headers),
            Some(Duration::from_millis(1_500))
        );
        assert_eq!(
            provider_retry_delay_with_hint(1, provider_retry_after(&headers)),
            Duration::from_millis(1_500)
        );

        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("999999"),
        );
        assert_eq!(
            provider_retry_after(&headers),
            Some(Duration::from_secs(15 * 60))
        );

        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("-1"),
        );
        assert_eq!(provider_retry_after(&headers), None);
        assert_eq!(
            provider_retry_delay_with_hint(2, provider_retry_after(&headers)),
            provider_retry_delay(2)
        );

        let retry_at =
            httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(2));
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_str(&retry_at).unwrap(),
        );
        let date_hint = provider_retry_after(&headers).expect("HTTP-date hint should parse");
        assert!(date_hint >= Duration::from_millis(1_000));
        assert!(date_hint <= Duration::from_secs(2));

        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("not-a-date"),
        );
        assert_eq!(provider_retry_after(&headers), None);
    }

    #[test]
    fn parses_codex_jsonl_session_usage_summary_and_failure() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"thread.started","thread_id":"thread-42"}
{"type":"item.completed","item":{"type":"agent_message","text":"implemented the change"}}
{"type":"turn.completed","usage":{"input_tokens":120,"cached_input_tokens":30,"output_tokens":45}}"#,
            "codex_local",
        );
        assert_eq!(outcome.session_id.as_deref(), Some("thread-42"));
        assert_eq!(outcome.result_summary.as_deref(), Some("implemented the change"));
        assert_eq!(outcome.input_tokens, 120);
        assert_eq!(outcome.cached_input_tokens, 30);
        assert_eq!(outcome.output_tokens, 45);
        assert!(!outcome.explicit_failure);

        let failed = parse_adapter_outcome(
            r#"{"type":"thread.started","thread_id":"thread-43"}
{"type":"turn.failed","error":{"message":"authentication required"}}"#,
            "codex_local",
        );
        assert_eq!(failed.session_id.as_deref(), Some("thread-43"));
        assert!(failed.explicit_failure);
        assert_eq!(failed.failure_reason.as_deref(), Some("authentication required"));
        assert_eq!(failed.error_code.as_deref(), Some("codex_auth_required"));
        assert_eq!(failed.error_family.as_deref(), Some("authentication"));

        let stale = parse_adapter_outcome(
            r#"{"type":"error","message":"state db missing rollout path for thread thread-44"}"#,
            "codex_local",
        );
        assert_eq!(stale.error_code.as_deref(), Some("codex_unknown_session"));
        assert_eq!(stale.error_family.as_deref(), Some("session"));
    }

    #[test]
    fn detects_codex_unknown_session_in_structured_or_plain_output() {
        let structured = AdapterCommandOutput {
            exit_code: 0,
            stdout: r#"{"type":"error","message":"state db missing rollout path for thread thread-44"}"#
                .to_string(),
            stderr: String::new(),
            resumed_session_id: Some("thread-44".to_string()),
            billing_type: "subscription".to_string(),
            runtime_secret_manifest: Vec::new(),
        };
        assert!(is_codex_unknown_session_output(&structured));

        let plain = AdapterCommandOutput {
            exit_code: 1,
            stdout: String::new(),
            stderr: "state db missing rollout path for thread thread-45".to_string(),
            resumed_session_id: Some("thread-45".to_string()),
            billing_type: "subscription".to_string(),
            runtime_secret_manifest: Vec::new(),
        };
        assert!(is_codex_unknown_session_output(&plain));

        let successful = AdapterCommandOutput {
            exit_code: 0,
            stdout: r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#
                .to_string(),
            stderr: String::new(),
            resumed_session_id: Some("thread-46".to_string()),
            billing_type: "subscription".to_string(),
            runtime_secret_manifest: Vec::new(),
        };
        assert!(!is_codex_unknown_session_output(&successful));
    }

    #[test]
    fn codex_default_invocation_uses_json_protocol_and_model() {
        assert_eq!(
            build_codex_exec_args(Some("gpt-5.4"), false),
            vec!["exec", "--json", "--model", "gpt-5.4", "-"]
        );
        assert_eq!(
            build_codex_exec_args(None, false),
            vec!["exec", "--json", "-"]
        );
        assert_eq!(
            build_codex_exec_args(Some("gpt-5.4"), true),
            vec!["acp", "--model", "gpt-5.4"]
        );
        assert_eq!(
            build_codex_exec_args(None, true),
            vec!["acp"]
        );
    }

    #[test]
    fn claude_auto_invocation_does_not_use_unsupported_acp_flag() {
        assert!(!resolve_acp_mode("claude_local", "auto").unwrap());
        assert!(!resolve_acp_mode("claude_local", "cli").unwrap());
        assert!(resolve_acp_mode("claude_local", "acp").is_err());
        assert!(resolve_acp_mode("codex_local", "auto").unwrap());
        assert!(resolve_acp_mode("codex_local", "acp").unwrap());
    }

    #[test]
    fn rejects_legacy_claude_acp_args_before_process_start() {
        assert!(reject_unsupported_claude_args(
            "claude_local",
            &["--print".to_string(), "--acp".to_string()]
        )
        .is_err());
        assert!(reject_unsupported_claude_args(
            "claude_local",
            &["--print".to_string()]
        )
        .is_ok());
        assert!(reject_unsupported_claude_args(
            "codex_local",
            &["--acp".to_string()]
        )
        .is_ok());
    }

    #[test]
    fn only_safe_codex_sessions_are_eligible_for_resume() {
        assert_eq!(
            valid_codex_resume_session(Some("thread-42")),
            Some("thread-42".to_string())
        );
        assert!(valid_codex_resume_session(Some("--resume")).is_none());
        assert!(valid_codex_resume_session(Some("thread 42")).is_none());
        assert!(valid_codex_resume_session(None).is_none());
    }

    #[test]
    fn only_uuid_claude_sessions_are_eligible_for_resume() {
        assert!(valid_claude_resume_session(Some("550e8400-e29b-41d4-a716-446655440000")).is_some());
        assert!(valid_claude_resume_session(Some("not-a-session")).is_none());
        assert!(valid_claude_resume_session(None).is_none());
    }

    #[test]
    fn parses_acpx_session_and_result_events() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"acpx.session","sessionId":"acp-sess-1","agent":"claude","mode":"persistent"}
{"type":"acpx.text_delta","text":"Working on the task","channel":"output"}
{"type":"acpx.tool_call","name":"read_file","status":"started"}
{"type":"acpx.tool_result","name":"read_file","isError":false}
{"type":"acpx.result","stopReason":"end_turn","summary":"Completed the implementation"}"#,
            "claude_local",
        );
        assert_eq!(outcome.session_id.as_deref(), Some("acp-sess-1"), "acpx.session sets session_id");
        assert_eq!(outcome.result_summary.as_deref(), Some("Completed the implementation"), "acpx.result summary");
        assert_eq!(outcome.tool_call_count, 1, "acpx.tool_call increments count");
        assert!(!outcome.explicit_failure, "successful ACP run has no error");
    }

    #[test]
    fn parses_acpx_error_event_as_explicit_failure() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"acpx.error","code":"auth_required","message":"Authentication required for provider","retryable":true}"#,
            "codex_local",
        );
        assert!(outcome.explicit_failure, "acpx.error sets explicit_failure");
        assert_eq!(outcome.failure_reason.as_deref(), Some("Authentication required for provider"));
        assert_eq!(outcome.error_code.as_deref(), Some("auth_required"));
        assert_eq!(outcome.error_family.as_deref(), Some("acp"));
    }

    #[test]
    fn parses_acpx_status_and_text_delta_as_summary_fallback() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"acpx.status","text":"Processing request...","cost":{"total":0.0},"contextWindow":{"used":0,"max":200000}}"#,
            "claude_local",
        );
        assert_eq!(outcome.result_summary.as_deref(), Some("Processing request..."), "acpx.status sets summary");
        assert!(!outcome.explicit_failure);

        // text_delta sets summary when status hasn't
        let delta_only = parse_adapter_outcome(
            r#"{"type":"acpx.text_delta","text":"Streaming output...","channel":"output"}"#,
            "claude_local",
        );
        assert_eq!(delta_only.result_summary.as_deref(), Some("Streaming output..."), "acpx.text_delta sets summary");
    }

    #[test]
    fn acpx_result_trumps_earlier_summary_for_final_outcome() {
        let outcome = parse_adapter_outcome(
            r#"{"type":"acpx.text_delta","text":"intermediate text"}
{"type":"acpx.status","text":"still working"}
{"type":"acpx.result","stopReason":"end_turn","summary":"Final result"}"#,
            "claude_local",
        );
        // acpx.result returns early, so the summary is set by result
        assert_eq!(outcome.result_summary.as_deref(), Some("Final result"), "acpx.result should be the final summary");
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
    fn command_with_env_logs_resolved_values_and_redacts_sensitive_values() {
        let environment = BTreeMap::from([
            ("ANTHROPIC_AUTH_TOKEN".to_string(), "secret-token".to_string()),
            ("ANTHROPIC_MODEL".to_string(), "claude-sonnet".to_string()),
        ]);
        let sensitive = HashSet::from(["ANTHROPIC_AUTH_TOKEN".to_string()]);

        let logged = command_with_env(
            &environment,
            &sensitive,
            "cd /tmp && claude --print",
            "ptg_gateway_secret",
        );

        assert!(logged.contains("ANTHROPIC_MODEL=claude-sonnet"));
        assert!(logged.contains("ANTHROPIC_AUTH_TOKEN="));
        assert!(logged.contains("[REDACTED]"));
        assert!(!logged.contains("secret-token"));
        assert!(logged.ends_with("cd /tmp && claude --print"));
    }

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

#[cfg(test)]
mod process_liveness_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn register_child_makes_process_visible_to_reconciliation() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@127.0.0.1:1/parrot_agent_test")
            .expect("test pool URL should parse");
        let service = DefaultHeartbeatService::new(pool);
        let run_id = Uuid::new_v4();
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn test child");

        let child_ref = service.register_child(run_id, child).await;
        assert!(service.children.lock().await.contains_key(&run_id));

        let mut child = child_ref.lock().await;
        child.kill().await.expect("kill test child");
        child.wait().await.expect("wait for test child");
        drop(child);
        service.children.lock().await.remove(&run_id);
        assert!(!service.children.lock().await.contains_key(&run_id));
    }

    #[tokio::test]
    async fn persisted_run_event_refreshes_running_run_activity() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping run liveness test: DATABASE_URL is not set");
            return;
        };
        let Ok(pool) = PgPool::connect(&database_url).await else {
            eprintln!("skipping run liveness test: DATABASE_URL is not reachable");
            return;
        };

        let company_id = Uuid::new_v4();
        let issue_prefix = format!("L{}", &company_id.simple().to_string()[..6]);
        sqlx::query(
            "INSERT INTO companies (id, name, issue_prefix)
             VALUES ($1, 'Heartbeat Liveness Test Co', $2)",
        )
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(&pool)
        .await
        .expect("insert liveness test company");

        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, company_id, name)
             VALUES ($1, $2, 'Heartbeat Liveness Test Agent')",
        )
        .bind(agent_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("insert liveness test agent");

        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO heartbeat_runs
                (id, company_id, agent_id, invocation_source, status,
                 context_snapshot, created_at, updated_at)
             VALUES ($1, $2, $3, 'on_demand', 'running', $4::jsonb,
                     NOW() - INTERVAL '10 minutes',
                     NOW() - INTERVAL '10 minutes')",
        )
        .bind(run_id)
        .bind(company_id)
        .bind(agent_id)
        .bind(serde_json::json!({ "test": "liveness" }))
        .execute(&pool)
        .await
        .expect("insert stale running test run");

        let before: DateTime<Utc> =
            sqlx::query_scalar("SELECT updated_at FROM heartbeat_runs WHERE id = $1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("read stale run timestamp");
        persist_heartbeat_run_event(
            &pool,
            company_id,
            run_id,
            agent_id,
            "heartbeat.run.log",
            Some("stdout"),
            Some("info"),
            Some("still working"),
            &serde_json::json!({ "chunk": "still working" }),
        )
        .await
        .expect("persist liveness event");

        let after: DateTime<Utc> =
            sqlx::query_scalar("SELECT updated_at FROM heartbeat_runs WHERE id = $1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("read refreshed run timestamp");
        assert!(after > before, "run activity timestamp must advance");

        let service = DefaultHeartbeatService::new(pool.clone());
        assert_eq!(
            service
                .reconcile_orphaned_runs(300)
                .await
                .expect("reconcile test runs"),
            0,
            "a run with a recent event must not be marked process-lost"
        );
        let status: String =
            sqlx::query_scalar("SELECT status::text FROM heartbeat_runs WHERE id = $1")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .expect("read liveness test status");
        assert_eq!(status, "running");

        sqlx::query(
            "UPDATE heartbeat_runs
             SET updated_at = NOW() - INTERVAL '10 minutes'
             WHERE id = $1",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("age liveness test run");
        assert_eq!(
            service
                .reconcile_orphaned_runs(300)
                .await
                .expect("reconcile stale test run"),
            1,
            "an actually stale run should be reconciled"
        );
        let process_lost: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status::text, error_code, error_family
             FROM heartbeat_runs WHERE id = $1",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("read process-lost metadata");
        assert_eq!(process_lost.0, "failed");
        assert_eq!(process_lost.1.as_deref(), Some("process_lost"));
        assert_eq!(process_lost.2.as_deref(), Some("process"));

        sqlx::query("DELETE FROM heartbeat_runs WHERE id = $1")
            .bind(run_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM companies WHERE id = $1")
            .bind(company_id)
            .execute(&pool)
            .await
            .ok();
    }
}
