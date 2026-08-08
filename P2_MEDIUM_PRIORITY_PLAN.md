# P2 中优先级 TODO 处理计划

生成时间: 2026-08-08  
执行者: Kiro AI

---

## 📋 中优先级 TODO 清单 (6 项)

### 1. Company Service - 角色和预算初始化 (2 处)

**文件**: `crates/services/src/company_service.rs`

#### TODO 1.1: AccessService.ensure_role_default_grants() (Line 18)
```rust
// TODO: Call AccessService.ensure_role_default_grants() when implemented
```

**问题**: 创建公司后未初始化默认角色权限

**分析**:
- `AccessService` 已存在 (`crates/services/src/access/access_service.rs`)
- 但没有 `ensure_role_default_grants` 方法
- Paperclip 使用 `accessService.ensureRoleDefaultGrants(company.id)`

**解决方案选项**:
1. **暂时跳过** - 等待 AccessService 实现该方法
2. **添加存根** - 在 AccessService 中添加空实现
3. **手动初始化** - 直接在 CompanyService 中创建默认角色

**推荐**: 选项 1 (暂时跳过) - 这是基础设施功能，不阻塞业务

#### TODO 1.2: BudgetService.upsert_policy() (Line 19)
```rust
// TODO: Call BudgetService.upsert_policy() when budget is set
```

**问题**: 未创建默认预算策略

**分析**:
- `BudgetService` 不存在于当前代码库
- Paperclip 有 `budgetService.upsertPolicy(...)`
- 预算功能是可选的增强特性

**解决方案选项**:
1. **暂时跳过** - 等待 BudgetService 实现
2. **添加注释** - 说明这是未来功能
3. **实现 BudgetService** - 完整迁移 (预估 4-6 小时)

**推荐**: 选项 2 (添加注释) - 预算不是核心功能

---

### 2. Project Service - 环境绑定和权限 (3 处)

**文件**: `crates/services/src/project_service.rs`

#### TODO 2.1: SecretService.normalize_env_bindings_for_persistence() (Line 20)
```rust
// TODO: Call SecretService.normalize_env_bindings_for_persistence() when implemented
```

**问题**: 环境变量绑定未规范化

**分析**:
- `SecretService` 已存在 (`crates/services/src/secret_service.rs`)
- 该方法已定义但未实现 (返回硬编码值)
- 影响环储格式

**解决方案**: 实现 `normalize_env_bindings_for_persistence` 方法

#### TODO 2.2: Optionally create workspace and sync env bindings (Line 23)
```rust
// TODO: Optionally create workspace and sync env bindings
```

**问题**: 未创建 workspace 和同步环境变量

**分析**:
- 这是可选功能 (Paperclip 中也是条件执行)
- 需要 `WorkspaceService` 集成
- 不阻塞 Project 创建

**解决方案**: 添加条件检查，暂时跳过

#### TODO 2.3: assert_mutation_allowed when AccessService is integrated (Line 164)
```rust
// TODO: assert_mutation_allowed when AccessService is integrated
```

**问题**: 权限检查被跳过

**分析**:
- `AccessService` 已存在
- 需要调用 `decide_a 影响安全性

**解决方案**: 添加权限检查 (高优先级)

---

### 3. Issue Tree Control Service - 深度和 run 计算 (3 处)

**文件**: `crates/services/src/issue_tree_control_service.rs`

#### TODO 3.1: Get active runs for affected issues (Line 215)
```rust
// TODO: Get active runs for affected issues
```

**问题**: 未获取活跃的 runs

**分析**:
- 需要查询 `runs` 表
- 影响 tree control 决策
- 不阻塞基本功能

**解决方案**: 添加 run 查询逻辑

#### TODO 3.2: calculate actual depth (Line 279)
```rust
depth: 0, // TODO: calculate actual depth
```

**问题**: Issue 层级深度硬编码为 0

**分析**:
- 需要递归计算父子关系深度
- 影响 UI 展示和层级控制
- Paperclip 有 `calculateIssueDepth` 方法

**解决方案**: 实现深度计算 (递归或 CTE)\### TODO 3.3: get active run (Line 285)
```rust
active_run_id: None, // TODO: get active run
```

**问题**: 未获取当前 issue 的 active run

**分析**:
- 与 TODO 3.1 类似
- 需要查询 issue 的当前运行状态

**解决方案**: 添加 active run 查询

---

## 🎯 处理优先级和策略

### 立即处理 (1-2 小时)

1. ✅ **Project Service TODO 2.3** - 权限检查 (30 分钟)
   - 影响安全性，应该尽快修复
   - AccessService 已存在，直接调用

2. ✅ **Issue Tree Control TODO 3.2 & 3.3** - 深度和 run (1-1.5 小时)
   - 影响功能完整性
   - 实现相对简单

### 延迟处理 (记录为技术债务)

3. **Company Service TODO 1.1 & 1.2** - 依赖未实现的服务
   - 添加清晰的注释说明等待原因
   - 不影响核心功能

4. **Project Service TODO 2.1 & 2.2** - 环境绑定
   - 可选功能
   - 需要更多上下文

---

## 📝 实施计划

### Phase 1: 权限检查修复 (30 分钟)

**文件**: `crates/services/src/project_service.rs:164`

```rust
// Before
// TODO: assert_mutation_allowed when AccessService is integrated

// After
use crate::auth::decision_engine;

let action = AuthorizationAction::ProjectUpdate { project_id };
if !decision_engine::decide_access(
    &self.pool,
    &actor,
    &action,
    Some(company_id),
).await {
    return Err(ServiceError::Forbidden("Insufficient permissions".to_string()));
}
```

### Phase 2: Issue 深度计算 (1 小时)

**文件**: `crates/services/src/issue_tree_control_service.rs:279`

```rust
// Calculate actual depth by traversing parent chain
async fn calculate_issue_depth(
    pool: &PgPool,
    issue_id: Uuid,
) -> Result<i32, ServiceError> {
    let depth: i32 = sqlx::query_scalar(
        r#"
        WITH RECURSIVE issue_tree AS (
            SELECT id, parent_id, 0 as depth
            FROM issues
            WHERE id = $1
            UNION ALL
            SELECT i.id, i.parent_id, it.depth + 1
            FROM issues i
            JOIN issue_tree it ON i.id = it.parent_id
        )
        SELECT MAX(depth) FROM issue_tree
        "#
    )
    .bind(issue_id)
    .fetch_one(pool)
    .await?;
    
    Ok(depth)
}

// Usage
depth: calculate_issue_depth(&self.pool, issue.id).await.unwrap_or(0),
```

### Phase 3: Active Run 查询 (30 分钟)

**文件**: `crates/services/src/issue_tree_control_service.rs:285`

```rust
// Get active run for issue
let active_run_id: Option<Uuid> = sqlx::query_scalar(
    "SELECT id FROM runs WHERE issue_id = $1 AND status IN ('running', 'paused') ORDER BY created_at DESC LIMIT 1"
)
.bind(issue.id)
.fetch_optional(&self.pool)
.await
.ok()
.flatten();
```

### Phase 4: Company Service 注释改进 (5 分钟)

**文件**: `crates/services/src/company_service.rs:18-19`

```rust
// Note: Default role grants initialization requires AccessService.ensure_role_default_grants()
// implementation. Tracked as tech debt - does not block company creation.

// Note: Budget policy creation requires BudgetService.upsert_policy() implementation.
// Budget enforcement is optional and does not block company creation.
```

---

## ⏱️ 总预估时间

- ✅ **立即处理**: 2-2.5 小时
  - 权限检查: 30 分钟
  - 深度计算: 1 小时
  - Active run: 30 分钟
  - 注释改进: 5 分钟

- ⏸️ **延迟处理**: 记录为技术债务 (不占用本次时间)

---

**现在开始实施 Phase 1-4？**
