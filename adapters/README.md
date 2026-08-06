# Adapter 默认配置

这个目录存放各个 adapter 的默认配置文件，用于为数据库中的 agent 提供配置回退（fallback）。

## 工作原理

当 agent 执行任务时：

1. **首先**从数据库读取 `adapter_config`
2. **如果**数据库配置中缺少某些字段（如 `env`）
3. **则**加载 `adapters/{adapter_type}.json` 作为默认配置
4. **合并**配置（数据库配置优先于默认配置）

## 配置文件命名规则

文件名必须与 `adapter_type` 一致：

```
adapter_type: "claude_local"  → adapters/claude-local.json
adapter_type: "codex_local"   → adapters/codex-local.json
adapter_type: "openai"        → adapters/openai.json
```

注意：下划线 `_` 会被转换为横线 `-`

## 配置文件格式

### Claude Local (`claude-local.json`)

```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL": "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL": "ANTHROPIC_MODEL"
  },
  "command": "claude",
  "dangerouslySkipPermissions": true,
  "maxTurnsPerRun": 20,
  "effort": "high",
  "timeoutSec": 1800,
  "promptTemplate": "Task: {{issue.title}}\n\n{{issue.description}}\n\nComplete this task."
}
```

### 环境变量引用

`env` 字段中的值支持智能引用：

- `"ANTHROPIC_AUTH_TOKEN"` - 自动从环境变量读取
- `"$ANTHROPIC_AUTH_TOKEN"` - 显式引用
- `"${ANTHROPIC_AUTH_TOKEN}"` - Shell 风格引用"sk-ant-real-key"` - 直接值（不推荐用于敏感信息）

## 配置合并规则

**数据库配置优先**，默认配置填充缺失的字段：

### 示例

**数据库中的配置**：
```json
{
  "command": "claude",
  "maxTurnsPerRun": 10
}
```

**默认配置** (`adapters/claude-local.json`)：
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"
  },
  "command": "claude",
  "maxTurnsPerRun": 20,
  "effort": "high"
}
```

**最终合并结果**：
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"
  },
  "command": "claude",
  "maxTurnsPerRun": 10,
  "effort": "high"
}
```

## 使用场景

### 1. 新建 Agent 时使用默认配置

创建 agent 时可以不提供完整的 `adapter_config`：

```bash
curl -X POST http://localhost:3100/api/companies/$COMPANY_ID/agent-hires \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-claude-agent",
    "adapter_type": "claude_local",
    "adapter_config": {
      "maxTurnsPerRun": 10
    }
  }'
```

系统会自动从 `adapters/claude-local.json` 补充 `env` 等默认配置。

### 2. 升级现有 Agent

如果数据库中的旧 agent 缺少 `env` 配置，无需手动更新数据库，系统会自动使用默认配置。

### 3. 统一配置管理

修改 `adapters/claude-local.json` 可以统一更新所有使用默认配置的 agent。

## 添加新的 Adapter 配置

1. 在 `adapters/` 目录创建新文件，如 `openai.json`
2. 定义该 adapter 的默认配置
3. 确保文件名与 `adapter_type` 匹配（下划线转横线）

## 安全注意事项

⚠️ **不要在默认配置中硬编码敏感信息**

✅ **推荐**：使用环境变量引用
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"
  }
}
```

❌ **避免**：硬编码实际的 token
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-ant-actual-secret-token"
  }
}
```

## 配置验证

启动服务时会显示加载的默认配置：

```bash
RUST_LOG=services=debug cargo run --bin parrot-server
```

查看日志：
```
loaded default adapter config: adapters/claude-local.json
```
