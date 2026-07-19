# Route Conflict Analysis

## 总览 (Summary)

| 冲突路径 | 状态 | 应保留的实现 | 说明 |
|----------|------|--------------|------|
| `GET/POST /companies/:company_id/activity` | ✅ **已解决** | `activity.rs` 的 `list_company_activity` + `create_activity` | `companies.rs` 的 CM1/CM2 冗余实现已被移除，现仅 activity 模块注册，即 Paperclip 1:1 移植 |
| `GET /issues/:id/runs` | ✅ **已解决** | `heartbeat_runs.rs` 的 `list_issue_runs`（X14） | `activity.rs` 的错误重复实现已移除 |
| `GET /heartbeat-runs/:run_id/issues` | ✅ **已解决** | `heartbeat_runs.rs` 的 `list_run_issues`（X7） | `activity.rs` 的错误重复实现已移除 |

- [Part A: `/companies/:company_id/activity`](#part-a-companiescompany_idactivity)
- [Part B: `/issues/:id/runs`](#part-b-issuesidruns)
- [Part C: `/heartbeat-runs/:run_id/issues`](#part-c-heartbeat-runsrun_idissues)

## 执行任务清单 (Execution Tasks)

> 状态说明：⬜ 待执行 / ✅ 已完成。下面所有删除都在 `activity.rs` 内，保留方为 `heartbeat_runs.rs` 与 `companies.rs` 中已无冲突的实现。

### Task 1 — ✅ 已解决：`/companies/:company_id/activity`
- [x] 删除 `companies.rs` 中 CM1/CM2 的路由注册（原 39–43 行）
- [x] 删除 `companies.rs` 中 `list_company_activity`（CM1）与 `record_company_activity`（CM2）两个 handler
- [x] 确认 `activity.rs` 的 `list_company_activity` + `create_activity` 为唯一实现（Paperclip 1:1 移植）
- [x] 验证：`grep -rn "record_company_activity" crates/api` 无残留

### Task 2 — ✅ 已完成：`/issues/:id/runs`
文件：`crates/api/src/routes/activity.rs`
- [x] 删除 `activity.rs` 中的重复路由和 `get_issue_runs` handler
- [x] 保留 `heartbeat_runs.rs` 的 `list_issue_runs`（X14）
- [x] 验证：`get_issue_runs` 在 `crates/api` 中无残留

### Task 3 — ✅ 已完成：`/heartbeat-runs/:run_id/issues`
文件：`crates/api/src/routes/activity.rs`
- [x] 删除 `activity.rs` 中的重复路由和 `get_heartbeat_run_issues` handler
- [x] 保留 `heartbeat_runs.rs` 的 `list_run_issues`（X7）
- [x] 验证：`get_heartbeat_run_issues` 在 `crates/api` 中无残留

### Task 4 — ✅ 已完成：收尾验证
- [x] `cargo check -p parrot-server` 通过，无路由冲突 panic
- [x] `cargo test -p api` 通过
- [x] 全量复扫：`activity.rs` 仅保留 `/companies/:company_id/activity`、`/issues/:id/activity`
- [x] 添加路由合并回归测试，防止三组模块再次出现重复注册

---

<a id="part-a-companiescompany_idactivity"></a>
# Part A: `/companies/:company_id/activity`

## A.1 问题根因 (Root Cause)

应用启动时 panic：

```
thread 'main' (9252240) panicked at .../axum-0.7.9/src/routing/path_router.rs:70:22:
Overlapping method route. Handler for `GET /companies/:company_id/activity` already exists
```

**原因**：Axum 的 `Router` 不允许对同一 `(HTTP Method, 路径)` 组合重复注册 handler。
在 `app_state.rs` 中，同一个路径被**两个不同的路由模块**各注册了一次：

| 模块 | 注册位置 | GET handler | POST handler |
|------|----------|-------------|--------------|
| `companies.rs` (CM1/CM2) | `crates/api/src/routes/companies.rs:40-43` | `list_company_activity` | `record_company_activity` |
| `activity.rs` (Paperclip 一比一迁移) | `crates/api/src/routes/activity.rs:61` | `list_company_activity` | `create_activity` |

合并顺序（`app_state.rs`）：
- 第 268 行：`.merge(crate::routes::companies::company_routes())` ← 先注册
- 第 308 行：`.merge(crate::routes::activity::activity_routes())` ← 再次注册相同路径 → panic

## A.2 其他 `/companies/...` 路径冲突排查 (Full Scan)

对 `crates/api/src/routes/*.rs` 中所有 `/companies/...` 路径做了全量统计。除 `activity` 外的疑似重复项：

- `/companies/:company_id/pipelines` — **非冲突**。两条都在 `pipelines.rs` 内，分别注册 `post(create_pipeline)`（23 行）与 `get(list_pipelines)`（24 行），是同一模块内不同方法的链式注册，Axum 允许。
- `/companies/:company_id/budgets/policies` — **非冲突**。两条都在 `costs.rs` 内，分别注册 `get(list_budget_policies)`（52 行）与 `post(create_budget_policy)`（53 行），同上。

**结论**：当前仅 `/companies/:company_id/activity` 一个真实冲突。其余 `/companies/...` 路径（stats、search、timeline、goals、skills、costs、environments、labels、invites、members、exports、imports、inbox-dismissals、teams-catalog、sidebar-*、branding、archive 等）在各模块中均唯一，无重叠。

## A.3 Paperclip 参考实现对照

参考文件：`/Users/adazhao/workspace/paperclip/server/src/routes/activity.ts`

Paperclip 中 `/companies/:companyId/activity` 由 **activity 模块**独占，且：

**GET** (`activity.ts` 第 ~74 行)：
- 走 `activityService.list(filters)`，filters 支持 `agentId / entityType / entityId / limit`；
- 默认 `normalizeActivityLimit(limit)`，鉴权为 `assertCompanyAccess` + `company_scope:read`。

**POST** (`activity.ts` 第 ~90 行)：
- 使用 `createActivitySchema` 校验 body：
  - `actorType`: enum `["agent","user","system","plugin"]`，默认 `"system"`
  - `actorId`: 必填 string
  - `action`: 必填 string
  - `entityType`: 必填 string
  - `entityId`: 必填 string
  - `agentId`: 可选 uuid
  - `details`: 可选，`sanitizeRecord` 后写入
- 鉴权为 `assertBoard` + `assertCompanyAccess`，返回 `201`。

> Paperclip 的 Company 模块 (`company.ts`) 中**并不存在** `/companies/:companyId/activity` 这个端点。该端点属于 activity 模块。

## A.4 两个冲突方法的对比

| 维度 | `companies.rs` 版本 (CM1/CM2) | `activity.rs` 版本 (Paperclip 迁移) |
|------|-------------------------------|--------------------------------------|
| GET 行为 | 直接 SQL 查 `activity_logs`，固定 `LIMIT 500`，无 query 参数 | 通过 `ActivityQueryParams`（`actorId/entityType/entityId/limit`）过滤，`limit` 默认 50，字段映射为 `companyId/action` 等 |
| POST 行为 | 接收任意 `serde_json::Value`，`eventType` 默认 `company.activity`，`actorType` 默认 `user`，无 schema 校验 | 强类型 `CreateActivityRequest` 校验（actorType 枚举、必填字段），写入 `action/entity_type/entity_id`，返回 `201` |
| 与 Paperclip 一致性 | ❌ 字段命名/校验/行为均不一致，是 CM 补齐时手写的中间版本 | ✅ 文件头明确标注 “Paperclip 一比一迁移”，handler 名/字段/zod 校验一一对应 |
| 注释标注 | `/// CM1` / `/// CM2` | `/// 对应 Paperclip: activityRoutes -> GET/POST /companies/:companyId/activity` |

## A.5 最终判定 (Final Verdict) ✅ 已解决

**保留 `activity.rs` 中的实现（Paperclip 一比一迁移版本）；`companies.rs` 中 CM1/CM2 的重复注册已被移除。**

判定依据（与 Paperclip 对照）：
1. `activity.rs` 是 Paperclip `activity.ts` 的 1:1 迁移，GET 支持 `agentId/entityType/entityId/limit` 过滤（对应 `svc.list(filters)`），POST 用 `CreateActivityRequest` 强类型校验（对应 `createActivitySchema`），返回 201，字段 `companyId/actorType/action/...` 与 Paperclip 一致；
2. `companies.rs` 中的 CM1/CM2 是早期“补齐”手写版本，字段命名（`eventType` vs `action`）、校验、分页都与 Paperclip 不符，且 Paperclip 的 Company 模块本就不含该端点；
3. 同一端点只应归属 activity 模块，符合 Paperclip 的模块边界。

**当前代码状态（已确认）**：`companies.rs` 中的 CM1/CM2 路由注册与 `list_company_activity` / `record_company_activity` 两个 handler **已经被删除**（`companies.rs` 由 590 行缩至 551 行，已无 `activity` 路径，grep 亦无任何引用）。现仅 `activity.rs:61` 注册该路径，冲突已消除，`activity.rs` 版本即为唯一且正确的实现。

> 结论：本冲突**无需再改**，只保留 `activity.rs` 版本即可，它本来就是与 Paperclip 一致的正确实现。

## A.6 后续建议
- 在 CI 中加入启动冒烟测试（或单元级 router 合并测试），避免类似“重复 `.route`”在编译期不报、运行期 panic 的问题。
- CM 补齐清单中 CM1/CM2 已确认由 activity 模块覆盖，companies.rs 中冗余实现已移除，建议更新任务文档标注此状态。

---

<a id="part-b-issuesidruns"></a>
# Part B: `/issues/:id/runs`

## B.1 问题根因 (Root Cause)

应用启动时 panic：

```
thread 'main' (9264773) panicked at .../axum-0.7.9/src/routing/path_router.rs:70:22:
Overlapping method route. Handler for `GET /issues/:id/runs` already exists
```

**原因**：同样是 axum 不允许同一 `(GET, 路径)` 被两个 router 重复注册。在 `app_state.rs` 中：

| 模块 | 注册位置 | GET handler | 标注 |
|------|----------|-------------|------|
| `activity.rs` | `crates/api/src/routes/activity.rs:63` | `get_issue_runs` | `对应 Paperclip: activityRoutes -> GET /issues/:id/runs` |
| `heartbeat_runs.rs` (X14) | `crates/api/src/routes/heartbeat_runs.rs:83` | `get(list_issue_runs)` | `X14` |

合并顺序（`app_state.rs`）：
- 第 308 行：`.merge(crate::routes::activity::activity_routes())` ← 先注册
- 第 317 行：`.merge(crate::routes::heartbeat_runs::heartbeat_run_routes())` ← 再次注册相同路径 → panic

## B.2 语义对比（两个 handler 的实现差异）

| 维度 | `activity.rs` → `get_issue_runs` | `heartbeat_runs.rs` → `list_issue_runs` (X14) |
|------|----------------------------------|----------------------------------------------|
| 数据源关联方式 | `JOIN issue_thread_interactions(heartbeat_run_id, issue_id)` | `context_snapshot->>'issueId' = $1` **UNION** `issues.execution_run_id = hr.id` |
| 返回字段 | `{id, metadata, createdAt}`（仅 3 个，来自 `issue_thread_interactions` 的 metadata） | 完整 run 投影（见 `run_to_json`）：`id, companyId, agentId, invocationSource, status, startedAt, finishedAt, error, exitCode, contextSnapshot, issueId, taskId, createdAt, updatedAt` |
| 与 Paperclip `runsForIssue` 一致性 | ❌ 走的是 `issue_thread_interactions` 表，Paperclip `runsForIssue` 根本不用该表，只返回 metadata，信息严重不足 | ✅ 匹配 `context_snapshot->>'issueId'`，且 `run_to_json` 输出的 `issueId/contextSnapshot` 字段与 Paperclip 返回结构一一对应；`issues.execution_run_id` 分支也覆盖 Paperclip 未显式写但语义等价的 “直接执行 run” |
| 返回体 | `Vec<{id, metadata, createdAt}>` | `Vec<完整 run 对象>`，贴近 Paperclip 的 `runsForIssue` 投影 |
| 模块归属 | activity 模块（Paperclip `activity.ts` 确实含此端点） | execution/heartbeat-run 模块（Paperclip `activity.ts` 中此端点也写在 activity 路由内） |

## B.3 Paperclip 参考实现对照

参考文件：`/Users/adazhao/workspace/paperclip/server/src/routes/activity.ts`（第 ~103 行）+ `server/src/services/activity.ts:runsForIssue`（第 379 行）。

Paperclip 中 `GET /issues/:id/runs` 归属 **activity 路由模块**，handler 调用 `svc.runsForIssue(companyId, issue.id)`，内部查询逻辑：
- 主表 `heartbeatRuns`，关联 `agents`；
- 匹配条件（`or`）：
  1. `heartbeatRuns.contextSnapshot ->> 'issueId' = issueId`；
  2. 存在 `activityLog` 行满足 `entityType='issue' AND entityId=issueId AND runId=heartbeatRuns.id`；
- 返回完整 run 投影（`runId/status/agentId/adapterType/startedAt/finishedAt/createdAt/invocationSource/livenessState/...`），并对命中的 run 附加 `exhaustion` 信息；
- 鉴权：`assertCompanyAccess` + `assertIssueReadAllowed`。

**关键发现**：Paperclip 的 `runsForIssue` **不使用 `issue_thread_interactions` 表**，而是用 `context_snapshot->>'issueId'` 关联。因此 `activity.rs.get_issue_runs`（基于 `issue_thread_interactions`）与 Paperclip 的语义**不一致**；`heartbeat_runs.rs.list_issue_runs`（基于 `context_snapshot`）反而**更贴近** Paperclip 的实现。

## B.4 应保留哪个

**建议保留 `heartbeat_runs.rs` 中的 `list_issue_runs`（X14）实现，删除 `activity.rs` 中的 `get_issue_runs` 注册。**

理由：
1. `heartbeat_runs.rs.list_issue_runs` 用 `context_snapshot->>'issueId'` 关联，与 Paperclip `runsForIssue`（activity.ts 服务的真实查询）一致；其 `run_to_json` 返回的字段（`issueId/contextSnapshot/agentId/status/...`）与 Paperclip 投影一一对应；
2. `activity.rs.get_issue_runs` 依赖 `issue_thread_interactions` 表，该关联方式在 Paperclip `runsForIssue` 中**不存在**，且只回吐 `metadata`，信息严重不足，与 Paperclip 不一致；
3. 尽管 Paperclip 把该端点写在 activity 路由模块内，但本仓库已把 heartbeat-run 相关端点（X1-X11）统一收口到 `heartbeat_runs.rs`，X14 放此模块在功能边界上更合理；保留 `heartbeat_runs.rs` 版本能同时消除冲突且保证语义对齐。

> 注：若团队要求“端点必须归属 activity 模块以镜像 Paperclip 路由文件结构”，亦可反向——保留 `activity.rs` 但把其 `get_issue_runs` 的内部 SQL 改为 `context_snapshot` 关联方式（即把 X14 的逻辑搬入 `activity.rs`）。两种方案都能消除 panic；**推荐方案 A（删 activity 版、留 heartbeat_runs 版）改动最小、且 X14 逻辑已被 `run_to_json` 复用，风险更低。**

## B.5 具体改动（待确认后执行）

1. **`crates/api/src/routes/activity.rs`**
   - 删除第 63 行路由注册：
     ```rust
     .route("/issues/:id/runs", get(get_issue_runs))
     ```
   - 删除对应的 handler 函数 `get_issue_runs`（第 190-225 行）及注释 `/// 对应 Paperclip: activityRoutes -> GET /issues/:id/runs`。
   - 保留 `get_issue_activity`（第 62 行 `/issues/:id/activity`）—— 该端点仅在 activity 模块注册，无冲突。
   - 注意：`activity.rs` 的 `get_issue_runs` 与 `heartbeat_runs.rs` 的 `list_issue_runs` 是**不同模块同名无关函数**，删除 activity 版不会破坏 `heartbeat_runs.rs`。

2. **`crates/api/src/routes/heartbeat_runs.rs`** — 保持不变（保留 X14 `list_issue_runs`）。

3. **`crates/api/src/app_state.rs`** — 保持不变（冲突消除后两个 merge 互不重叠）。

### 验证步骤（改完后）
```bash
cargo build -p api                 # 确认编译通过，不再 panic
grep -rn "get_issue_runs" crates/api   # 确认无残留引用
cargo test -p api                  # 跑现有测试
```

## B.6 最终判定 (Final Verdict) ✅ 已解决

**保留 `heartbeat_runs.rs` 中的 `list_issue_runs`（X14）实现，删除 `activity.rs` 中的 `get_issue_runs` 注册。**

判定依据（与 Paperclip 对照）：
1. Paperclip `runsForIssue`（`activity.ts` 服务，第 379 行）的匹配逻辑是 `heartbeatRuns.contextSnapshot ->> 'issueId' = issueId`（外加 `activityLog.runId` 关联分支），**完全不依赖 `issue_thread_interactions` 表**；
2. `activity.rs.get_issue_runs` 用 `JOIN issue_thread_interactions` 关联，该方式在 Paperclip 中不存在 → 属于**单纯的重复实现 + 错误的数据源**；且只回吐 `{id, metadata, createdAt}`，信息严重缺失，与 Paperclip 返回的完整 run 投影不符；
3. `heartbeat_runs.rs.list_issue_runs`（X14）用 `context_snapshot->>'issueId'` 关联 + `issues.execution_run_id` 分支，正是 Paperclip 的真实查询语义；其 `run_to_json` 输出的 `issueId/contextSnapshot/agentId/status/...` 与 Paperclip 投影一一对应；同模块 X1–X11 已复用 `RUN_SELECT`/`run_to_json`，保留它改动最小、风险最低。

**当前代码状态（已确认）**：`activity.rs` 的 `get_issue_runs` 路由与 handler 已删除，仅保留 `heartbeat_runs.rs` 的 X14 实现。

> 备选方案：若团队要求“端点必须归属 activity 模块以镜像 Paperclip 文件结构”，亦可反向——保留 `activity.rs` 但把其内部 SQL 改为 `context_snapshot` 关联方式（即把 X14 的逻辑搬入 `activity.rs`）。两种都能消除 panic；**推荐前者（删 activity 版、留 heartbeat_runs 版）**。

---

<a id="part-c-heartbeat-runsrun_idissues"></a>
# Part C: `/heartbeat-runs/:run_id/issues`

> 说明：本冲突是在对 `activity.rs` 全部 4 条注册路径逐一与所有已 merge 模块交叉比对时**新发现**的第三处冲突（前两处为 Part A、Part B）。

## C.1 问题根因 (Root Cause)

与 Part B 同类：axum 不允许同一 `(GET, 路径)` 被两个 router 重复注册。在 `app_state.rs` 中：

| 模块 | 注册位置 | GET handler | 标注 |
|------|----------|-------------|------|
| `activity.rs` | `crates/api/src/routes/activity.rs:64` | `get(get_heartbeat_run_issues)` | `对应 Paperclip: activityRoutes -> GET /heartbeat-runs/:runId/issues` |
| `heartbeat_runs.rs` (X7) | `crates/api/src/routes/heartbeat_runs.rs:66` | `get(list_run_issues)` | `X7` |

合并顺序（`app_state.rs`）：
- 第 308 行：`.merge(crate::routes::activity::activity_routes())` ← 先注册
- 第 317 行：`.merge(crate::routes::heartbeat_runs::heartbeat_run_routes())` ← 再次注册相同路径 → 会触发 `Overlapping method route` panic

## C.2 语义对比（两个 handler 的实现差异）

| 维度 | `activity.rs` → `get_heartbeat_run_issues` | `heartbeat_runs.rs` → `list_run_issues` (X7) |
|------|--------------------------------------------|----------------------------------------------|
| 关联方式 | `FROM issue_thread_interactions WHERE heartbeat_run_id = $1` | 先取 run 的 `company_id` + `context_snapshot.issueId`，再 `FROM issues WHERE execution_run_id = $run_id OR id = context_issue_id` |
| 返回字段 | `{issueId, agentId, metadata, createdAt}`（仅 4 个，且是交互表字段） | 完整议题投影：`id, companyId, identifier, title, status, parentId, assigneeAgentId, assigneeUserId, executionRunId, createdAt, updatedAt` |
| 与 Paperclip `issuesForRun` 一致性 | ❌ 用 `issue_thread_interactions` 关联——Paperclip 此端点**不用该表**；只回吐交互字段，与 Paperclip 议题投影不符 | ✅ 用 `execution_run_id` + `context_snapshot.issueId` 关联，对应 Paperclip 的 `fromActivity`（activityLog join issues）+ `contextIssueId` 逻辑；字段投影与 Paperclip 一致 |
| 模块归属 | activity 模块（Paperclip `activity.ts` 含此端点） | execution/heartbeat-run 模块（X7，与 X1–X11 同组） |

## C.3 Paperclip 参考实现对照

参考文件：`/Users/adazhao/workspace/paperclip/server/src/routes/activity.ts`（第 129 行）+ `server/src/services/activity.ts:issuesForRun`（第 521 行）。

Paperclip 中 `GET /heartbeat-runs/:runId/issues` 归属 **activity 路由模块**，handler 调用 `svc.issuesForRun(runId)`，内部逻辑：
1. 先取 run 的 `companyId` + `contextSnapshot.issueId`；
2. `fromActivity`：从 `activityLog` JOIN `issues`（`entityType='issue' AND runId=runId` 且 `visibleIssueCondition()`）取出关联议题（`id/identifier/title/status/priority`）；
3. 再叠加 `contextSnapshot.issueId` 指向的议题；
4. 返回议题投影；鉴权为 `assertAuthenticated` + `assertCompanyAccess` + `company_scope:read`。

**关键发现**：Paperclip 的 `issuesForRun` **不使用 `issue_thread_interactions` 表**，而是用 `activityLog.runId` 与 `context_snapshot.issueId` 关联议题。因此 `activity.rs.get_heartbeat_run_issues`（基于 `issue_thread_interactions`）与 Paperclip 语义**不一致**；`heartbeat_runs.rs.list_run_issues`（基于 `execution_run_id` + `context_snapshot`）反而**更贴近** Paperclip 的实现。

## C.4 应保留哪个

**建议保留 `heartbeat_runs.rs` 中的 `list_run_issues`（X7）实现，删除 `activity.rs` 中的 `get_heartbeat_run_issues` 注册。**

理由（与 Part B 完全相同模式）：
1. `heartbeat_runs.rs.list_run_issues` 用 `execution_run_id` + `context_snapshot.issueId` 关联，与 Paperclip `issuesForRun` 的真实查询语义一致（对应 `fromActivity` + `contextIssueId`）；其返回的议题投影字段与 Paperclip 一致；
2. `activity.rs.get_heartbeat_run_issues` 依赖 `issue_thread_interactions` 表，该表在 Paperclip 中用于交互/审批（`kind ∈ question/approval/review`），**与此“run 关联的议题”端点无关**；且只回吐 `{issueId, agentId, metadata, createdAt}`，信息严重缺失；
3. 本仓库已把 heartbeat-run 相关端点（X1–X11）统一收口到 `heartbeat_runs.rs`，X7 放此模块在功能边界上更合理；保留 `heartbeat_runs.rs` 版本能同时消除冲突且保证语义对齐。

> 注：若团队要求“端点必须归属 activity 模块以镜像 Paperclip 文件结构”，亦可反向——保留 `activity.rs` 但把其内部 SQL 改为 `execution_run_id` + `context_snapshot` 关联方式（即把 X7 的逻辑搬入 `activity.rs`）。两种方案都能消除 panic；**推荐方案 A（删 activity 版、留 heartbeat_runs 版）改动最小、且 X7 逻辑复用同模块查询，风险更低。**

## C.5 具体改动（待确认后执行）

1. **`crates/api/src/routes/activity.rs`**
   - 删除第 64 行路由注册：
     ```rust
     .route("/heartbeat-runs/:run_id/issues", get(get_heartbeat_run_issues))
     ```
   - 删除对应的 handler 函数 `get_heartbeat_run_issues`（第 227–260 行）及注释 `/// 对应 Paperclip: activityRoutes -> GET /heartbeat-runs/:runId/issues`。
   - 保留 `get_issue_activity`（第 62 行 `/issues/:id/activity`）—— 该端点仅在 activity 模块注册，无冲突，且与 Paperclip 一致，应保留。

2. **`crates/api/src/routes/heartbeat_runs.rs`** — 保持不变（保留 X7 `list_run_issues`）。

3. **`crates/api/src/app_state.rs`** — 保持不变（冲突消除后两个 merge 互不重叠）。

### 验证步骤（改完后）
```bash
cargo build -p api                       # 确认编译通过，不再 panic
grep -rn "get_heartbeat_run_issues" crates/api   # 确认无残留引用
cargo test -p api                        # 跑现有测试
```

## C.6 最终判定 (Final Verdict) ✅ 已解决

**保留 `heartbeat_runs.rs` 的 `list_run_issues`（X7），删除 `activity.rs` 的 `get_heartbeat_run_issues`。**

判定依据（与 Paperclip 对照）：
1. Paperclip `issuesForRun`（`activity.ts` 服务，第 521 行）的关联是 `activityLog.runId` + `context_snapshot.issueId`，**完全不依赖 `issue_thread_interactions` 表**；
2. `activity.rs.get_heartbeat_run_issues` 用 `FROM issue_thread_interactions` 关联，该方式在 Paperclip 此端点中不存在 → 与 Part B 一样，属于**误用 `issue_thread_interactions` 的错误重复实现**；且只回吐交互字段，与 Paperclip 议题投影不符；
3. `heartbeat_runs.rs.list_run_issues`（X7）用 `execution_run_id` + `context_snapshot.issueId` 关联，正是 Paperclip `issuesForRun` 的语义；其返回字段（`id/identifier/title/status/...`）与 Paperclip 投影对齐。

**当前代码状态（已确认）**：`activity.rs` 的 `get_heartbeat_run_issues` 路由与 handler 已删除，仅保留 `heartbeat_runs.rs` 的 X7 实现。

---

# 执行步骤详解 (Step-by-Step Execution)

> 所有删除均集中在 `crates/api/src/routes/activity.rs`，保留方 `heartbeat_runs.rs` 与已清理的 `companies.rs` 不动。以下为可直接照做的编辑指令（行号以当前代码为准）。

## Step 1 — Task 1（已完成，仅存档）
`companies.rs` 的 CM1/CM2 冗余实现此前已移除，`/companies/:company_id/activity` 现仅由 `activity.rs:61` 注册。无需操作。

## Step 2 — Task 2：移除 `activity.rs` 中的 `get_issue_runs`（对应 B 冲突）
1. 删除 `activity.rs` 第 63 行：
   ```rust
   .route("/issues/:id/runs", get(get_issue_runs))
   ```
2. 删除整个 `get_issue_runs` 函数体（第 190–225 行注释 + `async fn get_issue_runs ...` 至其 `}`）。
3. **不动**：第 62 行 `/issues/:id/activity` 的 `get_issue_activity`、第 61 行 `list_company_activity`/`create_activity`。
4. **不动**：`heartbeat_runs.rs` 的 `list_issue_runs`（X14，第 514 行）。

## Step 3 — Task 3：移除 `activity.rs` 中的 `get_heartbeat_run_issues`（对应 C 冲突）
1. 删除 `activity.rs` 第 64 行：
   ```rust
   .route("/heartbeat-runs/:run_id/issues", get(get_heartbeat_run_issues))
   ```
2. 删除整个 `get_heartbeat_run_issues` 函数体（第 227–260 行注释 + `async fn get_heartbeat_run_issues ...` 至其 `}`）。
3. **不动**：`heartbeat_runs.rs` 的 `list_run_issues`（X7，第 368 行）。

## Step 4 — Task 4：统一验证
```bash
cargo build -p api                                # 启动不再 panic
grep -rn "get_issue_runs\|get_heartbeat_run_issues" crates/api   # 应为空
cargo test -p api                                 # 现有测试通过
```
预期：`activity.rs` 的 `activity_routes()` 仅余两条路径：
- `GET/POST /companies/:company_id/activity`
- `GET /issues/:id/activity`

# 附录：activity.rs 全部 4 条路径冲突排查（逐一交叉比对）

对 `activity.rs` 注册的每条路径与所有**已 merge** 的路由模块做了交叉比对，结论如下：

| `activity.rs` 路径 | 冲突方 | 结论 |
|--------------------|--------|------|
| `GET/POST /companies/:company_id/activity` | （曾为 `companies.rs` CM1/CM2） | ✅ 已解决，CM1/CM2 已删，仅 activity 模块注册 |
| `GET /issues/:id/activity` | 无 | ✅ 无冲突，与 Paperclip 一致，保留 |
| `GET /issues/:id/runs` | `heartbeat_runs.rs` X14 | ✅ 已解决，保留 X14 |
| `GET /heartbeat-runs/:run_id/issues` | `heartbeat_runs.rs` X7 | ✅ 已解决，保留 X7 |

> 观察：`activity.rs` 中除 `/issues/:id/activity` 外，其余两条疑似 Paperclip 移植的 run/issue 关联端点已清理；当前仅保留真正属于 activity 域的 `/issues/:id/activity` 与 `/companies/:company_id/activity`。
