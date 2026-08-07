# 新增55个MCP工具功能分析

## 概述
这些工具都是**从paperclip的REST API扩展而来**,paperclip的MCP工具层本身**不包含**这些功能。

---

## 一、Cases管理 (21个工具)

### 用途
Cases是Paperclip的**案例/工作项管理系统**,类似于JIRA的Case或Linear的Issue,但更侧重于结构化工作流。

### 1.1 基础CRUD (4个)
- **paperclipListCases** - 列出公司的所有案例
  - 功能: 支持按类型、状态、项目、标签等过滤
  - 使用场景: Agent查看待处理的案例列表
  
- **paperclipGetCase** - 获取单个案例详情
  - 功能: 获取案例的完整信息(标题、摘要、状态、字段等)
  - 使用场景: Agent读取案例上下文
  
- **paperclipCreateCase** - 创建新案例
  - 功能: 支持指定案例类型、父案例、项目、自定义字段
  - 使用场景: Agent创建新的工作项
  
- **paperclipUpdateCase** - 更新案例
  - 功能: 修改标题、状态、字段、父案例等
  - 使用场景: Agent更新案例进度

### 1.2 案例文档管理 (8个)
- **paperclipListCaseDocuments** - 列出案例文档
- **paperclipGetCaseDocument** - 获取案例文档内容
- **paperclipUpsertCaseDocument** - 创建/更新案例文档
- **paperclipListCaseDocumentRevisions** - 列出文档历史版本
- **paperclipRestoreCaseDocumentRevision** - 恢复文档历史版本
- **paperclipDeleteCaseDocument** - 删除案例文档
- **paperclipLockCaseDocument** - 锁定文档(防止并发编辑)
- **paperclipUnlockCaseDocument** - 解锁文档

**功能**: 每个Case可以有多个文档(类似Confluence页面),支持版本控制和协作编辑
**使用场景**: Agent管理案例的技术文档、需求规格、测试计划等

### 1.3 案例文档标注 (5个)
- **paperclipListCaseDocumentAnnotations** - 列出文档标注
- **paperclipGetCaseDocumentAnnotationThread** - 获取标注线程
- **paperclipCreateCaseDocumentAnnotation** - 创建标注
- **paperclipReplyCaseDocumentAnnotation** - 回复标注
- **paperclipUpdateCaseDocumentAnnotation** - 更新标注状态

**功能**: 类似Google Docs的评论功能,可在文档特定位置创建讨论线程
**使用场景**: Agent对文档提出问题、标记需要修改的部分

### 1.4 案例关联 (4个)
- **paperclipGetCaseChildren** - 获取子案例列表
- **paperclipCreateCaseLink** - 创建案例与Issue的关联
- **paperclipGetIssueCases** - 获取Issue关联的所有案例
- **paperclipGetCaseEvents** - 获取案例事件历史

**功能**: Cases和Issues是独立的系统,通过Link关联
**使用场景**: 
- Agent将Issue的工作成果关联到Case
- 查看Case的完整工作历史

---

## 二、Routines管理 (17个工具)

### 用途
Routines是**自动化工作流/例程**,类似于GitHub Actions或Jenkins Pipeline,但更侧重于Agent驱动的自动化。

### 2.1 基础CRUD (4个)
- **paperclipListRoutines** - 列出公司的所有例程
- **paperclipGetRoutine** - 获取例程详情
- **paperclipCreateRoutine** - 创建新例程
- **paperclipUpdateRoutine** - 更新例程

**功能**: 管理自动化任务的定义(标题、描述、环境变量、负责Agent)
**使用场景**: Agent创建/管理定期执行的任务

### 2.2 版本控制 (2个)
- **paperclipListRoutineRevisions** - 列出例程历史版本
- **paperclipRestoreRoutineRevision** - 恢复历史版本

**功能**: Routine的描述(工作流定义)支持版本控制
**使用场景**: 回滚错误的例程修改

### 2.3 描述标注 (5个)
- **paperclipListRoutineDescripons** - 列出描述标注
- **paperclipGetRoutineDescriptionAnnotationThread** - 获取标注线程
- **paperclipCreateRoutineDescriptionAnnotation** - 创建标注
- **paperclipReplyRoutineDescriptionAnnotation** - 回复标注
- **paperclipUpdateRoutineDescriptionAnnotation** - 更新标注

**功能**: 在Routine描述上添加注释和讨论
**使用场景**: Agent对工作流步骤提出改进建议

### 2.4 触发器管理 (4个)
- **paperclipCreateRoutineTrigger** - 创建触发器
- **paperclipUpdateRoutineTrigger** - 更新触发器
- **paperclipDeleteRoutineTrigger** - 删除触发器
- **paperclipRotateRoutineTriggerSecret** - 轮换触发器密钥

**功能**: 配置何时执行Routine(定时、Webhook、事件触发)
**使用场景*、PR触发的测试等

### 2.5 执行管理 (2个)
- **paperclipListRoutineRuns** - 列出例程执行历史
- **paperclipRunRoutine** - 手动执行例程

**功能**: 查看和触发Routine执行
**使用场景**: Agent手动触发部署、查看执行结果

---

## 三、Issue文档标注 (5个工具)

- **paperclipListIssueDocumentAnnotations** - 列出Issue文档标注
- **paperclipGetIssueDocumentAnnotationThread** - 获取标注线程
- **paperclipCreateIssueDocumentAnnotation** - 创建标注
- **paperclipReplyIssueDocumentAnnotation** - 回复标注
- **paperclipUpdateIssueDocumentAnnotation** - 更新标注

**功能**: 在Issue文档上添加行内评论和讨论
**使用场景**: Agent对需求文档、设计文档标记问题和建议

---

## 四、Labels标签 (3个工具)

- **paperclipListLabels** - 列出公司的所有标签
- **paperclipCreateLabel** - 创建新标签
- **paperclipDeleteLabel** - 删除标签

**功能**: 管理Issues和Cases的标签体系(类似GitHub Labels)
**使用场景**: Agent创建"bug"、"feature"等分类标签

---

## 五、Attachments附件 (4个工具)

- **paperclipListIssueAttachments** - 列出Issue附件
- **paperclipCreateIssueAttachment** - 上传附件(base64编码)
- **paperclipGetAttachmentContent** - 下载附件内容
- **paperclipDeleteAttachment** - 删除附件

**功能**: 管理Issue的文件附件(截图、日志、设计稿等)
**使用场景**: Agent上传错误日志、截图等辅助材料

---

## 六、External Objects外部对象 (2个工具)

- **paperclipListIssueExternalObjects** - 列出Issue关联的外部对象
- **paperclipRefreshIssueExternalObjects** - 刷新外部对象数据

**功能**: Issue可以关联外部系统的对象(JIRA ticket、GitHub PR、Slack thread等)
**使用场景**: Agent查看Issue关联的外部资源、刷新同步状态

---

## 七、File Resources文件资源 (3个工具)

- **paperclipListIssueFileResources** - 列出Issue文件资源
- **paperclipResolveIssueFileResource** - 解析文件资源路径
- **paperclipGetIssueFileResourceContent** - 获取文件内容

**功能**: 访问Issue执行环境中的文件(workspace文件、构建产物等)
**使用场景**: Agent读取测试结果、构建日志等

---

## 总结对比

| 功能域 | 工具数 | paperclip MCP | paperclip REST | parrot MCP | 使用频率 |
|--------|--------|---------------|----------------|------------|----------|
| **Cases** | 21 | ❌ | ✅ | ✅ | 🔥🔥🔥 高 |
| **Routines** | 17 | ❌ | ✅ | ✅ | 🔥🔥 中高 |
| **Issue标注** | 5 | ❌ | ✅ | ✅ | 🔥 中 |
| **Labels** | 3 | ❌ | ✅ | ✅ | 🔥 中 |
| **Attachments** | 4 | ❌ | ✅ | ✅ | 🔥 中 |
| **External Objects** | 2 | ❌ | ✅ | ✅ | 🔥 低 |
| **File Resources** | 3 | ❌ | ✅ | ✅ | 🔥 低 |

---

## 关键判断

### 应该保留为MCP工具的(推荐)

#### 🟢 高优先级 - Cases基础CRUD (4个)
理由: Agent需要直接操作Cases作为工作项管理的核心能力

#### 🟢 高优先级 - Routines基础CRUD + 执行 (6个)
理由: Agent自动化工作流管理是核心场景

#### 🟡 中优先级 - Issue/Case文档标注 (10个)
理由: Agent协作和反馈能力,但使用频率可能不高

#### 🟡 中优先级 - Labels (3个)
理由: 分类管理的基础能力

#### 🟡 中优先级 - Attachments (4个)
理由: 上传截图、日志等辅助材料

### 可以降级为REST API的(可选)

#### 🔴 低优先级 - Case文档管理 (8个)
理由: 文档版本控制是高级功能,Agent使用场景有限

#### 🔴 低优先级 - Routine触发器管理 (4个)
理由: 触发器配置通常是一次性设置,不需要频繁调用

#### 🔴 低优先级 - Routine版本控制 (2个)
理由: 版本回滚是低频操作

#### 🔴 低优先级 - External Objects (2个)
理由: 外部系统集成是边缘功能

#### 🔴 低优先级 - File Resources (3个)
理由: 文件访问可以通过workspace runtime实现

---

## 建议方案

### 方案A: 全部保留(当前实现)
- 优点: Agent能力最全面
- 缺点: MCP工具列表过长(96个),认知负担重

### 方案B: 精简到核心26个
保留:
- Cases基础CRUD (4个)
- Routines基础CRUD + 执行 (6个)
- Issue标注 (5个)
- Labels (3个)
- Attachments (4个)
- Cases关联 (4个)

移除29个低频工具,通过`paperclipApiRequest`兜底

### 方案C: 分阶段推进
1. **Phase 1**: 先实现Cases + Routines基础(10个)
2. **Phase 2**: 根据实际使用情况增加标注、附件等(16个)
3. **Phase 3**: 按需添加高级功能(29个)

---

## 推荐决策

**建议采用方案B(精简到26个核心工具)**

理由:
1. **认知负担**: 96个工具太多,Agent难以正确选择
2. **使用频率**: 文档版本控制、触发器管理等是低频操作
3. **兜底机制**: `paperclipApiRequest`可以覆盖所有REST API
4. **渐进增强**: 未来发现高频需求再提升为MCP工具

保留的26个工具覆盖:
- ✅ Cases核心工作流(创建、查询、更新、关联)
- ✅ Routines自动化管理(定义、执行、历史)
- ✅ 协作反馈(标注、标签)
- ✅ 附件上传(截图、日志)
