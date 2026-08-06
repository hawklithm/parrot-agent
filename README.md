# Parrot Agent

A Rust port of Paperclip's agent orchestration backend. Built with Axum, SQLx, and Tokio.

## Architecture

```
parrot-agent/
├── Cargo.toml                  # Workspace root
├── migrations/                 # SQL migrations (31 files)
└── crates/
    ├── models/                 # Domain models, enums, state machines
    ├── repositories/           # Data access layer (PostgreSQL via SQLx)
    ├── services/               # Business logic layer
    ├── api/                    # HTTP API (Axum routes, middleware, schemas)
    ├── access/                 # ABAC permission model
    ├── adapters/               # Adapter pattern (Process, Claude Local)
    └── migrations/             # Migration runner
```

## Core Modules

| Module | Status | Description |
|--------|--------|-------------|
| **Agent Management** | ✅ Complete | Agent CRUD, state machine, org chart, config revisions |
| **Issue/Case Management** | ✅ Complete | Full lifecycle, tree control, checkout/release, diagnostics |
| **Task Watchdog** | ✅ Complete | Subtree liveness classifier, periodic evaluation, fingerprinting |
| **Authentication** | ✅ Complete | JWT, API keys (Board + Agent), Session, Cloud Tenant |
| **Authorization** | ✅ Complete | ABAC engine, field-level redaction, company isolation |
| **Event Bus** | ✅ Complete | InMemory event bus with 7 listener types |
| **Adapter Plugin** | ✅ Complete | npm-based plugin system with real npm install support |
| **Pipeline** | ✅ Complete | Stage-based pipeline with case transitions |
| **Routine/Goal** | ✅ Complete | Cron triggers, revision control, goal progress tracking |
| **Secrets** | ✅ Complete | Provider configs, remote import, environment binding |
| **Environment** | ✅ Complete | Runtime leases, workspace isolation, codex_local isolation |

## Key Features

- **Watchdog Subsystem** — Monitors issue subtrees for liveness. When a subtree stops (no live execution paths), creates a review issue for the watchdog agent. Includes 5-state classifier (Live, Stopped, PendingFirstRun, AlreadyReviewed, NotApplicable) and stable fingerprinting.

- **Adapter Plugin System** — Supports npm-based plugins and local path loading. Reads `package.json` for metadata and entry point. Error-typed with `AdapterPluginError`.

- **Event-Driven Architecture** — In-memory event bus with typed events (Issue, Approval, Routine, Agent, Environment, Goal). Listeners for watchdog evaluation, recovery reconciliation, goal progress updates, and more.

- **Auth Middleware** — Multi-strategy: Bearer token (Board API Key `bak_*`, Agent API Key `aak_*`, JWT), Session Cookie, Cloud Tenant Header, Local implicit. Rate-limited with audit logging.

## Database

31 SQL migration files covering all tables. Run via:

```rust
migrations::run_migrations(&pool).await?;
```

For local development, PostgreSQL may be either a local service or Docker. For
the existing local database used by the Paperclip migration:

```bash
export DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
export PAPERCLIP_API_URL=http://127.0.0.1:3102/api
cargo run -p parrot-server
```

The server runs idempotent migrations against the existing database; do not
drop and recreate `parrot_agent_dev` to resolve schema errors. The MCP gateway
runbook is in [`docs/paperclip-mcp-runbook.md`](docs/paperclip-mcp-runbook.md).

Alternatively, start PostgreSQL with Docker Compose:

```bash
docker compose up -d postgres
cargo run -p parrot-server
```

The container publishes PostgreSQL on `localhost:5433`, matching the default
`DATABASE_URL` in `.env`. Data is kept in the `parrot-agent-postgres-data`
Docker volume.

## Quick Start

```bash
# Build
cargo build --workspace

# Check
cargo check --workspace

# Test (lib only - some test modules have pre-existing compilation issues)
cargo test --lib -p services
```

## Dependencies

- **Web**: Axum 0.7, Tower, Tower-HTTP
- **DB**: SQLx 0.7 (PostgreSQL), SeaORM 0.12
- **Async**: Tokio (full features)
- **Serialization**: Serde, Serde JSON
- **Auth**: SHA-2, UUID v4
- **Validation**: Garde 0.18

## 🤖 Claude Local Agent 配置

parrot-agent 支持使用 Claude Code CLI 作为本地 AI agent。通过**环境变量智能引用功能**，你可以安全地配置认证信息，避免硬编码敏感数据。

### 前置条件

1. **安装 Claude Code CLI**
   ```bash
   npm install -g @anthropic-ai/claude-code
   claude --version  # 验证安装
   ```

2. **配置环境变量**（二选一或两者结合）
   
   **方式 A：在项目 `.env` 文件中**（推荐用于本地开发）
   ```bash
   # 数据库配置
   DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
   
   # Claude 认证配置
   ANTHROPIC_AUTH_TOKEN=your_token_here
   ANTHROPIC_BASE_URL=http://127.0.0.1:8787
   ANTHROPIC_MODEL=claude-3-5-sonnet-20241022
   ```
   
   **方式 B：在 Shell 配置文件中**（推荐用于全局使用）
   
   编辑 `~/.zshrc` 或 `~/.bashrc`：
   ```bash
   export ANTHROPIC_AUTH_TOKEN="your_token_here"
   export ANTHROPIC_BASE_URL="http://127.0.0.1:8787"
   export ANTHROPIC_MODEL="claude-3-5-sonnet-20241022"
   ```
   
   然后执行：
   ```bash
   source ~/.zshrc  # 或 source ~/.bashrc
   ```

### 环境变量优先级

系统会按以下优先级加载环境变量：

1. **操作系统环境变量**（优先级最高）
   - Shell 中设置的：`export ANTHROPIC_AUTH_TOKEN="xxx"`
   - `~/.zshrc` 或 `~/.bashrc` 中的全局配置

2. **项目 `.env` 文件**
   - 仅在操作系统环境变量不存在时生效
   - 适合本地开发环境

**工作原理**：
```
服务启动
  ↓
加载 .env 文件到进程环境变量
  ↓
如果操作系统已有同名变量，保持不变（不覆盖）
  ↓
Agent 执行时从进程环境变量读取
```

**示例**：
```bash
# 操作系统环境变量
export ANTHROPIC_AUTH_TOKEN="token_from_shell"

# .env 文件中
ANTHROPIC_AUTH_TOKEN=token_from_dotenv

# 实际使用：token_from_shell（操作系统优先）
```
### 快速配置

#### 方式 1：使用自动化脚本（推荐）

```bash
# 检查环境并创建 agent
./setup-claude-local-agent.sh
```

#### 方式 2：使用 SQL 脚本

```bash
# 如果你有 PostgreSQL 客户端
psql $DATABASE_URL -f setup-claude-local-agent.sql

# 或使用简化脚本
./final_setup_claude.sh
```

#### 方式 3：通过 API 使用项目配置文件

项目中已包含完整的配置文件 `claude-local-agent-config.json`：

```bash
# 查看配置文件
cat claude-local-agent-config.json

# 通过 API 创建 agent
curl -X POST http://localhost:3100/api/agents \
  -H "Content-Type: application/json" \
  -d @claude-local-agent-config.json
```

**配置文件结构**（`claude-local-agent-config.json`）：
```json
{
  "name": "claude-local-agent",
  "adapter_type": "claude_local",
  "adapter_config": {
    "env": {
      "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",
      "ANTHROPIC_BASE_URL": "ANTHROPIC_BASE_URL",
      "ANTHROPIC_MODEL": "ANTHROPIC_MODEL"
    },
    "command": "claude",
    "dangerouslySkipPermissions": true,
    "maxTurnsPerRun": 20,
    "effort": "high",
    "timeoutSec": 1800
  }
}
```

### 环境变量智能引用

**核心特性**：在 `adapter_config.env` 中，你可以引用宿主环境变量，而不是硬编码敏感信息。

**工作原理**：
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"  // ← 引用环境变量
  }
}
```

系统会自动：
1. 识别这是一个环境变量引用（纯大写字母数字下划线）
2. 从宿主环境读取 `$ANTHROPIC_AUTH_TOKEN` 的实际值
3. 传递给 Claude CLI

**支持的引用格式**：
- `"ANTHROPIC_AUTH_TOKEN"` - 直接格式
- `"$ANTHROPIC_AUTH_TOKEN"` - Shell 风格
- `"${ANTHROPIC_AUTH_TOKEN}"` - Shell 风格（带花括号）

**混合使用示例**：
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",  // 从环境读取
    "ANTHROPIC_BASE_URL": "http://localhost:8787",   // 直接值
    "ANTHROPIC_MODEL": "claude-3-opus"               // 直接值
  }
}
```

**执行流程**：
```
Agent 启动
  ↓
读取 adapter_config.env: {"ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN"}
  ↓
resolve_env_value("ANTHROPIC_AUTH_TOKEN")  // 智能解析函数
  ↓
识别为环境变量引用（纯大写）
  ↓
std::env::var("ANTHROPIC_AUTH_TOKEN")  // 从系统环境读取
  ↓
获取实际值: "ck_fr6dggn0zxfk.xxx..."
  ↓
传递给 Claude CLI
  ↓
✅ 认证成功，不再报 "claude_auth_required"
```

### 验证配置

1. **检查环境变量**
   ```bash
   env | grep ANTHROPIC
   ```
   应该能看到你的认证配置。

2. **启动服务并查看日志**
   ```bash
   RUST_LOG=services=debug cargo run --bin parrot-server
   ```
   
   应该能看到：
   ```
   resolved env var reference from host environment, key=ANTHROPIC_AUTH_TOKEN
   ```

3. **测试 Claude CLI**
   ```bash
   claude --version
   claude chat "hello" --print
   ```

4. **创建测试任务**
   ```bash
   curl -X POST http://localhost:3100/api/issues \
     -H "Content-Type: application/json" \
     -d '{
       "title": "测试任务",
       "description": "创建一个 Hello World 脚本",
       "agentId": "<your-agent-id>"
     }'
   ```

### 故障排查

#### 问题 1: 报错 "claude_auth_required"

**原因**：环境变量未正确传递给 Claude CLI

**解决步骤**：
1. 确认环境变量已设置：`echo $ANTHROPIC_AUTH_TOKEN`
2. 确认 agent 配置中的 `env` 字段使用了引用格式
3. 检查 `.env` 文件是否包含认证配置
4. 重启服务，确保加载了最新的环境变量
5. 查看日志确认环境变量解析成功（应该有 "resolved env var reference"）

#### 问题 2: 环境变量没有被引用

**可能原因**：值包含小写字母或特殊字符，被识别为直接值

**解决**：使用明确的引用格式
```json
{
  "MY_TOKEN": "$MY_TOKEN"  // 使用 $ 前缀明确表示引用
}
```

#### 问题 3: Agent 未创建成功

**检查数据库**：
```bash
# 如果安装了 psql
psql $DATABASE_URL -c "SELECT id, name, adapter_type FROM agents WHERE adapter_type='claude_local';"
```

**重新创建**：
```bash
./final_setup_claude.sh
```

### 项目配置文件说明


| 文件 | 说明 |
|------|------|
| `claude-local-agent-config.json` | ✅ **主配置文件** - Agent 完整配置模板（可直接使用） |
| `setup-claude-local-agent.sql` | SQL 数据库插入脚本 |
| `setup-claude-local-agent.sh` | 自动化配置脚本（带环境检查） |
| `final_setup_claude.sh` | 简化版配置脚本（快速执行） |
| `.env` | 环境变量配置文件（包含 `ANTHROPIC_*` 认证信息） |
| `CLAUDE_AGENT_SETUP_COMPLETE.md` | 配置完成总结文档 |

### 详细文档

- **完整功能文档**: [docs/adapter-env-var-reference.md](docs/adapter-env-var-reference.md)
- **快速开始指南**: [docs/QUICKSTART-env-var-reference.md](docs/QUICKSTART-env-var-reference.md)
- **实现方案说明**: [docs/IMPLEMENTATION-env-var-reference.md](docs/IMPLEMENTATION-env-var-reference.md)
- **配置完成总结**: [CLAUDE_AGENT_SETUP_COMPLETE.md](CLAUDE_AGENT_SETUP_COMPLETE.md)
- **示例配置文件**: [docs/examples/claude-agent-with-env-ref.json](docs/examples/claude-agent-with-env-ref.json)

### 安全建议

✅ **推荐做法**：
- 使用环境变量引用，不要硬编码 API Key
- 将 `.env` 文件添加到 `.gitignore`
- 在不同环境（dev/staging/prod）使用不同的环境变量值
- 在 shell 配置文件（`~/.zshrc`）中设置全局环境变量

❌ **避免**：
- 在配置文件中硬编码敏感信息
- 将包含真实 token 的 `.env` 文件提交到版本控制
- 在日志中输出完整的认证 token

### 核心实现

环境变量智能引用功能的核心实现在 `crates/services/src/heartbeat_service.rs`：

```rust
fn resolve_env_value(configured_value: &str) -> String {
    let trimmed = configured_value.trim();
    let key = trimmed
        .strip_prefix("${").and_then(|s| s.strip_suffix("}"))
        .or_else(|| trimmed.strip_prefix("$"))
        .unwrap_or(trimmed);
    
    let looks_like_env_var = !key.is_empty() 
        && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    
    if looks_like_env_var {
        if let Ok(env_value) = std::env::var(key) {
            if !env_value.is_empty() {
                return env_value;
            }
        }
    }
    
    configured_value.to_string()
}
```

