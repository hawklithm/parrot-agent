# 队列管理和优雅终止功能测试指南

本文档提供了测试 parrot-agent 新实现的队列管理和优雅进程终止功能的详细指南。

## 功能概述

### 1. 队列管理
- **功能**: 当 agent 取消一个 run 后，自动启动队列中的下一个 run
- **配置**: `adapter_config.maxConcurrentRuns`（默认 1，最大 50）
- **优先级**: `in_progress` 的 issue > 高优先级 issue > 创建时间

### 2. 优雅进程终止
- **功能**: 取消 run 时先发送 SIGTERM，等待 grace period，超时后 SIGKILL
- **配置**: `adapter_config.graceSec`（默认 2 秒，范围 1-30 秒）
- **平台**: Unix (SIGTERM/SIGKILL), Windows (直接 kill)

---

## 前置要求

### 环境变量
```bash
export AUTH_TOKEN="your_auth_token_here"
export BASE_URL="http://localhost:3000"  # 可选，默认 localhost:3000
export COMPANY_ID="your_company_id"      # 可选，默认测试 UUID
```

### 依赖
- `curl`
- `jq`
- 运行中的 parrot-agent 服务

---

## 测试1: 队列自动启动

### 测试场景
1. 创建一个 `maxConcurrentRuns=2` 的 agent
2. 分配 3 个 issues
3. 前 2 个 run 启动，第 3 个进入队列
4. 取消第 1 个 run
5. **预期**: 第 3 个 run 自动从队列启动

### 运行测试
```bash
cd tests
./test_queue_management.sh
```

### 成功标准
```
✅ 创建 agent: <agent_id>
✅ 创建 Issue 1: <issue_id> (priority: 3)
✅ 创建 Issue 2: <issue_id> (priority: 2)
✅ 创建 Issue 3: <issue_id> (priority: 1)

Running runs: 2 (expected: 2)
Queued runs: 1 (expected: 1)

After cancel:
  Running runs: 2 (expected: 2)
  Queued runs: 0 (expected: 0)

✅ 测试通过！队列自动启动功能正常
```

### 验证点
- [ ] 初始状态: 2 个 running, 1 个 queued
- [ ] 取消后: 队列中的 run 自动启动
- [ ] 最终状态: 2 个 running, 0 个 queued

---

## 测试2: 优雅进程终止

### 测试场景
1. 创建一个 `graceSec=5` 的 agent
2. Agent 运行一个可以优雅处理 SIGTERM 的进程
3. 取消 run
4. **预期**: 进程收到 SIGTERM，在 2-5 秒内优雅退出

### 运行测试
```bash
cd tests
./test_graceful_termination.sh
```

### 成功标准
```
✅ 创建 agent: <agent_id>
   Grace period: 5 秒
✅ Run 已启动: <run_id>

  [1 s] Status: running
  [2 s] Status: running
  [3 s] Status: cancelled

终止耗时: 3 秒

✅ 优雅终止成功！进程在 grace period 内退出
   预期：≤ 5秒（grace period）
   实际：3 秒
```

### 验证点
- [ ] Run 在 grace period 内取消（≤ 5秒）
- [ ] 服务器日志显示 "sent SIGTERM"
- [ ] 服务器日志显示 "process exited gracefully"（或 "grace period expired, sending SIGKILL"）

### 日志验证
在服务器日志中查找：
```bash
# 成功案例（优雅退出）
grep "sent SIGTERM" logs/parrot-agent.log
grep "process exited gracefully" logs/parrot-agent.log

# 失败案例（强制终止）
grep "grace period expired, sending SIGKILL" logs/parrot-agent.log
```

---

## 手动测试

### 测试队列管理

#### 1. 创建测试 Agent
```bash
curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/agents" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Queue Test Agent",
    "adapterType": "claude_local",
    "adapterConfig": {
      "maxConcurrentRuns": 2,
      "command": "sleep",
      "args": ["30"]
    }
  }'
```

#### 2. 创建多个 Issues
```bash
for i in 1 2 3; do
  curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/issues" \
    -H "Authorization: Bearer $AUTH_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
      \"title\": \"Test Issue $i\",
      \"assigneeAgentId\": \"<agent_id>\"
    }"
done
```

#### 3. 检查状sh
# 查看运行中的 runs
curl "$BASE_URL/api/companies/$COMPANY_ID/agents/<agent_id>/heartbeat-runs?status=running" \
  -H "Authorization: Bearer $AUTH_TOKEN" | jq '.runs | length'

# 查看队列中的 runs
curl "$BASE_URL/api/companies/$COMPANY_ID/agents/<agent_id>/heartbeat-runs?status=queued" \
  -H "Authorization: Bearer $AUTH_TOKEN" | jq '.runs | length'
```

#### 4. 取消一个 Run
```bash
curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/issues/<issue_id>/cancel-run" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason": "Testing queue"}'
``### 5. 验证队列自动启动
等待 1-2 秒后，再次检查状态：
```bash
curl "$BASE_URL/api/companies/$COMPANY_ID/agents/<agent_id>/heartbeat-runs?status=running" \
  -H "Authorization: Bearer $AUTH_TOKEN" | jq '.runs | length'
# 应该仍然是 2（从队列启动了一个）
```

---

### 测试优雅终止

#### 1. 创建支持 SIGTERM 的 Agent
```bash
curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/agents" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Graceful Termination Test Agent",
    "adapterType": "claude_local",
    "adapterConfig": {
      "graceSec": 5,
      "command": "bash",
      "args": [
        "-c",
        "trap \"echo SIGTERM received; sleep 2; exit 0\" TERM; sleep 60 & wait"
      ]
    }
  }'
```

#### 2. 创建 Issue 并启动 Run
```bash
curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/issues" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"title\": \"Graceful Test\",
    \"assigneeAgentId\": \"<agent_id>\"
  }"
```

#### 3. 取消 Run 并计时
```bash
time curl -X POST "$BASE_URL/api/companies/$COMPANY_ID/issues/<issue_id>/cancel-run" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason": "Testing graceful termination"}'
```

#### 4. 检查日志
```bash
# 查找 SIGTERM 相关日志
tail -f logs/parrot-agent.log | grep -i "sigterm\|graceful"
```

**预期日志**:
```
[DEBUG] sent SIGTERM, waiting for graceful shutdown
[DEBUG] process exited gracefully
```

或（超时情况）:
```
[WARN] grace period expired, sending SIGKILL
```

---

## 配置示例

### Agent 配置模板
```json
{
  "name": "Production Agent",
  "adapterType": "claude_local",
  "adapterConfig": {
    // 队列管理
    "maxConcurrentRuns": 3,     // 同时运行的最大任务数（1-50）
    
    // 优雅终止
    "graceSec": 10,             // 优雅终止等待时间（1-30 秒）
    
    // 工作目录
    "cwd": "/path/to/workspace", // 可选，不配置则自动创建 ~/.parrot-agent/<company>/
    
    // 其他配置
    "command": "claude",
    "args": [],
    "env": {}
  }
}
```

---

## 故障排查

### 队列未自动启动

**症状**: 取消 run 后，队列中的任务没有启动

**检查**:
1. Agent 状态是否为 `idle` 或 `running`（不是 `paused`/`terminated`）
2. `maxConcurrentRuns` 配置是否正确
3. 服务器日志中是否有错误

**日志查找**:
```bash
grep "start_next_queued_run_for_agent" logs/parrot-agent.log
grep "no available slots" logs/parrot-agent.log
grep "agent not invokable" logs/parrot-agent.log
```

---

### 进程未优雅退出

**症状**: 所有进程都被 SIGKILL 强制终止

**检查**:
1. `graceSec` 配置是否足够长
2. 进程是否正确处理 SIGTERM 信号
3. Unix 平台是否支持信号处理

**日志查找**:
```bash
grep "grace period expired" logs/parrot-agent.log
grep "failed to send SIGTERM" logs/parrot-agent.log
```

---

## 性能基准

### 队列启动延迟
- **目标**: < 1 秒
- **测量**: `cancel_run` 调用到新 run 启动的时间

### 优雅终止时间
- **目标**: 根据 `graceSec` 配置
- **测量**: `cancel_run` 调用到 run 状态变为 `cancelled` 的时间

---

## 已知限制

1. **队列优先级**: 当前仅支持 issue priority 和创建时间，不支持自定义排序
2. **Grace Period**: 当前范围限制在 1-30 秒，不支持毫秒级配置
3. **Windows**: 不支持优雅终止，直接使用 `kill()`
4. **依赖检查**: 队列启动时不检查 issue 依赖关系（blocked issues）

---

## 后续改进建议

1. 添加队列状态 API endpoint（`GET /agents/:id/queue`）
2. 支持手动调整队列顺序
3. 添加进程终止的监控指标（graceful vs forced）
4. 支持 Windows 的优雅终止（通过 Ctrl+C 或其他机制）
