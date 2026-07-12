# Routine/Goal 自动化模块 - Rust 实现任务拆解

> 基于 [backend/routine-goal.md](../backend/routine-goal.md) 架构分析文档，拆解为 Rust 版本实现任务。
> 版本: 1.0
> 日期: 2026/07/11

---

## 1. 数据模型层 实现任务

### 阶段一：基础架构

- [ ] **定义 Routine 核心枚举类型**
  - 定义 `RoutineStatus` 枚举（active / paused / draft）
  - 定义 `ConcurrencyPolicy` 枚举（coalesce_if_active / parallel / skip_if_active）
  - 定义 `CatchUpPolicy` 枚举（run_missed / skip_missed）

- [ ] **定义 Routine 结构体**
  - 定义 `Routine` 结构体，映射核心字段（id, company_id, project_id, goal_id, parent_issue_id, title, description, assignee_agent_id, priority, status, concurrency_policy, catch_up_policy, variables, env, latest_revision_id, latest_revision_number, responsible_user_id, last_triggered_at, last_enqueued_at, created_at, updated_at）
  - 定义 `CreateRoutineInput` 结构体（company_id, title, description, project_id, goal_id, assignee_agent_id, priority, status, concurrency_policy, catch_up_policy, variables, env, responsible_user_id）
  - 定义 `UpdateRoutineInput` 结构体（可选字段更新：title, description, status, priority, assignee_agent_id, concurrency_policy, catch_up_policy, variables, env）

- [ ] **定义 RoutineVariable 与 EnvConfig 类型**
  - 定义 `RoutineVariable` 结构体（name, label, type, default_value, required, options）
  - 定义 `RoutineVariableType` 枚举（text / number / boolean / select / secret）
  - 定义 `RoutineEnvConfig` 结构体（环境配置映射）

- [ ] **定义 RoutineTrigger 枚举与结构体**
  - 定义 `TriggerKind` 枚举（schedule / webhook / manual）
  - 定义 `RoutineTrigger` 结构体（id, company_id, routine_id, kind, label, enabled, cron_expression, timezone, next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec, last_rotated_at, last_result, created_at, updated_at）
  - 定义 `CreateTriggerInput` / `UpdateTriggerInput` 结构体

- [ ] **定义 RoutineRevision 结构体**
  - 定义 `RoutineRevision` 结构体（id, company_id, routine_id, revision_number, title, description, snapshot, change_summary, restored_from_revision_id, created_by_agent_id, created_by_user_id, created_at）
  - 定义 `RoutineRevisionSnapshotV1` 结构体（version, routine 快照, triggers 快照）
  - 为 `RoutineRevisionSnapshotV1` 实现 Serialize/Deserialize

- [ ] **定义 RoutineRun 结构体**
  - 定义 `RunSource` 枚举（schedule / manual / webhook / api）
  - 定义 `RunStatus` 枚举（received / queued / dispatched / coalesced / skipped / succeeded / failed）
  - 定义 `RoutineRun` 结构体（id, company_id, routine_id, trigger_id, source, status, triggered_at, routine_revision_id, idempotency_key, trigger_payload, dispatch_fingerprint, linked_issue_id, coalesced_into_run_id, failure_reason, completed_at, created_at, updated_at）

- [ ] **定义 Goal 核心枚举与结构体**
  - 定义 `GoalLevel` 枚举（company / project / task）
  - 定义 `GoalStatus` 枚举（planned / active / completed / archived）
  - 定义 `Goal` 结构体（id, company_id, title, description, level, status, parent_id, owner_agent_id, created_at, updated_at）
  - 定义 `CreateGoalInput` / `UpdateGoalInput` 结构体

- [ ] **定义 Approval 核心枚举与结构体**
  - 定义 `ApprovalType` 枚举（hire_agent / spend_credits / create_resource / deploy_agent）
  - 定义 `ApprovalStatus` 枚举（pending / approved / rejected / revision_requested）
  - 定义 `Approval` 结构体（id, company_id, approval_type, requested_by_agent_id, requested_by_user_id, status, payload, decision_note, decided_by_user_id, decided_at, created_at, updated_at）
  - 定义 `IssueApproval` 关联结构体（id, approval_id, issue_id）

### 阶段二：核心功能

- [ ] **实现 Database Schema 迁移 - Routine 相关表**
  - 编写 `routines` 表 migration（所有核心字段 + 索引：company_id, goal_id, assignee_agent_id, status）
  - 编写 `routine_triggers` 表 migration（外键 -> routines.id, 唯一约束：public_id）
  - 编写 `routine_revisions` 表 migration（外键 -> routines.id, 索引：routine_id + revision_number）

- [ ] **实现 Database Schema 迁移 - RoutineRun 与 Approval 表**
  - 编写 `routine_runs` 表 migration（外键 -> routines.id, routine_triggers.id, 索引：routine_id + status, 唯一约束：trigger_id + idempotency_key）
  - 编写 `approvals` 表 migration（外键 -> companies.id, agents.id, 索引：company_id + status）
  - 编写 `issue_approvals` 表 migration（外键 -> approvals.id, issues.id）

- [ ] **实现 Database Schema 迁移 - Goal 表**
  - 编写 `goals` 表 migration（外键 -> companies.id, goals.id 自引用 parent_id, agents.id, 索引：company_id + level + status）
  - 编写 `routines.goal_id` 外键约束追加 migration

### 阶段三：高级特性

- [ ] **实现 JSONB 字段类型安全映射**
  - 为 `variables: Jsonb`（RoutineVariable[]）实现 Rust 类型安全序列化/反序列化
  - 为 `snapshot: Jsonb`（RoutineRevisionSnapshotV1）实现类型安全映射
  - 为 `payload: Jsonb`（Approval）实现 serde_json::Value 到业务类型的转换

- [ ] **实现枚举类型的数据库映射**
  - 为 RoutineStatus / ConcurrencyPolicy / CatchUpPolicy 实现 sqlx 的 Type/Encode/Decode trait
  - 为 TriggerKind / RunSource / RunStatus 实现 sqlx 的 Type/Encode/Decode trait
  - 为 GoalLevel / GoalStatus / ApprovalType / ApprovalStatus 实现 sqlx 的 Type/Encode/Decode trait

---

## 2. Routine CRUD 服务层 实现任务

### 阶段一：基础架构

- [ ] **定义 RoutineRepository trait**
  - 定义 `RoutineRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 定义 `RoutineQueryFilter` 结构体（company_id, project_id, goal_id, status, assignee_agent_id 过滤条件）
  - 定义分页参数 `Pagination` 结构体（limit, offset, cursor）

- [ ] **实现 RoutineRepository 基础查询**
  - 实现 `get_by_id()` 从数据库查询单个 Routine
  - 实现 `list_by_company()` 按 company_id 查询 Routine 列表（含过滤与分页）
  - 实现 `create()` 创建 Routine 记录并返回

- [ ] **实现 RoutineRepository 写入操作**
  - 实现 `update()` 更新 Routine 字段（动态 SQL，仅更新非 None 字段）
  - 实现 `update_last_triggered_at()` / `update_last_enqueued_at()` 时间戳更新
  - 实现 `delete()` 软删除或硬删除 Routine

### 阶段二：核心功能

- [ ] **实现 RoutineService 创建逻辑**
  - 实现 `create()` 验证 routine variables（名称唯一性、类型合法性、默认值类型匹配）
  - 调用 `assert_routine_variable_definitions()` 校验变量定义
  - 创建 Routine 记录 + 创建初始 Revision（v1 快照）

- [ ] **实现 RoutineService 更新与版本控制**
  - 实现 `update()` 更新 Routine 字段
  - 当核心字段变更时，自动创建新 Revision（增量版本号 + 新快照）
  - 实现 `get_revisions()` 获取历史版本列表，`restore_revision()` 恢复到指定版本

- [ ] **实现 Routine 权限校验**
  - 实现 `assert_board_can_assign_tasks()` 验证 Board 用户任务分配权限
  - 实现 `assert_can_manage_company_routine()` 验证 Agent 只能管理分配给自己的 Routine
  - 将权限校验集成到 create/update 路由 handler 中

### 阶段三：高级特性

- [ ] **实现 ResponsibleUserId 解析逻辑**
  - 解析 `responsibleUserId` 字段的特殊赋值逻辑（从 context/agent/user 多来源解析）
  - 实现 responsibleUserId 的默认值回退机制
  - 添加单元测试覆盖各种解析场景

---

## 3. Routine 触发器管理 实现任务

### 阶段一：基础架构

- [ ] **定义 RoutineTriggerRepository trait**
  - 定义 `RoutineTriggerRepository` trait（create, get_by_id, update, delete, get_by_public_id, list_by_routine_id）
  - 定义 `CreateTriggerInput` / `UpdateTriggerInput` 结构体
  - 定义 `RotateSecretResult` 结构体（new_secret_id, rotated_at）

- [ ] **实现 RoutineTriggerRepository 基础操作**
  - 实现 `create()` 创建触发器记录（自动生成 public_id 用于 webhook）
  - 实现 `get_by_public_id()` 通过 public_id 查询触发器
  - 实现 `list_by_routine_id()` 查询 Routine 下的所有触发器

- [ ] **实现 RoutineTriggerRepository 更新与删除**
  - 实现 `update()` 更新触发器配置（cron_expression, timezone, enabled, label, replay_window_sec）
  - 实现 `delete()` 删除触发器
  - 实现 `rotate_secret()` 轮换 Webhook 触发器密钥（生成新 secret_id，更新 last_rotated_at）

### 阶段二：核心功能

- [ ] **实现 Cron 表达式解析与调度计算**
  - 引入 cron 表达式解析库（如 `cron` crate）
  - 实现 `next_cron_tick_in_timezone()` 计算指定时区下的下次触发时间
  - 实现 `assert_schedule_compatible_variables()` 校验定时触发器与变量定义的兼容性

- [ ] **实现 Webhook 触发器签名验证**
  - 实现 `validate_webhook_signature()` HMAC 签名验证逻辑
  - 实现 `validate_replay_window()` 重放窗口检查（replay_window_sec）
  - 实现 `fire_public_trigger()` 公共 Webhook 触发入口（public_id 查找 + 验证 + 执行）

- [ ] **实现触发器执行与并发策略**
  - 实现 `fire_scheduled_trigger()` 定时触发执行（查询 routine + triggers + 检查并发策略）
  - 实现 `check_coalesce_policy()` 并发策略判定（coalesce_if_active / parallel / skip_if_active）
  - 实现 `check_catch_up_policy()` 追赶策略判定（run_missed / skip_missed）

- [ ] **实现 Cron 定时触发后台调度器**
  - 实现后台任务（tokio::spawn），每分钟扫描 routine_triggers.next_run_at <= NOW() 且 enabled = true
  - 实现批量触发：逐个调用 fire_scheduled_trigger()
  - 实现触发后更新 next_run_at 和 last_fired_at
  - 实现调度器健康检查与死锁检测

### 阶段三：高级特性

- [ ] **实现触发器幂等性机制**
  - 从 HTTP header 提取 `idempotency-key`
  - 基于 (trigger_id, idempotency_key) 唯一约束去重
  - 重复请求返回已有 run 记录

- [ ] **实现触发器与 Routine Run 的关联**
  - 创建 RoutineRun 时记录 trigger_id, source, routine_revision_id, trigger_payload
  - 生成 dispatch_fingerprint 用于运行追踪
  - 处理 linked_issue_id 关联逻辑

---

## 4. Routine Run 执行与生命周期 实现任务

### 阶段一：基础架构

- [ ] **定义 RoutineRunRepository trait**
  - 定义 `RoutineRunRepository` trait（create, get_by_id, list_by_routine, update_status, find_active_runs）
  - 定义 `RunQueryFilter` 结构体（routine_id, status, source, trigger_id 过滤条件）
  - 定义 `CreateRunInput` 结构体（routine_id, trigger_id, source, idempotency_key, trigger_payload, routine_revision_id）

- [ ] **实现 RoutineRunRepository 基础操作**
  - 实现 `create()` 创建 Run 记录（初始状态 received）
  - 实现 `get_by_id()` 查询单个 Run
  - 实现 `list_by_routine()` 查询 Routine 下的 Run 列表（含过滤与分页）

- [ ] **实现 RoutineRunRepository 状态更新**
  - 实现 `update_status()` 更新 Run 状态（received -> queued -> dispatched -> succeeded/failed）
  - 实现 `mark_coalesced()` 标记 Run 为合并状态（coalesced_into_run_id）
  - 实现 `mark_failed()` 标记 Run 为失败状态（failure_reason）

### 阶段二：核心功能

- [ ] **实现 RoutineRun 生命周期状态机**
  - 定义 `RunStateMachine`，实现状态转换校验（received -> queued -> dispatched -> succeeded/failed, received -> coalesced, received -> skipped）
  - 实现 `transition()` 方法，校验前置状态合法性后更新
  - 为非法状态转换返回明确错误类型

- [ ] **实现 Routine 执行调度逻辑**
  - 实现 `fire_routine()` 手动触发入口（创建 Run + 唤醒 Agent）
  - 实现 `enqueue_run()` 创建 Run 记录并关联触发上下文
  - 实现 `dispatch_run()` 将 Run 分发到 Heartbeat 服务（状态从 queued -> dispatched）

- [ ] **实现 Routine 触发完整执行链路**
  - 实现 Routine 触发 -> Issue 创建集成（fire_routine 成功后自动创建关联 Issue）
  - 实现 Issue 创建 + Agent 分配原子事务（确保 Issue 和 assignee 同时生效）
  - 实现 Issue 创建失败时的 RoutineRun 回滚逻辑（标记 failed，记录 failure_reason）
  - 实现 Agent 分配后的 Environment Lease 自动获取（可选，基于配置）

- [ ] **实现 Heartbeat 唤醒集成**
  - 定义 `HeartbeatService` trait（wakeup 方法签名）
  - 实现 `wakeup()` 调用 Heartbeat 服务唤醒 Agent（source: routine_trigger, reason: routine_execution, payload, context_snapshot）
  - 唤醒成功后更新 Run 状态为 dispatched，更新 Routine 的 last_enqueued_at

- [ ] **实现 Routine 执行失败通知机制**
  - 实现 RoutineRun failed 状态时的通知逻辑：通知 responsible_user_id
  - 实现 failure_reason 的结构化记录（error_type, error_message, stack_trace）
  - 实现连续失败检测：3 次连续失败时发送告警并暂停 Routine

### 阶段三：高级特性

- [ ] **实现 Run 活跃度检测与 Coalesce 合并**
  - 实现 `find_active_runs()` 查询当前活跃的 Run（状态为 received/queued/dispatched）
  - 当并发策略为 coalesce_if_active 时，将新 Run 合并到已有活跃 Run
  - 合并时更新已有 Run 的 coalesced_into_run_id 关联

- [ ] **实现 Run 完成回调与清理**
  - 实现 `complete_run()` 标记 Run 完成（succeeded/failed + completed_at）
  - 实现完成时触发下游活动日志记录
  - 实现失败 Run 的重试策略配置（预留接口）

---

## 5. Goal 追踪服务 实现任务

### 阶段一：基础架构

- [ ] **定义 GoalRepository trait**
  - 定义 `GoalRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 定义 `GoalQueryFilter` 结构体（company_id, level, status, parent_id, owner_agent_id 过滤条件）
  - 定义 `GoalTree` 结构体（goal + children 递归结构）

- [ ] **实现 GoalRepository 基础查询**
  - 实现 `get_by_id()` 查询单个 Goal
  - 实现 `list_by_company()` 按 company_id 查询 Goal 列表（含过滤）
  - 实现 `list_children()` 查询子 Goal 列表（parent_id 过滤）

- [ ] **实现 GoalRepository 写入操作**
  - 实现 `create()` 创建 Goal 记录
  - 实现 `update()` 更新 Goal 字段（title, description, level, status, owner_agent_id）
  - 实现 `delete()` 删除 Goal（校验无子 Goal 和关联 Routine）

### 阶段二：核心功能

- [ ] **实现 GoalService CRUD**
  - 实现 `create()` 创建 Goal（校验层级合法性：company 级无 parent，project 级 parent 为 company，task 级 parent 为 project）
  - 实现 `update()` 更新 Goal（校验状态流转合法性）
  - 实现 `delete()` 删除 Goal（级联处理或拒绝删除）

- [ ] **实现 Goal 层级结构查询**
  - 实现 `get_tree()` 递归查询 Goal 树（使用 WITH RECURSIVE CTE）
  - 实现 `get_ancestors()` 查询 Goal 的祖先链
  - 实现 `get_descendants()` 查询 Goal 的所有后代

- [ ] **实现默认 Goal 逻辑**
  - 实现 `get_or_create_default_goal()` 为公司自动创建默认 Goal
  - 实现默认 Goal 选择优先级：活跃根 Goal > 任何根 Goal > 第一个创建的 Goal
  - 在 Routine 创建时自动关联默认 Goal（如果未指定 goalId）

### 阶段三：高级特性

- [ ] **实现 Goal 与 Routine/Issue 的关联查询**
  - 实现 `get_routines_by_goal()` 查询 Goal 关联的 Routine 列表
  - 实现 `get_issues_by_goal()` 查询 Goal 关联的 Issue 列表
  - 实现 `get_goal_progress()` 计算 Goal 完成进度（关联 Routine/Issue 的完成比例）

- [ ] **实现 Goal 状态自动推进**
  - 当 Goal 下所有 Routine/Issue 完成时，自动将 Goal 状态推进为 completed
  - 当 Goal 下有 Routine/Issue 变为 active 时，自动将 Goal 状态推进为 active
  - 实现 `recalculate_goal_status()` 批量重新计算 Goal 状态

---

## 6. Approval 审批服务 实现任务

### 阶段一：基础架构

- [ ] **定义 ApprovalRepository trait**
  - 定义 `ApprovalRepository` trait（create, get_by_id, list_by_company, update_status）
  - 定义 `ApprovalQueryFilter` 结构体（company_id, status, approval_type, requested_by_agent_id 过滤条件）
  - 定义 `IssueApprovalRepository` trait（link_many, get_by_approval, get_issues_by_approval）

- [ ] **实现 ApprovalRepository 基础操作**
  - 实现 `create()` 创建审批记录（初始状态 pending）
  - 实现 `get_by_id()` 查询单个审批
  - 实现 `list_by_company()` 按 company_id 查询审批列表（含 status 过滤）

- [ ] **实现 IssueApprovalRepository 关联操作**
  - 实现 `link_many_for_approval()` 批量创建 Issue-Approval 关联
  - 实现 `get_issues_by_approval()` 查询审批关联的 Issue 列表
  - 实现 `get_approvals_by_issue()` 查询 Issue 关联的审批列表

### 阶段二：核心功能

- [ ] **实现 ApprovalService 创建逻辑**
  - 实现 `create()` 创建审批（校验审批类型合法性，payload 与类型匹配校验）
  - 实现 Cheap Recovery Run 安全限制：`is_status_only_cheap_recovery_context()` 检测运行上下文
  - 防止 cheap + status_only recovery 的运行创建或修改审批

- [ ] **实现 Approval 状态流转**
  - 实现 `approve()` 批准审批（status -> approved, 记录 decided_by_user_id, decided_at）
  - 实现 `reject()` 拒绝审批（status -> rejected, 记录 decision_note）
  - 实现 `request_revision()` 请求修改（status -> revision_requested）

- [ ] **实现 Approval 重新提交与 Agent 唤醒**
  - 实现 `resubmit()` 重新提交审批（status: revision_requested -> pending, 更新 payload）
  - 审批批准时检查 `requested_by_agent_id`，若存在则唤醒 Agent（heartbeat.wakeup, source: automation, reason: approval_approved）
  - 唤醒成功后记录活动日志 approval.requester_wakeup_queued

### 阶段三：高级特性

- [ ] **实现 Approval 评论系统**
  - 定义 `ApprovalComment` 结构体（id, approval_id, user_id, body, created_at）
  - 实现 `add_comment()` 添加审批评论
  - 实现 `list_comments()` 获取审批评论列表

- [ ] **实现 Approval 与预算服务集成**
  - 定义 `BudgetService` trait（overview 方法签名）
  - 在 spend_credits 类型审批创建前校验预算限制
  - 审批批准后更新预算消耗记录

---

## 7. Activity 活动日志服务 实现任务

### 阶段一：基础架构

- [ ] **定义 Activity 核心类型与 Repository trait**
  - 定义 `ActivityEvent` 结构体（id, company_id, entity_type, entity_id, action, actor_type, actor_id, metadata, created_at）
  - 定义 `EntityType` 枚举（issue / routine / routine_run / approval）
  - 定义 `ActivityRepository` trait（create, list_by_company, list_by_entity）

- [ ] **实现 ActivityRepository 基础操作**
  - 实现 `create()` 创建活动日志记录
  - 实现 `list_by_company()` 按 company_id 查询活动日志（含过滤与分页）
  - 实现 `list_by_entity()` 按实体类型 + ID 查询活动日志

- [ ] **实现 ActivityService 核心日志记录**
  - 实现 `log_activity()` 通用活动日志记录函数
  - 定义活动日志 action 类型常量（routine.created, routine.revision_created, approval.created, approval.approved 等）
  - 在 Routine/Approval 服务中集成活动日志记录调用

### 阶段二：核心功能

- [ ] **实现 Issue 活动查询**
  - 实现 `for_issue()` 查询 Issue 相关活动日志
  - 集成权限校验：`assert_issue_read_allowed()` 确保用户有读取权限
  - 实现 `resolve_issue_by_ref()` 通过 identifier 解析 Issue

- [ ] **实现 Issue 运行记录查询**
  - 实现 `for_issue_runs()` 查询 Issue 关联的运行记录
  - 实现 `get_run_issues()` 通过 runId 查询关联的 Issue（heartbeat_runs -> issues）
  - 实现跨模块数据聚合（activity + runs 联合查询）

- [ ] **实现 Activity 后台填充机制**
  - 实现 `schedule_run_liveness_backfill()` 异步调度运行活跃度填充
  - 实现 `backfill_missing_run_liveness_for_issue()` 填充缺失的运行活跃度数据
  - 填充失败时仅记录警告，不阻塞主请求

### 阶段三：高级特性

- [ ] **实现 Board 专用活动创建**
  - 实现 `create_board_activity()` Board 用户专用活动日志创建
  - 校验 Board 用户权限后允许创建
  - 与通用 `log_activity()` 区分权限控制逻辑

- [ ] **实现活动日志过滤与排序优化**
  - 实现 `ActivityFilters` 高级过滤（entity_type, entity_id, actor_id, 时间范围）
  - 实现活动日志按 created_at 降序排序与游标分页
  - 添加数据库索引优化（company_id + created_at 复合索引）

---

## 8. Dashboard 聚合服务 实现任务

### 阶段一：基础架构

- [ ] **定义 Dashboard 数据结构**
  - 定义 `DashboardSummary` 结构体（company_info, agent_stats, issue_stats, pending_approvals, monthly_cost, run_activity, budget_overview）
  - 定义 `AgentStats` 结构体（total, by_status: HashMap<AgentStatus, i64>）
  - 定义 `IssueStats` 结构体（total, by_status: HashMap<IssueStatus, i64>）

- [ ] **定义 DashboardRepository trait**
  - 定义 `DashboardRepository` trait（get_company_info, get_agent_stats, get_issue_stats, get_pending_approvals_count, get_monthly_cost, get_run_activity）
  - 定义 `RunActivityStats` 结构体（recovered_runs 数量、运行时长统计）
  - 定义 `BudgetOverview` 结构体（预算额度、已使用、剩余）

- [ ] **实现 DashboardRepository 基础查询**
  - 实现 `get_company_info()` 查询公司基本信息
  - 实现 `get_agent_stats()` 按 status 分组统计 Agent 数量
  - 实现 `get_issue_stats()` 按 status 分组统计 Issue 数量

### 阶段二：核心功能

- [ ] **实现 DashboardService 聚合逻辑**
  - 实现 `summary()` 并行查询所有数据源（使用 tokio::join! 并发执行）
  - 聚合 Agent 统计、Issue 统计、待审批数量、月度成本
  - 聚合运行活动统计（使用递归 CTE 查询 recovered_runs）

- [ ] **实现 Dashboard 成本与预算聚合**
  - 实现 `get_monthly_cost()` 查询月度成本汇总（SUM costCents）
  - 集成 BudgetService::overview() 获取预算概览
  - 合并成本与预算数据到 DashboardSummary

- [ ] **实现 Dashboard API Handler**
  - 实现 `get_dashboard()` HTTP handler（GET /companies/:companyId/dashboard）
  - 权限校验：assert_company_access()
  - 响应序列化与错误处理

### 阶段三：高级特性

- [ ] **实现 Dashboard 缓存策略**
  - 实现内存缓存（TTL 可配置，默认 60s）
  - 缓存 key 基于 company_id + 时间窗口
  - 缓存失效策略：数据变更时主动失效

- [ ] **实现 Dashboard 数据预计算**
  - 实现定时预计算任务（后台 Job 定期聚合统计数据）
  - 预计算结果存储到专用表（dashboard_snapshots）
  - Dashboard 请求优先读取预计算结果，fallback 到实时查询

---

## 9. Costs 成本统计服务 实现任务

### 阶段一：基础架构

- [ ] **定义 Costs 数据结构**
  - 定义 `CostSummary` 结构体（total_cost_cents, period_start, period_end, breakdown）
  - 定义 `CostByAgent` 结构体（agent_id, agent_name, total_cost_cents）
  - 定义 `CostByProvider` / `CostByBiller` / `CostByProject` 结构体

- [ ] **定义 CostRepository trait**
  - 定义 `CostRepository` trait（get_summary, get_by_agent, get_by_provider, get_by_biller, get_by_project, get_window_spend, get_issue_tree_summary）
  - 定义 `CostQueryFilter` 结构体（company_id, start_date, end_date, project_id, agent_id 过滤条件）

- [ ] **定义 Issue 树成本统计结构**
  - 定义 `IssueTreeCostSummary` 结构体（root_issue_id, total_cost_cents, children_cost, run_summary）
  - 定义 `RunSummary` 结构体（run_count, total_duration_ms, total_cost_cents）
  - 定义 `IssueTreeOptions` 结构体（exclude_root: bool）

### 阶段二：核心功能

- [ ] **实现 CostRepository 聚合查询**
  - 实现 `get_summary()` 查询成本汇总（SUM costCents 按时间范围过滤）
  - 实现 `get_by_agent()` 按 Agent 分组聚合成本
  - 实现 `get_by_provider()` / `get_by_biller()` / `get_by_project()` 按维度聚合成本

- [ ] **实现时间窗口成本查询**
  - 实现 `get_window_spend()` 查询指定时间窗口的成本消耗
  - 支持滑动窗口（如最近 7 天、30 天）和固定窗口（月度）
  - 返回窗口内的成本趋势数据

- [ ] **实现 Issue 树递归成本统计**
  - 实现 `get_issue_tree_summary()` 使用 WITH RECURSIVE CTE 递归查询子 Issue
  - 并行查询：CTE issue_tree + costEvents BY issue tree + runSummarySql
  - 合并结果为 IssueTreeCostSummary，支持 exclude_root 选项

### 阶段三：高级特性

- [ ] **实现 Costs API Handler**
  - 实现 GET /companies/:companyId/costs/summary handler
  - 实现 GET /companies/:companyId/costs/by-agent, by-provider, by-biller, by-project handlers
  - 实现 GET /companies/:companyId/costs/window-spend handler
  - 实现 GET /issues/:id/cost-summary handler

- [ ] **实现成本数据预聚合与缓存**
  - 实现成本事件写入时触发预聚合更新
  - 实现按天/周/月维度的预聚合表（cost_daily_summaries）
  - 查询优先读取预聚合数据，fallback 到实时聚合

---

## 10. HTTP 路由层 实现任务

### 阶段一：基础架构

- [ ] **实现 Routine 路由注册**
  - 定义 Routine 路由组（/companies/:companyId/routines, /routines/:id）
  - 实现 GET/POST /companies/:companyId/routines handler 骨架
  - 实现 GET/PATCH /routines/:id handler 骨架

- [ ] **实现 Routine 子资源路由**
  - 实现 GET /routines/:id/revisions + POST /routines/:id/revisions/:revisionId/restore
  - 实现 GET /routines/:id/runs + POST /routines/:id/run
  - 实现 POST /routines/:id/triggers + PATCH/DELETE /routine-triggers/:id
  - 实现 POST /routine-triggers/:id/rotate-secret

- [ ] **实现 Goal 与 Approval 路由注册**
  - 定义 Goal 路由组（/companies/:companyId/goals, /goals/:id）
  - 定义 Approval 路由组（/companies/:companyId/approvals, /approvals/:id）
  - 实现 CRUD handler 骨架

### 阶段二：核心功能

- [ ] **实现 Routine 路由完整逻辑**
  - 集成权限校验（assertBoardCanAssignTasks, assertCanManageCompanyRoutine）到路由中间件
  - 集成活动日志记录（routine.created, routine.revision_created）
  - 集成遥测数据发送

- [ ] **实现 Approval 路由完整逻辑**
  - 实现审批状态操作路由（approve, reject, request-revision, resubmit）
  - 实现审批评论路由（GET/POST /approvals/:id/comments）
  - 实现审批关联 Issue 查询路由（GET /approvals/:id/issues）
  - 集成权限校验（assertBoard 仅 Board 可批准/拒绝）

- [ ] **实现 Activity 与 Dashboard 路由完整逻辑**
  - 实现 Activity 路由（GET /companies/:companyId/activity, GET /issues/:id/activity, GET /issues/:id/runs, GET /heartbeat-runs/:runId/issues）
  - 实现 Dashboard 路由（GET /companies/:companyId/dashboard）
  - 实现 Costs 路由（所有 7 个端点）

### 阶段三：高级特性

- [ ] **实现 Routine 文档注释系统数据模型**
  - 定义 `RoutineAnnotationThread` 结构体（id, routine_id, position, status, created_by_user_id, created_by_agent_id, created_at, updated_at）
  - 定义 `RoutineAnnotationComment` 结构体（id, thread_id, body, author_user_id, author_agent_id, created_at, updated_at）
  - 定义 `AnnotationThreadStatus` 枚举（open / resolved）
  - 实现 routine_annotation_threads 表 migration
  - 实现 routine_annotation_comments 表 migration

- [ ] **实现 Routine 注释系统 Repository**
  - 实现 `RoutineAnnotationRepository` trait（create_thread, list_threads, add_comment, update_thread_status）
  - 实现 `get_threads_by_routine()` - 查询指定 routine 的所有注释线程
  - 实现 `create_thread()` - 创建新注释线程（记录创建位置）
  - 实现 `add_comment()` - 向线程添加评论
  - 实现 `update_thread_status()` - 解决或重新打开线程

- [ ] **实现 Routine 注释 API 路由**
  - 实现 GET `/routines/:id/description/annotations` - 列出所有注释线程
  - 实现 POST `/routines/:id/description/annotations` - 创建新注释线程
  - 实现 POST `/routines/:id/description/annotations/:threadId/comments` - 添加评论
  - 实现 PATCH `/routines/:id/description/annotations/:threadId` - 更新线程状态（resolve/reopen）
  - 集成权限校验（routine 读权限用于 GET，写权限用于 POST/PATCH）

- [ ] **实现 Routine 注释通知机制**
  - 实现新注释线程创建时的通知（通知 routine 负责人和相关成员）
  - 实现评论添加时的通知（通知线程参与者）
  - 实现线程解决时的通知
  - 集成活动日志记录（routine.annotation.created, routine.annotation.resolved）

- [ ] **实现 Webhook 公共触发路由**
  - 实现 POST /routine-triggers/public/:publicId/fire
  - 从 header 提取 idempotency-key 和签名信息
  - 集成签名验证、重放窗口检查、幂等性去重

- [ ] **实现路由中间件与统一错误处理**
  - 实现公司访问权限中间件（assert_company_access）
  - 实现统一错误响应格式（AppError -> HTTP 状态码映射）
  - 实现请求日志与性能追踪中间件

---

## 11. 模块集成与测试 实现任务

### 阶段一：基础架构

- [ ] **定义模块间依赖注入接口**
  - 定义 `RoutineService` 对 HeartbeatService / AccessService / DocumentAnnotationService 的 trait 依赖
  - 定义 `ApprovalService` 对 HeartbeatService / AccessService / IssueApprovalService / BudgetService 的 trait 依赖
  - 使用 Rust trait object 或泛型实现依赖注入

- [ ] **实现 Service 层依赖注入容器**
  - 定义 `AppServices` 结构体持有所有 Service 实例
  - 实现 Service 初始化与依赖注入（构造函数注入）
  - 确保 Service 间无循环依赖

- [ ] **定义统一错误类型**
  - 定义 `AppError` 枚举（NotFound, Unauthorized, Forbidden, Validation, Conflict, Internal）
  - 为 AppError 实现 Into<HttpResponse> 转换
  - 为 AppError 实现 From<sqlx::Error> / From<serde_json::Error> 等转换

### 阶段二：核心功能

- [ ] **实现 Routine 完整流程集成测试**
  - 测试 Routine 创建 -> 版本管理 -> 触发器创建 -> 定时/手动触发 -> Run 生命周期
  - 测试并发策略（coalesce_if_active / parallel / skip_if_active）
  - 测试幂等性去重（idempotency-key）

- [ ] **实现 Approval 完整流程集成测试**
  - 测试审批创建 -> 批准/拒绝/请求修改/重新提交 完整状态流转
  - 测试审批批准后的 Agent 唤醒机制
  - 测试 Cheap Recovery Run 安全限制

- [ ] **实现 Dashboard 与 Costs 聚合集成测试**
  - 测试 Dashboard 并行查询与数据聚合正确性
  - 测试 Costs 多维度聚合查询
  - 测试 Issue 树递归成本统计（WITH RECURSIVE CTE）

### 阶段三：高级特性

- [ ] **实现并发安全与压力测试**
  - 测试高并发下触发器执行的幂等性保证
  - 测试并发创建审批的竞态条件处理
  - 测试 Dashboard 聚合查询在大量数据下的性能

- [ ] **实现端到端 API 测试**
  - 使用 test server 搭建完整 HTTP 测试环境
  - 覆盖所有 API 端点的正常流程与错误流程
  - 验证响应格式、状态码、错误信息一致性

- [ ] **实现数据迁移验证测试**
  - 验证所有 migration 的正确性（up/down）
  - 验证与 TypeScript 版本的数据兼容性（共享同一数据库）
  - 验证枚举值在 TypeScript <-> Rust 间的一致性
