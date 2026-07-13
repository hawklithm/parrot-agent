# 实时通信与执行环境模块 - Rust 实现任务拆解

> 基于 [backend/realtime-environment.md](../backend/realtime-environment.md) 架构分析文档，拆解为 Rust 版本实现任务。

---

## 1. 数据模型层 实现任务

### 阶段一：基础架构
- [x] **定义环境管理核心枚举类型**
  - 定义 `EnvironmentDriver` 枚举（local / ssh / sandbox / plugin）
  - 定义 `EnvironmentStatus` 枚举（active / in_use / provisioning / error）
  - 定义 `LeaseStatus` 枚举（active / released / expired / failed）

- [x] **定义环境配置结构体**
  - 定义 `LocalEnvironmentConfig` 结构体（空配置占位）
  - 定义 `SshEnvironmentConfig` 结构体（host, port, username, remote_workspace_path, private_key, private_key_secret_ref, known_hosts, strict_host_key_checking）
  - 定义 `SandboxEnvironmentConfig` 结构体（provider, image, reuse_lease, stream_run_logs, timeout_ms）

- [x] **定义核心表结构体**
  - 定义 `Environment` 结构体，映射 environments 表（id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at）
  - 定义 `EnvironmentLease` 结构体，映射 environment_leases 表（id, company_id, environment_id, execution_workspace_id, issue_id, heartbeat_run_id, status, lease_policy, provider, provider_lease_id, acquired_at, last_used_at, expires_at, released_at, failure_reason, cleanup_status）
  - 定义 `ExecutionWorkspace` 结构体，映射 execution_workspaces 表（id, company_id, project_id, project_workspace_id, source_issue_id, name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url, metadata, created_at, updated_at）

### 阶段二：核心功能
- [x] **实现 Database Schema 迁移**
  - 使用 sqlx / sea-orm 定义 environments 表 migration
  - 定义 environment_leases 表 migration
  - 定义 execution_workspaces 表 migration

- [x] **定义 Secrets 与 Assets 数据模型**
  - 定义 `CompanySecret` 结构体，映射 company_secrets 表（id, company_id, name, key, provider, provider_config_id, managed_mode, scope, description, status, created_at, updated_at）
  - 定义 `Asset` 结构体，映射 assets 表（id, company_id, provider, object_key, content_type, byte_size, sha256, original_filename, created_by_agent_id, created_by_user_id, created_at, updated_at）
  - 定义 `SecretScope` 枚举（company / user）与 `SecretManagedMode` 枚举（paperclip_managed / external）

- [x] **实现 Repository trait**
  - 定义 `EnvironmentRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 定义 `ExecutionWorkspaceRepository` trait（create, get_by_id, list_by_company, update）
  - 定义 `SecretRepository` trait（create, get_by_id, list_by_company, update, delete, list_bindings）

### 阶段三：高级特性
- [x] **实现 JSONB 字段类型安全映射**
  - 为 `Environment.config: Jsonb` 实现按 `EnvironmentDriver` 分支的 Rust 类型安全序列化/反序列化
  - 为 `Environment.env_vars: Jsonb` 实现 `HashMap<String, EnvVarValue>` 类型映射（支持明文值与 Secret 引用两种变体）
  - 为 `EnvironmentLease.lease_policy: Jsonb` 与 `ExecutionWorkspace.metadata: Jsonb` 实现类型安全映射

---

## 2. 环境驱动层 实现任务

### 阶段一：基础架构
- [x] **定义 EnvironmentDriver trait**
  - 定义 `EnvironmentDriverTrait` trait（probe, acquire_lease, release_lease, ensure_ready）
  - 定义 `EnvironmentProbeResult` 结构体（ok, driver, summary, details）
  - 定义 `LeaseAcquisitionResult` 结构体（lease_id, provider, connection_info, expires_at）

- [x] **实现 EnvironmentDriver Registry**
  - 实现 `DriverRegistry` 结构体，持有 `HashMap<EnvironmentDriver, Box<dyn EnvironmentDriverTrait>>`
  - 实现 `register()` 与 `find_driver()` 方法
  - 实现 `resolve_driver_config()` 按 driver 类型路由到对应配置解析

- [x] **定义配置解析接口**
  - 定义 `DriverConfig` 枚举（Local / Ssh / Sandbox / Plugin）及对应配置结构体
  - 实现 `resolve_environment_driver_config_for_runtime()` 从 Environment 解析出 DriverConfig
  - 实现 `parse_environment_driver_config()` 按 driver 类型解析配置

### 阶段二：核心功能
- [x] **实现 Local Driver**
  - 实现 `LocalDriver` struct，满足 `EnvironmentDriverTrait`
  - 实现 `probe()` 验证本地工具可用性（git, node 等）
  - 实现 `acquire_lease()` / `release_lease()` 本地环境的简单租约管理

- [x] **实现 SSH Driver**
  - 实现 `SshDriver` struct，满足 `EnvironmentDriverTrait`
  - 实现 `probe()` 测试 SSH 连接与远程路径可达性
  - 实现 `acquire_lease()` 建立远程会话 / `release_lease()` 关闭远程会话

- [x] **实现 Sandbox Driver**
  - 实现 `SandboxDriver` struct，满足 `EnvironmentDriverTrait`
  - 实现 `probe()` 调用云原生沙箱提供者 API 健康检查
  - 实现 `acquire_lease()` 调用沙箱提供者创建实例 / `release_lease()` 销毁实例

### 阶段三：高级特性
- [x] **实现 Plugin Driver**
  - 实现 `PluginDriver` struct，满足 `EnvironmentDriverTrait`
  - 实现 `probe()` 通过插件系统执行探针
  - 实现 `resolve_plugin_driver_config()` 解析插件配置（plugin_key, driver_key, driver_config）

---

## 3. 租约管理服务层 实现任务

### 阶段一：基础架构
- [x] **定义 LeaseService trait**
  - 定义 `LeaseService` trait（acquire_lease, release_lease, refresh_heartbeat, get_active_leases）
  - 定义 `AcquireLeaseRequest` 结构体（environment_id, execution_workspace_id, issue_id）
  - 定义 `LeasePolicy` 结构体（heartbeat_interval, max_ttl, auto_release_on_expire）

- [x] **实现租约状态机**
  - 定义 `LeaseStateMachine`，实现状态转换（idle -> active -> released / expired / failed）
  - 实现租约过期检测逻辑（基于 expires_at 与 last_used_at）
  - 实现租约释放逻辑（更新 environment_leases.status 与 environments.status）

### 阶段二：核心功能
- [x] **实现租约获取流程**
  - 实现 `acquire_lease()` 查询环境 -> 解析 driver -> 调用 driver.acquire_lease()
  - 实现 environment_leases 记录插入（status: active）
  - 实现 environments.status 更新为 in_use
  - 实现租约并发控制：使用 SELECT FOR UPDATE 锁定 environment 行
  - 实现 max_concurrent_leases 配置检查（拒绝超量请求）

- [x] **实现心跳保活机制**
  - 实现 `refresh_heartbeat()` 更新 environment_leases.last_used_at = now()
  - 使用 tokio::time 启动后台心跳任务，按 heartbeat_interval 周期刷新
  - 实现心跳超时检测：租约过期时自动调用 release_lease()

- [x] **实现租约后台调度器**
  - 实现 `LeaseScheduler` struct，持有 tokio::spawn 的 JoinHandle
  - 在 LeaseService::acquire_lease() 成功后自动注册心跳任务
  - 实现租约过期扫描器（每 1 分钟扫描一次 last_used_at 超时的租约）
  - 实现优雅关闭：取消所有活跃心跳任务

### 阶段三：高级特性
- [x] **实现租约清理与恢复**
  - 实现 cleanup_status 跟踪（pending / in_progress / completed / failed）
  - 实现服务重启后的租约恢复逻辑（从数据库加载 active 租约并重建内存状态）
  - 实现僵尸租约检测与自动释放（last_used_at 超时且无心跳）

- [x] **实现僵尸租约清理调度器**
  - 实现独立后台任务（tokio::spawn），每 5 分钟扫描一次
  - 僵尸检测标准：last_used_at > heartbeat_interval * 3 且 status = active
  - 实现分批清理（避免一次性释放大量租约导致雪崩）
  - 实现清理失败重试队列（exponential backoff）

---

## 4. 执行工作空间服务层 实现任务

### 阶段一：基础架构
- [x] **定义 ExecutionWorkspaceService trait**
  - 定义 `ExecutionWorkspaceService` trait（get_by_id, list_by_company, ensure_available, update）
  - 定义 `WorkspaceRuntimeConfig` 结构体（services, env_vars, driver_config）
  - 定义 `RuntimeServiceEntry` 结构体（name, command, env_vars, restart_policy）

- [x] **定义运行时服务核心类型**
  - 定义 `RuntimeServiceRecord` 结构体（id, name, status, child_pid, reuse_key, started_at）
  - 定义 `RuntimeServiceStatus` 枚举（starting / running / stopped / failed）
  - 定义 `StartedService` 结构体（service_id, name, status）

### 阶段二：核心功能
- [x] **定义工作空间生命周期触发器**
  - 定义 `WorkspaceProvisioningTrigger` 枚举（agent_run / manual_api / scheduled）
  - 实现 `POST /execution-workspaces` API 端点（手动创建）
  - 定义工作空间创建与 Agent 执行流的集成点

- [x] **实现工作空间编排服务**
  - 实现 `WorkspaceOrchestrator` 协调创建 -> 租约获取 -> 运行时启动
  - 实现失败时的级联回滚（创建失败 -> 清理租约）
  - 实现状态机跟踪（provisioning -> ready -> running -> teardown）

- [x] **实现工作空间可用性保证**
  - 实现 `ensure_persisted_execution_workspace_available()` 确保工作空间已持久化
  - 实现 git worktree 创建或共享工作区复用逻辑
  - 实现工作空间状态校验（status 必须为 ready 才可启动服务）

- [x] **实现运行时服务启动流程**
  - 实现 `start_runtime_services_for_workspace_control()` 解析 workspaceRuntime 配置 -> 列出服务条目 -> 逐个启动
  - 实现 `ensure_runtime_service_available()` 生成稳定 service ID（基于 SHA256）-> 启动子进程 -> 注册到运行时
  - 实现 `spawn_runtime_service_process()` 使用 tokio::process::Command 启动子进程并注册退出回调

- [x] **集成 Secrets 注入到运行时服务启动**
  - 在 `spawn_runtime_service_process()` 中调用 `secretService.resolve_environment_secrets()`
  - 将解析后的 secrets 合并到 `Command::envs()`
  - 实现注入失败时的启动n- [x] **实现运行时服务控制端点**
  - 实现 `POST /execution-workspaces/:id/runtime-services/:action` 路由（action: start / stop / restart / run）
  - 实现 `POST /execution-workspaces/:id/runtime-commands/:action` 路由
  - 实现 `POST /execution-workspaces/:id/reconcile-branch` 分支协调路由

### 阶段三：高级特性
- [x] **实现运行时服务重启策略**
  - 定义 `RestartPolicy` 枚举（never / always / on_failure / exponential_backoff）
  - 实现进程退出监听器：根据 exit_code 决定是否重启
  - 实现重启计数器与熔断机制（5 次失败后停止重启）
  - 实现 exponential backoff 延迟重启（1s, 2s, 4s, 8s, 16s）

- [x] **实现运行时服务双写与持久化**
  - 实现内存 Map（DashMap）+ 数据库持久化双写策略
  - 实现进程退出时状态更新（内存清除 + 数据库持久化）
  - 实现服务重启后从数据库恢复运行时服务状态

- [x] **实现分支协调与冲突处理**
  - 实现 `reconcile_execution_workspace_branch()` 支持 force-push / merge 两种模式
  - 实现 `inspect_branch_state()` 检查分支状态（ahead/behind/diverged）
  - 实现冲突检测：解析 git merge 输出识别冲突文件
  - 实现自动冲突解决策略（ours / theirs / manual）
  - 实现冲突时的工作空间状态锁定（status = conflict_requires_resolution）
  - 实现 `POST /execution-workspaces/:id/resolve-conflicts` API 端点
  - 实现协调操作审计日志记录

---

## 5. WebSocket 实时通信层 实现任务

### 阶段一：基础架构
- [x] **定义 WebSocket 连接管理核心类型**
  - 定义 `WsSession` 结构体（session_id, user_id, company_id, permissions, subscriptions, ws_sender）
  - 定义 `WsMessage` 枚举（Subscribe / Unsubscribe / Event / Response / Error）
  - 定义 `SessionManager` trait（register_connection, remove_connection, broadcast, send_to_session）

- [x] **实现 WebSocket 升级与认证**
  - 使用 axum + tokio-tungstenite 实现 HTTP 到 WebSocket 升级（`/api/ws?sessionId=xxx&token=xxx`）
  - 实现 `validate_token()` 验证连接 token 并提取用户信息
  - 实现 WebSocket 连接注册到 SessionManager

### 阶段二：核心功能
- [x] **实现双向消息处理**
  - 实现 Server->Client 推送：`broadcast(session_id, event)` 将事件推送到所有订阅连接
  - 实现 Client->Server 请求处理：subscribe / unsubscribe / exec_command 三种 action 路由
  - 实现事件处理器注册（on_agent_execution_event, on_workspace_runtime_update, on_issue_comment_event）

- [x] **实现心跳与重连机制**
  - 实现 WebSocket Ping/Pong 心跳（每 30 秒一次）
  - 实现连接丢失检测与 `mark_connection_unhealthy()` 标记
  - 实现重连逻辑：`restore_session(session_id)` 恢复会话 + 推送 missed_events

### 阶段三：高级特性
- [x] **实现频道订阅与事件过滤**
  - 实现频道订阅管理（add_subscription / remove_subscription）
  - 实现事件过滤：仅推送与订阅频道匹配的事件
  - 实现连接池管理与背压控制（高并发下的消息队列限制）

- [x] **实现连接池管理与背压控制**
  - 实现 `max_connections_per_company` 配置限制
  - 实现连接准入控制（超限时返回 429 Too Many Requests）
  - 实现慢消费者检测（消息队列深度超过阈值时断开连接）
  - 实现自适应背压（动态调整事件推送速率）

- [x] **实现错过事件缓冲与重放机制**
  - 实现 `MissedEventBuffer`（固定大小环形缓冲区，默认 1000 条）
  - 实现事件保留策略（最多保留 5 分钟，超时清除）
  - 实现溢出时的 "events_truncated" 标记推送
  - 实现重连时的增量推送（基于客户端 last_seen_event_id）

---

## 6. SSE 流式事件层 实现任务

### 阶段一：基础架构
- [x] **定义 SSE 核心类型**
  - 定义 `SseFrame` 结构体（event: Option<String>, data: String）
  - 定义 `SseStreamEvent` 枚举（Log / Delta / Terminal）
  - 定义 SSE 响应的 Content-Type 与帧格式常量

- [x] **实现 SSE 帧解析器**
  - 实现 `parse_sse_frames(buffer: &str) -> (Vec<SseFrame>, &str)` 解析 SSE 文本流
  - 处理规范化换行（\r\n -> \n）、跳过注释行（以 : 开头）
  - 支持多行 data 字段拼接

### 阶段二：核心功能
- [x] **实现 SSE 流创建与推送**
  - 实现 `create_sse_stream(run_id)` 创建 SSE 连接（`GET /api/execute/sse?runId=xxx`）
  - 实现 onLog 回调：推送 `data: {"stream":"stdout","chunk":"..."}` 帧
  - 实现 onDelta 回调：推送 `event: message.delta\ndata: {"text":"..."}` 帧
  - 实现 onTerminal 回调：推送 `event: run.complete\ndata: {...}` 帧并关闭连接

- [x] **实现敏感信息过滤**
  - 实现 `sanitize_sensitive_text()` 移除日志中的凭证信息（api_key/token/password/secret=***REDACTED***）
  - 实现 `redact_for_log()` 对 SSE 推送内容进行脱敏
  - 定义 `CRITICAL_HEADERS` 常量集合（authorization, x-api-key, secret, password）

### 阶段三：高级特性
- [x] **实现 SSE 连接管理与背压控制**
  - 实现客户端断开检测（EventSource abort）
  - 实现 SSE 流的 cleanup 资源释放
  - 实现慢消费者的背压控制与连接断开策略

---

## 7. Secrets 管理服务层 实现任务

### 阶段一：基础架构
- [x] **定义 SecretProvider trait**
  - 定义 `SecretProvider` trait（store, retrieve, delete, rotate）
  - 定义 `SecretProviderType` 枚举（local_encrypted / aws_secrets_manager / gcp_secret_manager / vault）
  - 定义 `SecretProviderConfig` 结构体（provider_type, endpoint, credentials_ref）

- [x] **定义 Secrets 服务核心类型**
  - 定义 `SecretService` trait（create, get_by_id, list, update, delete, rotate, resolve_secret_ref, list_bindings）
  - 定义 `CreateSecretInput` 结构体（name, key, value, provider, managed_mode, scope, description）
  - 定义 `SecretBinding` 结构体（id, secret_id, target_type, target_id）

- [x] **定义用户 Secret 定义类型**
  - 定义 `UserSecretDefinition` 结构体（id, company_id, name, key, description, required）
  - 定义 `UserSecret` 结构体（id, definition_id, user_id, value_ref, status）
  - 定义 `SecretBindingTargetType` 枚举（agent / environment / project / routine）

### 阶段二：核心功能
- [x] **实现 Local Encrypted Provider**
  - 实现 `LocalEncryptedProvider`，满足 `SecretProvider` trait
  - 使用 AES-256-GCM 加密存储 Secret 值
  - 实现 store / retrieve / delete / rotate 四个核心操作

- [x] **实现 Secret CRUD 与版本控制**
  - 实现 `create()` 根据 managed_mode 分支处理（paperclip_managed -> 生成值并存储；external -> 校验 external_ref）
  - 实现 Secret 版本控制：每次更新创建新版本记录（company_secret_versions 表）
  - 实现 `rotate()` 生成新值 / 调用 provider.rotate() / 创建新版本记录

- [x] **实现 Secret 绑定与注入**
  - 实现 `resolve_environment_secrets(environment_id)` 查询绑定 -> 批量 retrieve -> 注入环境变量
  - 实现 `resolve_secret_ref()` 运行时解析 Secret 引用为实际值
  - 实现日志脱敏：`redact_secret_value()` 值长度 <=4 返回 "****"，否则首尾各保留2字符

- [x] **实现 User Secret Definitions Repository**
  - 实现 user_secret_definitions 表 migration（id, company_id, name, key, description, required, created_at, updated_at）
  - 实现 user_secrets 表 migration（id, definition_id, user_id, value_ref, status, created_at, updated_at）
  - 实现 `UserSecretDefinitionRepository` trait（create, get_by_id, list_by_company, update, delete）

- [x] **实现用户密钥 CRUD 服务**
  - 实现 `UserSecretService::create_definition()` - 创建用户密钥定义
  - 实现 `UserSecretService::list_user_secrets()` - 列出当前用户的密钥值
  - 实现 `UserSecretService::upsert_user_secret()` - 创建或更新用户密钥值

- [x] **实现用户密钥统计功能**
  - 实现 `get_definition_coverage()` - 统计指定定义的用户填写覆盖率
  - 实现 `list_missing_definitions()` - 列出当前用户未填写的必填密钥
  - 实现用户密钥值的加密存储（复用 LocalEncryptedProvider）

- [x] **实现 Secret Provider Configuration 数据模型**
  - 实现 secret_provider_configs 表 migration（id, company_id, provider_type, name, config, is_default, status, created_at, updated_at）
  - 定义 `SecretviderConfig` 结构体（id, company_id, provider_type, name, config, is_default）
  - 定义 `ProviderHealthStatus` 枚举（healthy / degraded / unhealthy / unknown）

- [x] **实现 Provider Config Repository**
  - 实现 `SecretProviderConfigRepository` trait（create, get_by_id, list_by_company, update, delete, set_default）
  - 实现配置的加密存储（config 字段中的凭证信息加密）
  - 实现默认提供商切换逻辑（set_default 时取消其他配置的 is_default 标记）

- [x] **实现 Provider Discovery 与预览**
  - 实现 `discover_secrets_preview()` - 连接外部 provider 并列出可发现的密钥
  - 实现 `test_provider_health()` - 健康检查指定 provider 配置
  - 实现预览结果的缓存机制（避免频繁调用外部 API）

- [x] **实现 Remote Import 功能**
  - 实现 `remote_import_preview()` - 预览远程导入（匹配现有密钥，检测冲突）
  - 实现 `remote_import_execute()` - 执行批量导入（创建新密钥，跳过重复，记录导入日志）
  - 实现导入结果统计（成功数、跳过数、失败数）

### 阶段三：高级特性
- [x] **实现 Secret 热更新机制**
  - 实现 secret.rotated 事件发布（通过 WebSocket 推送）
  - 实现运行时服务订阅 secret 变更并重启
  - 实现优雅重启：等待当前请求完成再应用新 secret

- [x] **实现外部 Provider 集成**
  - 实现 `AwsSecretsManagerProvider`，满足 `SecretProvider` trait
  - 实现 `GcpSecretManagerProvider`，满足 `SecretProvider` trait
  - 实现 `VaultProvider`，满足 `SecretProvider` trait

- [x] **实现 Provider 配置管理 API**
  - 实现 GET `/companies/:companyId/secret-provider-configs` - 列出配置
  - 实现 POST `/companies/:companyId/secret-provider-configs` - 创建配置
  - 实现 PATCH `/secret-provider-configs/:id` - 更新配置
  - 实现 DELETE `/secret-provider-configs/:id` - 删除配置
  - 实现 POST `/secret-provider-configs/:id/default` - 设置默认提供商

- [x] **实现 Provider 健康检查与发现 API**
  - 实现 POST `/secret-provider-configs/:id/health` - 健康检查端点
  - 实现 GET `/companies/:companyId/secret-providers/health` - 所有提供商健康状态
  - 实现 POST `/companies/:companyId/secret-provider-configs/discovery/preview` - 预览发现结果

- [x] **实现用户密钥 API 端点**
  - 实现 GET `/companies/:companyId/user-secret-definitions` - 列出密钥定义
  - 实现 POST `/companies/:companyId/user-secret-definitions` - 创建定义
  - 实现 PATCH `/companies/:companyId/user-secret-definitions/:definitionId` - 更新定义
  - 实现 DELETE `/companies/:companyId/user-secret-definitions/:definitionId` - 删除定义
  - 实现 GET `/companies/:companyId/user-secret-definitions/:definitionId/coverage` - 覆盖率统计

- [x] **实现当前用户密钥 API 端点**
  - 实现 GET `/companies/:companyId/me/user-secrets` - 列出当前用户密钥
  - 实现 POST `/companies/:companyId/me/user-secrets` - 创建用户密钥
  - 实现 PATCH `/companies/:companyId/me/user-secrets/:secretId` - 更新用户密钥
  - 实现 POST `/companies/:companyId/me/user-secrets/:secretId/rotate` - 轮换用户密钥
  - 实现 DELETE `/companies/:companyId/me/user-secrets/:secretId` - 删除用户密钥

- [x] **实现 Remote Import API 端点**
  - 实现 POST `/companies/:companyId/secrets/remote-import/preview` - 预览导入
  - 实现 POST `/companies/:companyId/secrets/remote-import` - 执行导入
  - 实现导入进度跟踪与异步任务支持（大批量导入）

- [x] **实现严格模式与审计**
  - 实现 `PAPERCLIP_SECRETS_STRICT_MODE` 环境变量支持（严格模式下未绑定 Secret 阻止执行）
  - 实现 Secret 访问权限校验（公司级: 公司成员资格；用户级: 仅本人可访问）
  - 实现 Secret 操作审计日志（secret.created, secret.resolved, secret.rotated）
  - 实现用户密钥的权限校验（仅定义所属公司的成员可填写）

---

## 8. 资产管理服务层 实现任务

### 阶段一：基础架构
- [x] **定义 StorageService trait**
  - 定义 `StorageService` trait（put_file, get_file, delete_file）
  - 定义 `StoragePutResult` 结构体（provider, object_key, content_type, byte_size, sha256）
  - 定义 `MAX_ATTACHMENT_BYTES` 常量

- [x] **定义 AssetService trait**
  - 定义 `AssetService` trait（create, get_by_id, get_content, delete）
  - 定义 `CreateAssetInput` 结构体（company_id, content_type, body, original_filename, created_by）
  - 定义 `AssetContent` 结构体（content_type, body: Vec<u8>, sha256）

### 阶段二：核心功能
- [x] **实现文件上传与存储**
  - 实现 `POST /companies/:companyId/assets/images` 路由端点
  - 使用 axum::extract::Multipart 处理文件上传（内存模式 + 大小限制）
  - 实现 `put_file()` 调用 StorageService 存储文件并返回 StoragePutResult

- [x] **实现 SVG 安全处理**
  - 实现 `sanitize_svg_buffer(input: &[u8]) -> Option<Vec<u8>>` 移除 script / foreignObject / 事件处理器
  - 实现 SVG 外部 href 过滤（仅保留锚点链接）
  - 在上传路由中根据 content_type === "image/svg+xml" 自动调用 sanitization

- [x] **实现资产内容分发**
  - 实现 `GET /assets/:assetId/content` 路由端点
  - 实现流式传输大文件（使用 axum::body::BodyStream）
  - SVG 文件添加 CSP sandbox 头（Content-Security-Policy: sandbox）

### 阶段三：高级特性
- [x] **实现外部存储后端**
  - 实现 `LocalStorageProvider`（本地文件系统存储）
  - 实现 `S3StorageProvider`（AWS S3 存储）
  - 实现 `GcsStorageProvider`（Google Cloud Storage 存储）

---

## 9. 文件资源服务层 实现任务

### 阶段一：基础架构
- [x] **定义文件资源核心类型**
  - 定义 `WorkspaceCandidate` 结构体（workspace_kind, workspace_id, project_id, provider, root_path, remote）
  - 定义 `WorkspaceKind` 枚举（execution_workspace / project_workspace）
  - 定义 `FileResourceProvider` 枚举（local_fs / git_worktree）

- [x] **定义 WorkspaceFileResourcesService trait**
  - 定义 `WorkspaceFileResourcesService` trait（list_files, preview_file, download_file）
  - 定义 `FileEntry` 结构体（name, path, is_dir, size, modified_at）
  - 定义 `FilePreview` 结构体（content, language, truncated）

### 阶段二：核心功能
- [x] **实现文件列表与预览**
  - 实现 `GET /workspace-file-resources/list` 路由端点（支持子目录浏览）
  - 实现 `GET /workspace-file-resources/preview` 路由端点（文件内容预览 + 截断）
  - 实现 local_fs provider 的文件读取（使用 tokio::fs）

- [x] **实现文件下载**
  - 实现 `GET /workspace-file-resources/download` 路由端点
  - 实现流式文件下载（大文件分块传输）
  - 实现 git_worktree provider 的文件读取（通过 git show 命令）

### 阶段三：高级特性
- [x] **实现文件资源访问控制**
  - 实现 Workspace 级别的文件访问权限校验
  - 实现路径遍历攻击防护（确保请求路径不超出 workspace root_path）
  - 实现文件类型限制（禁止访问 .git 目录等敏感路径）

---

## 10. 授权服务层 实现任务

### 阶段一：基础架构
- [x] **定义工作空间授权核心类型**
  - 定义 `CommandAuthzRequest` 结构体（workspace_id, command, agent_id）
  - 定义 `RuntimeServiceAuthzRequest` 结构体（workspace_id, service_name, action, agent_id）
  - 定义 `AuthzDecision` 结构体（allowed, reason）

- [x] **定义授权服务接口**
  - 定义 `WorkspaceCommandAuthzService` trait（check_command_permission）
  - 定义 `WorkspaceRuntimeServiceAuthzService` trait（check_runtime_service_permission）
  - 定义授权策略配置结构体（allowed_commands, denied_commands, allowed_actions）

### 阶段二：核心功能
- [x] **实现命令执行权限检查**
  - 实现 `POST /workspace-command-authz/check` 路由端点
  - 实现命令模式匹配（通配符 + 正则表达式）
  - 实现危险命令拦截（rm -rf /, sudo, chmod 777 等）

- [x] **实现运行时服务权限检查**
  - 实现 `POST /workspace-runtime-service-authz/check` 路由端点
  - 实现 runtime:manage 权限校验（调用 accessService.decide）
  - 实现服务操作白名单校验（start / stop / restart / run）

### 阶段三：高级特性
- [x] **实现动态授权策略**
  - 实现基于 Agent 角色的差异化授权策略
  - 实现授权策略热更新（无需重启服务）
  - 实现授权审计日志（记录每次权限检查结果）

---

## 11. 自定义镜像管理 实现任务

### 阶段一：基础架构
- [x] **定义自定义镜像核心类型**
  - 定义 `CustomImageSetupSession` 结构体（id, environment_id, status, created_at, updated_at）
  - 定义 `SetupSessionStatus` 枚举（pending / in_progress / completed / cancelled / failed）
  - 定义 `TerminalSessionToken` 结构体（session_id, token, expires_at）

### 阶段二：核心功能
- [x] **实现自定义镜像设置会话**
  - 实现 `POST /environments/:environmentId/custom-image-setup-sessions` 创建设置会话
  - 实现 `POST /environment-custom-image-setup-sessions/:sessionId/finish` 与 `/cancel` 完成或取消会话
  - 实现会话状态跟踪（pending -> in_progress -> completed / cancelled / failed）

- [x] **实现自定义镜像模板与详情查询**
  - 实现 `GET /environments/:environmentId/custom-image-template` 获取镜像模板概览
  - 实现 `GET /environment-custom-image-setup-sessions/:sessionId` 获取会话详情
  - 实现会话超时自动取消逻辑

- [x] **实现终端会话 Token 管理**
  - 实现 `POST /environment-custom-image-setup-sessions/:sessionId/terminal-session-token` 创建终端 Token
  - 使用 JWT 生成短期 token（TTL: 5分钟）
  - 实现 token 验证与 WebSocket 认证集成

### 阶段三：高级特性
- [x] **实现镜像构建流水线**
  - 实现镜像构建状态机（pending -> building -> pushing -> completed / failed）
  - 实现构建日志流式推送（通过 SSE）
  - 实现镜像版本管理与回滚

---

## 12. 工作空间操作记录层 实现任务

### 阶段一：基础架构
- [x] **定义操作记录核心类型**
  - 定义 `WorkspaceOperation` 结构体（id, company_id, execution_workspace_id, phase, command, status, started_at, completed_at, metadata）
  - 定义 `OperationPhase` 枚举（workspace_provision / workspace_teardown / runtime_start / runtime_stop）
  - 定义 `OperationStatus` 枚举（in_progress / completed / failed）

### 阶段二：核心功能
- [x] **实现操作记录器**
  - 实现 `WorkspaceOperationService` trait（create_recorder, record_operation, list_operations）
  - 实现 `OperationRecorder` 结构体（company_id, execution_workspace_id, db_pool）
  - 实现 `record_operation()` 插入 workspace_operations 记录

- [x] **实现操作记录查询端点**
  - 实现 `GET /execution-workspaces/:id/workspace-operations` 路由端点
  - 实现按时间倒序分页查询
  - 实现操作详情包含 phase / command / duration 等信息

### 阶段三：高级特性
- [x] **实现操作统计与告警**
  - 实现工作空间操作耗时统计（P50 / P95 / P99）
  - 实现异常操作检测（频繁 start/stop 循环、长时间 in_progress）
  - 实现操作失败告警与自动重试机制

---

## 13. 路由与请求验证层 实现任务

### 阶段一：基础架构
- [x] **定义验证 Schema**
  - 使用 garde / validator crate 定义 `CreateEnvironmentSchema`
  - 定义 `AcquireLeaseSchema` 与 `StartRuntimeServiceSchema` 验证规则
  - 定义 `CreateSecretSchema` 与 `UploadAssetSchema` 验证规则

- [x] **搭建路由框架**
  - 使用 axum 定义环境管理路由组（`/api/companies/:companyId/environments`）
  - 定义执行工作空间路由组（`/api/execution-workspaces`）
  - 实现 `CompanyId` / `EnvironmentId` / `WorkspaceId` 路径参数提取器

### 阶段二：核心功能
- [x] **实现环境健康探测集成**
  - 实现 `POST /environments/:id/probe` 环境探测端点
  - 实现探测结果解析与状态更新（ok -> active, failed -> error）
  - 定义探测触发场景（手动触发、租约获取前、定期健康检查）

- [x] **实现环境管理路由端点**
  - 实现 `GET /companies/:companyId/environments` 列出公司所有环境
  - 实现 `GET /environments/:id/delete-blast-radius` 获取环境删除影响分析
  - 实现 `GET /companies/:companyId/environments/capabilities` 获取环境能力清单
  - 实现 `POST /environments/:id/acquire` 获取租约端点

- [x] **实现执行工作空间路由端点**
  - 实现 `GET /companies/:companyId/execution-workspaces` 列出执行工作空间
  - 实现 `GET /execution-workspaces/:id` 获取工作空间详情
  - 实现 `GET /execution-workspaces/:id/close-readiness` 获取关闭就绪状态

- [x] **实现 Secrets 与资产路由端点**
  - 实现 `GET /companies/:companyId/secrets` 列出公司 Secrets
  - 实现 `POST /companies/:companyId/secrets` 创建公司 Secret
  - 实现 `POST /companies/:companyId/assets/images` 上传图片资产

### 阶段三：高级特性
- [x] **实现环境健康探测调度器**
  - 实现后台任务（tokio::spawn），每 5 分钟探测所有 active 环境
  - 更新 environments.status 基于探测结果
  - 发送告警当探测连续失败 3 次
  - 实现探测结果缓存（避免频繁探测同一环境）

- [x] **实现工作空间空闲回收器**
  - 实现后台任务，每 10 分钟扫描无活动工作空间
  - 空闲阈值：30 分钟无 runtime service 活动
  - 自动调用 teardown 并通知所有者

- [x] **实现高级路由端点**
  - 实现 `GET /companies/:companyId/workspace-overview` 工作空间概览
  - 实现 `GET /companies/:companyId/secret-providers/health` Provider 健康检查
  - 实现 `POST /companies/:companyId/secrets/remote-import` 远程导入 Secrets

---

## 依赖顺序总览

```
阶段一（基础架构）推荐实现顺序:

  1. 数据模型层 (枚举 + 结构体定义 + Repository trait)
  2. 环境驱动层 (trait + Registry + 配置解析)
  3. 租约管理服务层 (trait + 状态机)
  4. 执行工作空间服务层 (trait + 核心类型)
  5. Secrets 管理服务层 (Provider trait + Service trait)
  6. 资产管理服务层 (StorageService trait + AssetService trait)
  7. WebSocket 实时通信层 (Session 类型 + 升级认证)
  8. SSE 流式事件层 (帧类型 + 解析器)
  9. 文件资源服务层 (核心类型 + trait)
  10. 授权服务层 (核心类型 + trait)
  11. 自定义镜像管理 (核心类型)
  12. 工作空间操作记录层 (核心类型)
  13. 路由与请求验证层 (Schema + 路由框架)

阶段二（核心功能）推荐实现顺序:

  1. 数据模型层 (Schema migration + Secrets/Assets 模型)
  2. 环境驱动层 (Local Driver + SSH Driver)
  3. 租约管理服务层 (获取流程 + 心跳保活)
  4. 执行工作空间服务层 (可用性保证 + 运行时服务启动)
  5. Secrets 管理服务层 (Local Encrypted Provider + CRUD + 绑定注入)
  6. 资产管理服务层 (文件上传 + SVG 安全 + 内容分发)
  7. WebSocket 实时通信层 (双向消息 + 心跳重连)
  8. SSE 流式事件层 (流创建推送 + 敏感信息过滤)
  9. 文件资源服务层 (列表预览 + 下载)
  10. 授权服务层 (命令权限 + 运行时权限)
  11. 自定义镜像管理 (设置会话 + 模板查询)
  12. 工作空间操作记录层 (记录器 + 查询端点)
  13. 路由与请求验证层 (环境 + 工作空间 + Secrets/Assets 端点)

阶段三（高级特性）推荐实现顺序:

  1. 数据模型层 (JSONB 类型安全映射)
  2. 环境驱动层 (Plugin Driver)
  3. 租约管理服务层 (清理与恢复)
  4. 执行工作空间服务层 (双写持久化 + 分支协调)
  5. Secrets 管理服务层 (外部 Provider + 严格模式 + 审计)
  6. 资产管理服务层 (外部存储后端)
  7. WebSocket 实时通信层 (频道订阅 + 事件过滤 + 背压)
  8. SSE 流式事件层 (连接管理 + 背压控制)
  9. 文件资源服务层 (访问控制 + 路径防护)
  10. 授权服务层 (动态策略 + 审计)
  11. 自定义镜像管理 (构建流水线)
  12. 工作空间操作记录层 (统计告警)
  13. 路由与请求验证层 (高级端点)
```

---

## Rust 技术选型建议

| 领域 | 推荐选型 | 说明 |
|------|----------|------|
| Web 框架 | axum | 生态成熟，原生支持 WebSocket 与 SSE |
| WebSocket | tokio-tungstenite | axum 集成良好，异步性能优秀 |
| SSE | axum::response::sse | axum 内置 SSE 支持 |
| ORM | sea-orm 或 sqlx | sea-orm 动态查询更强；sqlx 编译时 SQL 检查 |
| 请求验证 | garde 或 validator | garde 更现代，支持嵌套结构验证 |
| 错误处理 | thiserror + anyhow | thiserror 定义业务错误，anyhow 用于内部 |
| 序列化 | serde + serde_json | JSONB 字段统一用 serde 映射 |
| 异步运行时 | tokio | 标准选择，子进程管理用 tokio::process |
| 加密 | aes-gcm crate | AES-256-GCM 加密用于 Secret 本地存储 |
| SVG 安全 | loopy-svg 或手动解析 | 移除危险元素（script/foreignObject/事件属性） |
| SSH | ssh2 crate | SSH 连接与远程命令执行 |
| 进程管理 | tokio::process::Command | 运行时服务子进程启动与管理 |
| 并发容器 | dashmap | 内存 Map 双写场景的并发安全 HashMap |
| 文件上传 | axum::extract::Multipart | axum 原生 Multipart 提取器 |
| 流式传输 | axum::body::BodyStream | 大文件流式下载 |
| 数据库迁移 | sea-orm-cli 或 sqlx-cli | 与 ORM 选择配套 |
| 测试 | testcontainers + mockall | 集成测试用 testcontainers，单测用 mockall |
