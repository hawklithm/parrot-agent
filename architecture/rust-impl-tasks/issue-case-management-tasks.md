# Issue/Case 管理模块 - Rust 实现任务拆解

> 基于 [backend/issue-case-management.md](../backend/issue-case-management.md) 架构分析文档，拆解为 Rust 版本实现任务。
> 版本: 1.0
> 日期: 2026/07/11

---

## 1. 数据模型层 实现任务

### 阶段一：基础架构

- [x] **定义 Issue 核心枚举类型**
  - 定义 `IssueStatus` 枚举（backlog / todo / in_progress / in_review / blocked / done / cancelled）
  - 定义 `IssuePriority` 枚举（low / medium / high / urgent）
  - 定义 `IssueWorkMode` 枚举（standard 及其他扩展模式）

- [x] **定义 Issue 结构体**
  - 定义 `Issue` 结构体，映射核心字段（id, company_id, project_id, project_workspace_id, goal_id, parent_id, title, description, status, work_mode, priority, assignee_agent_id, assignee_user_id, checkout_run_id, execution_run_id, execution_locked_at）
  - 定义 `CreateIssueInput` 结构体（company_id, project_id, title, description, status, priority, parent_id 等）
  - 定义 `UpdateIssueInput` 结构体（可选字段更新：title, description, status, priority, assignee_agent_id 等）

- [x] **定义 Issue 执行与监控字段类型**
  - 定义 `IssueExecutionPolicy` 结构体（JSONB 映射：执行策略配置）
  - 定义 `IssueExecutionState` 结构体（JSONB 映射：运行时执行状态）
  - 定义 Issue 监控字段（monitor_next_check_at, monitor_wake_requested_at, monitor_last_triggered_at, monitor_attempt_count）

- [x] **定义 Case 核心枚举类型**
  - 定义 `CaseStatus` 枚举（draft / in_progress / in_review / approved / done / cancelled）
  - 定义 `CaseType` 枚举（按业务需求扩展）
  - 定义 `CaseIssueLinkRole` 枚举（origin / work / reference）

- [x] **定义 Case 结构体**
  - 定义 `Case` 结构体，映射核心字段（id, company_id, case_number, identifier, case_type, key, title, summary, status, fields, project_id, parent_case_id, completed_at）
  - 定义 `CreateCaseInput` 结构体（project_id, case_type, key, title, summary, status, fields, parent_case_id）
  - 定义 `UpdateCaseInput` 结构体（可选字段更新）

- [x] **定义关联表结构体**
  - 定义 `CaseIssueLink` 结构体（id, company_id, case_id, issue_id, role, created_at）
  - 定义 `IssueComment` 结构体（id, issue_id, body, actor_type, actor_id, created_at, updated_at）
  - 定义 `IssueDocument` / `CaseDocument` 结构体（id, issue_id/case_id, key, content, locked_by, locked_at, created_at, updated_at）

### 阶段二：核心功能

- [x] **实现 Database Schema 迁移 - Issue 相关表**
  - 编写 `issues` 表 migration（所有核心字段 + 索引）
  - 编写 `issue_comments` 表 migration（外键 -> issues.id）
  - 编写 `issue_documents` 表 migration（外键 -> issues.id）

- [x] **实现 Database Schema 迁移 - Case 相关表**
  - 编写 `cases` 表 migration（唯一约束：company_id + case_type + key）
  - 编写 `case_events` 表 migration（事件溯源，外键 -> cases.id）
  - 编写 `case_issue_links` 表 migration（多对多关联，外键 -> cases.id, issues.id）

- [x] **实现 Database Schema 迁移 - 树形控制与辅助表**
  - 编写 `issue_tree_holds` 表 migration（mode, reason, release_policy, metadata 等）
  - 编写 `issue_tree_hold_members` 表 migration（外键 -> issue_tree_holds.id, issues.id）
  - 编写 `issue_approvals` / `issue_work_products` / `issue_attachments` 表 migration

### 阶段三：高级特性

- [x] **实现 JSONB 字段类型安全映射**
  - 为 `execution_policy: Jsonb` 实现 Rust 类型安全的序列化/反序列化
  - 为 `execution_state: Jsonb` 实现 Rust 类型安全映射
  - 为 `fields: Jsonb`（Case）实现 `serde_json::Value` 到业务类型的转换

---

## 2. Issue CRUD 服务层 实现任务

### 阶段一：基础架构

- [x] **定义 IssueRepository trait**
  - 定义 `IssueRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 定义 `IssueQueryFilter` 结构体（status, priority, assignee, project_id, parent_id 等过滤条件）
  - 定义分页参数 `Pagination` 结构体（limit, offset, cursor）

- [x] **实现 IssueRepository 基础查询**
  - 实现 `get_by_id()` 从数据库查询单个 Issue
  - 实现 `list_by_company()` 按 company_id 查询 Issue 列表（含过滤与分页）
  - 实现 `count_by_company()` 统计 Issue 数量

- [x] **定义 IssueService trait**
  - 定义 `IssueService` trait（create, get, update, delete, checkout, release）
  - 定义 `IssueServiceInput` / `IssueServiceOutput` 辅助类型
  - 定义 `IssueMutationResult` 结构体（changed, issue, change_kind）

### 阶段二：核心功能

- [x] **实现 Issue 创建**
  - 实现 `create()` 插入 Issue 记录（含 project 归属校验）
  - 实现创建子 Issue 的 `create_child()` 方法（设置 parent_id，校验父 Issue 存在）
  - 实现 Issue 树完整性校验：循环引用检测（递归 CTE 检查）、跨公司约束（parent/child 必须同 company_id）
  - 实现 Issue 树深度限制检查（防止无限深度导致查询性能问题）
  - 实现创建后活动日志记录（`issue.created` 事件）

- [x] **实现 Issue 更新与删除**
  - 实现 `update()` 更新 Issue 字段（支持部分更新，检测状态流转合法性）
  - 实现 `delete()` 软删除或状态置为 cancelled
  - 实现状态流转校验（如从 in_progress 不可直接回退到 backlog）

- [x] **定义 Issue 状态机与验证规则**
  - 定义 `IssueStateMachine` 结构体，包含所有合法状态转换
  - 实现状态转换验证器：检查 current_status -> target_status 是否合法
  - 实现状态继承逻辑：父 Issue cancelled 时，所有子 Issue 自动 cancelled
  - 实现状态变更权限校验（某些转换需特定角色）

- [x] **实现 Issue 搜索**
  - 实现 `search()` 全文搜索方法（title, description 模糊匹配）
  - 实现按 status / priority / assignee 的过滤查询
  - 实现跨公司隔离（确保搜索结果限于公司范围）

### 阶段三：高级特性

- [x] **实现 Issue 批量操作**
  - 实现批量状态更新（配合树形控制模块）
  - 实现批量分配变更
  - 实现批量优先级调整

- [x] **实现 Issue 心跳上下文查询**
  - 实现 `get_heartbeat_context()` 获取 Issue 相关的心跳运行上下文
  - 联表查询 heartbeat_runs 获取当前执行状态
  - 实现 `get_external_objects()` 和 `refresh_external_objects()` 外部对象管理

---

## 3. Issue Checkout/Release 服务层 实现任务

### 阶段一：基础架构

- [x] **定义 Checkout/Release 输入输出类型**
  - 定义 `CheckoutInput` 结构体（agent_id, expected_statuses, checkout_run_id）
  - 定义 `ReleaseInput` 结构体（release_run_id, result, target_status）
  - 定义 `ForceReleaseInput` 结构体（admin 操作，含 reason）

- [x] **定义 Checkout Wakeup 决策类型**
  - 定义 `CheckoutWakeInput` 结构体（actor_type, actor_agent_id, checkout_agent_id, checkout_run_id）
  - 定义 `ActorType` 枚举（board / user / agent）
  - 定义 `should_wake_assignee_on_checkout()` 函数签名

- [x] **实现 checkout_issue_schema 验证**
  - 使用 garde / validator crate 定义 `CheckoutIssueSchema`（agent_id: uuid, expected_statuses: nonempty vec）
  - 定义 `ReleaseIssueSchema` 验证规则
  - 定义 `ForceReleaseSchema` 验证规则（需 Board 权限）

### 阶段二：核心功能

- [x] **实现 Issue Checkout 流程**
  - 实现 `checkout()` 方法：校验 expected_statuses -> 更新 assignee_agent_id -> 设置 checkout_run_id -> 更新 status（如 todo -> in_progress）
  - 实现事务性更新（单个数据库事务内完成状态校验与字段更新）
  - 实现 checkout 后与 Environment Lease 集成：调用 LeaseService 获取环境租约
  - 实现租约获取失败时的回滚逻辑（Issue 状态回退、checkout 取消）
  - 实现 checkout 后活动日志记录（`issue.checked_out` 事件）

- [x] **实现 should_wake_assignee_on_checkout 决策逻辑**
  - 实现：非 agent 类型 actor（board/user）总是唤醒
  - 实现：没有 actor_agent_id 或 actor_agent_id != checkout_agent_id 时唤醒
  - 实现：没有 checkout_run_id 时唤醒，否则不重复唤醒（同一 agent 的 run）

- [x] **实现 Issue Release 流程**
  - 实现 `release()` 方法：清除 checkout_run_id -> 根据配置转换状态（如 in_progress -> in_review）
  - 实现 `force_release()` 方法：管理员强制释放，无需 expected_statuses 校验
  - 实现 force_release 权限校验（Board vs Company Admin 角色检查）
  - 实现 force_release 通知机制：通知原 assignee，记录中断原因到活动日志
  - 实现 release 后活动日志记录（`issue.released` 事件）

### 阶段三：高级特性

- [x] **实现 Heartbeat 集成**
  - [x] 实现 checkout 后调用 `heartbeat_service.wakeup()` 唤醒被分配人
  - [x] 实现 force_release 后调用 `heartbeat_service.cancel_run()` 取消活跃运行
  - [x] 实现心跳上下文的异步通知机制（HeartbeatService trait + HeartbeatContext）

- [x] **实现 Issue 执行锁定**
  - [x] 实现 `execution_locked_at` 字段的设置与检查
  - [x] 实现分布式锁机制：使用数据库字段级别的锁获取（execution_locked_at + execution_run_id）
  - [x] 实现执行锁超时检测与自动释放（基于 execution_locked_at + 阈值）
  - [x] 实现僵尸锁清理：检测超时锁并强制释放（cleanup_zombie_locks）
  - [x] 实现锁冲突解决策略（返回 409 Conflict）
  - [x] 实现 `execution_run_id` 关联的运行状态查询

---

## 4. Issue 评论与交互服务层 实现任务

### 阶段一：基础架构

- [x] **定义评论数据类型**
  - 定义 `IssueComment` 结构体（id, issue_id, body, actor_type, actor_id, metadata, created_at, updated_at）
  - 定义 `AddCommentInput` 结构体（body, reopen_requested, metadata）
  - 定义 `CommentActorType` 枚举（agent / user / board）

- [x] **定义 Thread Interaction 类型**
  - 定义 `ThreadInteraction` 结构体（id, issue_id, interaction_type, actor, created_at, resolved_at）
  - 定义 `CreateInteractionInput` 结构体
  - 定义 `ResolveInteractionInput` 结构体

- [x] **实现 CommentRepository trait**
  - 定义 `CommentRepository` trait（create, get_by_id, list_by_issue, delete）
  - 定义 `InteractionRepository` trait（create, get, list_by_issue, resolve）
  - 实现基础的数据库查询方法

### 阶段二：核心功能

- [x] **实现评论添加流程**
  - 实现 `add_comment()` 方法：插入评论 -> 更新 Issue 的 last_activity_at 和 updated_at
  - 实现 `reopen_requested` 逻辑：评论时可请求重新打开 Issue
  - 实现评论后活动日志记录

- [x] **实现评论权限校验**
  - 实现 `assert_agent_issue_comment_allowed()`：校验 Agent 是否有评论权限
  - 实现 watchdog scope 检查和 `decide_issue_access()` 权限判定
  - 实现评论删除权限校验（仅作者或管理员可删除）

- [x] **实现 Thread Interaction 管理**
  - 实现 `create_interaction()` 创建线程交互
  - 实现 `resolve_interaction()` 解决交互
  - 实现按 issue_id 列出所有交互

### 阶段三：高级特性

- [x] **实现评论触发恢复重新校验**
  - 实现 `revalidate_active_source_recovery()` 方法：评论后检查恢复动作是否过期
  - 调用 `recovery_actions_svc.reconcile_for_issue_and_ancestors()` 协调恢复
  - 实现 `recovery_actions_svc.resolve_active_for_issue()` 解决过期的恢复动作

- [x] **实现评论触发 Watchdog 评估**
  - 实现 `queue_task_watchdog_evaluation()` 方法：评论后触发任务看门狗评估
  - 调用 `task_watchdogs_svc.reconcile_for_issue_and_ancestors()` 协调看门狗
  - 实现评估结果的事件通知

---

## 5. Issue 文档与注释服务层 实现任务

### 阶段一：基础架构

- [x] **定义文档数据类型**
  - 定义 `IssueDocument` / `CaseDocument` 结构体（id, parent_type, parent_id, key, content, content_type, locked_by, locked_at, created_at, updated_at）
  - 定义 `CreateDocumentInput` 结构体（key, content, content_type）
  - 定义 `UpdateDocumentInput` 结构体（content, content_type）

- [x] **定义文档锁定类型**
  - 定义 `DocumentLock` 结构体（locked_by, locked_at, run_id）
  - 定义 `LockDocumentInput` 结构体（run_id）
  - 实现 `is_locked()` 辅助方法

- [x] **定义注释线程类型**
  - 定义 `AnnotationThread` 结构体（id, document_id, position, status, created_at, updated_at）
  - 定义 `AnnotationComment` 结构体（id, thread_id, body, actor, created_at）
  - 定义 `CreateAnnotationThreadInput` 结构体

### 阶段二：核心功能

- [x] **实现文档 CRUD**
  - 实现 `list_documents()` 按 issue/case 列出文档
  - 实现 `get_document()` 按 key 获取文档内容
  - 实现 `upsert_document()` 创建或更新文档（PUT 语义）

- [x] **实现文档锁定/解锁**
  - 实现 `lock_document()` 锁定文档（设置 locked_by + locked_at）
  - 实现 `unlock_document()` 解锁文档（清除锁定字段）
  - 实现锁定冲突检测（非锁定者不可修改已锁定文档）

- [x] **实现注释线程 CRUD（Issue 与 Case 共用）**
  - 实现 `create_annotation_thread()` 创建注释线程
  - 实现 `list_annotation_threads()` 按文档列出线程
  - 实现 `update_annotation_thread()` 更新线程状态（如 resolved）

### 阶段三：高级特性

- [x] **实现 Case 文档修订版本**
  - 定义 `DocumentRevision` 结构体（id, document_id, revision_number, content, created_at, created_by）
  - 实现 `list_revisions()` 列出文档修订历史
  - 实现 `restore_revision()` 恢复到指定修订版本

- [x] **实现注释线程评论**
  - 实现 `add_annotation_comment()` 在注释线程中添加评论
  - 实现评论通知机制（@mention 解析与推送）
  - 实现线程状态自动更新（有新评论时标记为 open）

---

## 6. Issue 树形控制服务层 实现任务

### 阶段一：基础架构

- [x] **定义树形控制核心枚举**
  - 定义 `IssueTreeControlMode` 枚举（pause / resume / cancel / restore）
  - 定义 `IssueTreeHoldReleasePolicyStrategy` 枚举（manual / all_done / first_done）
  - 定义 `IssueTreeHoldReleasePolicy` 结构体（strategy, note）

- [x] **定义树形保持数据类型**
  - 定义 `IssueTreeHold` 结构体（id, company_id, root_issue_id, mode, reason, release_policy, metadata, actor, created_at, released_at）
  - 定义 `IssueTreeHoldMember` 结构体（id, hold_id, issue_id, previous_status, created_at）
  - 定义 `CreateIssueTreeHoldInput` 结构体（mode, reason, release_policy, metadata）

- [x] **定义树形控制预览类型**
  - 定义 `IssueTreeControlPreview` 结构体（affected_issues, active_runs, status_changes）
  - 定义 `AffectedIssue` 结构体（issue_id, current_status, target_status）
  - 定义 `PreviewActiveRun` 结构体（run_id, agent_id, issue_id）

### 阶段二：核心功能

- [x] **实现树形预览功能**
  - 实现 `preview()` 方法：遍历子树所有 Issue -> 计算各 Issue 的状态变更 -> 收集活跃 Runs -> 返回 IssueTreeControlPreview
  - 实现子树遍历（递归 CTE 查询 parent_id 链）
  - 实现按 mode 计算目标状态（pause -> 保持不变但标记暂停，cancel -> 全部置为 cancelled）

- [x] **实现创建树形保持**
  - 实现 `create_hold()` 方法：INSERT issue_tree_holds -> INSERT issue_tree_hold_members（每个后代） -> UPDATE issues SET status
  - 实现事务性操作（单事务完成 hold 创建 + 状态批量更新）
  - 实现活动日志记录（`tree_hold_created` 事件 + 每个 run 的 `tree_hold_run_interrupted` 事件）

- [x] **实现释放树形保持**
  - 实现 `release_hold()` 方法：清除 hold 记录 -> 恢复 Issue 状态（resume 模式）
  - 实现 release_policy 评估（manual 直接释放，all_done 等所有子 Issue 完成，first_done 第一个完成即释放）
  - 实现释放后的活动日志记录

### 阶段三：高级特性

- [x] **实现暂停门控查询**
  - 实现 `get_active_pause_hold_gate()` 方法：检查 Issue 是否被暂停门控拦截
  - 在 Issue checkout 流程中集成门控检查（被暂停的 Issue 不可 checkout）
  - 实现门控状态缓存（减少数据库查询）

- [x] **实现级联取消与恢复**
  - 实现 `cancel_issue_statuses_for_hold()` 批量取消子树 Issues
  - 实现 `restore_issue_statuses_for_hold()` 从 hold members 恢复原始状态
  - 实现取消时的心跳 Run 中断（调用 heartbeat_service.cancel_run()）

- [x] **实现取消未认领的唤醒**
  - 实现 `cancel_unclaimed_wakeup_requests_for_tree()` 批量取消子树中的待处理唤醒
  - 实现唤醒请求状态检查（claimed vs unclaimed）
  - 集成到 pause/cancel 操作的后续处理流程

---

## 7. Case CRUD 服务层 实现任务

### 阶段一：基础架构

- [x] **定义 CaseRepository trait**
  - 定义 `CaseRepository` trait（create, get_by_id, list_by_company, update, delete）
  - 定义 `CaseEventRepository` trait（create_event, list_by_case）
  - 定义 `CaseQueryFilter` 结构体（status, case_type, project_id 等过滤条件）

- [x] **定义 CaseService trait**
  - 定义 `CaseService` trait（create, get, update, link_issue, load_detail）
  - 定义 `CaseDetail` 结构体（case + labels + issue_links + documents + attachments + parent_case）
  - 定义 `CaseEvent` 结构体（id, case_id, kind, metadata, created_at）

- [x] **定义 Case Event 类型**
  - 定义 `CaseEventKind` 枚举（created, updated, status_changed, document_revised, issue_linked, issue_unlinked）
  - 定义事件元数据结构（changed_fields, actor 等）
  - 实现 `CaseEventKind` 的 Display trait

### 阶段二：核心功能

- [x] **实现 Case 创建（含 Upsert 逻辑）**
  - 实现 `create()` 方法：检查 case_type + key 唯一键 -> 已存在则更新 -> 不存在则插入
  - 实现标识符生成：使用 advisory lock + company.issue_prefix 生成 `PAP-XXX-C1` 格式
  - 实现创建/更新后记录 case_events（kind: created / updated，upsert: true）

- [x] **实现 Case 自动关联 Issue**
  - 实现 `auto_link_run_issue()` 方法：根据 actor.run_id 解析关联 Issue -> INSERT case_issue_links（role: origin）
  - 实现关联校验（避免重复关联同一 Issue）
  - 实现关联角色推导（origin: 创建者运行，work: 工作关联，reference: 参考引用）

- [x] **实现 Case-Issue 角色语义校验**
  - 实现 origin 角色唯一性约束（一个 Case 只能有一个 origin Issue）
  - 实现 role 互斥性校验（同一 Issue 不能同时有多个角色）
  - 实现 role 转换验证：reference 可升级为 work，但 origin 不可变更
  - 实现 work 角色的状态约束：关联 Issue 需为 in_progress 或 done

- [x] **实现 Case 详情加载**
  - 实现 `load_case_detail()` 方法：联表查询 labels + issue_links + documents + attachments + parent_case
  - 实现分步加载（先查 Case 主记录，再并行查关联数据）
  - 实现 N+1 查询优化（使用 IN 查询替代逐个加载）

### 阶段三：高级特性

- [x] **实现 Case 事件溯源**
  - 实现所有变更的 case_events 记录（status_changed, document_revised, issue_linked 等）
  - 实现事件查询（按 case_id 分页、按 kind 过滤）
  - 实现事件回溯（从事件流重建 Case 状态）

- [x] **实现 Case 与 Issue 的双向查询**
  - 实现 `GET /issues/:issueId/cases` 查询 Issue 关联的所有 Cases
  - 实现 `POST /cases/:id/links` 主动关联 Issue 到 Case
  - 实现关联角色变更（从 reference 升级为 work 等）

- [x] **实现 Cases 功能开关**
  - 实现 `assert_cases_enabled()` 检查公司是否启用 Cases 功能
  - 在所有 Case 路由中添加功能开关守卫
  - 未启用时返回 404 Not Found

---

## 8. Issue 附属资源服务层 实现任务

### 阶段一：基础架构

- [x] **定义 WorkProduct 数据类型**
  - 定义 `WorkProduct` 结构体（id, issue_id, name, description, artifact, created_at, updated_at）
  - 定义 `CreateWorkProductInput` 结构体（name, description, artifact）
  - 定义 `UpdateWorkProductInput` 结构体

- [x] **定义 Approval 与 Watchdog 类型**
  - 定义 `IssueApproval` 结构体（id, issue_id, approval_id, created_at）
  - 定义 `IssueWatchdog` 结构体（id, issue_id, config, created_at, updated_at）
  - 定义 `RecoveryAction` 结构体（id, issue_id, action_type, status, created_at）

- [x] **定义附件与标签类型**
  - 定义 `Attachment` 结构体（id, parent_type, parent_id, asset_id, filename, content_type, size, created_at）
  - 定义 `Label` 结构体（id, company_id, name, color, created_at）
  - 定义 `FeedbackVote` 结构体（id, issue_id, voter_id, vote, created_at）

### 阶段二：核心功能

- [x] **实现 WorkProduct CRUD**
  - 实现 `list_work_products()` 按 issue_id 列出工作产物
  - 实现 `create_work_product()` 创建工作产物
  - 实现 `update_work_product()` / `delete_work_product()` 更新和删除

- [x] **实现 Approval 关联管理**
  - 实现 `list_approvals()` 获取 Issue 审批列表
  - 实现 `link_approval()` 关联审批到 Issue
  - 实现 `unlink_approval()` 取消审批关联
  - 定义 Approval Service 集成接口（获取审批状态、注册状态变更回调）

- [x] **实现 Approval 状态传播机制**
  - 实现 Approval approved 事件监听器：自动更新 Issue 状态（blocked -> in_progress）
  - 实现 Approval rejected 事件处理：Issue 状态转换、通知相关人员
  - 实现 Approval 状态变更的活动日志记录

- [x] **实现 Watchdog 配置管理**
  - 实现 `get_watchdog()` / `upsert_watchdog()` / `delete_watchdog()` CRUD
  - 实现 `check_now()` 立即触发监控检查
  - 实现 `retry_now()` 立即重试计划的定时重试

- [x] **实现 Monitor 定时调度器**
  - 实现后台作业（tokio::spawn），轮询 `monitor_next_check_at < NOW()` 的 Issues
  - 执行检查逻辑，更新 `monitor_last_triggered_at` 和 `monitor_attempt_count`
  - 根据结果调度下次检查时间（exponential backoff with jitter）
  - 实现调度器健康检查与死锁检测

### 阶段三：高级特性

- [x] **实现附件上传与管理**
  - 实现 `list_attachments()` 按 issue 列出附件
  - 实现 `upload_attachment()` 上传附件（含文件大小校验：company.attachment_max_bytes）
  - 实现 `get_attachment_content()` / `delete_attachment()` 获取内容和删除

- [x] **实现诊断端点**
  - 实现 `get_blockers_diagnostics()` 获取阻碍诊断
  - 实现 `get_wakes_diagnostics()` 获取唤醒诊断
  - 实现 `get_subtree_diagnostics()` 获取子树诊断

- [x] **实现低信任审查机制**
  - 实现 `promote_low_trust()` 提升低信任输出（Board 用户操作）
  - 实现提升时记录 sourceTrust 元数据
  - 实现读取时可选脱敏（根据用户权限过滤敏感字段）

---

## 9. Issue 辅助功能服务层 实现任务

### 阶段一：基础架构

- [x] **定义已读/归档状态类型**
  - 定义 `IssueReadStatus` 结构体（issue_id, user_id, read_at）
  - 定义 `IssueInboxArchive` 结构体（issue_id, user_id, archived_at）
  - 定义 Repository trait（mark_read, unmark_read, archive, unarchive）

- [x] **定义反馈与追踪类型**
  - 定义 `FeedbackVote` 结构体与 `FeedbackTrace` 结构体
  - 定义 `FeedbackTraceBundle` 结构体（trace + 关联数据）
  - 定义 `CreateFeedbackVoteInput` 结构体

- [x] **定义计划分解类型**
  - 定义 `PlanDecomposition` 结构体（id, issue_id, plan, created_at）
  - 定义 `CreatePlanDecompositionInput` 结构体
  - 定义分解结果的结构化格式

### 阶段二：核心功能

- [x] **实现已读标记与归档**
  - 实现 `mark_read()` / `unmark_read()` 标记/取消已读
  - 实现 `archive()` / `unarchive()` 归档/取消归档
  - 实现已读状态聚合查询（批量检查多个 Issue 的已读状态）

- [x] **实现反馈投票与追踪**
  - 实现 `list_feedback_votes()` / `add_feedback_vote()` 反馈投票管理
  - 实现 `list_feedback_traces()` / `get_feedback_trace()` 反馈追踪查询
  - 实现 `get_feedback_trace_bundle()` 获取追踪束（trace + 关联 Issue/Case 数据）

- [x] **实现标签管理**
  - 实现 `create_label()` 创建公司级标签
  - 实现 `delete_label()` 删除标签
  - 实现标签与 Issue/Case 的关联管理

### 阶段三：高级特性

- [x] **实现计划分解**
  - 实现 `list_accepted_plan_decompositions()` 查询已接受的分解
  - 实现 `create_plan_decomposition()` 创建分解
  - 实现分解执行跟踪（子 Issue 自动创建）

- [x] **实现恢复动作管理**
  - 实现 `list_recovery_actions()` 获取恢复动作列表
  - 实现 `resolve_recovery_action()` 解决恢复动作
  - 实现恢复动作解决后的状态传播：更新 Issue 状态、发送通知、创建审计日志
  - 实现恢复动作的自动触发与协调（recovery_actions_svc.reconcile）
  - 定义 Recovery Action 协调算法：比较当前 Issue 状态与预期恢复结果 -> 匹配则 resolve，仍失败则 re-trigger

---

## 10. API 路由层 实现任务

### 阶段一：基础架构

- [x] **定义验证 Schema**
  - 使用 garde / validator crate 定义 `CreateIssueSchema`、`UpdateIssueSchema`
  - 定义 `CheckoutIssueSchema`（agent_id: uuid, expected_statuses: nonempty vec）
  - 定义 `CreateCaseSchema`（project_id, case_type, key, title 等）

- [x] **搭建 Issue 路由框架**
  - 使用 axum 定义 Issue 路由组（`/api/issues`, `/api/companies/:companyId/issues`）
  - 实现路径参数提取器（IssueId, CompanyId）
  - 统一错误响应格式（AppError -> axum::Json）

- [x] **搭建 Case 路由框架**
  - 使用 axum 定义 Case 路由组（`/api/companies/:companyId/cases`, `/api/cases`）
  - 实现 Cases 功能开关中间件
  - 定义 Case 路由的公共提取器

### 阶段二：核心功能

- [x] **实现 Issue CRUD 路由端点**
  - 实现 `GET /issues` 列表、`GET /issues/:id` 详情、`POST /companies/:companyId/issues` 创建
  - 实现 `PATCH /issues/:id` 更新、`DELETE /issues/:id` 删除
  - 实现 `GET /companies/:companyId/issues/count` 统计、`GET /companies/:companyId/search` 搜索

- [x] **实现 Checkout/Release 路由端点**
  - 实现 `POST /issues/:id/checkout` 检出
  - 实现 `POST /issues/:id/release` 释放
  - 实现 `POST /issues/:id/admin/force-release` 强制释放（需 Board 认证）

- [x] **实现 Case CRUD 路由端点**
  - 实现 `POST /companies/:companyId/cases` 创建（含 Upsert）
  - 实现 `GET /companies/:companyId/cases` 列表、`GET /cases/:id` 详情
  - 实现 `PATCH /cases/:id` 更新

### 阶段三：高级特性

- [x] **实现 Issue 子资源路由端点**
  - 实现评论路由（`GET/POST /issues/:id/comments`, `DELETE /issues/:id/comments/:commentId`）
  - 实现交互路由（`GET/POST /issues/:id/interactions`, `POST /issues/:id/interactions/:interactionId/resolve`）
  - 实现文档路由（`GET/PUT /issues/:id/documents/:key`, `POST .../lock`, `POST .../unlock`）

- [x] **实现 Case 子资源路由端点**
  - 实现事件路由（`GET /cases/:id/events`）
  - 实现文档路由（含修订版本：`GET/PUT /cases/:id/documents/:key`, `GET .../revisions`, `POST .../revisions/:revisionId/restore`）
  - 实现关联路由（`POST /cases/:id/links`, `GET /issues/:issueId/cases`）

- [x] **实现树形控制路由端点**
  - 实现 `POST /issues/:id/tree-control/preview` 预览
  - 实现 `POST /issues/:id/tree-holds` 创建保持
  - 实现 `GET /issues/:id/tree-control/state` 和 `GET /issues/:id/tree-holds` 查询
  - 实现 `POST /issues/:id/tree-holds/:holdId/release` 释放保持

- [x] **实现 Issue 附属资源路由端点**
  - 实现 WorkProduct 路由（`GET/POST /issues/:id/work-products`, `PATCH/DELETE /work-products/:id`）
  - 实现 Approval 路由（`GET/POST /issues/:id/approvals`, `DELETE /issues/:id/approvals/:approvalId`）
  - 实现 Watchdog 路由（`GET/PUT/DELETE /issues/:id/watchdog`）
  - 实现附件路由（`GET /issues/:id/attachments`, `POST .../attachments`, `GET /attachments/:id/content`, `DELETE /attachments/:id`）

- [x] **实现 Issue 辅助功能路由端点**
  - 实现已读/归档路由（`POST/DELETE /issues/:id/read`, `POST/DELETE /issues/:id/inbox-archive`）
  - 实现反馈路由（`GET/POST /issues/:id/feedback-votes`, `GET /issues/:id/feedback-traces`）
  - 实现诊断路由（`GET /issues/:id/diagnostics/blockers`, `GET .../wakes`, `GET .../subtree`）
  - 实现低信任路由（`POST /issues/:id/low-trust/promotions`）

---

## 11. 认证与授权集成 实现任务

### 阶段一：基础架构

- [x] **定义 Issue/Case 权限模型**
  - 定义 `IssueAction` 枚举（create, read, update, delete, checkout, release, comment, mutate）
  - 定义 `CaseAction` 枚举（create, read, update, link_issue, delete）
  - 定义 `TreeControlAction` 枚举（preview, create_hold, release_hold, view_state）

- [x] **实现 Issue 权限断言**
  - 实现 `assert_agent_issue_mutation_allowed()` 校验 Agent 变更权限
  - 实现 `assert_agent_issue_comment_allowed()` 校验评论权限
  - 实现 `require_agent_run_id()` 从请求中提取并校验 Agent Run ID

- [x] **实现 Case 权限断言**
  - 实现 `assert_cases_enabled()` 检查 Cases 功能是否启用
  - 实现 `assert_company_access()` 校验公司访问权限
  - 实现 `assert_project_belongs_to_company()` 校验项目归属

### 阶段二：核心功能

- [x] **实现 Board 认证守卫**
  - 实现树形控制路由的 Board 认证守卫（`assert_board()`）
  - 实现强制释放路由的管理员认证
  - 实现低信任提升路由的 Board 认证

- [x] **实现 Issue 访问决策集成**
  - 实现 `decide_issue_access()` 方法：综合判断 Agent 对 Issue 的访问权限
  - 集成 watchdog scope 检查
  - 实现批量权限过滤（`filter_issues_for_actor()`）

- [x] **实现 Case 访问控制**
  - 实现 Case 的公司级隔离（确保跨公司不可访问）
  - 实现 Case Upsert 的并发控制（`lock_case_upsert_key()` 使用 advisory lock）
  - 实现 parent_case 归属校验

### 阶段三：高级特性

- [x] **实现字段级权限控制**
  - 实现 Issue 字段级脱敏（根据权限过滤 execution_state 等敏感字段）
  - `redact_issue_for_actor()` 基于 SourceTrustLevel 脱敏敏感字段
  - `filter_issues_by_source_trust()` 按信任级别过滤不可见 Issue
  - 实现 Case fields 的按角色脱敏
  - 实现文档内容的按权限访问控制

---

## 12. 活动日志与事件通知 实现任务

### 阶段一：基础架构

- [x] **定义活动日志类型**
  - 定义 `ActivityEvent` enum（issue_created, issue_updated, issue_checked_out, issue_released, issue_commented, tree_hold_created, tree_hold_run_interrupted, case_created, case_updated, case_status_changed 等）
  - 定义 `ActivityLogEntry` 结构体（event_type, actor, resource_type, resource_id, metadata, created_at）
  - 定义 `ActivityLogService` trait

- [x] **实现 ActivityLogService 基础**
  - 实现 `log_activity()` 方法：将活动事件持久化到 activity_logs 表
  - 实现按 company / issue / case 的活动查询
  - 实现分页和时间范围过滤

### 阶段二：核心功能

- [x] **集成活动日志到 Issue 流程**
  - 在 checkout 后记录 `issue.checked_out` 事件
  - 在 release 后记录 `issue.released` 事件
  - 在树形控制操作后记录 `tree_hold_created` 和 `tree_hold_run_interrupted` 事件

- [x] **集成活动日志到 Case 流程**
  - 在 Case 创建后记录 `case.created` 事件
  - 在 Case 更新后记录 `case.updated` 事件（含变更字段）
  - 在 Case 状态变更后记录 `case.status_changed` 事件

### 阶段三：高级特性

- [x] **实现事件通知机制**
  - 实现 Issue 变更的异步通知（watchdog 评估、恢复动作协调）
  - 实现 Case 事件的订阅与推送
  - 实现通知去重与频率控制

---

## 依赖顺序总览

```
阶段一（基础架构）推荐实现顺序:

  1. 数据模型层 (枚举 + 结构体 + Schema 迁移)
  2. 认证与授权集成 (权限模型 + 断言函数)
  3. Issue CRUD 服务层 (Repository + Service trait)
  4. Case CRUD 服务层 (Repository + Service trait + Upsert 逻辑)
  5. Issue Checkout/Release 服务层 (输入输出类型 + 验证 Schema)
  6. Issue 评论与交互服务层 (类型 + Repository)
  7. Issue 文档与注释服务层 (类型 + 锁定机制)
  8. Issue 树形控制服务层 (枚举 + 保持类型 + 预览类型)
  9. Issue 附属资源服务层 (WorkProduct + Approval + Watchdog 类型)
  10. Issue 辅助功能服务层 (已读/归档 + 反馈类型)
  11. API 路由层 (验证 Schema + 路由框架)
  12. 活动日志与事件通知 (类型 + 基础 Service)

阶段二（核心功能）推荐实现顺序:

  1. Issue CRUD 服务层 (创建 + 更新 + 搜索)
  2. Issue Checkout/Release 服务层 (checkout + release + wake 决策)
  3. Case CRUD 服务层 (创建含 Upsert + 自动关联 + 详情加载)
  4. Issue 评论与交互服务层 (评论添加 + 权限校验 + Interaction)
  5. Issue 文档与注释服务层 (CRUD + 锁定/解锁 + 注释线程)
  6. Issue 树形控制服务层 (预览 + 创建保持 + 释放保持)
  7. Issue 附属资源服务层 (WorkProduct + Approval + Watchdog)
  8. Issue 辅助功能服务层 (已读/归档 + 反馈 + 标签)
  9. 认证与授权集成 (Board 守卫 + 访问决策 + 字段级控制)
  10. API 路由层 (Issue/Case CRUD + Checkout/Release 端点)
  11. 活动日志与事件通知 (集成到 Issue/Case 流程)

阶段三（高级特性）推荐实现顺序:

  1. Issue Checkout/Release (Heartbeat 集成 + 执行锁定)
  2. Issue 评论与交互 (恢复重新校验 + Watchdog 评估)
  3. Issue 文档与注释 (修订版本 + 注释评论)
  4. Issue 树形控制 (暂停门控 + 级联取消恢复 + 取消唤醒)
  5. Case CRUD (事件溯源 + 双向查询 + 功能开关)
  6. Issue 附属资源 (附件 + 诊断 + 低信任审查)
  7. Issue 辅助功能 (计划分解 + 恢复动作)
  8. API 路由层 (子资源 + 树形控制 + 附属资源 + 辅助功能路由)
  9. 活动日志与事件通知 (异步通知 + 订阅推送)
```

---

## Rust 技术选型建议

| 领域 | 推荐选型 | 说明 |
|------|----------|------|
| Web 框架 | axum 0.7+ | 生态成熟，与 tower 中间件集成好 |
| ORM / 查询 | sqlx 0.7+ | 编译时 SQL 检查，异步 PostgreSQL |
| 数据库迁移 | sqlx-cli 或 refinery | 与 sqlx 配套 |
| 请求验证 | garde 或 validator | garde 更现代，支持嵌套结构验证 |
| 错误处理 | thiserror + anyhow | thiserror 定义业务错误，anyhow 内部使用 |
| 序列化 | serde + serde_json | JSONB 字段统一用 serde 映射 |
| UUID | uuid crate (v7) | 时间有序，索引友好 |
| 时间 | chrono 或 time | 时间戳字段处理 |
| 异步运行时 | tokio | 标准选择 |
| 并发控制 | PostgreSQL advisory lock | Case Upsert 并发控制（`lock_case_upsert_key`） |
| 测试 | testcontainers + mockall | 集成测试用 testcontainers，单测用 mockall |
| 递归查询 | CTE (Common Table Expressions) | Issue 子树遍历使用递归 CTE |
