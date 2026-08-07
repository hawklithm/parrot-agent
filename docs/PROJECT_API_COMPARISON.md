# Project API 实现对比：paperclip vs parrot-agent

## 概览

本文档对比 paperclip 和 parrot-agent 的 Project API 实现，标记缺失功能并制定迁移计划。

## 数据结构对比

### paperclip Project 接口

```typescript
interface Project {
  // 基础字段
  id: string;
  companyId: string;
  urlKey: string;                    // ✅ 已迁移 (commit f02c773)
  
  // Goals 相关
  goalId: string | null;             // ❌ 缺失（deprecated）
  goalIds: string[];                 // ❌ 缺失
  goals: ProjectGoalRef[];           // ❌ 缺失
  
  // 基础信息
  name: string;
  description: string | null;
  status: ProjectStatus;
  leadAgentId: string | null;
  targetDate: string | null;
  color: string | null;
  icon: string | null;
  env: AgentEnvConfig | null;
  pauseReason: PauseReason | null;
  pausedAt: Date | null;
  executionWorkspacePolicy: ProjectExecutionWorkspacePolicy | null;
  
  // 派生字段
  codebase: ProjectCodebase;         // ❌ 缺失
  workspaces: ProjectWorkspace[];    // ✅ 已有
  primaryWorkspace: ProjectWorkspace | null;  // ✅ 已有
  managedByPlugin?: ProjectManagedByPlugin | null;  // ❌ 缺失
  
  // 列表专用字段（单个project不返回）
  taskCount?: number;                // ❌ 缺失（list only）
  budget?: ProjectBudgetSummary | null;  // ❌ 缺失（list only）
  
  // 时间戳
  archivedAt: Date | null;
  createdAt: Date;
  updatedAt: Date;
}
```

### ProjectCodebase 结构

```typescript
interface ProjectCodebase {
  workspaceId: string | null;        // 主 workspace ID
  repoUrl: string | null;            // Git 仓库 URL
  repoRef: string | null;            // Git 分支/标签/commit
  defaultRef: string | null;         // 默认分支
  repoName: string | null;           // 仓库名称（从 URL 提取）
  localFolder: string | null;        // 本地路径（workspace.cwd）
  managedFolder: string;             // 托管目录路径
  effectiveLocalFolder: string;      // 实际使用的路径
  origin: "local_folder" | "managed_checkout";  // 代码来源
}
```

## 实现流程对比

### paperclip getById 流程

```typescript
const getProjectById = async (id: string) => {
  const row = await db.select().from(projects).where(eq(projects.id, id));
  const [withGoals] = await attachGoals(db, [row]);
  const [enriched] = await attachWorkspaces(db, [withGoals]);
  return enriched;
};
```

#### attachGoals 添加：
- `urlKey` - 从 name + id 派生
- `goalIds` - goal ID 数组
- `goals` - goal 对象数组 `{id, title}[]`
- `executionWorkspacePolicy` - 解析后的策略

#### attachWorkspaces 添加：
- `workspaces` - workspace 数组（含 runtimeServices）
- `primaryWorkspace` - 主 workspace
- `codebase` - 从 primaryWorkspace 派生
- `managedByPlugin` - 插件管理信息

### parrot-agent hydrate_project 流程

```rust
async fn hydrate_project(state: &AppState, project: Project) -> Result<Value, AppError> {
  // 1. 查询 workspaces
  let workspaces = sqlx::query_as("SELECT * FROM project_workspaces WHERE project_id = $1");
  
  // 2. 找到 primary workspace
  let primary = workspaces.iter().find(|w| w.is_primary);
  
  // 3. 派生 urlKey
  let url_key = derive_project_url_key(Some(&project.name), Some(project.id));
  
  // 4. 组装 JSON
  let mut value = serde_json::to_value(project)?;
  object.insert("urlKey", url_key);
  object.insert("workspaces", workspaces);
  object.insert("primaryWorkspace", primary);
  
  return value;
}
```

#### 当前已添加：
- ✅ `urlKey`
- ✅ `workspaces`
- ✅ `primaryWorkspace`

#### 缺失功能：
- ❌ `goalIds` / `goals`
- ❌ `codebase`
- ❌ `managedByPlugin`
- ❌ `taskCount` / `budget` (list only)

## 缺失功能分析

### 1. Goals 相关字段

**优先级**: 🟡 中

**依赖**: 需要 goals 表和 project_goals 关联表

**实现工作量**: 中等
- 创建 goals 表 migration
- 创建 project_goals 关联表 migration
- 实现 goal repository
- 在 hydrate_project 中查询并添加

**前端影响**: 
- 部分组件可能访问 `project.goals` 显示关联的目标
- 如果缺失可能导致 UI 不显示目标列表，但不应崩溃

### 2. codebase 字段

**优先级**: 🔴 高

**依赖**: 仅依赖 primaryWorkspace（已有）

**实现工作量**: 小
- 从 paperclip 迁移 `deriveProjectCodebase` 函数
- 在 hydrate_project 中调用

**前端影响**: 
- 前端可能访问 `project.codebase.repoUrl`、`project.codebase.localFolder` 等
- 缺失可能导致代码库相关功能无法使用

**迁移步骤**:
```rust
// 1. 创建 codebase 生成函数
fn derive_project_codebase(
    company_id: Uuid,
    project_id: Uuid,
    primary_workspace: Option<&ProjectWorkspace>,
    fallback_workspaces: &[ProjectWorkspace],
) -> serde_json::Value {
    let workspace = primary_workspace.or_else(|| fallback_workspaces.first());
    let repo_url = workspace.and_then(|w| w.repo_url.as_ref());
    let local_folder = workspace.and_then(|w| w.cwd.as_ref());
    // ... 详见 paperclip 实现
}

// 2. 在 hydrate_project 中添加
object.insert("codebase", derive_project_codebase(...));
```

### 3. managedByPlugin 字段

**优先级**: 🟢 低

**依赖**: 需要 plugin_managed_resources 表

**实现工作量**: 大
- 创建 plugins 表
- 创建 plugin_managed_resources 表
- 实现插件系统基础设施

**前端影响**: 
- 仅影响插件管理的项目
- 大多数项目此字段为 null

### 4. taskCount / budget 字段

**优先级**: 🟢 低

**依赖**: 
- taskCount: issues 表统计
- budget: 预算策略表

**实现工作量**: 中等

**前端影响**: 
- 仅在项目列表页显示
- 单个项目详情不需要

## 迁移计划

### Phase 1: 紧急修复（立即）
- ✅ 添加 `urlKey` 字段 (commit f02c773)
- ⏳ 添加 `codebase` 字段

### Phase 2: 基础功能（本周）
- ⏳ 添加 `goalIds` / `goals` 字段
- ⏳ 创建 goals 表和关联表

### Phase 3: 可选功能（下周）
- ⏳ 添加 `managedByPlugin` 字段
- ⏳ 添加 `taskCount` / `budget` 字段

## 当前状态总结

### ✅ 已完成
- `urlKey` - URL 友好的项目标识符
- `workspaces` - 项目 workspace 列表
- `primaryWorkspace` - 主 workspace

### 🚧 进行中
- `codebase` - **下一步任务**

### ❌ 待实现
- `goalIds` / `goals` - 需要 goals 表
- `managedByPlugin` - 需要插件系统
- `taskCount` / `budget` - 需要统计查询

## 错误排查

如果前端仍然报错，检查以下字段访问：
1. `project.urlKey` - ✅ 已添加
2. `project.codebase.*` - ❌ 需要添加
3. `project.goals` - ❌ 需要添加（或前端需要容错处理）
4. `project.workspaces[0].runtimeServices` - ❌ 需要添加（或前端需要容错处理）

## 参考文件

### paperclip
- `server/src/services/projects.ts` - 完整的 project service 实现
- `packages/shared/src/types/project.ts` - Project 类型定义
- `packages/shared/src/project-url-key.ts` - URL key 工具

### parrot-agent
- `crates/api/src/routes/projects.rs` - Project API 路由
- `crates/models/src/project.rs` - Project 模型
- `crates/models/src/project_url_key.rs` - URL key 工具（已迁移）
- `crates/services/src/project_service.rs` - Project 服务
