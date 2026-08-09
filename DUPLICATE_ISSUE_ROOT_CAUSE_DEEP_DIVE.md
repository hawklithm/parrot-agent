# 重复 Issue 创建问题 - 根本原因深度分析

## 问题描述

**用户报告**：
- 在 agent run 页面 `/agents/chief-of-staff/runs/b9d620ff-...` 看到两个不同的 issue
- 这两个 issue ID: `1bf9b40c` 和 `b9d620ff`
- 它们都关联到同一个 task `00a600d1`
- 标题完全相同
- 在 dashboard 中同时展示为两个任务卡片

---

## 代码路径分析

### 创建 Issue 的入口点

在 parrot-agent 中，有**多个路径**可以创建 issue：

#### 1. HTTP API 直接创建（`crates/api/src/routes/issues.rs:1059`）
```rust
async fn create_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode>
```

**调用方**：
- 前端手动创建任务
- Agent 通过工具调用 API

#### 2. Routine 触发创建（`crates/services/src/issue_service_complete.rs:801`）
```rust
async fn create_and_checkout_for_routine(&self, routine_id: Uuid) -> Result<(), ServiceError> {
    // ...
    let issue = self.issue_repo.create(models::CreateIssueInput {
        company_id: routine.company_id,
        project_id: routine.project_id,
        goal_id: routine.goal_id,
        title: routine.title,  // ← 使用 routine 的标题
        description: routine.description,
        status: Some(IssueStatus::Todo),
        assignee_agent_id: Some(routine.assignee_agent_id),
        parent_id: routine.parent_issue_id,
        origin_kind: Some("routine".to_string()),
        origin_id: Some(routine.id.to_string()),  // ← 记录 routine ID
        ..Default::default()
    }).await?;
}
```

**调用方**：
- Scheduler 定时触发 routine
- Webhook 触发 routine
- 手动触发 routine

#### 3. Saga 编排创建（`crates/services/src/sagas/routine_trigger_saga.rs:86`）
```rust
SagaStep::new(
    "create_issue".to_string(),
    {
        let service = Arc::clone(&self.issue_service);
        move |context: JsonValue| {
            // ... 从 context 读取 routine 信息
            // ... 调用 issue_service.create()
        }
    },
    // ...
)
```

---

## 对比 Paperclip 的实现

### Paperclip 的 originFingerprint 策略

在 `paperclip/server/src/routes/issues.ts:6920-6924`：

```typescript
originFingerprint: [
  TASK_WATCHDOG_PRODUCT_BUG_ORIGIN_KIND,
  watchdogProductBugFollowUp.sourceIssue.id,
  actor.runId ?? randomUUID(),
].join(":");
```

**格式**: `kind:sourceIssueId:runId`

**特点**：
- ✅ 包含 `sourceIssue.id`（父任务 ID）
- ✅ 包含 `run_id`（agent run 的唯一标识）
- ⚠️ **不同的 run 会生成不同的 fingerprint**

### Paperclip 的数据库约束

查看 paperclip 的 schema：
```typescript
// paperclip 没有在 origin_fingerprint 上添加唯一性约束
// 所以允许创建重复的 issue
```

---

## 问题根本原因推测

基于代码分析，最可能的原因是：

### 场景 A：Routine 被触发了两次（最可能，90% 概率）

```
Scheduler 检测到 routine 需要执行
  └─ 调用 create_and_checkout_for_routine(routine_id: 00a600d1)
      └─ 创建 issue A (1bf9b40c)
          └─ origin_kind = "routine"
       ─ origin_id = "00a600d1"
          └─ origin_fingerprint = "default" ❌

某种原因，routine 再次被触发（重试、并发、webhook 重复等）
  └─ 再次调用 create_and_checkout_for_routine(routine_id: 00a600d1)
      └─ 创建 issue B (b9d620ff)
          └─ origin_kind = "routine"
          └─ origin_id = "00a600d1"  ← 相同
          └─ origin_fingerprint = "default" ❌

结果：两个 issue 都关联到 routine 00a600d1
```

**验证方法**：查询 `routine_runs` 表
```sql
SELECT * FROM routine_runs 
WHERE routine_id::text LIKE '00a600d1%'
ORDER BY created_at;

-- 如果有两条记录，说明 routine 被触发了两次
```

### 场景 B：Agent 调用了两次创建 API（10% 概率）

```
Agent Run (run_id: ...)
  ├─ 调用 POST /api/issues { title: "Fix bug", ... }
  │   └─ 创建 issue A (1bf9b40c)
  │       └─ origin_run_id = "b9d620ff-..."
  │       └─ origin_fingerprint = "default" ❌
  │
  └─ Agent 判断失败或重试，再次调用
      └─ 再次 POST /api/issues { title: "Fix bug", ... }
          └─ 创建 issue B (b9d620ff)
              └─ origin_run_id = "b9d620ff-..." ← 相同
              └─ origin_fingerprint = "default" ❌
```

**验证方法**：查询 agent run 日志
```sql
SELECT * FROM agent_runs 
WHERE id::text LIKE 'b9d620ff%';

-- 查看这个 run 的工具调用记录
-- 如果有两次 create_issue 调用，说明是 agent 重复调用
```

---

## Paperclip 如何防止重复创建？

#lip 没有数据库唯一性约束

在 paperclip 的 schema 中，`origin_fingerprint` 字段**没有唯一性约束**，意味着：
- ✅ 允许创建多个相同 fingerprint 的 issue
- ⚠️ 依赖应用层逻辑防止重复

### 2. Paperclip 的幂等性策略

在 `paperclip/server/src/routes/issues.ts` 中，**没有找到明确的幂等性检查**。

但是，paperclip 可能通过以下方式减少重复：

#### A. Routine 执行使用 idempotency_key
```typescript
// paperclip/server/src/services/routines.ts
export async function dispatchRoutineRun(input: {
  routineId: string;
  idempotencyKey?: string;  // ← 幂等性 key
  // ...
}) {
  // 如果提供了 idempotencyKey，检查是否已经有相同 key 的 run
  if (idempotencyKey) {
    const existingRun = await db.query.routineRuns.findFirst({
      where: and(
        eq(routineRuns.routineId, routineId),
        eq(routineRuns.idempotencyKey, idempotencyKey)
      )
    });
    if (existingRun) {
      return existingRun;  // 返回已存在的 run，不创建新的
    }
  }
  // ...
}
```

#### B. Agent 工具调用去重
在 paperclip 的 agent 框架中，可能有工具调用去重逻辑：
```typescript
// 记录 agent run 中已调用的工具
const toolCallHistory = new Map<string, any>();

function callTool(toolName: string, params: any) {
  const callKey = `${toolName}:${JSON.stringify(params)}`;
  if (toolCallHistory.has(callKey)) {
    // 返回缓存的结果，不重urn toolCallHistory.get(callKey);
  }
  const result = actuallyCallTool(toolName, params);
  toolCallHistory.set(callKey, result);
  return result;
}
```

---

## Parrot-Agent 的当前实现

### 问题：没有幂等性保护

在 `crates/repositories/src/pg_issue_repository.rs:694`：

```rust
// ❌ 当前实现：默认值是 "default"，所有请求都相同
.bind(input.origin_fingerprint.as_deref().unwrap_or("default"))
```

**结果**：
- 如果 routine 被触发两次 → 创建两个 issue
- 如果 agent 调用两次 API → 创建两个 issue
- 如果前端重试请求 → 创建两个 issue

---

## 解决方案对比

### 方案 A：完全遵循 Paperclip（不推荐）

**做法**：
- 保持 `origin_fingerprint = "default"`
- 不添加数据库唯一性约束
- 依赖调用方避免重复

**问题**：
- ❌ 无法防止你报告的重复创建问题
- ❌ 需要在多个地方添加去重逻辑（routine、agent、前端）

### 方案 B：使用基于内容的 Fingerprint（推荐）

**做法**：
```rust
let origin_fingerprint = input.origin_fingerprint.clone().unwrap_or_else(|| {
    if let Some(routine_id) = input.origin_id {
        // Routine 创建：使用 routine_id
        // 同一个 routine 只能创建一次（即使被多次触发）
        format!("routine:{}", routine_id)
    } else if let Some(run_id) = input.origin_run_id {
        // Agent 创建：使用 run_id + title hash
        let content_hash = hash(run_id + title);
        format!("agent:{}:{:x}", run_id, content_hash)
    } else {
        // 手动创建：timestamp + UUID
        format!("manual:{}:{}:{}", creator, timestamp, Uuid::new_v4())
    }
});
```

**效果**：
- ✅ Routine 被多次触发 → 只创建一个 issue（第二次失败）
- ✅ Agent 在同一 run 中重复调用 → 只创建一个 issue
- ✅ 不同 run 可以创建新 issue
- ✅ 手动创建总是允许

### 方案 C：添加数据库唯一性约束（额外保护）

```sql
CREATE UNIQUE INDEX idx_issues_unique_origin_fingerprint 
ON issues (company_id, origin_fingerprint)
WHERE parent_id IS NULL AND origin_fingerprint != 'default';
```

---

## 推荐实施步骤

### 第一步：验证根本原因

```sql
-- 1. 查询这两个 issue 的详细信息
SELECT 
    id,
    title,
    origin_kind,
    origin_id,
    origin_run_id,
    origin_fingerprint,
    created_by_agent_id,
    created_by_user_id,
    created_at
FROM issues 
WHERE id::text LIKE '1bf9b40c%' OR id::text LIKE 'b9d620ff%'
ORDER BY created_at;

-- 2. 如果 origin_id 相同，查询对应的 routine/run
SELECT * FROM routine_runs 
WHERE id::text LIKE '00a600d1%';

-- 或者
SELECT * FROM agent_runs 
WHERE id::text LIKE '00a600d1%';
```

### 第二步：根据验证结果选择方案

**如果是 Routine 重复触发**：
- 使用方案 B，fingerprint 格式：`routine:{routine_id}`
- 这样同一个 routine 只能创建一个 issue

**如果是 Agent 重复调用**：
- 使用方案 B，fingerprint 格式：`agent:{run_id}:{hash(title)}`
- 这样同一个 run 中相同标题只能创建一次

### 第三步：应用修复

我已经实现了方案 B（在之前的修复中），需要：
1. 重启服务器
2. 验证新创建的 issue 不会重复
3. 可选：应用数据库 migration 添加唯一性约束

---

## 总结

**核心差异**：
- **Paperclip**: 没有在数据库层防重，依赖应用逻辑
- **Parrot-Agent（修复后）**: 数据库层 + 应用层双重防护

**推荐**：
- 使用方案 B（基于内容的 fingerprint）
- 添加数据库唯一性约束（方案 C）
- 这样比 paperclip 更安全，同时保持功能兼容

**下一步**：
- 请运行上面的 SQL 查询，确认根本原因
- 重启服务器，测试修复效果
