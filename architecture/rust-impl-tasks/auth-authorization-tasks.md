# 认证授权模块 - Rust 实现任务拆解

> 基于 [后端架构分析文档](../backend/auth-authorization.md) 拆解，面向 Rust 实现。
> 版本: 2.0
> 日期: 2026/07/11

---

> **实现状态跟踪（2026/07/12 核对）**
>
> 代码位于 `parrot-agent/crates/`：`access`、`models`、`repositories`、`services/src/auth`、`api`。
> 已核对并标记完成：核心类型(§1)、JWT 模块(§3)、认证中间件骨架(§4 阶段一)、
> Board/Agent API Key Repository + 哈希(§5/§6 阶段一)、授权守卫 `assert_company_access`/`assert_instance_admin`/`assert_company_permission`(§7 阶段一)、
> Skills 系统 + 技能端点 + 邀请子资源 + 用户目录/OpenClaw 路由(§9)、安全 Header 中间件(§10)。
>
> **§5/§6 阶段二进展（2026/07/12 更新）**：
> - `resolve_board_access` 已实现（`services/src/auth/board_access.rs`）。
> - `load_responsible_user_memberships` 已实现（`services/src/auth/board_access.rs`）。
> - CLI 认证挑战流程已实现：`create_cli_auth_challenge` / `get_cli_auth_challenge` / `approve_cli_auth_challenge`（批准时创建 Board API Key 并返回明文 token）/ `cancel_cli_auth_challenge`，配套 `CliAuthChallengeRepository` + `PgCliAuthChallengeRepository`（`repositories/src/cli_auth_challenge_repository.rs`）。
> - Board 认领流程已实现：`BoardClaimService`（`create_board_claim_challenge` / `inspect_board_claim_challenge` / `claim_board_ownership` 事务：归档 local-board 成员关系 -> 移除其 instance_admin -> 认领用户置为 instance_admin -> 所有公司 owner）。
> - **§4/§6 Agent 认证中间件已落地（2026/07/12）**：`BearerTokenResolver` / `SessionCookieResolver` / `CloudTenantHeaderResolver` 的 `resolve()` 已从 TODO 桩补全为真实实现（Bearer 分派 bak_/aak_/JWT；Session 解析 cookie；CloudTenant 派生 upsert 用户/公司/成员）；`AgentApiKeyResolver`（agent_key 来源：hash 查 key -> 校验 agent Running -> 加载 responsible user memberships -> 构造 `AuthorizationActor::Agent`）与 `AgentJwtResolver`（调用 `verify_local_agent_jwt` -> 校验 agent active -> run_id 审计 -> 构造 `Agent` actor with `ActorSource::AgentJwt`）已集成进 `BearerTokenResolver` 的分派逻辑，services crate 通过 `cargo check`（0 error）。
>
> **§7 授权决策引擎进展（2026/07/12）**：新增 `services/src/auth/decision_engine.rs`，实现：
> - `TrustPresetResolver`（High/Low 信任预设，Agent issue mention 进入低信任边界需显式授予）。
> - `AuthorizationService::decide(actor, action, scope)` 优先级链：未认证 deny -> 实例管理员 allow -> 本地隐式 Board allow -> Agent 跨公司 deny -> 公司成员授予（角色默认权限 + 显式授予）。
> - 权限继承链：`check_explicit_grants` / `check_manager_chain` / `check_issue_mention_grant`。
> - `RolePermissions::default_permissions_for_role`（Owner 全量 / Admin 管理 / Operator 读写 / Viewer 只读）并在 decide 中合并。
> - onBehalfOf 委托：Agent 以 `on_behalf_of_user_id` / `on_behalf_of_memberships` 进行权限检查；缺少活跃成员关系时设置 `Decision.code = RESPONSIBLE_USER_UNAVAILABLE`。
> - services crate `cargo check` 通过（0 error）。
>
> **尚未实现（待后续迭代）**：
> - §2 数据模型：通用 `Repository`/`CrudOps`/`with_tx()` trait 仍未实现（各 repo 各自定义 `RepositoryError`）。
> - §10 审计日志(`log_auth_event`/`activity_log`)、速率限制、API Key 过期轮换已实现。
> - §10 集成测试已实现（20 个测试覆盖认证流程、授权决策、安全场景）。

---

## [Core Types] 核心类型与错误体系

### 阶段一：基础框架

- [x] **定义 Actor 类型系统**
  - 定义 `AuthorizationActor` 枚举（Board / Agent / None 三种变体）及对应结构体字段
  - 定义 `ActorSource` 枚举（local_implicit / session / board_key / agent_key / agent_jwt / cloud_tenant / none）
  - 定义 `AgentApiKeyScope` 结构体

- [x] **定义授权决策类型**
  - 定义 `AuthorizationAction` 枚举（PermissionKey 变体 + 自定义 action 变体，覆盖源文档全部 action）
  - 定义 `AuthorizationDecision` 结构体（allowed / action / explanation / code / reason / grant）
  - 定义 `DecisionReason` 枚举（allow_* / deny_* 全部变体，含 allow_issue_mention_grant / deny_low_trust_boundary 等）

- [x] **定义统一错误类型**
  - 定义 `AuthError` 枚举（Unauthorized / Forbidden(reason) / BadRequest / Internal 等），实现 `IntoResponse`
  - 定义 `AuthResult<T>` 类型别名
  - 为 `AuthError` 实现 `std::error::Error` 与 `Display`，确保不暴露内部实现细节

### 阶段二：扩展类型

- [x] **定义成员与角色类型**
  - 定义 `MembershipRole` 枚举（Owner / Admin / Operator / Viewer）
  - 定义 `PrincipalType` 枚举（User / Agent）
  - 定义 `MembershipStatus` 枚举（Active / Archived）

- [x] **定义权限授予类型**
  - 定义 `PermissionGrant` 结构体（companyId / principalType / principalId / permissionKey / scope / grantedByUserId）
  - 定义 `GrantScope` 类型（JSON scope wrapper）
  - 定义 `PermissionKey` 新类型包装，含公司级（users:invite / joins:approve）、项目级（issues:read / issues:write）、Agent 特定（tasks:assign）权限常量

- [x] **定义邀请与认领相关类型**
  - 定义 `InviteType` 枚举（CompanyJoin / BootstrapCeo）
  - 定义 `AllowedJoinTypes` 枚举（Human / Agent / Both）
  - 定义 `JoinRequestStatus` 枚举（PendingApproval / Approved / Rejected）

---

## [Data Model] 数据模型与数据库层

### 阶段一：基础框架

- [x] **定义核心表结构（SQL schema + Rust 映射）**
  - 定义 `auth_users` 表 schema 及 `AuthUser` 结构体（实现 `sqlx::FromRow`）
  - 定义 `auth_sessions` 表 schema 及 `AuthSession` 结构体
  - 定义 `companies` 表 schema 及 `Company` 结构体

- [x] **定义认证相关表结构**
  - 定义 `board_api_keys` 表 schema 及 `BoardApiKey` 结构体
  - 定义 `agent_api_keys` 表 schema 及 `AgentApiKey` 结构体
  - 定义 `cli_auth_challenges` 表 schema 及 `CliAuthChallenge` 结构体

- [x] **定义授权相关表结构**
  - 定义 `instance_user_roles` 表 schema 及 `InstanceUserRole` 结构体
  - 定义 `company_memberships` 表 schema 及 `CompanyMembership` 结构体
  - 定义 `principal_permission_grants` 表 schema 及 `PrincipalPermissionGrant` 结构体

### 阶段二：核心功能

- [x] **定义邀请与加入请求表结构**
  - 定义 `invites` 表 schema 及 `Invite` 结构体
  - 定义 `join_requests` 表 schema 及 `JoinRequest` 结构体
  - 实现 `InviteType` / `AllowedJoinTypes` / `JoinRequestStatus` 枚举及数据库映射

- [x] **实现数据库迁移模块**
  - 创建 migrations 目录结构，编写初始 migration 文件
  - 实现 `run_migrations()` 函数，封装 `sqlx::migrate!()`
  - 添加迁移回滚支持（down migration）

- [x] **实现通用 Repository trait**
  - 定义 `Repository` trait（`new(pool) -> Self` / `pool() -> &PgPool`）
  - 定义 `CrudOps` 子 trait（find_by_id / create / update / delete 泛型方法签名）
  - 为所有 Repository 实现统一事务辅助方法（`with_tx()` / `with_transaction()` 通过 `RepositoryExt` blanket impl 覆盖全部 Repository）

---

## [JWT Module] JWT 签发与验证

### 阶段一：基础框架

- [x] **定义 JWT 配置与 Claims 结构**
  - 定义 `JwtConfig` 结构体（secret / ttl / issuer / audience / instance_id）
  - 定义 `LocalAgentJwtClaims` 结构体（sub / company_id / adapter_type / run_id / iat / exp / iss / aud / instance_id）
  - 定义 `LocalAgentJwtHeader` 结构体（alg: HS256 / typ: JWT）

- [x] **实现公司级签名密钥派生**
  - 实现 `derive_company_signing_key(secret, company_id, instance_id) -> HmacKey`
  - 使用 HMAC-SHA256 派生，确保跨公司密钥隔离
  - 编写单元测试验证不同 company_id + instance_id 组合产生不同密钥

- [x] **实现 JWT 签发函数**
  - 实现 `create_local_agent_jwt(config, agent_id, company_id, adapter_type, run_id, responsible_user_id, key_scope) -> Option<String>`
  - 配置不存在时返回 `None`
  - 编写集成测试验证签发的 token 可被正确解析

### 阶段二：核心功能

- [x] **实现 JWT 验证函数**
  - 实现 `verify_local_agent_jwt(config, token) -> Option<LocalAgentJwtClaims>`
  - 处理四种失败场景：配置缺失 / 格式无效 / 签名无效 / 已过期，均返回 `None`
  - 使用 constant-time 比较防止时序攻击

- [x] **实现 JWT 配置加载**
  - 实现 `JwtConfig::from_env()` 从环境变量加载配置
  - 实现 `JwtConfig::from_db(pool, instance_id)` 从数据库加载配置
  - 实现配置验证逻辑（secret 非空 / ttl 合理范围，默认 1 小时）

---

## [Auth Middleware] 认证中间件

### 阶段一：基础框架

- [x] **定义 Actor 解析 trait 与中间件结构**
  - 定义 `ActorResolver` trait（`async fn resolve(&self, request: &Request) -> AuthResult<AuthorizationActor>`）
  - 定义 `AuthMiddleware` 结构体（内含 `ActorResolver` 列表 + `AuthMode` 枚举）
  - 定义 `AuthMode` 枚举（LocalTrusted / Authenticated）

- [x] **实现 local_trusted 模式**
  - 实现 `LocalTrustedResolver`，始终返回 `AuthorizationActor::Board { source: local_implicit, ... }`
  - 在 `AuthMiddleware` 中根据 `AuthMode` 分派到对应 resolver
  - 编写单元测试验证 local_trusted 模式返回正确的 actor

### 阶段二：核心功能

- [x] **实现 Bearer Token 分派逻辑**
  - 从 `Authorization` header 提取 Bearer token
  - 按 token 前缀/格式分派：Board API Key -> `BoardAuthResolver` / Agent API Key -> `AgentKeyResolver` / Agent JWT -> `AgentJwtResolver`
  - 无有效 token 时 fallback 到 Session / Cloud Tenant / None

- [x] **实现 Session Cookie 认证**
  - 集成 BetterAuth session 解析（`resolve_session(request) -> Option<(Session, AuthUser)>`）
  - 查询 instance admin 角色与公司成员资格
  - 构建 `AuthorizationActor::Board { source: session, ... }` 含 memberships / isInstanceAdmin

- [x] **实现 Cloud Tenant Header 认证**
  - 从 `X-Paperclip-Cloud-*` headers 解析租户信息（stack ID / stack role）
  - Upsert 用户 / 公司 / 成员资格（基于 stack ID 派生公司 ID，基于 stack role 映射成员角色）
  - 构建 `AuthorizationActor::Board { source: cloud_tenant, ... }`

- [x] **将中间件集成到路由层**
  - 实现 `axum::middleware::from_fn(auth_middleware)` 作为 layer
  - 将解析后的 `AuthorizationActor` 注入到 request extensions
  - 实现 `extract_actor()` 辅助函数供 handler 使用

---

## [Board Auth Service] Board 认证服务

### 阶段一：基础框架

- [x] **实现 BoardApiKey Repository**
  - 实现 `find_by_key_hash(pool, key_hash) -> Option<BoardApiKey>`
  - 实现 `create(pool, user_id, name, key_hash, expires_at) -> BoardApiKey`
  - 实现 `revoke(pool, key_id) -> AuthResult<()>`

- [x] **实现 API Key 哈希与校验**
  - 实现 `hash_api_key(token) -> String`（SHA-256）
  - 实现 `verify_api_key(token, hash) -> bool`（constant-time 比较）
  - 编写单元测试验证哈希与校验的一致性

### 阶段二：核心功能

- [x] **实现 resolve_board_access 流程**
  - 实现 `resolve_board_access(pool, user_id) -> AuthResult<(AuthUser, Vec<CompanyMembership>, bool)>`
  - 查询用户信息 / 公司成员资格 / 是否实例管理员（instance_user_roles 表）
  - 由 Bearer/Session 解析器调用，构建带 memberships / is_instance_admin 的 Board actor

- [x] **实现 CLI 认证挑战流程**
  - 实现 `create_cli_auth_challenge(pool, user_id, requested_access, company_id) -> CliAuthChallenge`
  - 实现 `approve_cli_auth_challenge(pool, challenge_id) -> AuthResult<BoardApiKey>`（创建 API Key 并返回）
  - 实现 `cancel_cli_auth_challenge(pool, challenge_id) -> AuthResult<()>`

- [x] **实现 Board 认领流程**
  - 实现 `inspect_board_claim_challenge(pool, token) -> AuthResult<ClaimChallenge>`
  - 实现 `claim_board_ownership(pool, user_id, token) -> AuthResult<()>`（删除 local-board admin -> 添加用户为 instance admin -> 添加到所有公司为 owner）
  - 编写事务完整性测试

---

## [Agent Auth Service] Agent 认证服务

### 阶段一：基础框架

- [x] **实现 AgentApiKey Repository**
  - 实现 `find_by_key_hash(pool, key_hash) -> Option<AgentApiKey>`
  - 实现 `create(pool, agent_id, company_id, key_hash, scope_config, responsible_user_id) -> AgentApiKey`
  - 实现 `revoke(pool, key_id) -> AuthResult<()>`

- [x] **实现 Agent API Key 认证流程**
  - 在中间件中通过 key hash 查找 AgentApiKey 记录
  - 查询关联的 Agent 记录与 responsible user memberships
  - 构建 `AuthorizationActor::Agent { source: agent_key, key_id, key_scope, ... }`

### 阶段二：核心功能

- [x] **实现 Agent JWT 认证流程**
  - 调用 `verify_local_agent_jwt()` 验证 token
  - 查询 Agent 是否存在且 active；验证 run_id 是否匹配（不匹配返回 422 + 审计日志）
  - 构建 `AuthorizationActor::Agent { source: agent_jwt, run_id, ... }`

- [x] **实现 Responsible User 加载**
  - 实现 `load_responsible_user_memberships(pool, responsible_user_id, company_id) -> AuthResult<Vec<CompanyMembership>>`
  - 返回 responsible user 在指定公司内的活跃成员关系（供 Agent actor 权限检查使用）
  - 实现 `on_behalf_of` 成员资格映射

---

## [Authorization Service] 授权决策引擎

### 阶段一：基础框架

- [x] **实现 assertCompanyAccess 守卫函数**
  - 实现 `assert_company_access(actor, company_id, is_write_op) -> AuthResult<()>`
  - 处理 actor.type = none -> 401 / agent 跨公司 -> 403 / viewer 写操作 -> 403
  - 编写单元测试覆盖所有拒绝分支

- [x] **实现 assertInstanceAdmin 守卫函数**
  - 实现 `assert_instance_admin(actor) -> AuthResult<()>`
  - 检查 actor.is_instance_admin 标志
  - 非实例管理员返回 403

- [x] **实现 assertCompanyPermission 守卫函数**
  - 实现 `assert_company_permission(pool, actor, company_id, permission_key) -> AuthResult<()>`
  - 先调用 `assert_company_access`，再查询 `principal_permission_grants` 表
  - 无权限授予时返回 403

### 阶段二：核心功能

- [x] **实现 TrustPresetResolver**
  - 定义 `TrustPreset` 枚举（High / Low）与 `TrustPresetResolution` 结构体
  - 实现 `resolve_core_trust_preset(actor, resource) -> TrustPresetResolution`
  - 低信任边界场景：issue mention 需要 explicit grant

- [x] **实现 decide() 授权决策主函数**
  - 实现 `authorization_service.decide(actor, action, resource, scope) -> AuthorizationDecision`
  - 按优先级链决策：unauthenticated deny -> instance admin allow -> local board allow -> agent company boundary -> company member grant
  - 编写集成测试覆盖全部 reason 分支

- [x] **实现权限继承链查询**
  - 实现 `check_explicit_grants(pool, company_id, principal_type, principal_id, permission_key) -> bool`
  - 实现 `check_manager_chain(pool, company_id, principal_id, permission_key) -> bool`
  - 实现 `check_issue_mention_grant(pool, company_id, agent_id, issue_id) -> bool`

### 阶段三：高级特性

- [x] **实现 Role 默认权限映射**
  - 定义 `RolePermissions` 常量映射（Owner -> 全部 / Admin -> 管理 / Operator -> 读写 / Viewer -> 只读）
  - 实现 `default_permissions_for_role(role) -> Vec<PermissionKey>`
  - 在 decide() 中合并角色默认权限与显式授权

- [x] **实现 onBehalfOf 委托模式**
  - 在 Agent actor 中解析 `on_behalf_of_user_id`
  - 加载 `on_behalf_of_memberships` 并用于权限检查
  - 缺少 active membership 时设置 Decision.code = `RESPONSIBLE_USER_UNAVAILABLE`

---

## [Auth Routes] 认证路由

### 阶段一：基础框架

- [x] **搭建路由骨架**
  - 使用 `axum::Router` 定义 `/api/auth` 路由组
  - 集成 `AuthMiddleware` layer
  - 定义统一的 `ApiResponse<T>` 响应包装

- [x] **实现 GET /api/auth/get-session**
  - 从 request extensions 提取 `AuthorizationActor`
  - 查询 BetterAuth session 信息
  - 返回 session JSON（未登录返回 null）

- [x] **实现 GET/PATCH /api/auth/profile**
  - GET: 查询当前用户资料
  - PATCH: 更新当前用户资料（name / image）
  - 未认证时返回 401

---

## [Access Control Routes] 访问控制路由

### 阶段一：基础框架

- [x] **搭建路由骨架**
  - 使用 `axum::Router` 定义 `/api` 下的访问控制路由组
  - 集成 `AuthMiddleware` + `assertCompanyAccess` 守卫
  - 定义路径参数提取器（`CompanyId` / `MemberId` / `Token`）

- [x] **实现 Board 认领端点**
  - `GET /api/board-claim/:token` -> `inspect_board_claim_challenge()`
  - `POST /api/board-claim/:token/claim` -> `claim_board_ownership()`
  - claim 逻辑：删除 local-board admin -> 添加用户为 instance admin -> 添加到所有公司为 owner

- [x] **实现首次管理员认领端点**
  - `POST /api/bootstrap/claim` -> `claim_first_instance_admin()`
  - 校验 session + 当前无 instance admin 的前置条件
  - 插入 `instance_user_roles` 记录

### 阶段二：核心功能

- [x] **实现 CLI 认证端点**
  - `POST /api/cli-auth/challenges` -> 创建挑战
  - `GET /api/cli-auth/challenges/:id` -> 查询状态
  - `POST /api/cli-auth/challenges/:id/approve` -> 批准并返回 API Key

- [x] **实现 CLI 认证辅助端点**
  - `POST /api/cli-auth/challenges/:id/cancel` -> 取消挑战
  - `GET /api/cli-auth/me` -> 获取当前 CLI 认证用户信息（resolve_board_access）
  - `POST /api/cli-auth/revoke-current` -> 撤销当前使用的 Board API Key

- [x] **实现 Board API Key 管理端点**
  - `GET /api/board-api-keys` -> 列出当前用户的 API Keys
  - `POST /api/board-api-keys` -> 创建新 API Key（返回明文仅一次）
  - `DELETE /api/board-api-keys/:keyId` -> 撤销 API Key

- [x] **实现邀请端点**
  - `POST /api/companies/:companyId/invites` -> 创建邀请（需 `users:invite` 权限）
  - `GET /api/invites/:token` -> 获取邀请详情
  - `POST /api/invites/:token/accept` -> 接受邀请，创建 join_request

### 阶段三：高级特性

- [x] **实现邀请子资源端点**
  - `GET /api/invites/:token/logo` -> 获取邀请 Logo
  - `GET /api/invites/:token/onboarding` -> 获取入职指南（JSON）
  - `GET /api/invites/:token/onboarding.txt` -> 获取纯文本入职指南
  - `POST /api/invites/:inviteId/revoke` -> 撤销邀请

- [x] **实现邀请技能与测试端点**
  - `GET /api/invites/:token/skills/index` -> 获取技能索引
  - `GET /api/invites/:token/skills/:skillName` -> 获取技能
  - `GET /api/invites/:token/test-resolution` -> 测试 URL 解析

- [x] **实现加入请求端点**
  - `GET /api/companies/:companyId/join-requests` -> 列出加入请求
  - `POST /api/companies/:companyId/join-requests/:requestId/approve` -> 批准（创建成员资格 + 设置默认授权）
  - `POST /api/companies/:companyId/join-requests/:requestId/reject` -> 拒绝
  - `POST /api/join-requests/:requestId/claim-api-key` -> 认领 Agent API Key（验证 claim secret）

- [x] **实现成员管理端点**
  - `GET /api/companies/:companyId/members` -> 列出成员
  - `PATCH /api/companies/:companyId/members/:memberId` -> 更新成员信息
  - `PATCH /api/companies/:companyId/members/:memberId/role-and-grants` -> 更新角色和权限授予
  - `PATCH /api/companies/:companyId/members/:memberId/permissions` -> 更新成员权限
  - `POST /api/companies/:companyId/members/:memberId/archive` -> 归档成员

- [x] **实现实例管理员端点**
  - `GET /api/admin/users` -> 列出所有用户（需 instance admin）
  - `POST /api/admin/users/:userId/promote-instance-admin` -> 提升为实例管理员
  - `POST /api/admin/users/:userId/demote-instance-admin` -> 降级实例管理员
  - `GET /api/admin/users/:userId/company-access` -> 获取用户公司访问权限（TODO）
  - `PUT /api/admin/users/:userId/company-access` -> 设置用户公司访问权限（TODO）

- [x] **实现邀请子资源端点**
  - `GET /api/invites/:token/logo` -> 返回公司 Logo（重定向到 Asset 服务）
  - `GET /api/invites/:token/onboarding` -> 返回 onboarding 文档（Markdown 格式）
  - `GET /api/invites/:token/onboarding.txt` -> 返回纯文本版本
  - `GET /api/invites/:token/skills/index` -> 列出邀请范围内可用的技能
  - `GET /api/invites/:token/skills/:skillName` -> 获取特定技能详情

- [x] **实现 Skills System 核心服务**
  - 定义 `SkillsRegistry` 结构体（内存缓存可用技能列表）
  - 实现 `list_available_skills()` - 列出可用技能（含名称、描述、分类）
  - 实现 `get_skill_index()` - 获取技能索引（元数据、版本、依赖）
  - 实现 `get_skill_details()` - 获取技能详情（用法示例、参数说明）

- [x] **实现技能查询端点**
  - `GET /api/skills/available` -> 列出可用技能（公开访问）
  - `GET /api/skills/index` -> 技能索引（需认证）
  - `GET /api/skills/:skillName` -> 获取技能详情（需认证）

- [x] **实现用户目录与 OpenClaw 端点**
  - `GET /api/companies/:companyId/user-directory` -> 获取公司用户目录（支持分页、搜索、排序）
  - `POST /api/companies/:companyId/openclaw/invite-prompt` -> 生成 OpenClaw 邀请提示
  - 实现用户目录的权限校验（assertCompanyAccess）

- [x] **完善实例管理员端点实现**
  - 实现 `GET /api/admin/users` 详细查询逻辑（分页、搜索过滤）
  - 支持按邮箱、用户名、实例管理员状态过滤
  - 返回用户信息（id, email, username, isInstanceAdmin, companies, createdAt）
  - 仅实例管理员可访问（assertInstanceAdmin）

---

## [Security Hardening] 安全加固

### 阶段一：基础框架

- [x] **实现审计日志**
  - 定义 `activity_log` 表 schema 及 `ActivityLog` 结构体（已存在 `003_create_activity_logs.sql` + `activity_log_repository.rs`）
  - 实现 `log_auth_event(pool, event_type, actor, details)` 函数（`services/src/auth/audit.rs`）
  - 在关键认证事件中调用：JWT run_id 不匹配 / Agent Key 缺少 responsible user（`middleware.rs` 中 `audit_jwt_rejected` 已调用）

- [x] **实现安全 Header 中间件**
  - 添加 `X-Content-Type-Options: nosniff`
  - 添加 CORS 策略配置
  - 生产环境 Cookie 安全标志（Secure / HttpOnly / SameSite）

- [x] **实现通用错误响应标准化**
  - 认证失败返回统一格式，不暴露内部实现细节（`AuthError` 的 `user_message()` 方法）
  - 使用 `subtle::ConstantTimeEq` 进行 token 比较（`board_api_key_repository.rs`）
  - 编写模糊测试确保错误响应无信息泄漏（集成测试 `test_error_response_no_info_leak`）

### 阶段二：核心功能

- [x] **实现速率限制**
  - 定义 `RateLimiter` 中间件（基于 IP / API Key 限流）（`services/src/auth/rate_limiter.rs`）
  - 配置每分钟请求阈值（`RateLimitConfig`）
  - 超限返回 429 Too Many Requests（`RateLimitError` 实现 `IntoResponse`）

- [x] **实现 API Key 过期与轮换**
  - `BoardApiKey` 过期检查（`expires_at` 字段）（`middleware.rs` + `board_api_key_repository.rs` SQL 查询）
  - 过期 key 在认证中间件中拒绝（`resolve_board_key` 检查 + 审计日志）
  - 实现 key 自动轮换策略（创建新 key 后旧 key 延迟失效）（`services/src/auth/key_rotation.rs`）

---

## [Integration Testing] 集成测试

### 阶段一：基础框架

- [x] **搭建测试基础设施**
  - 创建 test fixture 模块（测试数据库初始化 / 清理）
  - 实现 `TestApp` 辅助结构体（启动测试服务器 + HTTP client）
  - 实现 `seed_test_data()` 函数（创建测试用户 / 公司 / 成员资格）

- [x] **实现认证流程端到端测试**
  - 测试 Board API Key 认证完整流程（创建 -> 使用 -> 撤销）
  - 测试 Agent API Key 认证完整流程（创建 -> 使用 -> 撤销）
  - 测试 Agent JWT 签发 -> 验证 -> 过期流程

### 阶段二：核心功能

- [x] **实现授权决策测试**
  - 测试 Instance Admin 全局权限
  - 测试公司成员角色权限（Owner / Admin / Operator / Viewer）
  - 测试自定义权限授予与回收

- [x] **实现邀请与加入流程测试**
  - 测试邀请创建 -> 接受 -> 批准 -> 认领 API Key 完整链路
  - 测试邀请过期与撤销场景
  - 测试 claim secret 消费幂等性

- [x] **实现安全场景测试**
  - 测试跨公司访问拒绝
  - 测试 JWT 跨实例 / 跨公司伪造拒绝
  - 测试时序攻击防护（constant-time 比较验证）

---

## 依赖顺序总览

```
阶段一（基础框架）推荐实现顺序:

  1. Core Types (Actor + Decision + Error 类型定义)
  2. Data Model (核心表 schema + Repository trait)
  3. JWT Module (Config + Claims + 密钥派生 + 签发)
  4. Auth Middleware (ActorResolver trait + local_trusted 模式)
  5. Board Auth Service (BoardApiKey Repository + 哈希校验)
  6. Agent Auth Service (AgentApiKey Repository + Key 认证)
  7. Authorization Service (assertCompanyAccess + assertInstanceAdmin)
  8. Security Hardening (审计日志 + 安全 Header)

阶段二（核心功能）推荐实现顺序:

  1. Data Model (邀请/加入请求表 + 迁移模块)
  2. JWT Module (验证函数 + 配置加载)
  3. Auth Middleware (Bearer Token 分派 + Session + Cloud Tenant + 路由集成)
  4. Board Auth Service (resolve_board_access + CLI 挑战 + Board 认领)
  5. Agent Auth Service (JWT 认证 + Responsible User 加载)
  6. Authorization Service (TrustPresetResolver + decide() + 权限继承链)
  7. Auth Routes (路由骨架 + session + profile)
  8. Access Control Routes (CLI + Board API Key + 邀请)

阶段三（高级特性）推荐实现顺序:

  1. Authorization Service (Role 默认权限 + onBehalfOf 委托)
  2. Skills System & Invite Sub-Resources (技能系统 + 邀请子资源)
  3. Access Control Routes (成员管理 + 管理员 + OpenClaw)
  4. Security Hardening (速率限制 + API Key 轮换)
  5. Integration Testing (授权决策 + 邀请流程 + 安全场景)
```

## 任务依赖关系图

```
Core Types ──> Data Model ──> JWT Module ──> Auth Middleware ──> Board Auth Service
     │                              │               │                  │
     │                              v               v                  v
     │                        Agent Auth     (Bearer 分派)      resolve_board_access
     │                        Service             │                  │
     │                              │             v                  v
     v                              v        Session / Cloud     CLI 挑战
Authorization                JWT 认证流程    Tenant 认证         Board 认领
  Service                          │             │                  │
     │                             v             v                  v
assert 函数                Responsible       中间件集成         Access Control
  trait                   User 加载        路由层              Routes
     │                          │             │                  │
     v                          v             v                  v
TrustPreset              Auth Routes     Auth Routes        邀请/加入/
  Resolver               (skeleton)      (profile)          成员管理
     │
     v
  decide() ──> 权限继承链 ──> Role 默认权限 + onBehalfOf
     │
     v
Security Hardening ──> Integration Testing
```

---

## Rust 技术选型建议

| 领域 | 推荐选型 | 说明 |
|------|----------|------|
| Web 框架 | axum 0.7+ | 生态成熟，与 tower 中间件集成好 |
| 数据库 | sqlx 0.7+ | async PostgreSQL，编译时 SQL 检查 |
| Migration | sqlx-cli 或 refinery | 与 sqlx 配套 |
| JWT | jsonwebtoken | HS256 支持，轻量 |
| 哈希 | sha2 | SHA-256 用于 API Key 哈希 |
| 时序安全 | subtle | ConstantTimeEq 防止时序攻击 |
| HMAC | hmac | 公司级签名密钥派生 |
| 序列化 | serde + serde_json | JSONB 字段统一用 serde 映射 |
| UUID | uuid crate (v7) | 主键生成 |
| 时间 | chrono 或 time | 时间戳处理 |
| 错误处理 | thiserror + anyhow | thiserror 定义业务错误，anyhow 用于内部 |
| 异步运行时 | tokio | 标准选择 |
| 请求验证 | garde 或 validator | garde 更现代，支持嵌套结构验证 |
| 测试 | testcontainers + mockall | 集成测试用 testcontainers，单测用 mockall |
| CORS | tower-http | CorsLayer 中间件 |
| 速率限制 | tower-governor 或自实现 | 基于 IP/Key 的限流 |
