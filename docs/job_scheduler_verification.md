# Job Scheduler 验证指南

## 概述

本文档提供了验证 parrot-agent job scheduler 功能的步骤和检查清单。

## 对比：Paperclip vs Parrot-Agent

### Paperclip 中的关键函数

**变量处理**：
```typescript
// 1. 验证变量定义
assertRoutineVariableDefinitions(variables: RoutineVariable[])

// 2. 清理输入变量
sanitizeRoutineVariableInputs(variables)

// 3. 检查 schedule 兼容性
assertScheduleCompatibleVariables(variables)

// 4. 解析变量值
resolveRoutineVariableValues(variables, input)

// 5. 合并 payload
mergeRoutineRunPayload(payload, variables)
```

**调度指纹**：
```typescript
// 生成调度指纹（用于去重）
createRoutineDispatchFingerprint(input)
createRoutineEnvFingerprint(env)
```

**状态管理**：
```typescript
// 状态转换
normalizeDraftRoutineStatus(status, assigneeAgentId)
assertRoutineCanEnable(status, assigneeAgentId)
statusRequiresDefaultAgent(status)
```

**Dispatch 主逻辑** (`dispatchRoutineRun`):
1. 验证 assigneeAgentId
2. 解析自动变量（如 workspace branch）
3. 解析用户提供的变量
4. 插值模板（title, description）
5. 生成调度指纹
6. 幂等性检查（idempotencyKey）
7. 创建 routine_run 记录
8. 创建执行 issue
9. 更新 routine 统计信息

### Parrot-Agent 当前实现

**已实现**：
- ✅ 基础的 `dispatch_routine_run` 框架
- ✅ Routine FOR UPDATE 锁
- ✅ 创建 routine_run 记录
- ✅ 创建执行 issue
- ✅ 更新 routine 统计

**缺失功能**：
- ❌ 变量验证和解析逻辑
- ❌ 模板插值（title/description 中的变量替换）
- ❌ 调度指纹生成（去重机制）
- ❌ 幂等性检查
- ❌ 自动变量（workspace branch）
- ❌ Managed routine binding 支持
- ❌ Catch-up 补发逻辑（MAX_CATCH_UP_RUNS）

## 验证步骤

### 1. 本地开发环境设置

```bash
# 启动数据库
docker-compose up -d postgres

# 运行 migrations
sqlx migrate run

# 启动服务器
cargo run --bin server
```

### 2. 创建测试 Routine

```bash
# 获取 company_id 和 agent_id
COMPANY_ID="48c1e93b-094d-46d9-8397-3fea50bb62c8"
AGENT_ID=$(psql -d parrot -t -c "SELECT id FROM agents WHERE company_id='$COMPANY_ID' LIMIT 1;" | tr -d ' ')

# 创建一个简单的 routine
curl -X POST "http://localhost:5173/api/companies/$COMPANY_ID/routines" \
  -H "Content-Type: application/json" \
  -d "{
    \"title\": \"Test Scheduler Routine\",
    \"description\": \"This routine tests the job scheduler\",
    \"assigneeAgentId\": \"$AGENT_ID\",
    \"status\": \"draft\",
    \"priority\": 50,
    \"concurrencyPolicy\": \"coalesce_if_active\",
    \"catchUpPolicy\": \"skip_missed\",
    \"variables\": [],
    \"env\": {}
  }"

# 保存返回的 routine_id
ROUTINE_ID="<从响应中获取>"
```

### 3. 创建 Schedule Trigger

```bash
# 创建每分钟触发的 cron trigger
curl -X POST "http://localhost:5173/api/routines/$ROUTINE_ID/triggers" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "schedule",
    "label": "Every Minute",
    "cronExpression": "* * * * *",
    "timezone": "UTC",
    "enabled": true
  }'

# 保存返回的 trigger_id
TRIGGER_ID="<从响应中获取>"
```

### 4. 激活 Routine

```bash
# 更新 routine 状态为 active
curl -X PATCH "http://localhost:5173/api/routines/$ROUTINE_ID" \
  -H "Content-Type: application/json" \
  -d '{"status": "active"}'
```

### 5. 观察 Scheduler 执行

```bash
# 观察服务器日志
tail -f logs/server.log | grep -E "RoutineCronTrigger|dispatch_routine_run"

# 或者查看数据库中的 routine_runs
watch -n 5 "psql -d parrot -c 'SELECT id, routine_id, source, status, triggered_at FROM routine_runs ORDER BY triggered_at DESC LIMIT 10;'"

# 查看创建的 issues
watch -n 5 "psql -d parrot -c 'SELECT id, title, status, origin_kind, created_at FROM issues WHERE origin_kind='\''routine_execution'\'' ORDER BY created_at DESC LIMIT 10;'"
```

### 6. 验证 Scheduler 任务

#### RoutineCronTrigger (每 30 秒)
```sql
-- 检查是否有 routine_runs 被创建
SELECT 
    rr.id,
    rr.routine_id,
    r.title as routine_title,
    rr.source,
    rr.status,
    rr.triggered_at,
    rr.linked_issue_id
FROM routine_runs rr
JOIN routines r ON r.id = rr.routine_id
WHERE rr.source = 'schedule'
ORDER BY rr.triggered_at DESC
LIMIT 20;

-- 检查 next_run_at 是否被更新
SELECT 
    r.id,
    r.title,
    r.status,
    r.last_triggered_at,
    r.next_run_at,
    r.run_count
FROM routines r
WHERE r.status = 'active'
ORDER BY r.next_run_at;
```

#### MonitorCheckJob (每分钟)
```sql
-- 检查 monitor 健康检查
SELECT 
    id,
    title,
    monitor_next_check_at,
    monitor_health_status,
    updated_at
FROM issues
WHERE monitor_next_check_at IS NOT NULL
ORDER BY monitor_next_check_at
LIMIT 10;
```

#### LeaseExpiryScanner (每分钟)
```sql
-- 检查过期的 leases
SELECT 
    id,
    environment_id,
    holder_issue_id,
    expires_at,
    released_at
FROM environment_leases
WHERE expires_at < NOW() AND released_at IS NULL
LIMIT 10;
```

### 7. 测试 Webhook Trigger

```bash
# 创建 webhook trigger
curl -X POST "http://localhost:5173/api/routines/$ROUTINE_ID/triggers" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "webhook",
    "label": "Test Webhook",
    "enabled": true
  }'

# 从响应中获取 publicId 和 secret
PUBLIC_ID="<从响应中获取>"
SECRET="<从响应中获取>"

# 触发 webhook
curl -X POST "http://localhost:5173/api/routine-triggers/public/$PUBLIC_ID/fire" \
  -H "Content-Type: application/json" \
  -H "X-Paperclip-Signature: $SECRET" \
  -d '{
    "test": "data",
    "variables": {
      "customVar": "value"
    }
  }'
```

### 8. 测试 Manual Trigger

```bash
# 手动触发 routine
curl -X POST "http://localhost:5173/api/routines/$ROUTINE_ID/trigger" \
  -H "Content-Type: application/json" \
  -d '{
    "variables": {
      "testVar": "manual value"
    }
  }'
```

## 性能基准

### 预期指标

- **RoutineCronTrigger**: 每 30 秒扫描一次，处理时间 < 500ms
- **MonitorCheckJob**: 每分钟扫描一次，处理时间 < 200ms
- **LeaseExpiryScanner**: 每分钟扫描一次，处理时间 < 100ms
- **EnvironmentHealthProber**: 每 5 分钟扫描一次，处理时间 < 1s
- **ConsistencyCheckJob**: 每小时扫描一次，处理时间 < 2s

### 监控查询

```sql
-- 查看 scheduler 执行统计（需要添加日志表）
SELECT 
    job_name,
    COUNT(*) as execution_count,
    AVG(duration_ms) as avg_duration_ms,
    MAX(duration_ms) as max_duration_ms,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) as failure_count
FROM job_execution_logs
WHERE executed_at > NOW() - INTERVAL '1 hour'
GROUP BY job_name
ORDER BY job_name;
```

## 已知问题和限制

1. **变量验证缺失**：当前不验证 routine 变量的有效性
2. **调度指纹缺失**：可能导致重复执行
3. **Catch-up 逻辑缺失**：不会补发错过的运行
4. **Concurrency policy 未实现**：所有 routine 都并行执行
5. **Managed routine binding 不支持**：无法关联 plugin operations

## 下一步优化

1. **实现变量验证和解析**
   - 从 paperclip 迁移 `assertRoutineVariableDefinitions`
   - 实现 `resolveRoutineVariableValues`
   - 添加模板插值功能

2. **实现调度指纹**
   - 添加 fingerprint 计算
   - 实现幂等性检查

3. **实现 Catch-up 逻辑**
   - 添加 MAX_CATCH_UP_RUNS 限制
   - 补发错过的运行

4. **添加监控和日志**
   - 创建 job_execution_logs 表
   - 记录每次 scheduler 执行

5. **性能优化**
   - 添加数据库索引
   - 批量处理 trigger
   - 优化查询

## 参考链接

- Paperclip routines service: `~/workspace/paperclip/server/src/services/routines.ts`
- Job scheduler 实现: `crates/services/src/job_scheduler.rs`
- Routine execution service: `crates/services/src/routine_execution_service.rs`
- Migration 文件: `migrations/20260711000006_create_routines.sql`
