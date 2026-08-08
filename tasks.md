# Issue Thread Interactions 功能迁移任务

## 执行日期
2026-08-08

## 概述

对比 Paperclip (`~/workspace/paperclip/server/src/services/issue-thread-interactions.ts`) 和 Parrot Agent (`crates/services/src/issue_thread_interaction_service.rs`) 的 Issue Thread Interactions 功能，发现**核心业务逻辑不完整**，导致 agent-user 协作工作流中断。

---

## 当前状态

### ✅ 已实现的功能
- `create` - 创建交互
- `list_for_issue` - 列出 issue 的所有交互
- `accept_interaction` - 接受交互（入口方法）
- `reject_interaction` - 拒绝交互（入口方法）
- `accept_suggest_tasks` - 接受任务建议（部分实现）
- `accept_request_confirmation` - 接受请求确认（空壳实现 ⚠️）
- `accept_simple_interaction` - 接受简单交互（空壳实现 ⚠️）
- `reject_suggest_tasks` - 拒绝任务建议

### ❌ 缺失的功能（共 6 个方法）

#### 1. `getById` - 按 ID 获取单个交互
**优先级**: 🟢 低

#### 2. `answerQuestions` - 回答 ask_user_questions 交互
**优先级**: 🟡 中

#### 3. `cancelQuestions` - 取消 ask_user_questions 交互
**优先级**: 🟡 中

#### 4. `expireRequestConfirmationsSupersededByComment` - 用户评论后过期交互
**优先级**: 🟡 中

#### 5. `expireRequestConfirmationsSupersededByHistoricalComments` - 历史评论过期交互
**优先级**: 🟢 低

#### 6. `expireStaleRequestConfirmationsForIssueDocument` - 文档变更后过期交互
**优先级**: 🟡 中

---

## 🔥 严重问题 - 已实现但功能不完整

### 问题 1: `accept_request_confirmation` 缺少 continuation issue 逻辑
**优先级**: 🔴 P0 - 阻塞核心工作流

#### 症状
```rust
warning: unused variable: `issue`
 --> crates/services/src/issue_thread_interaction_service.rs:245:9
```

#### 根本原因
**Paperclip 实现**:
```typescript
// 1. 检查是否需要将 issue 返回给创建交互的 agent
if (shouldReturnAcceptedConfirmationToCreatorAgent({
  issue: issueContext,
  current: args.current,
  actor: args.actor,
})) {
  // 2. 更新 issue 状态和 assignee
  const returnStatus = issueContext.status === "blocked" ? "blocked" : "todo";
  const returnedIssue = await issueService(db).update(args.issue.id, {
    status: returnStatus,
    assigneeAgentId: args.current.createdByAgentId,
    assigneeUserId: null,
  }, tx);
  
  // 3. 返回 continuationIssue
  continuationIssue = {
    id: returnedIssue.id,
    assigneeAgentId: returnedIssue.assigneeAgentId,
    status: returnedIssue.status,
  };
}
```

**Rust 当前实现**:
```rust
// ❌ 只标记 interaction 为 accepted，不处理 issue
Ok(AcceptInteractionResult {
    interaction: updated_interaction,
    created_issues: vec![],
    continuation_issue: None, // 总是 None！
})
```

#### 业务影响
```
正常流程:
1. Agent 处理 issue，需要用户确认是否删除某个文件
2. Agent 创建 request_confirmation，将 issue 分配给用户
3. 用户接受了删除请求
4. ✅ Issue 自动返回给 agent，agent 继续处理
5. ✅ 工作流继续

当前实现:
1. Agent 处理 issue，需要用户确认
2. Agent 创建 request_confirmation，将 issue 分配给用户
3. 用户接受了删除请求
4. ❌ Issue 仍然分配给用户
5. ❌ Agent 不知道可以继续
6. ❌ 工作流完全中断 - issue 卡在用户手上
```

#### 修复范围
- `crates/services/src/issue_thread_interaction_service.rs::accept_request_confirmation`
- `crates/services/src/issue_thread_interaction_service.rs::accept_simple_interaction`
- 新增辅助函数: `should_return_accepted_confirmation_to_creator_agent`

---

### 问题 2: `accept_suggest_tasks` 缺少 workspace finalization 检查
**优先级**: 🟡 P1 - 数据一致性风险

#### 症状
```rust
warning: unused variable: `issue`
 --> crates/services/src/issue_thread_interaction_service.rs:273:9
```

#### 根本原因
Paperclip 在接受 `request_confirmation` 前会调用 `assertIssueWorkspaceFinalizedForAccept`，确保 workspace 已同步完成，防止用户基于旧代码做决策。

**Paperclip 实现**:
```typescript
case "request_confirmation": {
  await assertIssueWorkspaceFinalizedForAccept({ 
    db, 
    issue, 
    sourceRun current.sourceRunId 
  });
  const accepted = await acceptRequestConfirmation({ ... });
  return { ... };
}
```

**Rust 当前实现**:
```rust
// ❌ 缺少 workspace finalization 检查
InteractionKind::RequestConfirmation => {
    self.accept_request_confirmation(issue, interaction, input, actor).await
}
```

#### 修复范围
- 新增模块: `crates/services/src/issue_workspace_validation.rs`
- 实现函数: `assert_issue_workspace_finalized_for_accept`
- 集成到 `accept_request_confirmation` 和 `accept_simple_interaction`

---

## 迁移任务拆解

### Phase 1: 修复核心工作流（P0 - 立即执行）

---

#### Task 1.1: 修复 `acceest_confirmation` - 实现 continuation issue 逻辑
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**前置条件**: 无

**内容**:
1. **实现辅助函数 `should_return_accepted_confirmation_to_creator_agent`**
   - 检查条件：
     - Interaction 的 `continuation_policy` 是 `"return_to_creator"`
     - Interaction 由 agent 创建 (`created_by_agent_id` 不为 NULL)
     - 当前 actor 不是创建者（用户接受了 agent 的请求）
   - 参考: Paperclip `issue-thread-interactions.ts:725-732`

2. **修改 `accept_request_confirmation` 方法**
   - 调用 `should_return_accepted_confirmation_to_creator_agent` 判断
   - 如果需要返回 issue:
     - 计算新状态: `issue.status == "blocked" ? "blocked" : "todo"`
     - 更新 issue:
       - `status` → 新状态
       - `assignee_agent_id` → `interaction.created_by_agent_id`
       - `assignee_user_id` → NULL
     - 返回 `continuation_issue: Some(IssueWakeTarget { ... })`
   - 参考: Paperclip `issue-thread-interactions.ts:747-819`

3. **修改 `accept_simple_interaction` 方法**
   - 与上述逻辑相同
   - 参考: Paperclip `issue-thread-interactions.ts:821-881`

4. **更新返回类型**
   - `AcceptInteractionResult` 的 `continuation_issue` 字段已存在
   - 确保正确填充

**验证标准**:
- [ ] `cargo check` 通过，无未使用变量警告
- [ ] 单元测试: 用户接受 agent 创建的 request_confirmation → issue 返回给 agent
- [ ] 单元测试: agent 自己接受自己的 confirmation → 不返回 issue
- [ ] 单元测试: continuation_policy != "return_to_creator" → 不返回 issue

**参考实现**: Paperclip `issue-thread-interactions.ts:725-881`

---

#### Task 1.2: 添加 workspace finalization 检查
**文件**: `crates/services/src/issue_workspace_validation.rs` (新建)

**前置条件**: 无

**内容**:
1. **创建新模块 `issue_workspace_validation`**
   ```rust
   pub async fn assert_issue_workspace_finalized_for_accept(
       pool: &PgPool,
       issue_id: Uuid,
       source_run_id: Option<Uuid>,
   ) -> Result<(), AppError>
   ```

2. **实现检查逻辑**
   - 如果 `source_run_id` 为 None，直接返回 Ok
   - 查询 `heartbeat_runs` 表，检查 run 的 `workspace_finalized` 字段
   - 如果 workspace 未 finalized，返回 `AppError::Conflict`
   - 参考: Paperclip `issues.ts:6780-6815`

3. **集成到 `accept_interaction`**
   - 在 `accept_request_confirmation` 调用前检查
   - 在 `accept_simple_interaction` 调用前检查

**验证标准**:
- [ ] `cargo check` 通过
- [ ] 单元测试: workspace 已 finalized → 接受成功
- [ ] 单元测试: workspace 未 finalized → 返回 Conflict 错误
- [ ] 单元测试: source_run_id 为 None → 直接通过

**参考实现**: Paperclip `issues.ts:6780-6815`

---

### Phase 2: 实现缺失的交互方法（P1 - 高优先级）

---

#### Task 2.1: 实现 `getById` 方法
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟢 低（便利性功能）

**内容**:
1. **添加方法**
   ```rust
   pub async fn get_by_id(
       &self,
       interaction_id: Uuid,
   ) -> Result<Option<IssueThreadInteraction>, AppError>
   ```

2. **实现逻辑**
   - 查询 `issue_thread_interactions` 表
   - 返回 `Option<IssueThreadInteraction>`

**验证标准**:
- [ ] 单元测试: 存在的 ID → 返回 Some
- [ ] 单元测试: 不存在的 ID → 返回 None

**参考实现**: Paperclip `issue-thread-interactions.ts:945-953`

---

#### Task 2.2: 实现 `answerQuestions` 方法
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟡 中（Agent 提问功能）

**内容**:
1. **添加输入类型**
   ```rust
   pub struct AnswerQuestionsInput {
       pub answers: Vec<QuestionAnswer>,
       pub summary_markdown: Option<String>,
   }
   
   pub struct QuestionAnswer {
       pub question_id: String,
       pub option_ids: Option<Vec<String>>,
       pub answer_text: Option<String>,
   }
   ```

2. **实现方法**
   ```rust
   pub async fn answer_questions(
       &self,
       issue_id: Uuid,
       interaction_id: Uuid,
       input: AnswerQuestionsInput,
       actor: &InteractionActor,
   ) -> Result<IssueThreadInteraction, AppError>
   ```

3. **实现逻辑**
   - 验证 interaction 是 `ask_user_questions` kind
   - 验证 interaction 状态是 `pending`
   - 规范化答案（验证 question_id 和 option_ids 有效性）
   - 更新状态为 `answered`
   - 填充 result 字段
   - 触发 telemetry（如果已实现）

**验证标准**:
- [ ] 单元测试: 正确答案 → 更新成功
- [ ] 单元测试: 错误的 kind → 返回错误
- [ ] 单元测试: 已 resolved → 返回 Conflict 错误
- [ ] 单元测试: 无效的 question_id → 返回错误

**参考实现**: Paperclip `issue-thread-interactions.ts:1577-1634`

---

#### Task 2.3: 实现 `cancelQuestions` 方法
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟡 中（Agent 提问功能）

**内容**:
1. **添加输入类型**
   ```rust
   pub struct CancelQuestionsInput {
       pub reason: Option<String>,
   }
   ```

2. **实现方法**
   ```rust
   pub async fn cancel_questions(
       &self,
       issue_id: Uuid,
       interaction_id: Uuid,
       input: CancelQuestionsInput,
       actor: &InteractionActor,
   ) -> Result<IssueThreadInteraction, AppError>
   ```

3. **实现逻辑**
   - 验证 interaction 是 `ask_user_questions` kind
   - 验证 interaction 状态是 `pending`
   - 更新状态为 `cancelled`
   - 填充 result: `{ cancelled: true, cancellationReason: ..., answers: [] }`
   - 触发 telemetry（如果已实现）

**验证标准**:
- [ ] 单元测试: 正确取消 → 更新成功
- [ ] 单元测试: 错误的 kind → 返回错误
- [ ] 单元测试: 已 resolved → 返回 Conflict 错误

**参考实现**: Paperclip `issue-thread-interactions.ts:1636-1691`

---

#### Task 2.4: 实现 `expireRequestConfirmationsSupersededByComment`
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟡 中（用户体验优化）

**内容**:
1. **实现方法**
   ```rust
   pub async fn expire_request_confirmations_superseded_by_comment(
       &self,
       issue_id: Uuid,
       comment: &IssueComment,
       actor: &InteractionActor,
   ) -> Result<Vec<IssueThreadInteraction>, AppError>
   ```

2. **实现逻辑**
   - 查询 issue 的所有 pending 的可被评论过期的交互:
     - `request_confirmation`
     - `request_checkbox_confirmation`
     - `ask_user_questions`
   - 筛选需要过期的交互:
     - `should_supersede_interaction_on_user_comment` 为 true
     - 评论时间 >= 交互创建时间
   - 批量更新状态为 `expired`
   - 填充 result: `{ expirationReason: "superseded_by_comment", ... }`
   - 返回过期的交互列表

3. **添加辅助函数**
   ```rust
   fn should_supersede_interaction_on_user_comment(
       interaction: &IssueThreadInteraction
   ) -> bool
   ```

**验证标准**:
- [ ] 单元测试: 用户评论后 → 过期所有符合条件的交互
- [ ] 单元测试: 评论时间早于交互 → 不过期
- [ ] 单元测试: agent 评论 → 不过期

**参考实现**: Paperclip `issue-thread-interactions.ts:1323-1380`

---

#### Task 2.5: 实现 `expireStaleRequestConfirmationsForIssueDocument`
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟡 中（防止基于旧文档的决策）

**内容**:
1. **实现方法**
   ```rust
   pub async fn expire_stale_request_confirmations_for_issue_document(
       &self,
       issue_id: Uuid,
       document: Option<&IssueDocument>,
       actor: &InteractionActor,
   ) -> Result<Vec<IssueThreadInteraction>, AppError>
   ```

2. **实现逻辑**
   - 查询 issue 的所有 pending 的 `request_confirmation` 类交互
   - 筛选目标是 `issue_document` 且已过期的交互:
     - `target.revisionId != document.latestRevisionId`
     - `target.revisionNumber != document.latestRevisionNumber`
   - 批量更新状态为 `expired`
   - 填充 result: `{ outcome: "stale_target", staleTarget: ... }`
   - 更新 payload 中的 target 为当前版本
   - 返回过期的交互列表

**验证标准**:
- [ ] 单元测试: 文档更新后 → 过期所有旧版本的交互
- [ ] 单元测试: 版本匹配 → 不过期
- [ ] 单元测试: document 为 None → 过期所有文档相关的交互

**参考实现**: Paperclip `issue-thread-interactions.ts:1501-1575`

---

### Phase 3: 低优先级功能（P2 - 可选）

---

#### Task 3.1: 实现 `expireRequestConfirmationsSupersededByHistoricalComments`
**文件**: `crates/services/src/issue_thread_interaction_service.rs`

**优先级**: 🟢 低（批量清理）

**内容**:
1. **实现方法**
   ```rust
   pub async fn expire_request_confirmations_superseded_by_historical_comments(
       &self,
       issue_id: Uuid,
   ) -> Result<Vec<IssueThreadInteraction>, AppError>
   ```

2. **实现逻辑**
   - 查询 issue 的所有 pending 交互和所有用户评论
   - 批量处理过期逻辑（与 Task 2.4 类似，但批量）
   - 返回过期的交互列表

**验证标准**:
- [ ] 单元测试: 批量过期历史交互

**参考实现**: Paperclip `issue-thread-interactions.ts:1382-1499`

---

## 路由层集成（Phase 4）

当 Phase 1-2 完成后，需要在 `crates/api/src/routes/interactions.rs` 中添加对应的路由：

### 新增路由

#### 1. `GET /interactions/:interactionId` - 获取单个交互
对应方法: `getById`

#### 2. `POST /issues/:issueId/interactions/:interactionId/answer` - 回答问题
对应方法: `answerQuestions`

#### 3. `POST /issues/:issueId/interactions/:interactionId/cancel` - 取消问题
对应方法: `cancelQuestions`

---

## 验证计划

### 端到端测试场景

#### 场景 1: Agent 请求用户确认后继续工作
```
1. 创建 issue，分配给 agent A
2. Agent A 创建 request_confirmation:
   - kind: "request_confirmation"
   - continuation_policy: "return_to_creator"
   - created_by_agent_id: A
3. Issue 自动分配给用户 U
4. 用户 U 接受确认:
   POST /issues/:id/interactions/:interactionId/accept
5. 验证结果:
  on 状态变为 "accepted"
   ✓ Issue 重新分配给 agent A
   ✓ Issue 状态变为 "todo"
   ✓ 返回的 continuation_issue 不为 None
6. Agent A 收到 wakeup（如果已实现）
```

#### 场景 2: 用户评论后过期交互
```
1. 创建 issue，分配给 agent A
2. Agent A 创建 request_confirmation
3. Issue 分配给用户 U
4. 用户 U 发表评论（未接受/拒绝交互）
5. 调用 expire_request_confirmations_superseded_by_comment
6. 验证结果:
   ✓ Interaction 状态变为 "expired"
   ✓ Result 包含 expirationReason: "superseded_by_comment"
```

#### 场景 3: Agent 向用户提问并获得答案
```
1. 创建 issue，分配给 agent A
2. Agent A 创建 ask_user_questions 交互
3. 用户 U 回答问题:
   POST /issues/:id/interactions/:interactionId/answer
   {
     "answers": [
       { "questionId": "q1", "optionIds": ["opt1", "opt2"] }
     ],
     "summaryMarkdown": "User chose options 1 and 2"
   }
4. 验证结果:
   ✓ Interaction 状态变为 "answered"
   ✓ Result 包含用户的答案
```

---

## 总结

### 缺失功能统计
- **P0 (阻塞工作流)**: 2 个问题
  1. `accept_request_confirmation` 缺少 continuation issue 逻辑
  2. 缺少 workspace finalization 检查

- **P1 (核心功能)**: 4 个方法
  1. `answerQuestions`
  2. `cancelQuestions`
  3. `expireRequestConfirmationsSupersededByComment`
  4. `expireStaleRequestConfirmationsForIssueDocument`

- **P2 (便利性)**: 2 个方法
  1. `getById`
  2. `expireRequestConfirmationsSupersededByHistoricalComments`

### 推荐执行顺序
1. **立即执行**: Task 1.1, 1.2（修复 P0 问题）
2. **高优先级**: Task 2.2, 2.3（实现 ask_user_questions 功能）
3. **中优先级**: Task 2.4, 2.5（交互过期逻辑）
4. **低优先级**: Task 2.1, 3.1（便利性功能）

### 估算工时
- Phase 1 (P0): 4-6 小时
- Phase 2 (P1): 6-8 小时
- Phase 3 (P2): 2-3 小时
- 总计: 12-17 小时

---

**文档版本**: v1.0 
**创建时间**: 2026-08-08 
**作者**: Kiro AI
