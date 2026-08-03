use async_trait::async_trait;
use chrono::{DateTime, Utc};
use models::Agent;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

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

    /// Cancel an active run for an issue
    /// Called after force_release to stop ongoing execution
    async fn cancel_run(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<(), HeartbeatError>;

    /// Get heartbeat context for an issue (diagnostics/monitoring)
    async fn get_heartbeat_context(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<HeartbeatContext, HeartbeatError>;
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
}

impl DefaultHeartbeatService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            children: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn load_agent(&self, id: Uuid) -> Result<Agent, HeartbeatError> {
        sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| HeartbeatError::Internal(e.to_string()))?
            .ok_or(HeartbeatError::AgentNotFound(id))
    }

    async fn execute_run(&self, run_id: Uuid, agent_id: Uuid, issue_id: Uuid, company_id: Uuid) {
        let result = self.run_command(run_id, agent_id, issue_id).await;
        let (status, exit_code, error, output) = match result {
            Ok((code, out)) if code == 0 => ("succeeded", Some(code), None, out),
            Ok((code, out)) => (
                "failed",
                Some(code),
                Some(format!(
                    "adapter exited with code {code}: {}",
                    out.lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or("no adapter output")
                )),
                out,
            ),
            Err(e) => ("failed", None, Some(e), String::new()),
        };
        let _ = sqlx::query(
            "UPDATE heartbeat_runs SET status = $2::heartbeat_run_status, exit_code = $3, error = $4, output = $5, finished_at = NOW(), updated_at = NOW() WHERE id = $1 AND status IN ('queued','running')")
            .bind(run_id).bind(status).bind(exit_code).bind(error).bind(output).execute(&self.pool).await;
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
    ) -> Result<(i32, String), String> {
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
        let cfg = agent.adapter_config.0;
        let adapter = agent.adapter_type.as_str();
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
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("DeepSeek-V4-Flash");
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
                configured_model
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
            return Ok((0, body));
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
            if matches!(adapter, "claude_local" | "codex_local" | "opencode") {
                if adapter == "codex_local" {
                    args.splice(1..1, ["--model".to_string(), configured_model.to_string()]);
                } else {
                    args.splice(0..0, ["--model".to_string(), configured_model.to_string()]);
                }
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
            if let Some(extra_args) = cfg.get("extraArgs").or_else(|| cfg.get("extra_args")) {
                if let Some(extra_args) = extra_args.as_array() {
                    args.extend(extra_args.iter().filter_map(|value| value.as_str().map(str::to_owned)));
                }
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
            .unwrap_or("http://127.0.0.1:3100/api/tool-gateway");
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
                            "headers": {"Authorization": "Bearer ${PAPERCLIP_TOOL_GATEWAY_TOKEN}"}
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
                    "mcp_servers.paperclip.bearer_token_env_var=\"PAPERCLIP_TOOL_GATEWAY_TOKEN\"".to_string(),
                ]);
            }
            _ => {}
        }
        // Do not accidentally inherit Claude Code's OpenAI compatibility mode
        // from the shell that launched parrot-server. Explicit per-agent env
        // values remain authoritative below.
        if adapter == "claude_local" {
            let explicit_env = cfg.get("env").and_then(|v| v.as_object());
            if explicit_env.map_or(true, |env| !env.contains_key("CLAUDE_CODE_USE_OPENAI")) {
                cmd.env_remove("CLAUDE_CODE_USE_OPENAI");
            }
            if explicit_env.map_or(true, |env| !env.contains_key("OPENAI_API_KEY")) {
                cmd.env_remove("OPENAI_API_KEY");
            }
            if explicit_env.map_or(true, |env| !env.contains_key("OPENAI_BASE_URL")) {
                cmd.env_remove("OPENAI_BASE_URL");
            }
            if explicit_env.map_or(true, |env| !env.contains_key("OPENAI_MODEL")) {
                cmd.env_remove("OPENAI_MODEL");
            }
        }
        let stdin_prompt = adapter == "claude_local" && !custom_args;
        let timeout_sec = cfg
            .get("timeoutSec")
            .or_else(|| cfg.get("timeout_sec"))
            .and_then(|v| v.as_u64())
            .filter(|value| *value > 0);
        cmd.args(args)
            .stdin(if stdin_prompt {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .env("PAPERCLIP_RUN_ID", run_id.to_string())
            .env("PAPERCLIP_AGENT_ID", agent_id.to_string())
            .env("PAPERCLIP_TOOL_GATEWAY_URL", gateway_url)
            .env("PAPERCLIP_TOOL_GATEWAY_TOKEN", gateway_token);
        if let Some(cwd) = cfg.get("cwd").and_then(|v| v.as_str()) {
            cmd.current_dir(cwd);
        }
        if let Some(env) = cfg.get("env").and_then(|v| v.as_object()) {
            for (k, v) in env {
                if let Some(s) = v.as_str() {
                    cmd.env(k, s);
                }
            }
        }
        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        sqlx::query("UPDATE heartbeat_runs SET status = 'running', started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND status = 'queued'").bind(run_id).execute(&self.pool).await.map_err(|e| e.to_string())?;
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
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let wait_result = timeout(
            timeout_sec.map(Duration::from_secs).unwrap_or(Duration::from_secs(u64::MAX)),
            async {
                let (_stdout_result, _stderr_result) =
                    tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
                child.wait().await
            },
        )
        .await;
        let status = match wait_result {
            Ok(status) => status.map_err(|e| e.to_string())?,
            Err(_) => {
                let _ = child.kill().await;
                return Err(format!(
                    "adapter timed out after {} seconds\n{}\n{}",
                    timeout_sec.unwrap_or(0),
                    String::from_utf8_lossy(&out),
                    String::from_utf8_lossy(&err),
                ));
            }
        };
        let mut output = String::from_utf8_lossy(&out).to_string();
        output.push_str(&String::from_utf8_lossy(&err));
        Ok((status.code().unwrap_or(-1), output))
    }
}

#[async_trait]
impl HeartbeatService for DefaultHeartbeatService {
    async fn wakeup(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
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
        let run_id: Uuid = sqlx::query_scalar("INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot) VALUES ($1,$2,'on_demand','queued',$3) RETURNING id")
            .bind(company_id).bind(agent_id).bind(serde_json::json!({"issueId": issue_id})).fetch_one(&self.pool).await.map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        sqlx::query("INSERT INTO agent_wakeup_requests (company_id, agent_id, status, payload) VALUES ($1,$2,'dispatched',$3)")
            .bind(company_id).bind(agent_id).bind(serde_json::json!({"issueId": issue_id, "runId": run_id})).execute(&self.pool).await.map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        sqlx::query("UPDATE issues SET assignee_agent_id = $2, assignee_user_id = NULL, status = CASE WHEN status IN ('todo','backlog') THEN 'in_progress'::issue_status ELSE status END, checkout_run_id = $3, execution_run_id = $3, started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE id = $1 AND company_id = $4 AND (assignee_agent_id IS NULL OR assignee_agent_id = $2) AND status NOT IN ('done','cancelled')")
            .bind(issue_id)
            .bind(agent_id)
            .bind(run_id)
            .bind(company_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        sqlx::query("UPDATE agents SET status = 'running', updated_at = NOW() WHERE id = $1")
            .bind(agent_id)
            .execute(&self.pool)
            .await
            .map_err(|e| HeartbeatError::WakeupFailed(e.to_string()))?;
        let service = self.clone_for_task();
        tokio::spawn(async move {
            service
                .execute_run(run_id, agent_id, issue_id, company_id)
                .await;
        });
        Ok(())
    }

    async fn cancel_run(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        reason: &str,
    ) -> Result<(), HeartbeatError> {
        let run: Option<Uuid> = sqlx::query_scalar("SELECT id FROM heartbeat_runs WHERE company_id=$1 AND agent_id=$2 AND status IN ('queued','running') AND (context_snapshot->>'issueId'=$3 OR context_snapshot->>'taskId'=$3) ORDER BY created_at DESC LIMIT 1")
            .bind(company_id).bind(agent_id).bind(issue_id.to_string()).fetch_optional(&self.pool).await.map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
        if let Some(run_id) = run {
            if let Some(child) = self.children.lock().await.remove(&run_id) {
                let _ = child.lock().await.kill().await;
            }
            sqlx::query("UPDATE heartbeat_runs SET status='cancelled', error=$2, finished_at=NOW(), updated_at=NOW() WHERE id=$1").bind(run_id).bind(reason).execute(&self.pool).await.map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
        }
        sqlx::query("UPDATE agent_wakeup_requests SET status='cancelled', updated_at=NOW() WHERE company_id=$1 AND agent_id=$2 AND status IN ('queued','dispatched','running') AND payload->>'issueId'=$3").bind(company_id).bind(agent_id).bind(issue_id.to_string()).execute(&self.pool).await.map_err(|e| HeartbeatError::CancelRunFailed(e.to_string()))?;
        Ok(())
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
            self.wakeup(agent_id, issue_id, company_id).await?;
            reconciled += 1;
        }
        Ok(reconciled)
    }

    fn clone_for_task(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            children: self.children.clone(),
        }
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    pub struct MockHeartbeatService {
        wakeup_count: AtomicI64,
        cancel_count: AtomicI64,
    }

    impl MockHeartbeatService {
        pub fn new() -> Self {
            Self {
                wakeup_count: AtomicI64::new(0),
                cancel_count: AtomicI64::new(0),
            }
        }

        pub fn wakeup_call_count(&self) -> i64 {
            self.wakeup_count.load(Ordering::Relaxed)
        }

        pub fn cancel_call_count(&self) -> i64 {
            self.cancel_count.load(Ordering::Relaxed)
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
