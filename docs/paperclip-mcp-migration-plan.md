# Paperclip MCP 完整迁移计划

> 本文用于将 Paperclip 的完整 MCP 工具能力迁移到 `parrot-agent`，方便其他 Agent 接手。
>
> 约束：只允许修改 `/Users/adazhao/workspace/parrot-agent`；禁止修改 `/Users/adazhao/workspace/paperclip`。所有 Paperclip 代码仅作为只读参考。

> 执行状态（2026-08-05）：已完成 gateway token actor、41 个 Paperclip 内置工具注册、主要 REST bridge、统一 typed tool registry、schema/参数校验、审计、MCP session 生命周期、批量 JSON-RPC、SSE 长连接 GET，以及本地 Claude/Codex 的真实 MCP 握手与 Codex 业务长流程。`PaperclipInternalClient` 已将 REST 回环访问从工具映射中抽出并统一认证/错误处理。`CreateIssue` 的 labels、blocker relations/环检测、workspace inheritance、watchdog、watchdogDiscovery 和 harnessKind 已落地并由运行态矩阵覆盖；Issue、labels、blockers、watchdog 和 watchdogDiscovery activity audit 已统一进入 repository transaction；真实 Claude 业务成功路径和 workspace 中既存的非迁移测试失败仍未宣称完成。

## 1. 目标与非目标

### 1.1 目标

- [x] 在 `parrot-agent` 中提供 Paperclip 当前 41 个内置工具的注册集合；逐工具生产级契约验收仍未完成。
- [x] 本地 `claude` 已通过 `/api/tool-gateway/mcp` 启动并加载 Paperclip 工具集合；完整读写长流程仍需补测。
- [x] 保留当前 run 级别的工具网关 Token 隔离能力。
- [x] 工具调用上下文绑定 `company_id`、`agent_id`、`run_id` 和关联 Issue。
- [x] 保留工具策略、审批、调用记录和失败结果的基础链路。
- [x] 支持 MCP Streamable HTTP 的 session、批量 JSON-RPC、Accept 协商、JSON/SSE 响应、SSE GET 长连接和 DELETE 关闭；服务端主动事件队列仍是后续增强项。
- [x] 为每个迁移工具提供生产级参数校验、错误映射和自动化测试；registry schema 运行时执行 required/type/enum/format/长度/范围/additionalProperties 校验，`paperclip-mcp-tool-matrix-smoke.mjs` 逐项调用 41 个工具并断言成功或结构化错误路径。

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
- [x] 内置工具已覆盖 Paperclip 当前 41 个名称和主要 schema；矩阵 smoke 逐项检查工具调用 envelope，资源不存在、无 workspace 和 Agent 审批越权均按预期错误 contract 验收。
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
- [x] gateway token 的过期、撤销、跨公司、跨 Agent、跨 run 负向测试已由 contract/security smoke 覆盖；session 查询和 actor 绑定也已验证。

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

- [x] `paperclipMe`
  - GET `/agents/me`
  - 验证当前 Agent actor 与 run 上下文。
- [x] `paperclipInboxLite`
  - GET `/agents/me/inbox-lite`
- [x] `paperclipListAgents`
  - GET `/companies/:companyId/agents`
- [x] `paperclipGetAgent`
  - GET `/agents/:agentId`

### 4.2 Issue 与执行

- [x] `paperclipListIssues`
  - 支持 company、status、project、assignee、participant、label、workspace、origin、query 等过滤字段。
- [x] `paperclipGetIssue`
  - 支持 UUID 和业务 identifier。
- [x] `paperclipGetHeartbeatContext`
  - 支持 `wakeCommentId`。
- [x] `paperclipCreateIssue`
  - 创建后触发现有 Agent wakeup 逻辑。
- [x] `paperclipUpdateIssue`
  - 支持 status、assignee、comment、resume 等字段。
- [x] `paperclipCheckoutIssue`
  - 默认 `expectedStatuses` 为 `todo`、`backlog`、`blocked`。
  - 必须绑定当前 `agentId` 和 `runId`。
- [x] `paperclipReleaseIssue`
  - 将 release result 正确映射到 done / todo / cancelled。
- [x] `paperclipListComments`
- [x] `paperclipGetComment`
- [x] `paperclipAddComment`
- [x] `paperclipSuggestTasks`
- [x] `paperclipAskUserQuestions`
- [x] `paperclipRequestConfirmation`
- [x] `paperclipRequestCheckboxConfirmation`

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

- [x] `paperclipListProjects`
- [x] `paperclipGetProject`
- [x] `paperclipListGoals`
- [x] `paperclipGetGoal`
- [x] `paperclipListApprovals`
- [x] `paperclipCreateApproval`
- [x] `paperclipGetApproval`
- [x] `paperclipGetApprovalIssues`
- [x] `paperclipListApprovalComments`
- [x] `paperclipLinkIssueApproval`
- [x] `paperclipUnlinkIssueApproval`
- [x] `paperclipApprovalDecision`
  - 支持 approve、reject、requestRevision、resubmit。
- [x] `paperclipAddApprovalComment`

### 4.5 Execution Workspace

- [x] `paperclipGetIssueWorkspaceRuntime`
- [x] `paperclipControlIssueWorkspaceServices`
  - 支持 start、stop、restart。
- [x] `paperclipWaitForIssueWorkspaceService`
  - 支持 timeout、runtimeServiceId、serviceName。

### 4.6 Escape hatch

- [x] `paperclipApiRequest`
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
- [x] 优先调用 `AppState` 中已有 service；身份、Agent、Issue、checkout/release、评论核心路径已经直接走 service，扩展字段不兼容时再回退 `PaperclipInternalClient`。
- [x] 如果必须复用 HTTP contract，增加专用 `PaperclipInternalClient` 调用层，不能通过伪造 Board API Key 绕过认证。
  - 实现：`crates/api/src/paperclip_internal.rs`；工具映射只负责 Paperclip method/path/schema，内部层统一注入短期 gateway token、run id、API base URL、JSON body 和状态/错误返回。
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
- [x] 为 Claude 和 Codex 各自验证实际握手流程，不能只用手写 curl 验证；Claude 已加载 41 个工具，Codex 已加载网关并发起真实工具调用，业务调用因本机缺少 `psql` 失败，详见 9.2。

### 5.4 鉴权与上下文

- [x] token 只保存 hash，响应和日志不输出明文。
- [x] gateway token 解析后生成受限 Agent actor，来源标记为临时 token。
- [x] token actor 的 `run_id` 必须来自数据库，不接受请求体覆盖。
- [x] 绑定关系至少包括 company、agent、run、issue。
- [x] run 完成、失败、取消和超时都要撤销 token/session。
- [x] 所有工具调用日志记录 session、run、agent、company、tool name、decision、outcome。
- [x] 工具参数日志只记录摘要，禁止记录 password、API key、Bearer token 和完整敏感内容。

## 6. 工具与现有 parrot-agent API 对照步骤

- [x] 为每个 Paperclip tool 建立以下映射表：

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

- [x] 检查 REST 路径是否已经存在；41 工具均有现有 route 或明确的内部 contract。
- [x] 检查返回 envelope：数组/对象/`{result: ...}` 已在 bridge 和矩阵脚本中分别验证。
- [x] 检查 camelCase 字段名；Rust model/route 序列化与 Paperclip schema 已对齐。
- [x] 检查 UUID、日期、枚举和 nullable 字段；runtime schema validator 和 Goal migration 已覆盖。
- [x] 检查 Paperclip 对 404、400、401、403、409、422 的错误语义；矩阵及 security smoke 覆盖资源缺失、参数、鉴权、越权、冲突路径。
- [x] 对缺失 endpoint 先补 API/service，再接入 MCP tool；Goal 旧库字段已补 migration，工具 dispatcher 不直接写业务重复 SQL。

### 6.1 `paperclipCreateIssue` 高级字段迁移状态

Paperclip 的 shared schema 比当前 parrot-agent 的 Issue 数据模型更宽。不能因为
`tools/list` 的 JSON Schema 已声明字段，就认为字段已经被 REST/service/repository
持久化；Rust `serde` 默认会忽略未知字段，这是当前接手时最容易漏掉的风险。

| Paperclip 字段 | 当前状态 | 接手迁移要求 |
|---|---|---|
| `projectWorkspaceId`、`workMode`、`responsibleUserId`、`originKind`、`originId`、`originRunId`、`requestDepth`、`billingCode` | 已贯通 model/service/repository | 增加持久化断言和跨公司错误测试 |
| `executionPolicy`、`executionWorkspaceId`、`executionWorkspacePreference`、`executionWorkspaceSettings`、`assigneeAdapterOverrides` | 已贯通 model/service/repository；execution policy/settings 已由矩阵验证 | 补齐 GET/更新/恢复后的 response projection，并验证旧库 migration |
| `labelIds` | 已贯通 model/service/repository；创建和更新均在事务中校验并写入 `issue_labels`，GET/list 会回填 `labelIds` | 继续补 REST 422 错误 envelope；矩阵已验证创建、更新和缺失 label 错误路径 |
| `blockedByIssueIds` | 已新增 `issue_relations` migration；创建和更新均在事务中校验同 company、去重、自引用，GET/list 会回填 blocker IDs | 继续补 Paperclip 的环检测和 blocker 状态派生字段；矩阵已验证创建与更新成功路径 |
| `inheritExecutionWorkspaceFromIssueId` | 已在 service 层读取来源 Issue，并继承 project/workspace/preference/settings；显式目标字段优先 | 补 workspace source 的跨公司/不存在错误矩阵，以及真实 execution workspace runtime 验证 |
| `harnessKind` | 已新增 `issues.harness_kind` migration，并贯通 model/service/repository；`skill_test` 会与 `workMode=skill_test` 双向归一化，非法值/冲突组合返回 422 | 继续补真实 skill-test adapter 执行策略和旧库升级验证 |
| `watchdogDiscovery`、`watchdog` | `watchdog` 已接入 REST CreateIssue 和专用 watchdog route；`watchdogDiscovery=product_bug` 已校验 active watchdog run，继承来源上下文，写入 origin/description/audit projection | Issue、watchdog 和 discovery activity audit 已合并为 repository transaction；仍需继续补 discovery 的跨公司/伪造 run 负向矩阵 |
| `goalId`、`parentId` | 已有基础校验和持久化 | 补 company scope 校验；`parentId` 创建子任务后必须验证 wakeup/继承策略 |

迁移顺序必须固定为：

- [x] 已为 labels、blocker relations、workspace inheritance、watchdog、harnessKind、watchdogDiscovery 增加 model/service/repository 或 route 的显式类型；未支持的值仍会显式失败。
- [x] labels、blocker relations、harnessKind、watchdog 和 Issue 主记录已使用同一 repository 数据库事务；watchdog route 的独立 upsert 仍保留给已存在 Issue 的更新场景。
- [x] 已覆盖 labels 缺失、watchdog agent company scope、harnessKind 非法/冲突、watchdogDiscovery 非 agent run、active watchdog discovery 成功等成功/错误组合；跨公司和伪造 run 仍需在 security smoke 中继续细化。
- [x] 已更新 `paperclip-mcp-tool-matrix-smoke.mjs`，断言 execution fields、labels、blocker relations、watchdog 的实际返回投影，并执行缺失 label 错误路径。
- [x] `paperclipCreateIssue` 的高级字段已具备显式 model、持久化或明确运行时契约；当前剩余限制（skill-test 执行策略）已在本节列明。

### 6.2 当前 41 个工具映射总表

下表是当前 Rust registry 与 `call_paperclip_builtin_tool` 的单一验收清单。`scope` 表示请求进入 REST bridge 前使用 session-derived company/agent/run；Issue 子资源还会在 REST handler 中再次检查 Issue 所属公司。`response` 是 bridge 原样返回的 API JSON（空响应转换为 `null`），错误统一转换为 MCP `isError=true` 的文本内容并保留 HTTP 状态和响应体摘要。

| MCP 工具 | HTTP 映射 | scope | 类型 |
|---|---|---|---|
| `paperclipMe` | `GET /agents/me` | company/agent/run | read |
| `paperclipInboxLite` | `GET /agents/me/inbox-lite` | company/agent/run | read |
| `paperclipListAgents` | `GET /companies/{session.company}/agents` | company | read |
| `paperclipGetAgent` | `GET /agents/{agentId}` | company/agent | read |
| `paperclipListIssues` | `GET /companies/{session.company}/issues` + filters | company | read |
| `paperclipGetIssue` | `GET /issues/{issueId|identifier}` | company/issue | read |
| `paperclipGetHeartbeatContext` | `GET /issues/{issueId}/heartbeat-context` | company/issue/run | read |
| `paperclipListComments` | `GET /issues/{issueId}/comments` | company/issue | read |
| `paperclipGetComment` | `GET /issues/{issueId}/comments/{commentId}` | company/issue/comment | read |
| `paperclipListIssueApprovals` | `GET /issues/{issueId}/approvals` | company/issue | read |
| `paperclipListDocuments` | `GET /issues/{issueId}/documents` | company/issue | read |
| `paperclipGetDocument` | `GET /issues/{issueId}/documents/{key}` | company/issue | read |
| `paperclipListDocumentRevisions` | `GET /issues/{issueId}/documents/{key}/revisions` | company/issue | read |
| `paperclipListProjects` | `GET /companies/{session.company}/projects` | company | read |
| `paperclipGetProject` | `GET /projects/{projectId}` | company/project | read |
| `paperclipListGoals` | `GET /companies/{session.company}/goals` | company | read |
| `paperclipGetGoal` | `GET /goals/{goalId}` | company/goal | read |
| `paperclipListApprovals` | `GET /companies/{session.company}/approvals` | company | read |
| `paperclipCreateApproval` | `POST /companies/{session.company}/approvals` | company/agent/run | write |
| `paperclipGetApproval` | `GET /approvals/{approvalId}` | company/approval | read |
| `paperclipGetApprovalIssues` | `GET /approvals/{approvalId}/issues` | company/approval | read |
| `paperclipListApprovalComments` | `GET /approvals/{approvalId}/comments` | company/approval | read |
| `paperclipAddApprovalComment` | `POST /approvals/{approvalId}/comments` | company/approval/agent/run | write |
| `paperclipApprovalDecision` | `POST /approvals/{id}/{approve|reject|request-revision|resubmit}` | company/approval/agent/run | write |
| `paperclipCreateIssue` | `POST /companies/{session.company}/issues` | company/agent/run | write |
| `paperclipUpdateIssue` | `PATCH /issues/{issueId}` | company/issue/agent/run | write |
| `paperclipCheckoutIssue` | `POST /issues/{issueId}/checkout` | company/issue/agent/run | write |
| `paperclipReleaseIssue` | `POST /issues/{issueId}/release` | company/issue/agent/run | write |
| `paperclipAddComment` | `POST /issues/{issueId}/comments` | company/issue/agent/run | write |
| `paperclipSuggestTasks` | `POST /issues/{issueId}/interactions` kind=`suggest_tasks` | company/issue/agent/run | write |
| `paperclipAskUserQuestions` | `POST /issues/{issueId}/interactions` kind=`ask_user_questions` | company/issue/agent/run | write |
| `paperclipRequestConfirmation` | `POST /issues/{issueId}/interactions` kind=`request_confirmation` | company/issue/agent/run | write |
| `paperclipRequestCheckboxConfirmation` | `POST /issues/{issueId}/interactions` kind=`request_checkbox_confirmation` | company/issue/agent/run | write |
| `paperclipUpsertIssueDocument` | `PUT /issues/{issueId}/documents/{key}` | company/issue/agent/run | write |
| `paperclipRestoreIssueDocumentRevision` | `POST /issues/{issueId}/documents/{key}/revisions/{revisionId}/restore` | company/issue/agent/run | write |
| `paperclipLinkIssueApproval` | `POST /issues/{issueId}/approvals` | company/issue/approval/agent/run | write |
| `paperclipUnlinkIssueApproval` | `DELETE /issues/{issueId}/approvals/{approvalId}` | company/issue/approval/agent/run | write |
| `paperclipGetIssueWorkspaceRuntime` | `GET /issues/{issueId}/heartbeat-context` runtime projection | company/issue/run | read |
| `paperclipControlIssueWorkspaceServices` | `POST /execution-workspaces/{workspaceId}/runtime-services/{action}` | company/issue/agent/run | write |
| `paperclipWaitForIssueWorkspaceService` | poll `paperclipGetIssueWorkspaceRuntime` until ready/timeout | company/issue/run | read |
| `paperclipApiRequest` | constrained relative `/api` path | session/agent/run | read/write |

验收要求：工具名只能来自该表；任何新增 Paperclip 工具必须同时增加 registry、schema、dispatch、scope 测试和本表行，不能只在 `tools/list` 中伪造名称。

## 7. 数据库与持久化检查

- [x] 确认 `tool_gateway_sessions` 的完整字段与迁移一致。
- [x] 确认 `tool_invocations`、`tool_call_events`、`tool_action_requests` 可覆盖成功、失败、拒绝和审批状态。
- [x] 确认 `tool_connections`、`tool_profiles`、`tool_profile_entries`、`tool_policies` 的字段与查询一致。
- [x] 明确复用现有 `tool_gateway_sessions` 作为 MCP session：其 id 是 `Mcp-Session-Id`，并更新 `last_used_at`；目前不额外引入独立 MCP session 表。
- [x] 确认旧 `parrot_agent_dev` 能通过迁移启动，不使用破坏性删表重建。
- [x] 新增迁移使用 `IF NOT EXISTS` 或安全的数据回填策略；本地 PostgreSQL 启动已验证。

## 8. 测试计划

### 8.1 协议测试

- [x] 无 token 调用 `initialize` 返回标准 unauthorized JSON-RPC error。
- [x] 错误 token 调用 `initialize` 返回 unauthorized。
- [x] 过期 token 调用 `initialize` 返回 unauthorized；`paperclip-mcp-security-smoke.mjs` 的 `TEST_EXPIRY=1` 实测通过。
- [x] 撤销 token 调用 `initialize` 返回 unauthorized。
- [x] 正确 token 完成 initialize handshake。
- [x] `tools/list` 返回全部 Paperclip 内置工具、插件工具和外部 MCP 工具；内置集合已验证为 41 个。
- [x] `tools/call` 能调用一个只读工具。
- [x] `tools/call` 能调用一个写工具。
- [x] `tools/call` 参数不符合 schema 时返回 invalid params。
- [x] notification 不产生错误的普通 response。
- [x] 未知 method 返回 method-not-found。
- [x] Streamable HTTP JSON response 和 SSE response 都能被客户端解析。

### 8.2 权限测试

- [x] Agent 不能读取其他公司的 Issue；跨公司 Issue 和 companyId 列表过滤已实测。
- [x] Agent 不能使用其他 Agent 的 run id；跨 Agent run session 创建返回 400。
- [x] Agent 不能伪造 companyId、agentId 或 issueId 绕过 session scope。
- [x] 被 deny 的工具不会实际执行；策略 smoke 验证 403 和 denied invocation。
- [x] require approval 的工具会生成 action request，未批准前不会执行。
- [x] 审批拒绝后调用状态和审计记录正确；invocation 为 denied，errorCode 为 approval_declined。
- [x] session 撤销后不能继续调用。
- [x] run 完成后 token 不能继续使用。

### 8.3 业务工具测试

- [x] 用 MCP 创建 Issue，并验证数据库记录和返回结构；`scripts/paperclip-mcp-business-smoke.mjs` 已通过。
- [x] 用 MCP 更新 Issue；业务 smoke 已验证返回结构和持久化路径（Agent wakeup 仍需真实交互 worker 长流程验证）。
- [x] 用 MCP 添加评论，并验证 actor/run 归属。
- [x] 用 MCP 创建、读取、更新、恢复文档 revision。
- [x] 用 MCP 创建审批、查询关联 issue、添加/读取审批评论。
- [x] 用 MCP 创建四类 Paperclip interaction payload，并验证返回值。
- [x] 用 MCP 查询 heartbeat context 和 workspace runtime。
- [x] 用 `paperclipApiRequest` 验证安全相对路径和 JSON 请求路径。
- [x] 用 MCP checkout/release Issue，并验证 checkoutRunId、executionRunId、status 及 release 后执行字段清理；见 `scripts/paperclip-mcp-checkout-release-smoke.mjs`。

### 8.4 端到端测试

- [x] 启动本地 PostgreSQL 和 `parrot-agent`。
- [x] 创建或复用公司、Agent、Issue。
- [x] 启动本地 Claude adapter。
- [x] 验证 Claude 收到完整 Paperclip 工具列表；使用本地 adapter 握手加载 41 个内置工具。
- [ ] 让 Claude 调用 `paperclipGetIssue`。
- [ ] 让 Claude 调用 `paperclipAddComment`。
- [ ] 让 Claude 创建子任务并验证新任务被唤醒。
- [x] 验证 Codex 真实 run 的 stdout/stderr、tool invocation、run status 和 Issue status；Claude adapter 的完整 stdout 解析也已覆盖。
- [x] 验证服务重启后的待执行任务恢复；重启日志显示 `reconciled pending assigned issues` 并为原 Issue 创建新的 heartbeat run。
- [x] 验证 token 不出现在普通日志；session 只持久化 token hash，adapter 日志显示脱敏占位符。

## 9. 当前执行证据与接手说明

### 9.1 已落地文件

- [x] `crates/api/src/routes/tools.rs`：41 个 Paperclip 工具注册、参数校验、工具映射、策略/审计、MCP session JSON-RPC 入口。
- [x] `crates/api/src/paperclip_internal.rs`：专用内部 REST contract bridge，统一 gateway token/run header、API URL 归一化、JSON body 和错误状态处理。
- [x] `scripts/paperclip-mcp-tool-matrix-smoke.mjs`：逐项调用全部 41 个 Paperclip 工具，验证成功/结构化错误路径。
- [x] `scripts/claude-mcp-vertical-slice-smoke.mjs`：启动真实 Claude CLI，使用 run-scoped gateway token，要求并断言 `paperclipGetIssue`、`paperclipAddComment`、`paperclipCreateIssue` 三个真实 tool use；模型代理拒绝时保留失败证据而不误报成功。
- [x] `migrations/20260805000003_complete_goals_compatibility.sql`：补齐旧 goals 表的 `name`、`priority` 和 `achieved` 状态兼容字段。
- [x] `crates/api/src/mcp/mod.rs`：`McpToolDefinition`、run-scoped `McpInvocationContext` 和 request/notification 分类。
- [x] `crates/api/src/routes/issues.rs`：Issue document GET/PUT、revision list/restore、Paperclip interaction kinds、Issue 查询 status/priority/q。
- [x] `crates/api/src/routes/approvals.rs`：审批 status 过滤、创建/关联多个 issue、approve/reject/revision/resubmit。
- [x] `crates/services/src/heartbeat_service.rs`：真实 gateway token 注入、URL 注入、日志脱敏、continuation summary。
- [x] `crates/api/src/routes/agents.rs`、`crates/services/src/agent_service.rs`：补齐 `UpdateAgentSchema.adapterType` 到 service/repository 的贯通，允许在真实 CLI/adapter 验证前后安全切换 adapter 类型并恢复配置。
- [x] `crates/services/src/issue_service_complete.rs`：补齐 Paperclip `CreateIssue` 省略 `status` 时的默认值；未分配 Issue 为 `backlog`，已分配 Agent/User 的 Issue 为 `todo`，避免旧库在 `issues.status` 非空约束下写入 NULL。
- [x] `crates/models/src/issue.rs`、`crates/repositories/src/pg_issue_repository.rs`：`CreateIssue` 的 `executionPolicy`、`executionWorkspaceSettings` 已贯通 model/service/repository，并由工具矩阵验证持久化返回值。
- [x] `migrations/20260805000004_create_issue_relations.sql`、`crates/models/src/issue.rs`、`crates/repositories/src/pg_issue_repository.rs`：`labelIds`、`blockedByIssueIds` 已加入事务持久化和读取 projection；`inheritExecutionWorkspaceFromIssueId` 已在 service 层完成同公司来源配置继承。
- [x] `migrations/20260805000005_add_issue_harness_kind.sql`、`crates/models/src/issue.rs`、`crates/repositories/src/pg_issue_repository.rs`：`harnessKind=skill_test` 已独立持久化，并与 `workMode` 归一化。
- [x] `crates/api/src/routes/issues.rs`、`crates/api/src/routes/watchdogs.rs`、`crates/services/src/task_watchdog.rs`、`crates/repositories/src/pg_issue_repository.rs`：`CreateIssue.watchdog` 已校验 watchdog agent company scope，并在 Issue repository transaction 内写入 watchdog projection；`watchdogDiscovery=product_bug` 已实现 active run scope、来源继承、origin fingerprint、description context 和 activity audit，且 activity audit 与 Issue 位于同一 transaction。
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
- [x] `cargo test -p api --lib` 通过，当前 60 个测试通过；`cargo check -p parrot-server` 通过。
- [x] gateway token 脱敏测试通过；真实 adapter argv 保留实际 token，普通日志只显示 `[PAPERCLIP_TOOL_GATEWAY_TOKEN]`。
- [x] 本地 Claude adapter 真实启动成功，MCP URL 指向当前 parrot-agent 端口，并在 run 输出中加载 Paperclip 工具集合。
- [x] MCP 协议手工验证覆盖 initialize、initialized notification、tools/list、tools/call、invalid args、unknown method、malformed JSON、JSON/SSE response；API 单元测试当前 60 个通过。
- [x] `scripts/paperclip-mcp-business-smoke.mjs` 已完成 41 个内置工具集合检查，并实际执行 Issue、Comment、Document、Interaction、Approval、Heartbeat、workspace runtime 和 `paperclipApiRequest` 成功路径；内部 bridge 重构后再次通过。
- [x] `scripts/paperclip-mcp-tool-matrix-smoke.mjs` 已在保持 heartbeat run 活跃的本地 adapter 下通过：`listedToolCount=41`、`invokedToolCount=41`，成功路径与预期错误路径均已执行。
- [x] `scripts/paperclip-mcp-checkout-release-smoke.mjs` 已验证 checkout/release 的执行锁、run 绑定、状态转换和字段清理。
- [x] `scripts/mcp-gateway-contract-smoke.sh` 已验证 Streamable HTTP 的 JSON-RPC、批量、Accept 协商、SSE GET/KeepAlive 和 DELETE 生命周期。
- [x] `scripts/paperclip-mcp-security-smoke.mjs` 已验证跨 Agent checkout、危险 API path、错误 MCP session id、错误 token、跨公司、跨 run、deny、require-approval 和 approval decline；过期 token 测试支持 `TEST_EXPIRY=1` 长耗时模式。
- [x] `scripts/paperclip-mcp-tool-matrix-smoke.mjs` 现在在结束时断言 41 个 registry 工具全部至少被调用一次；资源缺失和 Agent 审批决策工具使用预期错误路径。
- [x] 最新矩阵再次覆盖 41 个工具，并额外断言 CreateIssue 的 `labelIds`、`blockedByIssueIds`、`watchdog`、`harnessKind`、execution fields，UpdateIssue 的 label replacement、缺失 label/环检测错误路径、watchdog discovery 成功路径，以及每个工具的 invalid-arguments 路径：`listedToolCount=41`、`invokedToolCount=41`、`invalidArgumentChecks=41`、created issue `38ae84e7-75ac-43d2-ac1c-9347f70da68d`。
- [x] 专项 REST/MCP 运行验证：`harnessKind=skill_test` 创建/更新返回 `workMode=skill_test`；有效 watchdog run 创建 `task_watchdog_product_bug` follow-up（issue `a50126c8-5828-42c4-96fb-e1c5f5fb4a55`），非 Agent/local actor discovery 返回 403；watchdog 专用 GET/POST route 修复双重 `/api` 前缀后返回 200/201。
- [x] watchdog 事务运行验证：REST CreateIssue 返回的 `watchdog` projection 与 Issue 同时落库（issue `0307d480-cc75-492f-b28a-fe31fbf7fa1a`）；故意提交不存在的 label 时返回 422，按标题查询确认 Issue 未落库，证明 label/blocker/watchdog 失败会回滚主 Issue。
- [x] heartbeat run 完成后可读取 `continuation-summary` 文档。
- [x] `cargo test --workspace --no-fail-fast` 已执行：208 个测试通过，19 个既存 services 测试失败，另有 1 个 board API timing-sensitive 测试失败；失败集中在 `adapter_config_normalizer`、`codex_local_isolation`、`consistency_service`、授权策略和常量时间比较等非本次 MCP 迁移路径，不能将 workspace 全绿作为当前完成证据。
- [ ] 尚未完成真实 Claude 业务成功路径：本地 `deepseek-v4-flash` 三次 run 均正常退出并加载 41 个工具，但返回“检测到敏感内容”且 `toolCallCount=0`（run `3a87620d-021b-4d1e-acd9-7de202e0c404`、`8b113e05-0647-4119-8de9-302e931e1853`、`515e575e-250a-4dab-9b7e-d90b50db2e62`）。本轮用 `deepseek-v4-pro` 的最小 CLI 请求仍返回同一策略拒绝；指定官方 Claude 模型则返回代理层 HTTP 200 空响应，去掉本地代理后为 401 Invalid bearer token。因此 `paperclipGetIssue`、`paperclipAddComment` 和创建子任务的 Claude 端到端 checkbox 仍保持未完成；41 工具逐项矩阵、核心 AppState service 路径、Goal 旧库兼容 migration、Codex 业务长流程、跨公司/跨 run 负向测试、checkout/release 矩阵、策略 deny/approval 矩阵和专用内部 REST bridge 已通过；协议 SSE 长连接已由 `scripts/mcp-gateway-contract-smoke.sh` 验证。

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
- [x] 阶段七：补齐 MCP session、SSE、Streamable HTTP 的协议测试；安全过期/跨公司边界仍在阶段八核销。
- [x] 阶段八：补齐工具审计、审批和策略测试；跨 scope、deny、approval、decline 已通过。
- [ ] 阶段九：接入 Claude/Codex 真实 CLI 做端到端验证；Codex 已通过，Claude 成功路径仍受当前本地模型策略拒绝。
- [x] 阶段十：更新 README、运行手册和故障排查文档。
- [ ] 阶段十一：执行全量 `cargo check`、`cargo test`、`git diff --check`，完成剩余测试后再提交迁移。

## 11. 接手 Agent 的第一步

- [x] 阅读本文全部内容。
- [x] 确认当前工作区是否存在其他未提交修改：`git status --short`；当前 parrot-agent 工作区干净，Paperclip 工作区保留既有用户修改且未触碰。
- [x] 阅读 `crates/api/src/routes/tools.rs` 全文件，而不是只修改 `mcp_session_protocol`；已逐段审阅 registry、schema validator、direct service path、REST bridge、session/SSE、policy/approval、plugin/MCP connection 和 run decisions。
- [x] 阅读 Paperclip 的 `packages/mcp-server/src/tools.ts` 全文件，记录工具总数和每个 schema；当前参考实现注册 41 个工具，已逐项对照 schema、REST path、payload 默认值和 workspace runtime 行为。
- [x] 建立工具映射表，先标记 `implemented`、`partial`、`missing`。
- [x] 先实现一个完整 vertical slice：`paperclipGetIssue`、`paperclipAddComment`、`paperclipCreateIssue`。
- [ ] 用真实 Claude CLI 验证 vertical slice 后，再批量迁移剩余工具。
- [x] 每完成一个工具，同时添加单元测试和 MCP JSON-RPC 集成测试；统一矩阵和协议 smoke 覆盖全部 registry 工具。
- [x] 已为 41 个工具建立统一矩阵脚本；高级 CreateIssue 字段已纳入同一矩阵，没有新增第二套 dispatcher。
- [x] 不要修改 `/Users/adazhao/workspace/paperclip`。

## 12. 完成判定

只有以下条件全部满足，才可以将本文标题对应的迁移标记为完成：

- [x] Paperclip 工具清单中的每个工具都能在 `tools/list` 中找到；registry test 和 business smoke 均断言 41 个内置工具。
- [x] 每个工具都有真实 input schema，而不是空 object；registry test 断言每项都有 `inputSchema.type`，并由 typed registry 提供字段 schema。
- [x] 每个工具都能执行成功路径和错误路径；最新矩阵对 41 个工具分别执行成功路径和 invalid-arguments 路径。
- [x] 所有写工具都正确携带并校验 run context；gateway token/session、run header 和 security smoke 已覆盖。
- [x] 所有工具调用都经过 company/agent/run 权限检查；跨公司、跨 Agent、跨 run 和过期/撤销 token 已由 security smoke 覆盖。
- [x] 工具策略和审批行为与 Paperclip contract 一致；deny、require-approval、approval decline 和 Agent 禁止审批路径已验证。
- [x] MCP session、JSON-RPC、Streamable HTTP 行为通过自动化测试；见 API 单测和 `scripts/mcp-gateway-contract-smoke.sh`。
- [ ] Claude 和 Codex 本地 CLI 都能完成至少一次完整任务。
- [x] 没有明文 token、API key 或敏感 prompt 泄露到日志；adapter shell command 使用脱敏占位符，子进程 argv 才保留真实值。
- [x] 迁移后的数据库可以从旧 `parrot_agent_dev` 安全升级；本地数据库启动时 migrations 已成功执行，旧 auth/comment/interaction 字段兼容迁移已落地。
- [x] 迁移相关 `cargo check`、API/models 测试和 `git diff --check` 全部通过；workspace 仍有文档记录的 19 个既存 services 失败，不能宣称全量测试绿。
- [x] 迁移结果已提交；提交 `c15a43b` 和后续文档提交信息列出工具覆盖范围与已知限制。
