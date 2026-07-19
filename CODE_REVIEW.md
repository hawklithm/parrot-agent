# Code Review: 未提交代码 vs Paperclip 一致性分析

> 审查日期: 2025-07-19
> 更新日期: 2025-07-19（细化修复方案为 Task）
> 审查范围: 24 个已修改但未提交的文件 + 4 个新增文件
> 参考源: `/Users/adazhao/workspace/paperclip`

---

## 目录

1. [编译错误修复](#1-编译错误修复)
2. [cloud_upstreams.rs — OAuth + Push Pipeline 补全](#2-cloud_upstreamsrs--oauth--push-pipeline-补全)
3. [companies.rs — 权限检查 + 业务逻辑补全](#3-companiesrs--权限检查--业务逻辑补全)
4. [plugins.rs / plugin_service.rs — 权限 + 子服务补全](#4-pluginsrs--plugin_servicers--权限--子服务补全)
5. [monitor_scheduler.rs — run_loop 启动修复](#5-monitor_schedulerrs--run_loop-启动修复)
6. [saga_orchestrator.rs — initiator_id 持久化改进](#6-saga_orchestratorrs--initiator_id-持久化改进)
7. [CloudUpstreamService 分层架构](#7-cloudupstreamservice-分层架构)
8. [Plugin 模型字段补全](#8-plugin-模型字段补全)
9. [migrations 表结构补全](#9-migrations-表结构补全)
10. [task.md 状态修正](#10-taskmd-状态修正)

---

## 1. 编译错误修复

### 🔴 问题：auth.rs 缺少 `axum::extract::Path` 导入

`crates/api/src/routes/auth.rs` 第 278-311 行的 5 个 admin handler 使用了 `Path(user_id): Path<Uuid>` 和 `Path(request_id): Path<Uuid>`，但第 12 行的 import 语句中缺少 `Path`。

**当前 imports** (auth.rs:12-18):
```rust
use axum::{
    extract::{Extension, State},
    ...
};
```

- [x] **T1.1** 在 `auth.rs` 的 `extract` import 中添加 `Path`：
  ```rust
  use axum::{
      extract::{Extension, Path, State},
      ...
  };
  ```
  - 文件: `crates/api/src/routes/auth.rs`
  - 修改: 第 13 行 `Extension, State` → `Extension, Path, State`
  - 验证: `cargo check -p api` 编译通过

---

## 2. cloud_upstreams.rs — OAuth + Push Pipeline 补全

### 2.1 OAuth 端点发现 (fetchDiscovery) 🔴

**Paperclip 参考** (`server/src/services/cloud-upstreams.ts:648-663`):
```typescript
async function fetchDiscovery(remoteUrl: string): Promise<Record<string, unknown>> {
  const parsed = new URL(remoteUrl);
  // HTTPS 校验（localhost 例外）
  const stackId = firstPathSegment(parsed.pathname);
  const discoveryUrl = new URL("/.well-known/paperclip-upstream", parsed.origin);
  if (stackId) discoveryUrl.searchParams.set("stackId", stackId);
  const response = await fetchWithTimeout(discoveryUrl, undefined, DISCOVERY_FETCH_TIMEOUT_MS);
  if (!response.ok) throw badRequest(`Cloud upstream discovery failed: ${response.status}`);
  return await response.json();
}
```

**Rust 端当前状态**: `start_cloud_connect` 直接硬编码 `{remote_url}/oauth/authorize?...`，不进行端点发现。

- [x] **T2.1.1** 实现 `fetch_discovery(remote_url: &str) -> Result<DiscoveryResponse>` 函数
  - 解析 remote_url，提取 origin 和 stackId（取 path 第一段）
  - 构造 `/.well-known/paperclip-upstream` 发现 URL
  - 仅允许 HTTPS（localhost/127.0.0.1 例外）
  - 使用 `reqwest` 发送 GET 请求，超时 10s
  - 解析 JSON 响应为 `DiscoveryResponse` 结构体
  - 文件: `crates/api/src/routes/cloud_upstreams.rs`（或新建 `crates/services/src/cloud_upstream_service.rs`）
  - 依赖: 添加 `reqwest` 到 `Cargo.toml`（如尚未添加）

- [x] **T2.1.2** 定义 `DiscoveryResponse` 结构体
  ```rust
  struct DiscoveryResponse {
      schema: String,
      stack: DiscoveryStack,
      auth: DiscoveryAuth,
      transfer: DiscoveryTransfer,
  }
  struct DiscoveryStack { id: String, slug: Option<String>, display_name: Option<String>, company_id: String, origin: String }
  struct DiscoveryAuth { pkce: Option<DiscoveryPkce>, device_code: Option<DiscoveryDeviceCode>, scopes: Option<Vec<String>> }
  struct DiscoveryPkce { authorize_url: String, token_url: String, code_challenge_method: String }
  struct DiscoveryTransfer { supported_schema_major: i32, feature_flags: Option<Vec<String>> }
  ```

- [x] **T2.1.3** 修改 `start_cloud_connect` 调用 `fetch_discovery` 获取端点信息
  - 从 discovery 获取 `authorize_url`（而非硬编码）
  - 从 discovery 获取 `token_url` 并持久化到 `pending_token_url`
  - 从 discovery 提取 `target` 信息（stackId, stackSlug, companyId, origin 等）

### 2.2 Ed25519 密钥对生成 🔴

**Paperclip 参考** (`cli/src/commands/client/cloud.ts`):
```typescript
const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519", {
    publicKeyEncoding: { type: "spki", format: "pem" },
    privateKeyEncoding: { type: "pkcs8", format: "pem" },
});
```

- [x] **T2.2.1** 在 `start_cloud_connect` 中生成 Ed25519 密钥对
  - 使用 `ed25519-dalek` crate 生成密钥对
  - 公钥导出为 PEM (SPKI) 格式
  - 私钥导出为 PEM (PKCS8) 格式
  - 公钥指纹: SHA256(public_key_pem) → hex

- [x] **T2.2.2** 持久化密钥和源实例信息
  - `source_instance_id`: 读取 `INSTANCE_ID` 环境变量
  - `source_instance_fingerprint`: 公钥 SHA256 指纹
  - `source_public_key`: 公钥 PEM
  - `private_key_pem`: 私钥 PEM（需加密存储，参考 Paperclip 的 `sealCloudUpstreamCredential`）

- [x] **T2.2.3** 持久化 target 信息
  - `target_stack_slug`: 从 discovery 获取
  - `target_company_id`: 从 discovery 获取
  - `target_origin`: 从 discovery 获取
  - `target_schema_major`: 从 discovery transfer 获取

- [x] **T2.2.4** 修改 authorization URL 构造
  - 使用 discovery 返回的 `authorize_url`（而非硬编码）
  - 添加 Paperclip 必需的 query 参数: `stackId`, `sourceInstanceId`, `sourceInstanceFingerprint`, `sourcePublicKey`, `scopes`

### 2.3 OAuth Token 交换 (finish_cloud_connect) 🔴

**Paperclip 参考** (`server/src/services/cloud-upstreams.ts:214-251`):
```typescript
// 1. 验证 state 匹配
// 2. POST 到 pendingTokenUrl 交换 token
const tokenResponse = await postJson(pending.pendingTokenUrl, {
    grantType: "authorization_code",
    code: input.code,
    redirectUri: pending.pendingRedirectUri,
    codeVerifier: await unsealCloudUpstreamCredential(pending.pendingCodeVerifier),
});
// 3. 持久化 accessToken（加密）、tokenId、expiresAt
```

- [x] **T2.3.1** 修改 `finish_cloud_connect` 接收 `code` 参数
  - 当前只接收 `pendingConnectionId` + `state`
  - 添加 `code: String` 参数

- [x] **T2.3.2** 实现 OAuth token 交换
  - 查询 connection 的 `pending_token_url`, `pending_code_verifier`, `pending_redirect_uri`
  - 向 `pending_token_url` 发送 POST 请求（Content-Type: application/json）
  - Body: `{ grantType: "authorization_code", code, redirectUri, codeVerifier }`
  - 使用 `reqwest` 发送请求

- [x] **T2.3.3** 持久化 token 响应
  - `access_token`: 加密存储（使用与 Paperclip 兼容的加密方式）
  - `token_id`: token 标识
  - `token_expires_at`: token 过期时间
  - `authorized_global_user_id`: 授权用户 ID（从 token 响应提取）
  - `scopes`: 授权范围

- [x] **T2.3.4** 清除 pending 字段并更新状态
  - `pending_state = NULL`
  - `pending_code_verifier = NULL`
  - `pending_redirect_uri = NULL`
  - `pending_token_url = NULL`
  - `status = 'connected'`
  - `token_status = 'connected'`

### 2.4 execute_push_run 完整 Push Pipeline 🔴

**Paperclip 参考** (`server/src/services/cloud-upstreams.ts:278-419`):
1. 检查 connection 状态为 `"connected"`
2. 检查无其他 running run（`assertNoRunningRun`，含 `FOR UPDATE` 锁）
3. 执行 `localPreview` 检查 schema 兼容性
4. 调用 `buildBundle` 构建传输包
5. 插入 run 记录（含 summary, warnings, conflicts, events, idempotencyKey, manifestHash）
6. 远程 POST `/api/companies/:targetCompanyId/upstream-imports/runs`
7. 分块上传 chunks
8. 远程 POST `/api/upstream-import-runs/:remoteRunId/apply`
9. 更新 run 为 `"succeeded"` 或 `"failed"`

- [x] **T2.4.1** 实现 `local_preview` 逻辑
  - 检查 connection.target_schema_major 与本地 TRANSFER_SCHEMA.major 是否兼容
  - 生成 summary（各实体类型计数: agents, projects, goals, issues, comments, routines）
  - 生成 warnings（schema 不兼容时警告）
  - 返回 `CloudUpstreamPreview`

- [x] **T2.4.2** 实现 `build_bundle` 逻辑
  - 调用 `ExportService.export` 导出公司数据
  - 构建 `UpstreamTransferManifest`（source/target 信息、实体列表、分块元数据）
  - 生成 `idempotency_key`（UUID v4）
  - 计算 `manifest_hash`（manifest JSON 的 SHA256）

- [x] **T2.4.3** 实现远程 push 流程
  - POST 到 `{target_origin}/api/companies/{target_company_id}/upstream-imports/runs` 创建远程 run
  - 分块上传: 遍历 chunks，每个 chunk POST 到远程
  - POST 到 `{target_origin}/api/upstream-import-runs/{remote_run_id}/apply` 触发应用
  - 使用 `source_private_key` 签名请求（cloudProofHeaders）

- [x] **T2.4.4** 实现异步执行和状态更新
  - 将 push pipeline 包装为 `tokio::spawn` 异步任务
  - 实时更新 `active_step`, `progress_percent`, `summary`, `warnings`, `conflicts`, `events`
  - 完成时更新 `status = 'succeeded'` 或 `status = 'failed'`，设置 `completed_at`

### 2.5 cancel_push_run 远程取消 🟡

- [x] **T2.5.1** 检查 run 是否有 `remote_run_id`
  - 如果有，向远程发送 `POST {target_url}/api/upstream-import-runs/{remote_run_id}/cancel`
  - 同时更新本地 run 状态为 `cancelled`

### 2.6 activate_push_run 状态检查 🟡

- [x] **T2.6.1** 在 UPDATE 之前检查 run.status === 'succeeded'
  - Paperclip 参考: `if (row.status !== "succeeded") throw badRequest(...)`
  - 当前 SQL 的 WHERE 子句中已添加 `AND status = 'succeeded'`，但 Rust 端未显式检查
  - 添加显式检查并在不符合时返回 409 Conflict

### 2.7 权限检查加强 🔴

- [x] **T2.7.1** 在每个 handler 中添加 `assertCompanyAccess` 检查
  - Paperclip 参考: 每个 cloud-upstreams handler 都有 `assertBoardOrgAccess(req)` + `assertCompanyAccess(req, companyId)`
  - 当前仅有 `require_authenticated` 中间件（检查非匿名）
  - 需添加: 从 `AuthorizationActor` 提取 `company_id` 并与请求的 `company_id` 比对

---

## 3. companies.rs — 权限检查 + 业务逻辑补全

### 3.1 权限检查 🔴

**Paperclip 参考** (`server/src/routes/authz.ts:74-120`):
```typescript
export function assertCompanyAccess(req: Request, companyId: string) {
  assertAuthenticated(req);
  if (req.actor.type === "agent" && req.actor.companyId !== companyId)
    throw forbidden("Agent key cannot access another company");
  if (req.actor.type === "board" && req.actor.source !== "local_implicit") {
    if (!allowedCompanies.includes(companyId))
      throw forbidden("User does not have access to this company");
    // 非 GET 方法需要 active membership 且不能是 viewer
  }
}
```

- [x] **T3.1.1** 实现 `assert_company_access(actor: &AuthorizationActor, company_id: Uuid) -> Result<(), AppError>`
  - Agent 类型: 检查 `actor.company_id == company_id`
  - Board 类型: 检查 `actor.company_ids` 包含 `company_id`
  - 非 GET 方法: 检查 membership role 非 viewer
  - 文件: `crates/api/src/routes/companies.rs`（或新建 `crates/api/src/authz.rs`）

- [x] **T3.1.2** 在所有 companies handler 开头调用 `assert_company_access`
  - `get_company_timeline`, `get_company_artifacts`, `list_company_feedback_traces`
  - `export_company`, `preview_company_export`
  - `preview_company_import`, `apply_company_import`
  - `list_inbox_dismissals`, `dismiss_inbox_item`
  - `get_teams_catalog`
  - 每个 handler 添加: `Extension(actor): Extension<AuthorizationActor>` 参数

### 3.2 Timeline handler 对齐 Paperclip 🟡

**Paperclip 参考** (`server/src/services/work-timeline.ts:463`):
- `WorkTimelineQuery` 参数: `companyId`, `from`, `to`, `limit`, `offset`, `userId`, `goalId`, `projectId`, `issueId`, `canReadIssue`
- 使用 `workTimelineService` 而非直接查 activity_logs
- 对每个 issue 执行 `canReadIssue` 权限过滤

- [x] **T3.2.1** 扩展 `TimelineQuery` 参数
  - 添加 `user_id: Option<Uuid>`
  - 添加 `goal_id: Option<Uuid>`
  - 添加 `project_id: Option<Uuid>`
  - 保留已有的 `issue_id`, `from`, `to`, `limit`, `offset`

- [x] **T3.2.2** 实现 `WorkTimelineService` trait 和默认实现
  - `collect_issue_ids`: 从 issues, heartbeat_runs, activity_logs, issue_comments, issue_thread_interactions, issue_approvals 多源收集相关 issue ID
  - `load_issues`: 加载 issue 详情
  - `apply_user_lens`: 按 user_id 过滤
  - `filter_readable_issues`: 执行 canReadIssue ACL 检查
  - 文件: `crates/services/src/work_timeline_service.rs`

- [x] **T3.2.3** 修改 `get_company_timeline` 使用 `WorkTimelineService`
  - 不再直接查询 `activity_logs`
  - 注入 `WorkTimelineService` 到 `AppState`
  - 返回完整的 `WorkTimelineResult`（actors, spans, events, edges, pagination, window）

### 3.3 占位符 Handler 补全 🟡

- [x] **T3.3.1** `record_company_activity` — 对接 activity_logs 持久化
  - 当前返回 `{"recorded": true}` 占位符
  - 实现: 写入 `activity_logs` 表，字段包括 company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata

- [x] **T3.3.2** `update_member_permissions` — 对接 MemberService
  - 当前返回 `{"updated": true}` 占位符
  - 实现: 更新 `company_members` 表的权限字段

- [x] **T3.3.3** `get_sidebar_preferences` / `update_sidebar_preferences` — 对接 UserPreferenceService
  - 当前返回空 `{}` 占位符
  - 实现: 从 `user_preferences` 表读写当前用户的侧边栏偏好

- [x] **T3.3.4** `get_user_profile` — 对接 UserProfileService
  - 当前返回空 `{}` 占位符
  - 实现: 从 `auth_users` 表查询用户公开资料（name, avatar_url, email 脱敏）

### 3.4 Teams Catalog 语义修复 ✅/🟡

- [x] **T3.4.1** 实现真实的 teams-catalog 逻辑
  - 当前返回空数组（已修复语义错误，但无实际数据）
  - Paperclip 参考: teams-catalog 是预定义的团队目录包清单
  - 实现方案: 从 `plugins` 表中查询 type='team-catalog' 的插件 manifest 列表
  - 如无此类数据，保持空数组但添加注释说明

---

## 4. plugins.rs / plugin_service.rs — 权限 + 子服务补全

### 4.1 细粒度权限检查 🔴

**Paperclip 参考** (`server/src/routes/authz.ts`):
- `assertBoard` — 大多数读操作
- `assertInstanceAdmin` — install/uninstall/upgrade
- `assertBoardOrAgent` — tool 执行
- `assertCompanyAccess` — company-scoped 操作

- [x] **T4.1.1** 实现 `assert_board(actor: &AuthorizationActor) -> Result<(), AppError>`
  - 检查 actor 类型为 Board（非 Agent、非 None）

- [x] **T4.1.2** 实现 `assert_instance_admin(actor: &AuthorizationActor) -> Result<(), AppError>`
  - 检查 Board + `is_instance_admin == true` 或 `source == LocalImplicit`

- [x] **T4.1.3** 实现 `assert_board_or_agent(actor: &AuthorizationActor) -> Result<(), AppError>`
  - Agent 类型直接放行
  - Board 类型调用 `assert_board_org_access`

- [x] **T4.1.4** 在 plugins handler 中添加权限检查
  - `install_plugin`, `delete_plugin`, `upgrade_plugin`: 需要 `assert_instance_admin`
  - `execute_plugin_tool`, `bridge_plugin_action`, `trigger_plugin_action`: 需要 `assert_board_or_agent`
  - `list_plugins`, `get_plugin`, `get_plugin_health`, `get_plugin_logs`, `get_plugin_dashboard`, `get_plugin_config`: 需要 `assert_board`
  - `store_plugin_data`, `trigger_plugin_job`: 需要 `assert_board`

### 4.2 插件执行引擎 🟡

- [x] **T4.2.1** `execute_plugin_tool` — 实现真实工具调度
  - 当前: 回显 `{"tool": tool, "result": parameters}`
  - 目标: 调用 `PluginToolDispatcher` 通过 JSON-RPC 与 worker 进程通信
  - 实现方案: 查找 plugin 的 worker 进程 → 发送 JSON-RPC `tools/call` 请求 → 返回结果
  - 短期方案（如 worker 未就绪）: 至少从 `plugin_data` 表查找已注册的工具定义并验证参数

- [x] **T4.2.2** `bridge_plugin_action` — 实现消息转发
  - 当前: 返回 `{"accepted": true}`
  - 目标: 通过 `PluginStreamBus` 向 worker 发送 action 消息

- [x] **T4.2.3** `trigger_plugin_action` — 实现触发器执行
  - 当前: 返回 `{"accepted": true}`
  - 目标: 查找 plugin 注册的 action → 通过 `PluginToolDispatcher` 执行

### 4.3 插件子服务补全 🟡

- [x] **T4.3.1** 实现 `PluginLoader` — manifest 解析和能力发现
  - 文件: `crates/services/src/plugin_loader.rs`
  - 功能: 从 `plugin.manifest` JSON 解析 `tools`, `actions`, `jobs`, `uiContributions`, `capabilities`
  - 验证 manifest schema

- [x] **T4.3.2** 实现 `PluginLifecycle` — 安装/卸载/启用/禁用/升级
  - 文件: `crates/services/src/plugin_lifecycle.rs`
  - 功能: 状态机 transition（installed → enabled → disabled → uninstalled）
  - `install`: 加载 manifest → 注册工具 → 初始化配置
  - `uninstall`: 清理数据 → 注销工具
  - `enable`/`disable`: 切换状态，不清理数据
  - `upgrade`: 备份旧版本 → 安装新版本 → 迁移数据

- [x] **T4.3.3** 实现 `PluginToolDispatcher` — 工具注册与执行
  - 文件: `crates/services/src/plugin_tool_dispatcher.rs`
  - 功能: 工具注册表（HashMap<plugin_key, Vec<ToolDefinition>>）
  - `dispatch(plugin_key, tool_name, params)`: 查找工具 → 验证参数 → 执行

- [x] **T4.3.4** 实现 `PluginConfigValidator` — 配置校验
  - 文件: `crates/services/src/plugin_config_validator.rs`
  - 功能: 根据 manifest 中的 configSchema 校验用户提交的配置

---

## 5. monitor_scheduler.rs — run_loop 启动修复

### 🟡 问题: `start()` 中 `run_loop()` 已正确调用，但需确认

**当前代码** (`monitor_scheduler.rs:152-158`):
```rust
let runner = Arc::new(Self { ... });
tokio::spawn(runner.run_loop());
```

**评价**: 检查报告指出 `run_loop` 仍为占位实现。重新审查代码发现 `start()` 方法**已经**通过 `tokio::spawn(runner.run_loop())` 启动。但需确认：

- [x] **T5.1** 验证 `run_loop()` 是否真正运行
  - 检查 `run_loop` 方法签名: `async fn run_loop(self: Arc<Self>)` 是否正确
  - 确认 `tokio::spawn` 捕获的 `runner: Arc<Self>` 生命周期正确
  - 验证: 启动服务后检查日志中是否有 "Monitor scheduler started" 和周期性 poll 日志
  - 如 `run_loop` 未运行，修复 `Arc` 捕获方式

- [x] **T5.2** 改进 `poll_due_issues` 查询精确度
  - 当前使用 `IssueQueryFilter` 查询所有 issues，未过滤 `monitor_next_check_at <= NOW()`
  - 添加 `monitor_next_check_at` 时间过滤

---

## 6. saga_orchestrator.rs — initiator_id 持久化改进

### 🟡 问题: `__saga_meta` 嵌入 context 有命名冲突风险

**Paperclip 参考**: initiator_id 是 SagaContext 的独立字段，存储在 saga 记录中。

- [x] **T6.1** 在 saga 表中添加 `initiator_id` 列
  - Migration: 添加 `initiator_id UUID` 列到 `saga_instances` 表
  - 与 `__saga_meta` 嵌入方式并存（向后兼容）

- [x] **T6.2** 修改 saga 创建逻辑
  - 优先将 `initiator_id` 写入独立的 `initiator_id` 列
  - 保留 `__saga_meta` 写入作为向后兼容（可在后续版本移除）

- [x] **T6.3** 修改补偿逻辑
  - 优先从 `initiator_id` 列读取
  - 回退到 `__saga_meta` JSON 解析（向后兼容旧 saga）
  - 添加 warning 日志当从 JSON 回退读取时

---

## 7. CloudUpstreamService 分层架构

### 🔴 问题: cloud_upstreams.rs 直接操作 pool 绕过 service 层

**Paperclip 参考**: `cloudUpstreamService(db, options)` 是独立 service。

- [x] **T7.1** 创建 `CloudUpstreamService` trait
  - 文件: `crates/services/src/cloud_upstream_service.rs`
  - 方法: `list`, `start_connect`, `finish_connect`, `preview`, `create_run`, `read_run`, `cancel_run`, `activate_entities`
  - 每个方法接收 `company_id: Uuid` 进行权限隔离

- [x] **T7.2** 实现 `DefaultCloudUpstreamService`
  - 持有 `PgPool` 和 `reqwest::Client`
  - 将当前 `cloud_upstreams.rs` 中的 SQL 查询和业务逻辑迁移到 service 层
  - 添加 `InstanceSettingsService` 依赖（用于 `ensure_cloud_sync`）

- [x] **T7.3** 在 `AppState` 中注册 `CloudUpstreamService`
  - 添加 `cloud_upstream_service: Arc<dyn CloudUpstreamService>`
  - 在 `main.rs` 中初始化

- [x] **T7.4** 修改 `cloud_upstreams.rs` handler 调用 service 层
  - 所有 handler 改为调用 `state.cloud_upstream_service.xxx()` 而非直接 `sqlx::query`

---

## 8. Plugin 模型字段补全

### 🟡 问题: Plugin 模型缺少 api_version / categories / install_order

- [x] **T8.1** 添加 `api_version: i32` 字段
  - 文件: `crates/models/src/plugin.rs`
  - 用于 API 兼容性检查

- [x] **T8.2** 添加 `categories: serde_json::Value` 字段
  - JSONB 数组，如 `["ai", "automation"]`

- [x] **T8.3** 添加 `install_order: i32` 字段
  - 控制插件加载顺序

- [x] **T8.4** 重命名 `manifest` 为 `manifest_json`（可选，与 Paperclip 命名一致）
  - 或在序列化时使用 `#[serde(rename = "manifestJson")]`

- [x] **T8.5** 更新 migration 添加新列
  - `ALTER TABLE plugins ADD COLUMN api_version INTEGER`
  - `ALTER TABLE plugins ADD COLUMN categories JSONB DEFAULT '[]'`
  - `ALTER TABLE plugins ADD COLUMN install_order INTEGER`

- [x] **T8.6** 更新 `install_plugin` 写入新字段

---

## 9. migrations 表结构补全

### 🟡 问题: cloud_upstream_connections 和 cloud_upstream_runs 缺少关键列

- [x] **T9.1** 确认已存在的列
  - 根据检查报告，以下列已添加: `source_instance_id`, `source_instance_fingerprint`, `source_public_key`, `private_key_pem`, `target_stack_slug`, `target_company_id`, `target_origin`, `pending_token_url`, `authorized_global_user_id`
  - `dry_run`, `idempotency_key`, `manifest_hash`, `target_url`, `remote_run_id`, `retry_of_run_id`, `completed_at`

- [x] **T9.2** 如缺失，补充 cloud_upstream_connections 列
  - `source_instance_id TEXT`
  - `source_instance_fingerprint TEXT`
  - `source_public_key TEXT`
  - `private_key_pem TEXT`（加密存储）
  - `target_stack_slug TEXT`
  - `target_company_id UUID`
  - `target_origin TEXT`
  - `target_schema_major INTEGER`
  - `pending_token_url TEXT`
  - `authorized_global_user_id UUID`

- [x] **T9.3** 如缺失，补充 cloud_upstream_runs 列
  - `dry_run BOOLEAN DEFAULT false`
  - `idempotency_key UUID`
  - `manifest_hash TEXT`
  - `target_url TEXT`
  - `remote_run_id UUID`
  - `retry_of_run_id UUID`
  - `completed_at TIMESTAMPTZ`

---

## 10. task.md 状态修正

- [x] **T10.1** 修正 Round 4.3 cloud_upstreams 状态
  - `start_cloud_connect` → `🟡 部分完成`（缺少 OAuth 发现、Ed25519 密钥）
  - `finish_cloud_connect` → `🟡 部分完成`（缺少 token 交换）
  - `execute_push_run` → `🟡 部分完成`（缺少完整 pipeline）
  - `cancel_push_run` → `🟡 部分完成`（缺少远程取消）

- [x] **T10.2** 修正 Round 4.4 plugins 状态
  - 4.4.3 `execute_plugin_tool` → `🟡 部分完成`（回显而非实际执行）
  - 4.4.4 `bridge_plugin_action` → `🟡 部分完成`
  - 4.4.5 `trigger_plugin_action` → `🟡 部分完成`
  - 4.4.6 `plugin_loader/lifecycle/tool_dispatcher` → `❌ 未开始`

- [x] **T10.3** 添加新增文件到 task.md
  - `crates/models/src/plugin.rs`
  - `crates/services/src/plugin_service.rs`
  - `crates/services/src/company_portability_service.rs`
  - `migrations/20260719000003_create_plugins.sql`

---

## 附录 A: 修复优先级矩阵

| 优先级 | Task | 文件 | 工作量估计 |
|--------|------|------|-----------|
| 🔴 P0 | T1.1 编译修复: auth.rs 添加 Path import | auth.rs | 5 min |
| 🔴 P0 | T3.1.1-2 权限检查: assertCompanyAccess + handler 集成 | companies.rs, authz.rs | 2h |
| 🔴 P0 | T4.1.1-4 权限检查: assertBoard/assertInstanceAdmin/assertBoardOrAgent | plugins.rs, authz.rs | 2h |
| 🔴 P1 | T2.1.1-3 OAuth 端点发现 | cloud_upstreams.rs | 3h |
| 🔴 P1 | T2.2.1-4 Ed25519 密钥对生成 | cloud_upstreams.rs | 2h |
| 🔴 P1 | T2.3.1-4 OAuth Token 交换 | cloud_upstreams.rs | 3h |
| 🔴 P1 | T7.1-4 CloudUpstreamService 分层 | cloud_upstream_service.rs | 4h |
| 🟡 P2 | T2.4.1-4 Push Pipeline 完整流程 | cloud_upstreams.rs | 8h |
| 🟡 P2 | T2.5.1 cancel 远程取消 | cloud_upstreams.rs | 1h |
| 🟡 P2 | T2.6.1 activate 状态检查 | cloud_upstreams.rs | 30 min |
| 🟡 P2 | T3.2.1-3 Timeline 对齐 Paperclip | companies.rs, work_timeline_service.rs | 6h |
| 🟡 P2 | T3.3.1-4 占位符补全 | companies.rs | 3h |
| 🟡 P2 | T4.2.1-3 插件执行引擎 | plugins.rs, plugin_tool_dispatcher.rs | 6h |
| 🟡 P2 | T4.3.1-4 插件子服务 | plugin_loader.rs, plugin_lifecycle.rs 等 | 8h |
| 🟡 P2 | T8.1-6 Plugin 模型字段补全 | plugin.rs, migration | 2h |
| 🟡 P2 | T9.1-3 Migration 表结构补全 | migration | 1h |
| 🟢 P3 | T5.1-2 monitor_scheduler run_loop 验证 | monitor_scheduler.rs | 1h |
| 🟢 P3 | T6.1-3 saga initiator_id 改进 | saga_orchestrator.rs | 2h |
| 🟢 P3 | T3.4.1 teams-catalog 真实数据 | companies.rs | 1h |
| 🟢 P3 | T10.1-3 task.md 状态修正 | task.md | 30 min |

---

## 附录 B: 已完成项（供参考）

以下项目已在前一轮修复中完成，保留在此供对比：

- ✅ 认证中间件 `require_authenticated` 已添加到 cloud_upstreams / companies / plugins 路由组
- ✅ cloud_upstreams `start_cloud_connect` 已生成 PKCE code_verifier 和 challenge
- ✅ cloud_upstreams `finish_cloud_connect` 已验证 state 参数
- ✅ cloud_upstreams `execute_push_run` 已检查 connection 状态和 running run
- ✅ cloud_upstreams `activate_push_run` 已使用 jsonb_set 更新
- ✅ cloud_upstreams `get_push_run` 已加入 company_id 参数
- ✅ companies `export_company` 已改为 POST
- ✅ companies timeline/artifacts/feedback-traces 已从 DB 查询真实数据
- ✅ companies teams-catalog 已改为返回空数组（修复语义错误）
- ✅ companies export/import/inbox 已对接 ExportService/ImportService/InboxService
- ✅ plugins 所有 handler 已从占位符改为调用 PluginService
- ✅ Plugin 模型已添加 api_version, categories, install_order
- ✅ plugins `install_plugin` 已写入 DB 并支持 ON CONFLICT DO UPDATE
- ✅ plugins 状态机 transition 检查已实现
- ✅ pipeline `list_by_company` / `list_stages` / `list_transitions` 已实现
- ✅ monitor_scheduler 遍历所有公司 poll 架构正确
- ✅ approval `BudgetOverrideRequired` payload 校验已实现
- ✅ cloud_upstream_connections 表已添加 source_instance_id 等关键列
- ✅ cloud_upstream_runs 表已添加 dry_run 等关键列
- ✅ plugins 5 张表结构完整
- ✅ DefaultCompanyPortabilityService 三个实例共享同一个对象
- ✅ DefaultPluginService 已注册
- ✅ DefaultSkillRegistryServiceImpl 接收 LOCAL_TRUSTED_USER_ID
