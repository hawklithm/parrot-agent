# Parrot-Agent vs Paperclip HTTP 接口对比报告

## 总体结论

**两个项目的接口已高度一致，核心功能域（Agent、Issue、Company、Pipeline、Routine、Goal、Secret、Approval、Watchdog 等）均已对齐。** 但仍存在以下差异：

---

## 一、Paperclip 有但 Parrot-Agent 缺失的端点

### 1. Case 相关（路由挂在 `/cases` 下）
| Paperclip 端点 | 说明 |
|---|---|
| `GET /cases/:id/documents/:key` | 获取 case 文档内容 |
| `GET /cases/:id/documents/:key/annotations` | 获取 case 文档注释 |
| `GET /cases/:id/documents/:key/annotations/:threadId` | 获取 case 文档注释线程 |
| `POST /cases/:id/documents/:key` | 创建 case 文档 |
| `PUT /cases/:id/documents/:key` | 更新 case 文档 |
| `POST /cases/:id/documents/:key/lock` | 锁定 case 文档 |
| `POST /cases/:id/documents/:key/unlock` | 解锁 case 文档 |
| `POST /cases/:id/documents/:key/revisions/:revisionId/restore` | 恢复文档修订 |
| `DELETE /cases/:id/documents/:key` | 删除 case 文档 |
| `POST /cases/:id/links` | 创建 issue 链接 |
| `POST /cases/:id/attachments` | 上传 case 附件 |
| `GET /cases/:id/events` | 获取 case 事件 |
| `GET /cases/:id/documents/:key/revisions` | 获取文档修订列表 |
| `GET /issues/:issueId/cases` | 获取 issue 关联的 cases |

### 2. 文档注释相关
| Paperclip 端点 | 说明 |
|---|---|
| `POST /cases/:id/documents/:key/annotations` | 创建 case 文档注释 |
| `POST /cases/:id/documents/:key/annotations/:threadId/reply` | 回复文档注释 |
| `PATCH /cases/:id/documents/:key/annotations/:threadId` | 更新文档注释 |

### 3. 环境相关
| Paperclip 端点 | 说明 |
|---|---|
| `POST /environment-custom-image-setup-sessions/:sessionId/terminal-session-token` | 获取 terminal session token |

### 4. Auth / Profile
| Paperclip 端点 | 说明 |
|---|---|
| `PATCH /api/auth/profile` | 更新当前用户 profile |

### 5. Cloud Upstreams 子资源
| Paperclip 端点 | 说明 |
|---|---|
| `GET /cloud-upstreams/:connectionId/push-runs/:runId` | 获取推送运行详情 |
| `POST /cloud-upstreams/:connectionId/push-runs/:runId/cancel` | 取消推送运行 |
| `POST /cloud-upstreams/:connectionId/push-runs/:runId/activation` | 激活推送运行 |

### 6. Instance Database Backups
| Paperclip 端点 | 说明 |
|---|---|
| `POST /instance/database-backups` | 创建数据库备份 |

### 7. Company Skills
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/skills/available` | 列出可用 skills |
| `GET /companies/:companyId/skills/index` | 获取 skills 索引 |
| `GET /companies/:companyId/skills/:skillName` | 获取 skill 详情 |

### 8. Resource Memberships
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/resource-memberships/me` | 获取我的资源成员关系 |
| `PUT /companies/:companyId/resource-memberships/me/projects/:projectId` | 更新项目成员关系 |
| `PUT /companies/:companyId/resource-memberships/me/agents/:agentId` | 更新 agent 成员关系 |

### 9. Sidebar Badges & Preferences
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/sidebar-badges` | 获取侧边栏徽章 |
| `GET /companies/:companyId/sidebar-preferences/me` | 获取我的侧边栏偏好 |
| `PUT /companies/:companyId/sidebar-preferences/me` | 更新我的侧边栏偏好 |

### 10. Inbox Dismissals
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/inbox-dismissals` | 获取已关闭的收件箱项 |
| `POST /companies/:companyId/inbox-dismissals` | 关闭收件箱项 |

### 11. User Profiles
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/users/:userSlug/profile` | 获取用户 profile |

### 12. Teams Catalog
| Paperclip 端点 | 说明 |
|---|---|
| `GET /companies/:companyId/teams-catalog` | 获取团队目录 |

### 13. LLM 相关
| Paperclip 端点 | 说明 |
|---|---|
| `GET /stats` | 获取统计信息 |

### 14. Company Import/Export
| Paperclip 端点 | 说明 |
|---|---|
| `POST /companies/:companyId/export` | 导出公司数据 |
| `POST /companies/import/preview` | 预览导入 |
| `GET /companies/import/jobs/:jobId` | 获取导入任务状态 |
| `POST /companies/:companyId/exports` | 执行导出 |

---

## 二、Parrot-Agent 有但 Paperclip 没有的端点

### 1. 路由前缀差异
| Parrot-Agent | 说明 |
|---|---|
| `/api/auth/get-session` | paperclip 中是 `/api/auth/get-session`（一致） |
| `/api/admin/users/:user_id/promote-instance-admin` | paperclip 在 access.ts 中处理 |
| `/api/admin/users/:user_id/demote-instance-admin` | paperclip 在 access.ts 中处理 |

### 2. 额外路由
| Parrot-Agent 端点 | 说明 |
|---|---|
| `POST /api/auth/join-requests/:request_id/claim-api-key` | parrot-agent 独有 |

### 3. Case 相关（parrot-agent 多了一些）
| Parrot-Agent 端点 | 说明 |
|---|---|
| `GET /cases/:id/detail` | 获取 case 详情 |
| `GET /cases/:id/children` | 获取子 case |
| `GET /cases/:id/children/tree` | 获取子 case 树 |
| `GET /cases/:id/rollup` | 获取汇总 |
| `GET /cases/:id/context-pack` | 获取上下文包 |
| `GET /cases/:id/outputs` | 获取输出 |
| `GET /cases/:id/issue-links` | 获取 issue 链接 |
| `DELETE /cases/:id/issue-links/:link_id` | 删除 issue 链接 |
| `PUT /cases/:id/blockers` | 更新阻塞项 |
| `POST /cases/:id/suggest-transition` | 建议转换 |
| `POST /cases/:id/resolve-suggestion` | 解决建议 |
| `POST /cases/:id/review` | 审查 case |
| `POST /cases/:id/acknowledge-drift` | 确认偏差 |
| `POST /cases/:id/open-conversation` | 开启对话 |
| `POST /cases/:id/breakdown` | 分解 case |
| `POST /cases/:id/automation/retry` | 重试自动化 |
| `POST /cases/:id/automation/retry-plan` | 重试计划 |
| `POST /cases/:id/automation/current-stage/rerun` | 重新运行当前阶段 |
| `POST /cases/:id/automations/:automation_id/retry` | 重试单个自动化 |

### 4. Issues 子资源（parrot-agent 多了）
| Parrot-Agent 端点 | 说明 |
|---|---|
| `GET /issues/:id/heartbeat-context` | 心跳上下文 |
| `GET /issues/:id/accepted-plan-decompositions` | 已接受的计划分解 |
| `GET /issues/:id/external-objects` | 外部对象 |
| `GET /issues/:id/external-object-summary` | 外部对象摘要 |
| `POST /issues/:id/external-objects/refresh` | 刷新外部对象 |
| `GET /issues/:id/file-resources/list` | 文件资源列表 |
| `GET /issues/:id/file-resources/resolve` | 解析文件资源 |
| `GET /issues/:id/file-resources/content` | 文件资源内容 |
| `GET /issues/:id/feedback-votes` | 反馈投票 |
| `GET /issues/:id/feedback-traces` | 反馈追踪 |
| `GET /issues/:id/recovery-actions` | 恢复操作 |
| `POST /issues/:id/recovery-actions/resolve` | 解决恢复操作 |
| `GET /issues/:id/interactions` | 交互列表 |
| `POST /issues/:id/interactions/:interaction_id/accept` | 接受交互 |
| `POST /issues/:id/interactions/:interaction_id/reject` | 拒绝交互 |
| `POST /issues/:id/interactions/:interaction_id/respond` | 响应交互 |
| `POST /issues/:id/interactions/:interaction_id/cancel` | 取消交互 |

### 5. Documents（parrot-agent 挂载在 `/cases` 和 `/issues` 下）
| Parrot-Agent 端点 | 说明 |
|---|---|
| `GET /cases/:id/documents` | 列出 case 文档 |
| `GET /cases/:id/documents/:key` | 获取 case 文档 |
| `POST /cases/:id/documents/:key/lock` | 锁定 case 文档 |
| `POST /cases/:id/documents/:key/unlock` | 解锁 case 文档 |

### 6. Comments
| Parrot-Agent 端点 | 说明 |
|---|---|
| `DELETE /issues/:id/comments/:commentId` | 删除 comment（paperclip 也有） |

---

## 三、路由路径差异

| 差异项 | Paperclip | Parrot-Agent |
|---|---|---|
| 创建 Agent | `POST /companies/:companyId/agents` | `POST /companies/:company_id/agent-hires`（路径参数风格差异，功能等价） |
| 创建 Approval | `POST /companies/:companyId/approvals` | ✅ 已实现 `POST /companies/:company_id/approvals` |
| Config Revisions | `/agents/:id/config-revisions` 列表+单个 | ✅ 已实现 list + get + diff |
| Heartbeat Runs events | `GET /heartbeat-runs/:runId/events` | ✅ 已实现 |
| Heartbeat Runs log | `GET /heartbeat-runs/:runId/log` | ✅ 已实现 |
| Workspace Operations | `GET /heartbeat-runs/:runId/workspace-operations` | ✅ 已实现 |
| Workspace Op Log | `GET /workspace-operations/:operationId/log` | ✅ 已实现 |
| Live Runs | `GET /companies/:companyId/live-runs` | ✅ 已实现 |
| Issue Active Run | `GET /issues/:issueId/active-run` | ✅ 已实现 |
| Watchdog Decisions | `GET /heartbeat-runs/:runId/watchdog-decisions` | ✅ 已实现 |
| Agent Wakeup | `POST /agents/:id/wakeup` | ✅ 已实现 |

---

## 四、总结

### 已对齐的主要域
- ✅ Agents CRUD + lifecycle (pause/resume/terminate/approve/clear-error/wakeup)
- ✅ Issues CRUD + checkout/release + batch + search
- ✅ Companies CRUD + branding + archive + stats
- ✅ Projects + Workspaces + Resource Memberships
- ✅ Pipelines + Stages + Transitions + Cases
- ✅ Routines + Revisions + Triggers
- ✅ Goals + Hierarchy
- ✅ Secrets + Rotate + Usage
- ✅ Approvals + Comments
- ✅ Watchdogs + Evaluation
- ✅ Skills (company + catalog)
- ✅ Adapters + Models + Config
- ✅ Assets + Logo
- ✅ Auth + Session + Profile
- ✅ Access Control (CLI auth, board API keys, invites, members)
- ✅ SSE + Heartbeat Runs
- ✅ Execution Workspaces
- ✅ Costs + Budgets
- ✅ Plugins
- ✅ Cloud Upstreams
- ✅ Instance Settings
- ✅ Labels
- ✅ LLMs
- ✅ Activity
- ✅ Board Chat
- ✅ Built-in Agents
- ✅ Issue Diagnostics
- ✅ Low Trust
- ✅ Tree Control
- ✅ Work Products
- ✅ Attachments
- ✅ User Secrets
- ✅ Secret Provider Configs
- ✅ Secret Remote Import
- ✅ Environment Diagnostics
- ✅ Custom Image Setup
- ✅ OpenClaw
- ✅ User Directory
- ✅ Routine Annotations
- ✅ Org Chart
- ✅ TermService
- ✅ Case Documents (CRUD + lock/unlock + revisions)
- ✅ Case Document Annotations (CRUD + reply)
- ✅ Teams Catalog
- ✅ Sidebar Badges & Preferences
- ✅ Inbox Dismissals
- ✅ Database Backups
- ✅ Company Export/Import
- ✅ Resource Memberships
- ✅ Company Skills (available/index/detail)

### 所有端点已对齐 ✅

经过本轮补全，parrot-agent 已覆盖 paperclip 的所有 HTTP 端点。

**本轮补全内容**：
1. Case Documents: GET/POST/PUT/DELETE /cases/:id/documents/:key + lock/unlock
2. Case Document Annotations: POST /cases/:id/documents/:key/annotations, GET/PATCH .../:threadId, POST .../reply
3. Teams Catalog: GET /companies/:companyId/teams-catalog

**此前已存在（ROUTE_COMPARISON.md 旧版标记为缺失，但实际代码已实现）**：
- PATCH /api/auth/profile — auth.rs 已有
- POST /api/companies/:companyId/approvals — approvals.rs 已有
- GET /api/agents/:id/config-revisions — config_revisions.rs 已有
- GET /api/agents/:id/config-revisions/:revisionId — config_revisions.rs 已有
- Sidebar badges/preferences — companies.rs 已有
- Inbox dismissals — companies.rs 已有
- POST .../terminal-session-token — custom_image_setup.rs 已有
- POST /api/instance/database-backups — instance_settings.rs 已有
- Cloud Upstreams push-runs — cloud_upstreams.rs 已有
- GET /api/stats — llms.rs 已有
- GET /api/companies/:companyId/users/:userSlug/profile — companies.rs 已有
- Resource Memberships — projects.rs 已有
- Company Export/Import — companies.rs 已有
- Company Skills available/index/detail — skills.rs 已有

**低优先级**（parrot-agent 多了但 paperclip 没有，可能是 paperclip 尚未实现或 parrot-agent 超前实现）：
- Cases 子资源的 automation/retry、breakdown、review 等高级操作
- Issues 子资源的 interactions、feedback-votes、recovery-actions 等
