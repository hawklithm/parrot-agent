# 统一认证中间件差异分析与修改方案

## 一、两个项目的统一认证中间件概览

### 1. Paperclip (TypeScript/Express) — 参考标准

Paperclip 使用 Express 中间件 `actorMiddleware`，单次请求完整遍历所有认证源：

```
请求 → actorMiddleware
  ├─ [1] 初始化默认 actor（local_trusted 模式 = local-board; authenticated 模式 = none）
  ├─ [2] 无 Bearer Token 时：
  │   ├─ [2a] CloudTenant 头部认证（x-paperclip-cloud-*）
  │   ├─ [2b] Session Cookie 认证（BetterAuth 框架）
  │   └─ [2c] 匿名通过
  └─ [3] 有 Bearer Token 时：
      ├─ [3a] Board API Key (bak_*)
      ├─ [3b] Agent API Key (aak_*) → 哈希匹配，查 DB
      └─ [3c] Agent JWT → 验签 + RunID 校验 + onBehalfOf 委托
```

**核心特征：**
- **BetterAuth 集成**：完整的用户认证框架（email/password、session、cookie 前缀派生）
- **CloudTenant 支持**：基于 `x-paperclip-cloud-*` 头部的多租户自动 upsert 用户/公司/成员关系
- **Agent Key Scope 细分**：`standard` / `task_bridge` / `skill_test` 三种 Agent API Key 作用域
- **RunID 校验**：Agent JWT 中 run_id 与 `x-paperclip-run-id` 头部必须一致
- **Responsible User 委托**：Agent 必须绑定 responsible user，权限检查时使用 onBehalfOf 委托
- **审计日志**：JWT 拒绝、Key 缺失 responsible user 等均有审计事件
- **instance_admin 清除**：CloudTenant 用户不会自动获得 instance_admin（主动清理旧角色）
- **role 默认授予**：CloudTenant 用户通过 `ensureHumanRoleDefaultGrants` 自动获得角色权限

### 2. Parrot-Agent (Rust/Axum) — 当前实现

Parrot-Agent 使用 Axum 中间件 `AuthMiddleware` + 策略模式 `ActorResolver`：

```
请求 → AuthMiddleware.resolve_actor()
  ├─ 按优先级遍历所有 Resolver（priority 从高到低）
  │   ├─ [1] BearerTokenResolver (priority=10): bak_/aak_/JWT 分派
  │   ├─ [2] SessionCookieResolver (priority=5): Session Cookie
  │   ├─ [3] CloudTenantHeaderResolver (priority=3): x-paperclip-cloud-*
  │   └─ [4] LocalTrustedResolver (priority=0): 默认身份（仅 local_trusted 模式）
  └─ 模式行为：
      ├─ LocalTrusted: 至少一个解析器成功 → Ok(actor); 全部失败 → Err
      └─ Authenticated: 至少一个解析器成功 → Ok(actor); 全部失败 → Ok(None)
```

**核心特征：**
- **策略模式架构**：`ActorResolver` trait + 优先级排序，可扩展
- **AuthMiddleware 聚合器**：按模式（LocalTrusted/Authenticated）决定全部失败时的行为
- **BearerTokenResolver 一体化**：同时处理 Board API Key、Agent API Key、Agent JWT
- **CloudTenant 支持**：基于 `x-paperclip-cloud-stack-id/role` 派生用户和公司
- **基本审计**：JWT 拒绝时有活动日志记录

---

## 二、关键差异详细对比

### 差异 1: 认证架构模式

| 维度 | Paperclip (Express) | Parrot-Agent (Axum) |
|------|-------------------|-------------------|
| 框架 | Express (Node.js) | Axum (Rust) |
| 架构模式 | 单体中间件函数，顺序分支 | 策略模式 + 优先级遍历 |
| 扩展性 | 需修改中间件函数内部 | 新增 Resolver 实现即可 |
| 代码组织 | 单文件 530 行 | 多模块（actor/middleware/board_access/jwt 等） |
| 认证源数量 | 6 种（CloudTenant/Session/BoardKey/AgentKey/AgentJWT/LocalImplicit） | 4 种 Resolver + 3 种子类型在 BearerTokenResolver 内 |

**结论**：Parrot-Agent 的策略模式架构更优（扩展性好），但存在一些细节差异需要对齐。

### 差异 2: BetterAuth 集成 — 🔴 重大缺失

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| 认证框架 | BetterAuth（完整实现） | 无 BetterAuth 集成 |
| Session 管理 | BetterAuth cookie + drizzle adapter | 简单的 session token cookie 解析 |
| Email/Password | 支持注册/登录/登出 | 未实现 |
| Cookie 前缀 | 按 instance_id 派生（`paperclip-{id}`） | 无 |
| Secure Cookie | 根据部署模式自动判断 | 无 |
| Trusted Origins | 动态推导 | 无 |
| Base URL 模式 | 支持 explicit/auto | 无 |

**结论**：Parrot-Agent 缺少完整的 BetterAuth 认证框架集成，当前 SessionCookieResolver 仅做简单的 cookie 中 token 解析，缺少：
- 用户注册/登录/登出 API
- Cookie 安全策略（Secure/HttpOnly/SameSite）
- Session 过期/续期管理
- 多实例 Cookie 前缀隔离

### 差异 3: CloudTenant 认证 — 🟡 部分差异

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| 触发条件 | 无 Bearer Token + authenticated 模式 | 总是尝试解析（priority=3） |
| 头部字段 | `x-paperclip-cloud-user-id/email/name/stack-id/role/token` | `x-paperclip-cloud-stack-id/role` |
| 用户创建 | 按传入 userId 直接 upsert authUsers | 按 stack_id 派生 UUID |
| 公司创建 | 按 stack_id SHA256 派生 UUID | 按 stack_id 派生 UUID |
| instance_admin | 主动清除旧 instance_admin 角色 | 无处理 |
| 角色默认授予 | `ensureHumanRoleDefaultGrants` | 仅创建成员关系 |
| Token 验证 | constant-time 比较 `x-paperclip-cloud-tenant-token` | 无 token 验证 |
| 成员角色映射 | owner/admin → owner; 其余 → member/support | owner/admin → Owner; 其余 → Operator |

**结论**：Parrot-Agent 的 CloudTenant 实现缺少：
- Cloud Tenant Token 验证（安全漏洞）
- 用户信息完整传递（只传 stack_id，不传 userId/email/name）
- instance_admin 角色清理逻辑
- role 默认权限授予逻辑

### 差异 4: Agent Key Scope 系统 — 🔴 重大缺失

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| Scope 类型 | `standard` / `task_bridge` / `skill_test` | `AgentApiKeyScope` 仅含 agent_id + company_id |
| Scope 验证 | Zod schema 校验 | 无 schema 校验 |
| 序列化存储 | JSON 字段持久化 | 当前未持久化 scope 字段 |
| 创建 API | `createAgentKeySchema`（含 name + scope） | 未实现 |

**结论**：Parrot-Agent 的 AgentApiKeyScope 结构过于简单，缺少：
- `task_bridge` scope（含 projectId/parentIssueId 边界限制）
- `skill_test` scope（含 issueId 限定）
- Scope 的 schema 校验和序列化

### 差异 5: Agent JWT 处理 — 🟡 部分差异

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| RunID 校验 | 严格：JWT run_id 与 header x-paperclip-run-id 必须一致，不一致返回 422 + 审计 | 基本：仅检查 agent 状态是否为 Running |
| Responsible User | 支持 legacy run 回退查询 + 成员关系加载 | 直接从 agent.reports_to 获取 |
| JWT Claims | 含 adapter_type, key_scope, instance_id, jti | 含 adapter_type, key_scope，无 instance_id/jti |
| 签名密钥派生 | `deriveCompanySigningKey(instanceId, companyId)` | 直接使用配置的 secret |
| 签发者/受众 | 可配置 + 校验 | 可配置 |
| Legacy 回退 | `PAPERCLIP_AGENT_JWT_DISABLE_LEGACY_FALLBACK` 开关 | 无 |

**结论**：Parrot-Agent 缺少：
- RunID 严格校验（含 header 一致性检查和审计）
- 签名密钥按 instance+company 派生
- Legacy run responsible user 回退查询
- instance_id / jti 声明支持

### 差异 6: Board API Key 管理 — 🟡 部分差异

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| Key 创建 | 通过 `boardAuthService.createNamedBoardApiKey` + CLI auth challenge | 基础 CRUD |
| Key 过期 | 支持 expires_at 检查 | 支持 expires_at 检查 |
| Key 撤销 | 支持 revokedAt | 支持 is_revoked 标志 |
| Key 命名 | 支持 name 字段 | 支持 name 字段 |
| CLI 认证挑战 | 完整流程（secret hash, pending key, polling） | 有 CliAuthChallenge 模型但流程不完整 |
| Key 前缀 | `bak_` 前缀 | `bak_` 前缀 |

**结论**：Parrot-Agent 的 Board API Key 管理基本对齐，但 CLI 认证挑战流程不如 Paperclip 完整。

### 差异 7: 审计日志 — 🟡 部分差异

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| JWT 拒绝 | 记录 activityLog（含 method/url/run_id/reason） | 记录 activity（含 event/reason/run_id） |
| Key 缺失 responsible user | 记录 activityLog | 无 |
| Key 过期拒绝 | 记录 activityLog | 无 |
| RunID 不匹配 | 记录 activityLog | 无 |

**结论**：Parrot-Agent 的审计日志覆盖不完整。

### 差异 8: 中间件注册方式

| 维度 | Paperclip | Parrot-Agent |
|------|-----------|--------------|
| 注册层级 | 全局 `app.use(actorMiddleware(db, opts))` | 仅 `/api/auth` 路由组挂载 `auth_middleware_fn` |
| 影响范围 | 所有请求均经过认证中间件 | 仅 `/api/auth/*` 路由经过认证中间件 |
| 其他路由 | 所有路由均可从 `req.actor` 读取身份 | 其他路由无认证层 |

**结论**：这是最关键的架构差异！Paperclip 的 `actorMiddleware` 是全局中间件，所有请求都会被认证；Parrot-Agent 的 `auth_middleware_fn` 仅在 `/api/auth` 路由组使用。这意味着 Parrot-Agent 的大部分 API 路由（agents/issues/companies 等）**没有认证保护**。

---

## 三、需要修改的内容 — 详细 Task 清单

### 阶段一：认证中间件全局化 🔴 高优先级

- [x] **T1.1** 将 `auth_middleware_fn` 从 `/api/auth` 路由组提升到全局 `/api` 层
  - 修改 `crates/api/src/app_state.rs` 的 `create_router` 函数
  - 在 `.nest("/api", api_routes)` 之前添加 `.layer(axum::middleware::from_fn_with_state(mw, auth_middleware_fn))`
  - 将 `AuthMiddleware` 实例构造提前到 `create_router` 层级

- [x] **T1.2** 实现 `DeploymentMode` 枚举与配置加载
  - 在 `crates/services/src/auth/middleware.rs` 中确认 `AuthMode` 已对齐 Paperclip 的 `DeploymentMode`（`local_trusted` / `authenticated`）
  - 添加从环境变量 `DEPLOYMENT_MODE` 读取部署模式的逻辑
  - 添加 `AuthMode` 的默认值（开发环境 `local_trusted`，生产环境 `authenticated`）

- [x] **T1.3** 将 `auth_middleware_fn` 中的认证失败行为与 Paperclip 对齐
  - `LocalTrusted` 模式：所有 Resolver 失败时返回 `LocalImplicit` 默认身份（而非 401）
  - `Authenticated` 模式：所有 Resolver 失败时返回 `AuthorizationActor::None`（匿名可访问）
  - 确认 Resolver 优先级排序与 Paperclip 一致：BearerToken > SessionCookie > CloudTenant > LocalTrusted

### 阶段二：BetterAuth 集成 🔴 高优先级

- [x] **T2.1** 引入 BetterAuth Rust 等价实现或自建 Session 管理
  - 方案A：使用 `axum-login` 或 `tower-sessions` crate 实现 session 管理
  - 方案B：自建完整的 session 中间件（推荐，与 Paperclip 行为对齐）
  - 实现 email/password 注册和登录 API（`POST /api/auth/sign-up/email`, `POST /api/auth/sign-in/email`）

- [x] **T2.2** 实现 Cookie 安全策略
  - 按 `instance_id` 派生 Cookie 前缀（如 `parrot-{instance_id}`）
  - 根据 `deploymentMode` + `deploymentExposure` 自动判断是否启用 Secure Cookie
  - 设置 HttpOnly、SameSite 属性

- [x] **T2.3** 实现 Session 过期与续期
  - Session 创建时设置 `expires_at`
  - 每次访问更新 `last_accessed_at`
  - 过期 Session 自动清理（定时任务或惰性清理）

- [x] **T2.4** 实现 Trusted Origins 推导
  - 从 `allowedHostnames` + `port` 动态推导 trusted origins 列表
  - 支持 `authBaseUrlMode` explicit/auto 两种模式

### 阶段三：CloudTenant 认证增强 🟡 中优先级

- [x] **T3.1** 添加 Cloud Tenant Token 验证
  - 读取环境变量 `PAPERCLIP_CLOUD_TENANT_SERVER_TOKEN`（或 parrot 等价变量）
  - 使用 constant-time 比较验证 `x-paperclip-cloud-tenant-token` 头部
  - Token 不存在或不匹配时跳过 CloudTenant 认证

- [x] **T3.2** 支持完整的 CloudTenant 用户信息传递
  - 新增解析 `x-paperclip-cloud-user-id`、`x-paperclip-cloud-user-email`、`x-paperclip-cloud-user-name` 头部
  - 当这些头部存在时，直接使用传入的 userId/email/name（而非从 stack_id 派生）
  - 保持 stack_id 派生作为 fallback

- [x] **T3.3** 实现 instance_admin 角色清理
  - CloudTenant 认证成功后，主动删除该用户的 `instance_admin` 角色（与 Paperclip 行为一致）
  - 防止旧部署残留的 instance_admin 权限泄露

- [x] **T3.4** 实现 role 默认权限授予
  - 新增 `ensureHumanRoleDefaultGrants` 函数
  - CloudTenant 用户创建后自动授予公司角色对应的默认权限

### 阶段四：Agent Key Scope 增强 🟡 中优先级

- [x] **T4.1** 扩展 `AgentApiKeyScope` 支持多种 Scope 类型
  - 添加 `TaskBridge` 变体（含 `project_id`, `parent_issue_id`, `allowed_assignee_agent_ids`）
  - 添加 `SkillTest` 变体（含 `issue_id`）
  - 保留现有 `Standard` 变体作为默认

- [x] **T4.2** 实现 Scope 的序列化/反序列化
  - 将 scope 字段持久化到 `agent_api_keys` 表
  - 添加 schema 校验（类似 Paperclip 的 Zod schema）
  - 实现 `normalize_agent_api_key_scope` 函数

- [x] **T4.3** 实现 Agent Key 创建 API 的 scope 参数
  - `POST /api/agents/:agentId/keys` 支持 `scope` 参数
  - 支持创建 `task_bridge` 和 `skill_test` 类型的 key

### 阶段五：Agent JWT 增强 🟡 中优先级

- [x] **T5.1** 实现 RunID 严格校验
  - 从请求头 `x-paperclip-run-id`（或 parrot 等价头 `x-parrot-run-id`）读取 run_id
  - JWT claims 中的 `run_id` 与 header 中的 `run_id` 必须一致
  - 不一致时返回 422 + 审计日志

- [x] **T5.2** 实现签名密钥按 instance+company 派生
  - 实现 `derive_company_signing_key(instance_id, company_id)` 函数
  - JWT 签发和验证均使用派生密钥

- [x] **T5.3** 添加 JWT Claims 扩展字段
  - 添加 `instance_id` 声明
  - 添加 `jti`（JWT ID）声明用于防重放
  - 添加 `responsible_user_id` 可选声明

- [x] **T5.4** 实现 Legacy Run Responsible User 回退查询
  - 当 JWT 不含 `responsible_user_id` 时，通过 `heartbeat_runs` 表回退查询
  - 支持 `PAPERCLIP_AGENT_JWT_DISABLE_LEGACY_FALLBACK` 开关

### 阶段六：审计日志完善 🟢 低优先级

- [x] **T6.1** 补充 Agent Key 缺失 responsible user 的审计日志
  - 在 `resolve_agent_key` 中，当 key 无 responsible user 时记录审计事件
  - 返回 403 错误（与 Paperclip 行为一致）

- [x] **T6.2** 补充 Key 过期拒绝的审计日志
  - 在 `resolve_board_key` 中，当 key 过期时记录审计事件

- [x] **T6.3** 补充 RunID 不匹配的审计日志
  - 与 T5.1 配套，记录 run_id mismatch 审计事件

### 阶段七：Express req.actor 对齐 🔵 代码质量

- [x] **T7.1** 统一 `AuthorizationActor` 的字段与 Paperclip 的 `req.actor` 对齐
  - Board 变体增加 `user_name`、`user_email`、`source`、`memberships`、`is_instance_admin` 字段（已部分实现，确认完整性）
  - Agent 变体增加 `key_id`、`key_scope`、`responsible_user_id`、`on_behalf_of_user_id`、`on_behalf_of_memberships`、`source` 字段（已部分实现，确认完整性）
  - 确保 `run_id` 字段在各变体中一致可用

- [x] **T7.2** 实现 `requireBoard` / `requireAgent` 守卫函数
  - 实现 `extract_board_actor` 从 request extensions 提取 Board 主体，非 Board 返回 403
  - 实现 `extract_agent_actor` 从 request extensions 提取 Agent 主体，非 Agent 返回 403

---

## 四、实施优先级建议

| 阶段 | 优先级 | 理由 |
|------|--------|------|
| 阶段一 | 🔴 最高 | 认证中间件全局化是安全基础，当前大部分 API 无认证保护 |
| 阶段二 | 🔴 最高 | BetterAuth 集成是用户认证的基础，无此功能无法支持多用户 |
| 阶段三 | 🟡 中 | CloudTenant 增强对 SaaS 多租户场景关键 |
| 阶段四 | 🟡 中 | Agent Key Scope 对 Agent 安全隔离关键 |
| 阶段五 | 🟡 中 | JWT 增强对 Agent 通信安全关键 |
| 阶段六 | 🟢 低 | 审计日志是合规需求，可后置 |
| 阶段七 | 🔵 代码质量 | 代码对齐提高可维护性 |
