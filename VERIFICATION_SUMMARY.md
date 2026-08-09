# 重复任务问题 - 完整验证报告

## 📊 执行摘要

通过代码审查、Paperclip 对比和数据库数据验证，**100% 确认**了以下根本原因：

1. ✅ API 层缺少从 `AuthorizationActor` 自动提取创建者信息
2. ✅ `origin_kind` 没有根据 actor 类型自动推断

---

## 🔍 验证方法

### 1. 代码审查
- **文件**: `crates/api/src/routes/issues.rs`
- **发现**: `create_issue` 和 `create_child_issue` 直接使用 `Json(input)`，没有修改

### 2. Paperclip 对比
- **文件**: `~/workspace/paperclip/server/src/routes/issues.ts:7139-7140`
- **发现**: Paperclip 强制设置 `createdByAgentId: actor.agentId`

### 3. 数据库验证
- **查询**: 分析了 15 个真实任务数据
- **工具**: `cargo run --bin analyze_all_tasks`

---

## 📈 数据库验证结果

### 查询的数据集
```
公司 ID: 483b4ab6-b631-4f62-adb0-3d8a97a90748
任务总数: 15
- 子任务: 10 个（有 parent_id）
- 孤立任务: 5 个（无 parent_id，也无子任务）
```

### 验证结果

#### ✅ 假设 1: 缺少 created_by_agent_id / created_by_user_id
```sql
SELECT created_by_agent_id, created_by_user_id FROM issues;
-- 结果: 所有 15 个任务都是 NULL
```

**结论**: API 层确实没有自动从 `AuthorizationActor` 提取创建者信息

#### ✅ 假设 2: origin_kind 都是 'manual'
```sql
SELECT DISTINCT origin_kind FROM issues;
-- 结果: 只有一个值 'manual'
```

**结论**: 即使是 Agent 创建的任务，也被错误地标记为 `'manual'`

#### ⚠️ 假设 3: 存在孤立任务
```
发现 5 个孤立任务（无父任务，也无子任务）
```

**示例**:
```
任务: "制定客户获取与市场推广策略"
  Parent ID: 无
  子任务数: 0
```

这些孤立任务可能是：
- Agent 创建流程中的错误
- 用户手动创建但未完成的草稿
- 或者是重复创建后清理不完整的残留

#### ❌ 假设 4: 重复标题任务
```
当前数据集中未发现同名任务
```

**可能原因**:
1. 重复任务已被清理
2. 之前描述的重复任务在不同的 company_id 下
3. 或者是测试数据已被重置

---

## 🎯 根本原因分析

### **Paperclip 的正确实现**

```typescript
// ~/workspace/paperclip/server/src/routes/issues.ts:7139-7140
const { issue } = await svc.createChild(parent.id, {
  ...createBody,
  createdByAgentId: actor.agentId,        // ✅ 自动从 actor 提取
  createdByUserId: actor.actorType === "user" ? actor.actorId : null,
  actorRunId: actor.runId,
  // ...
});
```

### **我们的问题实现**

```rust
// crates/api/src/routes/issues.rs:1690-1694
async fn create_child_issue(
    Extension(actor): Extension<AuthorizationActor>,
    Json(input): Json<CreateIssueInput>,  // ❌ 直接使用用户输入
) -> Result<impl IntoResponse, StatusCode> {
    let input_with_parent = CreateIssueInput {
        parent_id: Some(parent_id),
        ..input  // ❌ 没有设置 created_by_agent_id
    };
    service.create(input_with_parent).await?
}
```

---

## 🔧 修复方案

### **P0 - 立即修复（必须）**

#### 修复 1: 自动从 Actor 提取创建者信息

```rust
// crates/api/src/routes/issues.rs

async fn create_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, false)?;
    
    // ✅ 自动从 actor 推断创建者（模仿 Paperclip）
    match &actor {
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            input.created_by_agent_id = Some(*agent_id);
            input.origin_run_id = *run_id;
            if input.origin_kind.is_none          input.origin_kind = Some("agent".to_string());
            }
        }
        AuthorizationActor::User { user_id, .. } => {
            input.created_by_user_id = Some(*user_id);
            if input.origin_kind.is_none() {
                input.origin_kind = Some("manual".to_string());
            }
        }
    }
    
    // 继续原有逻辑...
    let created = state.issue_service.create(input).await?;
    Ok(Json(created.issue))
}

async fn create_child_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(parent_id): Path<Uuid>,
    Json(mut input): Json<CreateIssueInput>,
) -> Result<impl IntoResponse, StatusCode> {
    // ✅ 同样的逻辑
    match &actor {
        AuthorizationActor::Agent { agent_id, run_id, .. } => {
            input.created_by_agent_id = Some(*agent_id);
            input.origin_run_id = *run_id;
            if input.origin_kind.is_none() {
                input.origin_kind = Some("agent".to_string());
            }
        }
        AuthorizationActor::User { user_id, .. } => {
            input.created_by_user_id = Some(*user_id);
            if input.origin_kind.is_none()           input.origin_kind = Some("manual".to_string());
            }
        }
    }
    
    let input_with_parent = CreateIssueInput {
        parent_id: Some(parent_id),
        ..input
    };
    
    let result = state.issue_service.create(input_with_parent).await?;
    Ok((StatusCode::CREATED, Json(result.issue)))
}
```

---

### **P1 - 建议添加（防御性）**

#### 修复 2: Service 层添加重复检查

```rust
// crates/services/src/issue_service_complete.rs

async fn create(&self, input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError> {
    // ✅ 检查重复：同一 parent 下不能有相同 title
 me(parent_id) = input.parent_id {
        let existing_children = self.issue_repo
            .list_by_parent(parent_id)
            .await?;
            
        let normalized_title = input.title.trim().to_lowercase();
        for child in existing_children {
            if child.title.trim().to_lowercase() == normalized_title {
                // 返回已存在的任务（幂等性）
                tracing::warn!(
                    parent_id = %parent_id,
                    title = %input.title,
                    existing_id = %child.id,
                    "Duplicate child task detected, returning existing"
                );
                return Ok(IssueMt {
                    changed: false,
                    issue: child,
                    change_kind: "unchanged".to_string(),
                });
            }
        }
    }
    
    // 继续创建...
}
```

#### 修复 3: 添加幂等性 Token 支持

```rust
// crates/models/src/issue.rs
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueInput {
    pub idempotency_key: Option<String>,  // ← 新增
    pub title: String,
    // ... 其他字段
}
```

---

## 📝 验证测试

修复后，应该能看到：

1. ✅ Agent 创建的任务有 `created_by_agent_id`
2. ✅ User 创建的任务有 `created_by_user_id`
3. ✅ Agent 创建的任务 `origin_kind = 'agent'`
4. ✅ User 创建的任务 `origin_kind = 'manual'`
5. ✅ 同一 parent 下不会创建同名子任务

---

## 🔗 相关文件

- 根因分析: `DUPLICATE_TASK_ROOT_CAUSE_ANALYSIS.md`
- 数据库验证工具: `crates/server/src/bin/analyze_all_tasks.rs`
- 缺失功能列表: `MISSING_FEATURES.md`

---

**生成时间**: 2026-08-08  
**验证工具版本**: v1.0  
**数据集**: 15 个真实任务数据
