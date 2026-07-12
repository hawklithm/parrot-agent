# Agent 管理模块 - Rust 实现任务拆解

> 基于 [backend/agent-management.md](../backend/agent-management.md) 架构分析文档，拆解为 Rust 版本实现任务。

---

## 1. 数据模型层 实现任务

### 阶段一：基础架构
- [x] **定义 Agent 核心枚举类型**
  - 定义 `AgentStatus` 枚举（idle / running / paused / pending_approval / terminated）
  - 定义 `AgentRole` 枚举（ceo / vp / manager / researcher / general）
  - 定义 `TrustPreset` 与 `TrustAuthorizationPolicy` 枚举

- [x] **定义 Agent 状态机与转换规则**
  - 定义 `AgentStateMachine` 结构体，包含所有合法状态转换
  - 实现状态转换触发器定义：idle->running（任务分配）、running->paused（心跳超时/预算耗尽/手动暂停）、pending_approval->idle（审批通过）
  - 实现状态转换验证器：检查 current_status -> target_status 是否合法
  - 实现状态转换权限校验（某些转换需特定角色）

- [x] **定义 Agent 结构体与权限模型**
  - 定义 `Agent` 结构体，映射表字段（id, company_id, name, role, status, adapter_type, adapter_config 等）
  - 定义 `AgentPermissions` 结构体（can_create_agents, can_create_skills, trust_preset, authorization_policy）
  - 定义 `AgentMetadata` 结构体（内置 Agent 标记等）

- [x] **定义配置版本与审批数据模型**
  - 定义 `AgentConfigRevision` 结构体（id, agent_id, snapshot, created_at）
  - 定义 `Approval` 结构体（id, agent_id, status, requested_by, created_at）
  - 定义 `CostEvent` 结构体用于花费计算

### 阶段二：核心功能
- [x] **实现 Database Schema 迁移**
  - 使用 diesel / sea-orm / sqlx 定义 agents 表 migration
  - 定义 agent_config_revisions 表 migration
  - 定义 approvals 与 cost_events 表 migration

- [x] **实现 CRUD Repository trait**
  - 定义 `AgentRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 实现 PostgreSQL 版本的 `AgentRepository`
  - 编写 Repository 单元测试（使用 test container 或 mock）

### 阶段三：高级特性
- [x] **实现 JSONB 字段类型安全映射**
  - 为 `adapter_config: Jsonb` 实现 Rust 类型安全的序列化/反序列化
  - 为 `runtime_config: Jsonb` 实现 Rust 类型安全映射
  - 为 `permissions: Jsonb` / `metadata: Jsonb` 实现类型安全映射

---

## 2. 适配器模式层 实现任务

### 阶段一：基础架构
- [x] **定义 ServerAdapter trait**
  - 定义 `ServerAdapterModule` trait（type, label, models, list_models, test_environment, supports_instructions_bundle）
  - 定义 `AdapterType` 枚举（claude_local / cursor / opencode / process / codex_local 等）
  - 定义 `TestEnvironmentResult` 结构体

- [x] **实现 Adapter Registry**
  - 实现 `AdapterRegistry` 结构体，持有 `HashMap<AdapterType, Arc<dyn ServerAdapterModule>>`
  - 实现 `register()` 与 `find_server_adapter()` 方法
  - 实现 `list_all()` 方法用于列举已注册适配器

### 阶段二：核心功能
- [x] **实现 Process 适配器（默认）**
  - 实现 `ProcessAdapter`，作为最基础的本地进程适配器
  - 实现 `list_models()` 返回空列表或本地可用模型
  - 实现 `test_environment()` 返回基础连通性测试
  - 定义 EnvironmentRuntimeService 接口（acquire_run_lease、release_run_lease、realize_workspace）
  - 实现 testEnvironment 中的环境租约获取与释放集成
  - 实现 testEnvironment 失败时的租约清理逻辑（确保资源释放）

- [x] **实现 Claude Local 适配器**
  - 实现 `ClaudeLocalAdapter`，对接 Claude Code CLI
  - 实现 `list_models()` 获取 Claude 可用模型列表
  - 实现 `test_environment()` 验证 API Key 与连通性

### 阶段三：高级特性
- [x] **实现适配器配置标准化**
  - 实现 `normalize_adapter_config_for_persistence()` 统一敏感配置处理
  - 实现 `normalize_runtime_config_adapter_configs_for_persistence()`
  - 实现 `apply_create_defaults_by_adapter_type()` 按适配器类型填充默认配置

---

## 3. 权限与访问控制层 实现任务

### 阶段一：基础架构
- [x] **定义 ABAC 权限模型**
  - 定义 `Action` 枚举（agents:create / agent:read / agent_config:update / agent_config:read / tasks:assign）
  - 定义 `AccessDecision` 结构体（allowed, reason）
  - 定义 `Actor` trait（company_id, is_agent, agent_id, permissions）

- [x] **实现 AccessService 核心**
  - 定义 `AccessService` trait 与 `decide(action, actor, resource)` 方法
  - 实现基于角色的基础判断逻辑（公司访问 / 同公司 Agent 验证）
  - 实现 `assert_company_access()` 与 `assert_agent_read_allowed()`

### 阶段二：核心功能
- [x] **实现 Agent 级权限断言**
  - 实现 `assert_can_create_agents_for_company()`（公司访问 + agents:create 权限 + Agent 同公司校验）
  - 实现 `assert_can_update_agent()`（agent_config:update + change grant + consent 校验）
  - 实现 `assert_can_read_configurations()`（agent_config:read 权限校验）

- [x] **实现内置 Agent 权限断言**
  - 实现 `assert_can_provision_built_in_agents()`
  - 实现 `assert_can_control_built_in_routine()`
  - 实现 `assert_built_in_agents_enabled()`（实验特性开关校验）

### 阶段三：高级特性
- [x] **实现权限过滤与脱敏**
  - 实现 `filter_agents_for_actor()`（批量权限判定 + 过滤）
  - 实现 `redact_for_restricted_agent_view()`（移除 adapter_config / runtime_config）
  - 实现 `redact_event_payload()`（通用敏感信息脱敏）

---

## 4. Agent CRUD 服务层 实现任务

### 阶段一：基础架构
- [x] **定义 AgentService trait**
  - 定义 `AgentService` trait（create, get_by_id, list, update, delete）
  - 定义 `CreateAgentInput` 与 `UpdateAgentInput` 结构体
  - 定义 `NormalizedAgentRow` 结构体（含 org_chain_health 与 spend 信息）

- [x] **实现 Agent 查询方法**
  - 实现 `get_by_id()` 从数据库查询单个 Agent
  - 实现 `list()` 按 company_id 查询 Agent 列表
  - 实现 `get_me()` 通过 Agent Key 认证获取当前 Agent 信息

### 阶段二：核心功能
- [x] **实现 Agent 创建（雇佣）**
  - 实现 `create()` 插入 Agent 记录
  - 实现 Company/Project 存在性验证（company_id、project_id 外键校验）
  - 实现 `materialize_default_instructions_bundle_for_new_agent()` 指令集物化
  - 实现 Agent 创建事务回滚逻辑（指令物化失败 -> 删除已创建的 Agent 和 Approval）
  - 实现审批流程分支（pending_approval 状态 + Approval 记录创建 + 返回 202）

- [x] **实现 Agent 更新**
  - 实现 `update()` 更新 Agent 字段
  - 实现 terminated 状态不可恢复校验
  - 实现 pending_approval 配置冻结校验
  - 实现配置更新乐观锁（基于 updated_at 或 version 字段防止并发冲突）
  - 实现 Agent 更新 + ConfigRevision 创建原子事务封装

- [x] **实现组织架构循环检测**
  - 实现 `detect_reporting_cycle()` 检测 reportsTo 循环
  - 实现 reportsTo 变更时的循环验证逻辑
  - 实现 `get_agent_work_eligibility()` 计算 orgChainHealth（循环检测 + manager 有效性 + heartbeat 新鲜度 + budget 状态）
  - 实现 orgChainHealth 评分算法：missing_manager 扣分、stale_heartbeat 扣分、budget_overrun 扣分

### 阶段三：高级特性
- [x] **实现 Agent 花费计算**
  - 实现 `hydrate_agent_spend()` 按月聚合 costEvents
  - 实现 spent_monthly_cents 字段计算
  - 实现 budget_monthly_cents 预算校验逻辑
  - 定义 CostEventService 接口（create_cost_event、aggregate_by_agent、monthly_rollover）
  - 实现 Agent 执行时的自动成本记录钩子（未定义触发点需补充）

- [x] **实现 Agent 终止与重置**
  - 实现 `delete()` 设置 terminated 状态（软删除）
  - 实现 `reset_session()` 重置 Agent 会话运行时状态
  - 定义 SessionManagementService 接口（register_session、cleanup_session、get_session_state）
  - 实现 Agent 启动时的 session 注册集成
  - 实现 Agent 终止/重置时的 session 清理集成
  - 实现 `sync_skills()` 同步 Agent 技能列表
  - 定义 SkillService 接口（list_skills、bind_to_agent、materialize_skill）

---

## 5. 请求验证与路由层 实现任务

### 阶段一：基础架构
- [x] **定义验证 Schema**
  - 使用 garde / validator crate 定义 `CreateAgentHireSchema`
  - 定义 `UpdateAgentSchema` 验证规则
  - 定义 `TestAdapterEnvironmentSchema` 验证规则

- [x] **搭建路由框架**
  - 使用 axum 定义 Agent CRUD 路由组（Router<CompanyId, AgentId>）
  - 实现 `router.param("id")` 路径参数提取与 shortname-to-UUID 转换
  - 统一错误响应格式（AppError -> axum::Json）

### 阶段二：核心功能
- [x] **实现 Agent CRUD 路由端点**
  - 实现 `GET /companies/:companyId/agents` 列表查询（含权限过滤 + 脱敏）
  - 实现 `GET /agents/:id` 与 `GET /agents/me` 详情查询
  - 实现 `POST /companies/:companyId/agent-hires` 创建 Agent

- [x] **实现 Agent 更新与删除路由端点**
  - 实现 `PUT/PATCH /agents/:id` 更新 Agent
  - 实现 `DELETE /agents/:id` 终止 Agent
  - 实现 `POST /agents/:id/skills/sync` 同步技能

### 阶段三：高级特性
- [x] **实现配置查询与版本路由端点**
  - 实现 `GET /agents/:id/configuration` 获取脱敏配置
  - 实现 `GET /agents/:id/config-revisions` 与 `GET /agents/:id/config-revisions/:revisionId`
  - 实现 `GET /agents/:id/skills` 获取技能列表

---

## 6. 配置版本控制层 实现任务

### 阶段一：基础架构
- [x] **定义 ConfigRevision 数据结构与 trait**
  - 定义 `AgentConfigRevision` 结构体（id, agent_id, snapshot_json, created_at, created_by）
  - 定义 `ConfigRevisionRepository` trait（create, list_by_agent, get_by_id）
  - 定义配置快照序列化格式（选择性地存储变更字段 vs 全量快照）

### 阶段二：核心功能
- [x] **实现配置版本创建**
  - 实现 `create_config_revision()` 在 Agent 更新时自动创建快照
  - 实现配置字段变更检测（diff old vs new，仅在有变更时创建版本）
  - 实现敏感值脱敏存储（API Key 等替换为占位符）

- [x] **实现配置版本查询**
  - 实现 `list_config_revisions()` 按 agent_id 分页查询
  - 实现 `get_config_revision()` 按 revision_id 查询单个版本
  - 实现版本内容反序列化与返回

### 阶段三：高级特性
- [x] **实现配置回滚**
  - 实现 `rollback_config_revision()` 恢复到指定版本
  - 实现回滚前的校验（Agent 状态、权限）
  - 实现 `POST /agents/:id/config-revisions/:revisionId/rollback` 路由端点

---

## 7. 内置 Agent 服务层 实现任务

### 阶段一：基础架构
- [x] **定义内置 Agent 核心类型**
  - 定义 `BuiltInAgentKey` 枚举（列出所有内置 Agent 标识）
  - 定义 `BuiltInAgentStatus` 枚举（not_provisioned / needs_setup / ready / paused / pending_approval）
  - 定义 `BuiltInAgentDefinition` 结构体（key, name, adapter_type, instructions_bundle, skills, routines）

- [x] **实现内置 Agent 元数据注册**
  - 实现 `BuiltInAgentMetadataRegistry` 持有所有内置 Agent 定义映射
  - 实现 `get_definition(key)` 查找内置 Agent 定义
  - 实现 `list_definitions()` 列举所有可用的内置 Agent

### 阶段二：核心功能
- [x] **实现内置 Agent 初始化（Provision）**
  - 实现 `provision()` 查找定义 -> 创建/获取 Agent -> 绑定资源
  - 实现 `materialize_instructions()` 创建指令文件
  - 实现 `materialize_skill()` 创建/同步 Skill

- [x] **实现内置 Agent 状态管理**
  - 实现状态机推导逻辑（Agent 状态 + 审批状态 + 暂停状态 -> BuiltInAgentStatus）
  - 实现 `get_status()` 查询内置 Agent 当前状态
  - 实现 `reset()` 重置内置 Agent（清除资源 + 恢复初始状态）

### 阶段三：高级特性
- [x] **实现内置 Agent 资源协调**
  - 实现 `reconcile()` 检测并修复资源漂移
  - 实现 `materialize_routine()` 创建/更新 Routine
  - 实现 Routine 的 enable / disable / manual run 控制

- [x] **实现内置 Agent 路由端点**
  - 实现 `GET /companies/:companyId/built-in-agents` 列表（含实验特性开关）
  - 实现 `POST /companies/:companyId/built-in-agents/:key/provision` 初始化
  - 实现 `POST /companies/:companyId/built-in-agents/:key/reconcile` 协调

---

## 8. 密钥与敏感信息管理 实现任务

### 阶段一：基础架构
- [x] **定义 SecretService trait**
  - 定义 `SecretService` trait（normalize_config, resolve_secret, redact_config）
  - 定义 `SecretReference` 结构体（引用外部密钥的标识）
  - 定义密钥存储后端接口（Vault / Env / DB-backed）

### 阶段二：核心功能
- [x] **实现密钥标准化与解析**
  - 实现 `normalize_adapter_config_for_persistence()` 规范化敏感配置
  - 实现 `resolve_secret_references()` 运行时解析 Secret 引用并注入环境变量
  - 实现 `sync_agent_secret_bindings()` 同步 Agent 与密钥的绑定关系

### 阶段三：高级特性
- [x] **实现 Codex Local 隔离**
  - 实现 `apply_codex_local_key_isolation()` 为 codex_local 适配器隔离环境
  - 实现环境变量作用域隔离
  - 实现隔离环境的清理逻辑

---

## 9. Adapter 信息路由层 实现任务

### 阶段一：基础架构
- [x] **定义 Adapter 路由与数据类型**
  - 定义 `ModelInfo` 结构体（id, label）
  - 定义 `ModelProfile` 结构体（模型配置信息）
  - 定义 `DetectModelResult` 结构体（检测可用模型结果）

### 阶段二：核心功能
- [x] **实现 Adapter 信息查询端点**
  - 实现 `GET /companies/:companyId/adapters/:type/models` 获取适配器支持的模型列表
  - 实现 `GET /companies/:companyId/adapters/:type/model-profiles` 获取模型配置
  - 实现 `GET /companies/:companyId/adapters/:type/detect-model` 检测可用模型

### 阶段三：高级特性
- [x] **实现 Adapter 环境测试端点**
  - 实现 `POST /companies/:companyId/adapters/:type/test-environment` 测试适配器环境
  - 实现运行时租约（acquire_run_lease / release_run_lease）
  - 实现工作区实例化（realize_workspace / resolve_environment_execution_target）

---

## 10. 组织架构与调度 实现任务

### 阶段一：基础架构
- [x] **定义组织架构数据类型**
  - 定义 `OrgNode` 结构体（agent_id, name, role, reports_to, children）
  - 定义 `OrgTree` 结构体（root_nodes, flatten_nodes）
  - 实现 OrgTree 构建逻辑（从 Agent 列表构建树形结构）

### 阶段二：核心功能
- [x] **实现组织架构查询端点**
  - 实现 `GET /companies/:companyId/org` 返回 JSON 组织架构树
  - 实现 `GET /companies/:companyId/org.svg` 生成 SVG 格式组织架构图
  - 实现 `GET /companies/:companyId/org.png` 生成 PNG 格式组织架构图

### 阶段三：高级特性
- [x] **实现实例调度心跳端点**
  - 实现 `GET /instance/scheduler-heartbeats` 获取调度心跳 Agent 列表
  - 实现 Instance Admin 权限校验
  - 实现心跳超时检测逻辑

---

## 依赖顺序总览

```
阶段一（基础架构）推荐实现顺序:

  1. 数据模型层 (枚举 + 结构体定义)
  2. 适配器模式层 (trait + registry)
  3. 权限与访问控制层 (trait + 基础断言)
  4. 密钥管理层 (trait + 接口)
  5. 请求验证与路由层 (schema + 框架)
  6. 配置版本控制层 (结构体 + trait)
  7. 内置 Agent 服务层 (类型 + 注册)
  8. 组织架构与调度 (数据类型)

阶段二（核心功能）推荐实现顺序:

  1. Agent CRUD 服务层 (查询 + 创建)
  2. Agent CRUD 服务层 (更新 + 循环检测)
  3. 请求验证与路由层 (CRUD 端点)
  4. 密钥管理层 (标准化 + 解析)
  5. 配置版本控制层 (创建 + 查询)
  6. 内置 Agent 服务层 (provision + 状态)
  7. Adapter 信息路由层 (查询端点)
  8. 组织架构查询端点

阶段三（高级特性）推荐实现顺序:

  1. 数据模型层 (JSONB 类型安全)
  2. 适配器模式层 (配置标准化)
  3. 权限与访问控制层 (过滤 + 脱敏)
  4. Agent CRUD 服务层 (花费 + 终止)
  5. 配置版本控制层 (回滚)
  6. 内置 Agent 服务层 (协调 + Routine)
  7. 密钥管理层 (Codex Local 隔离)
  8. Adapter 环境测试端点
  9. 实例调度心跳
```

---

## Rust 技术选型建议

| 领域 | 推荐选型 | 说明 |
|------|----------|------|
| Web 框架 | axum | 生态成熟，与 tower 中间件集成好 |
| ORM | sea-orm 或 sqlx | sea-orm 动态查询更强；sqlx 编译时 SQL 检查 |
| 请求验证 | garde 或 validator | garde 更现代，支持嵌套结构验证 |
| 错误处理 | thiserror + anyhow | thiserror 定义业务错误，anyhow 用于内部 |
| 序列化 | serde + serde_json | JSONB 字段统一用 serde 映射 |
| 异步运行时 | tokio | 标准选择 |
| 数据库迁移 | sea-orm-cli 或 sqlx-cli | 与 ORM 选择配套 |
| SVG 生成 | resvg 或 text-svg | 组织架构图渲染 |
| 测试 | testcontainers + mockall | 集成测试用 testcontainers，单测用 mockall |
