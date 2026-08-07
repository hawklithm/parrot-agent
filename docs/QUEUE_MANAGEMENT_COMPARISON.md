# 队列管理实现对比：parrot-agent vs paperclip

## 警告说明

编译器警告：
```rust
warning: fields `issue_id`, `priority`, and `created_at` are never read
   --> crates/services/src/heartbeat_service.rs:555:13
```

这些字段**不应该被删除**，因为它们在 paperclip 的完整实现中用于：
1. `issue_id` - 依赖就绪检查
2. `priority` - 优先级排序
3. `created_at` - 时间排序

## 当前实现 vs paperclip 完整实现

### 当前 parrot-agent 实现（简化版）

```rust
// 当前实现：简单的 SQL 排序
SELECT r.id, r.context_snapshot->>'issueId' as issue_id, i.priority, r.created_at
FROM heartbeat_runs r
LEFT JOIN issues i ON i.id = (r.context_snapshot->>'issueId')::uuid
WHERE r.agent_id = $1 AND r.status = 'queued'
ORDER BY 
    CASE WHEN i.status = 'in_progress' THEN 0 ELSE 1 END,
    COALESCE(i.priority, 3),
    r.created_at ASC
LIMIT $2
```

**特点**：
- ✅ 简单直接，性能好
- ✅ 基本的优先级排序
- ❌ **缺少依赖就绪检查**
- ❌ **缺少复杂的排序逻辑**

---

### paperclip 完整实现

```typescript
// Line 10850: 依赖就绪检查
const dependencyReadiness = await listQueuedRunDependencyReadiness(agent.companyId, queuedRuns);

// Line 10870-10886: 复杂的优先级排序
const prioritizedRuns = [...queuedRuns].sort((left, right) => {
    // 1. 检查依赖是否就绪
    const leftReady = leftIssueId ? (leftReadiness?.isDependencyReady ?? true) : true;
    const rightReady = rightIssueId ? (rightReadiness?.isDependencyReady ?? true) : true;
    
    // 2. 计算排序 rank
    // Rank 0: 依赖就绪 + in_progress
    // Rank 1: 依赖就绪 + 其他状态
    // Rank 2: 非 issue 任务（heartbeat等）
    // Rank 3: 依赖未就绪（blocked）
    const leftRank = leftIssueId ? 
        (leftReady ? (leftIssue?.status === "in_progress" ? 0 : 1) : 3) : 2;
    
    // 3. 按 rank → priority → createdAt 排序
    if (leftRank !== rightRank) return leftRank - rightRank;
    const leftPriorityRank = issueRunPriorityRank(leftIssue?.priority);
    const rightPriorityRank = issueRunPriorityRank(rightIssue?.priority);
    if (leftPriorityRank !== rightPriorityRank) return leftPriorityRank - rightPriorityRank;
    return left.createdAt.getTime() - right.createdAt.getTime();
});

// Line 10891: Claim run（检查权限）
const claimed = await claimQueuedRun(queuedRun, companyAgents);
```

**特点**：
- ✅ 完整的依赖就绪检查
- ✅ 4 级排序（ready + status + priority + time）
- ✅ Agent 组织结构检查
- ❌ 复杂，性能开销大

---

## 缺失功能分析

### 1. 依赖就绪检查 ❌

**paperclip 实现**：
```typescript
async function listQueuedRunDependencyReadiness(
    companyId: string,
    runs: Array<typeof heartbeatRuns.$inferSelect>
): Promise<Map<string, { isDependencyReady: boolean }>> {
    // 1. 提取所有 issue IDs
    // 2. 查询 issue_relations 表（type='blocks'）
    // 3. 检查所有 blocker 是否 done
    // 4. 返回每个 issue 的就绪状态
}
```

**parrot-agent 状态**：
- ✅ **数据结构已存在**: `issue_relations` 表 + `blocked_by_issue_ids` 字段
- ✅ **Repository 已实现**: `load_blocked_by_issue_ids()`
- ❌ **队列逻辑未使用**: `start_next_queued_run_for_agent` 没有调用依赖检查

**影响**：
- ⚠️ **Blocked issues 会被启动**：即使依赖未完成，也会从队列启动
- ⚠️ **运行失败或卡住**：依赖未完成的任务会立即失败或无法继续

---

### 2. 复杂排序逻辑 ⚠️

**当前实现**（2 级排序）：
1. `in_progress` 优先
2. Priority
3. Created time

**paperclip 实现**（4 级排序）：
1. **Dependency ready** (blocked issues 排到最后)
2. **Status** (`in_progress` > others)
3. **Priority** (1-4)
4. **Created time**

**差异示例**：

假设队列中有 3 个 runs：
- Run A: priority=1, in_progress, **依赖未就绪**
- Run B: priority=2, todo, 依赖就绪
- Run C: priority=3, in_progress, 依赖就绪

**当前排序**：A → C → B（忽略依赖）  
**paperclip 排序**：C → B → A（blocked 排到最后）

---

### 3. claimQueuedRun 检查 ❌

**paperclip 实现**：
```typescript
async function claimQueuedRun(
    run: typeof heartbeatRuns.$inferSelect,
    companyAgents: Array<OrgRow>
): Promise<typeof heartbeatRuns.$inferSelect | null> {
    // 1. 检查 agent 组织结构权限
    // 2. 检查 issue 分配权限
    // 3. 更新状态为 running
    // 4. 返回 claimed run 或 null
}
```

**parrot-agent 实现**：
```rust
// 直接更新状态，没有权限检查
sqlx::query("UPDATE heartbeat_runs SET status = 'running' WHERE id = $1 AND status = 'queued'")
```

**影响**：
- ⚠️ **跳过权限检查**：不验证 agent 是否有权限启动该 run
- ⚠️ **可能违反组织结构规则**

---

### 4. Agent 启动锁 ❌

**paperclip 实现**：
```typescript
return withAgentStartLock(agentId, async () => {
    // 整个队列启动逻辑在锁保护下执行
});
```

**parrot-agent 实现**：
- 没有锁机制

**影响**：
- ⚠️ **并发启动风险**：多个 `cancel_run` 可能同时触发队列启动
- ⚠️ **超出 maxConcurrentRuns**：竞争条件下可能启动过多 runs

---

## 解决方案建议

### 选项 1: 完整迁移（推荐）

**实现**：
1. 添加依赖就绪检查函数
2. 改进排序逻辑（4 级排序）
3. 添加 claim run 权限检查
4. 添加 agent 启动锁

**优点**：
- 完全匹配 paperclip 行为
- 避免启动 blocked issues
- 正确的优先级排序

**缺点**：
- 代码复杂度增加（~150 行）
- 多次数据库查询（依赖、blocker 状态）
- 性能开销

**工作量**：约 2-3 小时

---

### 选项 2: 渐进式改进（平衡）

**阶段 1**（立即）：
- ✅ 添加依赖就绪检查（最重要）
- ✅ 改进排序逻辑（加入 ready 维度）
- ⚠️ 暂不添加 claim 检查和锁

**阶段 2**（后续）：
- 添加 agent 启动锁
- 添加 claim run 权限检查

**优点**：
- 解决最关键问题（blocked issues）
- 逐步完善，风险可控

**缺点**：
- 仍有并发风险
- 权限检查缺失

**工作量**：阶段 1 约 1小时，阶段 2 约 1小时

---

### 选项 3: 保持当前实现（最简）

**做法**：
- 添加 `#[allow(dead_code)]` 消除警告
- 在文档中说明限制

**优点**：
- 简单直接
- 性能最好
- 零开销

**缺点**：
- **会启动 blocked issues**（可能导致任务失败）
- 排序不够智能
- 有并发风险

**适用场景**：
- 项目不使用 issue 依赖功能
- Agent 很 runs
- 可接受简化的队列管理

---

## 当前问题修复方案

### 最小修复（消除警告）

```rust
// 选项 A: 添加 allow 属性
#[allow(dead_code)]
struct QueuedRun {
    id: Uuid,
    issue_id: Option<String>,
    priority: Option<i32>,
    created_at: DateTime<Utc>,
}
```

```rust
// 选项 B: 简化查询（只返回 id）
let queued_run_ids: Vec<Uuid> = sqlx::query_scalar(
    "SELECT r.id FROM heartbeat_runs r ...
    ORDER BY ...
"
```

**推荐**：选项 A（保留字段为将来扩展做准备）

---

## 实现路线图

### Phase 1: 修复警告（立即）
- [ ] 添加 `#[allow(dead_code)]` 属性
- [ ] 更新文档说明当前限制

### Phase 2: 核心功能（优先）
- [ ] 实现依赖就绪检查
- [ ] 改进排序逻辑（4 级）
- [ ] 添加测试用例

### Phase 3: 完善（可选）
- [ ] 添加 agent 启动锁
- [ ] 添加 claim run 权限检查
- [ ] 性能优化

---

## 测试建议

### 测试场景：Blocked Issues

**设置**：
1. 创建 Issue A（blocker）
2. 创建 Issue B（blocked by A）
3. 将 A 和 B 分配给同一个 agent
4. 取消 Issue A 的 run

**当前行为**（有 bug）：
- ❌ Issue B 立即从队列启动
- ❌ Issue B 因依赖未完成而失败

**期望行为**：
- ✅ Issue B 保持在队列中（不启动）
- ✅ 只有当 Issue A 完成后，B 才启动

---

## 参考资料

### paperclip 源码位置
- `startNextQueuedRunForAgent`: Line 10825-10903
- `listQueuedRunDependencyReadiness`: 需要查找
- `claimQueuedRun`: 需要查找
- `withAgentStartLock`: 需要查找

### parrot-agent 已有基础设施
- ✅ `issue_relations` 表
- ✅ `blocked_by_issue_ids` 字段
- ✅ `load_blocked_by_issue_ids()` repository 方法

---

## 结论

**当前实现是功能性的简化版本**，适用于：
- 不使用 issue 依赖的场景
- 对队列管理要求不高的场景

**如果要匹配 paperclip 的完整功能**，需要：
1. ✅ 添加依赖就绪检查（最重要）
2. ✅ 改进排序逻辑
3. ⚠️ 添加启动锁（推荐）
4. ⚠️ 添加权限检查（可选）

**立即行动**：
- 添加 `#[allow(dead_code)]` 消除警告
- 更新 TESTING_GUIDE.md 说明当前限制
- 规划 Phase 2 的依赖检查实现
