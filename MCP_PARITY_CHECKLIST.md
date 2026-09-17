# Paperclip ↔ Parrot MCP 对齐工作清单

更新时间：2026-09-17

对照范围：

- Paperclip standalone MCP server（`packages/mcp-server`）
- Paperclip managed MCP gateway、Tool Access、OAuth、catalog、profile/policy、stdio runtime
- Parrot `crates/api/src/routes/tools.rs`、`crates/api/src/routes/tool_access.rs`、services contract 及数据库迁移

## 1. Standalone MCP tool list

Paperclip standalone server 的 41 个 canonical tools：

- [x] `paperclipMe`
- [x] `paperclipInboxLite`
- [x] `paperclipListAgents`
- [x] `paperclipGetAgent`
- [x] `paperclipListIssues`
- [x] `paperclipGetIssue`
- [x] `paperclipGetHeartbeatContext`
- [x] `paperclipListComments`
- [x] `paperclipGetComment`
- [x] `paperclipListIssueApprovals`
- [x] `paperclipListDocuments`
- [x] `paperclipGetDocument`
- [x] `paperclipListDocumentRevisions`
- [x] `paperclipListProjects`
- [x] `paperclipGetProject`
- [x] `paperclipGetIssueWorkspaceRuntime`
- [x] `paperclipControlIssueWorkspaceServices`
- [x] `paperclipWaitForIssueWorkspaceService`
- [x] `paperclipListGoals`
- [x] `paperclipGetGoal`
- [x] `paperclipListApprovals`
- [x] `paperclipCreateApproval`
- [x] `paperclipGetApproval`
- [x] `paperclipGetApprovalIssues`
- [x] `paperclipListApprovalComments`
- [x] `paperclipCreateIssue`
- [x] `paperclipUpdateIssue`
- [x] `paperclipCheckoutIssue`
- [x] `paperclipReleaseIssue`
- [x] `paperclipAddComment`
- [x] `paperclipSuggestTasks`
- [x] `paperclipAskUserQuestions`
- [x] `paperclipRequestConfirmation`
- [x] `paperclipRequestCheckboxConfirmation`
- [x] `paperclipUpsertIssueDocument`
- [x] `paperclipRestoreIssueDocumentRevision`
- [x] `paperclipLinkIssueApproval`
- [x] `paperclipUnlinkIssueApproval`
- [x] `paperclipApprovalDecision`
- [x] `paperclipAddApprovalComment`
- [x] `paperclipApiRequest`

补充说明：Paperclip 的“雇佣 agent”不是 standalone MCP tool，而是 canonical `/companies/:companyId/agent-hires` API；Parrot 额外提供了 `paperclipHireAgent`，并已按该 API 的 request body 对齐。

## 2. Parrot tool registry、schema 和 dispatcher

- [x] 保持上述 41 个 Paperclip canonical tools 的名称、schema、必填字段和 closed-object 校验。
- [x] 补齐 `paperclipHireAgent`。
- [x] 注册 Parrot 已有的 case、attachment、routine、label、external-object、file-resource 等 55 个扩展 tools。
- [x] registry 数量断言：`41 + 1 + 55 = 97`，名称唯一。
- [x] dispatcher 对 unknown fields、UUID、enum、limit、敏感 wrapper 参数执行运行时校验。
- [x] hire agent 使用 `sourceIssueIds`、`adapterConfig`、`runtimeConfig` 等 canonical 字段。

## 3. MCP transport、catalog 和安全边界

- [x] Streamable HTTP JSON 响应。
- [x] `text/event-stream` SSE 响应解析和 JSON/SSE `Accept` 协商。
- [x] upstream response body 大小上限、JSON 结构校验、JSON-RPC error 透传。
- [x] remote endpoint authenticated-public 部署下的 localhost/private/reserved endpoint 防 SSRF 校验。
- [x] stdio template allowlist、清理 inherited environment、最大行长度和请求超时。
- [x] initialize → initialized 协议握手及 stderr 隔离。
- [x] catalog discovery、健康状态、enabled/status/quarantine 过滤。
- [x] 稳定的 `mcp.<application>-<connection>:<tool>` 名称及 collision suffix。
- [x] `search_tools` / `run_tool` virtual tools。
- [x] MCP elicitation 转为 issue `ask_user_questions`，并保留 awaiting/409 行为。
- [x] credential/header policy、OAuth access token 到 `Authorization: Bearer` 的安全投影。

## 4. Managed MCP gateway、token 和 session

- [x] `/mcp/gateways/:gatewayPublicId` GET/POST/DELETE 协议入口。
- [x] initialize、tools/list、tools/call、notifications/initialized 的 MCP wire behavior。
- [x] durable named gateway 的 profile、scope、metadata、approval、header/on-demand 配置。
- [x] Paperclip token 格式 `pcgw_<gatewayUuid>.<base64url secret>`，只落库 hash。
- [x] token name/clientLabel/ownerNote/allowedActions/expiry/override reason 校验。
- [x] agent、run、client subject 和 MCP session metadata 持久化。
- [x] agentless named gateway 访问 connected MCP catalog。
- [x] protocol/gateway/token/auth-failure rate limits。
- [x] gateway profile 更新时同步 `tool_profile_bindings`，避免旧 profile 残留决策。

## 5. Tool Access 管理面

- [x] gallery 对齐 Zapier、GitHub、Slack、Notion、Linear、Google Sheets、Context7 七个 Paperclip providers。
- [x] 保留 Parrot custom HTTP/stdio extension，并标注 `parrotExtension`。
- [x] gallery connect：真实创建 application/connection、加密 credential refs。
- [x] direct MCP link、API key、OAuth、无认证连接分支。
- [x] OAuth PKCE state、callback、token exchange、redirect constraint。
- [x] OAuth metadata discovery、DCR client registration、PKCE method/response 校验。
- [x] reconnect：credential rotation、secret ref 更新、catalog refresh。
- [x] finish：quarantine review、enabled/ask-first catalog entries、profile entries、agent/company binding。
- [x] action request list/status filter、attention apps、connection activity、test call。
- [x] generic policy CRUD：allow/block/require_approval/rate_limit。
- [x] trust rule promotion/revoke、exact argument hash、reviewed selector、audit/event 记录。
- [x] runtime slot status、stdio template、health 聚合接口使用真实数据库数据。
- [x] deterministic `safe-read-only-todo-kv` example：真实安装 application/connection/profile/catalog/binding。
- [x] example smoke 检查六个 fixture tools、read review/profile 和 write/destructive quarantine。

## 6. Profile / binding 行为

- [x] profile entry 支持 Paperclip 五种 selector：application、connection、catalog_entry、tool_name、risk_level。
- [x] profile entry 支持 include/exclude、conditions 持久化和 legacy `{tool, enabled}` 兼容。
- [x] profile 创建/更新支持最多 250 个 entries 的批量创建/替换。
- [x] profile entry 引用的 application/connection/catalog entry 做 company-scope 校验。
- [x] profile list 返回 entries 和 bindings，而不再只有 profile 元数据。
- [x] binding 支持 company/agent/project/routine/issue/gateway、多 profile、priority、metadata。
- [x] effective profile scope 按 gateway > issue > routine > agent > project > company 选择最窄 scope。
- [x] 同一 scope 内按 priority/createdAt/profile id 排序。
- [x] exclude 只跳过当前 profile，后续 profile 仍可显式 allow，符合 Paperclip decision loop。
- [x] application/risk/catalog/connection/tool-name selector 进入实际 gateway decision。

## 7. Database / regression coverage

- [x] migration 89：agentless gateway session 和 durable rate-limit counter。
- [x] migration 90：多 profile binding、priority/metadata/creator、profile metadata。
- [x] migration 91：gateway/token/session/invocation/call-event 的 Paperclip durable metadata。
- [x] schema version/build check 已更新到 91。
- [x] API route registry、MCP schema、wire normalization、Accept negotiation 测试。
- [x] named token、OAuth secret projection、DCR response validation、profile selector 测试。

## 8. 尚未完全等价的项目

- [ ] Paperclip 的独立 npm `packages/mcp-server` 发布包尚未在 Parrot 中提供；Parrot 当前通过 Rust hosted gateway 提供相同 canonical tools。这是打包形态差异，不影响 hosted MCP 调用。
- [ ] Paperclip stdio supervisor 的跨调用进程复用、idle eviction、capacity 限制和 restart-storm 防护尚未完整移植；Parrot 当前执行 approved template，并持久化 runtime slot 状态。
- [ ] OAuth refresh token 的自动刷新、refresh lease 和并发 single-flight 尚未接入每次 MCP credential resolution；当前已支持 metadata/DCR、PKCE、code exchange、加密存储 access/refresh token。
- [ ] Google Sheets 的 provider-specific service-account runtime 仍依赖已启用的本地 template；没有 active template 时 gallery 会明确返回 unavailable，而不是假装可用。
- [ ] synthetic fixture smoke 当前验证 catalog/profile/quarantine 结构，不启动持久状态的独立 fixture server；实际业务工具仍可通过 built-in deterministic handler 执行。

上述未完成项都是当前代码中的明确边界，不应在 parity 报告中标记为已完成。

## 9. 本轮验证

- [x] `cargo check -p api`
- [x] `cargo test -p api routes::tools::tests --lib`
- [x] `cargo test -p api routes::tool_access::tests --lib`
- [x] `cargo check --workspace`
- [x] `git diff --check`
