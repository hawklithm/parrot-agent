/// Plugin Worker 进程管理器
/// 
/// 基于 Paperclip 的 plugin-worker-manager.ts 实现
/// 管理 Plugin Worker 进程的生命周期、通信和崩溃恢复
///
/// 主要功能：
/// - Worker 进程生命周期管理（启动、停止、重启）
/// - JSON-RPC 2.0 通信协议
/// - 进程崩溃恢复和指数退避
/// - 健康检查和心跳
/// - 进程池管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 常量定义
// ---------------------------------------------------------------------------

/// RPC 调用默认超时时间（30秒）
const DEFAULT_RPC_TIMEOUT_MS: u64 = 30_000;

/// 优雅关闭等待时间（10秒）
const GRACEFUL_SHUTDOWN_TIMEOUT_SECS: u64 = 10;

/// 最大连续崩溃次数
const MAX_CONSECUTIVE_CRASHES: u32 = 5;

/// 初始退避时间（1秒）
const INITIAL_BACKOFF_MS: u64 = 1000;

/// 最大退避时间（60秒）
const MAX_BACKOFF_MS: u64 = 60_000;

/// 退避指数
const BACKOFF_MULTIPLIER: f64 = 2.0;

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/// Worker 进程状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Backoff,
}

/// JSON-RPC 2.0 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC Error {}: {}", self.code, self.message)
    }
}

/// JSON-RPC ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum JsonRpcId {
    String(String),
    Number(i64),
}

/// Worker 启动选项
#[derive(Debug, Clone)]
pub struct WorkerStartOptions {
    pub plugin_id: Uuid,
    pub entrypoint_path: String,
    pub config: serde_json::Value,
    pub instance_info: InstanceInfo,
    pub api_version: u32,
    pub rpc_timeout_ms: Option<u64>,
    pub auto_restart: bool,
    pub env: HashMap<String, String>,
}

/// 实例信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub host_version: String,
}

/// Worker 诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDiagnostics {
    pub plugin_id: Uuid,
    pub status: WorkerStatus,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub consecutive_crashes: u32,
    pub total_crashes: u32,
    pub pending_requests: usize,
    pub last_crash_at: Option<u64>,
    pub next_restart_at: Option<u64>,
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("worker not running: {0}")]
    NotRunning(Uuid),

    #[error("worker already running: {0}")]
    AlreadyRunning(Uuid),

    #[error("RPC call timeout: method={method}, timeout_ms={timeout_ms}")]
    RpcTimeout { method: String, timeout_ms: u64 },

    #[error("RPC call failed: {0}")]
    RpcError(JsonRpcError),

    #[error("worker crashed: code={code:?}, signal={signal:?}")]
    WorkerCrashed { code: Option<i32>, signal: Option<i32> },

    #[error("spawn failed: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("channel closed")]
    ChannelClosed,

    #[error("max consecutive crashes reached: {0}")]
    MaxCrashesReached(u32),

    #[error("initialization failed: {0}")]
    InitializationFailed(String),

    #[error("shutdown failed: {0}")]
    ShutdownFailed(String),
}

pub type WorkerResult<T> = Result<T, WorkerError>;

// ---------------------------------------------------------------------------
// 内部消息类型
// ---------------------------------------------------------------------------

struct PendingRequest {
    id: JsonRpcId,
    method: String,
    sender: tokio::sync::oneshot::Sender<JsonRpcResponse>,
    sent_at: Instant,
}

// ---------------------------------------------------------------------------
// PluginWorkerHandle - 单个 Worker 进程句柄
// ---------------------------------------------------------------------------

/// 单个 Plugin Worker 进程句柄
pub struct PluginWorkerHandle {
    plugin_id: Uuid,
    options: WorkerStartOptions,

    // 进程状态
    status: Arc<RwLock<WorkerStatus>>,
    child: Arc<Mutex<Option<Child>>>,
    started_at: Arc<RwLock<Option<Instant>>>,

    // RPC 通信
    stdin_tx: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    pending_requests: Arc<Mutex<HashMap<String, PendingRequest>>>,
    next_request_id: Arc<Mutex<u64>>,

    // 崩溃恢复
    consecutive_crashes: Arc<Mutex<u32>>,
    total_crashes: Arc<Mutex<u32>>,
    last_crash_at: Arc<Mutex<Option<Instant>>>,

    // 关闭信号
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl PluginWorkerHandle {
    pub fn new(options: WorkerStartOptions) -> Self {
        Self {
            plugin_id: options.plugin_id,
            options,
            status: Arc::new(RwLock::new(WorkerStatus::Stopped)),
            child: Arc::new(Mutex::new(None)),
            started_at: Arc::new(RwLock::new(None)),
            stdin_tx: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(Mutex::new(1)),
            consecutive_crashes: Arc::new(Mutex::new(0)),
            total_crashes: Arc::new(Mutex::new(0)),
            last_crash_at: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 启动 Worker 进程
    pub async fn start(&self) -> WorkerResult<()> {
        let current_status = *self.status.read().await;
        if current_status == WorkerStatus::Running || current_status == WorkerStatus::Starting {
            return Err(WorkerError::AlreadyRunning(self.plugin_id));
        }

        info!(
            "plugin_worker_manager: starting worker for plugin_id={}",
            self.plugin_id
        );
        *self.status.write().await = WorkerStatus::Starting;

        // 启动进程
        let mut cmd = Command::new("node");
        cmd.arg(&self.options.entrypoint_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // 设置环境变量
        for (key, value) in &self.options.env {
            cmd.env(key, value);
        }
        cmd.env("PAPERCLIP_PLUGIN_ID", self.plugin_id.to_string());

        let mut child = cmd.spawn()?;

        // 获取 stdin/stdout/stderr
        let stdin = child.stdin.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "failed to capture stdin")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "failed to capture stdout")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "failed to capture stderr")
        })?;

        *self.stdin_tx.lock().await = Some(stdin);

        // 启动 IO 循环
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        self.spawn_io_loops(stdout, stderr, shutdown_rx).await;

        // 保存进程句柄
        *self.child.lock().await = Some(child);
        *self.started_at.write().await = Some(Instant::now());

        // 发送 initialize RPC
        self.send_initialize().await?;

        *self.status.write().await = WorkerStatus::Running;
        *self.consecutive_crashes.lock().await = 0;

        info!(
            "plugin_worker_manager: worker started successfully for plugin_id={}",
            self.plugin_id
        );
        Ok(())
    }

    /// 发送 initialize RPC 调用
    async fn send_initialize(&self) -> WorkerResult<()> {
        let params = serde_json::json!({
            "config": self.options.config,
            "instanceInfo": self.options.instance_info,
            "apiVersion": self.options.api_version,
        });

        let result = timeout(
            Duration::from_secs(30),
            self.call("initialize".to_string(), params, None),
        )
        .await;

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(WorkerError::InitializationFailed(e.to_string())),
            Err(_) => Err(WorkerError::RpcTimeout {
                method: "initialize".to_string(),
                timeout_ms: 30_000,
            }),
        }
    }

    /// 启动 stdout/stderr IO 循环
    async fn spawn_io_loops(
        &self,
        stdout: tokio::process::ChildStdout,
        stderr: tokio::process::ChildStderr,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let plugin_id = self.plugin_id;
        let pending_requests = Arc::clone(&self.pending_requests);

        // stdout 循环：处理 JSON-RPC 消息
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                tokio::select! {
                    line_result = lines.next_line() => {
                        match line_result {
                            Ok(Some(line)) => {
                                if let Err(e) = Self::handle_stdout_line(&line, &pending_requests).await {
                                    warn!("plugin_worker_manager: error handling stdout line: {}", e);
                                }
                            }
                            Ok(None) => {
                                debug!("plugin_worker_manager: stdout closed for plugin_id={}", plugin_id);
                                break;
                            }
                            Err(e) => {
                                error!("plugin_worker_manager: error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        debug!("plugin_worker_manager: shutdown signal received, stopping stdout loop");
                        break;
                    }
                }
            }
        });

        // stderr 循环：记录错误日志
        let plugin_id = self.plugin_id;
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                warn!(
                    "plugin_worker_manager: [plugin {} stderr] {}",
                    plugin_id, line
                );
            }
        });
    }

    /// 处理 stdout 行（JSON-RPC 消息）
    async fn handle_stdout_line(
        line: &str,
        pending_requests: &Arc<Mutex<HashMap<String, PendingRequest>>>,
    ) -> WorkerResult<()> {
        if line.trim().is_empty() {
            return Ok(());
        }

        // 解析 JSON-RPC 消息
        let message: serde_json::Value = serde_json::from_str(line)?;

        // 检查是否是响应
        if let Some(_id) = message.get("id") {
            if message.get("result").is_some() || message.get("error").is_some() {
                let response: JsonRpcResponse = serde_json::from_value(message)?;
                Self::handle_response(response, pending_requests).await;
            }
        }

        Ok(())
    }

    /// 处理 RPC 响应
    async fn handle_response(
        response: JsonRpcResponse,
        pending_requests: &Arc<Mutex<HashMap<String, PendingRequest>>>,
    ) {
        let id_key = match &response.id {
            JsonRpcId::String(s) => s.clone(),
            JsonRpcId::Number(n) => n.to_string(),
        };

        let mut requests = pending_requests.lock().await;
        if let Some(pending) = requests.remove(&id_key) {
            let _ = pending.sender.send(response);
        } else {
            warn!(
                "plugin_worker_manager: received response for unknown request id: {:?}",
                response.id
            );
        }
    }

    /// 停止 Worker 进程
    pub async fn stop(&self) -> WorkerResult<()> {
        info!(
            "plugin_worker_manager: stopping worker for plugin_id={}",
            self.plugin_id
        );
        *self.status.write().await = WorkerStatus::Stopping;

        // 1. 发送 shutdown RPC
        if let Err(e) = self.send_shutdown().await {
            warn!(
                "plugin_worker_manager: failed to send shutdown RPC: {}",
                e
            );
        }

        // 2. 等待进程退出（最多 10 秒）
        let wait_result = timeout(
            Duration::from_secs(GRACEFUL_SHUTDOWN_TIMEOUT_SECS),
            self.wait_for_exit(),
        )
        .await;

        if wait_result.is_err() {
            warn!("plugin_worker_manager: graceful shutdown timeout, killing process");
            self.kill_process().await?;
        }

        // 3. 清理资源
        *self.stdin_tx.lock().await = None;
        *self.child.lock().await = None;
        *self.started_at.write().await = None;

        // 发送关闭信号给 IO 循环
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }

        // 清理所有待处理的请求
        let mut requests = self.pending_requests.lock().await;
        for (_key, pending) in requests.drain() {
            let _ = pending.sender.send(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: pending.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: "Worker stopped".to_string(),
                    data: None,
                }),
            });
        }

        *self.status.write().await = WorkerStatus::Stopped;
        info!(
            "plugin_worker_manager: worker stopped for plugin_id={}",
            self.plugin_id
        );
        Ok(())
    }

    /// 发送 shutdown RPC 调用
    async fn send_shutdown(&self) -> WorkerResult<()> {
        let params = serde_json::json!({});
        timeout(
            Duration::from_secs(5),
            self.call("shutdown".to_string(), params, Some(5000)),
        )
        .await
        .map_err(|_| WorkerError::ShutdownFailed("timeout".to_string()))?
        .map(|_| ())
    }

    /// 等待进程退出
    async fn wait_for_exit(&self) {
        if let Some(child) = self.child.lock().await.as_mut() {
            let _ = child.wait().await;
        }
    }

    /// 强制杀死进程
    async fn kill_process(&self) -> WorkerResult<()> {
        if let Some(child) = self.child.lock().await.as_mut() {
            child.kill().await?;
        }
        Ok(())
    }

    /// 重启 Worker 进程
    pub async fn restart(&self) -> WorkerResult<()> {
        self.stop().await?;
        sleep(Duration::from_secs(1)).await;
        self.start().await?;
        Ok(())
    }

    /// 发送 RPC 调用
    pub async fn call(
        &self,
        method: String,
        params: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> WorkerResult<serde_json::Value> {
        let status = *self.status.read().await;
        if status != WorkerStatus::Running {
            return Err(WorkerError::NotRunning(self.plugin_id));
        }

        // 生成请求 ID
        let mut next_id = self.next_request_id.lock().await;
        let request_id = *next_id;
        *next_id += 1;
        drop(next_id);

        let id = JsonRpcId::Number(request_id as i64);
        let id_key = request_id.to_string();

        // 创建请求
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            method: method.clone(),
            params,
        };

        // 序列化并发送
        let request_json = serde_json::to_string(&request)?;
        let request_line = format!("{}\n", request_json);

        let mut stdin = self.stdin_tx.lock().await;
        if let Some(stdin) = stdin.as_mut() {
            stdin.write_all(request_line.as_bytes()).await?;
            stdin.flush().await?;
        } else {
            return Err(WorkerError::NotRunning(self.plugin_id));
        }
        drop(stdin);

        // 注册待处理请求
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let pending = PendingRequest {
            id,
            method: method.clone(),
            sender: response_tx,
            sent_at: Instant::now(),
        };

        self.pending_requests
            .lock()
            .await
            .insert(id_key.clone(), pending);

        // 等待响应（带超时）
        let timeout_duration = Duration::from_millis(
            timeout_ms.unwrap_or(self.options.rpc_timeout_ms.unwrap_or(DEFAULT_RPC_TIMEOUT_MS)),
        );

        let result = timeout(timeout_duration, response_rx).await;

        match result {
            Ok(Ok(response)) => {
                if let Some(error) = response.error {
                    Err(WorkerError::RpcError(error))
                } else {
                    Ok(response.result.unwrap_or(serde_json::Value::Null))
                }
            }
            Ok(Err(_)) => {
                // 通道关闭
                self.pending_requests.lock().await.remove(&id_key);
                Err(WorkerError::ChannelClosed)
            }
            Err(_) => {
                // 超时
                self.pending_requests.lock().await.remove(&id_key);
                Err(WorkerError::RpcTimeout {
                    method,
                    timeout_ms: timeout_duration.as_millis() as u64,
                })
            }
        }
    }

    /// 获取当前状态
    pub async fn get_status(&self) -> WorkerStatus {
        *self.status.read().await
    }

    /// 获取诊断信息
    pub async fn diagnostics(&self) -> WorkerDiagnostics {
        let uptime_secs = if let Some(started_at) = *self.started_at.read().await {
            Some(started_at.elapsed().as_secs())
        } else {
            None
        };

        let pid = if let Some(child) = self.child.lock().await.as_ref() {
            child.id()
        } else {
            None
        };

        let last_crash_at = if let Some(crash_at) = *self.last_crash_at.lock().await {
            Some(crash_at.elapsed().as_secs())
        } else {
            None
        };

        WorkerDiagnostics {
            plugin_id: self.plugin_id,
            status: *self.status.read().await,
            pid,
            uptime_secs,
            consecutive_crashes: *self.consecutive_crashes.lock().await,
            total_crashes: *self.total_crashes.lock().await,
            pending_requests: self.pending_requests.lock().await.len(),
            last_crash_at,
            next_restart_at: None,
        }
    }

    /// 处理崩溃恢复
    pub async fn handle_crash(
        &self,
        code: Option<i32>,
        signal: Option<i32>,
    ) -> WorkerResult<()> {
        error!(
            "plugin_worker_manager: worker crashed for plugin_id={}, code={:?}, signal={:?}",
            self.plugin_id, code, signal
        );

        *self.status.write().await = WorkerStatus::Crashed;
        *self.last_crash_at.lock().await = Some(Instant::now());

        let mut consecutive = self.consecutive_crashes.lock().await;
        *consecutive += 1;
        let consecutive_count = *consecutive;
        drop(consecutive);

        let mut total = self.total_crashes.lock().await;
        *total += 1;
        drop(total);

        // 检查是否超过最大崩溃次数
        if consecutive_count >= MAX_CONSECUTIVE_CRASHES {
            error!(
                "plugin_worker_manager: max consecutive crashes reached for plugin_id={}",
                self.plugin_id
            );
            return Err(WorkerError::MaxCrashesReached(consecutive_count));
        }

        // 如果启用了自动重启，使用指数退避重启
        if self.options.auto_restart {
            let backoff_ms = Self::calculate_backoff(consecutive_count);
            info!(
                "plugin_worker_manager: scheduling restart for plugin_id={} after {}ms",
                self.plugin_id, backoff_ms
            );

            *self.status.write().await = WorkerStatus::Backoff;
            sleep(Duration::from_millis(backoff_ms)).await;

            self.start().await?;
        }

        Ok(())
    }

    /// 计算指数退避时间
    fn calculate_backoff(consecutive_crashes: u32) -> u64 {
        let backoff = INITIAL_BACKOFF_MS as f64
            * BACKOFF_MULTIPLIER.powi(consecutive_crashes as i32 - 1);
        backoff.min(MAX_BACKOFF_MS as f64) as u64
    }
}

// ---------------------------------------------------------------------------
// PluginWorkerManager - Worker 管理器
// ---------------------------------------------------------------------------

/// Plugin Worker 管理器
pub struct PluginWorkerManager {
    workers: Arc<RwLock<HashMap<Uuid, Arc<PluginWorkerHandle>>>>,
}

impl PluginWorkerManager {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 启动 Worker
    pub async fn start_worker(&self, options: WorkerStartOptions) -> WorkerResult<()> {
        let plugin_id = options.plugin_id;
        let mut workers = self.workers.write().await;

        if workers.contains_key(&plugin_id) {
            return Err(WorkerError::AlreadyRunning(plugin_id));
        }

        let handle = Arc::new(PluginWorkerHandle::new(options));
        handle.start().await?;
        workers.insert(plugin_id, handle);

        info!(
            "plugin_worker_manager: worker started for plugin_id={}",
            plugin_id
        );
        Ok(())
    }

    /// 停止 Worker
    pub async fn stop_worker(&self, plugin_id: Uuid) -> WorkerResult<()> {
        let mut workers = self.workers.write().await;
        if let Some(handle) = workers.remove(&plugin_id) {
            handle.stop().await?;
            info!(
                "plugin_worker_manager: worker stopped for plugin_id={}",
                plugin_id
            );
        }
        Ok(())
    }

    /// 停止所有 Worker
    pub async fn stop_all(&self) -> WorkerResult<()> {
        let workers = self.workers.read().await;
        let handles: Vec<_> = workers.values().cloned().collect();
        drop(workers);

        for handle in handles {
            if let Err(e) = handle.stop().await {
                error!("plugin_worker_manager: error stopping worker: {}", e);
            }
        }

        self.workers.write().await.clear();
        Ok(())
    }

    /// 检查 Worker 是否存在
    pub async fn has_worker(&self, plugin_id: Uuid) -> bool {
        self.workers.read().await.contains_key(&plugin_id)
    }

    /// 检查 Worker 是否运行中
    pub async fn is_running(&self, plugin_id: Uuid) -> bool {
        if let Some(handle) = self.workers.read().await.get(&plugin_id) {
            handle.get_status().await == WorkerStatus::Running
        } else {
            false
        }
    }

    /// 发送 RPC 调用
    pub async fn call(
        &self,
        plugin_id: Uuid,
        method: String,
        params: serde_json::Value,
        timeout_ms: Option<u64>,
    ) -> WorkerResult<serde_json::Value> {
        let workers = self.workers.read().await;
        let handle = workers
            .get(&plugin_id)
            .ok_or(WorkerError::NotRunning(plugin_id))?;
        handle.call(method, params, timeout_ms).await
    }

    /// 重启 Worker
    pub async fn restart_worker(&self, plugin_id: Uuid) -> WorkerResult<()> {
        let workers = self.workers.read().await;
        let handle = workers
            .get(&plugin_id)
            .ok_or(WorkerError::NotRunning(plugin_id))?;
        handle.restart().await
    }

    /// 获取所有 Worker 的诊断信息
    pub async fn diagnostics(&self) -> Vec<WorkerDiagnostics> {
        let workers = self.workers.read().await;
        let mut diagnostics = Vec::new();

        for handle in workers.values() {
            diagnostics.push(handle.diagnostics().await);
        }

        diagnostics
    }

    /// 获取 Worker 数量
    pub async fn worker_count(&self) -> usize {
        self.workers.read().await.len()
    }
}

impl Default for PluginWorkerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_worker_manager_creation() {
        let manager = PluginWorkerManager::new();
        assert_eq!(manager.worker_count().await, 0);
    }

    #[tokio::test]
    async fn test_worker_handle_creation() {
        let options = WorkerStartOptions {
            plugin_id: Uuid::new_v4(),
            entrypoint_path: "/path/to/worker.js".to_string(),
            config: serde_json::json!({}),
            instance_info: InstanceInfo {
                instance_id: "test-instance".to_string(),
                host_version: "1.0.0".to_string(),
            },
            api_version: 1,
            rpc_timeout_ms: Some(30000),
            auto_restart: true,
            env: HashMap::new(),
        };

        let handle = PluginWorkerHandle::new(options);
        assert_eq!(handle.get_status().await, WorkerStatus::Stopped);
    }

    #[test]
    fn test_backoff_calculation() {
        assert_eq!(PluginWorkerHandle::calculate_backoff(1), 1000);
        assert_eq!(PluginWorkerHandle::calculate_backoff(2), 2000);
        assert_eq!(PluginWorkerHandle::calculate_backoff(3), 4000);
        assert_eq!(PluginWorkerHandle::calculate_backoff(4), 8000);
        assert_eq!(PluginWorkerHandle::calculate_backoff(5), 16000);
        // 应该被最大值限制
        assert!(PluginWorkerHandle::calculate_backoff(10) <= MAX_BACKOFF_MS);
    }

    #[test]
    fn test_json_rpc_id_serialization() {
        let id_string = JsonRpcId::String("test-123".to_string());
        let serialized = serde_json::to_string(&id_string).unwrap();
        assert_eq!(serialized, r#""test-123""#);

        let id_number = JsonRpcId::Number(42);
        let serialized = serde_json::to_string(&id_number).unwrap();
        assert_eq!(serialized, "42");
    }
}
