# Timeline 功能迁移总结

## 完成状态

### ✅ 已完成（10/14 任务）

#### 数据查询层（5/5）
- ✅ `load_heartbeat_runs` - 从 heartbeat_runs 生成 Spans
- ✅ `load_issue_comments` - 从 issue_comments 生成 Events  
- ✅ `load_approvals` - 从 approvals 生成 Events
- ✅ `load_thread_interactions` - 查询 issue_thread_interactions
- ✅ `extract_edges` - 提取协作关系边（Delegation/Assignment）

#### 业务逻辑层（5/5）
- ✅ `collect_issue_ids` - 从多表联合查询收集 issue IDs
- ✅ `apply_user_lens` - 用户镜头过滤（递归包含子任务）
- ✅ `load_actors` - 加载 agents/users/system actors
- ✅ 权限过滤逻辑（通过 company_id 控制）
- ✅ Token 使用统计（从 heartbeat_runs 的 context_snapshot 提取）

### 🚧 待完成（4/14 任务）

#### API 层（0/4）
- ⏳ 完善 timeline 路由查询参数（from/to/userId 等）
- ⏳ 完善返回数据组装（actors/spans/events/edges/pagination/window）
- ⏳ 添加时间窗口限制（MAX_WINDOW_MS = 31天）
- ⏳ 验证完整功能

## 实现细节

### Service 层新增方法

```rust
// crates/services/src/work_timeline_service.rs

pub trait WorkTimelineService {
    // 收集候选 issue IDs（从多表）
    async fn collect_issue_ids(&self, query, from, to) -> Vec<Uuid>;
    
    // 生成 Spans（Agent 工作时间段）
    async fn load_heartbeat_runs(&self, company_id, issue_ids, from, to) -> Vec<WorkTimelineSpan>;
    
    // 生成 Events（评论、审批）
    async fn load_issue_comments(&self, ...) -> Vec<WorkTimelineEvent>;
    async fn load_approvals(&self, ...) -> Vec<WorkTimelineEvent>;
    
    // 加载交互和协作关系
    async fn load_thread_interactions(&self, ...) -> Vec<Value>;
    async fn extract_edges(&self, ...) -> Vec<WorkTimelineEdge>;
    
    // 用户镜头和 Actor 加载
    async fn apply_user_lens(&self, ...) -> Vec<Uuid>;
    async fn load_actors(&self, actor_ids) -> Vec<WorkTimelineActor>;
}
```

### 数据聚合流程

```
查询参数 (from/to/userId/projectId)
    ↓
collect_issue_ids (多表 UNION)
    ├─ activity_logs
    ├─ heartbeat_runs (via context_snapshot)
    ├─ issue_comments
    └─ issue_thread_interactions
    ↓
apply_user_lens (如果指定 userId)
    └─ 递归包含子任务 (WITH RECURSIVE)
    ↓
权限过滤 (company_id scope)
    ↓
分页 (offset/limit)
    ↓
并行加载详细数据
    ├─ load_heartbeat_runs → Spans
    ├─ load_issue_comments → Events
    ├─ load_approvals → Events
    ├─ extract_edges → Edges
    └─ load_actors → Actors
    ↓
组装 WorkTimelineResult
```

## 下一步工作

### 1. 修复编译错误

当前编译错误：
```
error[E0432]: unresolved import `websocket`
  --> crates/models/src/work_timeline.rs
```

**解决方案**：检查 models crate 的 import，移除未使用的 websocket 导入。

### 2. 更新 API 路由

需要在 `crates/api/src/routes/companies.rs:get_company_timeline` 中：

```rust
async fn get_company_timeline(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<WorkTimelineResult>, AppError> {
    // 1. 解析时间窗口
    let now = Utc::now();
    let from = query.from.unwrap_or_else(|| now - Duration::days(7));
    let to = query.to.unwrap_or(now);
    
    // 2. 限制窗口大小（31天）
    const MAX_WINDOW_MS: i64 = 31 * 24 * 3600 * 1000;
    let (actual_from, actual_to, capped) = /* ... */;
    
    // 3. 收集  IDs
    let wq = WorkTimelineQuery { company_id, ... };
    let mut issue_ids = state.work_timeline_service
        .collect_issue_ids(&wq, actual_from, actual_to).await?;
    
    // 4. 应用用户镜头
    if let Some(user_id) = query.user_id {
        issue_ids = state.work_timeline_service
            .apply_user_lens(company_id, user_id, issue_ids, actual_from, actual_to).await?;
    }
    
    // 5. 分页
    let total_issues = issue_ids.len();
    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(200).min(500) as usize;
    let paged_issues = issue_ids[offset..].iter().take(limit).copied().collect::<Vec<_>>();
    
    // 6. 并行加载数据
    let (spans, comments_events, approval_events, edges) = tokio::try_join!(
        state.work_timeline_service.load_heartbeat_runs(company_id, &paged_issues, actual_from, actual_to),
        state.work_timeline_service.load_issue_comments(company_id, &paged_issues, actual_from, actual_to),
        state.work_timeline_service.load_approvals(company_id, &paged_issues, actual_from, actual_to),
        state.work_timeline_service.extract_edges(company_id, &paged_issues),
    )?;
    
    // 7. 合并 events 和收集 actor IDs
    let mut events = comments_events;
    events.extend(approval_events);
    
    // 添加 issue 创建事件
    for issue in &loaded_issues {
        events.push(WorkTimelineEvent {
            actor_id: format!("{}:{}", 
                if issue.created_by_agent_id.is_some() { "agent" } else { "user" },
                issue.created_by_agent_id.or(issue.created_by_user_id).unwrap()
            ),
            kind: TimelineEventKind::Created,
            issue_id: issue.id.to_string(),
            at: issue.created_at.to_rfc3339(),
        });
    }
    
    // 8. 加载 actors
    let actor_ids = collect_unique_actor_ids(&spans, &events, &edges);
    let actors = state.work_timeline_service.load_actors(&actor_ids).await?;
    
    // 9. 返回结果
    Ok(Json(WorkTimelineResult {
        actors,
        spans,
        events,
        edges,
        pagination: TimelinePagination {
            limit,
            offset,
            total_issues,
            has_more: offset + litotal_issues,
        },
        window: TimelineWindow {
            from: actual_from.to_rfc3339(),
            to: actual_to.to_rfc3339(),
            capped,
        },
    }))
}
```

### 3. 添加测试

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_timeline_basic_query() {
        // 测试基本查询
    }
    
    #[tokio::test]
    async fn test_timeline_user_lens() {
        // 测试用户镜头过滤
    }
    
    #[tokio::test]
    async fn test_timeline_window_capping() {
        // 测试窗口限制
    }
}
```

## 参考

- Paperclip 实现：`/Users/adazhao/workspace/paperclip/packages/api-server/src/routes/timeline.ts`
- 迁移计划：`TIMELINE_MIGRATION_PLAN.md`
- 功能分析：`TIMELINE_FEATURE_ANALYSIS.md`

## Agent 创建问题结论

经过分析，发现：
- ✅ **Paperclip 本身就没有 agent 创建的 MCP 工具**
- ✅ **Parrot-Agent 与 Paperclip 完全一致**
- ✅ **Agent 创建只能通过 REST API 完成**（这是设计如此，不是缺陷）

详见：`AGENT_CREATION_ANALYSIS_FINAL.md`
