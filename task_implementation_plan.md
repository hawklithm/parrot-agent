# 三项任务实现方案

## 任务1: 启动队列中的下一个 run

### 实现位置
- 在 `cancel_run` 结束时调用
- 创建新的私有方法 `start_next_queued_run_for_agent`

### 实现逻辑（从 paperclip 迁移）
1. 检查 agent 是否可调用（不在 paused/terminated 状态）
2. 获取 maxConcurrentRuns 配置（默认 1）
3. 查询当前运行的 run 数量
4. 计算可用槽位
5. 查询队列中的 runs（按优先级排序）
6. 启动每个排队的 run（异步执行）

### 关键点
- 不在 `execute_run` 中调用（避免递归 Future 和 Send trait 问题）
- 使用 `tokio::spawn` 异步启动 run
- 需要添加 `clone_for_background` 方法

---

## 任务2: 优雅终止进程

### 实现位置
- 创建新的私有方法 `terminate_process_gracefully`
- 在 `cancel_run` 中替换直接 `kill()` 调用

### 实现逻辑（从 paperclip 迁移）
1. 获取进程 PID
2. 发送 SIGTERM（Unix）或直接 kill（Windows）
3. 等待 grace period（默认 2000ms）
4. 定期检查进程是否退出
5. 超时后发送 SIGKILL 强制终止

### 依赖
- 添加 `nix` crate 用于 Unix 信号处理
- 使用 `#[cfg(unix)]` 和 `#[cfg(not(unix))]` 区分平台

---

## 任务3: 前端添加工作目录配置

### 当前状态
- paperclip 已有 `cwd` 字段，但标记为 deprecated
- parrot-agent 后端已支持 `cwd` 字段（最近刚添加了默认目录逻辑）

### 实现方案
由于 parrot-agent 使用 paperclip 的前端，无需修改。如果需要独立前端：
1. 在 Agent 配置表单中添加 "Working Directory" 输入框
2. 保存到 `adapter_config.cwd`
3. 提供路径验证和目录选择器

### 结论
**无需修改** - parrot-agent 后端已完全支持 cwd 配置，前端可以通过 paperclip UI 配置

---

## 实施步骤

### Step 1: 添加依赖
```bash
cd crates/services && cargo add nix --features signal
```

### Step 2: 添加导入
```rust
use models::{Agent, AgentStatus, SseEvent, SseEventType};
```

### Step 3: 实现优雅终止函数（60行）
在 `impl DefaultHeartbeatService` 中添加

### Step 4: 实现队列启动函数（120行）
在 `impl DefaultHeartbeatService` 中添加

### Step 5: 添加 clone_for_background 方法（10行）
支持异步任务中使用 service

### Step 6: 修改 cancel_run 调用优雅终止和队列启动
替换 Line 1195 的直接 kill
在 Line 1199 后添加队列启动调用

### Step 7: 测试验证
- 测试 pause 按钮
- 测试队列自动启动
- 测试进程优雅终止
