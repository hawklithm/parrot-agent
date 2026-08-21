use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::built_in_agent_service::{
    BuiltInAgentBundleDefinition, BuiltInAgentDefinition, BuiltInAgentKey,
    BuiltInAgentMetadataRegistry, BuiltInAgentStatus, RoutineCatchUpPolicy,
    RoutineConcurrencyPolicy, RoutinePriority, RoutineStatus,
};
use repositories;
use repositories::BuiltInManagedResourceRepository;

#[derive(Debug, Error)]
pub enum BuiltInAgentError {
    #[error("Built-in agent not found: {0}")]
    NotFound(BuiltInAgentKey),

    #[error("Agent repository error: {0}")]
    RepositoryError(String),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Provision failed: {0}")]
    ProvisionFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type BuiltInAgentResult<T> = Result<T, BuiltInAgentError>;

/// 内置Agent服务接口
#[async_trait]
pub trait BuiltInAgentService: Send + Sync {
    /// 初始化（Provision）内置Agent
    ///
    /// 查找定义 -> 创建/获取Agent -> 绑定资源
    ///
    /// `input` 允许调用方覆盖内置 Agent 的默认配置（适配器类型、配置、预算）。
    /// 如果 Agent 已存在，传入 `input` 会更新其配置。
    async fn provision(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent>;

    /// 获取内置Agent的当前状态
    async fn get_status(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<BuiltInAgentStatus>;

    /// 重置内置Agent
    ///
    /// 清除资源 + 恢复初始状态
    async fn reset(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<()>;

    /// 协调（Reconcile）内置Agent资源
    ///
    /// 检测并修复资源漂移
    async fn reconcile(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<ReconcileResult>;

    /// 列举所有可用的内置Agent定义
    fn list_definitions(&self) -> Vec<&BuiltInAgentDefinition>;

    /// 获取特定内置Agent的定义
    fn get_definition(&self, key: BuiltInAgentKey) -> Option<&BuiltInAgentDefinition>;
}

/// 资源协调结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReconcileResult {
    pub agent_updated: bool,
    pub instructions_materialized: bool,
    pub skills_synced: bool,
    pub routines_synced: bool,
    pub changes: Vec<String>,
}

impl Default for ReconcileResult {
    fn default() -> Self {
        Self {
            agent_updated: false,
            instructions_materialized: false,
            skills_synced: false,
            routines_synced: false,
            changes: Vec::new(),
        }
    }
}

/// Provision 输入参数，允许用户覆盖内置 Agent 的默认配置
#[derive(Debug, Clone)]
pub struct ProvisionInput {
    /// 自定义适配器类型（覆盖定义中的默认值）
    pub adapter_type: Option<String>,
    /// 自定义适配器配置（覆盖定义中的默认值）
    pub adapter_config: Option<serde_json::Value>,
    /// 自定义月度预算（覆盖定义中的默认值）
    pub budget_monthly_cents: Option<i32>,
}

/// 默认的内置Agent服务实现
pub struct DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    registry: BuiltInAgentMetadataRegistry,
    agent_repo: std::sync::Arc<A>,
    managed_repo: std::sync::Arc<dyn BuiltInManagedResourceRepository>,
    resource_pool: Option<PgPool>,
}

impl<A> DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    pub fn new(
        agent_repo: std::sync::Arc<A>,
        managed_repo: std::sync::Arc<dyn BuiltInManagedResourceRepository>,
    ) -> Self {
        Self {
            registry: BuiltInAgentMetadataRegistry::new(),
            agent_repo,
            managed_repo,
            resource_pool: None,
        }
    }

    pub fn with_resource_pool(mut self, resource_pool: PgPool) -> Self {
        self.resource_pool = Some(resource_pool);
        self
    }

    /// 将内置 Agent 的 Skill/Routine 受管资源同步到数据库：
    /// - 先在 `resource_pool` 的**同一事务**中幂等物化 `company_skills`/`skill_files` 与
    ///   `routines`/`routine_triggers`，保证技能与例程数据行（以及触发器）要么一起成功、
    ///   要么一起回滚（跨资源原子回滚）。
    /// - 再按 (company, built_in_key, resource_type, canonical_key) 幂等 upsert 到
    ///   `builtin_managed_resources` ledger；若存量行的 `stock_version` 落后，则修复漂移。
    ///
    /// 返回 (是否有变更, 变更说明)。
    async fn sync_managed_resources(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        definition: &BuiltInAgentDefinition,
        agent_id: Uuid,
    ) -> BuiltInAgentResult<(bool, Vec<String>)> {
        let mut changed = false;
        let mut changes = Vec::new();
        let bundle = match &definition.bundle {
            Some(b) => b,
            None => return Ok((changed, changes)),
        };

        // 在 resource_pool 的同一事务内原子物化 Skill + Routine 数据行。
        let (skill_id, routine_id) = if let Some(pool) = &self.resource_pool {
            let mut tx = pool
                .begin()
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            let skill_id = self
                .materialize_skill_in_tx(&mut tx, company_id, definition)
                .await?;
            let existing_routine_id = self
                .managed_repo
                .get(company_id, key.as_str(), "routine", &bundle.routine.routine_key)
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?
                .and_then(|row| row.target_resource_id);
            let routine_id = self
                .materialize_routine_in_tx(&mut tx, company_id, bundle, agent_id, existing_routine_id)
                .await?;
            tx.commit()
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            (skill_id, routine_id)
        } else {
            (None, None)
        };

        // Ledger 绑定（自身幂等 upsert）。
        let (skill_changed, skill_msg) = self
            .sync_one_resource(
                company_id,
                key,
                "skill",
                &bundle.skill.canonical_key,
                skill_id,
                &bundle.stock_version,
            )
            .await?;
        if skill_changed {
            changed = true;
            changes.push(skill_msg);
        }

        let (routine_changed, routine_msg) = self
            .sync_one_resource(
                company_id,
                key,
                "routine",
                &bundle.routine.routine_key,
                routine_id,
                &bundle.stock_version,
            )
            .await?;
        if routine_changed {
            changed = true;
            changes.push(routine_msg);
        }

        Ok((changed, changes))
    }

    async fn sync_one_resource(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        resource_type: &str,
        canonical_key: &str,
        target_resource_id: Option<Uuid>,
        stock_version: &str,
    ) -> BuiltInAgentResult<(bool, String)> {
        let existing = self
            .managed_repo
            .get(company_id, key.as_str(), resource_type, canonical_key)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
        match existing {
            None => {
                self.managed_repo
                    .upsert(
                        company_id,
                        key.as_str(),
                        resource_type,
                        canonical_key,
                        target_resource_id,
                        stock_version,
                        stock_version,
                    )
                    .await
                    .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                Ok((
                    true,
                    format!(
                        "Bound managed {} '{}' at {}",
                        resource_type, canonical_key, stock_version
                    ),
                ))
            }
            Some(row)
                if row.stock_version != stock_version
                    || row.current_version != stock_version
                    || row.drift_detected
                    || row.status != "active"
                    || (target_resource_id.is_some()
                        && row.target_resource_id != target_resource_id) =>
            {
                self.managed_repo
                    .upsert(
                        company_id,
                        key.as_str(),
                        resource_type,
                        canonical_key,
                        target_resource_id,
                        stock_version,
                        stock_version,
                    )
                    .await
                    .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                Ok((
                    true,
                    format!(
                        "Repaired {} '{}' stock drift to {}",
                        resource_type, canonical_key, stock_version
                    ),
                ))
            }
            Some(_) => Ok((false, String::new())),
        }
    }

    async fn materialize_skill_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        company_id: Uuid,
        definition: &BuiltInAgentDefinition,
    ) -> BuiltInAgentResult<Option<Uuid>> {
        let bundle = match &definition.bundle {
            Some(b) => b,
            None => return Ok(None),
        };

        let config = serde_json::json!({
            "managedBy": "built_in_agent",
            "builtInKey": definition.key.as_str(),
            "canonicalKey": bundle.skill.canonical_key,
            "stockVersion": bundle.stock_version,
        });
        let skill_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO company_skills
               (company_id, name, slug, description, version, config, is_paperclip_managed, status)
               VALUES ($1, $2, $3, $4, $5, $6, TRUE, 'active')
               ON CONFLICT (company_id, slug) DO UPDATE SET
                   name = EXCLUDED.name,
                   description = EXCLUDED.description,
                   version = EXCLUDED.version,
                   config = EXCLUDED.config,
                   is_paperclip_managed = TRUE,
                   status = 'active',
                   updated_at = now()
               RETURNING id"#,
        )
        .bind(company_id)
        .bind(&bundle.skill.display_name)
        .bind(&bundle.skill.slug)
        .bind(&definition.short_purpose)
        .bind(&bundle.stock_version)
        .bind(config)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        let paths: Vec<&str> = bundle.skill.files.keys().map(String::as_str).collect();
        sqlx::query(
            r#"DELETE FROM skill_files
               WHERE company_id = $1 AND skill_id = $2
                 AND path <> ALL($3::text[])"#,
        )
        .bind(company_id)
        .bind(skill_id)
        .bind(&paths)
        .execute(&mut **tx)
        .await
        .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        for (path, content) in &bundle.skill.files {
            sqlx::query(
                r#"INSERT INTO skill_files (company_id, skill_id, path, content, mime_type, size_bytes)
                   VALUES ($1, $2, $3, $4, 'text/markdown', LENGTH($4))
                   ON CONFLICT (skill_id, path) DO UPDATE SET
                       content = EXCLUDED.content,
                       mime_type = EXCLUDED.mime_type,
                       size_bytes = EXCLUDED.size_bytes,
                       updated_at = now()"#,
            )
            .bind(company_id)
            .bind(skill_id)
            .bind(path)
            .bind(content)
            .execute(&mut **tx)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
        }

        Ok(Some(skill_id))
    }

    /// 物化内置 Agent 的受管 Routine 数据行（以及其触发器），在传入的事务内幂等执行。
    ///
    /// - 第一次 provision：按 `(company_id, routine_key)` 通过 ledger 解析既有的 routine id；
    ///   不存在则新建 `routines` 行（默认沿用定义中的 `status`，内置均为 `paused`），并写入
    ///   对应的 `routine_triggers`（内置均为 `enabled=false` 的 schedule 触发器）。
    /// - 重复 provision / stock 升级：原地 UPDATE routine 内容与触发器，保证幂等且修复内容漂移。
    async fn materialize_routine_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        company_id: Uuid,
        bundle: &BuiltInAgentBundleDefinition,
        agent_id: Uuid,
        existing_routine_id: Option<Uuid>,
    ) -> BuiltInAgentResult<Option<Uuid>> {
        let routine_def = &bundle.routine;

        let priority: i32 = match routine_def.priority {
            RoutinePriority::Critical => 0,
            RoutinePriority::High => 1,
            RoutinePriority::Medium => 2,
            RoutinePriority::Low => 3,
        };
        let status_str: &str = match routine_def.status {
            RoutineStatus::Active => "active",
            RoutineStatus::Paused => "paused",
        };
        let concurrency_str: &str = match routine_def.concurrency_policy {
            RoutineConcurrencyPolicy::AlwaysEnqueue => "parallel",
            RoutineConcurrencyPolicy::CoalesceIfActive => "coalesce_if_active",
            RoutineConcurrencyPolicy::SkipIfActive => "skip_if_active",
        };
        let catch_up_str: &str = match routine_def.catch_up_policy {
            RoutineCatchUpPolicy::EnqueueMissedWithCap => "run_missed",
            RoutineCatchUpPolicy::SkipMissed => "skip_missed",
        };
        let variables: serde_json::Value =
            serde_json::to_value(&routine_def.variables).unwrap_or(serde_json::Value::Null);

        let routine_id = match existing_routine_id {
            Some(id) => {
                sqlx::query(
                    r#"UPDATE routines SET
                       title=$2, description=$3, status=$4::routine_status, priority=$5,
                       concurrency_policy=$6::concurrency_policy,
                       catch_up_policy=$7::catch_up_policy, trigger_config=$8, variables=$9,
                       env=$10, assignee_agent_id=$11, updated_at=now()
                       WHERE id=$1 AND company_id=$12"#,
                )
                .bind(id)
                .bind(&routine_def.title)
                .bind(&routine_def.description)
                .bind(status_str)
                .bind(priority)
                .bind(concurrency_str)
                .bind(catch_up_str)
                .bind(serde_json::json!({}))
                .bind(&variables)
                .bind(serde_json::json!({}))
                .bind(agent_id)
                .bind(company_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                id
            }
            None => {
                let new_id = Uuid::new_v4();
                sqlx::query(
                    r#"INSERT INTO routines
                       (id, company_id, project_id, goal_id, parent_issue_id, name, title, description,
                        agent_id, assignee_agent_id, priority, status, concurrency_policy, catch_up_policy,
                        trigger_config, variables, env, latest_revision_id, latest_revision_number,
                        responsible_user_id, created_by_user_id, last_run_at, next_run_at, run_count,
                        success_count, failure_count, last_triggered_at, last_enqueued_at, created_at, updated_at)
                       VALUES ($1, $2, NULL, NULL, NULL, $3, $4, $5, $6, $7, $8, $9::routine_status,
                               $10::concurrency_policy, $11::catch_up_policy, $12, $13, $14, NULL, 0,
                               NULL, NULL, NULL, NULL, 0, 0, 0, NULL, NULL, now(), now())
                       RETURNING id"#,
                )
                .bind(new_id)
                .bind(company_id)
                .bind(&routine_def.routine_key)
                .bind(&routine_def.title)
                .bind(&routine_def.description)
                .bind(agent_id)
                .bind(agent_id)
                .bind(priority)
                .bind(status_str)
                .bind(concurrency_str)
                .bind(catch_up_str)
                .bind(serde_json::json!({}))
                .bind(&variables)
                .bind(serde_json::json!({}))
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                new_id
            }
        };

        // 幂等物化触发器：按 (routine_id, kind, cron_expression, timezone) 匹配，存在则更新，不存在则插入。
        for (idx, trigger) in routine_def.triggers.iter().enumerate() {
            let kind_str = trigger.kind.as_str();
            let trigger_type_str = "schedule";
            let trigger_status_str: &str = if trigger.enabled {
                "enabled"
            } else {
                "disabled"
            };
            let config = serde_json::json!({
                "cronExpression": trigger.cron_expression,
                "timezone": trigger.timezone,
            });
            let existing = sqlx::query_scalar::<_, Uuid>(
                r#"SELECT id FROM routine_triggers
                   WHERE routine_id = $1 AND kind = $2::trigger_kind
                     AND cron_expression = $3 AND timezone = $4"#,
            )
            .bind(routine_id)
            .bind(kind_str)
            .bind(&trigger.cron_expression)
            .bind(&trigger.timezone)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

            match existing {
                Some(tid) => {
                    sqlx::query(
                        r#"UPDATE routine_triggers SET label=$2, enabled=$3, status=$4::trigger_status,
                           config=$5, cron_expression=$6, timezone=$7, updated_at=now()
                           WHERE id=$1"#,
                    )
                    .bind(tid)
                    .bind(&trigger.label)
                    .bind(trigger.enabled)
                    .bind(trigger_status_str)
                    .bind(&config)
                    .bind(&trigger.cron_expression)
                    .bind(&trigger.timezone)
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                }
                None => {
                    let tid = Uuid::new_v4();
                    sqlx::query(
                        r#"INSERT INTO routine_triggers
                           (id, company_id, routine_id, kind, label, enabled, trigger_type, config,
                            status, next_trigger_at, last_triggered_at, cron_expression, timezone,
                            next_run_at, last_fired_at, public_id, secret_id, signing_mode,
                            replay_window_sec, last_rotated_at, last_result, created_at, updated_at)
                           VALUES ($1, $2, $3, $4::trigger_kind, $5, $6, $7::trigger_type, $8,
                                   $9::trigger_status, NULL, NULL, $10, $11, NULL, NULL,
                                   $12, NULL, NULL, NULL, NULL, NULL, now(), now())"#,
                    )
                    .bind(tid)
                    .bind(company_id)
                    .bind(routine_id)
                    .bind(kind_str)
                    .bind(&trigger.label)
                    .bind(trigger.enabled)
                    .bind(trigger_type_str)
                    .bind(&config)
                    .bind(trigger_status_str)
                    .bind(&trigger.cron_expression)
                    .bind(&trigger.timezone)
                    .bind(format!("builtin-{}-{}", routine_id, idx))
                    .execute(&mut **tx)
                    .await
                    .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                }
            }
        }

        Ok(Some(routine_id))
    }

    async fn clear_managed_resource_bindings(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<()> {
        let bindings = self
            .managed_repo
            .list_by_company_and_key(company_id, key.as_str())
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        if let Some(pool) = &self.resource_pool {
            let mut transaction = pool
                .begin()
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            for binding in bindings.iter() {
                if binding.resource_type == "skill" {
                    if let Some(skill_id) = binding.target_resource_id {
                        sqlx::query(
                            "DELETE FROM company_skills WHERE id = $1 AND company_id = $2",
                        )
                        .bind(skill_id)
                        .bind(company_id)
                        .execute(&mut *transaction)
                        .await
                        .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                    }
                } else if binding.resource_type == "routine" {
                    if let Some(routine_id) = binding.target_resource_id {
                        sqlx::query("DELETE FROM routine_triggers WHERE routine_id = $1")
                            .bind(routine_id)
                            .execute(&mut *transaction)
                            .await
                            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                        sqlx::query(
                            "DELETE FROM routines WHERE id = $1 AND company_id = $2",
                        )
                        .bind(routine_id)
                        .bind(company_id)
                        .execute(&mut *transaction)
                        .await
                        .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
                    }
                }
            }
            sqlx::query(
                "DELETE FROM builtin_managed_resources WHERE company_id = $1 AND built_in_key = $2",
            )
            .bind(company_id)
            .bind(key.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            transaction
                .commit()
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            return Ok(());
        }

        self.managed_repo
            .delete_by_company_and_key(company_id, key.as_str())
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
        Ok(())
    }

    /// 查找公司的唯一根Agent
    async fn find_single_root_agent(&self, company_id: Uuid) -> BuiltInAgentResult<Option<Uuid>> {
        let agents = self
            .agent_repo
            .list_by_company(company_id, repositories::ListAgentsOptions::default())
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        // 查找没有上级的Agent（根Agent）
        let root_agents: Vec<_> = agents
            .iter()
            .filter(|a| a.reports_to.is_none())
            .collect();

        if root_agents.len() == 1 {
            Ok(Some(root_agents[0].id))
        } else {
            Ok(None)
        }
    }

    /// 根据定义和用户输入创建Agent
    async fn create_agent_from_definition(
        &self,
        company_id: Uuid,
        definition: &BuiltInAgentDefinition,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent> {
        // 解析默认上级
        let reports_to = if let Some(ref manager) = definition.default_manager {
            if manager == "single_root_agent" {
                self.find_single_root_agent(company_id).await?
            } else {
                None
            }
        } else {
            None
        };

        // 确定适配器类型：用户输入 > 定义默认
        let adapter_type = input
            .and_then(|i| i.adapter_type.clone())
            .or_else(|| {
                definition
                    .allowed_adapter_types
                    .as_ref()
                    .and_then(|types| types.first())
                    .cloned()
            })
            .unwrap_or_else(|| "process".to_string());

        // 确定适配器配置：用户输入 > 空对象
        let adapter_config = input
            .and_then(|i| i.adapter_config.clone())
            .unwrap_or(serde_json::json!({}));

        // 确定预算：用户输入 > 定义默认 > 0
        let budget = input
            .and_then(|i| i.budget_monthly_cents)
            .or(definition.default_budget_monthly_cents)
            .unwrap_or(0);

        let agent = models::Agent {
            id: Uuid::new_v4(),
            company_id,
            name: definition.display_name.clone(),
            role: definition.default_role,
            status: definition.default_status.unwrap_or(models::AgentStatus::Idle),
            adapter_type,
            adapter_config: sqlx::types::Json(adapter_config),
            runtime_config: sqlx::types::Json(serde_json::json!({})),
            permissions: sqlx::types::Json(
                definition
                    .default_permissions
                    .clone()
                    .unwrap_or_default(),
            ),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: Some(true),
                built_in_key: Some(definition.key.as_str().to_string()),
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: budget,
            reports_to,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.agent_repo
            .create(agent)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))
    }

    /// 查找已存在的内置Agent
    async fn find_existing_agent(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<Option<models::Agent>> {
        let agents = self
            .agent_repo
            .list_by_company(company_id, repositories::ListAgentsOptions {
                include_terminated: true,
                limit: None,
                offset: None,
            })
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        Ok(agents
            .into_iter()
            .find(|a| {
                a.metadata
                    .0
                    .built_in_key
                    .as_ref()
                    .map(|k| k == key.as_str())
                    .unwrap_or(false)
            }))
    }

    /// 将内置 Agent 的指令资源物化到 Agent 元数据。
    ///
    /// Paperclip 将内置资源包作为受管文件树同步到运行时工作区；当前
    /// Agent 模型没有独立的工作区文件表，因此使用同等语义的
    /// `instructions_bundle` 持久化文件树，并通过 `instructions_path`
    /// 暴露入口文件。
    async fn materialize_bundle(
        &self,
        agent: &mut models::Agent,
        definition: &BuiltInAgentDefinition,
    ) -> BuiltInAgentResult<()> {
        let (entry_file, files) = if let Some(bundle) = &definition.bundle {
            (
                bundle.instructions.entry_file.clone(),
                bundle.instructions.files.clone(),
            )
        } else {
            let entry_file = "AGENTS.md".to_string();
            let mut files = std::collections::HashMap::new();
            files.insert(entry_file.clone(), definition.default_instructions.clone());
            (entry_file, files)
        };

        let (skill, routine) = definition
            .bundle
            .as_ref()
            .map(|bundle| {
                (
                    serde_json::to_value(&bundle.skill).unwrap_or(serde_json::Value::Null),
                    serde_json::to_value(&bundle.routine).unwrap_or(serde_json::Value::Null),
                )
            })
            .unwrap_or((serde_json::Value::Null, serde_json::Value::Null));
        let instructions = serde_json::json!({
            "entryFile": entry_file,
            "files": files,
        });
        let bundle = serde_json::json!({
            "stockVersion": definition.bundle.as_ref().map(|b| b.stock_version.clone()),
            "instructions": instructions,
            "skill": skill,
            "routine": routine,
        });
        let changed = agent.metadata.0.instructions_path.as_deref() != Some(entry_file.as_str())
            || agent.metadata.0.instructions_bundle.as_ref() != Some(&bundle);
        agent.metadata.0.instructions_path = Some(entry_file);
        agent.metadata.0.instructions_bundle = Some(bundle);
        if changed {
            agent.updated_at = chrono::Utc::now();
        }
        Ok(())
    }
}

#[async_trait]
impl<A> BuiltInAgentService for DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    async fn provision(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent> {
        // 获取定义
        let definition = self
            .registry
            .get_definition(key)
            .ok_or(BuiltInAgentError::NotFound(key))?;

        let is_new = self.find_existing_agent(company_id, key).await?.is_none();
        let mut agent = if is_new {
            self.create_agent_from_definition(company_id, definition, input)
                .await?
        } else {
            let mut existing = self
                .find_existing_agent(company_id, key)
                .await?
                .ok_or(BuiltInAgentError::NotFound(key))?;
            if let Some(input) = input {
                if let Some(ref adapter_type) = input.adapter_type {
                    existing.adapter_type = adapter_type.clone();
                }
                if let Some(ref adapter_config) = input.adapter_config {
                    existing.adapter_config = sqlx::types::Json(adapter_config.clone());
                }
                if let Some(budget) = input.budget_monthly_cents {
                    existing.budget_monthly_cents = budget;
                }
                existing.updated_at = chrono::Utc::now();
            }
            existing
        };

        // 物化指令文件
        self.materialize_bundle(&mut agent, definition).await?;
        let saved = self
            .agent_repo
            .update(agent)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        // 同步受管资源绑定；若绑定失败则回滚新建的 Agent，避免留下半预置的孤立 Agent。
        if let Err(e) = self.sync_managed_resources(company_id, key, definition, saved.id).await {
            if is_new {
                let _ = self.agent_repo.delete(saved.id).await;
            }
            return Err(e);
        }

        Ok(saved)
    }

    async fn get_status(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<BuiltInAgentStatus> {
        let agent = self.find_existing_agent(company_id, key).await?;
        Ok(crate::built_in_agent_service::derive_built_in_agent_status(
            agent.as_ref(),
            None,
        ))
    }

    async fn reset(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<()> {
        // 查找Agent
        let agent = self
            .find_existing_agent(company_id, key)
            .await?
            .ok_or(BuiltInAgentError::NotFound(key))?;

        let definition = self
            .registry
            .get_definition(key)
            .ok_or(BuiltInAgentError::NotFound(key))?;

        // 回滚边界：先清理受管资源绑定，失败则不应宣称 Agent 已重置。
        self.clear_managed_resource_bindings(company_id, key).await?;

        // 重置为初始状态
        let mut updated_agent = agent;
        updated_agent.status = definition.default_status.unwrap_or(models::AgentStatus::Idle);
        updated_agent.adapter_config = sqlx::types::Json(serde_json::json!({}));
        updated_agent.runtime_config = sqlx::types::Json(serde_json::json!({}));
        updated_agent.metadata.0.instructions_path = None;
        updated_agent.metadata.0.instructions_bundle = None;
        updated_agent.updated_at = chrono::Utc::now();

        self.agent_repo
            .update(updated_agent)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn reconcile(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<ReconcileResult> {
        let mut result = ReconcileResult::default();

        // 检查Agent是否存在
        let agent = self.find_existing_agent(company_id, key).await?;
        if agent.is_none() {
            result.changes.push("Agent not provisioned".to_string());
            return Ok(result);
        }

        let mut agent = agent.expect("checked above");
        let before_path = agent.metadata.0.instructions_path.clone();
        let before_bundle = agent.metadata.0.instructions_bundle.clone();
        let definition = self
            .registry
            .get_definition(key)
            .ok_or(BuiltInAgentError::NotFound(key))?;
        self.materialize_bundle(&mut agent, definition).await?;
        let after_bundle = agent.metadata.0.instructions_bundle.as_ref();
        let before_instructions = before_bundle
            .as_ref()
            .and_then(|bundle| bundle.get("instructions"))
            .or_else(|| before_bundle.as_ref());
        let after_instructions = after_bundle.and_then(|bundle| bundle.get("instructions"));
        result.instructions_materialized = before_path != agent.metadata.0.instructions_path
            || before_instructions != after_instructions;

        // 同步受管资源绑定并检测/修复漂移。
        let (managed_changed, managed_changes) =
            self.sync_managed_resources(company_id, key, definition, agent.id).await?;
        result.changes.extend(managed_changes);

        let bindings = self
            .managed_repo
            .list_by_company_and_key(company_id, key.as_str())
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
        result.skills_synced = bindings.iter().any(|b| b.resource_type == "skill");
        result.routines_synced = bindings.iter().any(|b| b.resource_type == "routine");

        if result.instructions_materialized || managed_changed {
            self.agent_repo
                .update(agent)
                .await
                .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;
            result.agent_updated = true;
            if result.instructions_materialized {
                result.changes.push("Synchronized managed instruction bundle".to_string());
            }
            if result.skills_synced {
                result.changes.push("Synchronized managed skill binding".to_string());
            }
            if result.routines_synced {
                result.changes.push("Synchronized managed routine binding".to_string());
            }
        }
        Ok(result)
    }

    fn list_definitions(&self) -> Vec<&BuiltInAgentDefinition> {
        self.registry.list_definitions()
    }

    fn get_definition(&self, key: BuiltInAgentKey) -> Option<&BuiltInAgentDefinition> {
        self.registry.get_definition(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconcile_result_default() {
        let result = ReconcileResult::default();
        assert!(!result.agent_updated);
        assert!(!result.instructions_materialized);
        assert!(!result.skills_synced);
        assert!(!result.routines_synced);
        assert!(result.changes.is_empty());
    }
}
