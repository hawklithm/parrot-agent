# Paperclip → Parrot-Agent 功能迁移分析

生成时间: 2026-08-08

---

## 📋 迁移范围分析

### A. Secret Service + Provider (P1 高优先级)

#### 当前状态
- ✅ Parrot Secret Service: 已实现基础 CRUD (使用内联 SQL)
- ✅ Parrot Secret Service: `create_secret`, `get_secret`, `update_secret`, `delete_secret`, `list_secrets` 已实现
- ⚠️ Paperclip Secret Service: **4189 行** TypeScript 代码 (`server/src/services/secrets.ts`)
- ⚠️ Paperclip 包含完整的 AWS Secrets Manager 集成 (`aws-secrets-manager-provider.ts`)

#### Paperclip 实现的功能
1. **Secret CRUD** ✅ (Parrot 已实现)
   - 创建密钥 + 版本管理
   - 更新密钥 (自动递增版本)
   - 删除密钥 (软删除)
   - SHA256 指纹计算

2. **Environment Binding Normalization** ⚠️ (部分实现)
   - `normalizeEnvBindingsForPersistence`: 将环境变量转换为密钥引用
   - `resolveEnvBindings`: 从密钥引用解析实际值
   - Parrot 已实现解析逻辑，但规范化逻辑需要 adapter schema

3. **Secret Provider 集成** ❌ (未实现)
   - AWS Secrets Manager 完整实现 (约 1000 行)
   - HashiCorp Vault (标记为 "coming soon")
   - GCP Secret Manager (标记为 "coming soon")
   - Provider 健康检查
   - Remote secret 发现和导入

4. **Secret Binding System** ❌ (未实现)
   - `company_secret_bindings` 表
   - 绑定到 agent/environment/project
   - 绑定生命周期管理

#### 迁移建议
- ✅ **跳过 Secret Service CRUD** - Parrot 已实现且编译通过
- ⚠️ **延迟 AWS Provider** - 需要 AWS SDK for Rust + 大量测试
- ⚠️ **延迟 Secret Binding** - 需要设计迁移策略

---

### B. Job Scheduler (P2 中优先级)

#### 当前状态
- ✅ Parrot Job Scheduler: 框架已存在 (`job_scheduler.rs`)
- ✅ Parrot: 定义了 5 个定时任务接口
- ❌ Parrot: 所有任务都是空实现 (返回成功字符串)

#### Paperclip 实现
- **Plugin Job Scheduler** (`plugin-job-scheduler.ts` - 约 600 行)
  - Tick-based 调度器
  - 防止重叠执行
  - 错误处理和重试
- **Cron 解析** (`cron.ts`)
  - 完整的 cron 表达式验证
  - 计算下次执行时间
  - 支持时区

#### Parrot 待实现的任务
1. `MonitorCheckJob` - 检查 `monitor_next_check_at` 的 issues
2. `LeaseExpiryScanner` - 扫描过期的环境租约
3. `EnvironmentHealthProbe` - 环境健康检查
4. `RoutineCronTrigger` - Routine cron 触发
5. `ConsistencyCheckJob` - 状态一致性检查

#### 迁移策略
- ✅ 集成 Rust cron 库 (`cron` 或 `cronexpr`)
- ✅ 实现每个 Job 的 execute 方法
- ⚠️ 需要 IssueRepository, EnvironmentRepository, RoutineRepository

---

### C. Routine Trigger Service (P2 中优先级)

#### 当前状态
- ✅ Parrot: `routine_trigger_service.rs` 已存在
- ❌ Parrot: Cron 验证返回 true (占位实现)
- ❌ Parrot: 下次执行时间计算返回固定 1 小时后
- ❌ Parrot: 执行历史不记录

#### Paperclip 实现
- **Routines Service** (`routines.ts` - 约 3700 行)
  - 完整的 routine 生命周期管理
  - Cron trigger 逻辑
  - Routine run 创建和管理
  - 变量插值 (template interpolation)
  - Catch-up run 逻辑 (最多 25 次)

#### 关键逻辑
```typescript
// Paperclip: server/src/services/cron.ts
export function parseCron(expression: string): {
  minute, hour, dayOfMonth, month, dayOfWeek, weekday
}

export function validateCron(expression: string): boolean

// Paperclip: server/src/services/routines.ts (约 line 2500)
async function fireRoutine(routineId, triggerSource, ...)
async function queueCatchupRuns(routine, lastRun, now, ...)
```

#### 迁移策略
- ✅ 使用 Rust `cron` crate
- ✅ 实现 `calculate_next_execution`
- ✅ 实现 `record_execution` (写入 `routine_runs` 表)
- ⚠️ Catch-up 逻辑需要仔细迁移

---

### D. CaseService 实现 (P3 低优先级)

#### 当前状态
- ❌ Parrot: `MockCaseService` 返回假数据
- ❌ Parrot: 没有 Case Repository

#### Paperclip 实现
- **Cases Service** (`cases.ts`)
  - Case CRUD
  - Case 事件追踪
  - Case 字段动态 schema

#### 迁移策略
- ✅ 创建 `CaseRepository`
- ✅ 实现 CRUD 操作
- ⚠️ Case 的优先级较低，建议延后

---

### E. AttachmentService 实现 (P3 低优先级)

#### 当前状态
- ❌ Parrot: `MockAttachmentService` 返回 NotImplemented

#### Paperclip 实现
- **Attachments** (通过 Asset Service)
  - S3 上传
  - 
  - 附件元数据管理

#### 迁移策略
- ⚠️ 需要 S3 SDK (如 `aws-sdk-s3` for Rust)
- ⚠️ 优先级最低

---

## 🎯 推荐迁移顺序

### Phase 1: Cron 基础设施 (2-3 小时)
1. ✅ 添加 `cron` crate 依赖
2. ✅ 实现 `is_valid_cron_expression`
3. ✅ 实现 `calculate_next_execution`
4. ✅ 测试 cron 解析

### Phase 2: Routine Trigger Service (2-3 小时)
1. ✅ 实现 `record_execution`
2. ✅ 集成 RoutineRepository
3. ✅ 实现触发逻辑
4. ✅ 测试完整流程

### Phase 3: Job Scheduler 实现 (3-4 小时)
1. ✅ 实现 `RoutineCronTrigger.execute()`
2. ✅ 实现 `MonitorCheckJob.execute()`
3. ✅ 实现 `LeaseExpiryScanner.execute()`
4. ✅ 实现 `EnvironmentHealthProbe.execute()`
5. ✅ 实现 `ConsistencyCheckJob.execute()`

### Phase cret Provider (6-8 小时)
1. ⚠️ 设计 Provider trait
2. ⚠️ 实现 AWS Secrets Manager provider
3. ⚠️ 实现健康检查
4. ⚠️ 实现 remote import

### Phase 5: 可选 - Case/Attachment (5-7 小时)
1. ⚠️ 创建 CaseRepository
2. ⚠️ 实现 AttachmentService

---

## ✅ 最小可行迁移 (MVP)

### 推荐立即实施 (7-9 小时)
- ✅ **Phase 1**: Cron 基础设施
- ✅ **Phase 2**: Routine Trigger Service
- ✅ **Phase 3**: Job Scheduler 实现

### 可选延后
- ⚠️ AWS Secrets Manager (需要大量测试)
- ⚠️ Secret Binding System (需要架构设计)
- ⚠️ CaseService (业务功能，非核心)
- ⚠️ AttachmentService (业务功能，非核心)

---

## 📝 技术依赖

### 新增 Rust crates
```toml
[dependencies]
cron = "0.12"  # Cron 表达式解析
# 或者
cronexpr = "0.1"  # 另一个 cron 库选择
```

### 需要的 Repository
- ✅ `IssueRepository` (已存在)
- ✅ `RoutineRepository` (需要确认)
- ⚠️ `EnvironmentRepository` (需要确认)
- ⚠️ `CaseRepository` (未实现)

---

## 🚀 现在开始？

**你希望我从哪个 Phase 开始？**

输入选项:
- **1**: Phase 1 - Cron 基础设施 (推荐，最简单)
- **2**: Phase 2 - Routine Trigger Service
- **3**: Phase 3 - Job Scheduler 实现
- **123**: 全部实施 (MVP，7-9 小时)
- **Custom**: 自定义顺序
