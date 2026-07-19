# Parrot Agent vs Paperclip 功能缺口分析与实施任务文档

> 对比基准：Paperclip `server/src/routes/`（Express，约 452 条唯一路由路径 / 547 条路由声明）
> 当前实现：Parrot Agent `crates/api/src/routes/`（Axum，约 161 条唯一路径 / 232 条路由声明）
> 已有 API_GAP_TASKS.md 覆盖 Controller 层接口缺失（约 345 个缺失接口），本文档从**功能实现**维度逐域分析，细化到具体改动点

---

## 一、概览

| 域 | Paperclip 路由数 | Parrot Agent 路由数 | 缺失数 | 严重程度 |
|---|:---:|:---:|:---:|:---:|
| Agents（智能体） | 56 | 37 | 19 | 🔴 P1 |
| Cases（案例） | ~40+ | 11 | ~30+ | 🔴 P1 |
| Issues（议题） | ~60+ | 20 | ~40+ | 🔴 P1 |
| Environments/Adapters（环境/适配器） | ~30 | 12 | ~18 | 🔴 P1 |
| Approvals（审批） | 10 | 0 | 10 | 🟠 P2 |
| Costs/Budgets（成本/预算） | 20 | 0 | 20 | 🟠 P2 |
| Executions/Runs（执行/运行） | 18 | 2 | 16 | 🟠 P2 |
| Skills（技能） | ~30 | 5 | ~25 | 🟠 P2 |
| Plugins（插件） | ~40 | 0 | ~40 | 🟠 P2 |
| Pipelines（管道） | ~20 | 8 | ~12 | 🟡 P3 |
| Goals/Routines（目标/例程） | ~20 | 12 | ~8 | 🟡 P3 |
| Companies（公司） | ~25 | 10 | ~15 | 🟡 P3 |
| Auth/Admin（认证/管理） | ~15 | 6 | ~9 | 🟡 P3 |
| Secrets/Providers（密钥/提供方） | ~25 | 12 | ~13 | 🟡 P3 |
| Activity/Dashboard（活动/仪表盘） | 7 | 0 | 7 | 🟢 P4 |
| Cloud Upstreams（云上游） | 8 | 0 | 8 | 🟢 P4 |
| Instance Settings（实例设置） | 8 | 0 | 8 | 🟢 P4 |
| File Resources（文件资源） | 3 | 0 | 3 | 🟢 P4 |
| Assets（资产） | 3 | 0 | 3 | 🟢 P4 |
| LLMs/OpenAPI | 5 | 0 | 5 | 🟢 P4 |
| Labels/Inbox/Sidebar | 6 | 0 | 6 | 🟢 P4 |
| Board Chat（看板对话） | 1 | 0 | 1 | 🟢 P4 |
| Resource Memberships（资源成员） | 3 | 2 | 1 | 🟢 P4 |
| **总计** | **~452** | **~161** | **~291** | |

---

## 二、P1 — 核心域子资源/状态机补齐

### 2.1 Agents（智能体）— 补齐生命周期/动作接口

#### 当前已实现
- GET /agents/:id, GET /agents/me, PATCH /agents/:id
- GET/POST/DELETE /agents/:id/keys, GET /agents/:id/keys/:key_id
- GET /agents/:id/instructions-bundle, GET/PUT/DELETE .../file
- GET /agents/:id/configuration, GET /agents/:id/skills
- GET /agents/:id/runtime-state, GET /agents/:id/task-sessions
- POST /agents/:id/pause, /resume, /clear-error, /approve, /terminate, /wakeup
- POST /agents/:id/runtime-state/reset-session
- POST /agents/:id/config-revisions/:revision_id/rollback
- POST /agents/:id/skills/sync
- PATCH /agents/:id/permissions, /instructions-path, /budgets
- GET /agents/me/inbox-lite, /inbox/mine

#### 缺失需补

**文件**: `crates/api/src/routes/agents.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| A1 | `POST /agents/:id/claude-login` | 实现 Claude 登录操作，调用 AgentService | AgentService |
| A2 | `POST /agents/:id/heartbeat/invoke` | 触发智能体心跳调用（已有 route? 检查是否仅缺 handler） | WatchdogService |
| A3 | `GET /agents/:id/config-revisions` | 列出配置版本（当前有 rollback 但无 list） | ConfigRevisionService |
| A4 | `GET /agents/:id/config-revisions/:revisionId` | 获取配置版本详情 | ConfigRevisionService |
| A5 | `GET /companies/:companyId/agent-configurations` | 公司级智能体配置列表 | AgentService |
| A6 | `GET /instance/scheduler-heartbeats` | 调度器心跳列表 | WatchdogService |

**Service 层需补齐**: `AgentService` trait 需添加 `claude_login()`、`heartbeat_invoke()` 等方法；`ConfigRevisionService` 需添加 `list_revisions()`、`get_revision()`。

---

### 2.2 Cases（案例）— 状态机/链接/文档/自动化

#### 当前已实现
- GET /api/cases/:id, PATCH /api/cases/:id, GET /api/cases/:id/detail
- POST /api/cases/:id/claim, /release, /transition
- GET /api/cases/:id/events, GET /api/cases/:id/documents
- GET/PUT /api/cases/:id/documents/:key
- POST /api/cases/:id/documents/:key/lock, /unlock
- POST /cases/:case_id/advance, POST /cases/:case_id/terminal
- GET /cases/:case_id, GET /cases/:case_id/events

#### 缺失需补

**文件**: `crates/api/src/routes/cases.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| C1 | `GET /cases/:id/children` | 获取子案例列表 | CaseService |
| C2 | `GET /cases/:id/children/tree` | 获取子案例树 | CaseService |
| C3 | `GET /cases/:id/rollup` | 获取案例汇总状态 | CaseService |
| C4 | `GET /cases/:id/context-pack` | 获取案例上下文包 | CaseService |
| C5 | `GET /cases/:id/outputs` | 获取案例产出物 | CaseService |
| C6 | `GET /cases/:id/issue-links` + `POST` + `DELETE /:linkId` | 案例-议题链接管理 | CaseIssueLinkService |
| C7 | `POST /cases/:id/links` | 创建链接（通用） | CaseService |
| C8 | `PUT /cases/:id/blockers` | 更新阻塞列表 | CaseService |
| C9 | `POST /cases/:id/suggest-transition` | 建议状态转换 | CaseService |
| C10 | `POST /cases/:id/resolve-suggestion` | 解决状态建议 | CaseService |
| C11 | `POST /cases/:id/review` | 发起审核 | CaseService |
| C12 | `POST /cases/:id/acknowledge-drift` | 确认偏移 | CaseService |
| C13 | `POST /cases/:id/open-conversation` | 打开对话 | CaseService |
| C14 | `POST /cases/:id/breakdown` | 分解案例 | CaseService |
| C15 | `POST /cases/:id/attachments` | 上传案例附件 | AttachmentService |
| C16 | `GET /cases/:id/documents/:key/revisions` | 文档版本历史 | DocumentService |
| C17 | `POST /cases/:id/documents/:key/revisions/:revisionId/restore` | 恢复文档版本 | DocumentService |
| C18 | `DELETE /cases/:id/documents/:key` | 删除文档 | DocumentService |
| C19 | `GET /cases/:id/documents/:key/annotations` (+thread, comments) | 文档注释系统 | AnnotationService |
| C20 | `POST /cases/:id/automation/retry` | 自动化重试 | CaseService |
| C21 | `POST /cases/:id/automation/retry-plan` | 重试计划 | CaseService |
| C22 | `POST /cases/:id/automation/current-stage/rerun` | 当前阶段重跑 | CaseService |
| C23 | `POST /cases/:id/automations/:automationId/retry` | 指定自动化重试 | CaseService |

**Service 层需补齐**: `CaseService` trait 大幅扩展（状态机动作、自动化、文档注释等）；新增 `CaseIssueLinkService`；`DocumentService` 添加版本管理。

---

### 2.3 Issues（议题）— 子资源补齐

#### 当前已实现
- GET /api/issues, POST /api/issues
- GET/PATCH/DELETE /api/issues/:id
- GET /api/companies/:companyId/issues, POST .../batch-update
- GET /api/companies/:companyId/issues/count, /search
- GET/POST /api/issues/:id/comments
- GET /api/issues/:id/documents, GET/PUT /issues/:issue_id/documents/:key
- POST /issues/:issue_id/documents/:key/lock, /unlock
- GET /issues/:id/tree-control/state, GET /issues/:id/tree-holds
- POST /issues/:id/tree-control/preview, POST /issues/:id/tree-holds
- POST /issues/:id/tree-holds/:holdId/release
- POST /api/issues/:id/checkout, /release, /admin/force-release
- GET /api/issues/:id/heartbeat-context
- GET /api/issues/:id/diagnostics/blockers, /subtree, /wakes
- GET /api/issues/low-trust, POST .../promotions
- GET /issues/:issue_id/comments, POST /issues/:issue_id/comments
- DELETE /api/issues/:id/comments/:commentId
- GET /issues/:id/attachments, POST /issues/:id/attachments

#### 缺失需补

**文件**: `crates/api/src/routes/issues.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| I1 | `GET /issues/:id/activity` | 获取议题活动日志 | ActivityService |
| I2 | `GET /issues/:id/cases` | 获取议题关联案例 | CaseService |
| I3 | `GET /issues/:id/active-run` | 获取当前活跃运行 | ExecutionService |
| I4 | `GET /issues/:id/live-runs` | 获取实时运行列表 | ExecutionService |
| I5 | `GET /issues/:id/runs` | 获取运行历史 | ExecutionService |
| I6 | `GET /issues/:id/accepted-plan-decompositions` | 已接受的计划分解 | IssueService |
| I7 | `POST /issues/:id/accepted-plan-decompositions` | 提交计划分解 | IssueService |
| I8 | `GET /issues/:id/approvals` | 获取议题关联审批 | ApprovalService |
| I9 | `POST /issues/:id/approvals` | 创建议题审批 | ApprovalService |
| I10 | `DELETE /issues/:id/approvals/:approvalId` | 删除议题审批 | ApprovalService |
| I11 | `POST /issues/:id/children` | 创建子议题 | IssueService |
| I12 | `POST /issues/:id/read` | 标记已读 | IssueService |
| I13 | `DELETE /issues/:id/read` | 取消标记已读 | IssueService |
| I14 | `POST /issues/:id/inbox-archive` | 归档到收件箱 | IssueService |
| I15 | `DELETE /issues/:id/inbox-archive` | 取消归档 | IssueService |
| I16 | `POST /issues/:id/monitor/check-now` | 立即监控检查 | WatchdogService |
| I17 | `POST /issues/:id/scheduled-retry/retry-now` | 立即重试调度 | WatchdogService |
| I18 | `GET /issues/:id/external-objects` | 外部对象列表 | ExternalObjectService |
| I19 | `GET /issues/:id/external-object-summary` | 外部对象摘要 | ExternalObjectService |
| I20 | `POST /issues/:id/external-objects/refresh` | 刷新外部对象 | ExternalObjectService |
| I21 | `GET /issues/:id/file-resources/list` | 文件资源列表 | FileResourceService |
| I22 | `GET /issues/:id/file-resources/resolve` | 解析文件资源 | FileResourceService |
| I23 | `GET /issues/:id/file-resources/content` | 获取文件资源内容 | FileResourceService |
| I24 | `GET /issues/:id/feedback-votes` | 反馈投票列表 | FeedbackService |
| I25 | `POST /issues/:id/feedback-votes` | 提交反馈投票 | FeedbackService |
| I26 | `GET /issues/:id/feedback-traces` | 反馈追踪 | FeedbackService |
| I27 | `GET /issues/:id/recovery-actions` | 恢复动作列表 | IssueService |
| I28 | `POST /issues/:id/recovery-actions/resolve` | 解决恢复动作 | IssueService |
| I29 | `GET /issues/:id/interactions` | 交互列表 | InteractionService |
| I30 | `POST /issues/:id/interactions` | 创建交互 | InteractionService |
| I31 | `POST /issues/:id/interactions/:interactionId/accept` | 接受交互 | InteractionService |
| I32 | `POST /issues/:id/interactions/:interactionId/reject` | 拒绝交互 | InteractionService |
| I33 | `POST /issues/:id/interactions/:interactionId/respond` | 回复交互 | InteractionService |
| I34 | `POST /issues/:id/interactions/:interactionId/cancel` | 取消交互 | InteractionService |
| I35 | `GET /issues/:id/documents/:key/revisions` | 文档版本列表 | DocumentService |
| I36 | `POST /issues/:id/documents/:key/revisions/:revisionId/restore` | 恢复文档版本 | DocumentService |
| I37 | `DELETE /issues/:id/documents/:key` | 删除文档 | DocumentService |
| I38 | `GET /issues/:id/documents/:key/annotations` | 文档注释列表 | AnnotationService |
| I39 | `POST /issues/:id/work-products` | 创建工作产物 | WorkProductService |
| I40 | `PATCH /work-products/:id` | 更新工作产物 | WorkProductService |
| I41 | `DELETE /work-products/:id` | 删除工作产物 | WorkProductService |
| I42 | `GET /issues/:id/comments/:commentId` | 获取单条评论 | IssueCommentService |
| I43 | `GET /issues/:id/cost-summary` | 议题成本汇总 | CostService |
| I44 | `POST /issues/:id/attachments` (Paperclip 有自定义上传) | 上传附件 | AttachmentService |

**Service 层需补齐**: 新增 `ActivityService`、`ExternalObjectService`、`FileResourceService`、`FeedbackService`、`InteractionService`；扩展 `IssueService`（计划分解、恢复动作等）；扩展 `DocumentService`（版本管理）。

---

### 2.4 Environments / Adapters（环境/适配器）— CRUD + 探测 + 配置

#### 当前已实现
- GET /companies/:companyId/adapters, GET .../:adapter_type
- GET /companies/:companyId/adapters/:adapter_type/models, /detect-model, /test-environment
- POST /environments/:id/probe, POST /environments/:id/acquire

#### 缺失需补

**文件**: `crates/api/src/routes/adapters.rs`, `crates/api/src/routes/environments.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| E1 | `GET /adapters` | 全局适配器列表 | AdapterRegistry |
| E2 | `POST /adapters/install` | 安装适配器 | AdapterRegistry |
| E3 | `GET /adapters/:type` | 获取适配器详情 | AdapterRegistry |
| E4 | `PATCH /adapters/:type` | 更新适配器配置 | AdapterRegistry |
| E5 | `PATCH /adapters/:type/override` | 覆盖适配器配置 | AdapterRegistry |
| E6 | `DELETE /adapters/:type` | 删除适配器 | AdapterRegistry |
| E7 | `POST /adapters/:type/reload` | 重新加载适配器 | AdapterRegistry |
| E8 | `POST /adapters/:type/reinstall` | 重新安装适配器 | AdapterRegistry |
| E9 | `GET /adapters/:type/config-schema` | 获取配置 Schema | AdapterRegistry |
| E10 | `GET /adapters/:type/ui-parser.js` | 获取 UI 解析器 | AdapterRegistry |
| E11 | `GET /companies/:companyId/environments/capabilities` | 环境能力列表 | EnvironmentService |
| E12 | `POST /companies/:companyId/environments/probe-config` | 探测配置 | EnvironmentService |
| E13 | `GET /environments/:id` | 获取环境详情 | EnvironmentService |
| E14 | `PATCH /environments/:id` | 更新环境 | EnvironmentService |
| E15 | `DELETE /environments/:id` | 删除环境 | EnvironmentService |
| E16 | `GET /environments/:id/delete-blast-radius` | 删除影响分析 | EnvironmentService |
| E17 | `GET /environments/:environmentId/custom-image-template` | 自定义镜像模板 | CustomImageService |
| E18 | `DELETE /environments/:environmentId/custom-image-template` | 删除镜像模板 | CustomImageService |
| E19 | `POST /environments/:environmentId/custom-image-template/rollback` | 回滚镜像模板 | CustomImageService |
| E20 | `POST /environments/:environmentId/custom-image-setup-sessions` | 创建设置会话 | CustomImageSetupService |
| E21 | `GET /environment-custom-image-setup-sessions/:id/finish` | 完成设置会话 | CustomImageSetupService |
| E22 | `POST /environment-custom-image-setup-sessions/:id/cancel` | 取消设置会话 | CustomImageSetupService |
| E23 | `GET /environment-leases/:leaseId` | 获取租约详情 | LeaseService |
| E24 | `GET /companies/:companyId/adapters/:type/model-profiles` | 模型配置档案（已有？需确认） | AdapterRegistry |

**Service 层需补齐**: `AdapterRegistry` 添加 install/configure/reload/reinstall/schema 方法；`EnvironmentService` 添加 CRUD + capabilities + probe-config + delete-blast-radius。

---

## 三、P2 — 业务支撑域

### 3.1 Approvals（审批）— 整域新增

**新建文件**: `crates/api/src/routes/approvals.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| AP1 | `GET /companies/:companyId/approvals` | 审批列表 | ApprovalService |
| AP2 | `GET /approvals/:id` | 审批详情 | ApprovalService |
| AP3 | `POST /companies/:companyId/approvals` | 创建审批（issue_id, title, description, required_approvers） | ApprovalService |
| AP4 | `GET /approvals/:id/issues` | 审批关联议题 | ApprovalService |
| AP5 | `POST /approvals/:id/approve` | 批准（decision_note） | ApprovalService |
| AP6 | `POST /approvals/:id/reject` | 驳回 | ApprovalService |
| AP7 | `POST /approvals/:id/request-revision` | 请求修改 | ApprovalService |
| AP8 | `POST /approvals/:id/resubmit` | 重新提交 | ApprovalService |
| AP9 | `GET /approvals/:id/comments` | 审批评论列表 | ApprovalService |
| AP10 | `POST /approvals/:id/comments` | 添加审批评论 | ApprovalService |

**Service 层**: 新增 `ApprovalService` trait + `DefaultApprovalService` + repository。

---

### 3.2 Costs / Budgets（成本/预算）— 整域新增

**新建文件**: `crates/api/src/routes/costs.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| CO1 | `POST /companies/:companyId/cost-events` | 记录成本事件 | CostService |
| CO2 | `POST /companies/:companyId/finance-events` | 记录财务事件 | CostService |
| CO3 | `GET /companies/:companyId/costs/summary` | 成本汇总 | CostService |
| CO4 | `GET /companies/:companyId/costs/by-agent` | 按智能体成本 | CostService |
| CO5 | `GET /companies/:companyId/costs/by-agent-model` | 按模型成本 | CostService |
| CO6 | `GET /companies/:companyId/costs/by-provider` | 按提供商成本 | CostService |
| CO7 | `GET /companies/:companyId/costs/by-biller` | 按计费方成本 | CostService |
| CO8 | `GET /companies/:companyId/costs/by-project` | 按项目成本 | CostService |
| CO9 | `GET /companies/:companyId/costs/window-spend` | 时间窗口消费 | CostService |
| CO10 | `GET /companies/:companyId/costs/quota-windows` | 配额窗口 | CostService |
| CO11 | `GET /companies/:companyId/costs/finance-summary` | 财务汇总 | CostService |
| CO12 | `GET /companies/:companyId/costs/finance-by-biller` | 按计费方财务 | CostService |
| CO13 | `GET /companies/:companyId/costs/finance-by-kind` | 按类型财务 | CostService |
| CO14 | `GET /companies/:companyId/costs/finance-events` | 财务事件列表 | CostService |
| CO15 | `GET /issues/:id/cost-summary` | 议题成本汇总 | CostService |
| CO16 | `GET /companies/:companyId/budgets/overview` | 预算概览 | BudgetService |
| CO17 | `POST /companies/:companyId/budgets/policies` | 创建预算策略 | BudgetService |
| CO18 | `PATCH /companies/:companyId/budgets` | 更新预算 | BudgetService |
| CO19 | `PATCH /agents/:agentId/budgets` | 更新智能体预算 | BudgetService |
| CO20 | `POST /companies/:companyId/budget-incidents/:incidentId/resolve` | 解决预算事件 | BudgetService |

**Service 层**: 新增 `CostService` trait + `DefaultCostService` + repository；新增 `BudgetService` + repository。

---

### 3.3 Executions / Runs（执行/运行）— 补齐

**文件**: `crates/api/src/routes/heartbeats.rs`, 可能需新建 `execution_workspaces.rs` / `runs.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| X1 | `GET /companies/:companyId/heartbeat-runs` | 心跳运行列表 | WatchdogService |
| X2 | `GET /companies/:companyId/live-runs` | 实时运行列表 | ExecutionService |
| X3 | `GET /heartbeat-runs/:runId` | 运行详情 | WatchdogService |
| X4 | `POST /heartbeat-runs/:runId/cancel` | 取消运行 | WatchdogService |
| X5 | `GET /heartbeat-runs/:runId/events` | 运行事件 | WatchdogService |
| X6 | `GET /heartbeat-runs/:runId/log` | 运行日志 | WatchdogService |
| X7 | `GET /heartbeat-runs/:runId/issues` | 运行关联议题 | WatchdogService |
| X8 | `GET /heartbeat-runs/:runId/watchdog-decisions` | 看门狗决策 | WatchdogService |
| X9 | `POST /heartbeat-runs/:runId/watchdog-decisions` | 提交看门狗决策 | WatchdogService |
| X10 | `GET /heartbeat-runs/:runId/workspace-operations` | 工作区操作 | ExecutionService |
| X11 | `GET /workspace-operations/:operationId/log` | 操作日志 | ExecutionService |
| X12 | `GET /issues/:issueId/live-runs` | 议题实时运行 | ExecutionService |
| X13 | `GET /issues/:issueId/active-run` | 当前活跃运行 | ExecutionService |
| X14 | `GET /issues/:id/runs` | 运行历史 | ExecutionService |
| X15 | `GET /execution-workspaces/:id/close-readiness` | 关闭就绪检查 | ExecutionWorkspaceService |
| X16 | `GET /execution-workspaces/:id/workspace-operations` | 工作区操作列表 | ExecutionWorkspaceService |
| X17 | `POST /execution-workspaces/:id/reconcile-branch` | 协调分支 | ExecutionWorkspaceService |
| X18 | `GET /companies/:companyId/workspace-overview` | 工作区概览 | ExecutionWorkspaceService |

---

### 3.4 Skills（技能）— catalog / versions / test-runs 补齐

**文件**: `crates/api/src/routes/skills.rs`

#### 当前已实现
- GET /api/skills/available, GET /api/skills/index, GET /api/skills/:skillName

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| SK1 | `GET /skills/catalog` | 技能目录 | SkillsCatalogService |
| SK2 | `GET /skills/catalog/:catalogId` | 目录详情 | SkillsCatalogService |
| SK3 | `GET /skills/catalog/files` | 目录文件 | SkillsCatalogService |
| SK4 | `GET /companies/:companyId/skills/categories` | 技能分类 | SkillRegistryService |
| SK5 | `GET /companies/:companyId/skills/:skillId` | 技能详情 | SkillRegistryService |
| SK6 | `GET /companies/:companyId/skills/:skillId/fork-precheck` | Fork 预检查 | SkillRegistryService |
| SK7 | `GET /companies/:companyId/skills/:skillId/versions` | 版本列表 | SkillRegistryService |
| SK8 | `GET /companies/:companyId/skills/:skillId/versions/:versionId` | 版本详情 | SkillRegistryService |
| SK9 | `GET /companies/:companyId/skills/:skillId/test-inputs` | 测试输入列表 | SkillRegistryService |
| SK10 | `POST /companies/:companyId/skills/:skillId/test-inputs` | 创建测试输入 | SkillRegistryService |
| SK11 | `PATCH /companies/:companyId/skills/:skillId/test-inputs/:inputId` | 更新测试输入 | SkillRegistryService |
| SK12 | `DELETE /companies/:companyId/skills/:skillId/test-inputs/:inputId` | 删除测试输入 | SkillRegistryService |
| SK13 | `GET /companies/:companyId/skill-test-run-templates` | 测试运行模板列表 | SkillRegistryService |
| SK14 | `POST /companies/:companyId/skill-test-run-templates` | 创建模板 | SkillRegistryService |
| SK15 | `PATCH /companies/:companyId/skill-test-run-templates/:templateId` | 更新模板 | SkillRegistryService |
| SK16 | `DELETE /companies/:companyId/skill-test-run-templates/:templateId` | 删除模板 | SkillRegistryService |
| SK17 | `GET /companies/:companyId/skills/:skillId/test-runs` | 测试运行列表 | SkillRegistryService |
| SK18 | `GET /companies/:companyId/skills/:skillId/test-runs/:runId` | 运行详情 | SkillRegistryService |
| SK19 | `POST /companies/:companyId/skills/:skillId/test-runs/:runId/cancel` | 取消运行 | SkillRegistryService |
| SK20 | `DELETE /companies/:companyId/skills/:skillId/test-runs/:runId` | 删除运行 | SkillRegistryService |
| SK21 | `POST /companies/:companyId/skills/:skillId/star` | 收藏技能 | SkillRegistryService |
| SK22 | `DELETE /companies/:companyId/skills/:skillId/star` | 取消收藏 | SkillRegistryService |
| SK23 | `POST /companies/:companyId/skills/:skillId/fork` | Fork 技能 | SkillRegistryService |
| SK24 | `POST /companies/:companyId/skills/:skillId/audit` | 审计技能 | SkillRegistryService |
| SK25 | `POST /companies/:companyId/skills/:skillId/install-update` | 安装更新 | SkillRegistryService |
| SK26 | `POST /companies/:companyId/skills/:skillId/reset` | 重置技能 | SkillRegistryService |
| SK27 | `GET /companies/:companyId/skills/:skillId/update-status` | 更新状态 | SkillRegistryService |
| SK28 | `GET /companies/:companyId/skills/:skillId/comments` | 评论列表 | SkillRegistryService |
| SK29 | `POST /companies/:companyId/skills/:skillId/comments` | 添加评论 | SkillRegistryService |
| SK30 | `PATCH /companies/:companyId/skills/:skillId/comments/:commentId` | 编辑评论 | SkillRegistryService |
| SK31 | `DELETE /companies/:companyId/skills/:skillId/comments/:commentId` | 删除评论 | SkillRegistryService |
| SK32 | `GET /companies/:companyId/skills/:skillId/files` | 文件列表 | SkillRegistryService |
| SK33 | `PATCH /companies/:companyId/skills/:skillId/files` | 更新文件 | SkillRegistryService |
| SK34 | `DELETE /companies/:companyId/skills/:skillId/files` | 删除文件 | SkillRegistryService |
| SK35 | `POST /companies/:companyId/skills/import` | 导入技能 | SkillRegistryService |
| SK36 | `POST /companies/:companyId/skills/install-catalog` | 安装目录 | SkillRegistryService |
| SK37 | `POST /companies/:companyId/skills/scan-projects` | 扫描项目 | SkillRegistryService |
| SK38 | `DELETE /companies/:companyId/skills/:skillId` | 删除技能 | SkillRegistryService |

**Service 层**: 大幅扩展 `SkillRegistryService`（当前为 `MockSkillRegistryService`）。

---

### 3.5 Plugins（插件）— 整域新增

**新建文件**: `crates/api/src/routes/plugins.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| PL1 | `GET /plugins` | 插件列表 | PluginService |
| PL2 | `GET /plugins/examples` | 插件示例 | PluginService |
| PL3 | `GET /plugins/ui-contributions` | UI 贡献 | PluginService |
| PL4 | `GET /plugins/tools` | 工具列表 | PluginService |
| PL5 | `POST /plugins/tools/execute` | 执行工具 | PluginService |
| PL6 | `POST /plugins/install` | 安装插件 | PluginService |
| PL7 | `GET /plugins/:pluginId` | 插件详情 | PluginService |
| PL8 | `DELETE /plugins/:pluginId` | 删除插件 | PluginService |
| PL9 | `POST /plugins/:pluginId/enable` | 启用插件 | PluginService |
| PL10 | `POST /plugins/:pluginId/disable` | 禁用插件 | PluginService |
| PL11 | `POST /plugins/:pluginId/upgrade` | 升级插件 | PluginService |
| PL12 | `GET /plugins/:pluginId/health` | 插件健康检查 | PluginService |
| PL13 | `GET /plugins/:pluginId/logs` | 插件日志 | PluginService |
| PL14 | `GET /plugins/:pluginId/dashboard` | 插件仪表盘 | PluginService |
| PL15 | `GET /plugins/:pluginId/config` | 获取配置 | PluginService |
| PL16 | `POST /plugins/:pluginId/config` | 更新配置 | PluginService |
| PL17 | `POST /plugins/:pluginId/config/test` | 测试配置 | PluginService |
| PL18 | `POST /plugins/:pluginId/bridge/data` | 桥接数据 | PluginService |
| PL19 | `POST /plugins/:pluginId/bridge/action` | 桥接动作 | PluginService |
| PL20 | `GET /plugins/:pluginId/bridge/stream/:channel` | SSE 桥接流 | PluginService |
| PL21 | `POST /plugins/:pluginId/data/:key` | 存储数据 | PluginService |
| PL22 | `POST /plugins/:pluginId/actions/:key` | 触发动作 | PluginService |
| PL23 | `GET /plugins/:pluginId/jobs` | 作业列表 | PluginService |
| PL24 | `GET /plugins/:pluginId/jobs/:jobId/runs` | 作业运行列表 | PluginService |
| PL25 | `POST /plugins/:pluginId/jobs/:jobId/trigger` | 触发作业 | PluginService |
| PL26 | `POST /plugins/:pluginId/webhooks/:endpointKey` | Webhook 入口 | PluginService |
| PL27 | `GET /plugins/:pluginId/companies/:companyId/local-folders` | 本地文件夹列表 | PluginService |
| PL28 | `POST /plugins/:pluginId/companies/:companyId/local-folders/validate` | 验证文件夹 | PluginService |
| PL29 | `PUT /plugins/:pluginId/companies/:companyId/local-folders/:folderKey` | 更新文件夹 | PluginService |
| PL30 | `GET /plugins/:pluginId/companies/:companyId/local-folders/:folderKey/status` | 文件夹状态 | PluginService |
| PL31 | `GET /_plugins/:pluginId/ui/*filePath` | 插件静态资源 | PluginService |

**Service 层**: 新增 `PluginService` trait + 实现（含 worker manager、生命周期管理等）。

---

## 四、P3 — 平台/运维域

### 4.1 Pipelines（管道）— 补齐

**文件**: `crates/api/src/routes/pipelines.rs`

#### 当前已实现
- GET /companies/:companyId/pipelines, POST /companies/:companyId/pipelines
- GET /companies/:companyId/pipelines-attention
- GET /pipelines/:pipelineId, GET .../health-warnings
- GET /pipelines/:pipelineId/cases, POST /pipelines/:pipelineId/cases
- GET /pipelines/:pipelineId/stages, GET /pipelines/:pipelineId/transitions

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| PP1 | `GET /companies/:companyId/review-cases` | 审核案例列表 | PipelineService |
| PP2 | `POST /companies/:companyId/review-cases/bulk` | 批量审核 | PipelineService |
| PP3 | `GET /companies/:companyId/case-events` | 案例事件列表 | PipelineService |
| PP4 | `GET /pipelines/:pipelineId/health` | 管道健康详情 | PipelineService |
| PP5 | `GET /pipelines/:pipelineId/intake-form` | 接收表单 | PipelineService |
| PP6 | `POST /pipelines/:pipelineId/stages` | 创建阶段 | PipelineService |
| PP7 | `PATCH /pipelines/:pipelineId/stages/:stageId` | 更新阶段 | PipelineService |
| PP8 | `PATCH /pipelines/:pipelineId/stages/:stageId/automation-env` | 更新自动化环境 | PipelineService |
| PP9 | `DELETE /pipelines/:pipelineId/stages/:stageId` | 删除阶段 | PipelineService |
| PP10 | `PUT /pipelines/:pipelineId/transitions` | 更新转换配置 | PipelineService |
| PP11 | `GET /pipelines/:pipelineId/documents/:key` | 管道文档 | DocumentService |
| PP12 | `PUT /pipelines/:pipelineId/documents/:key` | 更新管道文档 | DocumentService |
| PP13 | `GET /pipelines/:pipelineId/documents/:key/revisions` | 文档版本 | DocumentService |
| PP14 | `POST /pipelines/:pipelineId/documents/:key/revisions/:revisionId/restore` | 恢复版本 | DocumentService |
| PP15 | `POST /pipelines/:pipelineId/cases/batch` | 批量创建案例 | PipelineService |

---

### 4.2 Goals / Routines（目标/例程）— 补齐

**文件**: `crates/api/src/routes/goals.rs`, `crates/api/src/routes/routines.rs`

#### 当前已实现
- GET /companies/:companyId/goals, POST .../goals
- GET/PATCH/DELETE /goals/:goalId
- GET /goals/:goalId/children, /hierarchy, /progress
- POST /goals/:goalId/abandon, /complete
- GET /companies/:companyId/routines, POST .../routines
- GET/PATCH/DELETE /routines/:routineId
- GET /routines/:routineId/runs
- POST /routines/:routineId/pause, /resume, /trigger

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| GR1 | `GET /routines/:id/revisions` | 例程版本列表 | RoutineService |
| GR2 | `POST /routines/:id/revisions/:revisionId/restore` | 恢复例程版本 | RoutineService |
| GR3 | `GET /routines/:id/triggers` | 触发器列表 | RoutineTriggerService |
| GR4 | `POST /routines/:id/triggers` | 创建触发器 | RoutineTriggerService |
| GR5 | `PATCH /routine-triggers/:id` | 更新触发器 | RoutineTriggerService |
| GR6 | `DELETE /routine-triggers/:id` | 删除触发器 | RoutineTriggerService |
| GR7 | `POST /routine-triggers/:id/rotate-secret` | 轮换触发器密钥 | RoutineTriggerService |
| GR8 | `POST /routine-triggers/public/:publicId/fire` | 公开触发（无需认证） | RoutineTriggerService |
| GR9 | `POST /routines/:id/run` | 手动触发运行 | RoutineService |

---

### 4.3 Companies（公司）— 补齐

**文件**: `crates/api/src/routes/companies.rs`

#### 当前已实现
- GET/POST /companies
- GET/PATCH/DELETE /companies/:companyId
- GET /companies/stats
- POST /companies/:companyId/archive
- PATCH /companies/:companyId/branding

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| CM1 | `GET /companies/:companyId/activity` | 公司活动日志 | ActivityService |
| CM2 | `POST /companies/:companyId/activity` | 记录活动 | ActivityService |
| CM3 | `PATCH /companies/:companyId/members/:memberId/permissions` | 成员权限更新 | AccessService |
| CM4 | `GET /companies/:companyId/search` | 公司内搜索 | SearchService |
| CM5 | `GET /companies/:companyId/labels` | 标签列表 | LabelService |
| CM6 | `POST /companies/:companyId/labels` | 创建标签 | LabelService |
| CM7 | `DELETE /labels/:labelId` | 删除标签 | LabelService |
| CM8 | `GET /companies/:companyId/sidebar-badges` | 侧边栏徽章 | SidebarService |
| CM9 | `GET /companies/:companyId/sidebar-preferences/me` | 个人侧边栏偏好 | SidebarService |
| CM10 | `PUT /companies/:companyId/sidebar-preferences/me` | 更新偏好 | SidebarService |
| CM11 | `GET /companies/:companyId/users/:userSlug/profile` | 用户资料 | UserProfileService |
| CM12 | `GET /companies/:companyId/export` | 公司数据导出 | CompanyService |
| CM13 | `POST /companies/:companyId/exports/preview` | 导出预览 | CompanyService |
| CM14 | `GET /companies/:companyId/timeline` | 公司时间线 | CompanyService |
| CM15 | `GET /companies/:companyId/artifacts` | 公司产物 | CompanyService |
| CM16 | `GET /companies/:companyId/feedback-traces` | 反馈追踪 | FeedbackService |
| CM17 | `POST /companies/:companyId/imports/preview` | 导入预览 | CompanyService |
| CM18 | `POST /companies/:companyId/imports/apply` | 执行导入 | CompanyService |
| CM19 | `GET /companies/:companyId/inbox-dismissals` | 收件箱关闭列表 | InboxService |
| CM20 | `POST /companies/:companyId/inbox-dismissals` | 关闭收件箱项 | InboxService |

---

### 4.4 Auth / Admin（认证/管理）— 补齐

**文件**: `crates/api/src/routes/auth.rs`

#### 当前已实现
- GET /api/auth/get-session
- GET/PATCH /api/auth/profile
- POST /api/bootstrap/claim
- GET /api/admin/users

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| AU1 | `POST /admin/users/:userId/promote-instance-admin` | 提升为实例管理员 | AdminService |
| AU2 | `POST /admin/users/:userId/demote-instance-admin` | 降级实例管理员 | AdminService |
| AU3 | `GET /admin/users/:userId/company-access` | 用户公司访问列表 | AdminService |
| AU4 | `PUT /admin/users/:userId/company-access` | 更新用户公司访问 | AdminService |
| AU5 | `POST /join-requests/:requestId/claim-api-key` | 认领 API Key | AuthService |

---

### 4.5 Secrets / Providers（密钥/提供方）— 补齐

**文件**: `crates/api/src/routes/user_secrets.rs`, `crates/api/src/routes/user_secret_definitions.rs`, `crates/api/src/routes/secret_provider_configs.rs`, `crates/api/src/routes/secrets.rs`

#### 当前已实现
- GET/POST /companies/:companyId/me/user-secrets
- GET /companies/:companyId/me/user-secrets/:definitionId
- GET /companies/:companyId/me/user-secrets/:secretId/bindings
- POST /companies/:companyId/me/user-secrets/:secretId/rotate
- GET/POST /companies/:companyId/user-secret-definitions
- GET /companies/:companyId/user-secret-definitions/:definitionId/coverage
- ✅ SE1-SE4: PATCH/DELETE user-secret-definitions + user-secrets — `user_secret_definitions.rs` / `user_secrets.rs`
- ✅ SE5: GET /companies/:companyId/secret-providers — `routes/secrets.rs`
- ✅ SE6: GET /companies/:companyId/secret-providers/health — `secret_provider_configs.rs`
- ✅ SE7-SE13: secret-provider-configs CRUD + discovery + default + health — `secret_provider_configs.rs`
- ✅ SE14: GET/POST /companies/:companyId/secrets — `routes/secrets.rs`
- ✅ SE15: GET /secrets/:id — `routes/secrets.rs`
- ✅ SE16: PATCH /secrets/:id — `routes/secrets.rs`
- ✅ SE17: DELETE /secrets/:id (软删除) — `routes/secrets.rs`
- ✅ SE18: POST /secrets/:id/rotate — `routes/secrets.rs`
- ✅ SE19: GET /secrets/:id/usage — `routes/secrets.rs`
- ✅ SE20: GET /secrets/:id/access-events — `routes/secrets.rs`
- ✅ GET /companies/:companyId/secret-provider-configs — `secret_provider_configs.rs`

#### 缺失需补

| # | 改动点 | 说明 | 涉及 Service | 状态 |
|---|--------|------|-------------|------|
| SE1 | `PATCH /companies/:companyId/user-secret-definitions/:definitionId` | 更新密钥定义 | UserSecretDefinitionService | ✅ |
| SE2 | `DELETE /companies/:companyId/user-secret-definitions/:definitionId` | 删除密钥定义 | UserSecretDefinitionService | ✅ |
| SE3 | `PATCH /companies/:companyId/me/user-secrets/:secretId` | 更新用户密钥 | UserSecretService | ✅ |
| SE4 | `DELETE /companies/:companyId/me/user-secrets/:secretId` | 删除用户密钥 | UserSecretService | ✅ |
| SE5 | `GET /companies/:companyId/secret-providers` | 密钥提供商列表 | SecretProviderService | ✅ |
| SE6 | `GET /companies/:companyId/secret-providers/health` | 提供商健康检查 | SecretProviderService | ✅ |
| SE7 | `POST /companies/:companyId/secret-provider-configs` | 创建提供商配置 | SecretProviderConfigService | ✅ |
| SE8 | `POST /companies/:companyId/secret-provider-configs/discovery/preview` | 发现预览 | SecretProviderConfigService | ✅ |
| SE9 | `GET /secret-provider-configs/:id` | 配置详情 | SecretProviderConfigService | ✅ |
| SE10 | `PATCH /secret-provider-configs/:id` | 更新配置 | SecretProviderConfigService | ✅ |
| SE11 | `DELETE /secret-provider-configs/:id` | 删除配置 | SecretProviderConfigService | ✅ |
| SE12 | `POST /secret-provider-configs/:id/default` | 设为默认 | SecretProviderConfigService | ✅ |
| SE13 | `POST /secret-provider-configs/:id/health` | 健康检查 | SecretProviderConfigService | ✅ |
| SE14 | `POST /companies/:companyId/secrets` | 创建公司级密钥 | SecretService | ✅ |
| SE15 | `GET /secrets/:id` | 密钥详情 | SecretService | ✅ |
| SE16 | `PATCH /secrets/:id` | 更新密钥 | SecretService | ✅ |
| SE17 | `DELETE /secrets/:id` | 删除密钥 | SecretService | ✅ |
| SE18 | `POST /secrets/:id/rotate` | 轮换密钥 | SecretService | ✅ |
| SE19 | `GET /secrets/:id/usage` | 密钥使用情况 | SecretService | ✅ |
| SE20 | `GET /secrets/:id/access-events` | 访问事件 | SecretService | ✅ |

---

## 五、P4 — 收尾域

### 5.1 Activity / Dashboard（活动/仪表盘）

**新建文件**: `crates/api/src/routes/activity.rs`, `crates/api/src/routes/dashboard.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| AD1 | `GET /issues/:id/activity` | 议题活动 | ActivityService |
| AD2 | `GET /issues/:id/runs` | 议题运行 | ActivityService |
| AD3 | `GET /heartbeat-runs/:runId/issues` | 运行关联议题 | ActivityService |
| AD4 | `GET /companies/:companyId/dashboard` | 公司仪表盘 | DashboardService |

### 5.2 Cloud Upstreams（云上游）

**新建文件**: `crates/api/src/routes/cloud_upstreams.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| CU1 | `GET /cloud-upstreams` | 上游列表 | CloudUpstreamService |
| CU2 | `POST /cloud-upstreams/connect/start` | 开始连接 | CloudUpstreamService |
| CU3 | `POST /cloud-upstreams/connect/finish` | 完成连接 | CloudUpstreamService |
| CU4 | `POST /cloud-upstreams/:connectionId/push-runs/preview` | 推送预览 | CloudUpstreamService |
| CU5 | `POST /cloud-upstreams/:connectionId/push-runs` | 执行推送 | CloudUpstreamService |
| CU6 | `GET /cloud-upstreams/:connectionId/push-runs/:runId` | 推送运行详情 | CloudUpstreamService |
| CU7 | `POST /cloud-upstreams/:connectionId/push-runs/:runId/cancel` | 取消推送 | CloudUpstreamService |
| CU8 | `POST /cloud-upstreams/:connectionId/push-runs/:runId/activation` | 激活推送 | CloudUpstreamService |

### 5.3 Instance Settings（实例设置）

**新建文件**: `crates/api/src/routes/instance_settings.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| IS1 | `GET /instance/settings` | 实例设置 | InstanceSettingsService |
| IS2 | `PATCH /instance/settings` | 更新实例设置 | InstanceSettingsService |
| IS3 | `GET /instance/settings/general` | 通用设置 | InstanceSettingsService |
| IS4 | `PATCH /instance/settings/general` | 更新通用设置 | InstanceSettingsService |
| IS5 | `GET /instance/settings/experimental` | 实验性功能设置 | InstanceSettingsService |
| IS6 | `PATCH /instance/settings/experimental` | 更新实验性设置 | InstanceSettingsService |
| IS7 | `POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/preview` | 自动恢复预览 | InstanceSettingsService |
| IS8 | `POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/run` | 执行自动恢复 | InstanceSettingsService |
| IS9 | `POST /instance/database-backups` | 数据库备份 | InstanceSettingsService |

### 5.4 LLMs / OpenAPI

**新建文件**: `crates/api/src/routes/llms.rs`, `crates/api/src/routes/openapi.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| LM1 | `GET /llms/agent-configuration.txt` | Agent 配置文本 | LlmService |
| LM2 | `GET /llms/agent-icons.txt` | Agent 图标文本 | LlmService |
| LM3 | `GET /llms/agent-configuration/:adapterType.txt` | 按适配器类型配置 | LlmService |
| LM4 | `GET /openapi.json` | OpenAPI 规范 | - |
| LM5 | `GET /stats` | 统计信息 | StatsService |

### 5.5 Assets / Board Chat / File Resources / Labels

**新建文件**: `crates/api/src/routes/assets.rs`, `crates/api/src/routes/board_chat.rs`, `crates/api/src/routes/labels.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| AS1 | `POST /companies/:companyId/assets/images` | 上传图片资产 | AssetService |
| AS2 | `POST /companies/:companyId/logo` | 上传 Logo | AssetService |
| AS3 | `GET /assets/:assetId/content` | 获取资产内容 | AssetService |
| BC1 | `POST /board/chat/stream` | SSE 流式看板对话 | BoardChatService |
| LB1 | `GET /companies/:companyId/labels` | 标签列表 | LabelService |
| LB2 | `POST /companies/:companyId/labels` | 创建标签 | LabelService |
| LB3 | `DELETE /labels/:labelId` | 删除标签 | LabelService |

### 5.6 Resource Memberships（资源成员）

**文件**: `crates/api/src/routes/access_control.rs`

| # | 改动点 | 说明 | 涉及 Service |
|---|--------|------|-------------|
| RM1 | `PUT /companies/:companyId/resource-memberships/me/projects/:projectId`（支持多个项目） | 批量项目成员 | AccessService |

---

## 六、落地执行计划

### 执行顺序与依赖关系

```
Phase 1 (P1) — 核心域补齐
  ├── 2.1 Agents 补齐 (A1-A6)
  ├── 2.2 Cases 补齐 (C1-C23)
  ├── 2.3 Issues 补齐 (I1-I44)
  └── 2.4 Environments/Adapters 补齐 (E1-E24)

Phase 2 (P2) — 业务支撑域
  ├── 3.1 Approvals (AP1-AP10)
  ├── 3.2 Costs/Budgets (CO1-CO20)
  ├── 3.3 Executions/Runs (X1-X18)
  ├── 3.4 Skills (SK1-SK38)
  └── 3.5 Plugins (PL1-PL31)

Phase 3 (P3) — 平台运维域
  ├── 4.1 Pipelines (PP1-PP15)
  ├── 4.2 Goals/Routines (GR1-GR9)
  ├── 4.3 Companies (CM1-CM20)
  ├── 4.4 Auth/Admin (AU1-AU5)
  └── 4.5 Secrets/Providers (SE1-SE20)

Phase 4 (P4) — 收尾域
  ├── 5.1 Activity/Dashboard (AD1-AD4)
  ├── 5.2 Cloud Upstreams (CU1-CU8)
  ├── 5.3 Instance Settings (IS1-IS9)
  ├── 5.4 LLMs/OpenAPI (LM1-LM5)
  ├── 5.5 Assets/Board Chat/Labels (AS1-BC1-LB3)
  └── 5.6 Resource Memberships (RM1)
```

### 每个改动点的标准实现模式

每个路由改动的实现需包含：

1. **Route 层** (`crates/api/src/routes/xxx.rs`)
   - 定义 `pub fn xxx_routes() -> Router<impl State>` 函数
   - 实现 `async fn handler(State(state), Path(params), Query/Json(body)) -> ApiResult<Json<T>>`
   - 在 `crates/api/src/app_state.rs` 的 `create_router()` 中注册 `.merge()`

2. **Service 层** (`crates/services/src/xxx_service.rs`)
   - 定义/扩展 trait（如 `pub trait IssueService`）
   - 实现 `DefaultXxxService`（DB 实现）
   - 或在 `main.rs` 中保留 `MockXxxService` 占位

3. **Repository 层** (`crates/repositories/src/xxx_repository.rs`)
   - 定义 trait + `PgXxxRepository` 实现
   - SQLx query + model 映射

4. **Model 层** (`crates/models/src/xxx.rs`)
   - `#[derive]` struct + sea-query/sqlx 映射

5. **测试**
   - Service 层单元测试：`cargo test --lib -p services`
   - API 层集成测试（可选）

### 当前 Mock 服务清单（需逐步替换为真实实现）

| Mock Service | 所在文件 | 涉及域 |
|-------------|----------|--------|
| `MockCaseService` | `services/src/case_service.rs` | Cases |
| `MockSkillRegistryService` | `services/src/skill_registry_service.rs` | Skills |
| `MockCustomImageSetupService` | `services/src/custom_image_setup_service.rs` | Environments |
| `MockEnvironmentDiagnosticsService` | `services/src/environment_diagnostics_service.rs` | Environments |
| `MockInviteResourceService` | `services/src/invite_resource_service.rs` | Invites |
| `MockRoutineAnnotationService` | `services/src/routine_annotation_service.rs` | Routines |
| `MockSecretRemoteImportService` | `services/src/secret_remote_import_service.rs` | Secrets |
| `MockWorkProductService` | `services/src/work_product_service.rs` | Work Products |
| `MockAttachmentService` | `services/src/attachment_service.rs` | Attachments |
| `MockSecretProviderConfigService` | `services/src/secret_provider_config_service.rs` | Secrets |

---

## 附录：路由对比统计

| 模块 | Paperclip 路由文件 | 行数 | Parrot 路由文件 | 行数 |
|------|-------------------|:---:|-----------------|:---:|
| Access Control | `access.ts` | 4821 | `access_control.rs` | 1077 |
| Agents | `agents.ts` | 3889 | `agents.rs` | 603 |
| Issues | `issues.ts` | 10423 | `issues.rs` | 267 |
| Cases | `cases.ts` | 1539 | `cases.rs` | 208 |
| Pipelines | `pipelines.ts` | 2913 | `pipelines.rs` | 233 |
| Plugins | `plugins.ts` | 2992 | — | 0 |
| Companies | `companies.ts` | 702 | `companies.rs` | 150 |
| Environments | `environments.ts` | 960 | `environments.rs` | 346 |
| Secrets | `secrets.ts` | 859 | `user_secrets.rs` + 相关 | 259+ |
| Skills | `company-skills.ts` | 1216 | `skills.rs` | ~50 |
| Approvals | `approvals.ts` | 425 | — | 0 |
| Costs | `costs.ts` | 412 | — | 0 |
| Routines | `routines.ts` | 665 | `routines.rs` | 176 |
| Projects | `projects.ts` | 724 | `projects.rs` | 214 |
| Goals | `goals.ts` | — | `goals.rs` | 207 |
| Auth | `auth.ts` | — | `auth.rs` | 231 |
| **总计** | **46 个路由文件** | **45558** | **42 个路由文件** | **7789** |
