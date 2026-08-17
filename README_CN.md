# Parrot Agent

Paperclip agent 编排后端的 Rust 实现。基于 Axum、SQLx 和 Tokio 构建。

## 架构概览

```
parrot-agent/
├── Cargo.toml                  # 工作区根配置
├── migrations/                 # SQL 迁移文件 (19 个)
├── docker-compose.yml          # PostgreSQL 容器配置
└── crates/
    ├── models/                 # 领域模型、枚举、状态机
    ├── repositories/           # 数据访问层 (PostgreSQL via SQLx)
    ├── services/               # 业务逻辑层
    ├── api/                    # HTTP API (Axum 路由、中间件、Schema)
    ├── access/                 # ABAC 权限模型
    ├── adapters/               # 适配器模式 (Process, Claude Local)
    ├── migrations/             # 迁移运行器
    └── server/                 # 主服务器程序
        ├── src/
        │   ├── main.rs         # 服务器入口
        │   └── bin/            # 20 个工具程序
        └── examples/           # 3 个示例程序
```

## 核心功能模块

| 模块 | 状态 | 说明 |
|------|------|------|
| **Agent 管理** | ✅ | Agent CRUD、状态机、组织架构、配置版本管理 |
| **Issue/Case 管理** | ✅ | 完整生命周期、树控制、checkout/release、诊断 |
| **Task Watchdog** | ✅ | 子树活性分类器、周期性评估、指纹识别 |
| **认证系统** | ✅ | JWT、API Keys (Board + Agent)、Session、Cloud Tenant |
| **权限系统** | ✅ | ABAC 引擎、字段级脱敏、公司隔离 |
| **事件总线** | ✅ | 内存事件总线，支持 7 种监听器类型 |
| **适配器插件** | ✅ | 基于 npm 的插件系统，支持真实 npm install |
| **Pipeline** | ✅ | 基于阶段的 pipeline，支持 case 状态转换 |
| **Routine/Goal** | ✅ | Cron 触发器、版本控制、目标进度跟踪 |
| **Secret 管理** | ✅ | Provider 配置、远程导入、环境绑定 |
| **环境管理** | ✅ | 运行时租约、workspace 隔离、codex_local 隔离 |

## 快速开始

### 环境要求

- Rust 1.75+
- PostgreSQL 16
- (可选) Docker & Docker Compose

### 1. 数据库配置

**方式 A：使用 Docker Compose (推荐)**

```bash
# 启动 PostgreSQL 容器
docker compose up -d postgres

# 数据库连接信息
# Host: localhost:5433
# User: postgres
# Password: postgres
# Database: parrot_agent_dev
```

**方式 B：使用本地 PostgreSQL**

```bash
# 创建数据库
createdb parrot_agent_dev

# 配置环境变量
export DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
```

### 2. 配置环境变量

编辑 `.env` 文件：

```bash
# 数据库连接
DATABASE_URL=postgres://postgres:postgres@localhost:5433/parrot_agent_dev

# 部署模式
DEPLOYMENT_MODE=local_trusted
```

### 3. 构建和运行

```bash
# 构建整个工作区
cargo build --workspace

# 运行主服务器（自动执行迁移）
cargo run -p server

# 服务器默认监听 http://localhost:3100
```

## 开发工具

### 工具程序 (在 `crates/server/src/bin/`)

服务器 crate 提供了 20 个管理工具：

```bash
# 数据库管理
cargo run -p server --bin clear_db              # 清空数据库
cargo run -p server --bin clean_all_companies   # 清空所有公司数据
cargo run -p server --bin truncate_all_data     # 截断所有表
cargo run -p server --bin fix_db_tables         # 修复数据库表结构

# 迁移管理
cargo run -p server --bin list_migrations          # 列出所有迁移
cargo run -p server --bin verify_migrations        # 验证迁移状态
cargo run -p server --bin fix_migrations           # 修复迁移
cargo run -p server --bin fix_migration_checksum   # 修复迁移校验和
cargo run -p server --bin apply_migration          # 应用迁移
cargo run -p server --bin clean_migration          # 清理迁移

# 数据修复
cargo run -p server --bin fix_agents_data       # 修复 agent 数据

# 查询和分析
cargo run -p server --bin query_db              # 查询数据库
cargo run -p server --bin check_db              # 检查数据库状态
cargo run -p server --bin simple_query          # 简单查询
cargo run -p server --bin test_uuid_query       # 测试 UUID 查询
cargo run -p server --bin analyze_all_tasks     # 分析所有任务
cargo run -p server --bin analyze_hire          # 分析招聘数据
cargo run -p server --bin verify_duplicate_tasks # 验证重复任务

# 测试工具
cargo run -p server --bin test_user_directory   # 测试用户目录
cargo run -p server --bin clean_test_data       # 清理测试数据
```

### 示例程序 (在 `crates/server/examples/`)

```bash
cargo run -p server --example check_scheduling   # 检查调度状态
cargo run -p server --example reset_database     # 重置数据库
cargo run -p server --example verify_scheduler   # 验证调度器
```

## 测试

```bash
# 运行所有库测试
cargo test --lib --workspace

# 运行特定 crate 的测试
cargo test -p services
cargo test -p repositories
cargo test -p models

# 检查编译
cargo check --workspace
```

## 数据库迁移

项目包含 19 个 SQL 迁移文件，自动在服务启动时执行。

```bash
# 查看迁移列表
ls -lh migrations/*.sql

# 手动运行迁移
cargo run -p server --bin apply_migration
```

迁移文件采用递增编号命名：
- `00_init_schema_unified.sql` - 初始化完整 schema
- `01_*.sql` ~ `18_*.sql` - 增量迁移

## 主要依赖

| 类别 | 依赖 |
|------|------|
| **Web 框架** | Axum 0.7, Tower, Tower-HTTP |
| **数据库** | SQLx 0.7 (PostgreSQL), SeaORM 0.12 |
| **异步运行时** | Tokio (full features) |
| **序列化** | Serde, Serde JSON |
| **UUID/时间** | UUID v4, Chrono |
| **错误处理** | thiserror, anyhow |
| **验证** | Garde 0.18 |
| **日志** | Tracing, Tracing-subscriber |

## Claude Local Agent 配置

Parrot Agent 支持使用 Claude Code CLI 作为本地 AI agent。

### 快速配置

1. **安装 Claude Code CLI**
   ```bash
   npm install -g @anthropic-ai/claude-code
   claude --version
   ```

2. **配置环境变量**

   在 `.env` 文件或 shell 配置文件 (`~/.zshrc`) 中添加：
   ```bash
   ANTHROPIC_AUTH_TOKEN=your_token_here
   ANTHROPIC_BASE_URL=http://127.0.0.1:8787
   ANTHROPIC_MODEL=claude-3-5-sonnet-20241022
   ```

3. **创建 Agent**

   ```bash
   # 使用自动化脚本
   ./setup-claude-local-agent.sh
   
   # 或使用 API
   curl -X POST http://localhost:3100/api/agents \
     -H "Content-Type: application/json" \
     -d @claude-local-agent-config.json
   ```

### 环境变量智能引用

Adapter 配置支持环境变量引用，避免硬编码敏感信息：

```json
{
  "adapter_config": {
    "env": {
      "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",  // 自动从环境读取
      "ANTHROPIC_BASE_URL": "ANTHROPIC_BASE_URL",
      "ANTHROPIC_MODEL": "ANTHROPIC_MODEL"
    },
    "command": "claude",
    "maxTurnsPerRun": 20
  }
}
```

系统会自动识别大写环境变量名并从宿主环境读取实际值。

### 默认配置 (adapters/ 目录)

`adapters/` 目录存放各 adapter 的默认配置，作为数据库配置的回退：

```
adapters/
├── claude-local.json      # Claude Local 默认配置
├── codex-local.json       # Codex Local 默认配置
└── README.md             # 详细说明
```

创建 agent 时，如果数据库配置缺少字段，系统会自动从对应的默认配置文件补充。

**配置合并规则**：数据库配置优先，默认配置补充缺失字段。

### 详细文档

- **完整功能文档**: [docs/adapter-env-var-reference.md](docs/adapter-env-var-reference.md)
- **快速开始指南**: [docs/QUICKSTART-env-var-reference.md](docs/QUICKSTART-env-var-reference.md)
- **Adapter 配置说明**: [adapters/README.md](adapters/README.md)
- **MCP Gateway 运行手册**: [docs/paperclip-mcp-runbook.md](docs/paperclip-mcp-runbook.md)

## 故障排查

### 问题：数据库连接失败

```bash
# 检查 PostgreSQL 是否运行
docker compose ps

# 查看容器日志
docker compose logs postgres

# 测试连接
psql $DATABASE_URL -c "SELECT 1"
```

### 问题：迁移失败

```bash
# 检查迁移状态
cargo run -p server --bin verify_migrations

# 修复迁移校验和
cargo run -p server --bin fix_migration_checksum

# 查看数据库表
psql $DATABASE_URL -c "\dt"
```

### 问题：Claude Agent 认证失败

```bash
# 检查环境变量
env | grep ANTHROPIC

# 测试 Claude CLI
claude chat "hello" --print

# 查看服务日志
RUST_LOG=services=debug cargo run -p server
```

## 项目状态

- ✅ 核心功能模块完成
- ✅ 数据库迁移系统稳定
- ✅ Claude Local Agent 集成
- ✅ Adapter 插件系统
- ✅ 完整的开发工具集

## 相关资源

- **测试指南**: [tests/TESTING_GUIDE.md](tests/TESTING_GUIDE.md)
- **架构文档**: [architecture/](architecture/)
- **团队目录**: [teams-catalog/](teams-catalog/)
- **脚本工具**: [scripts/](scripts/)

## License

[待补充]
