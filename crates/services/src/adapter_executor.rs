//! Adapter 执行引擎
//!
//! 提供本地和远程 adapter 执行能力
//! 对应 pipeline-adapter-tasks.md §5 Adapter 执行引擎

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ============================================================================
// 执行引擎核心接口
// ============================================================================

/// 运行时命令规范
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRuntimeCommandSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
}

/// 执行目标配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTargetConfig {
    pub target_type: ExecutionTargetType,
    pub connection_info: Option<serde_json::Value>,
    pub asset_sync_config: Option<serde_json::Value>,
}

/// 执行目标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTargetType {
    Local,
    Remote,
    Sandbox,
}

/// Adapter 执行上下文
#[derive(Debug, Clone)]
pub struct AdapterExecutionContext {
    pub run_id: String,
    pub agent_id: String,
    pub config: serde_json::Value,
    pub working_dir: Option<String>,
    pub execution_target: ExecutionTargetConfig,
}

/// Adapter 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterExecutionResult {
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub output: String,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

/// 执行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Ok,
    Error,
}

// ============================================================================
// 执行引擎 trait
// ============================================================================

/// Adapter 执行器 trait
#[async_trait]
pub trait AdapterExecutor: Send + Sync {
    /// 执行 adapter
    async fn execute(&self, ctx: AdapterExecutionContext) -> AdapterExecutionResult;

    /// 取消执行
    async fn cancel(&self, run_id: &str);
}

// ============================================================================
// 日志回调接口
// ============================================================================

/// 日志流类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioKind {
    Stdout,
    Stderr,
}

/// 日志接收器
#[async_trait]
pub trait LogSink: Send + Sync {
    async fn on_log(&self, stream: StdioKind, chunk: &str);
}

/// 运行时状态接收器
#[async_trait]
pub trait RuntimeStatusSink: Send + Sync {
    async fn on_runtime_progress(&self, status: &RuntimeStatus);
}

/// 运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub phase: String,
    pub progress: f64,
    pub message: Option<String>,
}

/// 进程生成通知器
#[async_trait]
pub trait SpawnNotifier: Send + Sync {
    async fn on_spawn(&self, pid: u32, process_group_id: Option<u32>, started_at: chrono::DateTime<chrono::Utc>);
}

// ============================================================================
// 本地执行器
// ============================================================================

/// 本地执行器
pub struct LocalExecutor {
    running_processes: Arc<Mutex<Vec<RunningProcess>>>,
}

struct RunningProcess {
    run_id: String,
    child: Option<Child>,
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            running_processes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl AdapterExecutor for LocalExecutor {
    async fn execute(&self, ctx: AdapterExecutionContext) -> AdapterExecutionResult {
        // 构建命令
        let config = &ctx.config;
        let command = config.get("command").and_then(|v| v.as_str()).unwrap_or("echo");
        let args: Vec<String> = config.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let mut cmd = Command::new(command);
        cmd.args(&args);

        // 设置工作目录
        if let Some(dir) = &ctx.working_dir {
            cmd.current_dir(dir);
        }

        // 设置环境变量
        if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
            for (key, value) in env {
                if let Some(val) = value.as_str() {
                    cmd.env(key, val);
                }
            }
        }

        // 执行命令
        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let status = if output.status.success() {
                    ExecutionStatus::Ok
                } else {
                    ExecutionStatus::Error
                };

                AdapterExecutionResult {
                    status,
                    exit_code: output.status.code(),
                    output: stdout,
                    error: if stderr.is_empty() { None } else { Some(stderr) },
                    metadata: serde_json::json!({
                        "run_id": ctx.run_id,
                        "command": command,
                    }),
                }
            }
            Err(e) => {
                AdapterExecutionResult {
                    status: ExecutionStatus::Error,
                    exit_code: None,
                    output: String::new(),
                    error: Some(format!("Failed to execute: {}", e)),
                    metadata: serde_json::json!({}),
                }
            }
        }
    }

    async fn cancel(&self, run_id: &str) {
        let mut processes = self.running_processes.lock().await;
        if let Some(pos) = processes.iter().position(|p| p.run_id == run_id) {
            if let Some(mut child) = processes[pos].child.take() {
                let _ = child.start_kill();
            }
            processes.remove(pos);
        }
    }
}

// ============================================================================
// 远程执行器（占位符）
// ============================================================================

pub struct RemoteExecutor;

#[async_trait]
impl AdapterExecutor for RemoteExecutor {
    async fn execute(&self, _ctx: AdapterExecutionContext) -> AdapterExecutionResult {
        AdapterExecutionResult {
            status: ExecutionStatus::Error,
            exit_code: None,
            output: String::new(),
            error: Some("Remote execution not yet implemented".to_string()),
            metadata: serde_json::json!({}),
        }
    }

    async fn cancel(&self, _run_id: &str) {
        // TODO: Implement remote cancellation
    }
}
