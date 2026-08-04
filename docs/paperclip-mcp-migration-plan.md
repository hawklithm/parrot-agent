# Paperclip MCP 完整迁移计划

> 本文用于将 Paperclip 的完整 MCP 工具能力迁移到 `parrot-agent`，方便其他 Agent 接手。
>
> 约束：只允许修改 `/Users/adazhao/workspace/parrot-agent`；禁止修改 `/Users/adazhao/workspace/paperclip`。所有 Paperclip 代码仅作为只读参考。

> 执行状态（2026-08-05）：已完成 gateway token actor、41 个 Paperclip 内置工具注册、主要 REST bridge、统一 typed tool registry、基础 schema/参数校验、审计、MCP session 生命周期、批量 JSON-RPC、SSE 长连接 GET，以及本地 Claude 的真实 token/端口注入验证。仍未宣称完成全部 Paperclip 生产级 response/error 契约、跨公司负向测试和 Claude/Codex 长流程 E2E。

## 1. 目标与非目标

### 1.1 目标

- [x] 在 `parrot-agent` 中提供 Paperclip 当前 41 个内置工具的注册集合；逐工具生产级契约验收仍未完成。
- [x] 本地 `claude` 已通过 `/api/tool-gateway/mcp` 启动并加载 Paperclip 工具集合；完整读写长流程仍需补测。
- [x] 保留当前 run 级别的工具网关 Token 隔离能力。
- [x] 工具调用上下文绑定 `company_id`、`agent_id`、`run_id` 和关联 Issue。
- [x] 保留工具策略、审批、调用记录和失败结果的基础链路。
- [x] 支持 MCP Streamable HTTP 的 session、批量 JSON-RPC、Accept 协商、JSON/SSE 响应、SSE GET 长连接和 DELETE 关闭；服务端主动事件队列仍是后续增强项。
- [ ] 为每个迁移工具提供生产级参数校验、错误映射和自动化测试；当前已有注册级和基础参数测试。

### 1.2 非目标

- [x] 不修改 Paperclip 源码、数据库或依赖。
- [x] 不把 Paperclip 的 TypeScript 代码在运行时作为 `parrot-agent` 的依赖。
- [x] 不绕过现有授权层直接暴露数据库查询。
- [x] 不把 `PAPERCLIP_TOOL_GATEWAY_TOKEN`、API Key 或其他密钥写入普通日志。

## 2. 当前现状

### 2.1 当前任务执行链路

- [x] Issue 创建/更新后，通过 `LegacyIssueService::wake_assigned_issue` 唤醒已分配 Agent。
  - 参考：`crates/services/src/issue_service_complete.rs:749`
- [x] `DefaultHeartbeatService::wakeup` 创建 `heartbeat_runs` 和 `agent_wakeup_requests`。
  - 参考：`crates/services/src/heartbeat_service.rs:526`
- [x] Issue 被设置为 `in_progress`，并写入 `checkout_run_id` / `execution_run_id`。
- [x] 本地 adapter 通过 `tokio::process::Command` 启动 `claude`、`codex` 或其他 CLI。
  - 参考：`crates/services/src/heartbeat_service.rs:195-500`
- [x] `claude_local` 使用 stdin 传入 prompt，`codex_local` 将 prompt 作为命令参数传入。
- [x] 子进程退出码为 0 时将 Issue 设置为 `done`，非 0 时恢复为 `todo`。
- [x] 服务启动时会重新唤醒 `todo + assignee_agent_id` 的任务。
  - 参考：`crates/services/src/heartbeat_service.rs:611`
- [x] 任务成功/失败后已生成并更新 `continuation-summary` Issue document；并发锁和完整 Paperclip outcome 仍需补强。
- [x] 当前执行结果以进程退出码为基础，并已解析 Claude/Codex JSONL 的显式 error/result、tool call 和 handoff 记录；Paperclip 更丰富的 outcome 字段仍需继续持久化。

### 2.2 当前 Tool Gateway 实现

- [x] `tool_routes()` 已注册 `/tool-gateway/mcp`。
  - 参考：`crates/api/src/routes/tools.rs:901`
- [x] 主 Router 通过 `.nest("/api", api_routes)` 挂载，因此实际地址是 `POST /api/tool-gateway/mcp`。
  - 参考：`crates/api/src/app_state.rs:251-331`
- [x] 当前 MCP handler 支持以下 JSON-RPC 方法：
  - [x] `initialize`
  - [x] `notifications/initialized`
  - [x] `tools/list`
  - [x] `tools/call`
  - 参考：`crates/api/src/routes/tools.rs:209-260`
- [x] 支持 `Authorization: Bearer <token>` 和 `x-paperclip-tool-gateway-token`。
- [x] `tool_gateway_sessions` 保存 token hash、公司、Agent、run、Issue、过期时间和撤销时间。
- [x] 工具网关会校验 token 是否存在、过期或撤销，并更新 `last_used_at`。
- [x] `tools/list` 当前可以列出：
  - [x] 已启用插件 manifest 中的工具。
  - [x] 已启用的外部 MCP connection 工具。
  - [x] 工具策略允许的工具。
- [x] `tools/call` 当前可以调用：
  - [x] 插件工具。
  - [x] `mcp.<connectionUid>:<upstreamToolName>` 外部 MCP 工具。
  - [x] 远程 HTTP MCP connection。
  - [x] 本地 stdio MCP connection。
- [x] 工具调用会写入 `tool_invocations`、`tool_call_events`，并处理 deny / require approval / allow。
- [x] 当前 `tools/list` 已开始暴露 Paperclip 的内置 `paperclip*` 工具集合。
- [x] 当前 `tools/call` 已开始分发 Paperclip 内置工具，并通过 gateway token 转发到 parrot-agent REST contract。
- [ ] 内置工具已覆盖 Paperclip 当前 41 个名称和主要 schema，但所有 REST response/error contract 尚未逐项完成验收。
- [x] 当前实现已增加 MCP session info、DELETE close 和 `Mcp-Session-Id` 响应头；三个 MCP 入口均接入同一处理器。
- [x] 当前实现支持 JSON/SSE POST 响应、SSE GET 长连接、KeepAlive、`Mcp-Protocol-Version` 和批量 JSON-RPC；主动事件路由仍未接入。
- [x] 当前实现已覆盖 `notifications/initialized`、未知 method、解析错误、会话错误、JSON-RPC 错误码和响应 envelope 的基础行为；仍需真实 Claude/Codex 客户端逐项验收。

### 2.3 当前 Token 设计

- [x] 本地 adapter 启动时生成 `ptg_<random>` token。
  - 参考：`crates/services/src/heartbeat_service.rs:357`
- [x] 数据库只保存 SHA-256 hash。
- [x] 明文 token 通过 `PAPERCLIP_TOOL_GATEWAY_TOKEN` 注入 Claude/Codex 子进程。
- [x] Claude MCP 配置使用 `Authorization: Bearer ${PAPERCLIP_TOOL_GATEWAY_TOKEN}`。
- [x] run 结束时撤销对应的 `tool_gateway_sessions`。
- [x] gateway token 已加入统一 Agent actor 解析，内置工具请求可以复用普通 REST 授权层。
  - 实现位置：`crates/services/src/auth/middleware.rs`
- [ ] 仍需补充 gateway token 的过期、撤销、跨公司、跨 Agent、跨 run 负向测试；当前已实现查询和 actor 绑定。

## 3. Paperclip 参考实现

### 3.1 参考文件

- [x] 阅读并以以下文件为唯一功能参考，不修改这些文件：
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/index.ts`
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/tools.ts`
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/client.ts`
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/config.ts`
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/format.ts`
  - `/Users/adazhao/workspace/paperclip/packages/mcp-server/README.md`
- [x] 对照 Paperclip API handler 和 shared schema，确认每个工具对应的 parrot-agent endpoint 与字段名；对 parrot-agent 不同的字段名通过 bridge 做显式转换。

### 3.2 Paperclip 实现的关键特征

- [x] Paperclip MCP Server 是一个薄 REST API wrapper，不直接访问数据库。
- [x] MCP 工具通过 `McpServer` 注册，工具参数由 Zod schema 校验。
- [x] 每个工具调用 Paperclip REST API，并将错误转换为 MCP text response。
- [x] 写请求自动透传 `X-Paperclip-Run-Id`。
- [x] `PAPERCLIP_COMPANY_ID`、`PAPERCLIP_AGENT_ID`、`PAPERCLIP_RUN_ID` 可以作为默认上下文。
- [x] Paperclip 提供 `paperclipApiRequest` 作为受限 API escape hatch。
- [x] `parrot-agent` 已在 Rust 中重建 41 个工具注册、主要 schema、上下文解析和 REST 转发行为。
  - 当前实现位置：`crates/api/src/routes/tools.rs`
  - 完整协议兼容和逐工具验收仍未完成。

## 4. 完整工具迁移清单

以下清单来自 Paperclip `packages/mcp-server/src/tools.ts`。每个条目完成前必须同时具备：工具定义、input schema、调用映射、权限检查、错误处理和测试。

### 4.1 身份与 Agent

- [ ] `paperclipMe`
  - GET `/agents/me`
  - 验证当前 Agent actor 与 run 上下文。
- [ ] `paperclipInboxLite`
  - GET `/agents/me/inbox-lite`
- [ ] `paperclipListAgents`
  - GET `/companies/:companyId/agents`
- [ ] `paperclipGetAgent`
  - GET `/agents/:agentId`

### 4.2 Issue 与执行

- [ ] `paperclipListIssues`
  - 支持 company、status、project、assignee、participant、label、workspace、origin、query 等过滤字段。
- [ ] `paperclipGetIssue`
  - 支持 UUID 和业务 identifier。
- [ ] `paperclipGetHeartbeatContext`
  - 支持 `wakeCommentId`。
- [ ] `paperclipCreateIssue`
  - 创建后触发现有 Agent wakeup 逻辑。
- [ ] `paperclipUpdateIssue`
  - 支持 status、assignee、comment、resume 等字段。
- [ ] `paperclipCheckoutIssue`
  - 默认 `expectedStatuses` 为 `todo`、`backlog`、`blocked`。
  - 必须绑定当前 `agentId` 和 `runId`。
- [ ] `paperclipReleaseIssue`
  - 将 release result 正确映射到 done / todo / cancelled。
- [ ] `paperclipListComments`
- [ ] `paperclipGetComment`
- [ ] `paperclipAddComment`
- [ ] `paperclipSuggestTasks`
- [ ] `paperclipAskUserQuestions`
- [ ] `paperclipRequestConfirmation`
- [ ] `paperclipRequestCheckboxConfirmation`

### 4.3 Documents

- [x] `paperclipListDocuments` 的 REST endpoint 已存在。
- [x] `paperclipGetDocument` 的 REST endpoint 已补充。
- [x] `paperclipListDocuments` 和 `paperclipGetDocument` 已加入 MCP 工具注册并映射 REST endpoint。
- [x] `paperclipListDocumentRevisions`
- [x] `paperclipUpsertIssueDocument`
- [x] `paperclipRestoreIssueDocumentRevision`
- [x] 校验 document key：非空、最大 64 字符、限定为小写字母/数字/`_`/`-`。
- [x] 校验 body 最大 524288 字符，format 目前只允许 markdown，并按 `baseRevisionId` 做乐观并发冲突检查。

### 4.4 Project、Goal 与 Approval

- [ ] `paperclipListProjects`
- [ ] `paperclipGetProject`
- [ ] `paperclipListGoals`
- [ ] `paperclipGetGoal`
- [ ] `paperclipListApprovals`
- [ ] `paperclipCreateApproval`
- [ ] `paperclipGetApproval`
- [ ] `paperclipGetApprovalIssues`
- [ ] `paperclipListApprovalComments`
- [ ] `paperclipLinkIssueApproval`
- [ ] `paperclipUnlinkIssueApproval`
- [ ] `paperclipApprovalDecision`
  - 支持 approve、reject、requestRevision、resubmit。
- [ ] `paperclipAddApprovalComment`

### 4.5 Execution Workspace

- [ ] `paperclipGetIssueWorkspaceRuntime`
- [ ] `paperclipControlIssueWorkspaceServices`
  - 支持 start、stop、restart。
- [ ] `paperclipWaitForIssueWorkspaceService`
  - 支持 timeout、runtimeServiceId、serviceName。

### 4.6 Escape hatch

- [ ] `paperclipApiRequest`
  - 只允许 `/api` 相对路径。
  - 拒绝 `..` 路径穿越。
  - 只允许 GET、POST、PUT、PATCH、DELETE。
  - JSON body 必须合法。
  - 写请求自动带当前 run id。

## 5. 推荐迁移架构

### 5.1 工具注册层

- [x] 新增独立模块 `crates/api/src/mcp/`，承载 `McpInvocationContext` 和 request/notification 类型；Axum 路由和旧 gateway 兼容逻辑暂仍在 `routes/tools.rs`，后续可继续拆分 registry/dispatcher。
- [x] 定义独立的统一工具结构 `McpToolDefinition`，由 typed registry 统一生成 `tools/list` wire representation：

```text
McpToolDefinition {
  name,
  description,
  input_schema,
}
```

- [x] 将 Paperclip 工具定义集中存放，确保 `tools/list` 和 `tools/call` 使用同一份注册表。
- [x] 工具名称保持 Paperclip 原名，不能改成 Rust 风格名称。
- [x] input schema 使用 MCP JSON Schema 表示；仍需补齐复杂 payload 的逐字段 schema。

### 5.2 工具调用层

- [x] 定义统一调用上下文：

```text
McpInvocationContext {
  session_id,
  company_id,
  agent_id,
  run_id,
  issue_id,
}
```

- [x] 所有内置工具先解析 required 字段、文档 key/body、approval action、JSON body、HTTP method 和主要字段类型，再调用现有 service/API contract；复杂 payload 仍需补充逐字段 JSON Schema 校验。
- [ ] 优先调用 `AppState` 中已有 service，避免通过 HTTP 回环调用自身。
- [ ] 如果必须复用 HTTP contract，增加专用内部调用层，不能通过伪造 Board API Key 绕过认证。
- [x] 所有写操作都要检查当前 Agent 是否属于 session 的 Agent 和 company；gateway actor 与 REST handler 均使用 session-derived actor。
- [x] Issue 相关工具必须额外检查 Issue 是否属于 session company；Issue、comment、document、approval 子资源已补齐 company scope。
- [x] 需要 run 的工具必须检查 `run_id` 存在且处于 queued/running 状态。

### 5.3 MCP Streamable HTTP 层

- [x] 保留 `POST /api/tool-gateway/mcp`。
- [x] 增加 MCP session 建立和 `Mcp-Session-Id` 响应。
- [x] 处理客户端带 session id 的后续请求，并确认 session 属于同一个 gateway token。
- [x] 按 MCP 客户端要求处理 `Accept: application/json, text/event-stream`；明确拒绝不支持的媒体类型。
- [x] 支持 JSON/SSE 响应；GET SSE 保持连接并发送 KeepAlive。
- [x] 正确区分 request、notification 和 response；`notifications/initialized` 不返回普通 JSON-RPC response。
- [x] 对未知 method 返回标准 JSON-RPC method-not-found error。
- [x] 对无效 JSON、缺少 `jsonrpc`、缺少 request id、参数类型错误返回标准 JSON-RPC parse/invalid-params error；合法 notification 按 JSON-RPC 规则返回 202 空响应。
- [x] 提供 `GET /api/tool-gateway/mcp` session info/SSE stream，并用 `DELETE /api/tool-gateway/mcp` 关闭 session。
- [ ] 为 Claude 和 Codex 各自验证实际握手流程，不能只用手写 curl 验证。

### 5.4 鉴权与上下文

- [x] token 只保存 hash，响应和日志不输出明文。
- [x] gateway token 解析后生成受限 Agent actor，来源标记为临时 token。
- [x] token actor 的 `run_id` 必须来自数据库，不接受请求体覆盖。
- [x] 绑定关系至少包括 company、agent、run、issue。
- [x] run 完成、失败、取消和超时都要撤销 token/session。
- [x] 所有工具调用日志记录 session、run、agent、company、tool name、decision、outcome。
- [x] 工具参数日志只记录摘要，禁止记录 password、API key、Bearer token 和完整敏感内容。

## 6. 工具与现有 parrot-agent API 对照步骤

- [ ] 为每个 Paperclip tool 建立以下映射表：

```text
MCP tool name
Paperclip method/path
parrot-agent method/path 或 service
input schema
company scope
agent scope
run scope
write/read
expected response
error mapping
```

- [ ] 检查 REST 路径是否已经存在，不能因为前端路由存在就假设 MCP contract 已存在。
- [ ] 检查返回 envelope：Paperclip 期望数组、对象还是 `{result: ...}`。
- [ ] 检查 camelCase 字段名。
- [ ] 检查 UUID、日期、枚举和 nullable 字段。
- [ ] 检查 Paperclip 对 404、400、401、403、409、422 的错误语义。
- [ ] 对缺失 endpoint 先补 API/service，再接入 MCP tool，避免在 MCP 层直接写重复 SQL。

## 7. 数据库与持久化检查

- [ ] 确认 `tool_gateway_sessions` 的完整字段与迁移一致。
- [ ] 确认 `tool_invocations`、`tool_call_events`、`tool_action_requests` 可覆盖成功、失败、拒绝和审批状态。
- [ ] 确认 `tool_connections`、`tool_profiles`、`tool_profile_entries`、`tool_policies` 的字段与查询一致。
- [x] 明确复用现有 `tool_gateway_sessions` 作为 MCP session：其 id 是 `Mcp-Session-Id`，并更新 `last_used_at`；目前不额外引入独立 MCP session 表。
- [x] 确认旧 `parrot_agent_dev` 能通过迁移启动，不使用破坏性删表重建。
- [x] 新增迁移使用 `IF NOT EXISTS` 或安全的数据回填策略；本地 PostgreSQL 启动已验证。

## 8. 测试计划

### 8.1 协议测试

- [ ] 无 token 调用 `initialize` 返回标准 unauthorized JSON-RPC error。
- [ ] 错误 token 调用 `initialize` 返回 unauthorized。
- [ ] 过期 token 调用 `initialize` 返回 unauthorized。
- [ ] 撤销 token 调用 `initialize` 返回 unauthorized。
- [ ] 正确 token 完成 initialize handshake。
- [ ] `tools/list` 返回全部 Paperclip 内置工具、插件工具和外部 MCP 工具。
- [ ] `tools/call` 能调用一个只读工具。
- [ ] `tools/call` 能调用一个写工具。
- [ ] `tools/call` 参数不符合 schema 时返回 invalid params。
- [ ] notification 不产生错误的普通 response。
- [ ] 未知 method 返回 method-not-found。
- [ ] Streamable HTTP JSON response 和 SSE response 都能被客户端解析。

### 8.2 权限测试

- [ ] Agent 不能读取其他公司的 Issue。
- [ ] Agent 不能使用其他 Agent 的 run id。
- [ ] Agent 不能伪造 companyId、agentId 或 issueId 绕过 session scope。
- [ ] 被 deny 的工具不会实际执行。
- [ ] require approval 的工具会生成 action request，未批准前不会执行。
- [ ] 审批拒绝后调用状态和审计记录正确。
- [ ] session 撤销后不能继续调用。
- [ ] run 完成后 token 不能继续使用。

### 8.3 业务工具测试

- [ ] 用 MCP 创建 Issue，并验证数据库记录和返回结构。
- [ ] 用 MCP 更新 Issue 状态和 assignee，并验证 Agent wakeup。
- [ ] 用 MCP checkout/release Issue，并验证执行字段。
- [ ] 用 MCP 添加评论，并验证 actor/run 归属。
- [ ] 用 MCP 创建、读取、更新、恢复文档 revision。
- [ ] 用 MCP 创建审批并完成 approve/reject/request revision/resubmit。
- [ ] 用 MCP 创建交互并验证 wake assignee 行为。
- [ ] 用 MCP 查询 heartbeat context 和 workspace runtime。
- [ ] 用 `paperclipApiRequest` 验证路径限制、JSON 校验和 run header 透传。

### 8.4 端到端测试

- [ ] 启动本地 PostgreSQL 和 `parrot-agent`。
- [ ] 创建或复用公司、Agent、Issue。
- [ ] 启动本地 Claude adapter。
- [ ] 验证 Claude 收到完整 Paperclip 工具列表。
- [ ] 让 Claude 调用 `paperclipGetIssue`。
- [ ] 让 Claude 调用 `paperclipAddComment`。
- [ ] 让 Claude 创建子任务并验证新任务被唤醒。
- [ ] 验证完整 stdout/stderr、tool invocation、run status 和 Issue status。
- [ ] 验证服务重启后的待执行任务恢复。
- [ ] 验证 token 不出现在普通日志和数据库明文列中。

## 9. 当前执行证据与接手说明

### 9.1 已落地文件

- [x] `crates/api/src/routes/tools.rs`：41 个 Paperclip 工具注册、参数校验、REST bridge、策略/审计、MCP session JSON-RPC 入口。
- [x] `crates/api/src/mcp/mod.rs`：`McpToolDefinition`、run-scoped `McpInvocationContext` 和 request/notification 分类。
- [x] `crates/api/src/routes/issues.rs`：Issue document GET/PUT、revision list/restore、Paperclip interaction kinds、Issue 查询 status/priority/q。
- [x] `crates/api/src/routes/approvals.rs`：审批 status 过滤、创建/关联多个 issue、approve/reject/revision/resubmit。
- [x] `crates/services/src/heartbeat_service.rs`：真实 gateway token 注入、URL 注入、日志脱敏、continuation summary。
- [x] `crates/services/src/auth/middleware.rs`：`ptg_` token hash 查询、过期/撤销校验和 Agent actor 解析。
- [x] `migrations/20260804000001_complete_auth_users.sql`：旧数据库缺失 auth user 字段的兼容迁移。
- [x] `migrations/20260804000002_complete_issue_interactions.sql`：Paperclip interaction kinds、payload、幂等键和 continuation policy。
- [x] Issue comments 已按 Paperclip `comment_actor_type`/`actor_id`/`actor_run_id` 数据库契约显式投影，兼容旧模型字段，不再依赖错误的 `SELECT *`。
- [x] MCP 已增加 JSON-RPC batch、Accept 406、`Mcp-Protocol-Version`、named gateway scope 校验和保持连接的 SSE GET。
- [x] 文档 PUT 已支持 `baseRevisionId` 乐观并发检查，并把 gateway run 的 Agent 归属写入 revision provenance。
- [x] adapter 已解析 Claude/Codex JSONL 的显式 result/error、tool call 和 handoff 记录，并写入 `heartbeat_runs.result_json`；零退出码不再覆盖显式失败。
- [x] Approval 的 issue 查询、approve、reject、request-revision 路径均先校验当前 actor 的 company scope。
- [x] Issue 文档、revision、heartbeat context 和 comment REST 子资源均校验 gateway actor 的 company scope，跨公司 UUID 返回 404/403，不再只按资源 UUID 查询。
- [x] checkout/release 已校验 Agent actor 与 run id，checkout 写入 `assignee_agent_id`、`checkout_run_id`、`execution_run_id`，release 清理执行锁字段。
- [x] `paperclipListIssues` 已把 status、priority、assignee、project、goal、parent、query、participant、label、execution workspace 和 origin filters 传入 repository；数据库层使用 company-scoped `EXISTS`/字段条件。
- [x] 对 create/update issue、approval、UUID 数组和日期/枚举字段补充 Paperclip shared schema 对应的 runtime 校验；invalid params 会在实际 REST bridge 前被拒绝。

### 9.2 已执行验证

- [x] 本地 PostgreSQL `parrot_agent_dev` 上服务启动并执行 migrations。
- [x] `cargo check -p parrot-server` 通过；仅有既存 `sqlx-postgres` future-incompatibility 警告。
- [x] `cargo test -p api --lib` 通过，当前 55 个测试通过。
- [x] gateway token 脱敏测试通过；真实 adapter argv 保留实际 token，普通日志只显示 `[PAPERCLIP_TOOL_GATEWAY_TOKEN]`。
- [x] 本地 Claude adapter 真实启动成功，MCP URL 指向当前 parrot-agent 端口，并在 run 输出中加载 Paperclip 工具集合。
- [x] MCP 协议手工验证覆盖 initialize、initialized notification、tools/list、tools/call、invalid args、unknown method、malformed JSON、JSON/SSE response；API 单元测试当前 57 个通过。
- [x] heartbeat run 完成后可读取 `continuation-summary` 文档。
- [x] `cargo test --workspace --no-fail-fast` 已执行：206 个测试通过，19 个既存 services 测试失败；失败集中在 `adapter_config_normalizer`、`codex_local_isolation`、`consistency_service` 等非本次 MCP 迁移路径，不能将 workspace 全绿作为当前完成证据。
- [ ] 尚未完成 Codex 真实长流程、MCP 全部会话错误矩阵、跨公司负向测试和完整 SSE 长连接测试。

### 9.3 接手时的安全顺序

1. 先执行 `git status --short`，保留已有用户修改；不要修改 `/Users/adazhao/workspace/paperclip`。
2. 使用本地 `DATABASE_URL` 启动服务，确认 migrations 成功后再做 API 测试。
3. 先验证 token/session，再验证 `initialize -> notifications/initialized -> tools/list -> tools/call`。
4. 每次只补一个工具的 schema、映射、权限、成功/失败测试，并同步更新第 4 节 checkbox。
5. 任何涉及公司、Agent、run、Issue 的读取都必须同时验证 session scope，不能只验证 URL 参数。
6. 测试产生的临时 Issue、Agent 或 token 应清理或标注，避免污染用户数据库。

## 10. 迁移执行顺序

- [x] 阶段一：冻结 Paperclip 当前参考版本和 41 个工具清单。
- [x] 阶段一：建立工具对照表并完成主要 schema/route mapping。
- [x] 阶段二：完成 gateway session/token 校验和 Agent actor 上下文。
- [x] 阶段二：补齐 gateway token 到 Agent actor 的授权上下文。
- [x] 阶段三：实现 Paperclip 工具 registry 和 JSON Schema 基础集合。
- [x] 阶段四：迁移只读工具：身份、Agent、Issue、Comment、Document、Project、Goal、Approval 查询；逐工具生产验收仍在进行。
- [x] 阶段五：迁移主要写工具：Issue、Comment、Document、Approval、Interaction。
- [x] 阶段六：迁移 workspace runtime 工具和 `paperclipApiRequest` 基础安全限制。
- [ ] 阶段七：补齐 MCP session、SSE、Streamable HTTP 的全部标准细节和自动化测试。
- [ ] 阶段八：补齐工具审计、审批和策略测试。
- [ ] 阶段九：接入 Claude/Codex 真实 CLI 做端到端验证。
- [ ] 阶段十：更新 README、运行手册和故障排查文档。
- [ ] 阶段十一：执行全量 `cargo check`、`cargo test`、`git diff --check`，完成剩余测试后再提交迁移。

## 11. 接手 Agent 的第一步

- [ ] 阅读本文全部内容。
- [ ] 确认当前工作区是否存在其他未提交修改：`git status --short`。
- [ ] 阅读 `crates/api/src/routes/tools.rs` 全文件，而不是只修改 `mcp_session_protocol`。
- [ ] 阅读 Paperclip 的 `packages/mcp-server/src/tools.ts` 全文件，记录工具总数和每个 schema。
- [ ] 建立工具映射表，先标记 `implemented`、`partial`、`missing`。
- [ ] 先实现一个完整 vertical slice：`paperclipGetIssue`、`paperclipAddComment`、`paperclipCreateIssue`。
- [ ] 用真实 Claude CLI 验证 vertical slice 后，再批量迁移剩余工具。
- [ ] 每完成一个工具，同时添加单元测试和 MCP JSON-RPC 集成测试。
- [ ] 不要修改 `/Users/adazhao/workspace/paperclip`。

## 12. 完成判定

只有以下条件全部满足，才可以将本文标题对应的迁移标记为完成：

- [ ] Paperclip 工具清单中的每个工具都能在 `tools/list` 中找到。
- [ ] 每个工具都有真实 input schema，而不是空 object。
- [ ] 每个工具都能执行成功路径和错误路径。
- [ ] 所有写工具都正确携带并校验 run context。
- [ ] 所有工具调用都经过 company/agent/run 权限检查。
- [ ] 工具策略和审批行为与 Paperclip contract 一致。
- [ ] MCP session、JSON-RPC、Streamable HTTP 行为通过自动化测试。
- [ ] Claude 和 Codex 本地 CLI 都能完成至少一次完整任务。
- [ ] 没有明文 token、API key 或敏感 prompt 泄露到日志。
- [ ] 迁移后的数据库可以从旧 `parrot_agent_dev` 安全升级。
- [ ] `cargo check`、相关测试和 `git diff --check` 全部通过。
- [ ] 迁移结果已提交，并在提交信息中列出工具覆盖范围和已知限制。
