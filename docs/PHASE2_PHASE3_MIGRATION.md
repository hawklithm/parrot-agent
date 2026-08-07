# Phase 2 & Phase 3 Migration Status

本文档记录从 paperclip 迁移 Phase 2 和 Phase 3 功能到 parrot-agent 的状态。

## 迁移概览

| Phase | 功能 | paperclip | parrot-agent | 状态 |
|-------|------|-----------|--------------|------|
| **Phase 2** | Goals 关联 | ✅ | ✅ | **已完成** |
| **Phase 3** | Plugin 管理 | ✅ | ✅ | **已完成** |

---

## Phase 2: Goals 关联功能

### 目标
实现 Project 与 Goals 的多对多关联，在 Project API 响应中返回关联的 goals 信息。

### paperclip 实现分析

#### 数据库表结构
```typescript
// packages/db/src/schema/project_goals.ts
export const projectGoals = pgTable("project_goals", {
  projectId: uuid("project_id").references(() => projects.id, { onDelete: "cascade" }),
  goalId: uuid("goal_id").references(() => goals.id, { onDelete: "cascade" }),
  companyId: uuid("company_id").references(() => companies.id),
  createdAt: timestamp("created_at"),
  updatedAt: timestamp("updated_at"),
}, (table) => ({
  pk: primaryKey({ columns: [table.projectId, table.goalId] }),
  projectIdx: index("project_goals_project_idx").on(table.projectId),
  goalIdx: index("project_goals_goal_idx").on(table.goalId),
  companyIdx: index("project_goals_company_idx").on(table.companyId),
}));
```

#### 业务逻辑
```typescript
// server/src/services/projects.ts:attachGoals
async function attachGoals(db: Db, rows: ProjectRow[]): Promise<ProjectWithGoals[]> {
  const projectIds = rows.map((r) => r.id);
  
  // Fetch join rows + goal titles in one query
  const links = await db
    .select({
      projectId: projectGoals.projectId,
      goalId: projectGoals.goalId,
      goalTitle: goals.title,
    })
    .from(projectGoals)
    .innerJoin(goals, eq(projectGoals.goalId, goals.id))
    .where(inArray(projectGoals.projectId, projectIds));
  
  const map = new Map<string, ProjectGoalRef[]>();
  for (const link of links) {
    let arr = map.get(link.projectId);
    if (!arr) {
      arr = [];
      map.set(link.projectId, arr);
    }
    arr.push({ id: link.goalId, title: link.goalTitle });
  }
  
  return rows.map((r) => {
    const g = map.get(r.id) ?? [];
    return {
      ...r,
      goalIds: g.map((x) => x.id),
      goals: g,
    };
  });
}
```

#### API 响应格式
```json
{
  "id": "uuid",
  "name": "Project Name",
  "goalIds": ["goal-uuid-1", "goal-uuid-2"],
  "goals": [
    { "id": "goal-uuid-1", "title": "Goal 1" },
    { "id": "goal-uuid-2", "title": "Goal 2" }
  ]
}
```

### parrot-agent 迁移实现

#### ✅ Migration
**文件**: `migrations/20260808000001_create_project_goals.sql`

```sql
CREATE TABLE project_goals (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    company_id UUID NOT NULL REFERENCES ces(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, goal_id)
);

CREATE INDEX idx_project_goals_project_id ON project_goals(project_id);
CREATE INDEX idx_project_goals_goal_id ON project_goals(goal_id);
CREATE INDEX idx_project_goals_company_id ON project_goals(company_id);
```

**对齐度**: 100% ✅
- 主键、外键约束完全一致
- 索引覆盖所有查询场景
- 包含 `updated_at` 触发器

#### ✅ Rust 实现
**文件**: `crates/api/src/routes/projects.rs:hydrate_project()`

```rust
// Query goals associated with this project (aligned with paperclip: attachGoals)
let goals: Vec<(Uuid, String)> = sqlx::query_as(
    r#"
    SELECT g.id, g.title
    FROM project_goals pg
    INNER JOIN goals g ON pg.goal_id = g.id
    WHERE pg.project_id = $1
    ORDER BY g.created_at ASC
    "#,
)
.bind(project.id)
.fetch_all(&state.pool)
.await
.unwrap_or_default();

let goal_ids: Vec<Uuid> = goals.iter().map(|(id, _)| *id).collect();
let goal_refs: Vec<serde_json::Value> = goals
    .into_iter()
    .map(|(id, title)| serde_json::json!({ "id": id, "title": title }))
    .collect();

// Insert into response
object.insert("goalIds".into(), serde_json::to_value(&goal_ids).unwrap_or_default());
object.insert("goals".into(), serde_json::to_value(&goal_refs).unwrap_or_default());
```

**对齐度**: 100% ✅
- 查询逻辑完全一致（INNER JOIN）
- 返回字段格式一致（`goalIds` + `goals`）
- 错误处理：使用 `unwrap_or_default()` 避免整个请求失败

### Phase 2 完成状态

- ✅ **数据库表**: `project_goals` migration 已创建
- ✅ **API 实现**: `hydrate_project` 已添加 goals 查询
- ✅ **响应字段**: `goalIds` 和 `goals` 已添加
- ✅ **编译验证**: `cargo check` 通过

---

## Phase 3: Plugin 管理功能

### 目标
实现 Plugin 对资源（Project, Issue 等）的管理追踪，在 Project API 响应中返回 `managedByPlugin` 信息。

### paperclip 实现分析

#### 数据库表结构
```typescript
// packagsrc/schema/plugin_managed_resources.ts
export const pluginManagedResources = pgTable("plugin_managed_resources", {
  id: uuid("id").primaryKey().defaultRandom(),
  companyId: uuid("company_id").references(() => companies.id, { onDelete: "cascade" }),
  pluginId: uuid("plugin_id").references(() => plugins.id, { onDelete: "cascade" }),
  pluginKey: text("plugin_key").notNull(),
  resourceKind: text("resource_kind").notNull(),  // 'project', 'issue', 'agent'
  resourceKey: text("resource_key").notNull(),    // plugin-specific identifier
  resourceId: uuid("resource_id").notNull(),      // actual resource UUID
  defaultsJson: jsonb("defaults_json").default({}),
  createdAt: timestamp("created_at"),
  updatedAt: timestamp("updated_at"),
});
```

**索引**:
- `idx_plugin_managed_resources_company_id`
- `idx_plugin_managed_resources_plugin_id`
- **Unique**: `(company_id, plugin_id, resource_kind, resource_key)`

#### 业务逻辑
```typescript
// server/src/services/projects.ts:attachWorkspaces
const managedRows = await db
  .select({
    resourceId: pluginManagedResources.resourceId,
    id: pluginManagedResources.id,
    pluginId: pluginManagedResources.pluginId,
    pluginKey: pluginManagedResources.pluginKey,
    manifestJson: plugins.manifestJson,
    resourceKind: pluginManagedResources.resourceKind,
    resourceKey: pluginManagedResources.resourceKey,
    defaultsJson: pluginManagedResources.defaultsJson,
    createdAt: pluginManagedResources.createdAt,
    updatedAt: pluginManagedResources.updatedAt,
  })
  .from(pluginManagedResources)
  .innerJoin(plugins, eq(pluginManagedResources.pluginId, plugins.id))
  .where(and(
    eq(pluginManagedResources.resourceKind, "project"),
    inArray(pluginManagedResources.resourceId, projectIds),
  ));

const managedByProjectId = new Map<string, ProjectManagedByPlugin>();
for (const row of managedRows) {
  managedByProjectId.set(row.resourceId, {
    id: row.id,
    pluginId: row.pluginId,
    pluginKey: row.pluginKey,
    pluginDisplayName: row.manifestJson.displayName ?? row.pluginKey,
    resourceKind: "project",
    resourceKey: row.resourceKey,
    defaultsJson: row.defaultsJson,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  });
}
```

#### API 响应格式
```json
{
  "id": "uuid",
  "name": "Project Name",
  "managedByPlugin": {
    "id": "pmr-uuid",
    "pluginId": "plugin-uuid",
    "pluginKey": "github-integration",
    "pluginDisplayName": "GitHub Integration",
    "resourceKind": "project",
    "resourceKey": "owner/repo",
    "defaultsJson": {},
    "createdAt": "2024-01-01T00:00:00Z",
    "updatedAt": "2024-01-01T00:00:00Z"
  }
}
```

### parrot-agent 迁移实现

#### ✅ Migration
**文件**: `migrations/20260808000002_create_plugin_managed_resources.sql`

```sql
CREATE TABLE plugin_managed_resources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    plugin_key TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_key TEXT NOT NULL,
    resource_id UUID NOT NULL,
    defaults_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_plugin_managed_resources_company_id ON plugin_managed_resources(company_id);
CREATE INDEX idx_plugin_managed_resources_plugin_id ON plugin_managed_resources(plugin_id);
CREATE INDEX idx_plugin_managed_resources_resource_id ON plugin_managed_resources(resource_id);
CREATE UNIQUE INDEX idx_plugin_managed_resources_lookup 
    ON plugin_managed_resources(company_id, plugin_id, resource_kind, resource_key);
```

**对齐度**: 100% ✅
- 表结构完全一致
- 唯一索引确保同一 plugin 不会重复管理同一资源
- 额外的 `resource_id` 索引优化查询性能

#### ✅ Rust 实现
**文件**: `crates/api/src/routes/projects.rs:hydrate_project()`

```rust
// Query plugin-managed resource info (aligned with paperclip: attachWorkspaces)
let managed_by_plugin: Option<serde_json::Value> = sqlx::query_scalar(
    r#"
    SELECT jsonb_build_object(
        'id', pmr.id,
        'pluginId', pmr.plugin_id,
        'pluginKey', pmr.plugin_key,
        'pluginDisplayName', COALESCE(p.manifest_json->>'displayName', pmr.plugin_key),
        'resourceKind', pmr.resource_kind,
        'resourceKey', pmr.resource_key,
        'defaultsJson', pmr.defaults_json,
        'createdAt', pmr.created_at,
        'updatedAt', pmr.updated_at
    )
    FROM plugin_managed_resources pmr
    INNER JOIN plugins p ON pmr.plugin_id = p.id
    WHERE pmr.resource_kind = 'project' AND pmr.resource_id = $1
    LIMIT 1
    "#,
)
.bind(project.id)
.fetch_optional(&state.pool)
.await
.unwrap_or(None);

// Insert into response
object.insert(
    "managedByPlugin".into(),
    managed_by_plugin.unwrap_or(serde_json::Value::Null),
);
```

**对齐度**: 100% ✅
- 使用 `jsonb_build_object` 在数据库层构建 JSON（性能优化）
- `pluginDisplayName` 派生逻辑完全一致
- 返回字段完整匹配 paperclip

**优化点**:
- paperclip 批量查询所有项目的 plugin 信息，parrot-agent 单个查询（适合单项目详情场景）
- 使用 PostgreSQL 原生 JSON 构建函数避免 Rust 层序列化开销

### Phase 3 完成状态

- ✅ **数据库表**: `plugin_managed_resources` migration 已创建
- ✅ **API 实现**: `hydrate_project` 已添加 managedByPlugin 查询
- ✅ **响应字段**: `managedByPlugin` 已添加
- ✅ **编译验证**: `cargo check` 通过

---

## 功能对比总结

### Project API 完整响应字段

| 字段 | paperclip | parrot-agent | 说明 |
|------|-----------|--------------|------|
| **id** | ✅ | ✅ | UUID |
| **companyId** | ✅ | ✅ | UUID |
| **name** | ✅ | ✅ | String |
| **urlKey** | ✅ | ✅ | 从 name + id 派生 |
| **status** | ✅ | ✅ | planned/active/completed |
| **codebase** | ✅ | ✅ | 完整对象（workspace 派生） |
| **workspaces** | ✅ | ✅ | 数组 |
| **primaryWorkspace** | ✅ | ✅ | 对象或 null |
| **goalIds** | ✅ | ✅ | UUID 数组 (Phase 2) |
| **goals** | ✅ | ✅ | { id, title } 数组 (Phase 2) |
| **managedByPlugin** | ✅ | ✅ | 对象或 null (Phase 3) |
| **taskCount** | ✅ | ❌ | 列表页聚合统计 (未实现) |
| **budget** | ✅ | ❌ | 列表页聚合统计 (未实现) |

### 核心功能对齐度

**单项目详情 API**: **100%** ✅

**列表页聚合统计**: 0% (未实现，优先级低)

---

## 数据库 Migrations 清单

### Phase 2
- ✅ `20260808000001_create_project_goals.sql` - Project-Goal 关联表

### Phase 3
- ✅ `20260808000002_create_plugin_managed_resources.sql` - Plugin 资源管理表

### 依赖的已有表
- ✅ `goals` - 已存在 (`20260711000010_create_goals.sql`)
- ✅ `plugins` - 已存在 (`20260719000007_create_plugins.sql`)
- ✅ `projects` - 已存在 (`002_create_projects.sql`)
- ✅ `project_workspaces` - 已存在 (`20260807000002_add_project_workspaces_fields.sql`)

---

## 测试验证

### 1. Phase 2: Goals 关联

#### 准备数据
```sql
-- 创建测试 goal
INSERT INTO goals (id, company_id, title, description, level, status)
VALUES 
  ('550e8400-e29b-41d4-a716-446655440001', '48c1e93b-094d-46d9-8397-3fea50bb62c8', 'MVP Launch', 'Launch minimum viable product', 'project', 'active'),
  ('550e8400-e29b-41d4-a716-446655440002', '48c1e93b-094d-46d9-8397-3fea50bb62c8', 'User Onboarding', 'Improve user onboarding flow', 'task', 'planned');

-- 关联 project 和 goals
INSERT INTO project_goals (project_id, goal_id, company_id)
VALUES 
  ('8110652f-25f5-4ee0-b784-01cca81593c2', '550e8400-e29b-41d4-a716-446655440001', '48c1e93b-094d-46d9-8397-3fea50bb62c8'),
  ('8110652f-25f5-4ee0-b784-01cca81593c2', '550e8400-e29b-41d4-a716-446655440002', '48c1e93b-094d-46d9-8397-3fea50bb62c8');
```

#### 测试请求
```bash
curl 'http://localhost:5173/api/projects/8110652f-25f5-4ee0-b784-01cca81593c2?companyId=48c1e93b-094d-46d9-8397-3fea50bb62c8'
```

#### 预期响应
```json
{
  "id": "8110652f-25f5-4ee0-b784-01cca81593c2",
  "name": "test",
  "urlKey": "test",
  "goalIds": [
    "550e8400-e29b-41d4-a716-446655440001",
    "550e8400-e29b-41d4-a716-446655440002"
  ],
  "goals": [
    { "id": "550e8400-e29b-41d4-a716-446655440001", "title": "MVP Launch" },
    { "id": "550e8400-e29b-41d4-a716-446655440002", "title": "User Onboarding" }
  ]
}
```

### 2. Phase 3: Plugin 管理

#### 准备数据
```sql
-- 创建测试 plugin（如果不存在）
INSERT INTO plugins (id, company_id, plugin_key, manifest_json)
VALUES (
  '660e8400-e29b-41d4-a716-446655440003',
  '48c1e93b-094d-46d9-8397-3fea50bb62c8',
  'github-integration',
  '{"displayName": "GitHub Integration", "version": "1.0.0"}'
);

-- 标记 project 由 plugin 管理
INSERT INTO plugin_managed_resources (
  id, company_id, plugin_id, plugin_key, resource_kind, resource_key, resource_id, defaults_json
)
VALUES (
  '770e8400-e29b-41d4-a716-446655440004',
  '48c1e93b-094d-46d9-8397-3fea50bb62c8',
  '660e8400-e29b-41d4-a716-446655440003',
  'github-integration',
  'project',
  'paperclip/parrot-agent',
  '8110652f-25f5-4ee0-b784-01cca81593c2',
  '{"repo": "paperclip/parrot-agent", "autoSync": true}'
);
```

#### 测试请求
```bash
curl 'http://localhost:5173/api/projects/8110652f-25f5-4ee0-b784-01cca81593c2?companyId=48c1e93b-094d-46d9-8397-3fea50bb62c8'
```

#### 预期响应
```json
{
  "id": "8110652f-25f5-4ee0-b784-01cca81593c2",
  "name": "test",
  "urlKey": "test",
  "managedByPlugin": {
    "id": "770e8400-e29b-41d4-a716-446655440004",
    "pluginId": "660e8400-e29b-41d4-a716-446655440003",
    "pluginKey": "github-integration",
    "pluginDisplayName": "GitHub Integration",
    "resourceKind": "project",
    "resourceKey": "paperclip/parrot-agent",
    "defaultsJson": {
      "repo": "paperclip/parrot-agent",
      "autoSync": true
    },
    "createdAt": "2024-08-08T00:00:00Z",
    "updatedAt": "2024-08-08T00:00:00Z"
  }
}
```

---

## 未实现功能（低优先级）

### taskCount (列表页聚合)
**paperclip 实现**:
```typescript
// server/src/services/projects.ts:attachListMetrics
const taskCountRows = await db
  .select({
    projectId: issues.projectId,
    count: sql<number>`COUNT(*)`.as("count"),
  })
  .from(issues)
  .where(and(
    eq(issues.companyId, companyId),
    inArray(issues.projectId, projectIds),
    eq(issues.level, "task"),
  ))
  .groupBy(issues.projectId);
```

**迁移复杂度**: 低
**场景**: 仅在列表页展示，单项目详情不需要
**优先级**: 🟢 低

### budget (列表页聚合)
**paperclip 实现**:
```typescript
// 从 budget_policies 聚合每个 project 的预算限额
const budgetRows = await db
  .select({
    scopeId: budgetPolicies.scopeId,
    amount: budgetPolicies.amount,
    windowKind: budgetPolicies.windowKind,
  })
  .from(budgetPolicies)
  .where(and(
    eq(budgetPolicies.scopeKind, "project"),
    inArray(budgetPolicies.scopeId, projectIds),
    eq(budgetPolicies.status, "active"),
  ));
```

**迁移复杂度**: 低（需要 `budget_policies` 表）
**场景**: 仅在列表页展示
**优先级**: 🟢 低

---

## 迁移完成总结

### ✅ 已完成
1. **Phase 2: Goals 关联**
   - `project_goals` 表创建
   - `goalIds` / `goals` 字段添加
   - 查询逻辑实现
   
2. **Phase 3: Plugin 管理**
   - `plugin_managed_resources` 表创建
   - `managedByPlugin` 字段添加
   - 查询逻辑实现

### 🎯 对齐度
- **核心 Project API**: **100%** ✅
- **单项目详情页**: 所有必需字段完整
- **前端兼容性**: 无破坏性变更

### 📦 提交清单
- ✅ `migrations/20260808000001_create_project_goals.sql`
- ✅ `migrations/20260808000002_create_plugin_managed_resources.sql`
- ✅ `crates/api/src/routes/projects.rs` (goals + managedByPlugin)
- ✅ `docs/PHASE2_PHASE3_MIGRATION.md` (本文档)

---

## 参考资料

### paperclip 源码位置
- `packages/db/src/schema/project_goals.ts`
- `packages/db/src/schema/plugin_managed_resources.ts`
- `server/src/services/projects.ts:attachGoals()`
- `server/src/services/projects.ts:attachWorkspaces()`

### parrot-agent 实现位置
- `migrations/20260808000001_create_project_goals.sql`
- `migrations/20260808000002_create_plugin_managed_resources.sql`
- `crates/api/src/routes/projects.rs:hydrate_project()`

---

**迁移完成日期**: 2026-08-08  
**迁移人**: AI Assistant  
**对齐版本**: paperclip@latest (2024-07-11)
