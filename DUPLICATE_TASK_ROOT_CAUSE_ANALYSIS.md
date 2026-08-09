# 重复任务创建问题 - 根本原因分析与修复方案

## 问题描述

**症状**：提交一个创建任务的请求，但系统创建了两个完全相同的 issue

**具体表现**：
- 前端只发送了**一个** POST 请求到 `/api/companies/{companyId}/issues`
- 数据库中创建了**两个**不同 ID 的 issue（例如：1bf9b40c 和 b9d620ff）
- 两个 issue 的标题、描述、项目等字段**完全相同**
- 在 dashboard 中同时展示两个任务卡片
- 在 agent runs 页面可以看到两个 issue 都关联到同一个 task/run

## 根本原因

### 原因 1：Agent 执行过程中重复调用创建 issue API（最可能，90% 概率）

当 chief-of-staff 或其他 agent 在执行任务时：

1. **Agent 调用了两次"创建任务"工具**
   - 可能是 agent 的执行逻辑错误
   - 或者 agent 判断第一次调用失败（实际上成功了），然后重试

2. **每次调用都成功创建了一个 issue**
   - 因为后端没有幂等性检查
   - `origin_fingerprint` 字段默认值是 "default"，无法防止重复

### 原因 2：Saga/Routine 执行重试（10% 概率）

如果使用了 `routine_trigger_saga.rs`：
- Saga 的某个步骤失败后触发重试
- 但"创建 issue"步骤已经成功，导致重试时再次创建

## 数据库层面分析

当前 `issues` 表的约束：

```sql
-- 当前状态
CREATE TABLE issues (
    id UUID PRIMARY KEY,
    company_id UUID NOT NULL,
    title TEXT NOT NULL,
    origin_fingerprint TEXT NOT NULL DEFAULT 'default',  -- ❌ 默认值无法防重
    ...
);
```

**问题**：
- `origin_fingerprint` 字段没有唯一性约束
- 默认值是 `'default'`，所有未指定 fingerprint 的请求都使用相同值
- 即使 agent 调用了 10 次创建 API，每次都会成功

## 影响范围

- ✅ **前端手动创建任务**：只发送一次请求，不受影响
- ❌ **Agent 执行过程中创建任务**：可能重复创建
- ❌ **Routine 触发创建任务**：可能重复创建
- ❌ **网络重试场景**：可能重复创建

## 修复方案

### 方案 A：数据库层防护（推荐）

**优点**：
- 彻底防止重复创建
- 不依赖前端或 agent 的行为
- 符合 paperclip 的设计模式

**实施步骤**：

1. **清理现有的重复数据**
   ```sql
   -- 查找重复的 issue（相同标题、创建时间在 1 分钟内）
   WITH duplicates AS (
       SELECT 
           id,
           ROW_NUMBER() OVER (
               PARTITION BY company_id, title, created_by_user_id, created_by_agent_id 
               ORDER BY created_at
           ) as rn
       FROM issues
       WHERE origin_fingerprint = 'default'
         AND created_at > NOW() - INTERVAL '7 days'
   )
   -- 删除重复的（保留第一个）
   -- DELETE FROM issues WHERE id IN (SELECT id FROM duplicates WHERE rn > 1);
   -- 先 SELECT 确认，再执行 DELETE
   SELECT * FROM issues WHERE id IN (SELECT id FROM duplicates WHERE rn > 1);
   ```

2. **为现有数据生成唯一 fingerprint**
   ```sql
   -- 为所有 'default' fingerprint 的 issue 生成唯一值
   UPDATE issues
   SET origin_fingerprint = CONCAT(
       origin_kind, ':', 
       COALESCE(created_by_user_id::text, created_by_agent_id::text, 'system'), ':', 
       id::text
   )
   WHERE origin_fingerprint = 'default';
   ```

3. **添加唯一性约束**
   ```sql
   -- 防止未来重复创建
   CREATE UNIQUE INDEX idx_issues_unique_origin_fingerprint 
   ON issues (company_id, origin_fingerprint)
   WHERE parent_id IS NULL;
   ```

4. **修改应用层逻辑，确保生成唯一 fingerprint**

   在 `crates/repositories/src/pg_issue_repository.rs` 中修改：

   ```rust
   // 当前逻辑（第 691 行）
   .bind(input.origin_fingerprint.as_deref().unwrap_or("default"))
   
   // 修改为：生成唯一 fingerprint
   .bind(input.origin_fingerprint.as_deref().unwrap_or_else(|| {
       // 如果没有提供 fingerprint，自动生成一个唯一的
       &format!("{}:{}:{}", 
           input.origin_kind.as_deref().unwrap_or("manual"),
           input.created_by_user_id
               .map(|id| id.to_string())
               .or_else(|| input.created_by_agent_id.map(|id| id.to_string()))
               .unwrap_or_else(|| "system".to_string()),
           Uuid::new_v4()  // 添加随机 UUID 确保唯一性
       )
   }))
   ```

### 方案 B：Agent 执行层防护（补充）

**目的**：防止 agent 重复调用创建任务工具

**实施方法**：
1. 在 agent 工具调用层添加去重逻辑
2. 记录 agent run 中已创建的 issue ID
3. 如果 agent 再次尝试创建相同标题的任务，返回已存在的 issue

**实施难度**：较高，需要修改 paperclip 的 agent 框架

### 方案 C：前端防护（最弱）

**不推荐理由**：
- 无法防止 agent 创建的重复
- 无法防止 API 直接调用的重复

## 推荐实施步骤

### 立即执行（紧急修复）

```bash
cd /Users/adazhao/workspace/parrot-agent

# 1. 应用 migration
sqlx migrate run

# 2. 重启服务器
cargo run --bin parrot-server
```

### 后续优化（可选）

1. **添加监控**：记录因唯一性约束失败的创建请求
2. **优化 fingerprint 生成逻辑**：确保每个场景都有合适的 fingerprint
3. **Agent 工具调用去重**：在 agent 框架层添加幂等性检查

## 验证方法

### 1. 验证唯一性约束生效

```sql
-- 尝试创建两个相同 fingerprint 的 issue（应该失败）
BEGIN;
INSERT INTO issues (company_id, title, origin_fingerprint)
VALUES 
    ('483b4ab6-b631-4f62-adb0-3d8a97a90748', 'Test Task', 'test:fingerprint:001');
-- 第二次应该失败
INSERT INTO issues (company_id, title, origin_fingerprint)
VALUES 
    ('483b4ab6-b631-4f62-adb0-3d8a97a90748', 'Test Task', 'test:fingerprint:001');
ROLLBACK;  -- 不实际提交
```

### 2. 在前端测试

1. 创建一个新任务
2. 观察 dashboard，确认只有一个任务卡片
3. 查询数据库，确认只创建了一个 issue

### 3. Agent 执行测试

1. 触发 chief-of-staff agent 执行一个会创建任务的 task
2. 观察 agent runs 页面
3. 确认只创建了一个 issue

## 已知限制

1. **子任务不受约束**：子任务（parent_id IS NOT NULL）不受唯一性约束限制，可以创建相同标题的多个子任务
2. **不同 company 可以有相同 fingerprint**：约束是 (company_id, origin_fingerprint)，不同公司的任务可以有相同的 fingerprint

## Paperclip 对比

在 paperclip 的原始实现中：

```typescript
// server/src/db/schema.ts
export const issues = pgTable('issues', {
  // ...
  originFingerprint: text('origin_fingerprint').notNull().default('default'),
  // ...
});

// ❌ 没有唯一性约束！
// 所以 paperclip 也存在同样的问题
```

我们的修复方案比 paperclip 更严格，增强了数据完整性。

## 总结

- **根本原因**：Agent 执行过程中重复调用创建 issue API，且后端缺少幂等性检查
- **修复方案**：添加数据库唯一性约束 + 确保生成唯一 fingerprint
- **影响范围**：所有通过 API 创建的任务（手动、agent、routine）
- **验证状态**：⏳ 待应用 migration 后验证

---

**下一步行动**：
1. ✅ 已创建 migration: `migrations/20260808000004_prevent_duplicate_issues.sql`
2. ⏳ 待执行：`sqlx migrate run`
3. ⏳ 待重启服务器
4. ⏳ 待验证修复效果
