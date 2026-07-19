# Parrot Agent API Gap vs Paperclip（Controller 层接口补齐任务文档）

> 对比基准：Paperclip `server/src/routes`（Express，约 428 个唯一路径 / 542 条路由）
> 当前实现：Parrot Agent `crates/api/src/routes`（Axum，约 190 个唯一路径）
> 结论：Paperclip 提供约 **428** 个接口，我们已实现约 **190** 个，**缺失约 345 个**（去重后）。
> 注：缺失项已剔除明显属于“实例运维 / 插件内部桥接 / dev-server”等可能超出本期范围的接口，但全部列出供决策。

---

## 一、为什么 controller 层没对齐

1. **根 `main.rs` 只是占位 `println!("Hello, world!")`**，没有启动 Axum server 的入口；所有 crate 都是 library，没有 `[[bin]]`。所以“controller 层”从未真正挂载到 HTTP 层（见上一轮启动说明）。
2. **接口是按 Domain 渐进补充的**：`crates/api/src/routes/mod.rs` 已声明 41 个 route 模块，但每个模块里只实现了核心 CRUD，**子资源 / 批量操作 / 状态机动作（transition/approve/reject/retry/restore/lock…）/ 分析统计类接口大量未实现**。
3. **整块功能域缺失**：Costs/Budgets、Approvals、Plugins、Skills（catalog/test-runs/versions）、Cloud Upstreams、Heartbeat/Live Runs、External Objects、Interactions、Labels、Activity、Dashboard、LLMs、OpenAPI、Instance Settings、Feedback Traces、File Resources、Resource Memberships(agents) 等在我方 `routes/` 里完全没有对应模块或仅有空壳（`skills.rs` / `org.rs` / `user_directory.rs` 等 `pub mod` 但 0 条路由）。

---

## 二、缺失接口总览（按域分组）

### 1. 认证 / 会话 / 实例管理（Auth & Admin）
- [x] `POST /bootstrap/claim` — 首次启动认领实例
- [x] `GET /get-session` — 取当前会话（我方为 `/api/auth/get-session`）
- [x] `GET /profile` + `PATCH /profile` — 当前用户资料
- [x] `POST /admin/users/:userId/promote-instance-admin` / `demote-instance-admin`
- [x] `GET /admin/users` / `GET /admin/users/:userId/company-access` / `PUT .../company-access`
- [x] `POST /join-requests/:requestId/claim-api-key`

### 2. 公司 / 成员 / 组织（Company & Members）
- [x] `GET /companies/:companyId/activity` + `POST .../activity`
- [x] `PATCH /companies/:companyId/members/:memberId/permissions`
- [x] `GET /companies/:companyId/agent-configurations`
- [x] `GET /companies/:companyId/search`、`GET .../labels` + `POST .../labels` + `DELETE /labels/:labelId`
- [x] `GET /companies/:companyId/sidebar-badges`
- [x] `GET /companies/:companyId/sidebar-preferences/me` + `PUT .../me`
- [x] `GET /companies/:companyId/users/:userSlug/profile`
- [x] `GET /companies/:companyId/review-cases` + `POST .../review-cases/bulk`
- [x] `GET /companies/:companyId/case-events`
- [x] 公司根资源：`GET /:companyId`、`PATCH /:companyId`、`POST /:companyId/export`、`GET /:companyId/timeline`、`GET /:companyId/artifacts`、`GET /:companyId/feedback-traces`、`POST /:companyId/exports(/preview)`、`POST /:companyId/imports(/preview|/apply)`

### 3. 智能体（Agents）— 已全部补齐
- [x] `GET /agents/:id/runtime-state`、`GET /agents/:id/task-sessions`
- [x] `PATCH /agents/:id/permissions`、`PATCH /agents/:id/instructions-path`
- [x] `GET /agents/:id/instructions-bundle` + `PATCH .../instructions-bundle`
- [x] `GET /agents/:id/instructions-bundle/file` + `PUT` + `DELETE`
- [x] `GET /agents/:id/keys` + `POST .../keys` + `DELETE /agents/:id/keys/:keyId`
- [x] `POST /agents/:id/pause` / `resume` / `clear-error` / `approve` / `terminate` / `wakeup`
- [x] `POST /agents/:id/heartbeat/invoke`、`POST /agents/:id/claude-login`
- [x] `PATCH /agents/:agentId/budgets`
- [x] `GET /agents/me/inbox-lite` + `GET /agents/me/inbox/mine`

### 4. 审批（Approvals）— 已实现
- [x] `GET /companies/:companyId/approvals`、`GET /approvals/:id`
- [x] `POST /companies/:companyId/approvals`（body: `createApprovalSchema`）
- [x] `GET /approvals/:id/issues`
- [x] `POST /approvals/:id/approve` / `reject`（body: `resolveApprovalSchema`）
- [x] `POST /approvals/:id/request-revision` / `resubmit`
- [x] `GET /approvals/:id/comments` + `POST .../comments`（`addApprovalCommentSchema`）

### 5. 执行 / 运行（Execution Workspaces, Heartbeat & Live Runs）
- [x] `GET /companies/:companyId/heartbeat-runs` / `live-runs`
- [x] `GET /heartbeat-runs/:runId` + `/cancel` + `/events` + `/log` + `/issues` + `/watchdog-decisions` + `/workspace-operations`
- [x] `GET /workspace-operations/:operationId/log`
- [x] `GET /issues/:issueId/live-runs` / `active-run`、`GET /issues/:id/runs`
- [x] `GET /execution-workspaces/:id/close-readiness` / `workspace-operations`、`POST .../reconcile-branch`
- [x] `GET /companies/:companyId/workspace-overview`

### 6. 环境 / 适配器（Environments & Adapters）
- [x] `GET /adapters` + `POST /adapters/install`
- [x] `GET /adapters/:type` + `PATCH /adapters/:type` + `PATCH .../override` + `DELETE .../type`
- [x] `POST /adapters/:type/reload` / `reinstall`
- [x] `GET /adapters/:type/config-schema` / `ui-parser.js`
- [x] `GET /companies/:companyId/adapters/:type/model-profiles`
- [x] `GET /companies/:companyId/environments/capabilities` + `POST .../environments/probe-config`
- [x] `GET /environments/:id` + `PATCH /environments/:id` + `DELETE /environments/:id`
- [x] `GET /environments/:environmentId/custom-image-template` + `DELETE` + `rollback`
- [x] `POST /environments/:environmentId/custom-image-setup-sessions` + `GET /environment-custom-image-setup-sessions/:id/finish` + `/cancel`
- [x] `GET /environment-leases/:leaseId`

### 7. 成本 / 预算（Costs & Budgets）— 已实现
- [x] `POST /companies/:companyId/cost-events` / `finance-events`
- [x] `GET /companies/:companyId/costs/summary` / `by-agent` / `by-agent-model` / `by-provider` / `by-biller` / `by-project` / `window-spend` / `quota-windows`
- [x] `GET /companies/:companyId/costs/finance-summary` / `finance-by-biller` / `finance-by-kind` / `finance-events`
- [x] `GET /issues/:id/cost-summary`
- [x] `GET /companies/:companyId/budgets/overview`、`POST .../budgets/policies`、`PATCH .../budgets`
- [x] `POST /companies/:companyId/budget-incidents/:incidentId/resolve`

### 8. 看板对话 / 仪表盘（Board Chat & Dashboard）
- [x] `POST /board/chat/stream` — SSE 流式对话
- [x] `GET /companies/:companyId/dashboard`

### 9. 插件系统（Plugins）— 已实现
- [x] `GET /plugins` / `/plugins/examples` / `/plugins/ui-contributions` / `/plugins/tools` / `POST /plugins/tools/execute`
- [x] `POST /plugins/install`
- [x] `GET /plugins/:pluginId` + `DELETE` + `/enable` + `/disable` + `/health` + `/logs` + `/upgrade` + `/dashboard`
- [x] `GET /plugins/:pluginId/config` + `POST .../config` + `POST .../config/test`
- [x] `POST /plugins/:pluginId/bridge/data` / `/bridge/action`、`GET .../bridge/stream/:channel`
- [x] `POST /plugins/:pluginId/data/:key` / `/actions/:key`
- [x] `GET /plugins/:pluginId/jobs` + `GET .../jobs/:jobId/runs` + `POST .../jobs/:jobId/trigger`
- [x] `POST /plugins/:pluginId/webhooks/:endpointKey`
- [x] `GET /plugins/:pluginId/companies/:companyId/local-folders`(+`:folderKey/status`)、`POST .../validate`、`PUT .../:folderKey`
- [x] `GET /_plugins/:pluginId/ui/*filePath` — 静态资源

### 10. 技能（Skills）— catalog / versions / test-runs 已实现
- [x] `GET /skills/catalog` + `/skills/catalog/:catalogId` + `/files`
- [x] `GET /companies/:companyId/skills/categories`
- [x] `GET .../skills/:skillId` + `/fork-precheck` + `/versions` + `/versions/:versionId`
- [x] `GET .../skills/:skillId/test-inputs` + `POST` + `PATCH` + `DELETE`
- [x] `GET .../skill-test-run-templates` + `POST` + `PATCH` + `DELETE`
- [x] `GET .../skills/:skillId/test-runs` + `/:runId` + `POST .../:runId/cancel` + `DELETE .../:runId`
- [x] `POST .../skills/:skillId/star` + `DELETE .../star` + `/fork` + `/audit` + `/install-update` + `/reset`
- [x] `GET .../skills/:skillId/update-status` + `/comments`(+`POST`/`PATCH`/`DELETE`)
- [x] `GET .../skills/:skillId/files` + `PATCH` + `DELETE`
- [x] `POST .../skills/import` + `/skills/install-catalog` + `/skills/scan-projects` + `DELETE .../skills/:skillId`

### 11. 案例（Cases）— 已全部补齐
- [x] `POST /cases/:id/links` + `GET /cases/:id/children` + `/children/tree` + `/rollup` + `/context-pack` + `/outputs`
- [x] `POST /cases/:id/claim` / `release` / `transition` / `suggest-transition` / `resolve-suggestion` / `review` / `acknowledge-drift` / `open-conversation` / `breakdown`
- [x] `PUT /cases/:id/blockers`
- [x] `GET /cases/:id/issue-links` + `POST` + `DELETE .../issue-links/:linkId`
- [x] `POST /cases/:id/attachments`
- [x] `GET /cases/:id/documents/:key/revisions` + `POST .../revisions/:revisionId/restore` + `DELETE .../documents/:key`
- [x] `GET /cases/:id/documents/:key/annotations`(+`:threadId`, `POST`/`PATCH`/`comments`)
- [x] `POST /cases/:id/automation/retry` + `/automation/retry-plan` + `/automation/current-stage/rerun` + `/automations/:automationId/retry`

### 12. 议题（Issues）— 已全部补齐
- [x] `GET /issues/:id/activity` + `GET /issues/:id/cases`
- [x] `GET /issues/:id/active-run` / `live-runs` / `runs`
- [x] `GET /issues/:id/accepted-plan-decompositions` + `POST .../accepted-plan-decompositions`
- [x] `GET /issues/:id/approvals` + `POST .../approvals` + `DELETE .../approvals/:approvalId`
- [x] `POST /issues/:id/children` + `POST /issues/:id/read` + `DELETE .../read`
- [x] `POST /issues/:id/inbox-archive` + `DELETE .../inbox-archive`
- [x] `POST /issues/:id/monitor/check-now` + `/scheduled-retry/retry-now`
- [x] `GET /issues/:id/external-objects` + `/external-object-summary` + `POST .../external-objects/refresh`
- [x] `GET /issues/:id/file-resources/list` / `/resolve` / `/content`
- [x] `GET /issues/:id/feedback-votes` + `POST .../feedback-votes` + `GET .../feedback-traces`
- [x] `GET /issues/:id/recovery-actions` + `POST .../recovery-actions/resolve`
- [x] `GET /issues/:id/interactions` + `POST` + `accept/reject/respond/cancel`(+`:interactionId`)
- [x] `GET /issues/:id/documents/:key/revisions` + `POST .../revisions/:revisionId/restore` + `DELETE .../documents/:key`
- [x] `GET /issues/:id/documents/:key/annotations`(+`:threadId`, `POST`/`PATCH`/`comments`)
- [x] `POST /issues/:id/work-products` + `PATCH /work-products/:id` + `DELETE /work-products/:id`
- [x] `GET /issues/:id/comments/:commentId` + `DELETE .../comments/:commentId`

### 13. 管道（Pipelines）— 已全部补齐
- [x] `GET /companies/:companyId/pipelines-attention` + `/review-cases` + `/case-events`
- [x] `GET /pipelines/:pipelineId/health` + `/intake-form`
- [x] `POST /pipelines/:pipelineId/stages` + `PATCH .../stages/:stageId` + `PATCH .../stages/:stageId/automation-env` + `DELETE .../stages/:stageId`
- [x] `PUT /pipelines/:pipelineId/transitions`
- [x] `GET /pipelines/:pipelineId/documents/:key` + `PUT` + `/revisions` + `POST .../revisions/:revisionId/restore`
- [x] `POST /pipelines/:pipelineId/cases` + `/cases/batch`

### 14. 目标 / 例程（Goals & Routines）— 已全部补齐
- [x] `GET /routines/:id/revisions` + `POST .../revisions/:revisionId/restore` + `POST /routines/:id/run`
- [x] `GET /routines/:id/triggers` + `POST .../triggers` + `PATCH /routine-triggers/:id` + `DELETE` + `POST .../rotate-secret`
- [x] `POST /routine-triggers/public/:publicId/fire`

### 15. 项目 / 资源成员（Projects & Resource Memberships）— 已全部补齐
- [x] `GET /projects/:id/workspaces` + `POST` + `PATCH /projects/:id/workspaces/:workspaceId` + `DELETE .../workspaces/:workspaceId`
- [x] `POST /projects/:id/workspaces/:workspaceId/runtime-services/:action` + `/runtime-commands/:action`
- [x] `GET /companies/:companyId/resource-memberships/me/agents/:agentId`（`projects/:projectId` 已有）

### 16. 密钥 / 提供方（Secrets & Providers）
- [x] `GET /companies/:companyId/secret-providers` + `/secret-providers/health` <!-- SE5 + SE6 -->
- [x] `GET /companies/:companyId/secrets` + `POST .../secrets` <!-- SE14 -->
- [x] `GET /secrets/:id` + `PATCH` + `DELETE` + `/rotate` + `/usage` + `/access-events` <!-- SE15-SE20 -->
- [x] `POST /companies/:companyId/secret-provider-configs` + `GET /secret-provider-configs/:id` + `PATCH` + `DELETE` + `/default` + `/health` <!-- SE7-SE13 -->

### 17. 云上游 / 外部对象 / 反馈（Cloud Upstreams & Misc）— 已全部实现
- [x] `GET /cloud-upstreams` + `POST /cloud-upstreams/connect/start` + `/finish`
- [x] `POST /cloud-upstreams/:connectionId/push-runs(/preview)` + `GET/PATCH/POST/CANCEL /push-runs/:runId(/activation)`
- [x] `GET /feedback-traces/:traceId` + `/bundle`、`GET /companies/:companyId/feedback-traces`
- [x] `POST /import/preview` + `GET /import/jobs/:jobId`
- [x] `GET /llms/agent-configuration(.txt)` + `/agent-configuration/:adapterType.txt` + `/agent-icons.txt`
- [x] `GET /openapi.json`、`GET /stats`
- [x] `GET /companies/:companyId/inbox-dismissals` + `POST .../inbox-dismissals`
- [x] 实例设置：`GET/PUT /instance/settings`(+`/general` `/experimental`)、`POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/(preview|run)`、`POST /instance/database-backups`
- [x] 资产：`POST /companies/:companyId/assets/images` + `/logo`、`GET /assets/:assetId/content`

---

## 三、缺失接口的明确接口定义（示例，可直接照此实现）

> 采用 Axum 风格：`async fn handler(State(app), Path(...), Json(body)) -> ApiResult<Json<T>>`。
> 路径参数统一用 `{id}`（Uuid），公司用 `{company_id}`（Uuid）。下面给出每域的 handler 签名 + 请求/响应骨架。

### 3.1 Approvals（新建 `crates/api/src/routes/approvals.rs`）
```rust
// GET  /companies/:company_id/approvals            -> list approvals
// GET  /approvals/:id                              -> ApprovalDetail
// POST /companies/:company_id/approvals           -> create (CreateApprovalInput)
// GET  /approvals/:id/issues                       -> Vec<IssueSummary>
// POST /approvals/:id/approve                      -> resolve (ResolveApprovalInput{ decision_note? })
// POST /approvals/:id/reject                       -> resolve
// POST /approvals/:id/request-revision             -> resolve
// POST /approvals/:id/resubmit                     -> ApprovalDetail
// GET  /approvals/:id/comments                     -> Vec<ApprovalComment>
// POST /approvals/:id/comments                     -> ApprovalComment (AddApprovalCommentInput)

#[derive(Deserialize)] pub struct CreateApprovalInput {
    pub issue_id: Uuid, pub title: String, pub description: Option<String>,
    pub required_approvers: Option<i32>,
}
#[derive(Deserialize)] pub struct ResolveApprovalInput { pub decision_note: Option<String> }
```

### 3.2 Costs & Budgets（新建 `crates/api/src/routes/costs.rs`）
```rust
// POST /companies/:company_id/cost-events          -> record (CreateCostEventInput)
// POST /companies/:company_id/finance-events       -> record (CreateFinanceEventInput)
// GET  /companies/:company_id/costs/summary        -> CostSummary{ total, currency, window }
// GET  /companies/:company_id/costs/by-agent        -> Vec<CostByDimension>
// GET  .../costs/by-agent-model | by-provider | by-biller | by-project
// GET  .../costs/window-spend | quota-windows
// GET  .../costs/finance-summary | finance-by-biller | finance-by-kind | finance-events
// GET  /issues/:id/cost-summary                    -> IssueCostSummary
// GET  .../budgets/overview                        -> BudgetOverview
// POST .../budgets/policies                        -> create policy
// PATCH .../budgets                                 -> update (UpdateBudgetInput)
// PATCH /agents/:agent_id/budgets                  -> update agent budget
// POST .../budget-incidents/:incident_id/resolve   -> resolve incident
```

### 3.3 Plugins（新建 `crates/api/src/routes/plugins.rs`）— 桥接/作业/配置
```rust
// GET  /plugins                                    -> Vec<PluginInfo>
// POST /plugins/install                            -> install (InstallPluginInput{ source, version? })
// GET  /plugins/:plugin_id                         -> PluginInfo
// POST /plugins/:plugin_id/enable | /disable | /upgrade
// GET  /plugins/:plugin_id/health | /logs | /dashboard
// GET  /plugins/:plugin_id/config  | POST .../config | POST .../config/test
// POST /plugins/:plugin_id/bridge/data | /bridge/action
// GET  /plugins/:plugin_id/bridge/stream/:channel  -> SSE
// POST /plugins/:plugin_id/data/:key | /actions/:key
// GET  /plugins/:plugin_id/jobs  | POST .../jobs/:job_id/trigger
// GET  /plugins/:plugin_id/jobs/:job_id/runs
// POST /plugins/:plugin_id/webhooks/:endpoint_key  -> webhook ingest
// GET/POST/PUT /plugins/:plugin_id/companies/:company_id/local-folders(/:folder_key)(/status|/validate)
// GET  /_plugins/:plugin_id/ui/*filepath           -> static asset
```

### 3.4 Skills（补全 `crates/api/src/routes/skills.rs`）
```rust
// GET  /skills/catalog | /skills/catalog/:catalog_id | /files
// GET  /companies/:company_id/skills/categories
// GET  .../skills/:skill_id/versions | /versions/:version_id
// GET  .../skills/:skill_id/test-inputs | POST | PATCH | DELETE
// GET  .../skill-test-run-templates | POST | PATCH | DELETE
// GET  .../skills/:skill_id/test-runs | /:run_id | POST .../:run_id/cancel | DELETE .../:run_id
// POST .../skills/:skill_id/star (DELETE) | /fork | /audit | /install-update | /reset
// GET  .../skills/:skill_id/update-status | /comments (+POST/PATCH/DELETE)
// GET  .../skills/:skill_id/files | PATCH | DELETE
// POST .../skills/import | /skills/install-catalog | /skills/scan-projects | DELETE .../skills/:skill_id
```

### 3.5 Executions / Runs（补全 `crates/api/src/routes/execution_workspaces.rs` + 新建 `runs.rs`）
```rust
// GET  /companies/:company_id/heartbeat-runs | /live-runs
// GET  /heartbeat-runs/:run_id                     -> RunDetail
// POST /heartbeat-runs/:run_id/cancel
// GET  /heartbeat-runs/:run_id/{events,log,issues,watchdog-decisions,workspace-operations}
// GET  /workspace-operations/:operation_id/log
// GET  /issues/:issue_id/live-runs | /active-run | /issues/:id/runs
// GET  /execution-workspaces/:id/close-readiness | /workspace-operations
// POST /execution-workspaces/:id/reconcile-branch
// GET  /companies/:company_id/workspace-overview
```

### 3.6 Agents 动作（补全 `crates/api/src/routes/agents.rs`）
```rust
// GET  /agents/:id/runtime-state | /task-sessions
// PATCH /agents/:id/permissions  (UpdateAgentPermissionsInput)
// GET/PATCH /agents/:id/instructions-bundle
// GET/PUT/DELETE /agents/:id/instructions-bundle/file
// GET/POST/DELETE /agents/:id/keys (/keys/:key_id)
// POST /agents/:id/{pause,resume,clear-error,approve,terminate,wakeup}
// POST /agents/:id/heartbeat/invoke | /claude-login
// PATCH /agents/:agent_id/budgets
// GET  /agents/me/inbox-lite | /agents/me/inbox/mine
```

### 3.7 Cases 状态机（补全 `crates/api/src/routes/cases.rs`）
```rust
// POST /cases/:id/{claim,release,transition,suggest-transition,resolve-suggestion,review,acknowledge-drift,open-conversation,breakdown}
// PUT  /cases/:id/blockers
// GET  /cases/:id/{children,children/tree,rollup,context-pack,outputs,issue-links}
// POST /cases/:id/links (+ GET /issue-links, DELETE /issue-links/:link_id)
// POST /cases/:id/attachments
// POST /cases/:id/automation/{retry,retry-plan,current-stage/rerun} | /automations/:automation_id/retry
// GET  /cases/:id/documents/:key/revisions | POST .../revisions/:revision_id/restore | DELETE .../documents/:key
// GET  /cases/:id/documents/:key/annotations(+:thread_id) | POST | PATCH | /comments
```

### 3.8 Issues 子资源（补全 `crates/api/src/routes/issues.rs` 等）
```rust
// GET  /issues/:id/{activity,cases,active-run,live-runs,runs,accepted-plan-decompositions,approvals,recovery-actions,interactions,feedback-votes,feedback-traces,external-objects,external-object-summary,file-resources/list|resolve|content}
// POST /issues/:id/{accepted-plan-decompositions,approvals,children,read,inbox-archive,monitor/check-now,scheduled-retry/retry-now,external-objects/refresh,recovery-actions/resolve,work-products,interactions,feedback-votes}
// DELETE /issues/:id/{read,inbox-archive,approvals/:approval_id}
// POST /issues/:id/interactions/:interaction_id/{accept,reject,respond,cancel}
// GET  /issues/:id/documents/:key/revisions | POST .../revisions/:revision_id/restore | DELETE .../documents/:key
// GET  /issues/:id/documents/:key/annotations(+:thread_id) | POST | PATCH | /comments
```

### 3.9 Environments / Adapters（补全 `crates/api/src/routes/environments.rs` + `adapters.rs`）
```rust
// GET  /adapters | POST /adapters/install
// GET/PATCH/DELETE /adapters/:adapter_type | PATCH .../override | POST .../reload | /reinstall
// GET  /adapters/:adapter_type/config-schema | /ui-parser.js
// GET  /companies/:company_id/adapters/:adapter_type/model-profiles
// GET  /companies/:company_id/environments/capabilities | POST .../environments/probe-config
// GET/PATCH/DELETE /environments/:environment_id
// GET/DELETE/rollback /environments/:environment_id/custom-image-template
// POST /environments/:environment_id/custom-image-setup-sessions | GET .../finish | /cancel
// GET  /environment-leases/:lease_id
```

### 3.10 基础/运维域（按需新建模块）
```rust
// GET  /get-session (或保留 /api/auth/get-session)、PATCH /profile、GET/POST/PUT ...
// Admin: /admin/users, /admin/users/:user_id/company-access, promote/demote-instance-admin
// Company: /:company_id/{export,timeline,artifacts,feedback-traces}, /:company_id/exports(/preview), /:company_id/imports(/preview|/apply)
// Activity: /companies/:company_id/activity (+POST)
// Labels: /companies/:company_id/labels (+POST), /labels/:label_id (DELETE)
// Dashboard: /companies/:company_id/dashboard
// Board chat: POST /board/chat/stream (SSE)
// Cloud upstreams: /cloud-upstreams(+/connect/start|finish, /:connection_id/push-runs...)
// Instance: /instance/settings(+/general,/experimental), /instance/database-backups
// LLMs: /llms/agent-configuration(.txt), /agent-icons.txt, /agent-configuration/:adapter_type.txt
// Misc: /openapi.json, /stats, /feedback-traces/:trace_id(/bundle), /import/preview, /import/jobs/:job_id
// Assets: POST /companies/:company_id/assets/images, /logo, GET /assets/:asset_id/content
```

---

## 四、落地建议（执行顺序）

- [x] **P0 启动入口**：已新增 `crates/server`（`[[bin]]`）并挂载现有 Router
- [x] **P1 补齐核心域子资源/状态机**：Agents 动作、Cases 状态机、Issues 子资源、Environments/Adapters
- [x] **P2 补齐业务支撑域**：Approvals、Costs/Budgets、Executions/Runs、Skills（catalog/versions/test-runs）
- [x] **P3 平台/运维域**：Plugins、Cloud Upstreams、Instance Settings、LLMs、OpenAPI、Dashboard、Board Chat、Assets
- [x] **P4 收尾**：Auth/Profile/Admin 规范化、Company 根资源导出导入、Activity/Labels、Feedback Traces、File Resources

> 每实现一个模块：在 `crates/api/src/routes/mod.rs` 注册 `xxx_routes`，在对应 service/repository 层补齐方法，并补单元测试（README 提示 `services` 测试命令：`cargo test --lib -p services`）。
