# Parrot-Agent / Paperclip 功能对齐任务

> 本文档是基于 2026-08-09 两个仓库当前源码的功能盘点和差异清单。
> 对照基准：`/Users/adazhao/workspace/paperclip` 的 `doc/SPEC-implementation.md`、
> `server/src/routes`、`server/src/services`、`packages/*` 和 `ui`。
>
> 状态含义：
> - **已具备**：Parrot 有对应路由/服务/模型，尚未发现明显的结构性缺失。
> - **部分一致**：有入口或基础实现，但行为、数据、权限或运行时语义还没有跟 Paperclip 对齐。
> - **缺失**：Paperclip 有明确功能，而 Parrot 当前没有对应实现。
>
> 这不是“把所有 Paperclip 文件逐个翻译成 Rust”的清单；目标是先完成 Paperclip V1 核心契约，再按当前 Paperclip 已经提供的扩展能力补齐。

## 1. Paperclip 功能全景

| 领域 | Paperclip 当前能力 | Parrot 当前判断 |
|---|---|---|
| 公司与组织 | 公司创建/归档/品牌、Goal、Org Chart、成员/邀请/加入请求、资源成员关系 | **已具备/部分一致**：`companies.rs`、`goals.rs`、`org_chart.rs`、`invites.rs`、`resource_memberships.rs` |
| Agent 生命周期 | 创建/雇佣审批、暂停/恢复/终止、API key、配置、心跳唤醒、状态与可调用性 | **部分一致**：基础生命周期存在；权限、审批后建 Agent、运行时状态和配置语义需核对 |
| 任务/Issue | CRUD、状态机、父子树、评论、标签、依赖/引用、原子 checkout/release、inbox、文档、附件、work products、线程交互 | **部分一致**：主体能力较全，但需做契约级回归，尤其 checkout、文档锁、附件、liveness/recovery |
| 执行与心跳 | Process/HTTP adapter、heartbeat runs、取消、日志、scheduler、watchdog、recovery、workspace/runtime service | **部分一致**：Rust 服务和路由较多；Paperclip 的执行策略、恢复路径、运行日志和工作区生命周期仍需逐项验收 |
| 成本与治理 | token/cost events、company/agent/project budget、80% 告警、100% hard-stop 自动暂停、approval/audit | **部分一致**：模型、路由和服务存在；需验证强制暂停、并发运行取消、汇总口径和审计完整性 |
| 适配器 | 内置/外部 adapter registry、安装/卸载/启停/override/reload/reinstall、schema、UI parser、模型发现、环境测试 | **部分一致**：公司级 adapter 基础能力存在；全局管理接口中安装、配置、override、reload、reinstall 多为存根；缺少 Paperclip 的 JS 插件加载语义 |
| 插件 | 安装、启停、升级、配置校验、health、logs、jobs/runs、webhooks、tools/actions、plugin DB、sandbox/worker | **部分一致**：`plugins.rs` 和 plugin service 存在，但当前 dispatcher/生命周期/运行时隔离明显简化，需要单独对齐 |
| Skills | app catalog、公司技能、版本/文件/评论/测试运行、star/fork、技能策略和 agent runtime selection | **部分一致**：`skills.rs`/`skills_service.rs` 提供基础能力；catalog、版本、测试运行、策略面和完整权限尚未证明一致 |
| Tools/MCP | tool gallery、OAuth/连接、MCP import、tool applications/connections/profiles/policies、runtime slots、gateway、action requests | **缺失**：当前未见等价的 `tool-access`、`tool-gateway` 路由和完整模型/服务 |
| Attention/决策 | attention feed、decision queues、triage、Keep/archive/revive、signed decision bundles、training | **缺失**：当前未见等价路由和持久化决策队列实现 |
| 文档/资源 | issue documents/revisions/annotations、assets/attachments、file resources、folders、workspace file resources、artifact delivery | **部分一致**：Issue 文档/附件/work product 有实现；通用 file resources、folders、完整资源授权和 artifact 交付需核对 |
| 实时与交互 | SSE、WebSocket、board chat、live events、dashboard/sidebar badges/status cards/summary slots | **部分一致**：SSE/WebSocket/board chat 存在；Paperclip 的 dashboard 辅助读模型和 UI 消费契约需核对 |
| 访问控制 | board session、agent bearer key、instance roles、company memberships、principal grants、tool/secret policy、CSRF/rate limit | **部分一致**：Rust ABAC/auth middleware 较完整；需要权限矩阵、所有路由覆盖和安全运营项验收 |
| 秘密 | company secrets/version、user secrets、secret providers、remote import、绑定和日志脱敏 | **部分一致**：对应 Rust 模块存在；secret binding、provider 行为和日志脱敏需对照测试 |
| 可移植性/运维 | company import/export preview/apply、database backup、instance settings、health/openapi、反馈导出 | **部分一致**：导入导出、backup、settings、feedback 入口存在；OpenAPI/健康检查/失败恢复契约需补齐或确认 |
| Board UI/CLI | React Board（dashboard/org/tasks/agents/costs/approvals/activity）、全局 company selector、CLI agent-facing commands | **缺失/部分一致**：Parrot 仓库没有对应 UI；Rust API 可被 CLI 调用但没有 Paperclip CLI 同等产品面 |

## 2. 对齐原则和已确认基线

### 2.1 Paperclip V1 核心契约

以下能力属于 `SPEC-implementation.md` 的 V1 in-scope，不能以“已有同名文件”视为完成：

- 公司生命周期、Goal 层级、Agent 生命周期和组织结构。
- Issue 状态机、父子层级、评论、原子 checkout/release 和冲突 `409`。
- Hire/CEO strategy 审批、Activity Log、Board/Agent 权限边界。
- Heartbeat invocation/status/cancel、Process/HTTP adapter、scheduler。
- Cost ingestion、company/agent/project budget，以及 100% hard-stop 自动暂停。
- Dashboard、Org Chart、任务、审批、成本等 Board UI。

### 2.2 Parrot 已有的主要实现面

以下项目中已经存在对应路由或服务，应改为“契约验收/补差异”，不再重复创建基础模块：

- `crates/api/src/routes/{companies,agents,issues,goals,projects,approvals,costs,activity}.rs`
- `crates/api/src/routes/{heartbeats,heartbeat_runs,adapters,environments,execution_workspaces}.rs`
- `crates/api/src/routes/{secrets,user_secrets,secret_provider_configs,secret_remote_import}.rs`
- `crates/api/src/routes/{routines,pipelines,plugins,skills,work_products,assets,attachments}.rs`
- `crates/api/src/routes/{sse,websocket,board_chat,user_directory,instance_settings}.rs`
- `crates/services/src/adapter_registry_state.rs`、`adapter_plugin_store.rs`、`plugin_lifecycle.rs`、`plugin_loader.rs`
- `crates/services/src/issue_checkout_service.rs`、`task_watchdog.rs`、`recovery_action_service.rs`、`work_timeline_service.rs`

## 3. 待完成任务

### P0：先修复“接口存在但行为不一致”的核心路径

#### P0.1 适配器全局管理必须落地

文件：`crates/api/src/routes/adapters.rs`。

- [ ] `GET /adapters` 返回 Paperclip 兼容的完整 adapter 描述：内置/外部、版本、disabled、override 状态、schema、模型和能力字段。
- [ ] `POST /adapters/install` 真正执行安装、manifest 校验、持久化、registry reload；不能只返回 `installed: true`。
- [ ] `PATCH /adapters/:type` 真正更新 disabled/config，并拒绝未知 adapter 或非法字段。
- [ ] `PATCH /adapters/:type/override` 对齐 override pause/恢复语义，并持久化状态。
- [ ] `DELETE /adapters/:type` 仅允许卸载外部 adapter，同时从 registry 和安装目录移除。
- [ ] `POST /adapters/:type/reload`、`/reinstall` 执行真实生命周期操作，返回实际结果和错误。
- [ ] 校验 `config-schema`、`ui-parser.js`、模型发现和环境测试的响应格式及权限。
- [ ] 明确 JS/TS adapter 的支持边界。Paperclip adapter 来自 npm 包；Parrot 至少需要 Node 子进程/协议桥，或在文档中明确 Rust-only 限制并让 API 正确返回“不支持”。

#### P0.2 补齐 adapter 安装运行时

- [ ] 创建 `crates/services/src/npm_manager.rs`：install/uninstall/local path、版本读取、超时、stdout/stderr、非零退出码和路径安全。
- [ ] 创建 adapter package loader/manifest validator；验证 adapter type、入口、配置 schema、能力声明和版本。
- [ ] 处理安装失败回滚、并发安装锁、registry 热加载失败恢复和重启后的 reconcile。
- [ ] adapter 配置和 auth header/env 必须脱敏，日志不能泄漏 secret。

#### P0.3 核心契约回归

- [ ] 原子 checkout 并发冲突返回 `409`，并严格校验 company、assignee、expected statuses。
- [ ] Agent pause/terminate 与 active heartbeat run 的 graceful cancel/force kill 对齐。
- [ ] company/agent/project budget 汇总、80% 告警、100% hard-stop 自动暂停和禁止新 invocation 对齐。
- [ ] hire approval -> agent 创建/API key -> activity log 的事务边界对齐。
- [ ] 所有 mutation 写 Activity Log；跨 company 访问统一返回正确的 `401/403/404/409/422`。

### P1：补齐 Paperclip 当前已提供但 Parrot 缺失的领域

#### P1.1 Tools / MCP / Tool Gateway（缺失）

- [ ] 增加 tool applications、connections、OAuth、grants/installations、catalog、health/test-call、usage/activity。
- [ ] 增加 tool profiles/entries/effective profile、policy/trust rules、stdio templates、MCP JSON import。
- [ ] 增加 runtime slots、tool runtime health、action requests/decisions 和 agent-facing gateway。
- [ ] 对齐 Paperclip 的 tool secret policy、低信任运行时隔离、内容 guard、权限和审计。

参考：Paperclip `server/src/routes/tool-access.ts`、`tool-gateway.ts`、`server/src/services/tool-*`、`packages/shared`。

#### P1.2 Attention / Decision Queues（缺失）

- [ ] 增加 `/companies/:companyId/attention`，返回 shelf、retentionDays、keep、archivedAt、retentionVersion 等服务端计算字段。
- [ ] 增加 decision queues、queue items、seed rules、triage、retention archive/revive。
- [ ] 增加 signed decision bundle、proposal、accept/reject/cancel 和幂等/版本冲突控制。
- [ ] 每次读写重新授权 source，不能由 queue membership 获得 source visibility；覆盖 agent/board/low-trust/task-bridge/skill-test 场景。

参考：Paperclip `routes/attention.ts`、`decision-queues.ts`、`decisions.ts` 及对应 decision service/repository。

#### P1.3 Skills catalog 与 company skill policy（部分一致）

- [ ] 对齐 app-shipped skills catalog、catalog files、版本、fork/star、评论、test inputs/templates/runs/cancel。
- [ ] 增加 company skill policy 的 read/replace/reset/simulation，并在 skill mutation 和 runtime selection 使用同一个 evaluator。
- [ ] 对齐 skill 来源可信度、secret 引用、执行权限和 agent 可见范围。

参考：Paperclip `routes/company-skills.ts`、`company-skill-policy.ts`、`packages/skills-catalog`。

#### P1.4 Plugin runtime（部分一致）

- [ ] 对齐 plugin manifest/schema/capability 校验、安装 guard、启停/升级/卸载和 reconcile。
- [ ] 对齐 plugin config/test、health/logs、jobs/runs/trigger、webhooks、company local folders。
- [ ] 实现 plugin worker/host service 生命周期、sandbox/provider runtime、plugin DB namespace/migrations、secret handler 和 stream/event bus。
- [ ] 对照 `plugin_service.rs` 中目前仅做声明检查/模拟 dispatch 的路径，替换为真实执行或明确返回未支持。

参考：Paperclip `routes/plugins.ts` 以及 `server/src/services/plugin-*`。

#### P1.5 文档、资源和交付链（部分一致）

- [ ] 对齐通用 file resources、folders、workspace file resources 的 API、路径安全和 company/workspace 授权。
- [ ] 完善 assets/attachments/work products：对象存储 provider、SHA256 去重、content/download、issue/comment 关联、artifact 元数据。
- [ ] 对齐文档 revision/lock/restore/annotation 的 actor、run、版本冲突和删除限制。
- [ ] 建立可检查的 artifact 交付契约：附件-backed artifact 与 workspace-only file reference 不能混用。

参考：Paperclip `routes/file-resources.ts`、`folders.ts`、`assets.ts`、`execution-workspaces.ts`、`services/work-products.ts`。

### P1：现有领域的契约级差异审计

- [ ] Agent：配置可见性 vs profile 可见性、assignability/invokability、manager chain、API key hash/rotation、heartbeat invoke 和 status machine。
- [ ] Issue：状态迁移 guard、parent/child/blocker/reference、comment/thread interaction、inbox/read state、watchdog liveness/recovery。
- [ ] Workspace：execution/project workspace、runtime services/operations、branch reconcile、delivery state、terminal cleanup 和 reopen 语义。
- [ ] Routines/Pipelines：revision/restore、schedule/API/webhook trigger、run provenance、retry/approval 和 stage transition。
- [ ] Secrets：company/user secret、provider config、remote import、agent binding、JSON schema refs、日志/响应脱敏。
- [ ] Companies/Access：export/import preview/apply、invites/join requests、memberships、principal grants、board API keys、CLI auth challenges。
- [ ] Realtime/UI read models：SSE/WebSocket event shape、dashboard counts、activity stream、sidebar badges、status cards、summary slots。

### P2：产品面和运维补齐

- [ ] 若 Parrot 的目标包含完整 Paperclip 产品，新增 Board UI：dashboard、company selector、org chart、tasks/kanban、agent detail、costs、approvals、activity，并覆盖失败提示和 checkout 冲突 toast。
- [ ] 增加 Paperclip CLI 等价的 onboarding、auth/token、company/team/agent/issue/goal/approval/cost/skill/adapter/plugin/routine/workspace 命令，或明确 Parrot 只提供 REST server。
- [ ] 补齐 `/openapi`、health/status、database backup、request id、结构化日志、rate limit、CSRF（如存在 board session）和部署 smoke tests。
- [ ] 对齐 company portability 的导出保真度、导入幂等、失败恢复和敏感信息处理。
- [ ] 对齐 Paperclip 的最小回归门槛：auth boundary、checkout race、hard budget stop、pause/resume、dashboard consistency、approval durability。

## 4. 适配器以外的明确差异索引

以下 Paperclip route family 在 Parrot 当前没有同名等价入口，需逐项确认是“应实现”还是“目标范围明确排除”：

- `attention.ts`、`decision-queues.ts`、`decision-training.ts`、`decisions.ts`
- `tool-access.ts`、`tool-gateway.ts`、`workspace-command-authz.ts`、`workspace-runtime-service-authz.ts`
- `company-skills.ts`、`company-skill-policy.ts`、`teams-catalog.ts`
- `file-resources.ts`、`folders.ts`、`openapi.ts`、`status-cards.ts`、`summary-slots.ts`、`sidebar-badges.ts`、`sidebar-preferences.ts`
- `company-import-paths.ts`、`instance-database-backups.ts`、`smoke-lab.ts`、`org-chart-svg.ts`

注意：路由文件名不是唯一判断标准。例如 Parrot 已把部分 Paperclip 的 access/user-profile/inbox 能力合并到 `access_control.rs`、`user_directory.rs`、`companies.rs` 或 `issues.rs`；验收时必须按 endpoint、响应、权限和数据库副作用对照。

## 5. 测试交付物

- [ ] `crates/api/tests/adapter_routes_test.rs`：列表、安装失败/成功、启停、override、reload、reinstall、卸载、schema、权限。
- [ ] `crates/api/tests/paperclip_contract_test.rs`：按领域覆盖公司、Agent、Issue、审批、成本、heartbeat、workspace、secrets、plugins、skills。
- [ ] 并发 checkout race、budget hard-stop、pause-active-run、approval transaction、company boundary、secret redaction 集成测试。
- [ ] plugin/adapter lifecycle 和 npm/Node bridge 的隔离测试；不依赖真实外部 API key。
- [ ] 若交付 UI，增加 dashboard consistency、approval flow、checkout conflict、agent pause/resume 的 E2E。

## 6. 文档和验收规则

- [ ] 每个对齐项记录 Paperclip 参考源码、Parrot 路径、请求/响应示例、权限和副作用。
- [ ] 实现前先把状态从“缺失/部分一致”改为具体验收项；实现后附测试命令和结果。
- [ ] 对明确不纳入 Parrot 范围的 Paperclip 能力，记录为“排除项”及理由，不能保留模糊的“可能需要”。
- [ ] 不允许存根返回成功：如果运行时暂不支持，应返回明确的 `501/422` 或兼容的错误码，并在文档中说明。
- [ ] 完成标准：P0 核心契约通过；P1 缺失领域有实现或正式排除决定；P2 产品/运维范围得到明确结论。

## 7. 参考位置

- Paperclip V1 合约：`/Users/adazhao/workspace/paperclip/doc/SPEC-implementation.md`
- Paperclip API：`/Users/adazhao/workspace/paperclip/server/src/routes/`
- Paperclip 服务：`/Users/adazhao/workspace/paperclip/server/src/services/`
- Paperclip adapter/plugin/catalog：`/Users/adazhao/workspace/paperclip/packages/`
- Parrot API：`crates/api/src/routes/`
- Parrot services/models/repositories：`crates/{services,models,repositories}/src/`
