use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ============================================================================
// Constants
// ============================================================================

/// Maximum time to wait for a graceful worker shutdown (seconds)
const GRACEFUL_SHUTDOWN_TIMEOUT_SECS: u64 = 10;

/// Default RPC call timeout (seconds)
const DEFAULT_RPC_TIMEOUT_SECS: u64 = 30;

/// Maximum RPC call timeout (15 minutes)
const MAX_RPC_TIMEOUT_SECS: u64 = 900;

/// Initial restart backoff delay (milliseconds)
const INITIAL_RESTART_BACKOFF_MS: u64 = 1000;

/// Maximum restart backoff delay (5 minutes)
const MAX_RESTART_BACKOFF_MS: u64 = 300_000;

/// Maximum consecutive crashes before giving up
const MAX_CONSECUTIVE_CRASHES: u32 = 5;

/// Maximum stderr excerpt length (characters)
const MAX_STDERR_EXCERPT_CHARS: usize = 2000;

// ============================================================================
// Types
// ============================================================================

/// Worker process status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Worker has not been started yet
    Idle,
    /// Worker is starting (process spawned, waiting for initialize)
    Starting,
    /// Worker is running and ready to accept calls
    Running,
    /// Worker is stopping (graceful shutdown in progress)
    Stopping,
    /// Worker has stopped cleanly
    Stopped,
    /// Worker crashed and may restart
    Crashed,
    /// Worker failed to start or exceeded max crashes
    Failed,
}

/// JSON-RPC 2.0 request ID
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    String(String),
    Number(i64),
}

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    pub params: Value,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse {
    Success {
        jsonrpc: String,
        id: JsonRpcId,
        result: Value,
    },
    Error {
        jsonrpc: String,
        id: JsonRpcId,
        error: JsonRpcError,
    },
}

/// JSON-RPC 2.0 error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Plugin manifest (simplified from Paperclip's PaperclipPluginManifestV1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub proactive: bool,
}

/// Options for starting a worker process
#[derive(Debug, Clone)]
pub struct WorkerStartOptions {
    /// Absolute path to the plugin worker entrypoint
    pub entrypoint_path: String,
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Resolved plugin configuration
    pub config: Value,
    /// Host instance information
    pub instance_info: InstanceInfo,
    /// Host API version
    pub api_version: i32,
    /// Host-derived plugin database namespace
    pub database_namespace: Option<String>,
    /// Default timeout for RPC calls (ms)
    pub rpc_timeout_ms: Option<u64>,
    /// Whether to auto-restart on crash
    pub auto_restart: bool,
    /// Node.js execArgv passed to e child process
    pub exec_argv: Vec<String>,
    /// Environment variables passed to the child process
    pub env: HashMap<String, String>,
    /// Companies this worker may act on from proactive calls
    pub proactive_company_scopes: Vec<String>,
}

/// Host instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub host_version: String,
}

/// Diagnostic information about a worker process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDiagnostics {
    pub plugin_id: String,
    pub status: WorkerStatus,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub consecutive_crashes: u32,
    pub total_crashes: u32,
    pub pending_requests: usize,
    pub last_crash_at: Option<DateTime<Utc>>,
    pub next_restart_at: Option<DateTime<Utc>>,
}

/// A pending RPC call waiting for a response
struct PendingRequest {
    id: JsonRpcId,
    method: String,
    sender: tokio::sync::oneshot::Sender<Result<Value, PluginWorkerError>>,
    sent_at: DateTime<Utc>,
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum PluginWorkerError {
    #[error("worker not running: {0}")]
    NotRunning(String),
    
    #[error("worker already exists: {0}")]
    AlreadyExists(String),
    
    #[error("worker failed to start: {0}")]
    StartFailed(String),
    
    #[error("RPC call timeout: {method} after {timeout_ms}ms")]
    RpcTimeout { method: String, timeout_ms: u64 },
    
    #[error("RPC call error: {0}")]
    RpcError(String),
    
    #[error("worker crashed: {0}")]
    Crashed(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("interror: {0}")]
    Internal(String),
}

pub type WorkerResult<T> = Result<T, PluginWorkerError>;

// ============================================================================
// PluginWorkerHandle — manages a single worker process
// ============================================================================

struct WorkerState {
    /// Child process handle
    child: Option<Child>,
    /// Current status
    status: WorkerStatus,
    /// Process ID
    pid: Option<u32>,
    /// Pending RPC requests
    pending_requests: HashMap<String, PendingRequest>,
    /// Consecutive crash counter
    consecutive_crashes: u32,
    /// Total crash counter
    total_crashes: u32,
    /// Last crash timestamp
    last_crash_at: Option<DateTime<Utc>>,
    /// Worker start timestamp
    started_at: Option<DateTime<Utc>>,
    /// Stderr excerpt from the last crash
    stderr_excerpt: String,
    /// Earliest time at which an automatic restart may begin
    next_restart_at: Option<DateTime<Utc>>,
    /// Authorized company scopes for proactive calls
    proactive_company_scopes: Vec<String>,
    /// Supported methods reported by the worker
    supported_methods: Vec<String>,
}

/// Handle for a single plugin worker process
pub struct PluginWorkerHandle {
    plugin_id: String,
    options: WorkerStartOptions,
    state: Arc<Mutex<WorkerState>>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    restart_tx: OnceLock<UnboundedSender<()>>,
}

impl PluginWorkerHandle {
    /// Create a new worker handle (does not start the process yet)
    pub fn new(plugin_id: String, options: WorkerStartOptions) -> Self {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
        
        Self {
            plugin_id: plugin_id.clone(),
            options,
            state: Arc::new(Mutex::new(WorkerState {
                child: None,
                status: WorkerStatus::Idle,
                pid: None,
                pending_requests: HashMap::new(),
                consecutive_crashes: 0,
                total_crashes: 0,
                last_crash_at: None,
                started_at: None,
                stderr_excerpt: String::new(),
                next_restart_at: None,
                proactive_company_scopes: vec![],
                supported_methods: vec![],
            })),
            shutdown_tx,
            restart_tx: OnceLock::new(),
        }
    }

    /// Attach the manager-owned restart channel. The manager owns the receiver
    /// and the worker monitor only requests a restart after its backoff.
    fn attach_restart_sender(&self, restart_tx: UnboundedSender<()>) {
        let _ = self.restart_tx.set(restart_tx);
    }
    
    /// Get the plugin ID
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
    
    /// Get current worker status
    pub async fn status(&self) -> WorkerStatus {
        self.state.lock().await.status
    }
    
    /// Start the worker process
    pub async fn start(&self) -> WorkerResult<()> {
        let mut state = self.state.lock().await;
        
        if !matches!(
            state.status,
            WorkerStatus::Idle | WorkerStatus::Stopped | WorkerStatus::Crashed
        ) {
            return Err(PluginWorkerError::StartFailed(
                format!("worker already started: {:?}", state.status)
            ));
        }
        
        info!(plugin_id = %self.plugin_id, "starting plugin worker");
        
        state.status = WorkerStatus::Starting;
        state.started_at = Some(Utc::now());
        state.stderr_excerpt.clear();
        state.next_restart_at = None;
        let _ = self.shutdown_tx.send(false);
        
        // Spawn the worker process
        let mut cmd = Command::new("node");
        
        // Add exec arguments
        for arg in &self.options.exec_argv {
            cmd.arg(arg);
        }
        
        // Add entrypoint
        cmd.arg(&self.options.entrypoint_path);
        
        // Set up stdio
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        // Set environment variables
        for (key, value) in &self.options.env {
            cmd.env(key, value);
        }
        
        // Spawn the child process
        let mut child = cmd.spawn()
            .map_err(|e| PluginWorkerError::StartFailed(format!("failed to spawn process: {}", e)))?;
        
        let pid = child.id();
        state.pid = pid;
        
        info!(plugin_id = %self.plugin_id, pid = ?pid, "worker process spawned");
        
        // Take stdout/stderr (stdin stays in child for RPC calls)
        let stdout = child.stdout.take()
            .ok_or_else(|| PluginWorkerError::Internal("failed to take stdout".into()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| PluginWorkerError::Internal("failed to take stderr".into()))?;
        
        state.child = Some(child);
        drop(state);

        // Monitor the process independently from stdout/stderr so an exit
        // also rejects pending RPC calls and can trigger bounded recovery.
        let state_clone = Arc::clone(&self.state);
        let plugin_id_clone = self.plugin_id.clone();
        let restart_tx = self.restart_tx.get().cloned();
        let auto_restart = self.options.auto_restart;
        tokio::spawn(async move {
            Self::monitor_process(state_clone, plugin_id_clone, restart_tx, auto_restart).await;
        });
        
        // Spawn stdout reader task
        let state_clone = Arc::clone(&self.state);
        let plugin_id_clone = self.plugin_id.clone();
        tokio::spawn(async move {
            Self::read_stdout_loop(state_clone, plugin_id_clone, stdout).await;
        });
        
        // Spawn stderr reader task
        let state_clone = Arc::clone(&self.state);
        let plugin_id_clone = self.plugin_id.clone();
        tokio::spawn(async move {
            Self::read_stderr_loop(state_clone, plugin_id_clone, stderr).await;
        });
        
        // Send initialize RPC call
        let init_params = serde_json::json!({
            "manifest": self.options.manifest,
            "config": self.options.config,
            "instanceInfo": self.options.instance_info,
            "apiVersion": self.options.api_version,
            "databaseNamespace": self.options.database_namespace,
        });
        
        let init_result = self.call("initialize", init_params, None).await?;
        
        // Parse supported methods from initialize response
        if let Some(methods) = init_result.get("methods").and_then(|v| v.as_array()) {
            let mut state = self.state.lock().await;
            state.supported_methods = methods
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        
        // Mark as running
        let mut state = self.state.lock().await;
        if state.status != WorkerStatus::Starting {
            return Err(PluginWorkerError::Crashed(
                "worker exited while initializing".to_string(),
            ));
        }
        state.status = WorkerStatus::Running;
        state.consecutive_crashes = 0;
        
        info!(ugin_id = %self.plugin_id, pid = ?pid, "worker initialized successfully");
        
        Ok(())
    }
    
    /// Stop the worker process gracefully
    pub async fn stop(&self) -> WorkerResult<()> {
        let mut state = self.state.lock().await;
        
        if state.status == WorkerStatus::Stopped || state.status == WorkerStatus::Idle {
            return Ok(());
        }
        
        info!(plugin_id = %self.plugin_id, "stopping plugin worker");
        
        state.status = WorkerStatus::Stopping;
        
        // Signal shutdown
        let _ = self.shutdown_tx.send(true);
        
        // Send shutdown RPC call (best effort)
        drop(state);
        let _ = timeout(
            Duration::from_secs(2),
            self.call("shutdown", serde_json::json!({}), None)
        ).await;
        
        let mut state = self.state.lock().await;
        
        // Wait for graceful exit
        if let Some(child) = state.child.as_mut() {
            match timeout(Duration::from_secs(GRACEFUL_SHUTDOWN_TIMEOUT_SECS), child.wait()).await {
                Ok(Ok(exit_status)) => {
                    info!(plugin_id = %self.plugin_id, ?exit_status, "worker exited gracefully");
                }
                Ok(Err(e)) => {
                    warn!(plugin_id = %self.plugin_id, error = %e, "error waiting for worker exit");
                }
                Err(_) => {
                    // Timeout - escalate to SIGTERM
                    warn!(plugin_id = %self.plugin_id, "graceful shutdown timeout, sending SIGTERM");
                    let _ = child.kill().await;
                    
                    // Wait a bit more, then SIGKILL if needed
                    if timeout(Duration::from_secs(5), child.wait()).await.is_err() {
                        error!(plugin_id = %self.plugin_id, "SIGTERM failed, sending SIGKILL");
                        let _ = child.start_kill();
                    }
                }
            }
        }
        
        state.child = None;
        state.pid = None;
        state.status = WorkerStatus::Stopped;
        
        // Fail all pending requests
        for (_, pending) in state.pending_requests.drain() {
            let _ = pending.sender.send(Err(PluginWorkerError::NotRunning(
                "worker stopped".into()
            )));
        }
        
        info!(plugin_id = %self.plugin_id, "worker stopped");
        
        Ok(())
    }
    
    /// Restart the worker (stop + start)
    pub async fn restart(&self) -> WorkerResult<()> {
        info!(plugin_id = %self.plugin_id, "restarting plugin worker");
        self.stop().await?;
        self.start().await
    }
    
    /// Send an RPC call to the worker
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        timeout_ms: Option<u64>,
    ) -> WorkerResult<Value> {
        let state = self.state.lock().await;
        
        if state.status != WorkerStatus::Running && state.status != WorkerStatus::Starting {
            return Err(PluginWorkerError::NotRunning(
                format!("worker status: {:?}", state.status)
            ));
        }
        
        // Generate request ID
        let request_id = Uuid::new_v4().to_string();
        
        // Prepare JSON-RPC request
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: JsonRpcId::String(request_id.clone()),
            method: method.into(),
            params,
        };
        
        // Serialize request
        let request_json = serde_json::to_string(&request)?;
        
        // Create response channel
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        // Store pending request
        let pending = PendingRequest {
            id: JsonRpcId::String(request_id.clone()),
            method: method.into(),
            sender: tx,
            sent_at: Utc::now(),
        };
        
        drop(state);
        let mut state = self.state.lock().await;

        // Take stdin out of the state while awaiting I/O. Holding the state
        // mutex across write/flush would make the auto-restart monitor
        // non-Send and would serialize process health checks behind slow I/O.
        let child = state.child.as_mut()
            .ok_or_else(|| PluginWorkerError::Internal("child process not found".into()))?;
        let mut stdin = child.stdin.take()
            .ok_or_else(|| PluginWorkerError::Internal("stdin not available".into()))?;
        state.pending_requests.insert(request_id.clone(), pending);
        drop(state);

        let write_result = async {
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await
        }
        .await;

        let mut state = self.state.lock().await;
        if let Some(child) = state.child.as_mut() {
            child.stdin = Some(stdin);
        }
        if let Err(error) = write_result {
            state.pending_requests.remove(&request_id);
            return Err(PluginWorkerError::Io(error));
        }
        drop(state);
        
        debug!(plugin_id = %self.plugin_id, method, request_id, "RPC request sent");
        
        // Wait for response with timeout
        let final_timeout_ms = timeout_ms.unwrap_or(self.options.rpc_timeout_ms.unwrap_or(DEFAULT_RPC_TIMEOUT_SECS * 1000));
        let timeout_duration = Duration::from_millis(final_timeout_ms.min(MAX_RPC_TIMEOUT_SECS * 1000));
        
        match timeout(timeout_duration, rx).await {
            Ok(Ok(Ok(result))) => {
                debug!(plugin_id = %self.plugin_id, method, request_id, "RPC call succeeded");
                Ok(result)
            }
            Ok(Ok(Err(e))) => {
                warn!(plugin_id = %self.plugin_id, method, request_id, error = %e, "RPC call failed");
                Err(e)
            }
            Ok(Err(_)) => {
                // Channel closed (worker crashed?)
                Err(PluginWorkerError::Internal("response channel closed".into()))
            }
            Err(_) => {
                // Timeout - remove pending request
                let mut state = self.state.lock().await;
                state.pending_requests.remove(&request_id);
                
                Err(PluginWorkerError::RpcTimeout {
                    method: method.into(),
                    timeout_ms: final_timeout_ms,
                })
            }
        }
    }
    
    /// Set authorized company scopes for proactive calls
    pub async fn set_proactive_company_scopes(&self, company_ids: Vec<String>) {
        let mut state = self.state.lock().await;
        state.proactive_company_scopes = company_ids;
    }
    
    /// Get diagnostic information
    pub async fn diagnostics(&self) -> WorkerDiagnostics {
        let state = self.state.lock().await;
        
        let uptime_secs = state.started_at.map(|started| {
            (Utc::now() - started).num_seconds() as u64
        });
        
        WorkerDiagnostics {
            plugin_id: self.plugin_id.clone(),
            status: state.status,
            pid: state.pid,
            uptime_secs,
            consecutive_crashes: state.consecutive_crashes,
            total_crashes: state.total_crashes,
            pending_requests: state.pending_requests.len(),
            last_crash_at: state.last_crash_at,
            next_restart_at: state.next_restart_at,
        }
    }
    
    // ========================================================================
    // Internal methods
    // ========================================================================
    
    /// Read and process stdout from the worker
    async fn read_stdout_loop(
        state: Arc<Mutex<WorkerState>>,
        plugin_id: String,
        stdout: tokio::process::ChildStdout,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            
            // Try to parse as JSON-RPC response
            match serde_json::from_str::<JsonRpcResponse>(&line) {
                Ok(response) => {
                    Self::handle_rpc_response(Arc::clone(&state), &plugin_id, response).await;
                }
                Err(e) => {
                    warn!(plugin_id = %plugin_id, line, error = %e, "failed to parse worker output");
                }
            }
        }
        
        debug!(plugin_id = %plugin_id, "stdout reader terminated");
    }
    
    /// Read and capture stderr from the worker
    async fn read_stderr_loop(
        state: Arc<Mutex<WorkerState>>,
        plugin_id: String,
        stderr: tokio::process::ChildStderr,
    ) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        
        while let Ok(Some(line)) = lines.next_line().await {
            warn!(plugin_id = %plugin_id, stderr = %line, "worker stderr");
            
            // Append to stderr excerpt
            let mut state = state.lock().await;
            Self::append_stderr_excerpt(&mut state.stderr_excerpt, &line);
        }
        
        debug!(plugin_id = %plugin_id, "stderr reader terminated");
    }

    /// Watch the child process without holding the worker lock across an await.
    /// A clean stop sets `Stopping` first, so it is never mistaken for a crash.
    async fn monitor_process(
        state: Arc<Mutex<WorkerState>>,
        plugin_id: String,
        restart_tx: Option<UnboundedSender<()>>,
        auto_restart: bool,
    ) {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let crash = {
                let mut state = state.lock().await;
                if matches!(state.status, WorkerStatus::Stopping | WorkerStatus::Stopped | WorkerStatus::Idle) {
                    return;
                }

                let Some(child) = state.child.as_mut() else {
                    return;
                };
                match child.try_wait() {
                    Ok(Some(exit_status)) => {
                        state.child = None;
                        state.pid = None;
                        state.status = WorkerStatus::Crashed;
                        state.consecutive_crashes = state.consecutive_crashes.saturating_add(1);
                        state.total_crashes = state.total_crashes.saturating_add(1);
                        state.last_crash_at = Some(Utc::now());
                        state.next_restart_at = None;
                        for (_, pending) in state.pending_requests.drain() {
                            let _ = pending.sender.send(Err(PluginWorkerError::Crashed(
                                format!("worker exited with status {exit_status}"),
                            )));
                        }

                        let restart = auto_restart
                            && state.consecutive_crashes <= MAX_CONSECUTIVE_CRASHES
                            && restart_tx.is_some();
                        let delay = restart.then(|| Self::restart_backoff(state.consecutive_crashes));
                        if let Some(delay) = delay {
                            state.next_restart_at = Some(
                                Utc::now()
                                    + chrono::Duration::from_std(delay).unwrap_or_else(|_| chrono::Duration::minutes(5)),
                            );
                        } else {
                            state.status = WorkerStatus::Failed;
                        }
                        Some((restart, delay))
                    }
                    Ok(None) => None,
                    Err(error) => {
                        warn!(plugin_id = %plugin_id, error = %error, "failed to poll plugin worker");
                        state.child = None;
                        state.pid = None;
                        state.status = WorkerStatus::Failed;
                        for (_, pending) in state.pending_requests.drain() {
                            let _ = pending.sender.send(Err(PluginWorkerError::Crashed(
                                error.to_string(),
                            )));
                        }
                        return;
                    }
                }
            };

            let Some((restart, Some(delay))) = crash else {
                continue;
            };
            if !restart {
                return;
            }

            info!(plugin_id = %plugin_id, ?delay, "scheduling plugin worker restart");
            tokio::time::sleep(delay).await;
            if let Some(restart_tx) = restart_tx {
                let _ = restart_tx.send(());
            }
            return;
        }
    }

    fn restart_backoff(consecutive_crashes: u32) -> Duration {
        let shift = consecutive_crashes.saturating_sub(1).min(63);
        let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
        Duration::from_millis(
            INITIAL_RESTART_BACKOFF_MS
                .saturating_mul(multiplier)
                .min(MAX_RESTART_BACKOFF_MS),
        )
    }
    
    /// Handle a JSON-RPC response from the worker
    async fn handle_rpc_response(
        state: Arc<Mutex<WorkerState>>,
        plugin_id: &str,
        response: JsonRpcResponse,
    ) {
        let request_id = match &response {
            JsonRpcResponse::Success { id, .. } => id.clone(),
            JsonRpcResponse::Error { id, .. } => id.clone(),
        };
        
        let request_id_str = match &request_id {
            JsonRpcId::String(s) => s.clone(),
            JsonRpcId::Number(n) => n.to_string(),
        };
        
        let mut state = state.lock().await;
        
        if let Some(pending) = state.pending_requests.remove(&request_id_str) {
            match response {
                JsonRpcResponse::Success { result, .. } => {
                    let _ = pending.sender.send(Ok(result));
                }
                JsonRpcResponse::Error { error, .. } => {
                    let _ = pending.sender.send(Err(PluginWorkerError::RpcError(
                        format!("{}: {}", error.code, error.message)
                    )));
                }
            }
        } else {
            warn!(plugin_id = %plugin_id, ?request_id, "received response for unknown request");
        }
    }
    
    /// Append a line to the stderr excerpt, truncating if needed
    fn append_stderr_excerpt(excerpt: &mut String, line: &str) {
        if !excerpt.is_empty() {
            excerpt.push('\n');
        }
        excerpt.push_str(line);
        
        if excerpt.len() > MAX_STDERR_EXCERPT_CHARS {
            let start = excerpt.len() - MAX_STDERR_EXCERPT_CHARS;
            *excerpt = excerpt[start..].to_string();
        }
    }
}

// ============================================================================
// PluginWorkerManager — manages all plugin workers
// ============================================================================

/// The top-level manager that holds all plugin worker handles
pub struct PluginWorkerManager {
    workers: Arc<RwLock<HashMap<String, Arc<PluginWorkerHandle>>>>,
}

impl PluginWorkerManager {
    /// Create a new plugin worker manager
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register and start a worker for a plugin
    pub async fn start_worker(
        &self,
        plugin_id: String,
        options: WorkerStartOptions,
    ) -> WorkerResult<Arc<PluginWorkerHandle>> {
        let mut workers = self.workers.write().await;
        
        if workers.contains_key(&plugin_id) {
            return Err(PluginWorkerError::AlreadyExists(plugin_id));
        }
        
        let handle = Arc::new(PluginWorkerHandle::new(plugin_id.clone(), options));
        let (restart_tx, mut restart_rx) = tokio::sync::mpsc::unbounded_channel();
        handle.attach_restart_sender(restart_tx);
        let handle_ref = Arc::downgrade(&handle);
        tokio::spawn(async move {
            while restart_rx.recv().await.is_some() {
                let Some(handle) = handle_ref.upgrade() else {
                    return;
                };
                if handle.status().await != WorkerStatus::Crashed {
                    continue;
                }
                if let Err(error) = handle.start().await {
                    error!(plugin_id = %handle.plugin_id(), error = %error, "plugin worker restart failed");
                    let mut state = handle.state.lock().await;
                    state.status = WorkerStatus::Failed;
                    state.next_restart_at = None;
                }
            }
        });
        handle.start().await?;
        
        workers.insert(plugin_id, Arc::clone(&handle));
        
        Ok(handle)
    }
    
    /// Stop and unregister a specific plugin worker
    pub async fn stop_worker(&self, plugin_id: &Uuid) -> WorkerResult<()> {
        let mut workers = self.workers.write().await;
        
        if let Some(handle) = workers.remove(&plugin_id.to_string()) {
            handle.stop().await?;
        }
        
        Ok(())
    }
    
    /// Get the worker handle for a plugin
    pub async fn get_worker(&self, plugin_id: &Uuid) -> Option<Arc<PluginWorkerHandle>> {
        let workers = self.workers.read().await;
        workers.get(&plugin_id.to_string()).map(Arc::clone)
    }
    
    /// Check if a worker is registered and running
    pub async fn is_running(&self, plugin_id: &Uuid) -> bool {
        if let Some(handle) = self.get_worker(plugin_id).await {
            handle.status().await == WorkerStatus::Running
        } else {
            false
        }
    }
    
    /// Set proactive company scopes for a plugin worker
    pub async fn set_proactive_company_scopes(
        &self,
        plugin_id: &Uuid,
        company_ids: Vec<String>,
    ) {
        if let Some(handle) = self.get_worker(plugin_id).await {
            handle.set_proactive_company_scopes(company_ids).await;
        }
    }
    
    /// Stop all managed workers
    pub async fn stop_all(&self) -> WorkerResult<()> {
        let workers = {
            let workers = self.workers.read().await;
            workers.values().map(Arc::clone).collect::<Vec<_>>()
        };
        
        info!(count = workers.len(), "stopping all plugin workers");
        
        for handle in workers {
            if let Err(e) = handle.stop().await {
                warn!(plugin_id = %handle.plugin_id(), error = %e, "failed to stop worker");
            }
        }
        
        self.workers.write().await.clear();
        
        Ok(())
    }
    
    /// Get diagnostic info for all workers
    pub async fn diagnostics(&self) -> Vec<WorkerDiagnostics> {
        let workers = self.workers.read().await;
        let mut diagnostics = Vec::new();
        
        for handle in workers.values() {
            diagnostics.push(handle.diagnostics().await);
        }
        
        diagnostics
    }
    
    /// Send an RPC call to a specific plugin worker
    pub async fn call(
        &self,
        plugin_id: &Uuid,
        method: &str,
        params: Value,
        timeout_ms: Option<u64>,
    ) -> WorkerResult<Value> {
        let handle = self.get_worker(plugin_id).await
            .ok_or_else(|| PluginWorkerError::NotRunning(format!("worker not found: {}", plugin_id)))?;
        
        handle.call(method, params, timeout_ms).await
    }
}

impl Default for PluginWorkerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_worker_manager_creation() {
        let manager = PluginWorkerManager::new();
        assert_eq!(manager.diagnostics().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_worker_status_transitions() {
        let options = WorkerStartOptions {
            entrypoint_path: "/path/to/worker.js".into(),
            manifest: PluginManifest {
                name: "test-plugin".into(),
                version: "1.0.0".into(),
                description: None,
                methods: vec![],
                proactive: false,
            },
            config: serde_json::json!({}),
            instance_info: InstanceInfo {
                instance_id: "test-instance".into(),
                host_version: "1.0.0".into(),
            },
            api_version: 1,
            database_namespace: None,
            rpc_timeout_ms: None,
            auto_restart: true,
            exec_argv: vec![],
            env: HashMap::new(),
            proactive_company_scopes: vec![],
        };
        
        let handle = PluginWorkerHandle::new("test-plugin".into(), options);
        assert_eq!(handle.status().await, WorkerStatus::Idle);
    }

    #[tokio::test]
    async fn worker_process_crash_is_restarted_and_pending_state_is_recovered() {
        let dir = tempdir().expect("temporary plugin directory");
        let marker_path = dir.path().join("starts");
        let entrypoint_path = dir.path().join("worker.js");
        fs::write(
            &entrypoint_path,
            r#"
const fs = require("fs");
const marker = process.env.PARROT_PLUGIN_MARKER;
let starts = 0;
try { starts = Number(fs.readFileSync(marker, "utf8")); } catch (_) {}
starts += 1;
fs.writeFileSync(marker, String(starts));
const crashAfterInitialize = starts === 1;
let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  const lines = buffer.split("\n");
  buffer = lines.pop();
  for (const line of lines) {
    if (!line.trim()) continue;
    const request = JSON.parse(line);
    if (request.method === "initialize") {
      process.stdout.write(JSON.stringify({jsonrpc: "2.0", id: request.id, result: {methods: []}}) + "\n");
      if (crashAfterInitialize) setTimeout(() => process.exit(17), 25);
    } else if (request.method === "shutdown") {
      process.stdout.write(JSON.stringify({jsonrpc: "2.0", id: request.id, result: {}}) + "\n");
      setTimeout(() => process.exit(0), 5);
    }
  }
});
"#,
        )
        .expect("write plugin fixture");

        let plugin_id = Uuid::new_v4();
        let mut env = HashMap::new();
        env.insert(
            "PARROT_PLUGIN_MARKER".to_string(),
            marker_path.to_string_lossy().into_owned(),
        );
        let options = WorkerStartOptions {
            entrypoint_path: entrypoint_path.to_string_lossy().into_owned(),
            manifest: PluginManifest {
                name: "crash-recovery-plugin".into(),
                version: "1.0.0".into(),
                description: None,
                methods: vec![],
                proactive: false,
            },
            config: serde_json::json!({}),
            instance_info: InstanceInfo {
                instance_id: "test-instance".into(),
                host_version: "1.0.0".into(),
            },
            api_version: 1,
            database_namespace: None,
            rpc_timeout_ms: None,
            auto_restart: true,
            exec_argv: vec![],
            env,
            proactive_company_scopes: vec![],
        };

        let manager = PluginWorkerManager::new();
        let handle = manager
            .start_worker(plugin_id.to_string(), options)
            .await
            .expect("initial plugin worker start");

        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            let diagnostics = handle.diagnostics().await;
            if diagnostics.total_crashes == 1
                && diagnostics.status == WorkerStatus::Running
                && fs::read_to_string(&marker_path)
                    .map(|starts| starts == "2")
                    .unwrap_or(false)
            {
                assert_eq!(diagnostics.consecutive_crashes, 0);
                assert!(diagnostics.next_restart_at.is_none());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not recover: {diagnostics:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        manager
            .stop_worker(&plugin_id)
            .await
            .expect("stop recovered worker");
        assert_eq!(handle.status().await, WorkerStatus::Stopped);
    }

    #[test]
    fn restart_backoff_is_bounded_and_exponential() {
        assert_eq!(PluginWorkerHandle::restart_backoff(0), Duration::from_millis(1000));
        assert_eq!(PluginWorkerHandle::restart_backoff(1), Duration::from_millis(1000));
        assert_eq!(PluginWorkerHandle::restart_backoff(2), Duration::from_millis(2000));
        assert_eq!(PluginWorkerHandle::restart_backoff(8), Duration::from_millis(128_000));
        assert_eq!(PluginWorkerHandle::restart_backoff(10), Duration::from_millis(MAX_RESTART_BACKOFF_MS));
        assert_eq!(PluginWorkerHandle::restart_backoff(32), Duration::from_millis(MAX_RESTART_BACKOFF_MS));
    }
}

// Re-export as WorkerError for backward compatibility with existing code
pub type WorkerError = PluginWorkerError;
