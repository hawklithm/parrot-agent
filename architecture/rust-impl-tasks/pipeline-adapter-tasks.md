# Pipeline/Adapter 模块 - Rust 实现任务拆解

> 基于 [backend/pipeline-adapter.md](../backend/pipeline-adapter.md) 架构分析文档，拆解为 Rust 版本实现任务。
> 版本: 1.0
> 日期: 2026/07/11

---

## 1. 数据模型层 实现任务

### 阶段一：基础架构

- [x] **定义 Pipeline 核心枚举与结构体**
  - 定义 `PipelineStageKind` 枚举（open / working / review / done / cancelled）
  - 定义 `Pipeline` 结构体，映射核心字段（id, company_id, key, name, description, project_id, enforce_transitions, created_at, updated_at）
  - 定义 `CreatePipelineInput` 结构体（company_id, key, name, description, project_id, enforce_transitions, stages）

- [x] **定义 PipelineStage 结构体**
  - 定义 `PipelineStage` 结构体（id, pipeline_id, key, name, kind, position, config, created_at, updated_at）
  - 定义 `PipelineStageConfig` 结构体（autonomy, auto_advance_on_children_terminal, approve_to_stage_key, reject_to_stage_key, request_changes_to_stage_key, require_reject_reason, require_request_changes_reason, require_children_terminal, require_no_unresolved_drift, disabled, require_approval, approver, reviewer_kind, variables, automation, breakdown）
  - 定义 `StageApprover` 结构体（kind: any_human / user / agent, id: Option<String>）

- [x] **定义 PipelineCase 结构体**
  - 定义 `PipelineCase` 结构体（id, company_id, pipeline_id, stage_id, case_key, title, summary, fields, terminal_kind, version, pending_suggestion, created_at, updated_at）
  - 定义 `TerminalKind` 枚举（done / cancelled / null 用 Option<TerminalKind> 表达）
  - 定义 `CreateCaseInput` 结构体（title, fields 等）

- [x] **定义 PipelineTransition 结构体**
  - 定义 `PipelineTransition` 结构体（id, pipeline_id, from_stage_id, to_stage_id, label, conditions）
  - 定义 `TransitionConditions` 结构体（JSONB 映射：转换前置条件）
  - 定义 `CreateTransitionInput` 结构体（from_stage_key, to_stage_key, label, conditions）

### 阶段二：核心功能

- [x] **实现 Database Schema 迁移 - Pipeline 相关表**
  - 编写 `pipelines` 表 migration（company_id 外键, key 唯一约束 per company, 索引）
  - 编写 `pipeline_stages` 表 migration（pipeline_id 外键, position 排序, kind 枚举约束）
  - 编写 `pipeline_transitions` 表 migration（from_stage_id / to_stage_id 外键 -> pipeline_stages.id）

- [x] **实现 Database Schema 迁移 - Case 相关表**
  - 编写 `pipeline_cases` 表 migration（pipeline_id 外键, stage_id 外键, company_id 外键, terminal_kind, version 乐观锁）
  - 编写 `case_events` 表 migration（事件溯源，case_id 外键, event_type, payload JSONB）
  - 编写索引：pipeline_id + stage_id 联合索引, company_id 索引, terminal_kind 索引

- [x] **实现 Repository trait - Pipeline**
  - 定义 `PipelineRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 实现 PostgreSQL 版本的 `PipelineRepository`（含 stages / transitions 级联创建）
  - 编写 Repository 单元测试（使用 test container 或 mock）

- [x] **实现 Repository trait - Case / Stage / Transition**
  - 定义 `CaseRepository` trait（create, get_by_id, list_by_pipeline, advance, update_terminal_kind）
  - 定义 `StageRepository` trait（create, get_by_id, list_by_pipeline, update, delete）
  - 定义 `TransitionRepository` trait（create, get_by_id, list_by_pipeline, update, delete）

### 阶段三：高级特性

- [x] **实现 JSONB 字段类型安全映射**
  - 为 `PipelineStageConfig` JSONB 实现 Rust 类型安全的序列化/反序列化
  - 为 `TransitionConditions` JSONB 实现类型安全映射
  - 为 `Case.fields` JSONB 实现 `serde_json::Value` 到业务类型的转换

- [x] **实现 Pipeline 级联创建事务**
  - 实现 `create_pipeline_with_stages_and_transactions()` 事务方法（单事务创建 Pipeline + Stages + Transitions）
  - 实现乐观并发控制（version 字段检查，冲突时返回 `OptimisticLockError`）
  - 编写事务边界测试（并发创建同 key Pipeline 应冲突）

---

## 2. Pipeline 服务层 实现任务

### 阶段一：基础架构

- [x] **定义 PipelineService trait**
  - 定义 `PipelineService` trait（create_pipeline, get_pipeline, update_pipeline, delete_pipeline, list_pipelines_by_company）
  - 定义 `CaseService` trait（create_case, advance_case, reject_case, approve_case, request_changes, breakdown_case）
  - 定义 `StageService` trait（create_stage, update_stage, delete_stage, list_stages）

- [x] **定义 Pipeline 校验逻辑**
  - 实现 `validate_pipeline_input()` 函数（校验 key 格式、name 非空、stages 非空）
  - 实现 `validate_stage_config()` 函数（校验 autonomy 合法值、approve/reject_to_stage_key 指向存在的 stage）
  - 定义 `PipelineValidationError` 枚举（InvalidKey, EmptyStages, DuplicateStageKey, InvalidTransitionCycle）

- [x] **定义案例操作枚举与输入**
  - 定义 `CaseAdvanceInput` 结构体（company_id, case_id, to_stage_key, actor）
  - 定义 `CaseReviewDecision` 枚举（approve, reject, request_changes）
  - 定义 `CaseReviewInput` 结构体（decision, reason, actor）

### 阶段二：核心功能

- [x] **实现 Pipeline CRUD 服务**
  - 实现 `create_pipeline()`（调用 repository 级联创建 + 校验）
  - 实现 `list_pipelines_by_company()`（含分页支持）
  - 实现 `update_pipeline()` / `delete_pipeline()`（含权限检查集成点）

- [x] **实现案例推进核心逻辑**
  - 实现 `advance_case()`：验证转换规则合法（from_stage -> to_stage 存在于 transitions）、更新 stage_id、评估自动推进
  - 实现 `evaluate_auto_advance()`：检查 `auto_advance_on_children_terminal` 配置，递归推进子案例
  - 实现 `review_case()`：根据 `CaseReviewDecision` 执行审批/拒绝/请求修改，更新 terminal_kind

- [x] **实现案例分解逻辑**
  - 实现 `breakdown_case()`：按 `PipelineStageConfig.breakdown` 配置创建子案例
  - 实现 `target_pipeline_id` / `target_stage_key` 解析（跨 Pipeline 分解支持）
  - 实现父案例与子案例的关联维护（parent_case_id 字段）

- [x] **实现健康警告与关注列表**
  - 实现 `get_health_warnings()`：检查停滞案例、阻塞案例、审批超时
  - 实现 `get_pipelines_attention()`：按公司维度聚合需要关注的 Pipeline
  - 定义 `HealthWarning` 结构体（warning_type, pipeline_id, case_id, message, severity）

### 阶段三：高级特性

- [x] **实现批量审核**
  - 实现 `bulk_review_cases()`：批量执行审批/拒绝操作（事务性保证）
  - 实现批量操作的结果聚合（部分成功/部分失败处理）
  - 定义 `BulkReviewResult` 结构体（succeeded: Vec<CaseId>, failed: Vec<(CaseId, Error)>）

- [x] **实现 Pipeline 阶段自动化配置**
  - 实现 `StageAutomationConfig` 解析（routine_id, assignee_agent_id）
  - 实现自动化触发集成点（stage 进入时触发 routine 的 hook）
  - 实现 `require_approval` + `approver` 的审批流程编排

---

## 3. Adapter Registry 层 实现任务

### 阶段一：基础架构

- [x] **定义 ServerAdapter trait**
  - 定义 `ServerAdapter` trait（type() -> &str, label() -> Option<&str>, execute()）
  - 定义 `AdapterModel` 结构体（id, label, provider, context_window, max_output_tokens）
  - 定义 `AdapterConfigSchema` 结构体（type: "object", properties, required）

- [x] **定义 Adapter 执行上下文与结果**
  - 定义 `AdapterExecutionContext` 结构体（run_id, agent, runtime, config, context, execution_target, on_log callback）
  - 定义 `AdapterExecutionResult` 结构体（status: ok/error, exit_code, output, error, metadata）
  - 定义 `AdapterExecutionTarget` 枚举（Local, Remote, Sandbox）

- [x] **定义 Adapter 能力标志接口**
  - 定义 `AdapterCapabilities` 结构体（supports_instructions_bundle, supports_skills, supports_local_agent_jwt, requires_materialized_runtime_skills, supports_model_profiles）
  - 定义 `SessionManagementConfig` 结构体（会话管理配置）
  - 定义 `AcpAdapterTargetDescriptor` 结构体（ACP 支持描述）

### 阶段二：核心功能

- [x] **实现 AdapterRegistry 核心注册表**
  - 实现 `AdapterRegistry` 结构体，持有 `HashMap<String, Box<dyn ServerAdapter>>`（adapters_by_type）
  - 实现 `register_server_adapter()` 方法（注册新 adapter，处理内置 fallback）
  - 实现 `unregister_server_adapter()` 方法（卸载外部 adapter，恢复内置 fallback）

- [x] **实现 Adapter 覆盖机制**
  - 实现 `builtin_fallbacks: HashMap<String, Box<dyn ServerAdapter>>` 存储原始内置 Adapter
  - 实现 `paused_overrides: HashSet<String>` 控制是否使用内置 Adapter
  - 实现 `find_active_server_adapter()` 方法（paused_overrides 优先返回 builtin_fallback）

- [x] **实现内置 Adapter 类型集合与查找**
  - 定义 `BUILTIN_ADAPTER_TYPES` 常量集合（claude_local, codex_local, cursor_local, cursor_cloud, gemini_local, pi_local, hermes, hermes_gateway, grok_local, opencode_local, openclaw_gateway, custom_acp）
  - 实现 `find_server_adapter()` 方法（直接查找 adapters_by_type）
  - 实现 `list_server_adapters()` 方法（返回所有已注册 adapter 列表）

### 阶段三：高级特性

- [x] **实现 Adapter 热重载机制**
  - 实现 `reload_adapter()` 方法：卸载当前 adapter -> 重新加载模块 -> 重新注册
  - 实现 `register_with_session_management()` 方法（注册时处理会话迁移）
  - 实现热重载期间的请求排队/缓冲策略（避免重载期间丢失执行请求）

- [x] **实现 Adapter 配置 Schema 验证**
  - 实现 `get_config_schema()` trait 方法（adapter 自描述配置 schema）
  - 实现 `validate_adapter_config()` 函数（根据 schema 验证配置 JSON）
  - 定义 `ConfigValidationError` 枚举（MissingRequiredField, TypeMismatch, InvalidEnumValue）

---

## 4. Adapter 插件系统 实现任务

### 阶段一：基础架构

- [ ] **定义 Adapter 插件加载接口**
  - 定义 `AdapterPluginLoader` trait（load_from_path(), load_from_npm()）
  - 定义 `AdapterPluginRecord` 结构体（type, package_name, version, is_local_path, local_path, installed_at）
  - 定义 `AdapterInstallRequest` 结构体（package_name, is_local_path, version）

- [ ] **定义技能与模型 Profile 接口**
  - 定义 `AdapterSkillEntry` 结构体（skill_id, name, description, content）
  - 定义 `AdapterModelProfileDefinition` 结构体（key, label, config_overrides）
  - 定义 `ModelProfileKey` 枚举（default, fast, balanced, deep）

- [x] **定义模型 Profile 应用解析**
  - 定义 `ModelProfileApplication` 结构体（requested, requested_by, applied, config_source, fallback_reason, adapter_config）
  - 定义 `ModelProfileRequestSource` 枚举（issue_override, wake_context）
  - 定义 `AppliedModelProfileConfigSource` 枚举（agent_runtime, adapter_default）

### 阶段二：核心功能

- [ ] **实现本地路径 Adapter 加载**
  - 实现 `normalize_local_path()` 函数（路径安全校验与规范化）
  - 实现 `load_external_adapter_package()` 从本地路径加载 adapter 动态库（使用 libloading 或 WASM）
  - 实现加载后的 `register_server_adapter()` 集成

- [ ] **实现 NPM 包 Adapter 安装**
  - 实现 `get_adapter_plugins_dir()` 函数（获取插件目录路径）
  - 实现 `npm_install_adapter()` 函数（执行 npm install --no-save，带 120s 超时）
  - 实现 `read_installed_package_version()` 从 package.json 读取版本信息

- [ ] **实现 Adapter 插件持久化**
  - 实现 `AdapterPluginStore` 结构体（管理 AdapterPluginRecord 列表）
  - 实现 `add_adapter_plugin()` / `remove_adapter_plugin()` 方法
  - 实现插件记录的文件持久化（JSON 文件存储）

### 阶段三：高级特性

- [x] **实现模型 Profile 优先级解析**
  - 实现 `resolve_model_profile_application()` 函数：Issue 级别 > 运行上下文 > Adapter 默认
  - 实现 `read_context_model_profile()` 从上下文快照提取 model profile
  - 实现 fallback 链路（requested profile 不支持时降级到 default）

- [x] **实现技能注入机制**
  - 实现 `materialize_skills()` 函数：将技能内容写入执行环境
  - 实现 `inject_skills()` 在 adapter 执行前注入技能到工作目录
  - 实现 `sync_skills()` 双向同步技能变更

---

## 5. Adapter 执行引擎 实现任务

### 阶段一：基础架构

- [ ] **定义执行引擎核心接口**
  - 定义 `AdapterExecutor` trait（execute(ctx) -> AdapterExecutionResult）
  - 定义 `AdapterRuntime` 结构体（runtime_command_spec, environment, working_dir）
  - 定义 `AdapterRuntimeCommandSpec` 结构体（command, args, env）

- [ ] **定义执行回调接口**
  - 定义 `LogSink` trait（on_log(stream: StdioKind, chunk: &str)）
  - 定义 `RuntimeStatusSink` trait（on_runtime_progress(status: &RuntimeStatus)）
  - 定义 `SpawnNotifier` trait（on_spawn(pid, process_group_id, started_at)）

- [ ] **定义本地与远程执行类型**
  - 定义 `LocalExecutor` 结构体（持有 tokio::process::Command）
  - 定义 `RemoteExecutor` 结构体（持有远程连接配置）
  - 定义 `ExecutionTargetConfig` 结构体（target_type, connection_info, asset_sync_config）

### 阶段二：核心功能

- [ ] **实现本地 Adapter 执行**
  - 实现 `LocalExecutor::execute()`：准备运行时配置 -> 构建环境变量 -> spawn 子进程
  - 实现 stdout/stderr 流式输出回调（tokio 异步管道读取 -> on_log）
  - 实现进程生命周期管理（启动、超时 kill、退出码收集）

- [x] **实现运行时配置准备**
  - 实现 `prepare_runtime_config()` 函数：合并 adapter config + agent runtime config + model profile config
  - 实现 `build_paperclip_env()` 函数：构建 PAPERCLIP_* 环境变量集
  - 实现 `prepare_credentials()` 函数：安全注入认证凭据（auth_token, api_key 等）

- [x] **实现执行目标抽象**
  - 实现 `read_adapter_execution_target()` 函数：从配置解析执行目标类型
  - 实现 `prepare_adapter_execution_target_runtime()` 函数：准备远程/Sandbox 执行环境
  - 实现 `sync_workspace()` / `sync_runtime_assets()` 函数：本地到远程文件同步

### 阶段三：高级特性

- [x] **实现远程/Sandbox 执行**
  - 实现 `RemoteExecutor::execute()`：建立连接 -> 同步资产 -> 远程 spawn -> 收集输出
  - 实现 `sync_assets()` 函数：增量同步工作目录到远程目标
  - 实现远程执行的错误恢复与重试策略

- [x] **实现 ACP (Agent Communication Protocol) 适配**
  - 实现 `AcpAdapterTargetDescriptor` 解析与运行时适配
  - 实现 ACP 协议的消息收发（基于 HTTP/WebSocket）
  - 实现 ACP 执行结果的结构化解析

---

## 6. Pipeline HTTP 路由层 实现任务

### 阶段一：基础架构

- [ ] **定义 Pipeline 路由框架**
  - 使用 axum/actix-web 定义 `/api/companies/:company_id/pipelines` 路由组
  - 定义 `PipelineRouter` 结构体，注入 `Arc<dyn PipelineService>` 和 `Arc<dyn AccessService>`
  - 实现路由中间件：认证 + 公司访问权限校验（assert_pipeline_company_access）

- [ ] **定义 Case 路由框架**
  - 定义 `/api/pipelines/:pipeline_id/cases` 路由组
  - 定义 `CaseRouter` 结构体，注入 `Arc<dyn CaseService>` 和 `Arc<dyn AccessService>`
  - 实现路由中间件：案例访问权限校验（assert_pipeline_case_access）

- [x] **定义 Stage/Transition 路由框架**
  - 定义 `/api/pipelines/:pipeline_id/stages` 路由组
  - 定义 `/api/pipelines/:pipeline_id/transitions` 路由组
  - 定义 `StageRouter` / `TransitionRouter` 结构体

### 阶段二：核心功能

- [ ] **实现 Pipeline CRUD 路由端点**
  - 实现 `POST /companies/:company_id/pipelines`（create_pipeline + 权限检查 pipelines:write）
  - 实现 `GET /companies/:company_id/pipelines`（list_pipelines_by_company）
  - 实现 `GET /pipelines/:pipeline_id` / `PATCH` / `DELETE`（详情/更新/删除）

- [ ] **实现 Case 操作路由端点**
  - 实现 `POST /pipelines/:pipeline_id/cases`（create_case）
  - 实现 `PATCH /pipelines/:pipeline_id/cases/:case_id/advance`（advance_case + 转换验证）
  - 实现 `POST /cases/:case_id/approve` / `reject` / `request-changes` / `breakdown`（案例生命周期操作）

- [x] **实现 Stage/Transition CRUD 路由端点**
  - 实现 `GET /pipelines/:pipeline_id/stages` / `POST` / `PATCH /:stage_id` / `DELETE`
  - 实现 `GET /pipelines/:pipeline_id/transitions` / `POST` / `PATCH /:transition_id` / `DELETE`
  - 实现输入校验中间件（使用 validator crate 或自定义校验）

### 阶段三：高级特性

- [ ] **实现审核与关注路由端点**
  - 实现 `GET /companies/:company_id/pipelines-attention`（获取需要关注的 Pipeline）
  - 实现 `GET /companies/:company_id/review-cases`（列出待审核案例）
  - 实现 `POST /companies/:company_id/review-cases/bulk`（批量审核）

- [ ] **实现健康警告与事件路由**
  - 实现 `GET /pipelines/:pipeline_id/health-warnings`（获取健康警告）
  - 实现 `GET /companies/:company_id/case-events`（列出公司案例事件）
  - 定义分页、过滤、排序查询参数结构体

---

## 7. Adapter HTTP 路由层 实现任务

### 阶段一：基础架构

- [x] **定义 Adapter 路由框架**
  - 使用 axum/actix-web 定义 `/api/adapters` 路由组
  - 定义 `AdapterRouter` 结构体，注入 `Arc<AdapterRegistry>` 和 `Arc<AdapterPluginStore>`
  - 实现路由中间件：实例管理员权限校验（assert_instance_admin）

- [x] **定义 LLM 配置路由框架**
  - 定义 `/api/llms` 路由组
  - 定义 `LlmRouter` 结构体，注入 `Arc<dyn AgentService>` 和 `Arc<AdapterRegistry>`
  - 定义配置文档响应格式（text/plain Content-Type）

### 阶段二：核心功能

- [x] **实现 Adapter 列表与详情路由**
  - 实现 `GET /api/adapters`（列出所有已注册 Adapter）
  - 实现 `GET /api/adapters/:type`（获取特定 Adapter 信息）
  - 实现 `GET /api/adapters/:type/config-schema`（获取 Adapter 配置 Schema）

- [x] **实现 Adapter 安装/卸载路由**
  - 实现 `POST /api/adapters/install`（安装外部 Adapter，支持本地路径 + NPM 两种模式）
  - 实现 `DELETE /api/adapters/:type`（卸载外部 Adapter，保护内置 Adapter）
  - 实现 `POST /api/adapters/:type/reinstall`（重新安装 Adapter）

- [x] **实现 Adapter 管理路由**
  - 实现 `PATCH /api/adapters/:type`（启用/禁用 Adapter）
  - 实现 `PATCH /api/adapters/:type/override`（暂停/恢复 Adapter 覆盖）
  - 实现 `POST /api/adapters/:type/reload`（热重载 Adapter）

### 阶段三：高级特性

- [x] **实现 LLM 配置文档路由**
  - 实现 `GET /api/llms/agent-configuration.txt`（获取 Agent 配置索引）
  - 实现 `GET /api/llms/agent-icons.txt`（获取可用 Agent 图标列表）
  - 实现 `GET /api/llms/agent-configuration/:adapter_type.txt`（获取特定 Adapter 配置文档）

- [x] **实现 Adapter UI 解析器路由**
  - 实现 `GET /api/adapters/:type/ui-parser.js`（获取 Adapter UI 解析器脚本）
  - 实现 JS 文件的缓存控制（ETag / Last-Modified）
  - 实现 Content-Type: application/javascript 响应头

---

## 8. 配置系统 实现任务

### 阶段一：基础架构

- [ ] **定义 Config 核心枚举**
  - 定义 `DeploymentMode` 枚举（local_trusted / authenticated）
  - 定义 `DeploymentExposure` 枚举（private / public）
  - 定义 `BindMode` 枚举（loopback / lan / tailnet / custom）

- [ ] **定义 Config 数据库与存储枚举**
  - 定义 `DatabaseMode` 枚举（embedded_postgres / postgres）
  - 定义 `StorageProvider` 枚举（local_disk / s3）
  - 定义 `SecretProvider` 枚举（local_encrypted / aws_secrets_manager / gcp_secret_manager / vault）

- [ ] **定义 Config 结构体**
  - 定义 `Config` 结构体，包含所有配置字段（deployment, auth, database, storage, secrets, ui, heartbeat 等）
  - 定义 `ConfigBuilder` 使用 builder 模式逐步构建
  - 定义 `ConfigError` 枚举（MissingRequired, InvalidValue, ParseError）

### 阶段二：核心功能

- [ ] **实现配置加载优先级**
  - 实现环境变量加载（PAPERCLIP_* 前缀，支持嵌套 key 如 PAPERCLIP_DATABASE_URL）
  - 实现配置文件加载（config.yaml / config.toml，使用 serde 反序列化）
  - 实现优先级合并逻辑：环境变量 > 配置文件 > 默认值

- [ ] **实现配置校验与默认值**
  - 实现 `Config::validate()` 方法（校验端口范围、URL 格式、路径存在性等）
  - 实现 `Default for Config` trait（合理默认值）
  - 实现 `Config::load()` 入口方法（加载 + 校验 + 返回）

- [x] **实现运行时配置热更新**
  - 实现 `Config::reload()` 方法（重新从文件/环境变量加载配置）
  - 实现 `ConfigWatch` 结构体（文件变更监听，使用 notify crate）
  - 实现配置变更通知机制（tokio broadcast channel）

### 阶段三：高级特性

- [x] **实现敏感配置加密存储**
  - 实现 `SecretsManager` trait（get_secret, set_secret, delete_secret）
  - 实现 `LocalEncryptedSecretsManager`（使用 AES-256-GCM + master key 文件）
  - 实现 master key 文件权限校验（0600）

- [x] **实现配置审计日志**
  - 实现 `ConfigChangeLog` 结构体记录配置变更历史
  - 实现敏感字段变更的审计追踪（谁/何时/改了什么）
  - 实现配置回滚功能（从审计日志恢复先前配置）

---

## 9. 权限集成层 实现任务

### 阶段一：基础架构

- [x] **定义 Pipeline 权限动作**
  - 定义 `PipelineAction` 枚举（pipelines:write, pipelines:read, pipeline_cases:write, pipeline_cases:read, pipeline_stages:write, pipeline_transitions:write）
  - 定义 `PipelineResource` 结构体（type: company/pipeline/case, id）
  - 定义 `AccessDecision` 结构体（allowed, explanation）

- [x] **定义公司访问校验接口**
  - 定义 `assert_pipeline_company_access()` 函数签名
  - 定义 `assert_pipeline_case_access()` 函数签名
  - 定义 `ActorForMutation` 结构体（actor_type, actor_id）

- [x] **定义实例管理员校验接口**
  - 定义 `assert_instance_admin()` 函数签名
  - 定义 `InstanceAdminError` 错误类型
  - 集成到 Adapter 路由中间件

### 阶段二：核心功能

- [x] **实现 ABAC 权限决策集成**
  - 实现 `AccessService::decide()` 在 Pipeline/Case 操作中的集成
  - 实现权限拒绝时的 HTTP 响应（403 Forbidden + explanation）
  - 实现 `actor_for_mutation()` 从请求上下文提取 actor 信息

- [x] **实现 Pipeline/Case 级别权限校验**
  - 实现 `assert_pipeline_company_access()`：验证用户对公司的 Pipeline 访问权限
  - 实现 `assert_pipeline_case_access()`：验证用户对特定案例的操作权限
  - 实现权限校验与路由 handler 的集成（axum middleware layer）

### 阶段三：高级特性

- [ ] **实现审批流程权限控制**
  - 实现 `StageApprover` 匹配逻辑（any_human / user / agent 三种审批者类型）
  - 实现 `require_approval` 配置下的操作拦截与审批请求生成
  - 实现审批请求的创建与追踪（issue_service 集成点）

---

## 10. 集成测试与模块组装 实现任务

### 阶段一：基础架构

- [x] **定义模块组装入口**
  - 定义 `PipelineAdapterModule` 结构体，聚合所有子模块（services, registry, routes, config）
  - 实现 `PipelineAdapterModule::new()` 依赖注入初始化
  - 实现 `PipelineAdapterModule::routes()` 返回配置好的 axum Router

- [x] **定义测试工具函数**
  - 实现 `create_test_pipeline()` 工具函数（创建测试用 Pipeline + Stages + Transitions）
  - 实现 `create_test_case()` 工具函数（创建测试用 Case）
  - 实现 `create_test_adapter()` 工具函数（创建 mock ServerAdapter）

- [x] **定义集成测试数据库 fixture**
  - 实现 test container PostgreSQL 启动与 migration
  - 实现测试数据种子（公司、用户、项目基础数据）
  - 实现测试结束后的数据清理策略

### 阶段二：核心功能

- [x] **实现 Pipeline 完整生命周期集成测试**
  - 测试：创建 Pipeline -> 创建 Case -> Agent 执行工作 -> Case 推进 -> Case 审批 -> 到达终态
  - 测试：转换规则校验（非法推进返回 400）
  - 测试：乐观并发控制（并发修改冲突）

- [x] **实现 Adapter 注册与执行集成测试**
  - 测试：内置 Adapter 注册与查找
  - 测试：外部 Adapter 安装 -> 注册 -> 覆盖内置 -> 暂停覆盖 -> 恢复覆盖 -> 卸载
  - 测试：本地 Adapter 执行（spawn 子进程 + stdout/stderr 回调）

- [x] **实现权限集成测试**
  - 测试：无权限用户创建 Pipeline 返回 403
  - 测试：非管理员安装 Adapter 返回 403
  - 测试：公司级别隔离（A 公司无法操作 B 公司的 Pipeline/Case）

### 阶段三：高级特性

- [x] **实现端到端 Pipeline + Adapter 联合测试**
  - 测试：Pipeline 自动化配置 -> Agent 执行 -> Adapter 调用 -> 结果回写
  - 测试：案例分解（breakdown）-> 子案例创建 -> 子案例推进 -> 父案例自动推进
  - 测试：模型 Profile 优先级解析（issue 级 > context 级 > adapter 默认级）

- [x] **实现性能与并发测试**
  - 测试：高并发 Case 创建与推进（1000+ 并发请求）
  - 测试：Adapter Registry 并发注册/查找（线程安全性验证）
  - 测试：Pipeline 级联创建事务性能（含 50+ stages 的 Pipeline 创建）

---

*文档生成时间: 2026-07-11*
*基于 Paperclip commit: 1f0769018*
