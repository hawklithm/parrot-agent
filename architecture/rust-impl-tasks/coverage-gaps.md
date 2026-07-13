# Rust 实现任务覆盖缺口分析

> 基于 7 个后端架构文档与对应 rust-impl-tasks 的对比分析结果
> 生成日期: 2026/07/11

---

## 执行摘要

| 模块 | 架构端点数 | 任务覆盖率 | 缺失端点数 | 严重程度 |
|------|-----------|----------|-----------|---------|
| **实时通信与执行环境** | 51 | 51% | **26** | 🔴 高 |
| **认证授权** | 55 | 80% | **11** | 🟠 中 |
| **Routine/Goal 自动化** | 35 | 89% | **4** | 🟡 低 |
| **Company/Org 组织** | 69 | 97% | **2** | 🟡 低 |
| **Pipeline/Adapter** | 39 | 100% | **0** | ✅ 完整 |
| **Agent 管理** | 24+ | 100% | **0** | ✅ 完整 |
| **Issue/Case 管理** | 70+ | 100% | **0** | ✅ 完整 |

**总计缺失**: **43 个端点/功能**

---

## 🔴 高优先级缺口（实时通信与执行环境模块）

### 1. User Secret Definitions 系统（11 个端点）- 完全缺失

**影响**: 无法支持用户级别的密钥管理，仅支持公司级密钥

**缺失端点**:
```
GET    /companies/:companyId/user-secret-definitions
POST   /companies/:companyId/user-secret-definitions
PATCH  /companies/:companyId/user-secret-definitions/:definitionId
DELETE /companies/:companyId/user-secret-definitions/:definitionId
GET    /companies/:companyId/user-secret-definitions/:definitionId/coverage
GET    /companies/:companyId/me/user-secrets
POST   /companies/:companyId/me/user-secrets
PATCH  /companies/:companyId/me/user-secrets/:secretId
POST   /companies/:companyId/me/user-secrets/:secretId/rotate
DELETE /companies/:companyId/me/user-secrets/:secretId
GET    /secrets/:secretId/bindings
```

**需要新增的任务**:
- [x] **定义 UserSecretDefinition 数据模型**
  - 定义 `UserSecretDefinition` 结构体（id, company_id, key, description, required, scope）
  - 定义 `UserSecret` 结构体（id, user_id, definition_id, encrypted_value）
  - 实现 user_secret_definitions 表 migration

- [x] **实现 UserSecretRepository**
  - 实现 `create_definition()` / `list_definitions()` / `get_definition()`
  - 实现 `update_definition()` / `delete_definition()`
  - 实现 `get_coverage_stats()` - 统计用户填写率

- [x] **实现用户密钥 CRUD API**
  - 实现 POST `/companies/:companyId/user-secret-definitions` - 创建定义
  - 实现 GET `/companies/:companyId/user-secret-definitions` - 列出定义
  - 实现 PATCH/DELETE 定义端点
  - 实现 GET `/companies/:companyId/me/user-secrets` - 当前用户密钥列表
  - 实现 POST/PATCH/DELETE `/me/user-secrets` 端点

### 2. Secret Provider Configuration 系统（9 个端点）- 管理层缺失

**影响**: 无法动态配置外部密钥提供商（AWS/GCP/Vault），仅能使用硬编码配置

**缺失端点**:
```
GET    /companies/:companyId/secret-provider-configs
POST   /companies/:companyId/secret-provider-configs/discovery/preview
POST   /companies/:companyId/secret-provider-configs
GET    /secreconfigs/:id
PATCH  /secret-provider-configs/:id
DELETE /secret-provider-configs/:id
POST   /secret-provider-configs/:id/default
POST   /secret-provider-configs/:id/health
GET    /companies/:companyId/secret-providers/health
```

**需要新增的任务**:
- [x] **定义 SecretProviderConfig 数据模型**
  - 定义 `SecretProviderConfig` 结构体（id, company_id, provider_type, config, is_default）
  - 实现 secret_provider_configs 表 migration
  - 定义 `ProviderHealthStatus` 枚举

- [x] **实现 SecretProviderConfigService**
  - 实现 `create_config()` - 创建提供商配置
  - 实现 `discover_secrets_preview()` - 预览外部密钥发现
  - 实现 `test_provider_health()` - 健康检查
  - 实现 `set_default_provider()` - 设置默认提供商

- [x] **实现 Provider Config API 路由**
  - 实现所有 9 个缺失端点
  - 集成权限校验（assertCanManageSecrets）
  - 实现配置加密存储

### 3. 环境核心诊断端点（3 个）

**缺失端点**:
```
POST /environments/:id/probe              # 环境探测
POST /environments/:id/acquire            # 获取租约
GET  /environments/:id/delete-blast-radius # 删除影响分析
```

**需要新增的任务**:
- [x] **实现环境探测 API**
  - 实现 POST `/environments/:id/probe` 路由
  - 调用 `EnvironmentDriver::probe()` 并返回诊断结果
  - 添加探测超时控制

- [x] **实现租约获取 API**
  - 实现 POST `/environments/:id/acquire` 路由
  - 集成 LeaseService 并返回租约 token
  - 实现租约冲突检测

- [x] **实现删除影响分析 API**
  - 实现 GET `/environments/:id/delete-blast-radius` 路由
  - 查询依赖此环境的 workspaces 数量
  - 返回影响的 agents/issues 列表

### 4. Secret Remote Import（2 个端点）

**缺失端点**:
```
POST /companies/:companyId/secrets/remote-import/preview
POST /companies/:companyId/secrets/remote-import
```

**需要新增的任务**:
- [x] **实现批量密钥导入**
  - 实现 `remote_import_preview()` - 预览导入（匹配规则、冲突检测）
  - 实现 `remote_import_execute()` - 执行导入（批量创建、去重）
  - 添加导入审计日志

### 5. Custom Image Setup Session 管理（2 个端点）

**缺失端点**:
```
POST /environment-custom-image-setup-sessions/:sessionId/terminal-session-token
GET  /environment-custom-image-setup-sessions/:sessionId
```

**需要新增的任务**:
- [x] **实现 Setup Session 查询**
  - 实现 GET `/setup-sessions/:sessionId` 端点
  - 返回 session 状态、进度、错误信息

- [x] **实现终端会话 Token 生成**
  - 实现 POST `/:sessionId/terminal-session-token` 端点
  - 生成短期 JWT token 用于 WebSocket 认证
  - 集成 terminal proxy 权限校验

---

## 🟠 中优先级缺口（认证授权模块）

### 1. Skills 系统端点（3 个）- 完全缺失

**影响**: 前端无法获取可用技能列表和详情

**缺失端点**:
```
GET /api/skills/available
GET /api/skills/index
GET /api/skills/:skillName
```

**需要新增的任务**:
- [x] **实现 Skills Registry Service**
  - 实现 `list_available_skills()` - 列出可用技能
  - 实现 `get_skill_index()` - 获取技能索引（元数据）
  - 实现 `get_skill_details()` - 获取技能详情（含用法示例）

- [x] **实现 Skills API 路由**
  - 实现 GET `/api/skills/available` - 公开访问
  - 实现 GET `/api/skills/index` - 需认证
  - 实现 GET `/api/skills/:skillName` - 需认证

### 2. Invite 子资源端点（5 个）

**缺失端点**:
```
GET /api/invites/:token/logo
GET /api/invites/:token/onboarding
GET /api/invites/:token/onboarding.txt
GET /api/invites/:token/skills/index
GET /api/invites/:token/skills/:skillName
```

**需要新增的任务**:
- [x] **实现 Invite 资源端点**
  - 实现 GET `/invites/:token/logo` - 返回公司 Logo
  - 实现 GET `/invites/:token/onboarding` - 返回 onboarding 文档（Markdown）
  - 实现 GET `/invites/:token/onboarding.txt` - 返回纯文本版本
  - 实现 GET `/invites/:token/skills/index` - 邀请范围内的技能索引
  - 实现 GET `/invites/:token/skills/:skillName` - 技能详情

### 3. OpenClaw 端点（1 个）

**缺失端点**:
```
POST /api/companies/:companyId/openclaw/invite-prompt
```

**需要新增的任务**:
- [x] **实现 OpenClaw Invite 提示生成**
  - 实现 POST `/companies/:companyId/openclaw/invite-prompt` 端点
  - 生成个性化邀请提示文本（基于公司配置）
  - 集成权限校验（assertCanManageMembers）

### 4. 其他缺失端点（2 个）

**缺失端点**:
```
GET /api/companies/:companyId/user-directory
GET /api/admin/users
```

**需要新增的任务**:
- [x] **实现用户目录查询**
  - 实现 GET `/companies/:companyId/user-directory` - 公司用户目录（含搜索）
  - 支持分页、过滤（按角色、状态）

- [x] **实现实例管理员用户
  - 实现 GET `/api/admin/users` - 列出所有用户（分页）
  - 添加搜索过滤（邮箱、用户名）
  - 仅实例管理员可访问

---

## 🟡 低优先级缺口

### Routine/Goal 模块（4 个端点）

**缺失的 Routine 文档注释系统**:
```
GET   /routines/:id/description/annotations
POST  /routines/:id/description/annotations
POST  /routines/:id/description/annotations/:threadId/comments
PATCH /routines/:id/description/annotations/:threadId
```

**需要新增的任务**:
- [x] **定义 Annotation 数据模型**
  - 定义 `AnnotationThread` 结构体（id, routine_id, position, status, created_by）
  - 定义 `AnnotationComment` 结构体（id, thread_id, body, author_id）
  - 实现 annotation_threads 表 migration

- [x] **实现 Annotation API**
  - 实现 4 个缺失端点
  - 集成权限校验（routine 读/写权限）
  - 实现 thread 状态流转（open/resolved）

### Company/Org 模块（2 个端点）

**缺失的组织图生成**:
```
GET /companies/:companyId/org-chart.svg
```

**需要新增的任务**:
- [x] **实现组织架构图生成**
  - 定义 `OrgNode` 结构体（id, name, role, reports）
  - 实现 Agent 层级树遍历（基于 reportsTo 字段）
  - 实现 SVG 渲染引擎（使用 `svg` crate）
  - 实现 GET `/companies/:companyId/org-chart.svg` 端点
  - 处理循环引用检测

**缺失的公司统计详细逻辑**:
```
GET /stats (实现细节不足)
```

**需要补充任务**:
- [x] **完善公司统计聚合逻辑**
  - 统计 agent 数量（按角色分组）
  - 统计 project 数量、issue 数量
  - 统计预算使用率、成本趋势
  - 添加缓存机制（Redis）

---

## ✅ 已完整覆盖的模块

### Pipeline/Adapter 模块
- 覆盖率: **100%**
- 所有 39 个端点均有对应任务
- 唯一注意点: 任务中提到的实现细节（drift detection、变量注入等）需在开发中补充

### Agent 管理模块
- 覆盖率: **100%**
- 所有 24+ 个端点均有对应任务
- 配置版本控制、权限校验等高级特性均已覆盖

### Issue/Case 管理模块
- 覆盖率: **100%**
- 所有 70+ 个端点均有对应任务
- 树形控制、文档注释、审批流等复杂特性均已覆盖

---

## 推荐修复计划

### 第一阶段（必须）- 实时通信与执行环境缺口
1. 补充 User Secret Definitions 系统（11 个端点）
2. 补充 Secret Provider Configuration 系统（9 个端点）
3. 补充环境核心诊断端点（3 个）

**优先级理由**: 这些是核心安全和运维功能，缺失会导致系统不可用

### 第二阶段（重要）- 认证授权缺口
1. 补充 Skills 系统端点（3 个）
2. 补充 Invite 子资源端点（5 个）
3. 补充 OpenClaw 和管理端点（3 个）

**优先级理由**: 影响用户注册流程和技能发现体验

### 第三阶段（可选）- 低优先级缺口
1. 补充 Routine 文档注释系统（4 个端点）
2. 补充组织架构图生成（1 个端点）
3. 完善公司统计逻辑

**优先级理由**: 这些是辅助功能，不影响核心流程

---

## 建议的任务文档更新

### 1. 创建新文档: `realtime-environment-gaps.md`
包含上述 26 个缺失端点的详细任务拆解

### 2. 更新 `thorization-tasks.md`
在现有 Phase 3 后添加 Phase 4: "Skills 系统与 Invite 资源"

### 3. 更新 `routine-goal-tasks.md`
在 Section 10 Phase 3 后添加 Phase 4: "文档注释系统"

### 4. 更新 `company-org-tasks.md`
在现有 Phase 4.5 后添加 Phase 4.6: "组织架构可视化"

---

*分析完成于: 2026/07/11*
*基于 7 个后端架构文档和 7 个 rust-impl-tasks 文档的对比*
