# 幂等性与重试场景分析

## 问题：当前修复会破坏幂等性吗？

### 当前实现的问题

```rust
// ❌ 每次调用都生成新的 UUID
let origin_fingerprint = format!("{}:{}:{}", origin_kind, creator, Uuid::new_v4());
```

**会导致**：
```
场景1：前端网络重试
  第1次请求 → fingerprint = "manual:user123:uuid-aaa" → 创建 issue A
  网络超时 → 前端重试
  第2次请求 → fingerprint = "manual:user123:uuid-bbb" → 创建 issue B ❌ 重复！
```

## 不同重试场景的需求

### 场景 1：前端网络重试（需要幂等性）
```
用户点击"创建任务"
  → HTTP 请求发出，但网络超时
  → 前端重试，应该返回已创建的 issue，而不是创建新的
```

**需求**：相同请求的重试应该返回相同的 issue

### 场景 2：Agent run 内重复调用（需要去重）
```
Agent Run abc123
  → 调用 create_issue({ title: "Fix bug" })  → 创建 issue A
  → Agent 判断失败，再次调用 create_issue({ title: "Fix bug" })
  → 应该：返回 issue A 或拒绝创建 ✅
  → 不应该：创建 issue B ❌
```

**需求**：同一个 run 中，相同参数不应该创建多个 issue

### 场景 3：Agent run 失败后重新执行（需要创建新 issue）
```
Agent Run 1 (run_id: aaa) 失败
  → 用户手动重新分配任务
Agent Run 2 (run_id: bbb) 重新执行
  → 应该创建新的 issue ✅
```

**需求**：不同的 run 应该可以创建新的 issue

### 场景 4：用户创建多个相同标题的任务（应该允许）
```
用户创建任务 "Review PR"  → 今天创建
用户创建任务 "Review PR"  → 明天再创建
  → 应该允许 ✅（不同的任务，只是标题相同）
```

**需求**：用户应该可以创建多个相同标题的任务

## 标准的幂等性设计

### 业界标准：Stripe, AWS, GitHub
```http
POST /api/issues
Idempotency-Key: 12345-abcde-67890
Content-Type: application/json

{
  "title": "Fix bug",
  ...
}
```

- **由调用方生成** idempotency key
- **重试时使用相同的 key**
- 后端保证：相同 key 的请求只执行一次

### Paperclip/Parrot-Agent 的现状

查看 parrot-agent 的前端代码：

当前**没有**在前端传递 `origin_fingerprint`，所以后端默认使用 `"default"`。

## 解决方案对比

### 方案 A：调用方生成 fingerprint（最佳，但工作量大）

**实现**：
1. 前端修改：在发起请求时生成 UUID，重试时保持不变
2. Agent 工具修改：在工具调用时生成 fingerprint
3. 后端：接受 fingerprint，添加唯一性约束

**优点**：
- ✅ 完美的幂等性
- ✅ 符合业界标准

**缺点**：
- ❌ 需要修改所有调用方（前端、agent 工具、CLI 等）
- ❌ 工作量大

### 方案 B：后端基于内容生成 fingerprint（推荐）

**实现**：
```rust
let origin_fingerprint = input.origin_fingerprint.clone().unwrap_or_else(|| {
    if let Some(run_id) = input.origin_run_id {
        // Agent 创建：基于 run_id + title 生成哈希
        // 同一个 run 中，相同标题只能创建一次
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        run_id.hash(&mut hasher);
        input.title.hash(&mut hasher);
        let content_hash = hasher.finish();
        format!("agent:{}:{:x}", run_id, content_hash)
    } else {
        // 手动创建：使用时间戳 + UUID
        // 允许用户创建多个相同标题的任务
        format!("manual:{}:{}:{}", 
            creator, 
            chrono::Utc::now().timestamp_millis(),
            Uuid::new_v4()
        )
    }
});
```

**优点**：
- ✅ 解决 agent 重复调用问题（场景 2）
- ✅ 允许不同 run 创建新 issue（场景 3）
- ✅ 允许用户创建多个相同标题的任务（场景 4）
- ✅ 不需要修改调用方

**缺点**：
- ⚠️ 前端网络重试仍然会创建重复（场景 1）
- ⚠️ 但这个问题可以通过前端逻辑解决（重试前检查是否已创建）

### 方案 C：只在后端生成 UUID（当前实现，不推荐）

**问题**：
- ❌ 无法防止 agent 重复调用
- ❌ 无法实现幂等性

## 推荐实施方案

### 第一阶段：后端基于内容生成（立即实施）

这是最低代价的方案，可以解决你报告的问题（agent 重复创建）。

### 第二阶段：前端支持 idempotency key（未来优化）

在前端添加：
```typescript
// 生成一个 idempotency key
const idempotencyKey = `manual:${userId}:${Date.now()}:${crypto.randomUUID()}`;

// 存储在请求配置中，重试时复用
const createIssue = async (data) => {
  return fetch('/api/issues', {
    method: 'POST',
    headers: {
      'X-Idempotency-Key': idempotencyKey,  // 或者放在 body 中
    },
    body: JSON.stringify({
      ...data,
      origin_fingerprint: idempotencyKey,  // 重试时保持不变
    }),
  });
};
```

## 数据库约束

无论使用哪个方案，都应该添加唯一性约束：

```sql
CREATE UNIQUE INDEX idx_issues_unique_origin_fingerprint 
ON issues (company_id, origin_fingerprint)
WHERE parent_id IS NULL;
```

**效果**：
- 如果后端生成了相同的 fingerprint（比如 agent 在同一个 run 中重复调用）
- 数据库会拒绝第二次插入，返回错误
- 前端/agent 可以捕获这个错误，返回已存在的 issue

## 总结

**回答你的问题**：

> 会不会导致任务重试的时候没法创建新的 issue?

**答案**：

1. **Agent run 重试（新的 run_id）**：✅ 可以创建新 issue
   - 因为不同的 run_id 会生成不同的 fingerprint

2. **Agent run 内重复调用（相同 run_id）**：❌ 无法创建重复 issue
   - 这是**正确的行为**，正好解决了你报告的问题

3. **前端网络重试**：⚠️ 会创建重复 issue
   - 这是我的方案的缺陷
   - 但可以通过前端逻辑解决（重试前检查）

---

**建议**：使用方案 B（后端基于内容生成），这是性价比最高的方案。
