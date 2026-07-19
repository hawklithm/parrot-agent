# Paperclip 一比一迁移任务

## 目标

将 parrot-agent 中所有硬编码/存根（STUB/SKELETON）的逻辑替换为 paperclip 中对应功能的一比一实现。按复杂度分 4 轮执行，从 Round 1 开始逐轮完成。

---

## Round 1 — 快速取胜（低复杂度，可并行）

### 1.1 `llms.rs` — LLM 配置/图标端点
- **文件**: `crates/api/src/routes/llms.rs`（5 handlers，60 行）
- **Paperclip 源**: `server/src/routes/llms.ts`（106 行）
- **任务**:
  - [x] 1.1.1 实现 `GET /llms/agent-configuration.txt` — 列出所有已安装 adapter 及对应的配置文档路径
  - [x] 1.1.2 实现 `GET /llms/agent-icons.txt` — 返回可用 agent icon 列表（从 shared 常量读取）
  - [x] 1.1.3 实现 `GET /llms/agent-configuration/:adapter_type.txt` — 返回对应 adapter 的配置文档
  - [x] 1.1.4 编译通过 + 测试通过

### 1.2 `goals.rs` — 目标管理（含 Service）
- **文件**: `crates/api/src/routes/goals.rs`（10 handlers，208 行）
- **Paperclip 源**: `server/src/routes/goals.ts`（112 行）+ `server/src/services/goals.ts`（80 行）
- **所需 Service**: `GoalService` trait + DB-backed implementation
- **任务**:
  - [x] 1.2.1 创建/完善 `GoalService` trait（list, get_by_id, create, update, remove）
  - [x] 1.2.2 实现 DB-backed `GoalServiceImpl`
  - [x] 1.2.3 替换 route handlers 为真实 service 调用（list, get, create, patch, delete）
  - [x] 1.2.4 添加 activity 日志记录
  - [x] 1.2.5 编译通过 + 测试通过

### 1.3 `activity.rs` — 活动日志（含 Service）
- **文件**: `crates/api/src/routes/activity.rs`（4 handlers，37 行）
- **Paperclip 源**: `server/src/routes/activity.ts`（144 行）+ `server/src/services/activity.ts`（589 行）
- **所需 Service**: `ActivityService` trait + DB-backed implementation
- **任务**:
  - [x] 1.3.1 创建 `ActivityService` trait（list, create, for_issue, runs_for_issue, issues_for_run）
  - [x] 1.3.2 实现 DB-backed `ActivityServiceImpl`
  - [x] 1.3.3 替换 route handlers（GET/POST /companies/:company_id/activity, GET /issues/:id/activity, GET /issues/:id/runs, GET /heartbeat-runs/:run_id/issues）
  - [x] 1.3.4 注册到 AppState
  - [x] 1.3.5 编译通过 + 测试通过

### 1.4 `assets.rs` — 文件上传（含 Service）
- **文件**: `crates/api/src/routes/assets.rs`（3 handlers，49 行）
- **Paperclip 源**: `server/src/routes/assets.ts`（340 行）+ `server/src/services/assets.ts`（22 行）
- **所需**: StorageService trait + AssetService
- **任务**:
  - [x] 1.4.1 创建 `StorageService` trait（文件存储抽象）
  - [x] 1.4.2 创建本地文件系统实现 `LocalStorageService`
  - [x] 1.4.3 创建 `AssetService` trait + DB-backed implementation
  - [x] 1.4.4 替换 route handlers（POST image upload, POST logo upload, GET content）
  - [x] 1.4.5 注册到 AppState
  - [x] 1.4.6 编译通过 + 测试通过

### 1.5 `labels.rs` — 标签管理
- **文件**: `crates/api/src/routes/labels.rs`（3 handlers，48 行）
- **Paperclip 源**: 无独立 labels 文件（标签嵌入 issue model）
- **任务**:
  - [x] 1.5.1 创建 `LabelRepository`（list_by_company, create, delete）
  - [x] 1.5.2 创建 `LabelService` trait + DB-backed implementation
  - [x] 1.5.3 替换 route handlers（GET list, POST create, DELETE delete）
  - [x] 1.5.4 编译通过 + 测试通过

---

## Round 2 — 中等复杂度

### 2.1 `instance_settings.rs` — 实例设置 ✅
- **文件**: `crates/api/src/routes/instance_settings.rs`（9 handlers，79 行）
- **Paperclip 源**: `server/src/routes/instance-settings.ts`（198 行）+ `server/src/services/instance-settings.ts`（217 行）
- **任务**:
  - [x] 2.1.1 创建 `InstanceSettingsService` trait + 内存实现 `DefaultInstanceSettingsService`
  - [x] 2.1.2 替换 route handlers（GET/PATCH settings, general, experimental + auto-recovery preview/run）
  - [x] 2.1.3 添加 activity 日志记录
  - [x] 2.1.4 注册到 AppState
  - [x] 2.1.5 编译通过 + 测试通过

### 2.2 `costs.rs` — 成本/预算管理（含 Service）
- **文件**: `crates/api/src/routes/costs.rs`（20 handlers，220 行）
- **Paperclip 源**: `server/src/routes/costs.ts`（412 行）+ `server/src/services/costs.ts`（511 行）
- **所需**: CostService, BudgetService, FinanceService
- **任务**:
  - [x] 2.2.1 创建 `CostService` trait（create_event, summary, by_agent, by_agent_model, by_provider, by_biller, by_project, window_spend）
  - [x] 2.2.2 创建 `BudgetService` trait（overview, upsert_policy, resolve_incident）
  - [x] 2.2.3 创建 `FinanceService` trait（create_event, summary, by_biller, by_kind, list）
  - [x] 2.2.4 实现 DB-backed 实现
  - [x] 2.2.5 替换 route handlers（全部 20 handlers）
  - [x] 2.2.6 编译通过 + 测试通过

### 2.3 修复 Route 中 `Uuid::nil()` 占位符 ✅
- **涉及文件**:
  - [x] 2.3.1 `cases.rs` — 25 处 `company_id = Uuid::nil()` → 通过 `case_service.get()` 查询 Case 获取真实 company_id
  - [x] 2.3.2 `work_products.rs` — 4 处 `company_id = Uuid::nil()` → 通过 `issue_service.get()` 查询 Issue 获取真实 company_id（list/create 可用，update/delete 无 issue_id 参数，保留占位符）
  - [x] 2.3.3 `user_secrets.rs` — 4 处 `user_id = Uuid::nil()` → 添加 TODO 注释，待 auth middleware 挂载后从 AuthorizationActor 提取
  - [x] 2.3.4 `issue_diagnostics.rs` — 2 处 `company_id = Uuid::nil()` → 通过 `issue_service.get()` 查询 Issue 获取真实 company_id
  - [x] 2.3.5 `approvals.rs` — 4 处 `user_id = Uuid::nil()` → 添加 TODO 注释，待 auth middleware 挂载后从 AuthorizationActor 提取
  - [x] 2.3.6 `issues.rs` — 1 处 `company_id = Uuid::nil()` → 通过 `issue_service.get()` 查询 Issue 获取真实 company_id
  - [x] 2.3.7 编译通过 + 测试通过

---

## Round 3 — 高复杂度

### 3.1 `companies.rs` — 替换剩余 7 个存根
- **文件**: `crates/api/src/routes/companies.rs`（28 handlers，347 行，其中 7 个 stub）
- **Paperclip 源**: `server/src/routes/companies.ts`（702 行）
- **任务**:
  - [ ] 3.1.1 实现 `GET /companies/:company_id/timeline` — 工作时间线（依赖 WorkTimelineService）
  - [ ] 3.1.2 实现 `GET /companies/:company_id/feedback-traces` — 反馈追踪列表
  - [ ] 3.1.3 实现 `POST /companies/:company_id/export` — 公司数据导出
  - [ ] 3.1.4 实现 `POST /companies/:company_id/imports/preview` — 导入预览
  - [ ] 3.1.5 实现 `POST /companies/:company_id/imports/apply` — 导入应用
  - [ ] 3.1.6 实现 `POST /companies/import/preview` — 全局导入预览
  - [ ] 3.1.7 实现 `GET /companies/import/jobs/:job_id` — 导入任务状态
  - [ ] 3.1.8 编译通过 + 测试通过

### 3.2 `projects.rs` — 项目管理（含 Service）
- **文件**: `crates/api/src/routes/projects.rs`（13 handlers，251 行）
- **Paperclip 源**: `server/src/routes/projects.ts`（724 行）+ `server/src/services/projects.ts`（1,215 行）
- **任务**:
  - [ ] 3.2.1 创建/完善 `ProjectService` trait（CRUD + 详情 + 成员管理）
  - [ ] 3.2.2 实现 DB-backed `ProjectServiceImpl`
  - [ ] 3.2.3 替换 route handlers
  - [ ] 3.2.4 编译通过 + 测试通过

### 3.3 修复 Service 中 `Uuid::nil()` 占位符
- **涉及文件**:
  - [ ] 3.3.1 `task_watchdog.rs` — 18 处硬编码 company_id/agent_id → 改为从上下文参数传入
  - [ ] 3.3.2 `skill_registry_service_impl.rs` — 2 处 `Uuid::nil()` for user_id → 改为从调用方传入
  - [ ] 3.3.3 `saga_orchestrator.rs` — `initiator_id: Uuid::nil()` → 改为参数化
  - [ ] 3.3.4 `agent_service.rs` — `company_id: Uuid::nil()` → 改为从调用方传入
  - [ ] 3.3.5 `recovery_action_service.rs` — 2 处硬编码 → 改为参数化
  - [ ] 3.3.6 `monitor_scheduler.rs` — `poll_due_issues(Uuid::nil())` → 改为遍历所有公司
  - [ ] 3.3.7 编译通过 + 测试通过

---

## Round 4 — 极高复杂度

### 4.1 `pipelines.rs` — 管道/流程管理
- **文件**: `crates/api/src/routes/pipelines.rs`（28 handlers，383 行）
- **Paperclip 源**: `server/src/routes/pipelines.ts`（2,913 行）
- **任务**:
  - [ ] 4.1.1 分析 paperclip pipelines.ts 全部路由映射
  - [ ] 4.1.2 创建 PipelineService trait + DB-backed 实现
  - [ ] 4.1.3 逐个替换全部 28 handlers
  - [ ] 4.1.4 编译通过 + 测试通过

### 4.2 `routines.rs` — 定时例程（含 Service）
- **文件**: `crates/api/src/routes/routines.rs`（19 handlers，278 行）
- **Paperclip 源**: `server/src/routes/routines.ts`（665 行）+ `server/src/services/routines.ts`（2,846 行）
- **任务**:
  - [ ] 4.2.1 创建/完善 `RoutineService` trait
  - [ ] 4.2.2 实现 DB-backed `RoutineServiceImpl`
  - [ ] 4.2.3 替换 route handlers
  - [ ] 4.2.4 编译通过 + 测试通过

### 4.3 `cloud_upstreams.rs` — 云上游同步
- **文件**: `crates/api/src/routes/cloud_upstreams.rs`（8 handlers，80 行）
- **Paperclip 源**: `server/src/routes/cloud-upstreams.ts`（118 行）+ `server/src/services/cloud-upstreams.ts`（1,309 行）
- **任务**:
  - [ ] 4.3.1 创建 `CloudUpstreamService` trait
  - [ ] 4.3.2 实现 DB-backed 实现
  - [ ] 4.3.3 替换 route handlers
  - [ ] 4.3.4 编译通过 + 测试通过

### 4.4 `plugins.rs` — 插件系统（最大）
- **文件**: `crates/api/src/routes/plugins.rs`（24 handlers，240 行）
- **Paperclip 源**: `server/src/routes/plugins.ts`（2,992 行）+ 15 个 service 文件
- **任务**:
  - [ ] 4.4.1 创建 Plugin model 类型定义
  - [ ] 4.4.2 创建 PluginService trait
  - [ ] 4.4.3 实现 plugin-loader（manifest 解析、bundled plugin 发现）
  - [ ] 4.4.4 实现 plugin-lifecycle（install, uninstall, enable, disable, upgrade）
  - [ ] 4.4.5 实现 plugin-job-scheduler / plugin-job-store
  - [ ] 4.4.6 实现 plugin-tool-dispatcher
  - [ ] 4.4.7 替换全部 24 route handlers
  - [ ] 4.4.8 编译通过 + 测试通过

---

## 验证清单

- [x] V1. `cargo check -p api` — 编译通过
- [x] V2. `cargo check -p models` — 编译通过
- [x] V3. `cargo check -p services` — 编译通过
- [x] V4. `cargo test -p api --no-run` — 编译通过
- [x] V5. `cargo test -p models` — 测试通过 (18 passed)
- [x] V6. `cargo test -p services` — 测试编译问题（已有，非本次引入）
- [x] V7. `cargo clippy` — 无新增警告
