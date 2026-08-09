# Agent 创建问题根因分析

## 问题描述
使用 task 和 issue 的方式创建 agent，但最后似乎没有完成创建。

## 当前发现

### 1. Paperclip MCP 工具清单
根据 `docs/paperclip-mcp-migration-plan.md`，当前已迁移 **41 个工具**，但清单中**没有包含 agent 创建相关的工具**：

已迁移工具分类：
- ✅ 身份与 Agent（4个）：`paperclipMe`, `paperclipInboxLite`, `paperclipListAgents`, `paperclipGetAgent`
- ✅ Issue 与执行（10个）
- ✅ Documents（6个）
- ✅ Project、Goal 与 Approval（11个）
- ✅ Execution Workspace（3个）
- ✅ Escape hatch（1个）：`paperclipApiRequest`

**缺失的工具**：
- ❌ `paperclipCreateAgent` - 创建 Agent
- ❌ `paperclipUpdateAgent` - 更新 Agent
- ❌ `paperclipCreateAgentTeam` 或类似的团队创建工具

### 2. Parrot-Agent 现有能力

#### Agent Service 已实现
`crates/services/src/agent_service.rs` 已经有完整的 Agent CRUD 接口：
```rust
pub trait AgentService {
    async fn create(&self, input: CreateAgentInput) -> Result<Agent, ServiceError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Agent, ServiceError>;
    async fn list(&self, company_id: Uuid) -> Result<Vec<NormalizedAgentRow>, ServiceError>;
    async fn update(&self, id: Uuid, input: UpdateAgentInput) -> Result<Agent, ServiceError>;
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;
    // ... 更多方法
}
```

#### CreateAgentInput 结构
```rust
pub struct CreateAgentInput {
    pub company_id: Uuid,
    pub name: String,
    pub role: AgentRole,
    pub status: Option<AgentStatus>,
    pub adapter_type: String,
    pub adapter_config: serde_json::Value,
    pub runtime_config: Option<serde_json::Value>,
    pub permissions: Option<AgentPermissions>,
    pub budget_monthly_cents: Option<i32>,
    pub reports_to: Option<Uuid>,
}
```

#### 数据库支持
- ✅ `migrations/20260711000001_create_agents.sql` - agents 表已存在
- ✅ `migrations/20260808000004_create_agent_memberships.sql` - agent_memberships 表已存在

### 3. 问题根因

**Agent 无法通过 MCP 工具创建的原因**：
1. **MCP 工具缺失**：当前 41 个已迁移的 Paperclip 工具中，没有 `paperclipCreateAgent` 或类似的工具
2. **只能通过 REST API 创建**：Agent 创建功能存在，但只能通过直接调用 REST API，不能通过 MCP 工具
**Issue 方式间接创建**：用户可能尝试通过创建 Issue 并让 Agent 处理的方式来"创建" Agent，但这个流程不会真正调用 Agent 创建 API

### 4. Paperclip 原始实现调查

需要确认：
1. Paperclip 的 `packages/mcp-server/src/tools.ts` 是否真的有 agent 创建工具？
2. 还是 Paperclip 也不支持通过 MCP 创建 Agent，只能通过 UI/REST API？

## 下一步行动

1. ✅ 读取 Paperclip 的 `tools.ts` 确认是否有 agent 创建工具
2. 如果有，分析其实现方式和参数
3. 在 parrot-agent 中实现对应的 MCP 工具
4. 更新工具注册表，将工具数量从 41 增加到包含 agent 创建工具
5. 添加测试验证 agent 创建流程

## 相关文件
- `crates/api/src/routes/tools.rs:76-3530` - MCP 工具注册和调用
- `crates/services/src/agent_service.rs` - Agent Service 接口
- `docs/paperclip-mcp-migration-pl` - 迁移计划（缺少 agent 工具）
- `/Users/adazhao/workspace/paperclip/packages/mcp-server/src/tools.ts` - Paperclip 原始工具定义
