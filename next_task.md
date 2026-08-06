# MCP 工具迁移任务清单

## 现状分析

### parrot-agent 当前已有的 41 个 MCP 工具
所有工具的**定义**和**schema**都已存在，但需要验证**实现完整性**：

- ✅ paperclipMe, paperclipInboxLite
- ✅ paperclipListAgents, paperclipGetAgent
- ✅ paperclipListIssues, paperclipGetIssue, paperclipGetHeartbeatContext
- ✅ paperclipListComments, paperclipGetComment
- ✅ paperclipListIssueApprovals, paperclipListDocuments, paperclipGetDocument, paperclipListDocumentRevisions
- ✅ paperclipListProjects, paperclipGetProject
- ✅ paperclipGetIssueWorkspaceRuntime, paperclipControlIssueWorkspaceServices, paperclipWaitForIssueWorkspaceService
- ✅ paperclipListGoals, paperclipGetGoal
- ✅ paperclipListApprovals, paperclipCreateApproval, paperclipGetApproval, paperclipGetApprovalIssues, paperclipListApprovalComments
- ✅ paperclipCreateIssue, paperclipUpdateIssue, paperclipCheckoutIssue, paperclipReleaseIssue
- ✅ paperclipAddComment
- ✅ paperclipSuggestTasks, paperclipAskUserQuestions, paperclipRequestConfirmation, paperclipRequestCheckboxConfirmation
- ✅ paperclipUpsertIssueDocument, paperclipRestoreIssueDocumentRevision
- ✅ paperclipLinkIssueApproval, paperclipUnlinkIssueApproval
- ✅ paperclipApprovalDecision, paperclipAddApprovalComment
- ✅ paperclipApiRequest

---

## 一、完全缺失的 MCP 工具（高优先级）

### 1.1 Cases（案例管理）- 20个工具
paperclip 中存在完整的 Cases REST API（`server/src/routes/cases.ts`），parrot-agent 也有对应的 Rust 路由（`crates/api/src/routes/cases.rs`），但 **MCP 工具层完全缺失**。

#### 基础 CRUD
- [x] **paperclipListCases** - 列出公司的所有案例
  - 参考：paperclip `GET /companies/:companyId/cases`
  - 实现位置：`crates/api/src/routes/tools.rs` 工具定义、验证、执行路由映射

- [x] **paperclipGetCase** - 获取单个案例详情
  - 参考：paperclip `GET /cases/:id`
  - 实现位置：同上

- [x] **paperclipCreateCase** - 创建新案例
  - 参考：paperclip `POST /companies/:companyId/cases`
  - Schema需包含：title, description, pipelineId, stageId 等

- [x] **paperclipUpdateCase** - 更新案例信息
  - 参考：paperclip `PATCH /cases/:id`
  - 实现位置：同上

#### 案例文档管理
- [ ] **paperclipListCaseDocuments** - 列出案例文档
  - 参考：需查看 paperclip cases.ts 中文档端点

- [ ] **paperclipGetCaseDocument** - 获取案例文档内容
  - 参考：paperclip `GET /cases/:id/documents/:key`

- [ ] **paperclipUpsertCaseDocument** - 创建或更新案例文档
  - 参考：paperclip `PUT /cases/:id/documents/:key`

- [ ] **paperclipListCaseDocumentRevisions** - 列出文档修订历史
  - 参考：paperclip `GET /cases/:id/documents/:key/revisions`

- [ ] **paperclipRestoreCaseDocumentRevision** - 恢复文档到指定版本
  - 参考：paperclip `POST /cases/:id/documents/:key/revisions/:revisionId/restore`

- [ ] **paperclipDeleteCaseDocument** - 删除案例文档
  - 参考：paperclip `DELETE /cases/:id/documents/:key`

#### 案例文档锁定
- [ ] **paperclipLockCaseDocument** - 锁定文档防止并发编辑
  - 参考：paperclip `POST /cases/:id/documents/:key/lock`

- [ ] **paperclipUnlockCaseDocument** - 解锁文档
  - 参考：paperclip `POST /cases/:id/documents/:key/unlock`

#### 案例文档标注
- [ ] **paperclipListCaseDocumentAnnotations** - 列出文档标注
  - 参考：paperclip `GET /cases/:id/documents/:key/annotations`

- [ ] **paperclipGetCaseDocumentAnnotationThread** - 获取标注线程详情
  - 参考：paperclip `GET /cases/:id/documents/:key/annotations/:threadId`

- [ ] **paperclipCreateCaseDocumentAnnotation** - 创建文档标注
  - 参考：paperclip `POST /cases/:id/documents/:key/annotations`

- [ ] **paperclipReplyCaseDocumentAnnotation** - 回复标注评论
  - 参考：paperclip `POST /cases/:id/documents/:key/annotations/:threadId/reply`

- [ ] **paperclipUpdateCaseDocumentAnnotation** - 更新标注线程
  - 参考：paperclip `PATCH /cases/:id/documents/:key/annotations/:threadId`

#### 案例关联和子案例
- [ ] **paperclipGetCaseChildren** - 获取子案例列表
  - 参考：paperclip `GET /cases/:caseId/children`

- [ ] **paperclipCreateCaseLink** - 创建案例与 Issue 的关联
  - 参考：paperclip `POST /cases/:id/links`

- [ ] **paperclipGetIssueCases** - 获取 Issue 关联的所有案例
  - 参考：paperclip `GET /issues/:issueId/cases`

---

### 1.2 Routines（例行程序）- 15个工具
paperclip 中存在 Routines REST API（`server/src/routes/routines.ts`），但 parrot-agent **MCP 工具层完全缺失**。

#### 基础 CRUD
- [x] **paperclipListRoutines** - 列出公司的所有例行程序
  - 参考：paperclip `GET /companies/:companyId/routines`

- [x] **paperclipGetRoutine** - 获取单个例行程序详情
  - 参考：paperclip `GET /routines/:id`

- [x] **paperclipCreateRoutine** - 创建新例行程序
  - 参考：paperclip `POST /companies/:companyId/routines`
  - Schema需包含：title, description, assigneeAgentId 等

- [x] **paperclipUpdateRoutine** - 更新例行程序
  - 参考：paperclip `PATCH /routines/:id`
#### 例行程序版本管理
- [ ] **paperclipListRoutineRevisions** - 列出例行程序修订版本
  - 参考：paperclip `GET /routines/:id/revisions`

- [ ] **paperclipRestoreRoutineRevision** - 恢复到指定版本
  - 参考：paperclip `POST /routines/:id/revisions/:revisionId/restore`

#### 例行程序描述文档标注
- [ ] **paperclipListRoutineDescriptionAnnotations** - 列出描述文档标注
  - 参考：paperclip `GET /routines/:id/description/annotations`

- [ ] **paperclipGetRoutineDescriptionAnnotationThread** - 获取标注线程
  - 参考：paperclip `GET /routines/:id/description/annotations/:threadId`

- [ ] **paperclipCreateRoutineDescriptionAnnotation** - 创建描述标注
  - 参考：paperclip `POST /routines/:id/description/annotations`

- [ ] **paperclipReplyRoutineDescriptionAnnotation** - 回复标注评论
  - 参考：paperclip `POST /routines/:id/description/annotations/:threadId/reply`

- [ ] **paperclipUpdateRoutineDescriptionAnnotation** - 更新标注
  - 参考：paperclip `PATCH /routines/:id/description/annotations/:threadId`

#### 例行程序触发器
- [ ] **paperclipCreateRoutineTrigger** - 创建触发器
  - 参考：paperclip `POST /routines/:id/triggers`

- [ ] **paperclipUpdateRoutineTrigger** - 更新触发器
  - 参考：paperclip `PATCH /routine-triggers/:id`

- [ ] **paperclipDeleteRoutineTrigger** - 删除触发器
  - 参考：paperclip `DELETE /routine-triggers/:id`

- [ ] **paperclipRotateRoutineTriggerSecret** - 轮换触发器密钥
  - 参考：paperclip `POST /routine-triggers/:id/rotate-secret`

#### 例行程序执行
- [ ] **paperclipListRoutineRuns** - 列出执行历史
  - 参考：paperclip `GET /routines/:id/runs`

- [ ] **paperclipRunRoutine** - 手动执行例行程序
  - 参考：paperclip `POST /routines/:id/run`

---

### 1.3 Issue Document Annotations（Issue文档标注）- 5个工具
REST API 已在前一个会话添加到 parrot-agent（`crates/api/src/routes/issues.rs:459-823`），但 **MCP 工具层缺失**。

- [ ] **paperclipListIssueDocumentAnnotations** - 列出Issue文档标注
  - 参考：parrot-agent `GET /api/issues/:id/documents/:key/annotations`
  - 实现位置：`crates/api/src/routes/tools.rs`

- [ ] **paperclipGetIssueDocumentAnnotationThread** - 获取标注线程详情
  - 参考：parrot-agent `GET /api/issues/:id/documents/:key/annotations/:thread_id`

- [ ] **paperclipCreateIssueDocumentAnnotation** - 创建Issue文档标注
  - 参考：parrot-agent `POST /api/issues/:id/documents/:key/annotations`
  - Schema需包含：body, anchorJson, resolved 等

- [ ] **paperclipReplyIssueDocumentAnnotation** - 回复标注评论
  - 参考：parrot-agent `POST /api/issues/:id/documents/:key/annotations/:thread_id/reply`

- [ ] **paperclipUpdateIssueDocumentAnnotation** - 更新标注线程状态
  - 参考：parrot-agent `PATCH /api/issues/:id/documents/:key/annotations/:thread_id`

---

### 1.4 Labels（标签管理）- 3个工具
paperclip 中存在 Labels REST API（`server/src/routes/issues.ts`），但 **MCP 工具层缺失**。

- [ ] **paperclipListLabels** - 列出公司的所有标签
  - 参考：paperclip `GET /companies/:companyId/labels`

- [ ] **paperclipCreateLabel** - 创建新标签
  - 参考：paperclip `POST /companies/:companyId/labels`
  - Schema需包含：name, color, description

- [ ] **paperclipDeleteLabel** - 删除标签
  - 参考：paperclip `DELETE /labels/:labelId`

---

### 1.5 Attachments（附件管理）- 4个工具
paperclip 中存在 Attachments REST API，但 **MCP 工具层缺失**。

- [ ] **paperclipListIssueAttachments** - 列出Issue附件
  - 参考：paperclip `GET /issues/:id/attachments`

- [ ] **paperclipCreateIssueAttachment** - 上传Issue附件
  - 参考：paperclip `POST /companies/:companyId/issues/:issueId/attachments`
  - 注意：需处理文件上传

- [ ] **paperclipGetAttachmentContent** - 获取附件内容
  - 参考：paperclip `GET /attachments/:attachmentId/content`

- [ ] **paperclipDeleteAttachment** - 删除附件
  - 参考：paperclip `DELETE /attachments/:attachmentId`

---

### 1.6 External Objects（外部对象）- 2个工具
paperclip 中存在 External Objects REST API，但 **MCP 工具层缺失**。

- [ ] **paperclipListIssueExternalObjects** - 列出Issue关联的外部对象
  - 参考：paperclip `GET /issues/:id/external-objects`

- [ ] **paperclipRefreshIssueExternalObjects** - 刷新外部对象数据
  - 参考：paperclip `POST /issues/:id/external-objects/refresh`

---

### 1.7 File Resources（文件资源）- 3个工具
paperclip 中存在 File Resources REST API（`server/src/routes/file-resources.ts`），但 **MCP 工具层缺失**。

- [ ] **paperclipListIssueFileResources** - 列出Issue文件资源
  - 参考：paperclip `GET /issues/:issueId/file-resources/list`

- [ ] **paperclipResolveIssueFileResource** - 解析文件资源路径
  - 参考：paperclip `GET /issues/:issueId/file-resources/resolve`

- [ ] **paperclipGetIssueFileResourceContent** - 获取文件资源内容
  - 参考：paperclip `GET /issues/:issueId/file-resources/content`

---

## 二、需要检查实现完整性的工具（中优先级）

以下工具在 parrot-agent 中**已有定义和schema**，但需要验证其**执行逻辑**是否完整（对比 paperclip 实现）：

### 2.1 需深度验证的工具

- [ ] **paperclipGetIssueWorkspaceRuntime** - 验证是否正确返回 workspace 和 runtimeServices
  - paperclip实现：`packages/mcp-server/src/tools.ts:361-364`
  - parrot-agent实现：`crates/api/src/routes/tools.rs:290-293 + 1820-1831`

- [ ] **paperclipControlIssueWorkspaceServices** - 验证 start/stop/restart 逻辑
  - paperclip实现：包含自动获取 workspaceId 的逻辑（`tools.ts:366-381`）
  - parrot-agent实现：`tools.rs:1802-1819` 已实现自动获取，需验证完整性

- [ ] **paperclipWaitForIssueWorkspaceService** - 验证轮询等待逻辑
  - paperclip实现：1秒轮询，检查 status 和 healthStatus（`tools.ts:383-407`）
  - parrot-agent实现：`tools.rs:1765-1801` 已实现，需验证超时和健康检查

- [ ] **paperclipAddComment** - 验证 presentation 和 metadata 复杂结构
  - 当前实现：validation 在 `tools.rs:447-527` 已非常详细
  - 需验证：实际执行时是否正确序列化和传递到 REST API

- [ ] **paperclipSuggestTasks / AskUserQuestions / RequestConfirmation** - 验证 interaction payload
  - 当前实现：validation 在 `tools.rs:425-433`
  - 需验证：payload 结构是否与 paperclip 完全一致

---

## 三、可能存在的硬编码或简化实现（低优先级）

### 3.1 需要检查的实现细节

- [ ] **paperclipApiRequest** - 检查路径验证逻辑
  - 当前实现：`tools.rs:558-566` 仅检查 method 和 jsonBody 格式
  - paperclip实现：`tools.ts:620-631` 还检查 path 必须以 `/` 开头且不能包含 `..`
  - 建议：在 parrot-agent 中添加相同的路径安全检查

- [ ] **paperclipApprovalDecision** - 检查 payloadJson 解析
  - 当前实现：`tools.rs:548-557` 在 resubmit 时解析 payloadJson
  - 需验证：是否与 paperclip 的 `parseOptionalJson` 逻辑一致（`tools.ts:49-52, 593-609`）

- [ ] **paperclipCreateApproval / CreateIssue** - 检查复杂嵌套对象
  - 需验证：executionPolicy, executionWorkspaceSettings, watchdog 等嵌套对象
  - 是否在序列化时保持结构完整性

---

## 四、实施计划建议

### 阶段一：高价值基础工具（2-3天）
1. **Cases 基础 CRUD**（4个工具）
   - paperclipListCases, GetCase, CreateCase, UpdateCase
   - 优先级：⭐⭐⭐⭐⭐

2. **Routines 基础 CRUD**（4个工具）
   - paperclipListRoutines, GetRoutine, CreateRoutine, UpdateRoutine
   - 优先级：⭐⭐⭐⭐⭐

3. **Issue Document Annotations**（5个工具）
   - 全部5个工具
   - 优先级：⭐⭐⭐⭐（REST API已存在，只需MCP层）

### 阶段二：文档和标注增强（2-3天）
4. **Case Document Management**（9个工具）
   - 文档CRUD、版本管理、锁定、标注
   - 优先级：⭐⭐⭐⭐

5. **Routine 版本和标注**（6个工具）
   - 版本管理、描述标注
   - 优先级：⭐⭐⭐

6. **Labels Management**（3个工具）
   - 优先级：⭐⭐⭐

### 阶段三：高级功能（1-2天）
7. **Routine Triggers 和执行**（5个工具）
   - 优先级：⭐⭐⭐

8. **Case 关联和层级**（3个工具）
   - 优先级：⭐⭐

9. **Attachments**（4个工具）
   - 优先级：⭐⭐

10. **External Objects + File Resources**（5个工具）
    - 优先级：⭐

### 阶段四：质量保证（1天）
11. **验证现有工具实现完整性**（第二部分的6个工具）
    - 优先级：⭐⭐⭐

12. **修复硬编码和简化实现**（第三部分的3个工具）
    - 优先级：⭐⭐

---

## 五、技术实施要点

### 5.1 添加新工具的标准流程

1. **在 `paperclip_builtin_tool_definitions()` 添加工具定义**
   - 位置：`crates/api/src/routes/tools.rs:77-119`
   - 添加到 `TOOLS` 常量数组

2. **在 match 块中添加 input_schema**
   - 位置：`tools.rs:123-319`
   - 定义 JSON Schema 验证规则

3. **在 `validate_paperclip_arguments()` 添加验证逻辑**
   - 位置：`tools.rs:347-568`
   - 添加 required 字段检查
   - 添加字段类型和格式验证

4. **在 `call_paperclip_builtin_tool()` 添加路由映射**
   - 位置：`tools.rs:1832-2184`
   - 定义 HTTP method、path 和 body 构造逻辑

5. **如有特殊逻辑，在 `direct_paperclip_service_call()` 实现**
   - 位置：需查看 `tools.rs` 中该函数定义
   - 用于需要直接调用服务层而非REST API的场景

### 5.2 从 paperclip 迁移的关键文件

- **工具定义参考**：`~/workspace/paperclip/packages/mcp-server/src/tools.ts`
- **Schema 参考**：`~/workspace/paperclip/packages/shared/src/schemas/*.ts`
- **REST API 参考**：
  - Cases: `~/workspace/paperclip/server/src/routes/cases.ts`
  - Routines: `~/workspace/paperclip/server/src/routes/routines.ts`
  - Issues: `~/workspace/paperclip/server/src/routes/issues.ts`
  - File Resources: `~/workspace/paperclip/server/src/routes/file-resources.ts`

### 5.3 数据库迁移检查

在实现工具前，需确认以下数据库表和字段已存在：

- [x] `document_annotation_threads` 表包含 `issue_id`, `routine_id`, `case_id` 列
- [x] `document_annotation_comments` 表包含对应外键列
- [ ] 检查 `cases` 表是否与 paperclip 结构一致
- [ ] 检查 `routines` 表是否与 paperclip 结构一致
- [ ] 检查 `labels`, `attachments` 等表是否存在

---

## 六、风险和注意事项

1. **API 兼容性**：parrot-agent 的 REST API 可能与 paperclip 有差异
   - 建议：逐个工具实现后进行集成测试

2. **数据库 Schema 差异**：Rust 和 TypeScript 项目的表结构可能不同
   - 建议：先运行 `sqlx migrate run` 确保迁移已应用

3. **复杂对象序列化**：嵌套的 JSON 对象（如 executionPolicy, metadata）
   - 建议：添加单元测试验证序列化正确性

4. **文件上传**：Attachments 工具涉及多部分表单
   - 建议：单独实现并测试文件上传逻辑

5. **权限检查**：某些操作可能需要特定权限
   - 建议：参考 paperclip 的 authz 逻辑确保权限一致

---

## 总结

- **完全缺失的工具**：57个（Cases 20 + Routines 15 + Issue Annotations 5 + Labels 3 + Attachments 4 + External Objects 2 + File Resources 3 + Case Relations 3 + Routine Triggers 2）
- **需验证实现的工具**：6个
- **需修复简化实现的工具**：3个
- **总计待处理**：66个工具

**预计总工作量**：7-10个工作日（按阶段实施，优先高价值工具）
