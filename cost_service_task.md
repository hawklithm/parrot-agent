# CostService 一比一迁移差异分析 & 待办任务

基于 `crates/services/src/cost_service.rs`（Rust）与 `/Users/adazhao/workspace/paperclip/server/src/services/costs.ts` + `budgets.ts` + `finance.ts` + `quota-windows.ts`（Paperclip/TypeScript）的对比分析。

---

## 已实现 ✅

### CostService（对应 paperclip `costService()`）

| 方法 | Rust | Paperclip | 状态 |
|------|------|-----------|------|
| `createEvent` | ✅ `create_event` | ✅ 含 agent 验证 + 月花费更新 + budget evaluate | ✅ 已迁移 |
| `summary` | ✅ `get_summary` | ✅ 含 utilizationPercent 计算 | ✅ 已迁移 |
| `byAgent` | ✅ `by_agent` | ✅ 含 apiRunCount/subscriptionRunCount | ⚠️ 缺少 billingType 维度 |
| `byAgentModel` | ✅ `by_agent_model` | ✅ 含 provider/biller/billingType 分组 | ⚠️ 缺少 billingType 维度 |
| `byProvider` | ✅ `by_provider` | ✅ 含 apiRunCount/subscriptionRunCount | ⚠️ 缺少 billingType 维度 |
| `byBiller` | ✅ `by_biller` | ✅ 含 providerCount/modelCount | ⚠️ 缺少 billingType 维度 |
| `byProject` | ✅ `by_project` | ✅ 通过 activity_log 关联 run→project | ❌ 实现不同，缺 run→project 关联 |
| `windowSpend` | ✅ `window_spend_multi` | ✅ 5h/24h/7d 三窗口按 provider 分组 | ⚠️ Rust 端 biller 字段硬编码 "mixed"，缺 cachedInputTokens |
| `issueTreeSummary` | ✅ `issue_tree_summary` | ✅ 含 runCount/runtimeMs 递归 CTE | ❌ 缺 heartbeat_runs 聚合 |
| `getInvocationBlock` | ❌ 无 | ✅ budgetService 方法 | ❌ 作为 BudgetService 方法存在 |
| `quotaWindows` | ❌ stub | ✅ `fetchAllQuotaWindows()` 从 adapter 获取 | ❌ 未实现 |

### BudgetService（对应 paperclip `budgetService()`）

| 方法 | Rust | Paperclip | 状态 |
|------|------|-----------|------|
| `overview` | ✅ `get_overview` | ✅ 含 pausedAgentCount/pausedProjectCount/pendingApprovalCount | ✅ 已迁移 |
| `listPolicies` | ✅ `list_policies` | ✅ 返回 BudgetPolicy[] | ✅ 已迁移 |
| `upsertPolicy` | ✅ `upsert_policy` / `upsert_policy_full` | ✅ 含 scope 验证 + 自动 pause/resume + 活动日志 | ⚠️ 缺活动日志记录 |
| `evaluateCostEvent` | ✅ `evaluate_cost_event` | ✅ 含 soft/hard threshold + incident 创建 + 自动 pause | ⚠️ 缺 approval 创建 + 活动日志 |
| `resolveIncident` | ✅ `resolve_incident` | ✅ 支持 raise_budget_and_resume / dismiss | ⚠️ 缺 approval 状态更新 + 活动日志 |
| `getInvocationBlock` | ✅ 实现完整 | ✅ 三层级检查 (company/agent/project) | ✅ 已迁移 |

### FinanceService（对应 paperclip `financeService()`）

| 方法 | Rust | Paperclip | 状态 |
|------|------|-----------|------|
| `createEvent` | ✅ `create_event` | ✅ 含 agent/issue/project/goal 归属验证 | ❌ 缺关联实体归属验证 |
| `summary` | ✅ `get_summary` | ✅ 含 debit/credit/net/estimatedDebit | ✅ 已迁移 |
| `byBiller` | ✅ `by_biller` | ✅ 含 netCents/kindCount | ⚠️ 缺 netCents 字段 |
| `byKind` | ✅ `by_kind` | ✅ 含 netCents/billerCount | ⚠️ 缺 netCents 字段 |
| `list` | ✅ `list_events` | ✅ 按 occurredAt+createdAt 倒序 | ⚠️ 缺 limit 参数支持 |

---

## 缺失/不完整功能清单

### CostService

- [x] **1. `by_agent` / `by_agent_model` / `by_provider` / `by_biller` — 缺少 billingType 维度聚合**
  - Paperclip 区分 `metered_api` vs `subscription_included`/`subscription_overage` 的 runCount 和 token 统计
  - Rust 端 `CostSummaryDto` 结构过于简化，缺少这些细分字段
  - 需要：扩展 Repository 查询 + 添加 `api_run_count`/`subscription_run_count`/`subscription_cached_input_tokens` 等字段

- [x] **2. `by_project` — 实现方式与 Paperclip 不一致**
  - Paperclip 通过 `activity_log` + `issues.projectId` 关联 `heartbeatRunId` 到 project
  - Rust 端直接从 `cost_events.project_id` 聚合，缺少 run→project 的间接关联
  - 需要：实现类似 Paperclip 的 CTE/subquery 关联逻辑

- [x] **3. `window_spend_multi` — biller 字段硬编码，缺 cachedInputTokens**
  - Paperclip 在 window spend 中每个 provider 会聚合真实的 biller（单一时返回该 biller，多个时返回 "mixed"）
  - Rust 端 biller 硬编码为 `"mixed"`
  - Paperclip 返回 `cachedInputTokens`，Rust 端为 `0.0`
  - 需要：按 provider 分组时聚合 biller + 返回 cachedInputTokens

- [x] **4. `issue_tree_summary` — 缺 heartbeat_runs 聚合**
  - Paperclip 的 `issueTreeSummary` 同时返回 `runCount` 和 `runtimeMs`（通过递归 CTE + heartbeatRuns + activityLog 关联）
  - Rust 端 Repository 实现可能缺这部分（需确认 `issue_tree_cost_summary` 的 SQL）
  - 需要：确认 Repository 实现是否包含 runCount/runtimeMs 聚合

- [x] **5. `get_quota_windows` — 目前是 stub**
  - Paperclip 的 `fetchAllQuotaWindows()` 从所有已注册 adapter 获取配额信息（claude_local → anthropic, codex_local → openai）
  - Rust 端返回空数组
  - 需要：实现 `AdapterRegistry` 遍历 + 调用 adapter 的 `get_quota_windows` 方法 + 超时处理

### BudgetService

- [x] **6. `upsert_policy_full` — 缺活动日志记录**
  - Paperclip 在 `upsertPolicy` 中调用 `logActivity()` 记录 `budget.policy_upserted`
  - Rust 端完成策略更新后未记录活动日志
  - 需要：调用 ActivityService 记录活动日志

- [x] **7. `evaluate_cost_event` — 缺 approval 创建 + 活动日志**
  - Paperclip 在 soft/hard threshold 触发时：
    - hard threshold 创建 `approval`（`budget_override_required` 类型）
    - 调用 `logActivity()` 记录 `budget.soft_threshold_crossed` / `budget.hard_threshold_crossed`
  - Rust 端：
    - 未创建 approval 记录
    - 未记录活动日志
  - 需要：集成 ApprovalService + ActivityService

- [x] **8. `resolve_incident` — 缺 approval 状态更新 + 活动日志**
  - Paperclip 在 resolve 时：
    - 更新关联 approval 的状态（approved/rejected）
    - 调用 `logActivity()` 记录 `budget.incident_resolved`
  - Rust 端：
    - `BudgetIncidentResolveInput` 结构过于简化（缺 `action`/`amount`/`decisionNote`）
    - 未更新 approval 状态
    - 未记录活动日志
  - 需要：扩展输入结构 + 集成 ApprovalService + ActivityService

### FinanceService

- [x] **9. `create_event` — 缺关联实体归属验证**
  - Paperclip 验证 agent/issue/project/goal/heartbeatRun/costEvent 属于同一 company
  - Rust 端直接插入，无验证
  - 需要：添加实体归属验证逻辑

- [x] **10. `by_biller` / `by_kind` — 缺 `netCents` 字段**
  - Paperclip 返回 `netCents`（debitCents - creditCents）
  - Rust 端 `FinanceSummaryRowDto` 有 `net_cents` 字段但 Repository 查询可能未返回
  - 需要：确认 Repository SQL 是否包含 netCents 计算

- [x] **11. `list_events` — 缺 `limit` 参数支持**
  - Paperclip 支持 `limit` 查询参数（默认 100，最大 500）
  - Rust 端 hardcode limit=100
  - 需要：添加 limit 参数支持

### Route层差异

- [x] **12. 路由 `PATCH /agents/:agentId/budgets` — 缺失**
  - Paperclip 支持为单个 agent 更新预算（调用 agents.update + budgets.upsertPolicy + logActivity）
  - Rust 端 `costs.rs` 中未注册此路由
  - 需要：添加 agent 级别预算更新路由

- [x] **13. 路由 `POST /companies/:companyId/budgets/policies` — 输入结构与 Paperclip 不匹配**
  - Paperclip 使用 `upsertBudgetPolicySchema` 完整输入（scopeType/scopeId/amount/windowKind/metric 等）
  - Rust 端使用简化版 `CreateBudgetPolicyBody`（仅 max_monthly_cents + alert_threshold_percent）
  - 需要：改用 `UpsertPolicyInput` 完整结构

- [x] **14. 路由 `PATCH /companies/:companyId/budgets` — 与 Paperclip 不一致**
  - Paperclip 同时更新 company.budgetMonthlyCents + 调用 budgets.upsertPolicy + logActivity
  - Rust 端仅调用 upsert_policy
  - 需要：补齐 activity 日志记录

- [x] **15. 路由 `/issues/:id/cost-tree-summary` — company_id 解析为占位符**
  - Paperclip 先 resolve issue（通过 identifier 或 id），然后使用 issue.companyId
  - Rust 端 `company_id = id` 是占位符，实际应通过 issue_service 获取
  - 需要：通过 issue_service 解析 issue 获取真实 company_id

### 整体缺失

- [x] **16. 活动日志集成**
  - Paperclip 在 createEvent/financeEvent/upsertPolicy/evaluateCostEvent/resolveIncident 中都调用 `logActivity()`
  - Rust 端完全没有活动日志
  - 需要：集成 `ActivityService` 或直接写入 `activity_log` 表

- [ ] **17. 权限/认证检查**
  - Paperclip 在 cost routes 中大量使用 `assertCompanyAccess()` / `assertBoard()` / `assertCompanyCostReadAllowed()` / `assertIssueCostReadAllowed()`
  - Rust 端完全没有权限检查
  - 需要：待认证中间件就绪后补全

- [x] **18. 测试覆盖**
  - Paperclip 有 `costs-service.test.ts` 测试文件
  - Rust 端 `cost_service.rs` 无单元测试
  - 需要：添加单元测试 + 集成测试

---

## 优先级建议

| 优先级 | 任务编号 | 说明 |
|--------|----------|------|
| P0 | #5, #12, #13, #14 | 功能缺失或影响业务流程 |
| P1 | #1, #2, #3, #4 | 数据维度不完整 |
| P2 | #6, #7, #8, #9, #10, #11 | 审计/追踪能力缺失 |
| P3 | #15, #16, #17 | 认证/日志/company_id 修复 |
| P4 | #18 | 测试覆盖 |
