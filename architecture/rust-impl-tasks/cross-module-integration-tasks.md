# 跨模块集成与协调层 - Rust 实现任务拆解

> 补充跨模块协调、事件驱动、长事务编排等系统性基础设施
> 版本: 1.0
> 日期: 2026/07/11

---

## 1. 统一活动日志服务 实现任务

### 阶段一：基础架构

- [ ] **定义 ActivityLogService 统一接口**
  - 定义 `ActivityLogService` trait（log_activity, query_activities, get_activity_feed）
  - 定义 `Activity` 结构体（id, company_id, actor_type, actor_id, action, resource_type, resource_id, metadata, created_at）
  - 定义 `ActivityAction` 枚举（created / updated / deleted / checked_out / released / approved / rejected / executed / failed）
  - 定义 `ResourceType` 枚举（agent / issue / case / routine / goal / approval / environment / workspace）

- [ ] **定义活动日志标准格式**
  - 定义 `ActivityMetadata` 结构体（变更前后对比、关联资源、上下文信息）
  - 定义活动分类标签（security / audit / operational / user_action）
  - 定义活动等级（info / warning / error / critical）

- [ ] **实现 ActivityLogRepository**
  - 定义 `ActivityLogRepository` trait（insert, query_by_company, query_by_resource, query_by_actor）
  - 实现索引策略（company_id + created_at, resource_type + resource_id + created_at）
  - 实现分页查询与时间范围过滤

### 阶段二：核心功能

- [ ] **实现 ActivityLogService 核心逻辑**
  - 实现 `log_activity()` 插入活动记录
  - 实现 `query_activities()` 多维度查询（按公司、资源、Actor、时间范围）
  - 实现 `get_activity_feed()` 生成用户活动流（aggregated view）

- [ ] **实现各模块集成点**
  - Agent 模块：`agent.created`, `agent.updated`, `agent.terminated`, `agent.hired`
  - Issue 模块：`issue.created`, `issue.checked_out`, `issue.released`, `issue.commented`
  - Case 模块：`case.created`, `case.updated`, `case.issue_linked`
  - Routine 模块：`routine.created`, `routine.triggered`, `routine.run_completed`
  - Environment 模块：`environment.created`, `lease.acquired`, `lease.released`
  - Approval 模块：`approval.requested`, `approval.approved`, `approval.rejected`

- [ ] **实现活动日志聚合与统计**
  - 实现按时间段聚合（hourly / daily / weekly）
  - 实现按资源类型统计（各模块活动数量）
  - 实现热点资源识别（频繁操作的资源）

### 阶段三：高级特性

- [ ] **实现活动日志敏感信息过滤**
  - 实现 metadata 字段的自动脱敏（移除 API keys, tokens, passwords）
  - 实现基于角色的日志可见性控制（Board 用户可见所有，普通用户仅可见自己相关）
  - 实现审计日志的不可篡改性保证（append-only, hash chain）

- [ ] **实现活动日志导出与归档**
  - 实现导出为 JSON / CSV 格式
  - 实现历史日志归档策略（30天后归档到冷存储）
  - 实现归档日志的查询接口（slower but complete）

---

## 2. 事件总线（EventBus）实现任务

### 阶段一：基础架构

- [x] **定义 EventBus 核心类型**
  - 定义 `Event` trait（event_type, payload, metadata, timestamp）
  - 定义 `EventHandler` trait（handle 异步方法，返回 Result）
  - 定义 `EventBus` trait（publish, subscribe, unsubscribe）

- [x] **定义标准事件类型**
  - 定义 `IssueEvent` 枚举（Created, CheckedOut, Released, StatusChanged, Completed）
  - 定义 `ApprovalEvent` 枚举（Requested, Approved, Rejected, RevisionRequested）
  - 定义 `RoutineEvent` 枚举（Triggered, RunStarted, RunCompleted, RunFailed）
  - 定义 `AgentEvent` 枚举（Hired, StatusChanged, Terminated）
  - 定义 `EnvironmentEvent` 枚举（LeaseAcquired, LeaseReleased, LeaseExpired）

- [x] **实现*
  - 实现 `InMemoryEventBus` 使用 tokio::sync::broadcast 或 DashMap
  - 实现 `publish()` 发布事件到所有订阅者
  - 实现 `subscribe()` 注册事件处理器（按 event_type 分类）

### 阶段二：核心功能

- [ ] **实现跨模块事件监听器**
  - **Issue 完成 → Goal 进度更新监听器**
    - 监听 `IssueEvent::Completed`
    - 查询 Issue 关联的 Goal
    - 调用 GoalService::recalculate_progress()
  
  - **Approval 批准 → Issue 解除阻塞监听器**
    - 监听 `ApprovalEvent::Approved`
    - 查询 Approval 关联的 Issue
    - 调用 IssueService::update_status(blocked -> in_progress)
  
  - **Routine 触发 → Issue 创建监听器**
    - 监听 `RoutineEvent::Triggered`
    - 调用 IssueService::create() 创建关联 Issue
    - 调用 IssueService::checkout()

  - **Environment Lease 过期 → Workspace 清理监听器**
    - 监听 `EnvironmentEvent::LeaseExpired`
    - 调用 WorkspaceService::cleanup() 清理工作空间
    - 记录活动日志

- [ ] **实现事件持久化（可选）**
  - 实现 `PersistentEventBus` 将事件写入数据库 events 表
  - 实现事件重放机制（从事件流重建系统状态）
  - 实现事件溯源查询（按时间、资源、事件类型查询历史事件）

### 阶段三：高级特性

- [ ] **实现事件总线性能优化**
  - 实现事件批处理（积累 N 个事件后批量发布）
  - 实现订阅者优先级队列（critical handlers 先执行）
  - 实现慢订阅者检测与隔离（超时的 handler 不阻塞其他订阅者）

- [ ] **实现死信队列（DLQ）**
  - 实现事件处理失败的重试机制（exponential backoff, max 3 retries）
  - 实现重试失败后移入死信队列
  - 实现死信队列的监控与人工干预接口

---

## 3. Saga 编排器 实现任务

### 阶段一：基础架构

- [x] **定义 Saga 核心类型**
  - 定义 `Saga` trait（execute, compensate, status）
  - 定义 `SagaStep` 结构体（step_name, action, compensation, timeout）
  - 定义 `SagaStatus` 枚举（pending / in_progress / compensating / succeeded / failed / compensated）
  - 定义 `SagaOrchestrator` trait（start_saga, get_saga_status, retry_saga）

- [x] **定义 Saga 实例存储**
  - 定义 `SagaInstance` 结构体（id, saga_name, status, current_step, context, started_at, completed_at）
  - 定义 `SagaStepExecution` 结构体（saga_id, step_name, status, started_at, completed_at, result, compensation_result）
  - 实现 saga_instances 和 saga_step_executions 表 migration

- [x] **实现 SagaOrchestrator 基础架构**
  - 实现 `SagaOrchestrator` 结构体，持有 SagaRepository 和 EventBus
  - 实现 `start_saga()` 创建 SagaInstance 并开始执行第一步
  - 实现 `execute_step()` 执行单个步骤，记录结果
  - 实现 `compensate_step()` 执行补偿逻辑

### 阶段二：核心功能

- [ ] **实现 Agent 雇佣 Saga**
  - **Step 1**: 创建 Agent 记录
  - **Step 2**: 创建 Approval 记录（如需**Step 3**: 物化指令集（materialize_instructions_bundle）
  - **Step 4**: 创建 Budget Policy 记录
  - **补偿逻辑**：删除 Agent、删除 Approval、清理指令集、删除 Policy

- [ ] **实现 Issue 执行 Saga**
  - **Step 1**: Checkout Issue（更新状态）
  - **Step 2**: 获取 Environment Lease
  - **Step 3**: 创建 Execution Workspace
  - **Step 4**: 启动 Runtime Services
  - **Step 5**: 唤醒 Agent（wakeup）
  - **补偿逻辑**：释放租约、清理工作空间、停止服务、回退 Issue 状态

- [ ] **实现 Routine 触发 Saga**
  - **Step 1**: 创建 RoutineRun 记录
  - **Step 2**: 创建关联 Issue
  - **Step 3**: Checkout Issue 并分配给 Agent
  - **Step 4**: 获取 Environment Lease（可选）
  - **Step 5**: 唤醒 Agent
  - **补偿逻辑**：删除 Issue、释放 RoutineRun、释放租## 阶段三：高级特性

- [ ] **实现 Saga 状态持久化与恢复**
  - 实现每个步骤执行后的状态持久化（checkpoint）
  - 实现服务重启后的 Saga 恢复（从数据库加载未完成的 Saga）
  - 实现 Saga 超时检测与自动补偿（step 超过 timeout 自动触发 compensate）

- [ ] **实现 Saga 可视化与监控**
  - 实现 `GET /sagas/:id` 查询 Saga 执行状态
  - 实现 `GET /sagas/:id/steps` 查询步骤执行历史
  - 实现 Saga 失败告警（连续失败 N 次发送通知）

---

## 4. 状态漂移检测作业 实现任务

### 阶段一：基础架构

- [ ] **定义状态一致性检查器接口**
  - 定义 `ConsistencyChecker` trait（check, fix, report）
  - 定义 `InconsistencyReport` 结构体（resource_type, resource_id, expected_state, actual_state, detected_at）
  - 定义 `FixStrategy` 枚举（auto_fix / manual_review / alert_only）

- [ ] **实现检查器注册表**
  - 实现 `CheckerRegistry` 注册所有一致性检查器
  - 实现 `schedule_checks()` 后台任务调度器（每小时运行一次）
  - 实现检查结果的持久化（consistency_check_results 表）

### 阶段二：核心功能

- [ ] **实现 Issue 状态一致性检查器**
  - **检查 1**: Issue.status = in_progress 但无 active RoutineRun 或 HeartbeatRun
    - 预期：有活跃运行
    - 修复：自动 release Issue，状态改为 todo
  
  - **检查 2**: Issue.status = blocked 但关联的 Approval 已 approved
    - 预期：Issue 应解除阻塞
    - 修复：更新 Issue.status = in_progress
  
  - **检查 3**: Issue.checkout_run_id 指向不存在的 Run
    - 预期：外键有效
    - 修复：清除 checkout_run_id

- [ ] **实现 Environment Lease 一致性检查器**
  - **检查 1**: environment_leases.status = active 但 last_used_at 超时（> heartbeat_interval * 3）
    - 预期：租约应过期
    - 修复：自动释放租约，更新 status = expired
  
  - **检查 2**: environments.status = in_use 但无 active lease
    - 预期：环境应为 active
    - 修复：更新 environments.status = active
  
  - **检查 3**: 租约关联的 ExecutionWorkspace 已删除
    - 预期：工作空间存在
    - 修复：释放租约并记录异常

- [ ] **实现 Agent 状态一致性检查器**
  - **检查 1**: Agent.status = running 但 lastHeartbeatAt 超时（> 5 分钟）
    - 预期：有活跃心跳
    - 修复：更新 status = paused，记录心跳超时
  
  - **检查 2**: Agent.reportsTo 指向已 terminated 的 Agent
    - 预期：上级 Agent 有效
    - 修复：清除 reportsTo 或重新分配上级

### 阶段三：高级特性

- [ ] **实现自动修复策略配置**
  - 实现 `FixStrategyConfig` 按不一致类型配置修复策略
  - 实现 auto_fix 白名单（安全的自动修复项）
  - 实现 manual_review 需人工确认的场景

- [ ] **实现一致性检查报告与告警**
  - 实现 `GET /admin/consistency-reports` 查询不一致记录
  - 实现不一致数量超过阈值时发送告警
  - 实现一致性健康评分（100% - 不一致比例）

---

## 5. 全局错误恢复策略 实现任务

### 阶段一：基础架构

- [ ] **定义统一错误类型体系**
  - 定义 `AppError` 枚举（包含所有模块的错误类型）
  - 定义 `ErrorCategory` 枚举（transient / permanent / user_error / system_error）
  - 定义 `ErrorSeverity` 枚举（info / warning / error / critical）
  - 定义错误码标准（模块前缀 + 错误编号，如 ISSUE-001, ENV-042）

- [ ] **定义重试策略配置**
  - 定义 `RetryPolicy` 结构体（max_attempts, backoff_strategy, timeout）
  - 定义 `BackoffStrategy` 枚举（fixed / exponential / fibonacci）
  - 定义 `RetryableError` trait（is_retryable 方法）

- [ ] **实现重试装饰器（Retry Decorator）**
  - 实现 `with_retry()` 高阶函数，包装任意异步函数
  - 实现 exponential backoff 算法（base: 1s, multiplier: 2, max: 60s）
  - 实现重试计数器与日志记录

### 阶段二：核心功能

- [ ] **实现关键操作的重试策略**
  - **数据库操作**: max_attempts=3, exponential backoff
    - 可重试：连接超时、死锁、临时不可用
    - 不可重试：唯一键冲突、外键约束违反
  
  - **外部 API 调用**: max_attempts=5, exponential backoff
    - 可重试：网络超时、5xx 服务器错误、429 限流
    - 不可重试：4xx 客户端错误（除 429）
  
  - **Environment 操作**: max_attempts=3, fixed backoff (5s)
    - 可重试：环境暂时不可用、租约获取竞争
    - 不可重试：环境不存在、配置错误

- [ ] **实现熔断器（Circuit Breaker）**
  - 实现 `CircuitBreaker` 结构体（状态：closed / open / half_open）
  - 当连续失败 N 次（默认 5）时打开熔断器（拒绝后续请求）
  - 打开状态持续 timeout（默认 60s）后进入 half_open
  - half_open 状态下成功 1 次则关闭熔断器，失败则重新打开

- [ ] **实现降级策略（Fallback）**
  - **数据库降级**: 主库失败 -> 只读副本
  - **环境获取降级**: 专用环境失败 -> 共享环境池
  - **通知发送降级**: 实时推送失败 -> 异步队列
  - 定义 `Fallback` trait（fallback 方法返回降级结果）

### 阶段三：高级特性

- [ ] **实现错误追踪与关联**
  - 实现 `ErrorTrace` 结构体记录错误链（root cause + wrapped errors）
  - 实现错误关联 ID（correlation_id）跨服务追踪
  - 实现错误聚合分析（相同错误类型的频率统计）

- [ ] **实现错误恢复监控与告警**
  - 实现 `GET /admin/error-stats` 错误统计端点（按类型、模块、时间段）
  - 实现错误率阈值告警（错误率 > 5% 发送通知）
  - 实现熔断器状态监控（熔断器打开时告警）

---

## 6. 后台调度器统一管理 实现任务

### 阶段一：基础架构

- [ ] **定义 ScheduledJob 核心类型**
  - 定义 `ScheduledJob` trait（job_name, schedule, execute, health_check）
  - 定义 `JobSchedule` 枚举（interval_seconds / cron_expression / on_event）
  - 定义 `JobStatus` 枚举（idle / running / failed / disabled）
  - 定义 `JobExecutionRecord` 结构体（job_name, started_at, completed_at, status, error_message）

- [ ] **实现 JobRegistry 注册表**
  - 实现 `JobRegistry` 注册所有后台任务
  - 实现 `register_job()` 注册任务到调度器
  - 实现 `unregister_job()` 取消注册

- [ ] **实现 JobScheduler 调度器**
  - 实现 `JobScheduler` 使用 tokio::time 管理所有定时任务
  - 实现 `start()` 启动所有已注册任务
  - 实现 `stop()` 优雅停止所有任务
  - 实现 `pause_job()` / `resume_job()` 暂停/恢复单个任务

### 阶段二：核心功能

- [ ] **注册所有后台任务到统一调度器**
  - Monitor 定时检查器（每分钟）
  - 租约过期扫描器（每分钟）
  - 僵尸租约清理器（每 5 分钟）
  - 环境健康探测器（每 5 分钟）
  - 工作空间空闲回收器（每 10 分钟）
  - Routine Cron 触发器（每分钟）
  - 成本聚合作业（每小时）
  - 状态一致性检查器（每小时）

- [ ] **实现任务执行日志与监控**
  - 实现每次任务执行的记录（job_execution_records 表）
  - 实现任务执行时间统计（平均、P50、P95、P99）
  - 实现任务失败告警（连续失败 3 次发送通知）

### 阶段三：高级特性

- [ ] **实现调度器健康检查与自愈**
  - 实现调度器心跳检测（每 30s 更新 scheduler_heartbeat 表）
  - 实现死锁检测（任务执行时间超过 timeout 自动终止）
  - 实现任务自动重启（失败后延迟重启，max 3 次）

- [ ] **实现调度器管理 API**
  - 实现 `GET /admin/scheduler/jobs` 列出所有注册任务
  - 实现 `POST /admin/scheduler/jobs/:jobName/pause` 暂停任务
  - 实现 `POST /admin/scheduler/jobs/:jobName/resume` 恢复任务
  - 实现 `POST /admin/scheduler/jobs/:jobName/trigger` 手动触发任务

---

## 依赖顺序总览

```
阶段一（基础架构）推荐实现顺序:

  1. 统一活动日志服务 (所有模块依赖)
  2. 事件总线 (跨模块通信基础)
  3. 后台调度器统一管理 (调度所有后台任务)
  4. 全局错误恢复策略 (错误处理基础)
  5. Saga 编排器 (长事务协调)
  6. 状态漂移检测作业 (依赖事件总线)

阶段二（核心功能）推荐实现顺序:

  1. 统一活动日志服务 -> 各模块集成点
  2. 事件总线 -> 跨模块事件监听器
  3. 后台调度器 -> 注册所有任务
  4. 全局错误恢复策略 -> 关键操作重试
  5. Saga 编排器 -> Agent 雇佣、Issue 执行、Routine 触发 Saga
  6. 状态漂移检测 -> Issue、Lease、Agent 一致性检查器

阶段三（高级特性）推荐实现顺序:

  1. 统一活动日志 -> 敏感信息过滤、导出归档
  2. 事件总线 -> 性能优化、死信队列
  3. Saga 编排器 -> 状态恢复、可视化监控
  4. 状态漂移检测 -> 自动修复策略、健康评分
  5. 全局错误恢复 -> 错误追踪、监控告警
  6. 后台调度器 -> 健康检查、管理 API
```

---

## Rust 技术选型建议

| 领域 | 推荐选型 | 说明 |
|------|----------|------|
| 事件总线 | tokio::sync::broadcast + DashMap | 高性能内存事件分发 |
| Saga ostgreSQL | 持久化状态机 |
| 后台调度 | tokio::time + tokio-cron-scheduler | 定时任务调度 |
| 重试机制 | tokio-retry crate | Exponential backoff 实现 |
| 熔断器 | failsafe-rs crate | Circuit breaker 模式 |
| 错误处理 | thiserror + anyhow | 统一错误类型 |
| 一致性检查 | 自定义 + 后台任务 | 定期扫描 + 修复 |
| 活动日志 | PostgreSQL + 索引优化 | 高吞吐写入 |

---

**总任务数**: ~90 个  
**覆盖模块**: 跨模块协调 + 系统基础设施  
**优先级**: P1 - 所有模块依赖的基础设施
