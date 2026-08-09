# Timeline 功能完整迁移计划

## 当前状态

### 已实现
✅ **数据模型**: `crates/models/src/work_timeline.rs` 已定义完整的类型
✅ **API 路由**: `GET /companies/:company_id/timeline` 已存在
✅ **Service 骨架**: `WorkTimelineService` trait 已定义
✅ **基础查询**: 只查询 `activity_logs` 表

### 缺失功能
❌ **Spans**: 从 `heartbeat_runs` 生成 Agent 工作时间段
❌ **完整 Events**: 只有 activity_logs，缺少 comments、approvals
❌ **Edges**: 协作关系边（delegation、assignment、mention）
❌ **多表聚合**: 缺少 heartbeat_runs、issue_comments、approvals、issue_thread_interactions
❌ **用户镜头**: userId 过滤不完整（应包含用户参与的所有任务及其子任务）
❌ **权限过滤**: 没有 issue 级别的权限检查
❌ **分页**: 没有实现 offset/limit
❌ **Token 统计**: 没有从 heartbeat_runs 提取 usage

## 迁移架构

### 数据流程
```
查询参数 → collect_issue_ids → 用户镜头过滤 → 权限过滤 → 分页 
                                                          ↓
返回结果 ← 组装 Timeline ← 转换数据 ← 并行加载(runs/comments/approvals/interactions)
```

### 实现层次
1. **Repository 层**: 新增查询方法（如果需要）
2. **Service 层**: `WorkTimelineService` 扩展方法
3. **API 层**: 完善路由处理和返回格式

## 迁移任务清单

### Phase 1: 数据查询层（5 tasks）
- [ ] 实现 `load_heartbeat_runs` - 查询 heartbeat_runs 生成 Spans
- [ ] 实现 `load_issue_comments` - 查询 issue_comments 生成 Events
- [ ] 实现 `load_approvals` - 查询 approvals 生成 Events  
- [ ] 实现 `load_thread_interactions` - 查询 issue_thread_interactions
- [ ] 实现 `extract_edges` - 从交互数据提取协作关系边

### Phase 2: 业务逻辑层（5 tasks）
- [ ] 增强 `collect_issue_ids` - 支持多表联合查询
- [ ] 实现 `apply_user_lens` - 用户镜头过滤（包含子任务递归）
- [ ] 实现 `filter_readable_issues` - 权限过滤
- [ ] 实现分页逻辑 - offset/limit/hasMore
- [ ] 实现 `extract_token_usage` - 从 runs 提取 token 统计

### Phase 3: API 层（4 tasks）
- [ ] 完善查询参数解析（from/to/userId/goalId/projectId）
- [ ] 实现时间窗口限制（MAX_WINDOW_MS = 31天）
- [ ] 完善返回数据组装（actors/spans/events/edges/pagination/window）
- [ ] 添加集成测试

## 数据表结构参考

### heartbeat_runs
```sql
- id, agent_id, issue_id
- started_at, finished_at
- status (queued/running/completed/failed/cancelled)
- usage (JSON: inputTokens, cachedInputTokens, outputTokens, totalTokens)
- retry_of_run_id, continuation_attempt
- invocation_source
```

### issue_comments
```sql
- id, issue_id
- author_user_id, author_agent_id
- body, created_at
```

### approvals
```sql
- id, type, status
- decided_by_user_id, decided_at
- (关联 issue_approvals 表获取 issue_id)
```

### issue_thread_interactions
```sql
- id, issue_id, agent_id
- created_at, resolved_at
```

## 实现细节

### 1. Spans 生成逻辑
```rust
WorkTimelineSpan {
    actor_id: format!("agent:{}", run.agent_id),
    lane_hint: run.issue.project_id.map(|p| format!("project:{}", p)),
    run_id: run.id,
    issue_id: run.issue_id,
    issue_identifier: run.issue.identifier,
    issue_title: run.issue.title,
    start: run.started_at.to_rfc3339(),
    end: run.finished_at.map(|t| t.to_rfc3339()),
    status: run.status,
    usage: run.usage, // JSON 直接映射
}
```

### 2. Events 生成逻辑
```rust
// From issue creation
Event { actor_id, kind: "created", issue_id, at: issue.created_at }

// From comments
Event { actor_id, kind: "commented", issue_id, at: comment.created_at }

// From approvals
Event { actor_id, kind: "approved", issue_id, at: approval.decided_at }
```

### 3. Edges 生成逻辑
```rust
// From issue parent-child relationship
Edge { from: parent.creator, to: child.assignee, kind: "delegation" }

// From issue assignment
Edge { from: issue.created_by, to: issue.assignee, kind: "assignment" }

// From thread interactions
Edge { from: issue.owner, to: interaction.agent, kind: "delegation" }
```

### 4. 用户镜头过滤
```sql
-- 用户参与的任务
WHERE created_by_user_id = $userId
   OR assignee_user_id = $userId
   OR id IN (SELECT issue_id FROM issue_comments WHERE author_user_id = $userId)
   OR id IN (SELECT ia.issue_id FROM issue_approvals ia 
             JOIN approvals a ON a.id = ia.approval_id 
             WHERE a.decided_by_user_id = $userId)
   
-- 递归子任务
WITH RECURSIVE task_tree AS (
  SELECT id FROM issues WHERE <user_filter>
  UNION
  SELECT i.id FROM issues i JOIN task_tree t ON i.parent_id = t.id
)
```

### 5. 权限过滤
- Company 级别: 所有查询都 WHERE company_id = $company_id
- Issue 级别: 后续可接入 access control service

## 时间窗口限制
```rust
const MAX_WINDOW_MS: i64 = 31 * 24 * 3600 * 1000; // 31 days

let requested_duration = (to - from).num_milliseconds();
let (actual_from, actual_to, capped) = if requested_duration > MAX_WINDOW_MS {
    (to - Duration::milliseconds(MAX_WINDOW_MS), to, true)
} else {
    (from, to, false)
};
```

## 错误处理
- 400: 无效的时间范围、分页参数
- 403: 无权访问该 company
- 500: 数据库查询失败

## 测试策略
1. 单元测试: 各个数据加载方法
2. 集成测试: 完整 timeline 查询流程
3. 性能测试: 大数据量下的查询性能

## 迁移顺序
1. 先实现数据查询层 - 确保能正确加载各表数据
2. 再实现业务逻辑层 - 确保过滤和聚合正确
3. 最后完善 API 层 - 确保返回格式符合前端需求
4. 添加测试 - 确保功能稳定

## 参考
- Paperclip 实现: `/Users/adazhao/w/paperclip/packages/api-server/src/routes/timeline.ts`
- 当前实现: `crates/services/src/work_timeline_service.rs`
- API 路由: `crates/api/src/routes/companies.rs:610-692`
