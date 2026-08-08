# Parrot-Agent 技术债务完整分析报告

生成时间: 2026-08-08  
分析范围: `crates/services/src/` (39 个 TODO)

---

## 📊 总体统计

### TODO 分布 (按文件排序)
1. **secret_service.rs**: 8 个 TODO - 密钥管理核心功能未实现
2. **job_scheduler.rs**: 7 个 TODO - 定时任务功能未实现
3. **secret_provider_service.rs**: 4 个 TODO - 第三方密钥提供商集成
4. **routine_trigger_service.rs**: 4 个 TODO - 定时任务触发器未实现
5. **access_service.rs**: 3 个 TODO - 权限检查增强
6. **issue_comment_service.rs**: 2 个 TODO - 评论功能增强
7. **authorization_service.rs**: 2 个 TODO - 授权服务集成
8. 其他 8 个文件: 各 1 个 TODO

### 未实现方法 (`unimplemented!()`)
- **issue_service_complete.rs**: 20+ 个测试桩方法 (Mock 实现)
- **recovery_action_service.rs**: 15+ 个方法
- **monitor_scheduler.rs**: 4 个方法
- **issue_diagnostics_service.rs**: 2 个方法

### Mock/硬编码实现
- **CaseService**: 完全是 Mock 实现
- **AttachmentService**: Mock 实现
- **issue_service_complete.rs**: 大量测试桩

---

## 🎯 优先级分类

### P0 - 关键阻塞 (0 项)
✅ 无阻塞性问题 - 核心功能已完整

### P1 - 高优先级 (影响主要功能)

#### 1. Secret Service 数据库持久化 (8 个 TODO)
**文件**: `crates/services/src/secret_service.rs`

**问题**:
```rust
// Line 548
async fn create(&self, _input: CreateSecretInput) -> ServiceResult<Secret> {
    // TODO: 实现数据库持久化
    Err(ServiceError::NotImplemented("create secret".to_string()))
}

// Line 566
async fn get_by_id(&self, _id: Uuid) -> ServiceResult<Option<Secret>> {
    // TODO: 实现数据库查询
    Ok(None)
}

// Line 579
async fn update(&self, _id: Uuid, _input: UpdateSecretInput) -> ServiceResult<Secret> {
    // TODO: 实现数据库更新
    Err(ServiceError::NotImplemented("update secret".to_string()))
}

// Line 593
async fn delete(&self, _id: Uuid) -> ServiceResult<()> {
    // TODO: 实现数据库删除
    Err(ServiceError::NotImplemented("delete secret".to_string()))
}

// Line 603
async fn list_by_scope(&self, _scope: SecretScope) -> ServiceResult<Vec<Secret>> {
    // TODO: 实现数据库查询
    Ok(vec![])
}
```

**影响**: 密钥管理功能完全不可用

**迁移来源**: 
- Paperclip: `server/src/services/secrets.ts`
- 需要实现: `secrets` 表的 CRUD 操作
- 需要加密存储密钥值

**预估时间**: 3-4 小时

---

#### 2. Secret Provider Service 集成 (4 个 TODO)
**文件**: `crates/services/src/secret_provider_service.rs`

**问题**:
```rust
// Line 167
async fn discover_secrets(&self, provider_type: &str, config: &JsonValue) -> ServiceResult<Vec<DiscoveredSecret>> {
    // TODO: Implement actual provider-specific discovery logic
    Ok(vec![])
}

// Line 188
async fn health_check(&self, provider_type: &str, config: &JsonValue) -> ServiceResult<ProviderHealthStatus> {
    // TODO: Implement actual provider-specific health check
    Ok(ProviderHealthStatus { ... })
}

// Line 273
async fn import_secret(&self, provider_type: &str, discovered: &DiscoveredSecret, target_scope: SecretScope) -> ServiceResult<Secret> {
    // TODO: Implement actual provider-specific import logic
    Err(ServiceError::NotImplemented(...))
}
```

**影响**: 无法从 AWS Secrets Manager / Vault 等导入密钥

**迁移来源**:
- Paperclip: `server/src/services/secret-providers/`
- 需要实现: AWS SDK, Vault API 集成

**预估时间**: 4-6 小时

---

### P2 - 中优先级 (功能增强)

#### 3. Job Scheduler 定时任务 (7 个 TODO)
**文件**: `crates/services/src/job_scheduler.rs`

**问题**:
```rust
// Line 224
async fn check_routine_triggers(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Check issues with monitor_next_check_at < NOW()
    Ok(())
}

// Line 243
async fn scan_environment_leaks(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Scan for expired environment leases
    Ok(())
}

// Line 262
async fn probe_environment_health(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Probe environment health
    Ok(())
}

// Line 281
async fn check_routines(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Check for routines due for cron trigger
    Ok(())
}
```

**影响**: 后台定时任务不执行

**迁移来源**:
- Paperclip: `server/src/services/job-scheduler.ts`
- 需要实现: Cron 调度逻辑

**预估时间**: 3-4 小时

---

#### 4. Routine Trigger Service Cron 解析 (4 个 TODO)
**文件**: `crates/services/src/routine_trigger_service.rs`

**问题**:
```rust
// Line 126
fn is_valid_cron_expression(&self, _expression: &str) -> bool {
    // TODO: Use a proper cron parser library for full validation
    true
}

// Line 409
fn calculate_next_execution(&self, trigger: &RoutineTrigger) -> Option<chrono::DateTime<chrono::Utc>> {
    // TODO: Use cron parser to calculate next execution time
    Some(chrono::Utc::now() + chrono::Duration::hours(1))
}

// Line 426
async fn record_execution(&self, _routine_id: Uuid, _execution_time: chrono::DateTime<chrono::Utc>) -> ServiceResult<()> {
    // TODO: Store execution history in a separate table
    Ok(())
}
```

**影响**: Cron 表达式不生效，定时任务不准确

**迁移来源**:
- Rust crate: `cron` 或 `cronexpr`
- Paperclip: `server/src/services/routine-triggers.ts`

**预估时间**: 2-3 小时

---

#### 5. Access Service 权限增强 (3 个 TODO)
**文件**: `crates/services/src/access_service.rs`

**问题**:
```rust
// Line 312
async fn decide(&self, ...) -> AccessResult<AccessDecision> {
    // TODO: Add change grant and consent checks
    ...
}

// Line 363
async fn assert_agent_read_allowed(&self, ...) -> AccessResult<()> {
    // TODO: Check feature flag in company settings
    ...
}
```

**影响**: 细粒度权限控制不完整

**迁移来源**:
- Paperclip: `server/src/services/access-service.ts`

**预估时间**: 2-3 小时

---

### P3 - 低优先级 (可选优化)

#### 6. Issue Comment 功能增强 (2 个 TODO)
```rust
// issue_comment_service.rs:106
// TODO: Check if actor is admin when we have access control

// issue_comment_service.rs:147
// TODO: Update issue's last_activity_at when we add that field to UpdateIssueInput
```

**预估时间**: 1 小时

---

#### 7. 其他零散 TODO (9 个)
- `agent_service.rs`: 集成 SessionManagementService
- `comment_service.rs`: Issue reopen 逻辑
- `authorization_service.rs`: accessService 集成
- `built_in_agent_service.rs`: bundle 定义
- `pipeline_service.rs`: 条件评估
- `cost_service.rs`: Scope-aware 聚合
- `issue_execution_lock_service.rs`: 分页查询
- `server_adapter.rs`: EnvironmentRuntimeService 集成

**预估时间**: 5-7 小时

---

## 🚨 Mock 实现需要替换

### 1. CaseService - 完全 Mock
**文件**: `crates/services/src/case_service.rs`

**问题**: 返回假数据，不连接数据库
```rust
pub struct MockCaseService;

impl CaseService for MockCaseService {
    async fn create(&self, input: CreateCaseInput) -> ServiceResult<Case> {
        let case = Self::create_mock_case(Uuid::new_v4(), input.company_id, input.title);
        Ok(case)
    }
    // ... 所有方法都是 mock
}
```

**迁移来源**:
- Paperclip: `server/src/services/cases.ts`
- 需要实现: `cases` 表的完整 CRUD

**预估时间**: 4-5 小时

---

### 2. AttachmentService - Mock 实现
**文件**: `crates/services/src/attachment_service.rs`

**问题**: 
```rust
pub struct MockAttachmentService;

impl AttachmentService for MockAttachmentService {
    async fn upload_attachment(&self, ...) -> ServiceResult<Attachment> {
        Err(ServiceError::NotImplemented("upload attachment".to_string()))
    }
}
```

**迁移来源**:
- Paperclip: `server/src/services/attachments.ts`
- 需要实现: S3/本地文件存储 + 数据库记录

**预估时间**: 3-4 小时

---

### 3. IssueServiceComplete - 测试桩
**文件**: `crates/services/src/issue_service_complete.rs`

**问题**: 20+ 个方法返回 `unimplemented!()`

**状态**: 这是**测试用的 Mock 实现**，不需要替换

---

## 📋 迁移优先级总结

### 立即实施 (P1 - 12 个 TODO)
1. ✅ **Secret Service 持久化** (8 个 TODO) - 3-4 小时
2. ✅ **Secret Provider 集成** (4 个 TODO) - 4-6 小时

**总计**: 7-10 小时

---

### 近期实施 (P2 - 14 个 TODO)
3. **Job Scheduler** (7 个 TODO) - 3-4 小时
4. **Routine Trigger Cron** (4 个 TODO) - 2-3 小时
5. **Access Service** (3 个 TODO) - 2-3 小时

**总计**: 7-10 小时

---

### 可选实施 (P3 - 13 个 TODO)
6. **Issue Comm 个 TODO) - 1 小时
7. **其他零散 TODO** (9 个) - 5-7 小时
8. **Mock 替换**: CaseService + AttachmentService - 7-9 小时

**总计**: 13-17 小时

---

## 🎯 推荐行动

### 选项 A: 完成 P1 高优先级 (7-10 小时)
**Secret Service + Secret Provider Service**
- 解锁密钥管理功能
- 支持环境变量安全存储
- 集成 AWS/Vault 密钥导入

### 选项 B: 完成 P2 中优先级 (7-10 小时)
**Job Scheduler + Routine Trigger + Access Service**
- 启用后台定时任务
- Cron 表达式正确解析
- 完善权限系统

### 选项 C: 替换 Mock 实现 (7-9 小时)
**CaseService + AttachmentService**
- 实现 Case 管理完整功能
- 实现文件上传和存储

### 选项 D: 从 next_task.md 开始 MCP 工具迁移
**Phase 1: 核心认证与用户信息** (2 tasks)
- 完善 `paperclipMe` 实现
- 完善 `paperclipInboxLite` 实现

---

## ✅ 当前系统状态

### 已完成 ✅
- ✅ 核心 Issue 管理
- ✅ Project/Agent CRUD
- ✅ Approval 审批流程
- ✅ Resource Membership
- ✅ Issue Tree Control (深度计算)
- ✅ EventBus 事件系统

### 未完成 ⚠️
- ⚠️ Secret 密钥管理 (8 个 TODO)
- ⚠️ Secret Provider 集成 (4 个 TODO)
- ⚠️ Job Scheduler (7 个 TODO)
- ⚠️ Routine Cron 触发 (4 个 TODO)
- ⚠️ CaseService Mock 实现
- ⚠️ AttachmentService Mock 实现

**总计**: 39 个 TODO + 2 个 Mock 服务

---

**你希望从哪个方向开始？**
- **A: P1 Secret 管理** (推荐，解锁密钥功能)
- **B: P2 定时任务系统** (Job Scheduler + Routine)
- **C: 替换 Mock 实现** (CaseService + Attachment)
- **D: MCP 工具迁移** (从 next_task.md Phase 1 开始)
- **E: 其他优先级**
