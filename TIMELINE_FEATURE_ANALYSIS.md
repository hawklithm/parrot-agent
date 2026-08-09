# Timeline 功能深度分析

## 🎯 核心功能

**Timeline 是一个工作时间线（类似甘特图）功能**，用于可视化展示公司内所有工作活动的时间流和协作关系。

---

## 📊 功能概览

### 用途

Timeline 提供了一个**统一的视图**，展示：
1. **谁**在什么时间做了什么事
2. **Agent 和人类的工作流程**（agent runs、任务创建、评论、审批等）
3. **任务之间的协作关系**（委派、分配、提及）
4. **资源使用情况**（token 消耗、运行时长）

### 典型使用场景

1. **项目管理视角**
   - 查看项目中所有任务的进度
   - 识别瓶颈和空闲期
   - 追踪任务依赖关系

2. **团队协作视角**
   - 查看某个用户参与的所有任务
   - 了解团队成员的工作负载
   - 发现协作模式

3. **Agent 性能监控**
   - 查看 Agent 的工作时间分布
   - 分析 Agent 的 token 消耗
   - 识别长时间运行的任务

4. **审计和回溯**
   - 追踪任务的完整生命周期
   - 查看谁在何时做了什么决策
   - 重现问题发生的上下文

---

## 🏗️ 数据结构

### API 端点

**路由**: `GET /api/companies/:companyId/timeline`

**查询参数**:
```typescript
interface TimelineQuery {
  companyId: string;
  from?: Date;        // 时间窗口开始（默认：7天前）
  to?: Date;          // 时间窗口结束（默认：现在）
  userId?: string;    // 过滤：只看某个用户参与的任务
  goalId?: string;    // 过滤：只看某个目标下的任务
  projectId?: string; // 过滤：只看某个项目下的任务
  issueId?: string;   // 过滤：只看某个具体任务
  limit?: number;     // 分页：每页任务数（默认 200，最大 500）
  offset?: number;    // 分页：偏移量
}
```

### 返回数据结构

```typescript
interface WorkTimelineResult {
  // 1. 参与者列表（agents、users、system）
  actors: WorkTimelineActor[];
  
  // 2. 工作时间段（agent runs、任务执行）
  spans: WorkTimelineSpan[];
  
  // 3. 关键事件（创建、评论、审批）
  events: WorkTimelineEvent[];
  
  // 4. 协作关系（委派、分配、提及）
  edges: WorkTimelineEdge[];
  
  // 5. 分页信息
  pagination: {
    limit: number;
    offset: number;
    totalIssues: number;
    hasMore: boolean;
  };
  
  // 6. 时间窗口
  window: {
    from: string;      // ISO timestamp
    to: string;        // ISO timestamp
    capped: boolean;   // 是否被限制到最大窗口（31天）
  };
}
```

---

## 📐 核心数据模型

### 1. WorkTimelineActor（参与者）

**代表系统中的行为主体**：Agent、人类用户、系统

```typescript
interface WorkTimelineActor {
  id: string;           // 命名空间 ID，例如 "agent:uuid", "user:uuid", "system:system"
  type: "agent" | "user" | "system" | "plugin";
  name: string;         // 显示名称
  avatar?: string | null; // 头像 URL
}
```

**示例**：
```json
{
  "id": "agent:123e4567-e89b-12d3-a456-426614174000",
  "type": "agent",
  "name": "Chief Technology Officer",
  "avatar": "🤖"
}
```

---

### 2. WorkTimelineSpan（工作时间段）

**代表一段连续的工作时间**：Agent Run 或人类工作会话

```typescript
interface WorkTimelineSpan {
  actorId: string;              // 参与者 ID（关联到 WorkTimelineActor）
  laneHint: string | null;      // 泳道提示（用于 UI 分组）
  runId: string;                // Agent Run ID
  issueId: string;              // 关联的任务 ID
  issueIdentifier: string | null; // 任务标识符（如 "PROJ-123"）
  issueTitle: string | null;    // 任务标题
  start: string;                // ISO timestamp - 开始时间
  end: string | null;           // ISO timestamp - 结束时间（null = 进行中）
  status: string;               // 运行状态（running, completed, failed, etc.）
  retryOfRunId?: string | null; // 如果是重试，指向原始 run
  continuationAttempt?: number; // 续行尝试次数
  invocationSource?: string | null; // 调用来源
  usage?: {                     // Token 使用统计
    inputTokens: number;
    cachedInputTokens: number;
    outputTokens: number;
    totalTokens: number;
  } | null;
}
```

**示例**：
```json
{
  "actorId": "agent:123e4567",
  "laneHint": "project:abc123",
  "runId": "run-456def",
  "issueId": "issue-789ghi",
  "issueIdentifier": "PROJ-42",
  "issueTitle": "Implement user authentication",
  "start": "2026-08-08T10:
  "end": "2026-08-08T10:15:23Z",
  "status": "completed",
  "usage": {
    "inputTokens": 15000,
    "cachedInputTokens": 5000,
    "outputTokens": 3000,
    "totalTokens": 18000
  }
}
```

**可视化效果**：
```
Agent A  |=====run-1=====|        |===run-2===|
Agent B         |====run-3====|
User X                  |comment|   |approval|
         10:00  10:30   11:00   11:30  12:00
```

---

### 3. WorkTimelineEvent（关键事件）

**代表时间点上的重要行为**：创建任务、评论、审批等

```typescript
type TimelineEventKind = 
  | "created"     // 创建任务
  | "commented"   // 添加评论
  | "approved"    // 审批
  | "delegated"   // 委派
  | "assigned";   // 分配

interface WorkTimelineEvent {
  actorId: string;            // 谁做的
  kind: TimelineEventKind;    // 做了什么
  issueId: string;            // 在哪个任务上
  at: string;                 // ISO timestamp - 什么时候
}
```

**示例**：
```json
[
  {
    "actorId": "agent:ceo-001",
    "kind": "created",
    "issueId": "issue-123",
    "at": "2026-08-08T09:00:00Z"
  },
  {
    "actorId": "user:alice",
    "kind": "commented",
    "issueId": "issue-123",
    "at": "2026-08-08T10:30:00Z"
  },
  {
    "actorId": "user:bob",
    "kind": "approved",
    "issueId": "issue-123",
    "at": "2026-08-08T11:00:00Z"
  }
]
```

---

### 4. WorkTimelineEdge（协作关系）

**代表参与者之间的关系边**：谁把任务委派给谁、谁提及了谁

```typescript
type TimelineEdgeKind = 
  | "delegation"  // 委派：Agent A 将子任务委派给 Agent B
  | "assignment"  // 分配：任务被分配给某人
  | "mention";    // 提及：评论中 @ 了某人

interface WorkTimelineEdge {
  fromActorId: string;        // 发起者
  toActorId: string;          // 接收者
  issueId: string;            // 关联任务
  at: string;                 // ISO timestamp
  kind: TimelineEdgeKind;
}
```

**示例**：
```json
{
  "fromActorId": "agent:ceo-001",
  "toActorId": "agent:engineer-005",
  "issueId": "issue-456",
  "at": "2026-08-08T09:30:00Z",
  "kind": "delegation"
}
```

**可视化效果**：
```
CEO Agent ----delegation----> Engineer Agent
          \
           \--assignment----> User Alice
```

---

## 🔍 数据来源

Timeline 从多个数据源聚合数据：

### 1. Issues 表（任务）
- **用途**：获取任务的基本信息
- **字段**：id, title, status, created_at, updated_at, assignee, creator

### 2. Heartbeat Runs 表（Agent 运行记录）
- **用途**：生成 `WorkTimelineSpan`
- **字段**：id, agent_id, issue_id, started_at, finished_at, status, usage

### 3. Activity Logs 表（活动日志）
- **用途**：生成 `WorkTimelineEvent`
- **字段**：event_type, actor_id, entity_type, entity_id, created_at

### 4. Issue Comments 表（评论）
- **用途**：生成 "commented" 事件
- **字段**：id, issue_id, author_user_id, author_agent_id, created_at

### 5. Issue Thread Interactions 表（线程交互）
- **用途**：Agent 和任务的交互记录
- **字段**：id, issue_id, agent_id, created_at, resolved_at

### 6. Approvals 表（审批）
- **用途**：生成 "approved" 事件
- **字段**：id, type, decided_by_user_id, decided_at

### 7. Issue Approvals 关联表
- **用途**：链接审批和任务

---

## 🔄 数据聚合流程

### Paperclip 的实现逻辑

```typescript
async function getTimeline(query: WorkTimelineQuery): Promise<WorkTimelineResult> {
  // 第一步：收集相关的任务 ID
  const candidateIssueIds = await collectIssueIds(query, from, to);
  // 从多个表中查询：
  // - issues 表（recently touched）
  // - heartbeat_runs 表（agent 工作过的）
  // - activity_logs 表（有活动记录的）
  // - issue_comments 表（有评论的）
  // - issue_thread_interactions 表（有交互的）
  // - approvals 表（有审批的）
  
  // 第二步：加载任务详情
  const loadedIssues = await loadIssues(query, candidateIssueIds);
  
  // 第三步：应用用户过滤（如果指定了 userId）
  const userScopedIssues = await applyUserLens(query, loadedIssues, from, to);
  // - 用户创建的任务
  // - 用户被分配的任务
  // - 用户评论过的任务
  // - 用户审批过的任务
  // - 用户参与交互的任务
  // - 以及这些任务的所有子任务（递归）
  
  // 第四步：权限过滤
  const accessibleIssues = await filterReadableIssues(userScopedIssues, canReadIssue);
  
  // 第五步：分页
  const pagedIssues = accessibleIssues.slice(offset, offset + limit);
  
  // 第六步：加载每个任务的详细数据
  const [runs, comments, interactions, approvals, activityLogs] = await Promise.all([
    loadHeartbeatRuns(pagedIssues),      // Agent runs
    loadIssueComments(pagedIssues),      // 评论
    loadThreadInteractions(pagedIssues), // 线程交互
    loadApprovals(pagedIssues),          // 审批
    loadActivityLogs(pagedIssues),       // 活动日志
  ]);
  
  // 第七步：转换为 Timeline 数据结构
  const spans = runs.map(run => ({
    actorId: `agent:${run.agentId}`,
    runId: run.id,
    issueId: run.issueId,
    start: run.startedAt.toISOString(),
    end: run.finishedAt?.toISOString() ?? null,
    status: run.status,
    usage: extractUsage(run),
  }));
  
  const events = [
    ...pagedIssues.map(issue => ({
      actorId: `${issue.createdByAgentId ? 'agent' : 'user'}:${issue.createdByAgentId || issue.createdByUserId}`,
      kind: "created",
      issueId: issue.id,
      at: issue.createdAt.toISOString(),
    })),
    ...comments.map(comment => ({
      actorId: `${comment.authorAgentId ? 'agent' : 'user'}:${comment.authorAgentId || comment.authorUserId}`,
      kind: "commented",
      issueId: comment.issueId,
      at: comment.createdAt.toISOString(),
    })),
    ...approvals.map(approval => ({
      actorId: `user:${approval.decidedByUserId}`,
      kind: "approved",
      issueId: approval.issueId,
      at: approval.decidedAt.toISOString(),
    })),
  ];
  
  const edges = extractDelegationAndAssignmentEdges(pagedIssues, interactions);
  
  const actors = await loadActors(allActorIds);
  
  return { actors, spans, events, edges, pagination, window };
}
```

---

## 🎨 UI 可视化

### Timeline 页面的典型布局

```
┌─────────────────────────────────────────────────────────────────┐
│ Timeline - ABC Company                        [Filters] [Export] │
├─────────────────────────────────────────────────────────────────┤
│ Time Range: 2026-08-01 to 2026-08-08         Total: 42 issues   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│ Actor/Lane    │  Aug 1  │  Aug 3  │  Aug 5  │  Aug 7  │         │
│───────────────┼─────────┼─────────┼─────────┼─────────┤         │
│ CEO Agent     │ ████▓▓  │         │   ████  │         │         │
│               │  PROJ-1 │         │ PROJ-5  │         │         │
│───────────────┼─────────┼─────────┼─────────┼─────────┤         │
│ CTO Agent     │         │ ██████▓ │ ▓▓████  │         │         │
│               │         │ PROJ-2  │ PROJ-3  │         │         │
│───────────────┼─────────┼─────────┼─────────┼─────────┤         │
│ Engineer Bob  │         │    ██   │         │  ████   │         │
│               │         │  PROJ-4 │         │ PROJ-6  │         │
│───────────────┼─────────┼─────────┼─────────┼─────────┤         │
│ User Alice    │    ●    │         │    ●    │    ✓    │         │
│               │ comment │         │ comment │approval │         │
│───────────────┴─────────┴─────────┴─────────┴─────────┘         │
│                                                                   │
│ Legend:                                                           │
│ ████ = Running  ▓▓▓▓ = Completed  ● = Event  ━━> = Delegation   │
└─────────────────────────────────────────────────────────────────┘
```

### 交互功能

1. **悬停查看详情**
   - Span：显示 token 使用、运行时长、状态
   - Event：显示具体操作内容
   - Edge：显示协作关系

2. **点击跳转**
   - 点击任务 → 跳转到任务详情页
   - 点击 Agent → 跳转到 Agent 详情页

3. **过滤器**
   - 按时间范围过滤
   - 按用户过滤（只看我参与的）
   - 按项目过滤
   - 按目标过滤

4. **导出**
   - 导出为 CSV
   - 导出为 Gantt 图片

---

## 💡 实际使用示例

### 场景 1：查看项目进度

**需求**：PM 想看"Web Redesign"项目的进度

**请求**：
```bash
GET /api/companies/{companyId}/timeline?projectId={web-redesign-project-id}&from=2026-08-01&to=2026-08-08
```

**返回**：
- **Spans**: 所有 Agent 在这个项目上的工作时间段
- **Events**: 任务创建、评论、审批等关键事件
- **Edges**: Agent 之间的任务委派关系

**洞察**：
- Agent A 工作了 15 小时，完成了 3 个任务
- Agent B 在 Aug 3 被阻塞，等待审批
- User Alice 在 Aug 5 批准了关键设计

---

### 场景 2：追踪用户工作负载

**需求**：Team Lead 想看 Alice 这周做了什么

**请求**：
```bash
GET /api/companies/{companyId}/timeline?userId={alice-user-id}&from=2026-08-01&to=2026-08-08
```

**返回**：
- Alice 创建的任务
- Alice 评论过的任务
- Alice 审批过的任务
- Alice 被分配的任务
- **以及这些任务的所有子任务**（递归）

**洞察**：
- Alice 参与了 12 个任务
- 主要集中在前端开发相关任务
- 审批了 5 个 Agent 的工作

---

### 场景 3：分析 Agent 性能

**需求**：DevOps 想看 Agent 的 token 消耗

**请求**：
```bash
GET /api/companies/{companyId}/timeline?from=2026-08-01&to=2026-08-08
```

**分析 spans**：
```javascript
const totalTokens = spans.reduce((sum, span) => 
  sum + (span.usage?.totalTokens || 0), 0
);

const avgDuration = spans
  .filter(s => s.end)
  .map(s => new Date(s.end) - new Date(s.start))
  .reduce((sum, dur) => sum + dur, 0) / spans.length;

console.log(`Total tokens used: ${totalTokens}`);
console.log(`Average run duration: ${avgDuration / 1000 / 60} minutes`);
```

---

## 🔒 权限控制

### 公司级别访问

只有**公司成员**才能访问 timeline：
- Board Members（人类用户）
- Company 内的 Agents
- 需要通过 `company_scope:read` 权限检查

### 任务级别过滤

Timeline 会自动过滤用户**无权查看**的任务：
- 调用 `canReadIssue` 回调
- 对每个任务执行 `issue:read` 权限检查
- 只返回用户有权限的任务

---

## 🚀 Parrot-Agent 的实现状态

### 已实现

✅ **API 路由**: `GET /companies/:company_id/timeline`
✅ **Service 层**: `WorkTimelineService`
✅ **数据库查询**: 从 `activity_logs` 表加载事件
✅ **基础数据结构**: Actor, Event

### 简化实现（相比 Paperclip）

当前 Parrot-Agent 的实现是**简化版**：

```rust
async fn load_events(&self, query: &WorkTimelineQuery) -> ServiceResult<Vec<serde_json::Value>> {
    // 只查询 activity_logs 表
    let rows = sqlx::query(
        "SELECT id, event_type, actor_id, resource_type, resource_id, metadata, created_at 
         FROM activity_logs 
         WHERE company_id=$1 
         AND ($2::uuid IS NULL OR resource_id=$2) 
         AND ($3::uuid IS NULL OR actor_id=$3) 
         ORDER BY created_at DESC 
         LIMIT 500"
    )
    .bind(query.company_id)
    .bind(query.issue_id)
    .bind(query.user_id)
    .fetch_all(&self.pool)
    .await?;
    
    Ok(rows.into_iter().map(|r| serde_json::json!({
        "id": r.get::<Uuid,_>("id"),
        "eventType": r.get::<String,_>("event_type"),
        "actorId": r.get::<Uuid,_>("actor_id"),
        "resourceType": r.get::<String,_>("resource_type"),
        "resourceId": r.get::<Option<Uuid>,_>("resource_id"),
        "metadata": r.get::<serde_json::Value,_>("metadata"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")
    })).collect())
}
```

### 缺失功能

❌ **Spans**（Agent runs 的时间段可视化）
❌ **Edges**（协作关系边）
❌ **多表聚合**（只查了 activity_logs，没有查 heartbeat_runs, comments, approvals 等）
❌ **用户镜头**（userId 过滤不完整）
❌ **权限过滤**（没有 canReadIssue 回调）
❌ **分页**（没有实现 offset/limit）
❌ **Token 使用统计**

---

## 📈 完整实现的价值

### 对团队管理的价值

1. **可视化工作流**
   - 一眼看清团队在忙什么
   - 识别工作分布不均
   - 发现协作瓶颈

2. **性能优化**
   - 追踪 Agent 的 token 消耗
   - 识别长时间运行的任务
   - 优化 Agent 配置

3. **审计追溯**
   - 完整的操作历史
   - 问题根因分析
   - 合规性证明

### 对产品的价值

1. **差异化功能**
   - 类似于 GitHub Insights
   - 类似于 Jira Roadmap
   - 但聚焦于 Agent 和人类的协作

2. **用户粘性**
   - 团队依赖这个视图做决策
   - 难以迁移到其他平台

---

## ✅ 总结

### Timeline 是什么？

**一个工作活动的统一可视化视图**，展示：
- Agent 和人类的工作时间线
- 任务的创建、评论、审批等关键事件
- 参与者之间的协作关系
- Token 使用和性能指标

### 核心价值

1. **透明度**：所有工作活动一目了然
2. **问责**：清楚知道谁在何时做了什么
3. **优化**：基于数据做资源分配决策
4. **协作**：理解团队的工作流和依赖

### Parrot-Agent 的实现状态

- ✅ **基础框架已搭建**（API、Service、数据结构）
- ⚠️ **功能简化**（只查 activity_logs，缺少 spans、edges、多表聚合）
- 📝 **需要扩展**（参考 Paperclip 的完整实现）

### 下一步建议

如果你想要**完整的 Timeline 功能**，需要：
1. 添加 `heartbeat_runs` 表的查询（生成 Spans）
2. 添加 `issue_comments`、`approvals`、`issue_thread_interactions` 的查询
3. 实现协作关系边的提取（Edges）
4. 添加 token 使用统计
5. 实现用户镜头过滤
6. 添加权限过滤
7. 实现分页

这是一个**高价值**的功能，但实现复杂度较高。需要我帮你实现完整版本吗？
