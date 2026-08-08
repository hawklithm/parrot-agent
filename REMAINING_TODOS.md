# Parrot-Agent 遗留 TODO 清单

生成时间: 2026-08-08  
扫描范围: `crates/api/src/routes/` + `crates/services/src/`

---

## 统计概览

- **总 TODO 数量**: 30 项 (已排除测试和 Mock 相关)
- **P0 (阻塞核心功能)**: 0 项
- **P1 (影响用户体验)**: 4 项
- **P2 (技术债务)**: 26 项

---

## P1: 需要尽快处理的 TODO (4 项)

### 1. User Secrets 路由 - 缺少 User ID 提取 (4 处)

**文件**: `crates/api/src/routes/user_secrets.rs`

```rust
// Line 106
// TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）

// Line 172
// TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）

// Line 191
// TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）

// Line 208
// TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
```

**问题**: 
- User secrets 路由已实现，但未提取真实 user_id
- 当前可能使用硬编码或空值

**解决方案** (参考 resource_memberships.rs):
```rust
let user_id = match &auth_actor {
    AuthorizationActor::Board { user_id, .. } => *user_id,
    _ => return Err(AppError::Forbidden("Board user access required".to_string())),
};
```

**预估时间**: 15 分钟

---

## P2: 技术债务 TODO (26 项)

### 2. Secret Service - 数据库持久化未实现 (7 处)

**文件**: `crates/services/src/secret_service.rs`

```rust
// Line 427
// TODO: 规范化 adapter schema 中标记为 secret 的字段

// Line 461
// TODO: 从数据库解析密钥值

// Line 485
// TODO: 从用户环境解析密钥

// Line 548
// TODO: 实现数据库持久化

// Line 566
// TODO: 实现数据库查询

// Line 579
// TODO: 实现数据库更新

// Line 593
// TODO: 实现数据库删除

// Line 603
// TODO: 实现数据库查询
```

**问题**: 
- Secret Service 的 trait 已定义
- 数据库操作方法返回硬编码值或空值

**影响**: 
- 密钥管理功能不可用
- Adapter 配置中的 secret 字段无法持久化

**预估时间**: 2-3 小时

---

### 3. Secret Provider Service - 发现和导入逻辑 (4 处)

**文件**: `crates/services/src/secret_provider_service.rs`

```rust
// Line 167
// TODO: Implement actual provider-specific discovery logic

// Line 188
// TODO: Implement actual provider-specific health check

// Line 230
// TODO: Implement actual provider-specific discovery with filters

// Line 273
// TODO: Implement actual provider-specific import logic
```

**问题**: 
- Secret provider (AWS Secrets Manager, HashiCorp Vault 等) 集成未实现
- 当前返回 mock 数据

**影响**: 
- 无法从外部密钥管理系统导入密钥
- 预览功能不可用

**预估时间**: 4-6 小时 (需要集成外部 SDK)

---

### 4. Approval Service - EventBus 事件发布 (1 处)

**文件**: `crates/services/src/approval_service.rs`

```rust
// Line 461
// TODO: Publish ApprovalApproved event to EventBus
```

**问题**: 
- Approval 审批通过后未发布事件
- 可能影响下游监听器 (通知、工作流等)

**影响**: 
- 审批通过后的自动化流程可能不触发

**预估时间**: 30

---

### 5. Company Service - 角色和预算初始化 (2 处)

**文件**: `crates/services/src/company_service.rs`

```rust
// Line 18
// TODO: Call AccessService.ensure_role_default_grants() when implemented

// Line 19
// TODO: Call BudgetService.upsert_policy() when budget is set
```

**问题**: 
- 创建公司时未初始化默认角色权限
- 未创建默认预算策略

**影响**: 
- 新公司可能缺少基础权限配置
- 预算控制不生效

**预估时间**: 1 小时

---

### 6. Agent Service - 会话清理和日志 (2 处)

**文件**: `crates/services/src/agent_service.rs`

```rust
// Line 442
// TODO: 记录日志警告

// Line 681
// TODO: 集成SessionManagementService清理会话
```

**问题**: 
- Agent 终止时未清理会话
- 缺少日志记录

**影响**: 
- 会话泄漏
- 调试困难

**预估时间**: 30 分钟

---

### 7. Issue Tree Control Service - 深度和活跃 run 计算 (3 处)

**文件**: `crates/services/src/issue_tree_control_service.rs`

```rust
// Line 215
// TODO: Get active runs for affected issues

// Line 279
depth: 0, // TODO: calculate actual depth

// Line 285
active_run_id: None, // TODO: get active run
```

**问题**: 
- Issue tree 深度硬编码为 0
- 未获取活跃的 run

**影响**: 
- Issue 层级关系不准确
- 无法展示正在执行的 run

**预估时间**: 1-2 小时

---

### 8. Project Service - 环境绑定和权限检查 (3 处)

**文件**: `crates/services/src/project_service.rs`

```rust
// Line 20
// TODO: Call SecretService.normalize_env_bindings_for_persistence() when implemented

// Line 23
// TODO: Optionally create workspace and sync env bindings

// Line 164
// TODO: assert_mutation_allowed when AccessService is integrated
```

**问题**: 
- 环境变量绑定未规范化
- 权限检查被跳过

**影响**: 
- 环境变量配置可能不一致
- 权限控制不完整

**预估时间**: 1-2 小时

---

### 9. Routine Trigger Service - Cron 解析和执行历史 (4 处)

**文件**: `crates/services/src/routine_trigger_service.rs`

```rust
// Line 126
// TODO: Use a proper cron parser library for full validation

// Line 311
// TODO: Integrate with RoutineService to fire the routine

// Line 409
// TODO: Use cron parser to calculate next execution time

// Line 426
// TODO: Store execution history in a separate table
```

**问题**: 
- Cron 表达式验证不完整
- 未计算下次执行时间
- 执行历史未持久化

**影响**: 
- Routine 定时执行可能不准确
- 无法追踪执行历史

**预估时间**: 2-3 小时 (需要集成 cron parser)

---

### 10. Comment Service - Issue 重开逻辑 (1 处)

**文件**: `crates/services/src/comment_service.rs`

```rust
// Line 83
// TODO: Trigger issue reopen logic
```

**问题**: 
- 评论触发 issue 重开的逻辑未实现

**影响**: 
- 已关闭的 issue 可能无法通过评论重开

**预估时间**: 30 分钟

---

### 11. Server Adapter - EnvironmentRuntimeService 集成 (1 处)

**文件**: `crates/services/src/server_adapter.rs`

```rust
// Line 246
// TODO: Integrate with EnvironmentRuntimeService
```

**问题**: 
- 服务端 Adapter 未集成运行时服务

**影响**: 
- 可能影响 Adapter 的环境管理

**预估时间**: 1 小时

---

### 12. Job Scheduler - Routine 触发检查 (1 处)

**文件**: `crates/services/src/job_scheduler.rs`

```rust
// Line 281
// TODO: Check for routines due for cron trigger
```

**问题**: 
- Job scheduler 未检查待触发的 routines

**影响**: 
- Cron routines 不会自动执行

**预估时间**: 1-2 小时

---

### 13. Skills Service - 硬编码技能列表 (1 处)

**文件**: `crates/services/src/skills_service.rs`

```rust
// Line 34
/// Load hardcoded available skills (placeholder implementation)
```

**问题**: 
- Skills 列表硬编码，未从数据库加载

**影响**: 
- 无法动态管理 skills

**预估时间**: 1 小时

---

### 14. Access Service - 实验特性开关 (1 处)

**文件**: `crates/services/src/access/access_service.rs`

```rust
// Line 363
// TODO: 实际实现需要检查公司配置中的实验特性开关
```

**问题**: 
- 实验特性开关未实现

**影响**: 
- 无法动态控制功能开关

**预估时间**: 30 分钟

---

## 优先级建议

### 立即处理 (P1)
1. ✅ **User Secrets 路由** - User ID 提取 (15 分钟)
   - 阻塞用户密钥管理功能
   - 修复简单，参考 resource_memberships.rs

### 近期处理 (P2 - 高优先级)
2. **Approval Service** - EventBus 集成 (30 分钟)
3. **Agent Service** - 会话清理 (30 分钟)
4. **Comment Service** - Issue 重开逻辑 (30 分钟)
5. **Access Service** - 实验特性开关 (30 分钟)

### 中期处理 (P2 - 中优先级)
6. **Company Service** - 角色和预算初始化 (1 小时)
7. **Issue Tree Control** - 深度和 run 计算 (1-2 小时)
8. **Project Service** - 环境绑定和权限 (1-2 小时)
9. **Server Adapter** - EnvironmentRuntimeService (1 小时)
10. **Skills Service** - 动态加载 (1 小时)

### 长期处理 (P2 - 低优先级)
11. **Secret Service** - 数据库持久化 (2-3 小时)
12. **Secret Provider Service** - 外部集成 (4-6 小时)
13. **Routine Trigger Service** - Cron 完整支持 (2-3 小时)
14. **Job Scheduler** - Routine 触发 (1-2 小时)

---

## 总预估工作量

- **P1 (立即)**: 15 分钟
- **P2 高优先级**: 2 小时
- **P2 中优先级**: 6-9 小时
- **P2 低优先级**: 9-14 小时

**总计**: 17-25 小时

---

## 不需要处理的 TODO

以下 TODO 不在核心功能路径上或已有替代方案:

1. **Mock/Test 相关** - 测试代码，不影响生产
2. **Custom Image Setup** - 自定义镜像功能 (非核心)
3. **Case Service mock 方法** - Mock 实现，仅用于开发

---

## 下一步行动

建议按以下顺序处理:

1. ✅ **立即修复 User Secrets** (15 分钟)
   ```bash
   # 文件: crates/api/src/routes/user_secrets.rs
   # 替换 4 处 TODO 为 AuthorizationActor 提取逻辑
   ```

2. **批量处理 30 分钟项** (2 小时)
   - Approval EventBus
   - Agent 会话清理
   - Comment issue 重开
   - Access 实验特性

3. **逐步处理中期项** (按业务优先级)
   - Company/Project 初始化
   - Issue Tree 完善
   - Skills 动态加载

4. **规划长期项** (与产品确认优先级)
   - Secret 持久化
   - Secret Provider 集成
   - Routine/Cron 完整支持

---

**现在开始处理 P1 项: User Secrets User ID 提取？**
