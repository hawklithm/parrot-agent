//! Adapter 执行引擎
//!
//! 提供本地和远程 adapter 执行能力
//! 对应 pipeline-adapter-tasks.md §5 Adapter 执行引擎

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

pub struct RemoteExecutor {
    running_pids: Arc<Mutex<HashMap<String, u32>>>,
}

impl RemoteExecutor {
    pub fn new() -> Self {
        Self { running_pids: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    fn connection_value<'a>(ctx: &'a AdapterExecutionContext, key: &str) -> Option<&'a serde_json::Value> {
        ctx.execution_target.connection_info.as_ref()
            .and_then(|info| info.get(key))
            .or_else(|| ctx.config.get(key))
    }
}

impl Default for RemoteExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl AdapterExecutor for RemoteExecutor {
    async fn execute(&self, ctx: AdapterExecutionContext) -> AdapterExecutionResult {
        let host = Self::connection_value(&ctx, "host").and_then(|v| v.as_str());
        let Some(host) = host.filter(|value| !value.trim().is_empty()) else {
            return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some("Remote execution requires connection_info.host".to_string()), metadata: serde_json::json!({"run_id": ctx.run_id}) };
        };
        let command = ctx.config.get("command").and_then(|v| v.as_str());
        let Some(command) = command.filter(|value| !value.trim().is_empty()) else {
            return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some("Remote execution requires config.command".to_string()), metadata: serde_json::json!({"run_id": ctx.run_id}) };
        };

        let user = Self::connection_value(&ctx, "user").and_then(|v| v.as_str());
        let target = user.map(|value| format!("{}@{}", value, host)).unwrap_or_else(|| host.to_string());
        let port = Self::connection_value(&ctx, "port").and_then(|v| v.as_u64());
        let identity_file = Self::connection_value(&ctx, "identity_file").and_then(|v| v.as_str());
        let mut remote_script = String::new();
        if let Some(dir) = &ctx.working_dir { remote_script.push_str("cd "); remote_script.push_str(&Self::shell_quote(dir)); remote_script.push_str(" && "); }
        if let Some(env) = ctx.config.get("env").and_then(|v| v.as_object()) {
            for (key, value) in env { if let Some(value) = value.as_str() { remote_script.push_str(key); remote_script.push('='); remote_script.push_str(&Self::shell_quote(value)); remote_script.push(' '); } }
        }
        remote_script.push_str(&Self::shell_quote(command));
        if let Some(args) = ctx.config.get("args").and_then(|v| v.as_array()) {
            for arg in args.iter().filter_map(|v| v.as_str()) { remote_script.push(' '); remote_script.push_str(&Self::shell_quote(arg)); }
        }

        let mut ssh = Command::new("ssh");
        ssh.args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new"]);
        if let Some(port) = port { ssh.args(["-p", &port.to_string()]); }
        if let Some(identity_file) = identity_file { ssh.args(["-i", identity_file]); }
        ssh.arg(target).args(["sh", "-lc"]).arg(remote_script);
        let child = match ssh.spawn() {
            Ok(child) => child,
            Err(error) => return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some(format!("Failed to start ssh: {}", error)), metadata: serde_json::json!({"run_id": ctx.run_id}) },
        };
        if let Some(pid) = child.id() { self.running_pids.lock().await.insert(ctx.run_id.clone(), pid); }
        let result = child.wait_with_output().await;
        self.running_pids.lock().await.remove(&ctx.run_id);
        match result {
            Ok(output) => AdapterExecutionResult { status: if output.status.success() { ExecutionStatus::Ok } else { ExecutionStatus::Error }, exit_code: output.status.code(), output: String::from_utf8_lossy(&output.stdout).to_string(), error: (!output.stderr.is_empty()).then(|| String::from_utf8_lossy(&output.stderr).to_string()), metadata: serde_json::json!({"run_id": ctx.run_id, "host": host, "command": command}) },
            Err(error) => AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some(format!("Failed to wait for remote command: {}", error)), metadata: serde_json::json!({"run_id": ctx.run_id}) },
        }
    }

    async fn cancel(&self, run_id: &str) {
        let pid = self.running_pids.lock().await.get(run_id).copied();
        let Some(pid) = pid else { return; };
        #[cfg(windows)]
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status().await;
        #[cfg(not(windows))]
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status().await;
    }
}
