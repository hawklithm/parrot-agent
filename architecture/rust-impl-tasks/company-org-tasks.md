# Company/Org 组织模块 - Rust 实现任务拆解

> 基于文档: `doc/architecture/backend/company-org.md`
> 版本: 1.0
> 日期: 2026/07/11

---

## 阶段一：基础架构

### 1.1 数据库 Schema 与 Migration

- [ ] **定义 companies 表 schema**
  - 创建 `src/db/schema/companies.rs`，使用 `sqlx` 或 `diesel` 定义 `companies` 表结构（id, name, description, status, pause_reason, paused_at, issue_prefix, issue_counter, budget_monthly_cents, spent_monthly_cents, attachment_max_bytes, default_responsible_user_id, require_board_approval_for_new_agents, feedback_data_sharing_enabled, feedback_data_sharing_consent_at, feedback_data_sharing_consent_by_user_id, feedback_data_sharing_terms_version, brand_color, logo_asset_id, created_at, updated_at）
  - 定义 `CompanyStatus` 枚举（Active, Paused, Archived）并实现 `FromStr`/`Display`
  - 编写 migration SQL（`CREATE TABLE companies ...`）

- [ ] **定义 company_memberships 表 schema**
  - 创建 `src/db/schema/company_memberships.rs`，定义表结构（id, company_id, principal_type, principal_id, status, membership_role, created_at, updated_at）
  - 定义 `PrincipalType` 枚举（User, Agent）和 `MembershipRole` 枚举（Owner, Member）
  - 编写 migration SQL，添加外键约束 `company_id -> companies.id`

- [ ] **定义 projects 表 schema**
  - 创建 `src/db/schema/projects.rs`，定义表结构（id, company_id, goal_id, name, description, status, lead_agent_id, target_date, color, icon, env, pause_reason, paused_at, execution_workspace_policy, archived_at, created_at, updated_at）
  - 定义 `ProjectStatus` 枚举（Backlog, Todo, InProgress, InReview, Blocked, Done）
  - 编写 migration SQL，添加外键 `company_id -> companies.id`、`goal_id -> goals.id`（nullable）

- [ ] **定义 project_workspaces 表 schema**
  - 创建 `src/db/schema/project_workspaces.rs`，定义表结构（id, project_id, name, config, is_primary, created_at, updated_at）
  - 编写 migration SQL，添加外键 `project_id -> projects.id`
  - 实现 `Project` 与 `ProjectWorkspace` 的关联查询 trait

- [ ] **定义 resource_memberships 表 schema**
  - 创建 `src/db/schema/project_memberships.rs`（id, company_id, project_id, user_id, state, starred_at, created_at, updated_at）
  - 创建 `src/db/schema/agent_memberships.rs`（id, company_id, agent_id, user_id, state, starred_at, created_at, updated_at）
  - 定义 `MembershipState` 枚举（Joined, Left），编写 migration SQL

### 1.2 核心类型定义

- [ ] **定义 Company 领域模型**
  - 创建 `src/domain/company/mod.rs`，定义 `Company` struct 及其 builder
  - 定义 `CreateCompanyInput` 和 `UpdateCompanyInput` struct
  - 实现 `Company` 的 `TryFrom<Row>` 转换（数据库行到领域模型）

- [ ] **定义 Project 领域模型**
  - 创建 `src/domain/project/mod.rs`，定义 `Project`、`ProjectWorkspace` struct
  - 定义 `CreateProjectInput`、`UpdateProjectInput`、`CreateWorkspaceInput` struct
  - 定义 `ProjectGoalRef`、`ProjectExecutionWorkspacePolicy` 辅助类型

- [ ] **定义 ResourceMemberships 领域模型**
  - 创建 `src/domain/resource_membership/mod.rs`，定义 `ResourceMemberships` struct（含 project_memberships, agent_memberships, starred_project_ids, starred_agent_ids, project_starred_at, agent_starred_at）
  - 定义 `MembershipUpdateResult` struct（changed, change_kind）
  - 定义 `PolicyDecision` struct（allowed, reason, source）

- [ ] **定义共享错误类型**
  - 创建 `src/error/mod.rs`，定义 `AppError` enum（NotFound, Forbidden, Conflict, BadRequest, Internal）
  - 实现 `IntoResponse` trait 将 `AppError` 转换为 HTTP 响应
  - 定义 `Result<T> = std::result::Result<T, AppError>` 类型别名

### 1.3 数据库连接与 Repository 层

- [ ] **建立数据库连接池**
  - 创建 `src/db/mod.rs`，使用 `sqlx::PgPool` 建立连接池
  - 实现 `AppState` struct 持有 `PgPool` 和配置
  - 编写连接池初始化函数 `init_db_pool(database_url: &str) -> PgPool`

- [ ] **实现 CompanyRepository**
  - 创建 `src/db/repository/company_repo.rs`，实现 `CompanyRepository` struct
  - 实现 `create()`、`get_by_id()`、`list()`、`update()`、`delete()` 方法
  - 实现 `get_stats()` 聚合查询方法

- [ ] **实现 ProjectRepository**
  - 创建 `src/db/repository/project_repo.rs`，实现 `ProjectRepository` struct
  - 实现 `create()`、`get_by_id()`、`list_by_company()`、`update()`、`delete()` 方法
  - 实现 workspace 的 CRUD 方法（create_workspace, list_workspaces, update_workspace, delete_workspace）

- [ ] **实现 ResourceMembershipRepository**
  - 创建 `src/db/repository/membership_repo.rs`，实现 `ResourceMembershipRepository` struct
  - 实现 `list_for_user()` 方法（联表查询 project_memberships + projects，agent_memberships + agents）
  - 实现 `upsert_project_membership()` 和 `upsert_agent_membership()` 方法（ON CONFLICT DO UPDATE）

---

## 阶段二：核心功能

### 2.1 Company Service

- [ ] **实现 CompanyService 基础 CRUD**
  - 创建 `src/service/company_service.rs`，定义 `CompanyService` struct（持有 `CompanyRepository`）
  - 实现 `create()` 方法：插入 companies 记录
  - 实现 `get_by_id()`、`list()`、`update()`、`delete()` 方法

- [ ] **实现 Company 创建完整流程**
  - 在 `create()` 中调用 `AccessService.ensure_membership()` 创建 owner 成员关系
  - 调用 `AccessService.ensure_role_default_grants()` 授予默认权限
  - 当 budget_monthly_cents > 0 时，调用 `BudgetService.upsert_policy()` 创建预算策略

- [ ] **实现 Company 归档与品牌更新**
  - 实现 `archive()` 方法：将 status 设为 archived，设置 archived_at
  - 实现 `update_branding()` 方法：更新 brand_color 和 logo_asset_id
  - 实现 `get_timeline()` 和 `get_feedback_traces()` 查询方法

### 2.2 Access Service

- [ ] **实现 AccessService 权限决策**
  - 创建 `src/service/access_service.rs`，定义 `AccessService` struct
  - 实现 `decide()` 方法：根据 actor、action、resource 判断权限
  - 定义 `BoardActor` struct（user_id, principal_type, scopes）

- [ ] **实现 assert 辅助函数**
  - 实现 `assert_board()`：验证请求者具有 Board 权限
  - 实现 `assert_instance_admin()`：验证是否为实例管理员
  - 实现 `assert_company_access()`：验证用户对公司有访问权限

- [ ] **实现成员关系管理方法**
  - 实现 `ensure_membership()`：创建或确认成员关系
  - 实现 `ensure_role_default_grants()`：授予角色默认权限
  - 定义 `BoardUserId` 提取逻辑（`require_board_user_id()`）

### 2.3 Project Service

- [ ] **实现 ProjectService 基础 CRUD**
  - 创建 `src/service/project_service.rs`，定义 `ProjectService` struct（持有 `ProjectRepository`）
  - 实现 `create()`、`get_by_id()`、`list_by_company()`、`update()`、`delete()` 方法
  - 实现 `resolve_by_reference()` 短名称解析方法

- [ ] **实现 Project 创建完整流程**
  - 在路由层调用 `assert_company_access()` 权限校验
  - 调用 `SecretService.normalize_env_bindings_for_persistence()` 处理环境变量
  - 创建项目后，可选创建 workspace，再调用 `SecretService.sync_env_bindings_for_target()`

- [ ] **实现 Workspace 运行时控制**
  - 实现 `runtime_service_action()` 方法：控制工作区运行时服务（start/stop/restart）
  - 实现 `runtime_command_action()` 方法：在工作区执行运行时命令
  - 定义 `RuntimeServiceAction` 和 `RuntimeCommandAction` 枚举

### 2.4 Resource Membership Service

- [ ] **实现 ResourceMembershipService 查询**
  - 创建 `src/service/membership_service.rs`，定义 `ResourceMembershipService` struct
  - 实现 `list_for_user()` 方法：查询用户的项目和 Agent 成员关系，构建 `ResourceMemberships` 响应
  - 过滤已归档项目和已终止 Agent

- [ ] **实现 ResourceMembershipService 更新**
  - 实现 `update_project()` 方法：assert_mutation_allowed -> policy_hook 评估 -> upsert
  - 实现 `update_agent()` 方法：同上逻辑但针对 agent_memberships
  - 实现状态变更检测和活动日志记录

- [ ] **实现 Policy Hook 机制**
  - 定义 `ResourceMembershipPolicyHook` trait（`fn evaluate(...) -> PolicyDecision`）
  - 在 `ResourceMembershipService` 中持有 `Option<Box<dyn ResourceMembershipPolicyHook>>`
  - 实现 `assert_mutation_allowed()` 和 `assert_board_self_membership_access()` 方法

---

## 阶段三：高级特性

### 3.1 Company Skills Service

- [ ] **实现 CompanySkillService 基础 CRUD**
  - 创建 `src/db/schema/company_skills.rs`，定义表结构和 migration
  - 创建 `src/service/company_skill_service.rs`，实现 `create()`、`get()`、`list()`、`update()`、`delete()` 方法
  - 定义 `CompanySkill` struct 和相关输入类型

- [ ] **实现 Skill 版本与测试体系**
  - 定义 `company_skill_versions` 表 schema 及 migration
  - 实现 `create_version()`、`list_versions()`、`get_version()` 方法
  - 实现测试输入（test_inputs）和测试运行（test_runs）的 CRUD

- [ ] **实现 Skill 收藏与同意门**
  - 实现 `star()` / `unstar()` 收藏功能
  - 集成 `ChangeConsentGateService`：对 Agent 操作检查同意门状态
  - 实现 `fork_precheck()` 方法：检查 fork 前置条件
  - 实现测试运行模板（skill_test_run_templates）的 CRUD

### 3.2 Activity Log Service

- [ ] **实现 ActivityLogService**
  - 创建 `src/service/activity_log_service.rs`，定义 `ActivityLogService` struct
  - 定义 `ActivityEvent` enum（CompanyCreated, ProjectCreated, ResourceMembershipStarred, SkillCreated 等）
  - 实现 `log_activity()` 方法：将活动事件持久化到 activity_logs 表

- [ ] **实现活动日志查询**
  - 实现 `get_company_timeline()` 方法：按时间倒序查询公司活动
  - 定义 `ActivityLogEntry` struct（event_type, actor, resource_type, resource_id, metadata, created_at）
  - 支持分页和事件类型过滤

- [ ] **集成活动日志到各 Service**
  - 在 CompanyService.create() 末尾记录 `company.created` 事件
  - 在 ProjectService.create() 末尾记录 `project.created` 事件
  - 在 ResourceMembershipService 更新方法中记录 `resource_membership.starred` 事件

### 3.3 Company Portability (导入导出)

- [ ] **实现 CompanyPortabilityService 导出**
  - 创建 `src/service/company_portability_service.rs`
  - 实现 `preview_export()` 方法：预览导出内容（不执行）
  - 实现 `execute_export()` 方法：序列化公司数据为 JSON

- [ ] **实现 CompanyPortabilityService 导入**
  - 实现 `preview_import()` 方法：预览导入内容，检测冲突
  - 实现 `apply_import()` 方法：安全导入，使用幂等性密钥（idempotency_key = content_hash）
  - 定义 `ImportJob` struct 和状态跟踪（pending, processing, completed, failed）

- [ ] **实现导入任务管理**
  - 定义 `import_jobs` 表 schema 及 migration
  - 实现 `get_import_job()` 方法：查询导入任务状态
  - 实现公共导入路由（不依赖特定 companyId 的导入）

### 3.4 Budget Service

- [ ] **实现 BudgetService**
  - 创建 `src/db/schema/budget_policies.rs`，定义表结构和 migration
  - 创建 `src/service/budget_service.rs`，实现 `upsert_policy()` 方法
  - 定义 `BudgetPolicy` struct（company_id, monthly_cents, alert_thresholds）

- [ ] **实现预算检查与更新**
  - 实现 `check_budget()` 方法：检查月度预算是否超限
  - 实现 `record_spend()` 方法：更新 spent_monthly_cents
  - 定义 `BudgetStatus` enum（WithinBudget, ApproachingLimit, Exceeded）

- [ ] **集成预算到 Company 流程**
  - 在 Company 创建时，budget_monthly_cents > 0 则自动创建预算策略
  - 在 Company 更新时，同步更新预算策略
  - 定义预算超限的错误类型和告警机制

### 3.5 Cloud Upstreams Service

- [ ] **实现 CloudUpstreamService 连接初始化**
  - 创建 `src/db/schema/cloud_upstream_connections.rs`，定义表结构和 migration
  - 创建 `src/service/cloud_upstream_service.rs`
  - 实现 `start_connect()` 方法：fetch_discovery -> 生成 PKCE 密钥对 -> 创建 pending 连接 -> 构建 authorization_url

- [ ] **实现 PKCE OAuth 流程**
  - 实现 PKCE 工具函数：`generate_code_verifier()`、`compute_code_challenge()`（SHA256 + Base64URL）
  - 实现 `finish_connect()` 方法：验证 state -> 用 code + code_verifier 交换 token -> 更新连接状态
  - 定义 `CloudUpstreamConnection` struct 和 `ConnectionStatus` enum（Pending, Connected, Failed）

- [ ] **实现推送运行管理**
  - 实现 `preview_push_run()` 方法：预览推送内容
  - 实现 `create_push_run()` / `cancel_push_run()` / `activate_entity()` 方法
  - 定义 `push_runs` 表 schema、`PushRun` struct 和 `PushRunStatus` enum

---

## 阶段四：API 路由层

### 4.1 Companies 路由

- [ ] **实现 Companies 基础路由**
  - 创建 `src/route/companies.rs`，使用 `axum::Router` 定义路由组
  - 实现 `GET /` (list)、`GET /:company_id` (get)、`POST /` (create)
  - 实现 `PATCH /:company_id` (update)、`DELETE /:company_id` (delete)

- [ ] **实现 Companies 辅助路由**
  - 实现 `GET /stats`、`GET /:company_id/artifacts`、`GET /:company_id/timeline`、`GET /:company_id/feedback-traces`
  - 实现 `PATCH /:company_id/branding` (更新品牌)
  - 实现 `POST /:company_id/archive` (归档)

- [ ] **实现 Companies 导入导出路由**
  - 实现 `POST /:company_id/export`、`POST /:company_id/exports/preview`、`POST /:company_id/exports`
  - 实现 `POST /:company_id/imports/preview`、`POST /:company_id/imports/apply`
  - 实现公共导入路由 `GET /import/jobs/:job_id`、`POST /import/preview`、`POST /import`

### 4.2 Projects 路由

- [ ] **实现 Projects 基础路由**
  - 创建 `src/route/projects.rs`，定义路由组
  - 实现 `GET /companies/:company_id/projects` (列表)、`GET /projects/:id` (详情)
  - 实现 `POST /companies/:company_id/projects` (创建)、`PATCH /projects/:id` (更新)、`DELETE /projects/:id` (删除)

- [ ] **实现 Projects Workspace 路由**
  - 实现 `GET /projects/:id/workspaces`、`POST /projects/:id/workspaces`
  - 实现 `PATCH /projects/:id/workspaces/:workspace_id`、`DELETE /projects/:id/workspaces/:workspace_id`
  - 实现 `GET /projects/:id/external-object-summary`

- [ ] **实现 Projects 运行时控制路由**
  - 实现 `POST /projects/:id/workspaces/:workspace_id/runtime-services/:action`
  - 实现 `POST /projects/:id/workspaces/:workspace_id/runtime-commands/:action`
  - 定义 `RuntimeServiceAction` 和 `RuntimeCommandAction` 路径参数提取器

### 4.3 Resource Memberships 路由

- [ ] **实现 Resource Memberships 路由**
  - 创建 `src/route/resource_memberships.rs`，定义路由组
  - 实现 `GET /companies/:company_id/resource-memberships/me`
  - 实现 `PUT /companies/:company_id/resource-memberships/me/projects/:project_id`
  - 实现 `PUT /companies/:company_id/resource-memberships/me/agents/:agent_id`

### 4.4 Company Skills 路由

- [ ] **实现 Company Skills 基础路由**
  - 创建 `src/route/company_skills.rs`，定义路由组
  - 实现 `GET /skills/catalog`、`GET /skills/catalog/:catalog_id`、`GET /skills/catalog/:catalog_id/files`
  - 实现 `GET /companies/:company_id/skills`、`GET /companies/:company_id/skills/categories`、`GET /companies/:company_id/skills/:skill_id`

- [ ] **实现 Company Skills 写入路由**
  - 实现 `POST /companies/:company_id/skills` (创建)、`PATCH /companies/:company_id/skills/:skill_id` (更新)、`DELETE /companies/:company_id/skills/:skill_id` (删除)
  - 实现 `POST /companies/:company_id/skills/:skill_id/star`、`DELETE /companies/:company_id/skills/:skill_id/star`
  - 实现 `GET /companies/:company_id/skills/:skill_id/fork-precheck`

- [ ] **实现 Company Skills 版本与测试路由**
  - 实现 `POST /companies/:company_id/skills/:skill_id/versions`、`GET /companies/:company_id/skills/:skill_id/versions`
  - 实现 test_inputs CRUD（`GET/POST/PATCH/DELETE .../test-inputs/...`）
  - 实现 test_runs CRUD + cancel（`GET/POST/DELETE .../test-runs/...`、`POST .../test-runs/:run_id/cancel`）

### 4.5 Cloud Upstreams 路由

- [ ] **实现 Cloud Upstreams 路由**
  - 创建 `src/route/cloud_upstreams.rs`，定义路由组
  - 实现 `GET /cloud-upstreams` (列表)
  - 实现 `POST /cloud-upstreams/connect/start`、`POST /cloud-upstreams/connect/finish`
  - 实现 `POST /cloud-upstreams/:connection_id/push-runs/preview`、`POST /cloud-upstreams/:connection_id/push-runs`

- [ ] **实现 Cloud Upstreams 推送运行路由**
  - 实现 `GET /cloud-upstreams/:connection_id/push-runs/:run_id`
  - 实现 `POST /cloud-upstreams/:connection_id/push-runs/:run_id/cancel`
  - 实现 `POST /cloud-upstreams/:connection_id/push-runs/:run_id/activation`

### 4.6 Org Chart SVG 路由

- [ ] **定义 Org Chart 数据模型**
  - 创建 `src/domain/org_chart/mod.rs`
  - 定义 `OrgNode` struct（id, name, role, status, reports: Vec<OrgNode>, collapsed_reports: Option<Vec<OrgNode>>）
  - 定义 `AgentHierarchyQuery` struct（用于查询 Agent 层级关系）

- [ ] **实现 Agent 层级树构建逻辑**
  - 实现 `build_org_tree(agents: Vec<Agent>) -> OrgNode` 函数
  - 处理 Agent 的 `reports_to` 字段构建父子关系
  - 实现循环引用检测（防止死循环）
  - 实现大型组织的折叠逻辑（超过 N 个下属时折叠）

- [ ] **实现 SVG 渲染引擎**
  - 使用 `svg` crate 或手动生成 SVG XML
  - 实现节点位置计算算法（树形布局）
  - 实现节点样式渲染（矩形框、文本、连接线）
  - 支持颜色主题配置（基于 company.brand_color）

- [ ] **实现 Org Chart SVG 路由**
  - 创建 `src/route/org_chart_svg.rs`
  - 实现 `GET /companies/:company_id/org-chart.svg`
  - 查询公司所有 Active Agent -> 构建 OrgNode 树 -> 渲染 SVG -> 返回 image/svg+xml 响应
  - 添加缓存控制头（Cache-Control: public, max-age=300）

- [ ] **实现 Company Stats 详细统计**
  - 扩展 `GET /stats` 端点实现
  - 实现 agent 数量统计（按角色分组：CEO/VP/Manager/Researcher/General）
  - 实现 project 数量统计（按状态分组）
  - 实现预算使用率统计（spent_monthly_cents / budget_monthly_cents）
  - 实现成本趋势统计（最近 30 天每日成本）
  - 添加 Redis 缓存机制（TTL: 5 分钟）

---

## 阶段五：集成与中间件

### 5.1 认证与授权中间件

- [ ] **实现 Board 认证中间件**
  - 创建 `src/middleware/auth.rs`，实现 `BoardAuth` axum middleware
  - 从请求头提取 Board user ID 和 token
  - 注入 `BoardActor` 到请求扩展中

- [ ] **实现权限守卫 extractors**
  - 实现 `RequireBoardUserId` axum extractor：从请求提取并验证 Board 用户
  - 实现 `AssertCompanyAccess` extractor：验证用户对公司的访问权限
  - 实现 `AssertBoardOrgAccess` extractor：验证 Board 组织访问权限

- [ ] **实现 SecretService 集成**
  - 创建 `src/service/secret_service.rs`
  - 实现 `normalize_env_bindings_for_persistence()` 方法
  - 实现 `sync_env_bindings_for_target()` 方法

### 5.2 路由组装与应用入口

- [ ] **组装主应用路由**
  - 创建 `src/route/mod.rs`，汇总所有子路由
  - 挂载 `/api/companies`、`/api/projects`、`/api/skills`、`/api/cloud-upstreams` 等路由组
  - 配置全局中间件（CORS、日志、错误处理）

- [ ] **实现应用入口与配置**
  - 创建 `src/main.rs`，初始化 PgPool、各 Service、Router
  - 定义 `Config` struct（database_url, listen_addr, experimental_features 等）
  - 使用 `tokio::main` 启动 axum 服务器

- [ ] **实现 ChangeConsentGateService**
  - 创建 `src/service/change_consent_gate_service.rs`
  - 实现 `assert_consented()` 方法：检查 Agent 对指定操作的同意状态
  - 定义 `consent_gates` 表 schema 及 migration

### 5.3 Instance Settings Service

- [ ] **实现 InstanceSettingsService**
  - 创建 `src/service/instance_settings_service.rs`
  - 实现 `get_experimental()` 方法：获取实验性功能开关
  - 定义 `ExperimentalFeatures` struct（cloud_upstreams_enabled 等）

- [ ] **实现功能开关集成**
  - 在 Cloud Upstreams 路由中检查 `cloud_upstreams_enabled` 开关
  - 定义 `FeatureFlag` enum 和统一的开关检查方法
  - 未启用时返回 404 Not Found

---

## 阶段六：测试与文档

### 6.1 单元测试

- [ ] **Service 层单元测试**
  - 为 `CompanyService` 编写创建、更新、归档的单元测试
  - 为 `ProjectService` 编写创建、短名称解析的单元测试
  - 为 `ResourceMembershipService` 编写 upsert 和 policy_hook 的单元测试

- [ ] **Repository 层单元测试**
  - 为 `CompanyRepository` 编写 CRUD 测试（使用 testcontainers 或 SQLite mock）
  - 为 `ProjectRepository` 编写含 workspace 关联的测试
  - 为 `MembershipRepository` 编写联表查询和 upsert 测试

- [ ] **Policy Hook 单元测试**
  - 为 `ResourceMembershipPolicyHook` 编写允许/拒绝场景测试
  - 为 `assert_mutation_allowed` 编写边界条件测试
  - 为 `ChangeConsentGateService` 编写同意/拒绝场景测试

### 6.2 集成测试

- [ ] **Company API 集成测试**
  - 编写创建公司 -> 查询 -> 更新 -> 归档的完整流程测试
  - 编写导入导出的集成测试
  - 编写权限校验的 403 场景测试

- [ ] **Project API 集成测试**
  - 编写创建项目 -> 创建 workspace -> 运行时控制的完整流程测试
  - 编写环境变量加密和同步的测试
  - 编写项目引用解析（短名称 vs UUID）的测试

- [ ] **Resource Membership API 集成测试**
  - 编写加入/离开/收藏项目和 Agent 的完整流程测试
  - 编写 PKCE OAuth 连接流程的端到端测试
  - 编写预算策略创建和超限的集成测试

---

## 任务依赖关系图

```
1.1 Schema ──> 1.2 Types ──> 1.3 Repository ──> 2.1-2.4 Services
                                                       │
                                                       v
                                           4.1-4.6 Routes ──> 5.1 Middleware
                                                       │              │
                                                       v              v
                                                 5.2 Assembly ──> 5.3 Settings
                                                       │
                                                       v
                                                  6.1-6.2 Tests

3.1-3.5 Advanced Features (可与阶段四并行，依赖阶段二的 Service)
```

## 关键技术选型建议

| 领域 | 建议方案 |
|------|---------|
| Web 框架 | `axum` 0.7+ |
| 数据库 | `sqlx` 0.7+ (async PostgreSQL) |
| Migration | `sqlx-cli` 或 `refinery` |
| 序列化 | `serde` + `serde_json` |
| UUID | `uuid` crate (v7) |
| 时间 | `chrono` 或 `time` |
| PKCE | `sha2` + `base64` + `rand` |
| SVG 生成 | 手写 XML 或 `svg` crate |
| 测试 | `tokio::test` + `testcontainers` |
| 错误处理 | `thiserror` + `anyhow`（内部用 anyhow，API 边界用 thiserror） |
