//! Adapter 执行引擎
//!
//! 提供本地和远程 adapter 执行能力
//! 对应 pipeline-adapter-tasks.md §5 Adapter 执行引擎

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use tokio::process::Command;
use std::process::Stdio;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio::io::{AsyncRead, AsyncReadExt};

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
#[derive(Clone)]
pub struct AdapterExecutionContext {
    pub run_id: String,
    pub agent_id: String,
    pub config: serde_json::Value,
    pub working_dir: Option<String>,
    pub execution_target: ExecutionTargetConfig,
    pub log_sink: Option<Arc<dyn LogSink>>,
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
    running_processes: Arc<Mutex<HashMap<String, u32>>>,
}

/// HTTP webhook executor corresponding to Paperclip's `http` adapter.
pub struct HttpExecutor {
    client: reqwest::Client,
    running_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
}

impl HttpExecutor {
    pub fn new() -> Self {
        Self { client: reqwest::Client::new(), running_requests: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl Default for HttpExecutor {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl AdapterExecutor for HttpExecutor {
    async fn execute(&self, ctx: AdapterExecutionContext) -> AdapterExecutionResult {
        let Some(url) = ctx.config.get("url").and_then(|v| v.as_str()).filter(|v| !v.trim().is_empty()) else {
            return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some("HTTP adapter missing url".to_string()), metadata: serde_json::json!({"run_id": ctx.run_id}) };
        };
        let method = ctx.config.get("method").and_then(|v| v.as_str()).unwrap_or("POST");
        let method = match reqwest::Method::from_bytes(method.as_bytes()) {
            Ok(method) => method,
            Err(error) => return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some(format!("Invalid HTTP method: {}", error)), metadata: serde_json::json!({"run_id": ctx.run_id}) },
        };
        let timeout_ms = ctx.config.get("timeoutMs").or_else(|| ctx.config.get("timeout_ms")).and_then(|v| v.as_u64()).unwrap_or(0);
        let retries = ctx.config.get("retries").and_then(|v| v.as_u64()).unwrap_or(0).min(10);
        let headers = ctx.config.get("headers").and_then(|v| v.as_object());
        let mut payload = ctx.config.get("payloadTemplate").and_then(|v| v.as_object()).cloned().unwrap_or_default();
        payload.insert("agentId".to_string(), serde_json::Value::String(ctx.agent_id.clone()));
        payload.insert("runId".to_string(), serde_json::Value::String(ctx.run_id.clone()));
        payload.insert("context".to_string(), ctx.config.get("context").cloned().unwrap_or(serde_json::Value::Null));

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();
        self.running_requests.lock().await.insert(ctx.run_id.clone(), cancel_tx);
        let mut last_error = None;
        for attempt in 0..=retries {
            let mut request = self.client.request(method.clone(), url).header(reqwest::header::CONTENT_TYPE, "application/json").json(&payload);
            if let Some(headers) = headers {
                for (key, value) in headers {
                    if let Some(value) = value.as_str() { request = request.header(key, value); }
                }
            }
            let response = if timeout_ms > 0 {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        self.running_requests.lock().await.remove(&ctx.run_id);
                        return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some("HTTP request cancelled".to_string()), metadata: serde_json::json!({"run_id": ctx.run_id, "cancelled": true}) };
                    }
                    result = tokio::time::timeout(Duration::from_millis(timeout_ms), request.send()) => result.map_err(|_| "timeout".to_string()).and_then(|result| result.map_err(|e| e.to_string()))
                }
            } else {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        self.running_requests.lock().await.remove(&ctx.run_id);
                        return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some("HTTP request cancelled".to_string()), metadata: serde_json::json!({"run_id": ctx.run_id, "cancelled": true}) };
                    }
                    result = request.send() => result.map_err(|e| e.to_string())
                }
            };
            match response {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    if (200..300).contains(&status_code) {
                        self.running_requests.lock().await.remove(&ctx.run_id);
                        if let Some(sink) = &ctx.log_sink { sink.on_log(StdioKind::Stdout, &body).await; }
                        return AdapterExecutionResult { status: ExecutionStatus::Ok, exit_code: Some(0), output: body, error: None, metadata: serde_json::json!({"run_id": ctx.run_id, "status": status_code, "attempt": attempt + 1}) };
                    }
                    last_error = Some(format!("HTTP invoke failed with status {}: {}", status_code, body));
                    if status_code < 500 && status_code != 429 { break; }
                }
                Err(error) => {
                    last_error = Some(if error == "timeout" { format!("HTTP request timed out after {}ms", timeout_ms) } else { format!("HTTP request failed: {}", error) });
                }
            }
            if attempt < retries { tokio::time::sleep(Duration::from_millis(50 * (attempt + 1))).await; }
        }
        self.running_requests.lock().await.remove(&ctx.run_id);
        AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: last_error, metadata: serde_json::json!({"run_id": ctx.run_id}) }
    }

    async fn cancel(&self, run_id: &str) {
        if let Some(sender) = self.running_requests.lock().await.remove(run_id) { let _ = sender.send(()); }
    }
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            running_processes: Arc::new(Mutex::new(HashMap::new())),
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

        // Paperclip uses config.cwd for process adapters; retain the execution
        // context as a fallback for callers that already resolved the directory.
        if let Some(dir) = config.get("cwd").and_then(|v| v.as_str()).or(ctx.working_dir.as_deref()) {
            cmd.current_dir(dir);
        }

        // 设置环境变量
        if let Some(env) = config.get("env").and_then(|v| v.as_object()) {
            for (key, value) in env {
                // Never allow adapter configuration to forge the run identity
                // or the API credential namespace. These are runtime-owned in
                // Paperclip and must not be inherited from user configuration.
                if key == "PAPERCLIP_API_KEY" || key == "PAPERCLIP_RUN_ID" { continue; }
                if let Some(val) = value.as_str() {
                    cmd.env(key, val);
                }
            }
        }
        cmd.env("PAPERCLIP_RUN_ID", &ctx.run_id);

        // 执行命令
        let timeout_seconds = config.get("timeoutSec")
            .or_else(|| config.get("timeout_sec"))
            .or_else(|| config.get("timeout"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let grace_seconds = config.get("graceSec")
            .or_else(|| config.get("grace_sec"))
            .and_then(|v| v.as_u64())
            .unwrap_or(15);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some(format!("Failed to execute: {}", e)), metadata: serde_json::json!({"run_id": ctx.run_id}) },
        };
        if let Some(pid) = child.id() { self.running_processes.lock().await.insert(ctx.run_id.clone(), pid); }
        let run_id = ctx.run_id.clone();
        let stdout_task = child.stdout.take().map(|reader| {
            let sink = ctx.log_sink.clone();
            tokio::spawn(read_process_stream(reader, sink, StdioKind::Stdout))
        });
        let stderr_task = child.stderr.take().map(|reader| {
            let sink = ctx.log_sink.clone();
            tokio::spawn(read_process_stream(reader, sink, StdioKind::Stderr))
        });
        let wait_result = if timeout_seconds == 0 {
            child.wait().await.map_err(|e| e.to_string())
        } else {
            match timeout(Duration::from_secs(timeout_seconds), child.wait()).await {
                Ok(result) => result.map_err(|e| e.to_string()),
                Err(_) => {
                    if let Some(pid) = self.running_processes.lock().await.get(&run_id).copied() { terminate_process(pid, grace_seconds).await; }
                    let _ = child.wait().await;
                    let _ = join_stream(stdout_task).await;
                    let _ = join_stream(stderr_task).await;
                    self.running_processes.lock().await.remove(&run_id);
                    return AdapterExecutionResult { status: ExecutionStatus::Error, exit_code: None, output: String::new(), error: Some(format!("Timed out after {}s", timeout_seconds)), metadata: serde_json::json!({"run_id": run_id, "timed_out": true}) };
                }
            }
        };
        self.running_processes.lock().await.remove(&run_id);
        let stdout = join_stream(stdout_task).await;
        let stderr = join_stream(stderr_task).await;
        match wait_result {
            Ok(status) => {
                let stdout = String::from_utf8_lossy(&stdout).to_string();
                let stderr = String::from_utf8_lossy(&stderr).to_string();

                let execution_status = if status.success() {
                    ExecutionStatus::Ok
                } else {
                    ExecutionStatus::Error
                };

                AdapterExecutionResult {
                    status: execution_status,
                    exit_code: status.code(),
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
        if let Some(pid) = self.running_processes.lock().await.remove(run_id) {
            kill_process(pid).await;
        }
    }
}

async fn read_process_stream<R: AsyncRead + Unpin>(mut reader: R, sink: Option<Arc<dyn LogSink>>, stream: StdioKind) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(size) => {
                output.extend_from_slice(&chunk[..size]);
                if let Some(sink) = &sink {
                    sink.on_log(stream, &String::from_utf8_lossy(&chunk[..size])).await;
                }
            }
            Err(_) => break,
        }
    }
    output
}

async fn join_stream(task: Option<tokio::task::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    match task { Some(task) => task.await.unwrap_or_default(), None => Vec::new() }
}

async fn terminate_process(pid: u32, grace_seconds: u64) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T"]).status().await;
        if grace_seconds > 0 { tokio::time::sleep(Duration::from_secs(grace_seconds)).await; }
    }
    kill_process(pid).await;
}

async fn kill_process(pid: u32) {
    #[cfg(windows)]
    let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status().await;
    #[cfg(not(windows))]
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status().await;
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestSink(AtomicUsize);

    #[async_trait]
    impl LogSink for TestSink {
        async fn on_log(&self, _stream: StdioKind, _chunk: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn context(run_id: &str, args: Vec<&str>, config_extra: serde_json::Value) -> AdapterExecutionContext {
        let mut config = serde_json::json!({"command": "cmd", "args": args, "env": {"PARROT_PROCESS_TEST": "ok", "PAPERCLIP_RUN_ID": "forged"}});
        if let (Some(base), Some(extra)) = (config.as_object_mut(), config_extra.as_object()) {
            for (key, value) in extra { base.insert(key.clone(), value.clone()); }
        }
        AdapterExecutionContext {
            run_id: run_id.to_string(),
            agent_id: "test-agent".to_string(),
            config,
            working_dir: None,
            execution_target: ExecutionTargetConfig { target_type: ExecutionTargetType::Local, connection_info: None, asset_sync_config: None },
            log_sink: Some(Arc::new(TestSink(AtomicUsize::new(0)))),
        }
    }

    #[tokio::test]
    async fn process_captures_env_stdout_stderr_and_exit_code() {
        let executor = LocalExecutor::new();
        let ctx = context("process-output", vec!["/C", "echo %PARROT_PROCESS_TEST% & echo %PAPERCLIP_RUN_ID% & echo stderr 1>&2 & exit /B 7"], serde_json::json!({}));
        let result = executor.execute(ctx).await;
        assert_eq!(result.status, ExecutionStatus::Error);
        assert_eq!(result.exit_code, Some(7));
        assert!(result.output.contains("ok"));
        assert!(result.output.contains("process-output"));
        assert!(!result.output.contains("forged"));
        assert!(result.error.as_deref().unwrap_or_default().contains("stderr"));
    }

    #[tokio::test]
    async fn process_timeout_returns_timed_out_result() {
        let executor = LocalExecutor::new();
        let ctx = context("process-timeout", vec!["-NoProfile", "-Command", "Start-Sleep -Seconds 5"], serde_json::json!({"command": "powershell", "timeoutSec": 1}));
        let result = executor.execute(ctx).await;
        assert_eq!(result.status, ExecutionStatus::Error);
        assert!(result.error.as_deref().unwrap_or_default().contains("Timed out"));
        assert_eq!(result.metadata.get("timed_out"), Some(&serde_json::Value::Bool(true)));
    }

    #[tokio::test]
    async fn process_cancel_terminates_registered_child() {
        let executor = Arc::new(LocalExecutor::new());
        let task_executor = executor.clone();
        let task = tokio::spawn(async move {
            task_executor.execute(context("process-cancel", vec!["-NoProfile", "-Command", "Start-Sleep -Seconds 30"], serde_json::json!({"command": "powershell"}))).await
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        executor.cancel("process-cancel").await;
        let result = tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
        assert_eq!(result.status, ExecutionStatus::Error);
        assert!(result.exit_code.is_some() || result.error.is_some());
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn test_server(status: u16, body: &'static str, delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8192];
            let size = socket.read(&mut request).await.unwrap();
            if status == 200 { assert!(String::from_utf8_lossy(&request[..size]).contains("agent-1")); }
            tokio::time::sleep(delay).await;
            let response = format!("HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", status, body.len(), body);
            let _ = socket.write_all(response.as_bytes()).await;
        });
        format!("http://{}", address)
    }

    fn context(run_id: &str, url: String, extra: serde_json::Value) -> AdapterExecutionContext {
        let mut config = serde_json::json!({"url": url, "payloadTemplate": {"kind": "test"}});
        if let (Some(base), Some(extra)) = (config.as_object_mut(), extra.as_object()) {
            for (key, value) in extra { base.insert(key.clone(), value.clone()); }
        }
        AdapterExecutionContext {
            run_id: run_id.to_string(), agent_id: "agent-1".to_string(), config,
            working_dir: None,
            execution_target: ExecutionTargetConfig { target_type: ExecutionTargetType::Local, connection_info: None, asset_sync_config: None },
            log_sink: None,
        }
    }

    #[tokio::test]
    async fn http_posts_payload_and_maps_success() {
        let url = test_server(200, "{\"ok\":true}", Duration::ZERO).await;
        let result = HttpExecutor::new().execute(context("http-success", url, serde_json::json!({}))).await;
        assert_eq!(result.status, ExecutionStatus::Ok);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.output.contains("ok"));
    }

    #[tokio::test]
    async fn http_maps_non_success_and_timeout() {
        let url = test_server(503, "down", Duration::ZERO).await;
        let result = HttpExecutor::new().execute(context("http-error", url, serde_json::json!({}))).await;
        assert_eq!(result.status, ExecutionStatus::Error);
        assert!(result.error.as_deref().unwrap_or_default().contains("503"));

        let url = test_server(200, "slow", Duration::from_millis(200)).await;
        let result = HttpExecutor::new().execute(context("http-timeout", url, serde_json::json!({"timeoutMs": 20}))).await;
        assert_eq!(result.status, ExecutionStatus::Error);
        assert!(result.error.as_deref().unwrap_or_default().contains("timed out"));
    }

    #[tokio::test]
    async fn http_cancel_aborts_in_flight_request() {
        let url = test_server(200, "slow", Duration::from_secs(5)).await;
        let executor = Arc::new(HttpExecutor::new());
        let task_executor = executor.clone();
        let task = tokio::spawn(async move { task_executor.execute(context("http-cancel", url, serde_json::json!({}))).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        executor.cancel("http-cancel").await;
        let result = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap();
        assert_eq!(result.status, ExecutionStatus::Error);
        assert_eq!(result.metadata.get("cancelled"), Some(&serde_json::Value::Bool(true)));
    }
}
