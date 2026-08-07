# MCP工具对齐分析与调整任务

## 📊 现状总结

### paperclip架构
- **MCP工具层**: 41个工具（packages/mcp-server/src/tools.ts）
- **REST API层**: 完整的Cases、Routines、Labels、Attachments等REST端点

### parrot-agent现状
- **原有MCP工具**: 41个（与paperclip MCP层一致）
- **新增MCP工具**: 55个（从paperclip REST API扩展）
- **总计**: 96个MCP工具

## 🔍 关键发现

**所有新增的55个工具都不在paperclip的MCP工具层中，它们对应的是paperclip的REST API功能。**

这意味着：
- ✅ paperclip通过41个MCP工具 + 直接REST API调用来提供完整功能
- ❌ parrot-agent将REST API功能包装成了55个额外的MCP工具

## 📋 新增工具分类与状态

### 1. Cases管理 (21个工具)

**paperclip实现**: REST API端点在 `server/src/routes/cases.ts`  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加21个MCP工具

#### 基础CRUD (4个)
- [ ] **paperclipListCases** 
  - paperclip: REST `GET /companies/:companyId/cases`
  - 建议: **保留MCP工具** - 列表查询是常用操作
  
- [ ] **paperclipGetCase**
  - paperclip: REST `GET /cases/:id`
  - 建议: **保留MCP工具** - 单个查询是常用操作
  
- [ ] **paperclipCreateCase**
  - paperclip: REST `POST /companies/:companyId/cases`
  - 建议: **保留MCP工具** - 创建操作是常用操作
  
- [ ] **paperclipUpdateCase**
  - paperclip: REST `PATCH /cases/:id`
  - 建议: **保留MCP工具** - 更新操作是常用操作

#### 文档管理 (8个)
- [ ] **paperclipListCaseDocuments**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 列表查询常用
  
- [ ] **paperclipGetCaseDocument**
  - paperclip: REST `GET /cases/:id/documents/:key`
  - 建议: **保留MCP工具** - 文档读取常用
  
- [ ] **paperclipUpsertCaseDocument**
  - paperclip: REST `PUT /cases/:id/documents/:key`
  - 建议: **保留MCP工具** - 文档编辑常用
  
- [ ] **paperclipListCaseDocumentRevisions**
  - paperclip: REST `GET /cases/:id/documents/:key/revisions`
  - 建议: **保留MCP工具** - 版本历史查看常用
  
- [ ] **paperclipRestoreCaseDocumentRevision**
  - paperclip: REST `POST /cases/:id/documents/:key/revisions/:revisionId/restore`
  - 建议: **保留MCP工具** - 版本恢复是重要功能
  
- [ ] **paperclipDeleteCaseDocument**
  - paperclip: REST `DELETE /cases/:id/documents/:key`
  - 建议: **保留MCP工具** - 删除操作常用
  
- [ ] **paperclipLockCaseDocument**
  - paperclip: REST `POST /cases/:id/documents/:key/lock`
  - 建议: **保留MCP工具** - 协作编辑需要
  
- [ ] **paperclipUnlockCaseDocument**
  - paperclip: REST `POST /cases/:id/documents/:key/unlock`
  - 建议: **保留MCP工具** - 协作编辑需要
  
- [ ] **paperclipGetCaseEvents**
  - paperclip: REST `GET /cases/:id/events`
  - 建议: **保留MCP工具** - 事件历史查询有用

#### 文档标注 (5个)
- [ ] **paperclipListCaseDocumentAnnotations**
  - paperclip: REST `GET /cases/:id/documents/:key/annotations`
  - 建议: **可选保留** - 如果案例文档协作重要则保留
  
- [ ] **paperclipGetCaseDocumentAnnotationThread**
  - paperclip: REST `GET /cases/:id/documents/:key/annotations/:threadId`
  - 建议: **可选保留** - 标注详情查看
  
- [ ] **paperclipCreateCaseDocumentAnnotation**
  - paperclip: REST `POST /cases/:id/documents/:key/annotations`
  - 建议: **可选保留** - 标注创建
  
- [ ] **paperclipReplyCaseDocumentAnnotation**
  - paperclip: REST `POST /cases/:id/documents/:key/annotations/:threadId/reply`
  - 建议: **可选保留** - 标注回复
  
- [ ] **paperclipUpdateCaseDocumentAnnotation**
  - paperclip: REST `PATCH /cases/:id/documents/:key/annotations/:threadId`
  - 建议: **可选保留** - 标注更新

#### 关联管理 (3个)
- [ ] **paperclipGetCaseChildren**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 层级关系查询常用
  
- [ ] **paperclipCreateCaseLink**
  - paperclip: REST `POST /cases/:id/links`
  - 建议: **保留MCP工具** - Issue-Case关联是核心功能
  
- [ ] **paperclipGetIssueCases**
  - paperclip: REST `GET /issues/:issueId/cases`
  - 建议: **保留MCP工具** - 反向查询常用

---

### 2. Routines管理 (17个工具)

**paperclip实现**: REST API端点在 `server/src/routes/routines.ts`  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加17个MCP工具

#### 基础CRUD (4个)
- [ ] **paperclipListRoutines**
  - paperclip: REST `GET /companies/:companyId/routines`
  - 建议: **保留MCP工具** - 列表查询常用
  
- [ ] **paperclipGetRoutine**
  - paperclip: REST `GET /routines/:id`
  - 建议: **保留MCP工具** - 单个查询常用
  
- [ ] **paperclipCreateRoutine**
  - paperclip: REST `POST /companies/:companyId/routines`
  - 建议: **保留MCP工具** - 创建自动化流程是核心功能
  
- [ ] **paperclipUpdateRoutine**
  - paperclip: REST `PATCH /routines/:id`
  - 建议: **保留MCP工具** - 更新流程定义常用

#### 版本管理 (2个)
- [ ] **paperclipListRoutineRevisions**
  - paperclip: REST `GET /routines/:id/revisions`
  - 建议: **保留MCP工具** - 版本历史查看有用
  
- [ ] **paperclipRestoreRoutineRevision**
  - paperclip: REST `POST /routines/:id/revisions/:revisionId/restore`
  - 建议: **保留MCP工具** - 版本回滚是重要功能

#### 描述文档标注 (5个)
- [ ] **paperclipListRoutineDescriptionAnnotations**
  - paperclip: REST `GET /routines/:id/description/annotations`
  - 建议: **可移除** - 非核心功能，使用频率低
  
- [ ] **paperclipGetRoutineDescriptionAnnotationThread**
  - paperclip: REST `GET /routines/:id/description/annotations/:threadId`
  - 建议: **可移除** - 非核心功能
  
- [ ] **paperclipCreateRoutineDescriptionAnnotation**
  - paperclip: REST `POST /routines/:id/description/annotations`
  - 建议: **可移除** - 非核心功能
  
- [ ] **paperclipReplyRoutineDescriptionAnnotation**
  - paperclip: REST `POST /routines/:id/description/annotations/:threadId/reply`
  - 建议: **可移除** - 非核心功能
  
- [ ] **paperclipUpdateRoutineDescriptionAnnotation**
  - paperclip: REST `PATCH /routines/:id/description/annotations/:threadId`
  - 建议: **可移除** - 非核心功能

#### 触发器管理 (4个)
- [ ] **paperclipCreateRoutineTrigger**
  - paperclip: REST `POST /routines/:id/triggers`
  - 建议: **保留MCP工具** - 配置触发器是核心功能
  
- [ ] **paperclipUpdateRoutineTrigger**
  - paperclip: REST `PATCH /routine-triggers/:id`
  - 建议: **保留MCP工具** - 更新触发器配置常用
  
- [ ] **paperclipDeleteRoutineTrigger**
  - paperclip: REST `DELETE /routine-triggers/:id`
  - 建议: **保留MCP工具** - 删除触发器常用
  
- [ ] **paperclipRotateRoutineTriggerSecret**
  - paperclip: REST `POST /routine-triggers/:id/rotate-secret`
  - 建议: **保留MCP工具** - Webhook密钥轮换是安全功能

#### 执行管理 (2个)
- [ ] **paperclipListRoutineRuns**
  - paperclip: REST `GET /routines/:id/runs`
  - 建议: **保留MCP工具** - 查看执行历史常用
  
- [ ] **paperclipRunRoutine**
  - paperclip: REST `POST /routines/:id/run`
  - 建议: **保留MCP工具** - 手动触发执行是核心功能

---

### 3. Issue Document Annotations (5个工具)

**paperclip实现**: REST API端点在 `server/src/routes/issues.ts`  
**paperclip MCP层**: ❌ 无MCP工具（但已有paperclipListDocuments等文档相关工具）  
**parrot-agent实现**: ✅ 已添加5个MCP工具

- [ ] **paperclipListIssueDocumentAnnotations**
  - paperclip: REST `GET /issues/:id/documents/:key/annotations`
  - 建议: **保留MCP工具** - 与已有的文档工具配套，协作场景常用
  
- [ ] **paperclipGetIssueDocumentAnnotationThread**
  - paperclip: REST `GET /issues/:id/documents/:key/annotations/:threadId`
  - 建议: **保留MCP工具** - 标注详情查看
  
- [ ] **paperclipCreateIssueDocumentAnnotation**
  - paperclip: REST `POST /issues/:id/documents/:key/annotations`
  - 建议: **保留MCP工具** - 标注创建
  
- [ ] **paperclipReplyIssueDocumentAnnotation**
  - paperclip: REST `POST /issues/:id/documents/:key/annotations/:threadId/reply`
  - 建议: **保留MCP工具** - 标注回复
  
- [ ] **paperclipUpdateIssueDocumentAnnotation**
  - paperclip: REST `PATCH /issues/:id/documents/:key/annotations/:threadId`
  - 建议: **保留MCP工具** - 标注状态更新（如resolved）

---

### 4. Labels (3个工具)

**paperclip实现**: REST API端点在 `server/src/routes/issues.ts`  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加3个MCP工具

- [ ] **paperclipListLabels**
  - paperclip: REST `GET /companies/:companyId/labels`
  - 建议: **保留MCP工具** - 标签查询常用于分类和筛选
  
- [ ] **paperclipCreateLabel**
  - paperclip: REST `POST /companies/:companyId/labels`
  - 建议: **保留MCP工具** - 创建标签是常用管理操作
  
- [ ] **paperclipDeleteLabel**
  - paperclip: REST `DELETE /labels/:labelId`
  - 建议: **保留MCP工具** - 删除标签是常用管理操作

---

### 5. Attachments (4个工具)

**paperclip实现**: REST API端点在 `server/src/routes/issues.ts`  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加4个MCP工具

- [ ] **paperclipListIssueAttachments**
  - paperclip: REST `GET /issues/:id/attachments`
  - 建议: **保留MCP工具** - 查看附件列表常用
  
- [ ] **paperclipCreateIssueAttachment**
  - paperclip: REST `POST /companies/:companyId/issues/:issueId/attachments`
  - 建议: **保留MCP工具** - 上传附件是重要功能
  
- [ ] **paperclipGetAttachmentContent**
  - paperclip: REST `GET /attachments/:attachmentId/content`
  - 建议: **保留MCP工具** - 下载/查看附件常用
  
- [ ] **paperclipDeleteAttachment**
  - paperclip: REST `DELETE /attachments/:attachmentId`
  - 建议: **保留MCP工具** - 删除附件常用

---

### 6. External Objects (2个工具)

**paperclip实现**: REST API端点存在  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加2个MCP工具

- [ ] **paperclipListIssueExternalObjects**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 查看外部对象集成常用
  
- [ ] **paperclipRefreshIssueExternalObjects**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 同步外部数据常用

---

### 7. File Resources (3个工具)

**paperclip实现**: REST API端点存在  
**paperclip MCP层**: ❌ 无MCP工具  
**parrot-agent实现**: ✅ 已添加3个MCP工具

- [ ] **paperclipListIssueFileResources**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - workspace文件访问是开发场景常用
  
- [ ] **paperclipResolveIssueFileResource**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 路径解析对开发场景有用
  
- [ ] **paperclipGetIssueFileResourceContent**
  - paperclip: REST API存在
  - 建议: **保留MCP工具** - 读取文件内容是开发场景核心功能

---

## 🎯 总体建议

### 策略A: 保守策略 - 完全对齐paperclip
**移除所有55个新增工具，仅保留paperclip MCP层的41个工具**

- ✅ 优点: 与paperclip架构完全一致
- ❌ 缺点: Agent需要直接调用REST API或使用paperclipApiRequest

### 策略B: 激进策略 - 全部保留
**保留所有96个MCP工具**

- ✅ 优点: Agent使用更方便，功能完整
- ❌ 缺点: 与paperclip架构差异大，维护成本高

### 策略C: 折中策略 - 保留核心功能 (推荐)
**保留高频使用的MCP工具，移除低频标注类工具**

保留建议 (45个新工具):
- ✅ Cases基础CRUD (4个) + 文档管理 (8个) + 关联管理 (3个) = **15个**
- ✅ Routines基础CRUD (4个) + 版本管理 (2个) + 触发器 (4个) + 执行 (2个) = **12个**
- ✅ Issue Document Annotations 全部 = **5个**
- ✅ Labels 全部 = **3个**
- ✅ Attachments 全部 = **4个**
- ✅ External Objects 全部 = **2个**
- ✅ File Resources 全部 = **3个**
- ✅ Case Document Annotations 全部 = **5个**（虽然低频但与Issue Annotations对称）

移除建议 (10个低频工具):
- ❌ Routine Description Annotations (5个) - 使用频率极低
- ❌ 可选：Case Document Annotations (5个) - 如果案例协作不是核心场景

**折中策略后**: 41 (原有) + 45 (保留) = **86个MCP工具**

---

## 📝 执行检查清单

### Phase 1: 决策
- [ ] 与团队讨论确定采用哪个策略
- [ ] 确定最终保留/移除的工具列表

### Phase 2: 代码调整 (如需要)
- [ ] 从tools.rs的TOOLS数组移除不需要的工具定义
- [ ] 从input_schema match块移除对应schema
- [ ] 从required验证match块移除验证规则
- [ ] 从字符串验证列表移除相关字段
- [ ] 从路由映射match块移除路由定义
- [ ] 运行 `cargo check` 确保编译通过

### Phase 3: 文档更新
- [ ] 更新next_task.md反映最终工具列表
- [ ] 更新工具使用文档
- [ ] 创建变更日志说明调整原因

### Phase 4: 测试验证
- [ ] 验证保留的工具功能正常
- [ ] 确认移除的工具不影响现有功能
- [ ] 进行git commit

---

## 💡 额外发现

1. **paperclip的设计哲学**: 
   - MCP工具层只包含**最高频使用**的41个工具
   - 其他功能通过`paperclipApiRequest`工具动态调用REST API
   - 这种设计减少了MCP工具维护成本

2. **parrot-agent的当前实现**:
   - 将大量REST API功能包装成MCP工具
   - 提供了更友好的Agent使用体验
   - 但增加了维护成本和与paperclip的差异

3. **建议的最佳实践**:
   - **高频核心功能** → MCP工具（如CRUD、列表查询）
   - **低频辅助功能** → 通过paperclipApiRequest调用REST API
   - **标注类协作功能** → 根据实际使用场景决定

---

## 📊 统计摘要

| 类别 | 新增工具数 | 建议保留 | 建议移除 | 理由 |
|------|-----------|---------|---------|------|
| Cases管理 | 21 | 15-20 | 0-5 | 核心功能，建议保留基础+文档+关联，标注类可选 |
| Routines管理 | 17 | 12 | 5 | 保留核心功能，移除Description Annotations |
| Issue Annotations | 5 | 5 | 0 | 与已有文档工具配套，协作常用 |
| Labels | 3 | 3 | 0 | 标签管理是常用功能 |
| Attachments | 4 | 4 | 0 | 附件是常用功能 |
| External Objects | 2 | 2 | 0 | 外部集成常用 |
| File Resources | 3 | 3 | 0 | 开发场景常用 |
| **总计** | **55** | **45-50** | **5-10** | **折中策略** |
