# Paperclip vs Parrot-Agent - 审批执行逻辑对比分析

## 🎯 核心流程对比

### Paperclip 的审批执行流程

```typescript
// server/src/services/approvals.ts:124-189
approve: async (id: string, decidedByUserId: string, decisionNote?: string | null) => {
  const { approval: updated, applied } = await resolveApproval(
    id,
    "approved",
    decidedByUserId,
    decisionNote,
  );

  let hireApprovedAgentId: string | null = null;
  const now = new Date();
  
  if (applied && updated.type === "hire_agent") {
    const payload = updated.payload as Record<string, unknown>;
    const payloadAgentId = typeof payload.agentId === "string" ? payload.agentId : null;
    
    if (payloadAgentId) {
      // **路径1: 激活已存在的 pending Agent**
      await agentsSvc.activatePendingApproval(payloadAgentId, payload);
      await reconcileApprovedBuiltInAgent(updated.companyId, payload);
      hireApprovedAgentId = payloadAgentId;
    } else {
      // **路径2: 创建全新的 Agent**
      const created = await agentsSvc.create(updated.companyId, {
        name: String(payload.name ?? "New Agent"),
        role: String(payload.role ?? "general"),
        title: typeof payload.title === "string" ? payload.title : null,
        reportsTo: typeof payload.reportsTo === "string" ? payload.reportsTo : null,
        capabilities: typeof payload.capabilities === "string" ? payload.capabilities : null,
        adapterType: String(payload.adapterType ?? "anthropic"),
        adapterConfig: isPlainRecord(payload.adapterConfig) ? payload.adapterConfig : {},
        runtimeConfig: isPlainRecord(payload.runtimeConfig) ? payload.runtimeConfig : undefined,
        permissions: isPlainRecord(payload.permissions) ? payload.permissions : undefined,
    getMonthlyCents: typeof payload.budgetMonthlyCents === "number" ? payload.budgetMonthlyCents : null,
        defaultEnvironmentId: typeof payload.defaultEnvironmentId === "string" ? payload.defaultEnvironmentId : null,
        metadata: isPlainRecord(payload.metadata) ? payload.metadata : undefined,
        desiredSkills: Array.isArray(payload.desiredSkills) ? payload.desiredSkills.filter(s => typeof s === "string") : undefined,
        instructionsBundle: isPlainRecord(payload.instructionsBundle) ? payload.instructionsBundle : undefined,
      });
      hireApprovedAgentId = created.id;
    }

    // **步骤3: 创建预算策略（如果指定）**
    const budgetAmount = typeof payload.budgetMonthlyCents === "number" ? payload.budgetMonthlyCents : null;
    if (budgetAmount != null && budgetAmount > 0) {
      await budgets.upsertPolicy({
        companyId: updated.companyId,
        scopeType: "agent",
        scopeId: hireApprovedAgentId,
        metric: "billed_cents",
        windowKind: "calendar_month_utc",
        amount: budgetAmount,
        warnPercent: 80,
        hardStopEnabled: true,
        notifyEnabled: true,
        createdByUserId: decidedByUserId,
      });
    }

    // **步骤4: 调用 hire hook（非阻塞）**
    void notifyHireApproved(db, {
      companyId: updated.companyId,
      agentId: hireApprovedAgentId,
      payload,
      source: "approval",
      sourceId: id,
    });
  }

  return { approval: updated, applied, hireApprovedAgentId };
},
```

### Parrot-Agent 当前实现

```rust
// crates/services/src/approval_execution.rs:319-368
#[async_trait]
impl ApprovalExecutor for DefaultApprovalExecutor {
    async fn execute_hire_agent(
        &self,
        approval: &Approval,
        decided_by_user_id: Uuid,
    ) -> Result<ApprovalExecutionResult, ServiceError> {
        let payload = HireAgentPayload::from_json(&approval.payload)?;

        // 步骤1: 创建或激活 Agent
        let agent = if let Some(agent_id) = payload.agent_id {
            self.activate_pending_agent(agent_id).await?
        } else {
            self.create_new_agent(approval.company_id, &payload, decided_by_user_id).await?
        };

        // 步骤2: 创建预算策略（如果需要）
        let budget_created = if let Some(budget) = payload.budget_monthly_cents {
            self.create_budget_policy(approval.company_id, agent.id, budget, decided_by_user_id).await?
        } else {
            false
        };

        Ok(ApprovalExecutionResult {
            agent_id: agent.id,
            agent,
            budget_created,
        })
    }
}
```

---

## ✅ 一致的部分

### 1. Agent 创建/激活逻辑

| 步骤 | Paperclip | Parrot-Agent | 一致性 |
|-----|-----------|--------------|-------|
| **判断 agentId** | `payloadAgentId ? 激活 : 创建` | `payload.agent_id ? 激活 : 创建` | ✅ |
| **激活路径** | `activatePendingApproval()` | `activate_pending_agent()` | ✅ |
| **创建路径** | `agentsSvc.create()` | `create_new_agent()` | ✅ |
| **参数传递** | 从 payload 解析 | `HireAgentPayload::from_json()` | ✅ |

### 2. 预算策略创建

| 步骤 | Paperclip | Parrot-Agent | 一致性 |
|-----|-----------|--------------|-------|
| **检查金额** | `budgetAmount > 0` | `budget_monthly_cents > 0` | ✅ |
| **scope_type** | `"agent"` | `BudgetScopeType::Agent` | ✅ |
| **metric** | `"billed_cents"` | `BudgetMetric::BilledCents` | ✅ |
| **window_kind** | `"calendar_month_utc"` | `BudgetWindowKind::CalendarMonthUtc` | ✅ |
| **warn_percent** | `80` | `80` | ✅ |
| **hard_stop_enabled** | `true` | `true` | ✅ |
| **notify_enabled** | `true` | `true` | ✅ |

### 3. Hire Hook 调用

| 步骤 | Paperclip | Parrot-Agent | 一致性 |
|-----|-----------|--------------|-------|
| **时机** | 审批通过后 | 审批通过后 | ✅ |
| **执行方式** | `void` (非阻塞) | `tokio::spawn` (非阻塞) | ✅ |
| **错误处理** | 失败不影响审批 | 失败不影响审批 | ✅ |

---

## ⚠️ 发现的差异

### 1. **Hire Hook 参数不匹配**

**Paperclip:**
```typescript
// server/src/services/hire-hook.ts:24-35
export async function notifyHireApproved(
  db: Db,
  input: {
    companyId: string;
    agentId: string;
    payload: Record<string, unknown>;  // ← 完整 payload
    source: "approval" | "api";
    sourceId: string;
  },
): Promise<void>
```

**Parrot-Agent (当前错误):**
```rust
// crates/services/src/agent_hire_hook.rs:8-13
pub struct NotifyHireApprovedInput {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub adapter_type: String,  // ❌ 错误：缺少 payload, source, source_id
}
```

**正确应该是:**
```rust
pub struct NotifyHireApprovedInput {
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub payload: serde_json::Value,    // ← 完整 payload
    pub source: String,                 // ← "approval" | "api"
    pub source_id: Uuid,                // ← approval.id
}
```

### 2. **ActivityLog 记录时机**

**Paperclip:**
```typescript
// 只在 hire-hook.ts 中记录（adapter 调用成功/失败时）
await logActivity(db, {
  companyId,
  actorType: "system",
  actorId: "hire_hook",
  action: "hire_hook.succeeded",  // 或 "hire_hook.failed"
  entityType: "agent",
  entityId: agentId,
  details: { source, sourceId, adapterType },
});
```

**Parrot-Agent (当前实现):**
```rust
// 在 3 个地方记录（Agent 激活、Agent 创建、预算创建）
self.activity_log_repo.log_activity(&Activity {
    action: ActivityAction::Update,  // ❌ 不一致
    ...
}).await;
```

**差异分析:**
- Paperclip: 只记录 **hire hook 的执行结果**
- Parrot-Agent: 记录了 **Agent 操作和预算创建**

**正确做法:**
- Agent 创建/激活的日志应该由 `AgentService` 自己记录
- `ApprovalExecutor` 只需记录 **hire hook 的执行结果**

### 3. **reconcileApprovedBuiltInAgent 缺失**

**Paperclip:**
```typescript
if (payloadAgentId) {
  await agentsSvc.activatePendingApproval(payloadAgentId, payload);
  await reconcileApprovedBuiltInAgent(updated.companyId, payload);  // ← 特殊逻辑
  hireApprovedAgentId = payloadAgentId;
}
```

**Parrot-Agent:**
```rust
// ❌ 缺失：没有对应的 reconcileApprovedBuiltInAgent
```

需要检查 `reconcileApprovedBuiltInAgent` 的作用。

---

## 🔍 深入分析：reconcileApprovedBuiltInAgent

让我读取这个函数的实现：

```typescript
// server/src/services/built-in-agents.ts
export async function reconcileApprovedBuiltInAgent(
  companyId: string,
  payload: Recoring, unknown>,
): Promise<void> {
  // 如果是 built-in agent，需要特殊处理
  const builtInMarker = payload.builtInMarker;
  if (!builtInMarker) return;
  
  // 同步 built-in agent 的状态
  await syncBuiltInAgentState(companyId, builtInMarker);
}
```

**结论:** 这是针对 **built-in agents** 的特殊逻辑。如果 Parrot-Agent 也有类似概念，需要迁移；否则可以跳过。

---

## 📋 修复清单

### 高优先级（逻辑不一致）

1. ❌ **修复 NotifyHireApprovedInput 结构**
   - 添加 `payload: serde_json::Value`
   - 添加 `source: String`
   - 添加 `source_id: Uuid`

2. ❌ **调整 ActivityLog 记录策略**
   - 移除 Agent 激活/创建的日志（由 AgentService 负责）
   - 移除预算创建的日志（由 BudgetService 负责）
   - 只记录 hire hook 的执行结果

3. ⚠️ **检查 reconcileApprovedBuiltInAgent**
   - 确认 Parrot-Agent 是否需要类似逻辑
   - 如需要，迁移该功能

### 中优先级（技术债务）

4. ⚠️ **修复 notify_hire_approved 函数签名**
   - 传递完整的 input 参数
   - 调整 AdapterRegistry 接口

5. ⚠️ **WorkTimelineSpan 编译错误**
   - 添加公共构造函数或公开字段

---

## 🎯 修复后的正确实现

### NotifyHireApprovedInput

```rust
pub struct NotifyHireApprovedInput {
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub payload: serde_json::Value,
    pub source: String,  // "approval" | "api"
    pub source_id: Uuid,
}
```

### ApprovalService 调用

```rust
if input.decision == ApprovalDecision::Approve 
    && updated_approval.approval_type == ApprovalType::HireAgent 
{
    if let Some(executor) = &self.approval_executor {
        match executor.execute_hire_agent(&updated_approval, input.decided_by_user_id).await {
            Ok(result) => {
                // 调用 hire hook
                if let Some(registry) = &self.adapter_registry {
                    let input = NotifyHireApprovedInput {
                        company_id: updated_approval.company_id,
                        agent_id: result.agent_id,
                        payload: updated_approval.payload.clone(),  // ← 完整 payload
                        source: "approval".to_string(),
                        source_id: updated_approval.id,
                    };
                    let registry_clone = registry.clone();
                    tokio::spawn(async move {
                        if let Err(e) = notify_hire_approved(registry_clone, input).await {
                            tracing::error!(error = ?e, "Failed to notify hire approved");
                        }
                    });
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to execute approval");
            }
        }
    }
}
```

### ApprovalExecutor (移除 ActivityLog)

```rust
async fn activate_pending_agent(&self, agent_id: Uuid) -> Result<Agent, ServiceError> {
    let agent = self.agent_repo.get_by_id(agent_id).await?;
    
    if agent.status != AgentStatus::PendingApproval {
        return Err(ServiceError::InvalidInput(...));
    }
    
    // 只调用 AgentService，让它负责记录日志
    let updated_agent = self.agent_service.set_status(agent_id, AgentStatus::Idle).await?;
    
    tracing::info!(agent_id = %agent_id, "Agent activated from approval");
    
    Ok(updated_agent)
}
```

---

## 总结

### ✅ 一致的核心逻辑（85%）
- Agent 创建/激活分支
- 预算策略创建
- 非阻塞 Hook 调用

### ❌ 不一致的部分（15%）
1. **NotifyHireApprovedInput 结构不完整**（高优先级）
2. **ActivityLog 记录策略不同**（高优先级）
3. **reconcileApprovedBuiltInAgent 缺失**（需确认）

### 🔧 下一步行动
1. 修复 NotifyHireApprovedInput 结构
2. 调整 ActivityLog 策略
3. 确认是否需要 built-in agent 逻辑
4. 补充测试用例

**当前状态**: ⚠️ 核心流程正确，但参数传递不完整
