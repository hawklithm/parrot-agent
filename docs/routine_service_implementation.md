# Routine Service 功能实现总结

## 已实现功能

### P0 - 核心调度功能 ✅ (100%)

| 功能 | Paperclip | Rust 实现 | 状态 |
|------|----------|----------|------|
| 调度触发器 | `tickScheduledTriggers` | `RoutineCronTrigger::execute` | ✅ 已完成 |
| 分发 routine run | `dispatchRoutineRun` | `RoutineExecutionService::dispatch_routine_run` | ✅ 已完成 |

### P1 - 基础 API 功能 ✅ (100%)

| 功能 | Paperclip | Rust 实现 | 状态 |
|------|----------|----------|------|
| 同步 issue 状态 | `syncRunStatusForIssue` | `RoutineExecutionService::sync_run_status_for_issue` | ✅ 已完成 |
| 列出运行历史 | `listRuns` | `RoutineExecutionService::list_runs` | ✅ 已完成 |
| 手动触发 routine | `runRoutine` | `RoutineExecutionService::run_routine` | ✅ 已完成 |

## 新增方法详情

### 1. `sync_run_status_for_issue`
**功能**: 当 issue 状态变化时，自动同步 routine run 状态

**逻辑**:
- Issue 状态为 `done` → routine run 状态设为 `succeeded`
- Issue 状态为 `blocked` 或 `cancelled` → routine run 状态设为 `failed`

**源码位置**: `crates/services/src/routine_execution_service.rs:535-605`

**对应 Paperclip**: `server/src/services/routines.ts:2818-2844`

---

### 2. `list_runs`
**功能**: 列出 routine 的运行历史，包含关联的 issue 和 trigger 信息

**参数**:
- `routine_id: Uuid` - Routine ID
- `limit: Option<i32>` - 最大返回数量（默认 50，最大 200）

**返回**: `Vec<RoutineRunSummary>` - 包含运行详情、关联 issue、trigger 信息

**源码位置**: `crates/services/src/routine_execution_service.rs:609-648`

**对应 Paperclip**: `server/src/services/routines.ts:2660-2732`

---

### 3. `run_routine`
**功能**: 手动触发 routine 执行

**参数**:
- `routine_id: Uuid` - Routine ID
- `variables: Option<HashMap<String, String>>` - 变量替换
- `actor_user_id: Option<Uuid>` - 执行用户 ID
- `actor_agent_id: Option<Uuid>` - 执行 agent ID

**返回**: `RoutineRun` - 创建的 routine run 记录

**验证**:
- Routine 存在
- Routine 状态不是 `archived`

**源码位置**: `crates/services/src/routine_execution_service.rs:652-690`

**对应 Paperclip**: `server/src/services/routines.ts:2517-2543`

---

## 新增数据结构

### `RoutineRunSummary`
运行历史的详细信息，用于列表展示

```rust
pub struct RoutineRunSummary {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: String,              // "schedule" | "manual" | "api" | "webhook"
    pub status: String,              // "received" | "queued" | "succeeded" | "failed"...
    pub linked_issue_id: Option<Uuid>,
    pub failure_reason: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub trigger: Option<TriggerInfo>,
    pub linked_issue: Option<LinkedIssueInfo>,
}
```

### `TriggerInfo`
触发器信息

```rust
pub struct TriggerInfo {
    pub id: Uuid,
    pub kind: String,           // "schedule" | "webhook" | "manual"
    pub label: Option<String>,
}
```

### `kedIssueInfo`
关联的 issue 信息

```rust
pub struct LinkedIssueInfo {
    pub id: Uuid,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub updated_at: DateTime<Utc>,
}
```

---

## 关键技术细节

### 1. PostgreSQL ENUM 类型处理
数据库中的 ENUM 类型需要在 SQL 查询中显式转换为 `TEXT`：

```sql
-- ✅ 正确
SELECT status::TEXT as "status!" FROM issues

-- ❌ 错误（会导致 sqlx 编译错误）
SELECT status FROM issues
```

**相关 ENUM 类型**:
- `issue_status` - issue 状态
- `issue_priority` - issue 优先级
- `run_source` - routine run 来源
- `run_status` - routine run 状态
- `trigger_kind` - trigger 类型
- `routine_status` - routine 状态

### 2. Run Status 映射
Paperclip 和 Rust 的状态值对应关系：

| Paperclip | Rust Database | 说明 |
|-----------|---------------|------|
| `completed` | `succeeded` | Issue 完成 → run 成功 |
| `failed` | `failed` | Issue blocked/cancelled → run 失败 |

### 3. RoutineRun 结构体字段
完整的 `RoutineRun` 字段：

```rust
pub struct RoutineRun {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub linked_issue_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

---

## 未实现功能（P2/P3）

### P2 - 中优先级（管理功能）
- ⏳ `list` - 列出 routines
- ⏳ `getDetail` - 获取 routine 详情
- ⏳ `create` - 创建 routine
- ⏳ `update` - 更新 routine
- ⏳ `createTrigger` - 创建 trigger
- ⏳ `updateTrigger` - 更新 trigger
- ⏳ `deleteTrigger` - 删除 trigger

### P3 - 低优先级（高级功能）
- ⏳ `firePublicTrigger` - 触发公共 webhook
- ⏳ `rotateTriggerSecret` - 轮换 webhook secret
- ⏳ `listRevisions` - 列出修订历史
- ⏳ `restoreRevision` - 恢复历史版本
- ⏳ `runPipelineStageEntryRoutine` - Pipeline stage 触发
- ⏳ `getDescriptionDocument` - 获取描述文档

---

## 验证建议

### 1. 单元测试
```bash
cargo test --package services routine_execution
```

### 2. 手动验证
在数据库中创建测试数据后：

```sql
-- 1. 创建 routine run
INSERT INTO routine_runs (company_id, routine_id, source, status, triggered_at)
VALUES (...);

-- 2. 创建关联 issue
INSERT INTO issues (company_id, title, status, origin_kind, origin_run_id)
VALUES (..., 'Test Issue', 'done', 'routine_execution', <run_id>);

-- 3. 调用 sync_run_status_for_issue
-- 应该将 routine run 状态更新为 'succeeded'
```

### 3. API 测试
需要在 API 层添加对应的路由：

```rust
// 示例路由
// GET /api/routines/:routine_id/runs
// POST /api/routines/:routine_id/run
// POST /api/internal/routines/sync-run-status/:issue_id
```

---

## 下一步工作

### 优先级排序
1. **P2 - CRUD 功能** - 完善 routine 和 trigger 的基本管理
2. **P3 - Webhook** - 支持外部系统触发
3. **P3 - 版本控制** - Routine 配置的版本管理
4. **P3 - Pipeline 集成** - 与 pipeli
### API 层实现
当前实现仅在 Service 层，需要：
1. 在 `crates/api/src/routes/` 中添加对应的 HTTP 路由
2. 添加请求/响应的数据结构
3. 实现权限验证和参数校验

---

## 编译验证

```bash
# 检查编译
cargo check --package services

# 构建服务
cargo build --release

# 运行服务器
cargo run --bin parrot-server
```

**当前状态**: ✅ 所有新增功能编译通过，无错误

---

## 参考文档
- [Job Scheduler 验证文档](./job_scheduler_verification.md)
- [Paperclip Routines Service](~/workspace/paperclip/server/src/services/routines.ts)
- [PostgreSQL ENUM 文档](https://www.postgresql.org/docs/current/datatype-enum.html)
