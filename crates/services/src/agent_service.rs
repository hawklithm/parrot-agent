use async_trait::async_trait;
use chrono::{Datelike, Utc};
use models::{Agent, AgentApiKey, AgentPermissions, AgentRuntimeState, AgentStatus, AgentTaskSession};
use repositories::{
    ActivityLogRepository, AgentApiKeyRepository, AgentRepository, ConfigRevisionRepository,
    CostEventRepository, ListAgentsOptions,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// ConfigSnapshot - 配置快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub adapter_type: String,
    pub adapter_config: serde_json::Value,
    pub runtime_config: serde_json::Value,
    pub permissions: serde_json::Value,
    pub budget_monthly_cents: i32,
}

impl ConfigSnapshot {
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            adapter_type: agent.adapter_type.clone(),
            adapter_config: agent.adapter_config.0.clone(),
            runtime_config: agent.runtime_config.0.clone(),
            permissions: serde_json::to_value(&agent.permissions.0).unwrap_or_default(),
            budget_monthly_cents: agent.budget_monthly_cents,
        }
    }
}

/// CreateAgentInput - Agent 创建输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentInput {
    pub company_id: Uuid,
    pub name: String,
    pub role: models::AgentRole,
    pub status: Option<AgentStatus>,
    pub adapter_type: String,
    pub adapter_config: serde_json::Value,
    pub runtime_config: Option<serde_json::Value>,
    pub permissions: Option<models::AgentPermissions>,
    pub budget_monthly_cents: Option<i32>,
    pub reports_to: Option<Uuid>,
}

/// UpdateAgentInput - Agent 更新输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAgentInput {
    pub name: Option<String>,
    pub role: Option<models::AgentRole>,
    pub status: Option<AgentStatus>,
    pub adapter_type: Option<String>,
    pub adapter_config: Option<serde_json::Value>,
    pub runtime_config: Option<serde_json::Value>,
    pub budget_monthly_cents: Option<i32>,
    pub reports_to: Option<Uuid>,
}

/// NormalizedAgentRow - 规范化的 Agent 数据（含花费和健康度）
#[derive(Debug, Clone, Serialize)]
pub struct NormalizedAgentRow {
    #[serde(flatten)]
    pub agent: Agent,
    pub spent_monthly_cents: i32,
    pub org_chain_health: f32,
}

/// AgentService trait - Agent 业务逻辑服务
#[async_trait]
pub trait AgentService: Send + Sync {
    /// 创建 Agent
    async fn create(&self, input: CreateAgentInput) -> Result<Agent, ServiceError>;

    /// 获取单个 Agent
    async fn get_by_id(&self, id: Uuid) -> Result<Agent, ServiceError>;

    /// 获取当前认证的 Agent
    async fn get_me(&self, agent_key: &str) -> Result<Agent, ServiceError>;

    /// 列出公司的所有 Agent
    async fn list(&self, company_id: Uuid) -> Result<Vec<NormalizedAgentRow>, ServiceError>;

    /// 更新 Agent
    async fn update(&self, id: Uuid, input: UpdateAgentInput) -> Result<Agent, ServiceError>;

    /// 删除 Agent（软删除 + 资源清理）
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;

    /// 终止 Agent：置为 Terminated 并执行资源清理（撤销 Key、重挂子节点、审计）。
    ///
    /// 语义对齐 Paperclip：`terminate` 与 `delete` 最终都进入 terminated 状态，
    /// 并触发相同的资源清理策略。
    async fn terminate(&self, id: Uuid) -> Result<Agent, ServiceError>;

    /// 检测汇报循环
    async fn detect_reporting_cycle(
        &self,
        agent_id: Uuid,
        reports_to: Uuid,
    ) -> Result<bool, ServiceError>;

    /// 计算组织链健康度
    async fn get_agent_work_eligibility(&self, agent_id: Uuid) -> Result<f32, ServiceError>;

    /// 获取公司的组织架构树
    async fn org_for_company(&self, company_id: Uuid) -> Result<Vec<models::OrgNode>, ServiceError>;

    /// 获取Agent的管理链（从直接上级到最高层级）
    async fn get_chain_of_command(&self, agent_id: Uuid) -> Result<Vec<models::OrgNode>, ServiceError>;

    /// 回滚配置到指定版本
    async fn rollback_config_revision(
        &self,
        agent_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Agent, ServiceError>;

    /// 获取Agent技能快照
    async fn get_skills(&self, agent_id: Uuid) -> Result<models::AgentSkillSnapshot, ServiceError>;

    /// 同步Agent技能列表：将 desired_skills 写入 agent 的
    /// `adapter_config.desired_skills`（并捕获配置快照以支持回滚），返回最新快照。
    async fn sync_skills(
        &self,
        agent_id: Uuid,
        desired_skills: Vec<String>,
    ) -> Result<models::AgentSkillSnapshot, ServiceError>;

    /// Remove a skill from the agent's desired runtime skill configuration.
    /// This intentionally does not delete the company-level skill itself.
    async fn remove_skill(&self, agent_id: Uuid, skill_id: &str) -> Result<(), ServiceError>;

    /// 重置Agent会话运行时状态
    async fn reset_session(&self, agent_id: Uuid) -> Result<(), ServiceError>;

    /// 设置 Agent 状态
    async fn set_status(&self, id: Uuid, status: AgentStatus) -> Result<Agent, ServiceError>;

    /// 更新 Agent 权限
    async fn update_permissions(
        &self,
        id: Uuid,
        permissions: models::AgentPermissions,
    ) -> Result<Agent, ServiceError>;

    /// 更新指令路径
    async fn update_instructions_path(
        &self,
        id: Uuid,
        path: Option<String>,
    ) -> Result<Agent, ServiceError>;

    /// 获取指令包
    async fn get_instructions_bundle(&self, id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// 更新指令包
    async fn update_instructions_bundle(
        &self,
        id: Uuid,
        bundle: serde_json::Value,
    ) -> Result<Agent, ServiceError>;

    /// 获取指令文件
    async fn get_bundle_file(&self, id: Uuid, file_path: &str) -> Result<String, ServiceError>;

    /// 保存指令文件
    async fn save_bundle_file(
        &self,
        id: Uuid,
        file_path: &str,
        content: String,
    ) -> Result<Agent, ServiceError>;

    /// 删除指令文件
    async fn delete_bundle_file(&self, id: Uuid, file_path: &str) -> Result<Agent, ServiceError>;

    /// 获取运行时状态
    async fn get_runtime_state(&self, id: Uuid) -> Result<AgentRuntimeState, ServiceError>;

    /// 获取任务会话
    async fn get_task_sessions(&self, id: Uuid) -> Result<Vec<AgentTaskSession>, ServiceError>;

    /// 列出 API Keys
    async fn list_keys(&self, id: Uuid) -> Result<Vec<AgentApiKey>, ServiceError>;

    /// 创建 API Key
    async fn create_key(
        &self,
        id: Uuid,
        name: String,
        scope: Option<serde_json::Value>,
    ) -> Result<AgentApiKey, ServiceError>;

    /// 吊销 API Key
    async fn revoke_key(&self, id: Uuid, key_id: Uuid) -> Result<(), ServiceError>;

    /// 更新预算
    async fn update_budget(
        &self,
        id: Uuid,
        budget_monthly_cents: i32,
    ) -> Result<Agent, ServiceError>;

    /// 轻量收件箱
    async fn inbox_lite(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// 当前 Agent 收件箱
    async fn inbox_mine(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// Claude 登录
    async fn claude_login(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// 获取公司级 Agent 配置列表
    async fn list_configurations(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, ServiceError>;
}

/// ServiceError - 服务层错误
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] repositories::RepositoryError),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Reporting cycle detected")]
    ReportingCycle,

    #[error("Agent in terminal state")]
    TerminalState,

    #[error("Configuration frozen (pending approval)")]
    ConfigurationFrozen,
}

fn normalize_bundle_path(path: &str) -> Result<String, ServiceError> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains("..") || normalized.split('/').any(|part| part.is_empty()) {
        return Err(ServiceError::InvalidInput("instruction path must be a relative file path".to_string()));
    }
    Ok(normalized)
}

fn validate_bundle(bundle: &serde_json::Value) -> Result<(), ServiceError> {
    let entry = bundle.get("entryFile").and_then(|v| v.as_str()).ok_or_else(|| ServiceError::InvalidInput("instructions bundle requires entryFile".to_string()))?;
    normalize_bundle_path(entry)?;
    let files = bundle.get("files").and_then(|v| v.as_object()).ok_or_else(|| ServiceError::InvalidInput("instructions bundle requires files object".to_string()))?;
    for (path, content) in files {
        normalize_bundle_path(path)?;
        if !content.is_string() { return Err(ServiceError::InvalidInput(format!("instruction file {path} must contain text"))); }
    }
    Ok(())
}

/// 校验 `reports_to` 分配是否合法（对齐 Paperclip `ensureManager` + 自引用检查）。
///
/// - 不能指向自身（自引用）。
/// - 上级 Agent 必须与本 Agent 同属一个 company，否则跨公司越权。
///
/// 返回 `Ok(())` 或 `ServiceError::InvalidInput`（映射为 422，与 Paperclip `unprocessable` 一致）。
fn validate_reports_to_assignment(
    agent_id: Uuid,
    agent_company_id: Uuid,
    reports_to: Uuid,
    manager_company_id: Uuid,
) -> Result<(), ServiceError> {
    if reports_to == agent_id {
        return Err(ServiceError::InvalidInput(
            "Agent cannot report to itself".to_string(),
        ));
    }
    if manager_company_id != agent_company_id {
        return Err(ServiceError::InvalidInput(
            "Manager must belong to same company".to_string(),
        ));
    }
    Ok(())
}

fn instruction_language(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
        "md" | "markdown" => "markdown",
        "mdx" => "mdx",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        _ => "text",
    }
}

fn public_instructions_bundle(
    agent: &Agent,
    bundle: &serde_json::Value,
) -> serde_json::Value {
    let entry_file = bundle
        .get("entryFile")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("AGENTS.md");
    let files = bundle
        .get("files")
        .and_then(serde_json::Value::as_object)
        .map(|files| {
            files
                .iter()
                .map(|(path, content)| {
                    let content_len = content.as_str().map_or(0, str::len);
                    serde_json::json!({
                        "path": path,
                        "size": content_len,
                        "language": instruction_language(path),
                        "markdown": path.ends_with(".md") || path.ends_with(".markdown"),
                        "isEntryFile": path == entry_file,
                        "editable": true,
                        "deprecated": false,
                        "virtual": false,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    serde_json::json!({
        "agentId": agent.id,
        "companyId": agent.company_id,
        "mode": "managed",
        "rootPath": null,
        "managedRootPath": "",
        "entryFile": entry_file,
        "resolvedEntryPath": null,
        "editable": true,
        "warnings": [],
        "legacyPromptTemplateActive": false,
        "legacyBootstrapPromptTemplateActive": false,
        "files": files,
    })
}

/// Compute org-chain health score for a single agent using a pre-loaded agent map.
///
/// Mirrors Paperclip's `getAgentWorkEligibility` but operates on in-memory data
/// to avoid N+1 database queries when computing scores for a list of agents.
fn compute_org_chain_health(
    agent: &Agent,
    agent_map: &std::collections::HashMap<Uuid, &Agent>,
) -> f32 {
    let mut score: f32 = 1.0;

    if let Some(ref reports_to_id) = agent.reports_to {
        match agent_map.get(reports_to_id) {
            Some(manager) => {
                if manager.status == AgentStatus::Terminated {
                    score -= 0.2; // missing_manager
                }
            }
            None => {
                score -= 0.2; // missing_manager (manager not found in company)
            }
        }
    }

    score.max(0.0)
}

/// DefaultAgentService - AgentService 的默认实现
pub struct DefaultAgentService<R, K, C, E, A>
where
    R: AgentRepository,
    K: AgentApiKeyRepository,
    C: ConfigRevisionRepository,
    E: CostEventRepository,
    A: ActivityLogRepository,
{
    repository: R,
    api_key_repo: Arc<K>,
    config_revision_repo: Option<Arc<C>>,
    cost_event_repo: Option<Arc<E>>,
    activity_log_repo: Option<Arc<A>>,
    pool: PgPool,
    heartbeat_pool: Option<PgPool>,
}

impl<R, K, C, E, A> DefaultAgentService<R, K, C, E, A>
where
    R: AgentRepository,
    K: AgentApiKeyRepository,
    C: ConfigRevisionRepository,
    E: CostEventRepository,
    A: ActivityLogRepository,
{
    pub fn new(repository: R, api_key_repo: Arc<K>, pool: PgPool) -> Self {
        Self {
            repository,
            api_key_repo,
            config_revision_repo: None,
            cost_event_repo: None,
            activity_log_repo: None,
            pool,
            heartbeat_pool: None,
        }
    }

    pub fn with_config_revision_repo(mut self, config_revision_repo: Arc<C>) -> Self {
        self.config_revision_repo = Some(config_revision_repo);
        self
    }

    pub fn with_cost_event_repo(mut self, cost_event_repo: Arc<E>) -> Self {
        self.cost_event_repo = Some(cost_event_repo);
        self
    }

    pub fn with_activity_log_repo(mut self, activity_log_repo: Arc<A>) -> Self {
        self.activity_log_repo = Some(activity_log_repo);
        self
    }

    pub fn with_heartbeat_pool(mut self, pool: PgPool) -> Self {
        self.heartbeat_pool = Some(pool);
        self
    }

    /// 记录活动日志（如果ActivityLogRepo已注入）
    async fn log_activity_if_enabled(&self, id: Uuid, company_id: Uuid, actor_id: Uuid) {
        if let Some(ref repo) = self.activity_log_repo {
            let repo_activity = repositories::activity_log_repository::Activity {
                id,
                company_id,
                actor_type: repositories::activity_log_repository::ActorType::Agent,
                actor_id,
                action: repositories::activity_log_repository::ActivityAction::Execute,
                resource_type: repositories::activity_log_repository::ResourceType::Agent,
                resource_id: actor_id,
                metadata: None,
                created_at: chrono::Utc::now(),
            };
            let _ = repo.log_activity(&repo_activity).await;
        }
    }

    /// 创建配置快照（如果ConfigRevisionRepo已注入）
    async fn capture_snapshot_if_enabled(&self, agent_id: Uuid) {
        if let Some(ref repo) = self.config_revision_repo {
            // 尝试创建快照，失败不阻塞主流程
            let snapshot_result = async {
                let agent = self.repository.get_by_id(agent_id).await.ok()?;
                let snapshot = crate::ConfigSnapshot::from_agent(&agent);
                let snapshot_json = serde_json::to_value(&snapshot).ok()?;

                let revision = models::AgentConfigRevision {
                    id: Uuid::new_v4(),
                    agent_id,
                    snapshot: sqlx::types::Json(snapshot_json),
                    created_at: Utc::now(),
                };

                repo.create(revision).await.ok()
            }
            .await;

            if snapshot_result.is_none() {
                tracing::warn!(
                    agent_id = %agent_id,
                    "Failed to capture agent config snapshot"
                );
            }
        }
    }

    /// Agent 进入 terminated 状态时的资源清理策略（对齐 Paperclip `remove` 的安全子集）。
    ///
    /// 终止是软删除，不会物理删除业务记录，但必须：
    /// 1. 重挂子节点：将 `reports_to` 指向本 Agent 的下级置空，避免已终止 Agent 继续担任 manager；
    /// 2. 撤销全部 API Key：已终止 Agent 无法再鉴权，从而无法触发 heartbeat / 被新任务分配（P0.4 item 3）；
    /// 3. 写入审计日志。
    ///
    /// instructions / skills / routines / workspace 等由各自服务的 terminated 状态守卫（P0.4 item 3）
    /// 负责隔离，不在终止时硬删除，避免误删公司级共享资源。
    async fn cleanup_terminated_resources(&self, id: Uuid, company_id: Uuid) {
        // 1. 重挂子节点（best-effort，失败仅记录）
        if let Err(e) = self.repository.clear_child_reports_to(id).await {
            tracing::warn!(agent_id = %id, error = %e, "Failed to re-parent child agents on termination");
        }

        // 2. 撤销全部 API Key（best-effort）
        if let Err(e) = self.api_key_repo.revoke_by_agent(id).await {
            tracing::warn!(agent_id = %id, error = %e, "Failed to revoke agent API keys on termination");
        }

        // 3. 审计日志（best-effort）
        self.log_activity_if_enabled(Uuid::new_v4(), company_id, id)
            .await;
    }
}

#[async_trait]
impl<R, K, C, E, A> AgentService for DefaultAgentService<R, K, C, E, A>
where
    R: AgentRepository,
    K: AgentApiKeyRepository,
    C: ConfigRevisionRepository,
    E: CostEventRepository,
    A: ActivityLogRepository,
{
    async fn create(&self, input: CreateAgentInput) -> Result<Agent, ServiceError> {
        let agent = Agent {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            name: input.name.clone(),
            role: input.role,
            status: input.status.unwrap_or(AgentStatus::Idle),
            adapter_type: input.adapter_type,
            adapter_config: sqlx::types::Json(input.adapter_config),
            runtime_config: sqlx::types::Json(
                input.runtime_config.unwrap_or(serde_json::json!({})),
            ),
            permissions: sqlx::types::Json(
                input.permissions.unwrap_or_else(|| AgentPermissions::for_role(input.role))
            ),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: None,
                built_in_key: None,
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: input.budget_monthly_cents.unwrap_or(0),
            reports_to: input.reports_to,
            pause_reason: None,
            paused_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // 检查循环引用与跨公司越权
        if let Some(reports_to) = input.reports_to {
            let manager = self.repository.get_by_id(reports_to).await?;
            validate_reports_to_assignment(agent.id, input.company_id, reports_to, manager.company_id)?;
            if self.detect_reporting_cycle(agent.id, reports_to).await? {
                return Err(ServiceError::ReportingCycle);
            }
        }

        let created_agent = self.repository.create(agent).await?;

        // 创建初始配置快照
        self.capture_snapshot_if_enabled(created_agent.id).await;

        // 记录活动日志: agent_hired
        self.log_activity_if_enabled(Uuid::new_v4(), created_agent.company_id, created_agent.id)
            .await;

        Ok(created_agent)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Agent, ServiceError> {
        Ok(self.repository.get_by_id(id).await?)
    }

    async fn get_chain_of_command(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<models::OrgNode>, ServiceError> {
        let start = self.repository.get_by_id(agent_id).await?;
        let mut chain = Vec::new();
        let mut current_id = start.reports_to;
        let mut visited = std::collections::HashSet::from([agent_id]);

        while let Some(manager_id) = current_id {
            if chain.len() >= 50 || !visited.insert(manager_id) {
                break;
            }
            let manager = match self.repository.get_by_id(manager_id).await {
                Ok(manager) => manager,
                // Keep stale hierarchy links from breaking agent detail responses,
                // matching Paperclip's best-effort chain traversal.
                Err(_) => break,
            };
            current_id = manager.reports_to;
            chain.push(models::OrgNode {
                id: manager.id.to_string(),
                name: manager.name,
                role: match manager.role {
                    models::AgentRole::Ceo => "CEO",
                    models::AgentRole::Vp => "VP",
                    models::AgentRole::Manager => "Manager",
                    models::AgentRole::Researcher => "Researcher",
                    models::AgentRole::General => "General Agent",
                }
                .to_string(),
                status: match manager.status {
                    models::AgentStatus::Idle => "idle",
                    models::AgentStatus::Running => "running",
                    models::AgentStatus::Paused => "paused",
                    models::AgentStatus::PendingApproval => "pending_approval",
                    models::AgentStatus::Terminated => "terminated",
                }
                .to_string(),
                reports: Vec::new(),
                collapsed_reports: None,
            });
        }
        Ok(chain)
    }

    async fn org_for_company(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<models::OrgNode>, ServiceError> {
        let agents = self
            .repository
            .list_by_company(company_id, ListAgentsOptions::default())
            .await?;
        let live_ids: std::collections::HashSet<Uuid> =
            agents.iter().map(|agent| agent.id).collect();
        let mut children: std::collections::HashMap<Option<Uuid>, Vec<models::OrgNode>> =
            std::collections::HashMap::new();

        for agent in agents {
            let parent = agent
                .reports_to
                .filter(|manager_id| live_ids.contains(manager_id));
            children.entry(parent).or_default().push(models::OrgNode {
                id: agent.id.to_string(),
                name: agent.name,
                role: match agent.role {
                    models::AgentRole::Ceo => "CEO",
                    models::AgentRole::Vp => "VP",
                    models::AgentRole::Manager => "Manager",
                    models::AgentRole::Researcher => "Researcher",
                    models::AgentRole::General => "General Agent",
                }
                .to_string(),
                status: match agent.status {
                    models::AgentStatus::Idle => "idle",
                    models::AgentStatus::Running => "running",
                    models::AgentStatus::Paused => "paused",
                    models::AgentStatus::PendingApproval => "pending_approval",
                    models::AgentStatus::Terminated => "terminated",
                }
                .to_string(),
                reports: Vec::new(),
                collapsed_reports: None,
            });
        }

        fn build(
            manager_id: Option<Uuid>,
            children: &mut std::collections::HashMap<Option<Uuid>, Vec<models::OrgNode>>,
        ) -> Vec<models::OrgNode> {
            let mut nodes = children.remove(&manager_id).unwrap_or_default();
            for node in &mut nodes {
                if let Ok(id) = node.id.parse::<Uuid>() {
                    node.reports = build(Some(id), children);
                }
            }
            nodes
        }

        Ok(build(None, &mut children))
    }

    async fn get_me(&self, agent_key: &str) -> Result<Agent, ServiceError> {
        // Hash the provided key with SHA256 (matching Paperclip's implementation)
        let mut hasher = Sha256::new();
        hasher.update(agent_key.as_bytes());
        let key_hash = hex::encode(hasher.finalize());

        // Find API key by hash
        let api_key = self
            .api_key_repo
            .find_by_key_hash(&key_hash)
            .await?
            .ok_or_else(|| ServiceError::Unauthorized("Invalid agent key".to_string()))?;

        // Verify key is active
        if !api_key.is_active() {
            return Err(ServiceError::Unauthorized(
                "Agent key is revoked".to_string(),
            ));
        }

        // Update last_used_at timestamp (fire-and-forget)
        let _ = self.api_key_repo.update_last_used(api_key.id).await;

        // Return the associated agent
        self.repository
            .get_by_id(api_key.agent_id)
            .await
            .map_err(Into::into)
    }

    async fn list(&self, company_id: Uuid) -> Result<Vec<NormalizedAgentRow>, ServiceError> {
        // Load the filtered set (excludes terminated by default) for the response.
        let agents = self
            .repository
            .list_by_company(company_id, repositories::ListAgentsOptions::default())
            .await?;

        // Load ALL company agents (including terminated) once for org-chain health
        // computation — mirrors Paperclip's listCompanyAgentRows pattern.
        let all_company_agents = self
            .repository
            .list_by_company(
                company_id,
                repositories::ListAgentsOptions {
                    include_terminated: true,
                    limit: None,
                    offset: None,
                },
            )
            .await?;

        // Build lookup maps for O(1) org-chain traversal.
        let agent_map: std::collections::HashMap<Uuid, &Agent> =
            all_company_agents.iter().map(|a| (a.id, a)).collect();

        // 获取当前年月用于花费聚合
        let now = Utc::now();
        let year = now.year();
        let month = now.month();

        // 批量聚合花费（如果CostEventRepository已注入）
        let agent_ids: Vec<Uuid> = agents.iter().map(|a| a.id).collect();
        let spend_map = if let Some(ref repo) = self.cost_event_repo {
            let summaries = repo
                .aggregate_monthly_spend_batch(agent_ids, year, month)
                .await
                .unwrap_or_default();
            summaries
                .into_iter()
                .map(|s| (s.agent_id, s.total_cost_cents))
                .collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };

        let mut normalized = Vec::new();
        for agent in agents {
            // 计算健康度评分 — uses the pre-loaded map instead of N+1 DB queries
            let org_chain_health = compute_org_chain_health(&agent, &agent_map);

            // 获取月度花费
            let spent_monthly_cents = spend_map.get(&agent.id).copied().unwrap_or(0);

            normalized.push(NormalizedAgentRow {
                agent,
                spent_monthly_cents,
                org_chain_health,
            });
        }

        Ok(normalized)
    }

    async fn update(&self, id: Uuid, input: UpdateAgentInput) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;

        // 检查终止状态
        if agent.status == AgentStatus::Terminated {
            return Err(ServiceError::TerminalState);
        }

        // 检查配置冻结
        if agent.status == AgentStatus::PendingApproval {
            return Err(ServiceError::ConfigurationFrozen);
        }

        // 验证状态转换
        if let Some(new_status) = input.status {
            let state_machine = models::AgentStateMachine::new(agent.status);
            if !state_machine.can_transition_to(new_status) {
                return Err(ServiceError::InvalidInput(format!(
                    "Invalid state transition from {:?} to {:?}",
                    agent.status, new_status
                )));
            }
        }

        // 检测是否有配置变更（在应用更新之前）
        let has_config_change = input.adapter_config.is_some()
            || input.adapter_type.is_some()
            || input.runtime_config.is_some()
            || input.budget_monthly_cents.is_some();

        // 应用更新
        if let Some(name) = input.name {
            agent.name = name;
        }
        if let Some(role) = input.role {
            agent.role = role;
        }
        if let Some(status) = input.status {
            agent.status = status;
        }
        if let Some(adapter_type) = input.adapter_type {
            agent.adapter_type = adapter_type;
        }
        if let Some(config) = input.adapter_config {
            agent.adapter_config = sqlx::types::Json(config);
        }
        if let Some(config) = input.runtime_config {
            agent.runtime_config = sqlx::types::Json(config);
        }
        if let Some(budget) = input.budget_monthly_cents {
            agent.budget_monthly_cents = budget;
        }
        if let Some(reports_to) = input.reports_to {
            // 检查跨公司越权与循环引用
            let manager = self.repository.get_by_id(reports_to).await?;
            validate_reports_to_assignment(id, agent.company_id, reports_to, manager.company_id)?;
            if self.detect_reporting_cycle(id, reports_to).await? {
                return Err(ServiceError::ReportingCycle);
            }
            agent.reports_to = Some(reports_to);
        }

        let updated_agent = self.repository.update(agent).await?;

        // 配置变更时自动创建快照
        if has_config_change {
            self.capture_snapshot_if_enabled(updated_agent.id).await;
        }

        Ok(updated_agent)
    }

    async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        // 软删除：更新状态为terminated而非物理删除
        let mut agent = self.repository.get_by_id(id).await?;

        if agent.status == AgentStatus::Terminated {
            return Ok(()); // 已经终止，幂等操作
        }

        agent.status = AgentStatus::Terminated;
        agent.updated_at = Utc::now();

        self.repository.update(agent.clone()).await?;

        self.cleanup_terminated_resources(agent.id, agent.company_id)
            .await;

        Ok(())
    }

    async fn terminate(&self, id: Uuid) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;

        if agent.status == AgentStatus::Terminated {
            return Ok(agent); // 已经终止，幂等操作
        }

        agent.status = AgentStatus::Terminated;
        agent.updated_at = Utc::now();

        let updated = self.repository.update(agent.clone()).await?;

        self.cleanup_terminated_resources(updated.id, updated.company_id)
            .await;

        Ok(updated)
    }

    async fn detect_reporting_cycle(
        &self,
        agent_id: Uuid,
        reports_to: Uuid,
    ) -> Result<bool, ServiceError> {
        let mut current = reports_to;
        let mut visited = std::collections::HashSet::new();
        visited.insert(agent_id);

        // 最多遍历 100 层
        for _ in 0..100 {
            if current == agent_id {
                return Ok(true); // 检测到循环
            }

            if visited.contains(&current) {
                return Ok(true); // 检测到循环
            }

            visited.insert(current);

            match self.repository.get_by_id(current).await {
                Ok(agent) => {
                    if let Some(next_reports_to) = agent.reports_to {
                        current = next_reports_to;
                    } else {
                        break; // 到达根节点
                    }
                }
                Err(_) => break,
            }
        }

        Ok(false)
    }

    async fn get_agent_work_eligibility(&self, agent_id: Uuid) -> Result<f32, ServiceError> {
        let agent = self.repository.get_by_id(agent_id).await?;

        let mut score: f32 = 1.0;

        // 检查是否有上级管理者
        if let Some(reports_to_id) = agent.reports_to {
            match self.repository.get_by_id(reports_to_id).await {
                Ok(manager) => {
                    // 上级存在但状态异常时扣分
                    if manager.status == AgentStatus::Terminated {
                        score -= 0.2; // missing_manager
                    }
                }
                Err(_) => {
                    score -= 0.2; // missing_manager
                }
            }
        }

        if agent.budget_monthly_cents > 0 {
            if let Some(cost_repo) = &self.cost_event_repo {
                let spent = cost_repo
                    .current_month_spend_by_agent(agent_id)
                    .await
                    .map_err(ServiceError::Repository)?;
                if spent > i64::from(agent.budget_monthly_cents) {
                    score -= 0.5;
                }
            }
        }

        Ok(score.max(0.0))
    }

    async fn rollback_config_revision(
        &self,
        agent_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Agent, ServiceError> {
        // 获取要回滚的Agent
        let mut agent = self.repository.get_by_id(agent_id).await?;

        // 检查终止状态
        if agent.status == AgentStatus::Terminated {
            return Err(ServiceError::TerminalState);
        }

        // 检查配置冻结
        if agent.status == AgentStatus::PendingApproval {
            return Err(ServiceError::ConfigurationFrozen);
        }

        // 获取配置版本快照
        let config_revision_repo = self.config_revision_repo.as_ref().ok_or_else(|| {
            ServiceError::NotFound("ConfigRevision repository not available".to_string())
        })?;

        let revision = config_revision_repo
            .get_by_id(revision_id)
            .await
            .map_err(|e| ServiceError::NotFound(format!("Config revision not found: {}", e)))?;

        // 验证revision属于该agent
        if revision.agent_id != agent_id {
            return Err(ServiceError::InvalidInput(
                "Revision does not belong to this agent".to_string(),
            ));
        }

        // 解析快照JSON
        let snapshot: crate::ConfigSnapshot = serde_json::from_value(revision.snapshot.0.clone())
            .map_err(|e| {
            ServiceError::InvalidInput(format!("Invalid snapshot format: {}", e))
        })?;

        // 应用回滚
        agent.adapter_type = snapshot.adapter_type;
        agent.adapter_config = sqlx::types::Json(snapshot.adapter_config);
        agent.runtime_config = sqlx::types::Json(snapshot.runtime_config);
        agent.permissions =
            sqlx::types::Json(serde_json::from_value(snapshot.permissions).map_err(|e| {
                ServiceError::InvalidInput(format!("Invalid permissions format: {}", e))
            })?);
        agent.budget_monthly_cents = snapshot.budget_monthly_cents;

        // 更新数据库
        let updated_agent = self.repository.update(agent).await?;

        // 创建新的配置快照记录回滚操作
        self.capture_snapshot_if_enabled(updated_agent.id).await;

        Ok(updated_agent)
    }

    async fn get_skills(&self, agent_id: Uuid) -> Result<models::AgentSkillSnapshot, ServiceError> {
        // 获取Agent信息
        let agent = self.repository.get_by_id(agent_id).await?;

        // 解析adapter_config中的desired_skills（如果存在）
        let desired_skills = agent
            .adapter_config
            .0
            .get("desired_skills")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        // 构建技能条目（简化实现，实际应查询skill表）
        let entries = desired_skills
            .iter()
            .map(|name| models::AgentSkillEntry {
                key: name.clone(),
                runtime_name: Some(name.clone()),
                version_id: None,
                current_version_id: None,
                desired: true,
                managed: true,
                state: models::AgentSkillState::Configured,
                origin: Some(models::AgentSkillOrigin::CompanyManaged),
                origin_label: Some("Company managed".to_string()),
                location_label: None,
                read_only: false,
                source_path: None,
                target_path: None,
                detail: None,
            })
            .collect();

        // 返回技能快照
        Ok(models::AgentSkillSnapshot {
            adapter_type: agent.adapter_type,
            supported: true,
            mode: models::AgentSkillSyncMode::Persistent,
            desired_skills,
            desired_skill_entries: None,
            entries,
            warnings: vec![],
        })
    }

    async fn sync_skills(
        &self,
        agent_id: Uuid,
        desired_skills: Vec<String>,
    ) -> Result<models::AgentSkillSnapshot, ServiceError> {
        let mut agent = self.repository.get_by_id(agent_id).await?;
        let mut adapter_config = agent.adapter_config.0;
        adapter_config["desired_skills"] = serde_json::Value::Array(
            desired_skills
                .iter()
                .map(|key| serde_json::Value::String(key.clone()))
                .collect(),
        );
        agent.adapter_config = sqlx::types::Json(adapter_config);
        agent.updated_at = Utc::now();
        self.repository.update(agent).await?;
        // 捕获配置快照，使配置回滚（config-revisions rollback）能恢复技能集。
        self.capture_snapshot_if_enabled(agent_id).await;
        self.get_skills(agent_id).await
    }

    async fn remove_skill(&self, agent_id: Uuid, skill_id: &str) -> Result<(), ServiceError> {
        let mut agent = self.repository.get_by_id(agent_id).await?;
        let desired = agent
            .adapter_config
            .get("desired_skills")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        let next = desired
            .iter()
            .filter(|value| value.as_str() != Some(skill_id))
            .cloned()
            .collect::<Vec<_>>();
        if next.len() == desired.len() {
            return Err(ServiceError::NotFound(format!(
                "Agent skill '{}' is not configured",
                skill_id
            )));
        }
        let mut adapter_config = agent.adapter_config.0;
        adapter_config["desired_skills"] = serde_json::Value::Array(next);
        agent.adapter_config = sqlx::types::Json(adapter_config);
        agent.updated_at = Utc::now();
        self.repository.update(agent).await?;
        Ok(())
    }

    async fn reset_session(&self, agent_id: Uuid) -> Result<(), ServiceError> {
        sqlx::query(
            "UPDATE agent_runtime_states
             SET session_id = NULL,
                 session_display_id = NULL,
                 session_params_json = NULL,
                 updated_at = NOW()
             WHERE agent_id = $1",
        )
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(|error| ServiceError::Internal(format!("Failed to reset agent session: {error}")))?;
        Ok(())
    }

    async fn set_status(&self, id: Uuid, status: AgentStatus) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;
        if agent.status == AgentStatus::Terminated && status != AgentStatus::Terminated {
            return Err(ServiceError::TerminalState);
        }
        agent.status = status;
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn update_permissions(
        &self,
        id: Uuid,
        permissions: models::AgentPermissions,
    ) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;
        agent.permissions = sqlx::types::Json(permissions);
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn update_instructions_path(
        &self,
        id: Uuid,
        path: Option<String>,
    ) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;
        agent.metadata = sqlx::types::Json(models::AgentMetadata {
            is_built_in: agent.metadata.is_built_in,
            built_in_key: agent.metadata.built_in_key.clone(),
            instructions_path: path,
            instructions_bundle: agent.metadata.instructions_bundle.clone(),
        });
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn get_instructions_bundle(&self, id: Uuid) -> Result<serde_json::Value, ServiceError> {
        let agent = self.repository.get_by_id(id).await?;
        let bundle = agent.metadata.instructions_bundle.clone().unwrap_or_else(|| serde_json::json!({
            "entryFile": "AGENTS.md",
            "files": {}
        }));
        validate_bundle(&bundle)?;
        Ok(public_instructions_bundle(&agent, &bundle))
    }

    async fn update_instructions_bundle(
        &self,
        id: Uuid,
        bundle: serde_json::Value,
    ) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;
        let mut next_bundle = agent
            .metadata
            .instructions_bundle
            .take()
            .unwrap_or_else(|| serde_json::json!({"entryFile":"AGENTS.md","files":{}}));

        if bundle.get("files").is_some() {
            validate_bundle(&bundle)?;
            next_bundle = bundle;
        } else {
            if let Some(entry_file) = bundle.get("entryFile") {
                next_bundle["entryFile"] = entry_file.clone();
            }
            validate_bundle(&next_bundle)?;
        }

        agent.metadata.0.instructions_bundle = Some(next_bundle);
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn get_bundle_file(&self, id: Uuid, file_path: &str) -> Result<String, ServiceError> {
        let path = normalize_bundle_path(file_path)?;
        let agent = self.repository.get_by_id(id).await?;
        let bundle = agent
            .metadata
            .instructions_bundle
            .clone()
            .unwrap_or_else(|| serde_json::json!({
                "entryFile": "AGENTS.md",
                "files": {}
            }));
        validate_bundle(&bundle)?;
        bundle.get("files").and_then(|files| files.get(&path)).and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| ServiceError::NotFound(format!("Instruction file not found: {path}")))
    }

    async fn save_bundle_file(
        &self,
        id: Uuid,
        file_path: &str,
        content: String,
    ) -> Result<Agent, ServiceError> {
        let path = normalize_bundle_path(file_path)?;
        let mut agent = self.repository.get_by_id(id).await?;
        let mut bundle = agent.metadata.instructions_bundle.take().unwrap_or_else(|| serde_json::json!({"entryFile":"AGENTS.md","files":{}}));
        validate_bundle(&bundle)?;
        bundle["files"].as_object_mut().expect("validated bundle files object").insert(path.clone(), serde_json::Value::String(content));
        if bundle.get("entryFile").and_then(|v| v.as_str()).is_none() { bundle["entryFile"] = serde_json::Value::String(path); }
        agent.metadata.0.instructions_bundle = Some(bundle);
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn delete_bundle_file(&self, id: Uuid, file_path: &str) -> Result<Agent, ServiceError> {
        let path = normalize_bundle_path(file_path)?;
        let mut agent = self.repository.get_by_id(id).await?;
        let mut bundle = agent.metadata.instructions_bundle.take().unwrap_or_else(|| serde_json::json!({"entryFile":"AGENTS.md","files":{}}));
        validate_bundle(&bundle)?;
        bundle["files"].as_object_mut().expect("validated bundle files object").remove(&path);
        agent.metadata.0.instructions_bundle = Some(bundle);
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn get_runtime_state(&self, id: Uuid) -> Result<AgentRuntimeState, ServiceError> {
        let agent = self.repository.get_by_id(id).await?;
        
        // 从 agent_runtime_states 表读取完整的 runtime state
        let Some(pool) = &self.heartbeat_pool else {
            return Err(ServiceError::Internal("heartbeat pool not configured".to_string()));
        };
        
        // 如果不存在，先初始化
        let state: Option<AgentRuntimeState> = sqlx::query_as(
            "SELECT * FROM agent_runtime_states WHERE agent_id = $1"
        )
        .bind(agent.id)
        .fetch_optional(pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("failed to load runtime state: {e}")))?;
        
        if let Some(state) = state {
            return Ok(state);
        }
        
        // 初始化 runtime state
        let new_state: AgentRuntimeState = sqlx::query_as(
            r#"
            INSERT INTO agent_runtime_states (
                agent_id, company_id, adapter_type,
                state_json, total_input_tokens, total_output_tokens,
                total_cached_input_tokens, total_cost_cents
            )
            VALUES ($1, $2, $3, '{}', 0, 0, 0, 0)
            RETURNING *
            "#
        )
        .bind(agent.id)
        .bind(agent.company_id)
        .bind(&agent.adapter_type)
        .fetch_one(pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("failed to initialize runtime state: {e}")))?;
        
        Ok(new_state)
    }

    async fn get_task_sessions(&self, id: Uuid) -> Result<Vec<AgentTaskSession>, ServiceError> {
        let agent = self.repository.get_by_id(id).await?;
        // Paperclip stores one row per adapter/task key in agent_task_sessions.
        // Parrot currently has no separate session table, so expose the durable
        // heartbeat executions as the equivalent session history instead of
        // returning a misleading empty list.
        let Some(pool) = &self.heartbeat_pool else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT id, agent_id, status::text AS status,
                    COALESCE(started_at, created_at) AS started_at,
                    finished_at, context_snapshot
             FROM heartbeat_runs
             WHERE company_id = $1 AND agent_id = $2
             ORDER BY COALESCE(started_at, created_at) DESC, created_at DESC",
        )
        .bind(agent.company_id)
        .bind(agent.id)
        .fetch_all(pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("failed to load task sessions: {e}")))?;

        rows.into_iter()
            .map(|row| {
                Ok(AgentTaskSession {
                    id: row.try_get("id").map_err(|e| ServiceError::Internal(e.to_string()))?,
                    agent_id: row.try_get("agent_id").map_err(|e| ServiceError::Internal(e.to_string()))?,
                    status: row.try_get("status").map_err(|e| ServiceError::Internal(e.to_string()))?,
                    started_at: row.try_get("started_at").map_err(|e| ServiceError::Internal(e.to_string()))?,
                    ended_at: row.try_get("finished_at").map_err(|e| ServiceError::Internal(e.to_string()))?,
                    metadata: row.try_get("context_snapshot").map_err(|e| ServiceError::Internal(e.to_string()))?,
                })
            })
            .collect()
    }

    async fn list_keys(&self, id: Uuid) -> Result<Vec<AgentApiKey>, ServiceError> {
        let _agent = self.repository.get_by_id(id).await?;
        let keys = self.api_key_repo.list_by_agent(id).await?;
        Ok(keys)
    }

    async fn create_key(
        &self,
        id: Uuid,
        name: String,
        scope: Option<serde_json::Value>,
    ) -> Result<AgentApiKey, ServiceError> {
        let agent = self.repository.get_by_id(id).await?;
        let raw_key = format!("aak_{}", Uuid::new_v4().simple());
        let mut digest = Sha256::new();
        digest.update(raw_key.as_bytes());
        let scope = scope.unwrap_or_else(|| serde_json::json!({"scope_type":"standard","agent_id":id,"company_id":agent.company_id}));
        let key = AgentApiKey {
            id: Uuid::new_v4(),
            agent_id: id,
            company_id: agent.company_id,
            name,
            scope,
            key_hash: digest
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            last_used_at: None,
            revoked_at: None,
            created_at: Utc::now(),
        };
        self.api_key_repo.create(key.clone()).await?;
        Ok(key)
    }

    async fn revoke_key(&self, id: Uuid, key_id: Uuid) -> Result<(), ServiceError> {
        let _agent = self.repository.get_by_id(id).await?;
        self.api_key_repo.revoke(key_id).await?;
        Ok(())
    }

    async fn update_budget(
        &self,
        id: Uuid,
        budget_monthly_cents: i32,
    ) -> Result<Agent, ServiceError> {
        let mut agent = self.repository.get_by_id(id).await?;
        agent.budget_monthly_cents = budget_monthly_cents;
        agent.updated_at = Utc::now();
        self.repository.update(agent.clone()).await?;
        Ok(agent)
    }

    async fn inbox_lite(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError> {
        let agent = self.repository.get_by_id(agent_id).await?;
        
        // Query issues assigned to this agent with status todo/in_progress/blocked
        let issues = sqlx::query(
            r#"
            SELECT 
                i.id,
                i.identifier,
                i.title,
                i.status,
                i.priority,
                i.project_id,
                i.goal_id,
                i.parent_id,
                i.updated_at
            FROM issues i
            WHERE i.company_id = $1 
              AND i.assignee_agent_id = $2
              AND i.status IN ('todo', 'in_progress', 'blocked')
            ORDER BY i.updated_at DESC
            LIMIT 100
            "#
        )
        .bind(agent.company_id)
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServiceError::Internal(format!("Failed to query issues: {e}")))?;

        let issue_ids: Vec<Uuid> = issues.iter().filter_map(|row| row.try_get("id").ok()).collect();

        // Calculate dependency readiness for each issue
        let mut dependency_readiness = std::collections::HashMap::new();
        if !issue_ids.is_empty() {
            // Query blocker relationships
            let blockers = sqlx::query(
                r#"
                SELECT 
                    ir.related_issue_id as issue_id,
                    ir.issue_id as blocker_issue_id,
                    bi.status as blocker_status
                FROM issue_relations ir
                INNER JOIN issues bi ON bi.id = ir.issue_id
                WHERE ir.company_id = $1
                  AND ir.type = 'blocks'
                  AND ir.related_issue_id = ANY($2)
                "#
            )
            .bind(agent.company_id)
            .bind(&issue_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to query blockers: {e}")))?;

            for issue_id in &issue_ids {
                let issue_blockers: Vec<_> = blockers
                    .iter()
                    .filter(|b| {
                        let bid: Uuid = b.try_get("issue_id").unwrap_or_default();
                        bid == *issue_id
                    })
                    .collect();

                let unresolved_blockers: Vec<Uuid> = issue_blockers
                    .iter()
                    .filter_map(|b| {
                        let blocker_id: Uuid = b.try_get("blocker_issue_id").ok()?;
                        let status: String = b.try_get("blocker_status").ok()?;
                        if status != "done" {
                            Some(blocker_id)
                        } else {
                            None
                        }
                    })
                    .collect();

                let dependency_ready = unresolved_blockers.is_empty();
                
                dependency_readiness.insert(*issue_id, (
                    dependency_ready,
                    unresolved_blockers.len(),
                    unresolved_blockers,
                ));
            }
        }


        let items: Vec<serde_json::Value> = issues
            .iter()
            .filter_map(|issue| {
                let id: Uuid = issue.try_get("id").ok()?;
                let (dep_ready, unresolved_count, unresolved_ids) = 
                    dependency_readiness.get(&id)
                        .cloned()
                        .unwrap_or((true, 0, vec![]));

                Some(serde_json::json!({
                    "id": id,
                    "identifier": issue.try_get::<String, _>("identifier").ok()?,
                    "title": issue.try_get::<String, _>("title").ok()?,
                    "status": issue.try_get::<String, _>("status").ok()?,
                    "priority": issue.try_get::<String, _>("priority").ok(),
                    "projectId": issue.try_get::<Uuid, _>("project_id").ok(),
                    "goalId": issue.try_get::<Uuid, _>("goal_id").ok(),
                    "parentId": issue.try_get::<Uuid, _>("parent_id").ok(),
                    "updatedAt": issue.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok(),
                    "dependencyReady": dep_ready,
                    "unresolvedBlockerCount": unresolved_count,
                    "unresolvedBlockerIssueIds": unresolved_ids,
                }))
            })
            .collect();

        Ok(serde_json::json!({
            "agentId": agent_id,
            "total": items.len(),
            "items": items,
        }))
    }

    async fn inbox_mine(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError> {
        let _agent = self.repository.get_by_id(agent_id).await?;
        Ok(serde_json::json!({
            "agentId": agent_id,
            "items": [],
        }))
    }

    async fn claude_login(&self, agent_id: Uuid) -> Result<serde_json::Value, ServiceError> {
        let agent = self.repository.get_by_id(agent_id).await?;
        Ok(serde_json::json!({
            "agentId": agent.id,
            "loginUrl": format!("/api/claude-login?agentId={}", agent_id),
            "expiresIn": 3600,
        }))
    }

    async fn list_configurations(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, ServiceError> {
        let agents = self
            .repository
            .list_by_company(company_id, repositories::ListAgentsOptions::default())
            .await?;
        let configs: Vec<serde_json::Value> = agents
            .into_iter()
            .map(|agent| {
                serde_json::json!({
                    "id": agent.id,
                    "name": agent.name,
                    "role": agent.role,
                    "status": agent.status,
                    "adapterType": agent.adapter_type,
                    "budgetMonthlyCents": agent.budget_monthly_cents,
                    "createdAt": agent.created_at,
                })
            })
            .collect();
        Ok(configs)
    }
}

#[cfg(test)]
mod tests {
    use super::validate_reports_to_assignment;
    use uuid::Uuid;

    #[test]
    fn reports_to_same_company_ok() {
        let agent = Uuid::new_v4();
        let manager = Uuid::new_v4();
        let company = Uuid::new_v4();
        assert!(validate_reports_to_assignment(agent, company, manager, company).is_ok());
    }

    #[test]
    fn reports_to_cross_company_rejected() {
        let agent = Uuid::new_v4();
        let manager = Uuid::new_v4();
        let agent_company = Uuid::new_v4();
        let manager_company = Uuid::new_v4();
        let err = validate_reports_to_assignment(agent, agent_company, manager, manager_company)
            .unwrap_err();
        assert!(format!("{err}").contains("same company"));
    }

    #[test]
    fn reports_to_self_rejected() {
        let agent = Uuid::new_v4();
        let company = Uuid::new_v4();
        let err = validate_reports_to_assignment(agent, company, agent, company).unwrap_err();
        assert!(format!("{err}").contains("cannot report to itself"));
    }
}
