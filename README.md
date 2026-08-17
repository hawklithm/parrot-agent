# Parrot Agent

[中文文档](README_CN.md)

Rust implementation of Paperclip's agent orchestration backend. Built with Axum, SQLx, and Tokio.

## Architecture Overview

```
parrot-agent/
├── Cargo.toml                  # Workspace root configuration
├── migrations/                 # SQL migration files (19 files)
├── docker-compose.yml          # PostgreSQL container configuration
└── crates/
    ├── models/                 # Domain models, enums, state machines
    ├── repositories/           # Data access layer (PostgreSQL via SQLx)
    ├── services/               # Business logic layer
    ├── api/                    # HTTP API (Axum routes, middleware, schemas)
    ├── access/                 # ABAC permission model
    ├── adapters/               # Adapter pattern (Process, Claude Local)
    ├── migrations/             # Migration runner
    └── server/                 # Main server application
        ├── src/
        │   ├── main.rs         # Server entry point
        │   └── bin/            # 20 utility programs
        └── examples/           # 3 example programs
```

## Core Feature Modules

| Module | Status | Description |
|--------|--------|-------------|
| **Agent Management** | ✅ | Agent CRUD, state machine, org chart, config revisions |
| **Issue/Case Management** | ✅ | Full lifecycle, tree control, checkout/release, diagnostics |
| **Task Watchdog** | ✅ | Subtree liveness classifier, periodic evaluation, fingerprinting |
| **Authentication** | ✅ | JWT, API Keys (Board + Agent), Session, Cloud Tenant |
| **Authorization** | ✅ | ABAC engine, field-level redaction, company isolation |
| **Event Bus** | ✅ | In-memory event bus with 7 listener types |
| **Adapter Plugin** | ✅ | npm-based plugin system with real npm install support |
| **Pipeline** | ✅ | Stage-based pipeline with case transitions |
| **Routine/Goal** | ✅ | Cron triggers, revision control, goal progress tracking |
| **Secret Management** | ✅ | Provider configs, remote import, environment binding |
| **Environment** | ✅ | Runtime leases, workspace isolation, codex_local isolation |

## Quick Start

### Requirements

- Rust 1.75+
- PostgreSQL 16
- (Optional) Docker & Docker Compose

### 1. Database Setup

**Option A: Using Docker Compose (Recommended)**

```bash
# Start PostgreSQL container
docker compose up -d postgres

# Database connection info
# Host: localhost:5433
# User: postgres
# Password: postgres
# Database: parrot_agent_dev
```

**Option B: Using Local PostgreSQL**

```bash
# Create database
createdb parrot_agent_dev

# Configure environment variable
export DATABASE_URL=postgres://postgres:admin123@localhost:5432/parrot_agent_dev
```

### 2. Configure Environment Variables

Edit the `.env` file:

```bash
# Database connection
DATABASE_URL=postgres://postgres:postgres@localhost:5433/parrot_agent_dev

# Deployment mode
DEPLOYMENT_MODE=local_trusted
```

### 3. Build and Run

```bash
# Build the entire workspace
cargo build --workspace

# Run the main server (migrations run automatically)
cargo run -p server

# Server listens on http://localhost:3100 by default
```

## Development Tools

### Utility Programs (in `crates/server/src/bin/`)

The server crate provides 20 management tools:

```bash
# Database Management
cargo run -p server --bin clear_db              # Clear database
cargo run -p server --bin clean_all_companies   # Clear all company data
cargo run -p server --bin truncate_all_data     # Truncate all tables
cargo run -p server --bin fix_db_tables         # Fix database table structure

# Migration Management
cargo run -p server --bin list_migrations          # List all migrations
cargo run -p server --bin verify_migrations        # Verify migration status
cargo run -p server --bin fix_migrations           # Fix migrations
cargo run -p server --bin fix_migration_checksum   # Fix migration checksums
cargo run -p server --bin apply_migration          # Apply migrations
cargo run -p server --bin clean_migration          # Clean migrations

# Data Repair
cargo run -p server --bin fix_agents_data       # Fix agent data

# Query and Analysis
cargo run -p server --bin query_db              # Query database
cargo run -p server --bin check_db              # Check database status
cargo run -p server --bin simple_query          # Simple query
cargo run -p server --bin test_uuid_query       # Test UUID query
cargo run -p server --bin analyze_all_tasks     # Analyze all tasks
cargo run -p server --bin analyze_hire          # Analyze hiring data
cargo run -p server --bin verify_duplicate_tasks # Verify duplicate tasks

# Testing Tools
cargo run -p server --bin test_user_directory   # Test user directory
cargo run -p server --bin clean_test_data       # Clean test data
```

### Example Programs (in `crates/server/examples/`)

```bash
cargo run -p server --example check_scheduling   # Check scheduling status
cargo run -p server --example reset_database     # Reset database
cargo run -p server --example verify_scheduler   # Verify scheduler
```

## Testing

```bash
# Run all library tests
cargo test --lib --workspace

# Run tests for specific crate
cargo test -p services
cargo test -p repositories
cargo test -p models

# Check compilation
cargo check --workspace
```

## Database Migrations

The project contains 19 SQL migration files that are automatically executed on server startup.

```bash
# View migration list
ls -lh migrations/*.sql

# Manually run migrations
cargo run -p server --bin apply_migration
```

Migration files use incremental numbering:
- `00_init_schema_unified.sql` - Initial complete schema
- `01_*.sql` ~ `18_*.sql` - Incremental migrations

## Main Dependencies

| Category | Dependencies |
|----------|-------------|
| **Web Framework** | Axum 0.7, Tower, Tower-HTTP |
| **Database** | SQLx 0.7 (PostgreSQL), SeaORM 0.12 |
| **Async Runtime** | Tokio (full features) |
| **Serialization** | Serde, Serde JSON |
| **UUID/Time** | UUID v4, Chrono |
| **Error Handling** | thiserror, anyhow |
| **Validation** | Garde 0.18 |
| **Logging** | Tracing, Tracing-subscriber |

## Claude Local Agent Configuration

Parrot Agent supports using Claude Code CLI as a local AI agent.

### Quick Configuration

1. **Install Claude Code CLI**
   ```bash
   npm install -g @anthropic-ai/claude-code
   claude --version
   ```

2. **Configure Environment Variables**

   Add to `.env` file or shell config file (`~/.zshrc`):
   ```bash
   ANTHROPIC_AUTH_TOKEN=your_token_here
   ANTHROPIC_BASE_URL=http://127.0.0.1:8787
   ANTHROPIC_MODEL=claude-3-5-sonnet-20241022
   ```

3. **Create Agent**

   ```bash
   # Using automation script
   ./setup-claude-local-agent.sh
   
   # Or using API
   curl -X POST http://localhost:3100/api/agents \
     -H "Content-Type: application/json" \
     -d @claude-local-agent-config.json
   ```

### Smart Environment Variable Reference

Adapter configuration supports environment variable references to avoid hardcoding sensitive information:

```json
{
  "adapter_config": {
    "env": {
      "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",  // Auto-read from environment
      "ANTHROPIC_BASE_URL": "ANTHROPIC_BASE_URL",
      "ANTHROPIC_MODEL": "ANTHROPIC_MODEL"
    },
    "command": "claude",
    "maxTurnsPerRun": 20
  }
}
```

The system automatically recognizes uppercase environment variable names and reads actual values from the host environment.

### Default Configuration (adapters/ directory)

The `adapters/` directory contains default configurations for each adapter, serving as a fallback for database configurations:

```
adapters/
├── claude-local.json      # Claude Local default config
├── codex-local.json       # Codex Local default config
└── README.md             # Detailed documentation
```

When creating an agent, if the database configuration is missing fields, the system automatically supplements them from the corresponding default configuration file.

**Configuration Merge Rule**: Database configuration takes priority, default configuration supplements missing fields.

### Detailed Documentation

- **Complete Feature Documentation**: [docs/adapter-env-var-reference.md](docs/adapter-env-var-reference.md)
- **Quick Start Guide**: [docs/QUICKSTART-env-var-reference.md](docs/QUICKSTART-env-var-reference.md)
- **Adapter Configuration**: [adapters/README.md](adapters/README.md)
- **MCP Gateway Runbook**: [docs/paperclip-mcp-runbook.md](docs/paperclip-mcp-runbook.md)

## Troubleshooting

### Issue: Database Connection Failed

```bash
# Check if PostgreSQL is running
docker compose ps

# View container logs
docker compose logs postgres

# Test connection
psql $DATABASE_URL -c "SELECT 1"
```

### Issue: Migration Failed

```bash
# Check migration status
cargo run -p server --bin verify_migrations

# Fix migration checksums
cargo run -p server --bin fix_migration_checksum

# View database tables
psql $DATABASE_URL -c "\dt"
```

### Issue: Claude Agent Authentication Failed

```bash
# Check environment variables
env | grep ANTHROPIC

# Test Claude CLI
claude chat "hello" --print

# View service logs
RUST_LOG=services=debug cargo run -p server
```

## Project Status

- ✅ Core feature modules complete
- ✅ Database migration system stable
- ✅ Claude Local Agent integration
- ✅ Adapter plugin system
- ✅ Complete development toolset

## Related Resources

- **Testing Guide**: [tests/TESTING_GUIDE.md](tests/TESTING_GUIDE.md)
- **Architecture Documentation**: [architecture/](architecture/)
- **Team Catalog**: [teams-catalog/](teams-catalog/)
- **Scripts**: [scripts/](scripts/)

## License

[To be added]
