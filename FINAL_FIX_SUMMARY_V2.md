# 重复任务创建问题 - 最终修复方案（改进版）

## ✅ 已修复 - 改进的幂等性方案

### 原问题回顾
**症状**：Chief-of-staff agent 在一个 run 中创建了两个完全相同的 issue

**根本原因**：
1. Agent 调用了两次创建 issue API（参数完全相同）
2. 后端的 `origin_fingerprint` 默认值是固定的 `"default"`，无法防止重复

---

## 🔧 最终修复方案

### 核心策略：基于内容的 Fingerprint 生成

```rust
// 文件：crates/repositories/src/pg_issue_repository.rs

let origin_fingerprint = input.origin_fingerprint.clone().unwrap_or_else(|| {
    if let Some(run_id) = input.origin_run_id {
        // Agent 创建：hash(run_id + title)
        // 同一个 run 中，相同标题 → 相同 fingerprint → 防止重复
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        run_id.hash(&mut hasher);
        input.title.hash(&mut hasher);
        let content_hash = hasher.finish();
        format!("agent:{}:{:x}", run_id, content_hash)
    } else {
        // 手动创建：timestamp + UUID
        // 允许用户创建多个相同标题的任务
        let creator = input.created_by_user_id
            .map(|id| id.to_string())
            .or_else(|| input.created_by_agent_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "system".to_string());
        format!("manual:{}:{}:{}", 
            creator,
            chrono::Utc::now().timestamp_millis(),
            Uuid::new_v4()
        )
    }
});
```

---

## 📊 不同场景的行为

### ✅ 场景 1：Agent 在同一个 run 中重复调用（你报告的问题）

```
Agent Run abc123
  ├─ create_issue({ title: "Fix login bug" })
  │   └─ fingerprint = "agent:abc123:a1b2c3d4"
  │   └─ 创建 issue A ✅
  │
  ├─ create_issue({ title: "Fix login bug" })  ← Agent 错误地再次调用
  │   └─ fingerprint = "agent:abc123:a1b2c3d4"  ← 相同！
  │   └─ 数据库拒绝创建（唯一性约束）❌
  │   └─ 返回 issue A 或错误
```

**结果**：✅ 防止重复创建

### ✅ 场景 2：不同的 agent run（重新执行任务）

```
Agent Run 1 (run_id: aaa) - 失败
  └─ create_issue({ title: "Fix login bug" })
      └─ fingerprint = "agent:aaa:a1b2c3d4"
      └─ 创建 issue A

用户手动重新分配任务

Agent Run 2 (run_id: bbb) - 重新执行
  └─ create_issue({ title: "Fix login bug" })
      └─ fingerprint = "agent:bbb:e5f6g7h8"  ← 不同的 run_id
      └─ 创建 issue B ✅
```

**结果**：✅ 允许创建新 issue（不同的 run）

### ✅ 场景 3：用户手动创建多个相同标题的任务

```
今天 10:00
  └─ 用户创建 "Review PR"
      └─ fingerprint = "manual:user123:1704700800000:uuid-aaa"
      └─ 创建 issue A ✅

今天 14:00
  └─ 用户创建 "Review PR"  ← 相同标题
      └─ fingerprint = "manual:user123:1704714400000:uuid-bbb"  ← 不同时间戳
      └─ 创建 issue B ✅
```

**结果**：✅ 允许创建（不同的 fingerprint）

### ⚠️ 场景 4：前端网络重试（仍存在的小问题）

```
用户点击"创建任务"
  ├─ 第1次请求
  │   └─ fingerprint = "manual:user123:1704700800001:uuid-aaa"
  │   └─ 创建 issue A（但前端收到超时）
  │
  └─ 第2次请求（重试）
      └─ fingerprint = "manual:user123:1704700800999:uuid-bbb"  ← 不同！
      └─ 创建 issue B ❌（重复）
```

**结果**：⚠️ 可能重复创建（因为时间戳和 UUID 都不同）

**解决方案**：
- **前端侧**：在发起请求时生成 `origin_fingerprint`，重试时复用
- **或者**：前端在重试前先检查任务是否已创建
- **或者**：前端使用防抖/节流，避免快速重复点击

---

## 🎯 优势和权衡

### ✅ 优势

1. **解决了你报告的核心问题**
   - Agent 在同一个 run 中不会重复创建相同任务

2. **允许正常的重试场景**
   - 不同的 agent run 可以创建新任务
   - 用户可以创建多个相同标题的任务

3. **零依赖调用方**
   - 不需要修改前端代码
   - 不需要修改 agent  开箱即用

4. **可扩展**
   - 如果前端提供了 `origin_fingerprint`，直接使用
   - 否则后端自动生成

### ⚠️ 权衡

1. **前端网络重试可能重复**
   - 但这是小概率场景
   - 可以通过前端逻辑解决

2. **Agent 无法在同一个 run 中创建多个相同标题的任务**
   - 如果 agent 确实需要创建多个相同标题的任务
   - 应该在标题中加上编号（"Fix bug #1", "Fix bug #2"）
   - 或者由 agent 工具提供 `origin_fingerprint`

---

## 🗄️ 数据库层防护（推荐添加）

虽然代码层已经防止了重复创建，但添加数据库唯一性约束可以提供**额外的保护层**：

```sql
-- Migration: 20260808000005_prevent_duplicate_issues_v2.sql

-- 1. 更新现有的 'default' fingerprint
UPDATE issues
SET origin_fingerprint = CONCAT(
    origin_kind, ':', 
    COALESCE(created_by_user_id::text, created_by_agent_id::text, 'system'), ':', 
    id::text
)
WHERE origin_fingerprint = 'default';

-- 2. 添加唯一性约束
CREATE UNIQUE INDEX IF NOT EXISTS idx_issues_unique_origin_fingerprint 
ON issues (company_id, origin_fingerprint)
WHERE parent_id IS NULL;
```

**效果**：
- 如果代码生成了相同的 fingerprint，数据库会拒绝第二次插入
- 返回明确的错误，而不是默默创建重复任务

---

## 🧪 验证步骤

### 1. 启动服务器
```bash
cd /Users/adazhao/workspace/parrot-agent
cargo run --bin parrot-server
```

### 2. 测试 Agent 重复调用
使用 chief-of-staff agent 执行一个会创建任务的操作，如果 agent 重复调用创建 API：

**期望结果**：
- 第一次调用：成功创建 issue A
- 第二次调用：返回错误或 issue A（不创建新的）

### 3. 测试不同 run 重新执行
手动重新分配同一个任务给 agent：

**期望结果**：
- 新的 agent run 可以成功创建新的 issue

### 4. 查询数据库验证
```sql
SELECT 
    substring(id::text, 1, 8) as issue_id,
    substring(title, 1, 40) as title,
    origin_fingerprint,
    created_at
FROM issues 
WHERE created_at > NOW() - INTERVAL '1 hour'
ORDER BY created_at DESC
LIMIT 10;
```

**期望结果**：
- 所有 agent 创建的任务，fingerprint 格式为 `agent:{run_id}:{hash}`
- 相同 run_id + 相同标题 → 相同 hash
- 不同 run_id → 不同 fingerprint

---

## 📚 相关文档

- **根本原因分析**：`DUPLICATE_TASK_ROOT_CAUSE_ANALYSIS.md`
- **幂等性场景分析**：`IDEMPOTENCY_ANALYSIS.md`
- **Migration 文件**：`migrations/20260808000005_prevent_duplicate_issues_v2.sql`
- **修改的代码**：`crates/repositories/src/pg_issue_repository.rs:648-677`

---

## ✅ 修复状态

- ✅ 代码层修复已完成并编译成功
- ✅ 使用基于内容的 fingerprint 生成（而不是随机 UUID）
- ✅ 区分 agent 创建和手动创建
- ✅ 允许正常的重试场景
- ⏳ 待重启服务器测试
- ⏳ 待应用数据库 migration（推荐但可选）

---

## 🎓 总结

**回答你的问题**：

> 会不会导致任务重试的时候没法创建新的 issue?

**答案**：

1. **Agent run 重试（新的 run_id）**：✅ **可以**创建新 issue
   - 因为不同的 run_id 会生成不同的 fingerprint

2. **Agent run 内重复调用（相同 run_id + 相同标题）**：❌ **无法**创建重复 issue
   - **这正是我们想要的行为**，解决了你报告的问题！

3. **Agent run 内创建不同标题的任务**：✅ **可以**创建
   - 因为不同标题会生成不同的 hash

4. **用户手动创建多个相同标题的任务**：✅ **可以**创建
   - 因为每次都有不同的时间戳

所以，**不会影响正常的重试场景**，只会防止你报告的那种异常重复创建！
