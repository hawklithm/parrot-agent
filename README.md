# Parrot Agent

[中文文档](README_CN.md)

Rust implementation of Paperclip's agent orchestration backend. Built with Axum, SQLx, and Tokio.

## Architecture Overview

```
parrot-agent/
├── Cargo.toml                  # Workspace root configuration
├── migrations/                 # SQL migration files (83 files)
├── docker-compose.yml          # PostgreSQL + application container
├── adapters/                   # Default adapter JSON configs (claude-local.json)
├── docs/                       # E2E test cases & database reset docs
├── scripts/                    # Python/MJS/Shell scripts for migration & API tooling
├── tests/                      # Server-level test suite + TESTING_GUIDE.md
└── crates/
    ├── models/                 # Domain models, enums, state machines
    ├── repositories/           # Data access layer (PostgreSQL via SQLx)
    ├── services/               # Business logic layer
    ├── api/                    # HTTP API (Axum routes, middleware, schemas)
    ├── access/                 # ABAC permission model
    ├── adapters/               # Adapter pattern (Process, Claude Local)
    ├── migrations/             # Migration runner
    ├── cli/ (parrot-cli)       # CLI client (install, config, update, onboard, fix)
    └── server/ (parrot-server) # Main server application
        ├── src/
        │   ├── main.rs         # Server entry point
        │   └── bin/            # 21 utility programs
        └── examples/           # 2 example programs
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
docker compose -f docker-compose.yml up -d postgres

# Database connection info
# Host:     localhost:5433  (host port mapped to container 5432)
# User:     postgres
# Password: postgres
# Database: parrot_agent_dev
```

**Option B: Using Local PostgreSQL**

```bash
# Create database
createdb parrot_agent_dev

# Configure environment variable
export DATABASE_URL=postgres://postgres:postgres@localhost:5433/parrot_agent_dev
```

### 2. Configure Environment Variables

Edit the `.env` file:

```bash
# Database connection (match your postgres port)
DATABASE_URL=postgres://postgres:postgres@localhost:5433/parrot_agent_dev

# Deployment mode
DEPLOYMENT_MODE=local_trusted
```

### 3. Build and Run

```bash
# Build the entire workspace
cargo build --workspace

# Run the main server (migrations run automatically on startup)
cargo run -p parrot-server

# Server listens on http://localhost:3100 by default
```

## Development Tools

### Utility Programs (in `crates/server/src/bin/`)

**Database Management**

```bash
cargo run -p parrot-server --bin clear_db              # Clear database
cargo run -p parrot-server --bin clean_all_companies   # Clear all company data
cargo run -p parrot-server --bin truncate_all_data     # Truncate all tables
cargo run -p parrot-server --bin fix_db_tables         # Fix database table structure
cargo run -p parrot-server --bin reset_database        # Full schema reset + seed data
```

**Migration Management**

```bash
cargo run -p parrot-server --bin list_migrations          # List all migrations
cargo run -p parrot-server --bin verify_migrations        # Verify migration status
cargo run -p parrot-server --bin fix_migrations           # Fix migrations
cargo run -p parrot-server --bin fix_migration_checksum   # Fix migration checksums
cargo run -p parrot-server --bin apply_migration          # Apply pending migrations
cargo run -p parrot-server --bin clean_migration          # Clean migrations
```

**Data Repair & Queries**

```bash
cargo run -p parrot-server --bin fix_agents_data      # Fix agent data
cargo run -p parrot-server --bin query_db             # Query database
cargo run -p parrot-server --bin check_db             # Check database status
cargo run -p parrot-server --bin simple_query         # Simple query
cargo run -p parrot-server --bin test_uuid_query      # Test UUID query
cargo run -p parrot-server --bin analyze_all_tasks    # Analyze all tasks
cargo run -p parrot-server --bin analyze_hire         # Analyze hiring data
cargo run -p parrot-server --bin verify_duplicate_tasks # Verify duplicate tasks
```

**Testing Tools**

```bash
cargo run -p parrot-server --bin test_user_directory  # Test user directory
cargo run -p parrot-server --bin clean_test_data      # Clean test data
```

### Example Programs (in `crates/server/examples/`)

```bash
cargo run -p parrot-server --example check_scheduling   # Check scheduling status
cargo run -p parrot-server --example verify_scheduler   # Verify scheduler
```

## Testing

```bash
# Run all library tests
cargo test --lib --workspace

# Run tests for specific crate
cargo test -p services
cargo test -p repositories
cargo test -p models

# Run server integration tests
cargo test -p parrot-server

# Run HTTP parity tests (require running Postgres)
cargo test -p parrot-server --test issue_documents_http_parity_test

# Check compilation
cargo check --workspace
```

## Database Migrations

The project contains 83 SQL migration files, automatically executed on server startup.

```bash
# View migration list
ls migrations/*.sql | wc -l

# Manually run migrations
cargo run -p parrot-server --bin apply_migration

# Verify migration status
cargo run -p parrot-server --bin verify_migrations
```

Migration files use incremental numbering:
- `00_init_schema_unified.sql` — Initial complete schema
- `01_*.sql` ~ `80_*.sql` — Incremental migrations (80 files)
- `20260818*.sql`, `20260829*.sql` — Date-stamped migrations

## Main Dependencies

| Category | Dependencies |
|----------|-------------|
| **Web Framework** | Axum 0.7, Tower 0.4/0.5, Tower-HTTP 0.5 |
| **Database** | SQLx 0.7 (PostgreSQL), SeaORM 0.12 |
| **Async Runtime** | Tokio (full features) |
| **Serialization** | Serde, Serde JSON |
| **UUID/Time** | UUID 1.6, Chrono 0.4 |
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
   # Or using API
   curl -X POST http://localhost:3100/api/agents \
     -H "Content-Type: application/json" \
     -d '{
       "name": "my-claude-agent",
       "adapter_type": "claude_local",
       "adapter_config": {
         "command": "claude",
         "maxTurnsPerRun": 20,
         "effort": "high",
         "timeoutSec": 1800
       }
     }'
   ```

### Smart Environment Variable Reference

Adapter configuration supports environment variable references to avoid hardcoding sensitive information:

```json
{
  "adapter_config": {
    "env": {
      "ANTHROPIC_AUTH_TOKEN": "ANTHROPIC_AUTH_TOKEN",
      "ANTHROPIC_BASE_URL": "ANTHROPIC_BASE_URL",
      "ANTHROPIC_MODEL": "ANTHROPIC_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL": "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL": "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_OPUS_MODEL": "ANTHROPIC_DEFAULT_OPUS_MODEL"
    },
    "command": "claude",
    "dangerouslySkipPermissions": true,
    "maxTurnsPerRun": 20,
    "effort": "high",
    "timeoutSec": 1800,
    "promptTemplate": "Task: {{issue.title}}\n\n{{issue.description}}\n\nPlease complete this task step by step and report the final results."
  }
}
```

The system automatically recognizes uppercase environment variable names and reads actual values from the host environment.

### Default Configuration (adapters/ directory)

The `adapters/` directory contains default configurations for each adapter, serving as a fallback for database configurations:

```
adapters/
├── README.md             # Configuration merge rules
└── claude-local.json     # Claude Local default config
```

When creating an agent, if the database configuration is missing fields, the system automatically supplements them from the corresponding default configuration file.

**Configuration Merge Rule**: Database configuration takes priority; default configuration fills in missing fields.

### Detailed Documentation

- **Adapter Configuration**: [adapters/README.md](adapters/README.md)
- **E2E Test Cases**: [docs/E2E_TEST_CASES.md](docs/E2E_TEST_CASES.md)
- **Database Reset Tools**: [docs/RESET_TOOLS.md](docs/RESET_TOOLS.md)
- **MCP Gateway Runbook**: [docs/paperclip-mcp-runbook.md](docs/paperclip-mcp-runbook.md)

## Troubleshooting

### Issue: Database Connection Failed

```bash
# Check if PostgreSQL is running
docker compose -f docker-compose.yml ps

# View container logs
docker compose -f docker-compose.yml logs postgres

# Test connection
psql $DATABASE_URL -c "SELECT 1"
```

### Issue: Migration Failed

```bash
# Check migration status
cargo run -p parrot-server --bin verify_migrations

# Fix migration checksums
cargo run -p parrot-server --bin fix_migration_checksum

# Full database reset (drops schema, re-runs all migrations, seeds data)
cargo run -p parrot-server --bin reset_database
```

### Issue: Claude Agent Authentication Failed

```bash
# Check environment variables
env | grep ANTHROPIC

# Test Claude CLI
claude chat "hello" --print

# View service logs
RUST_LOG=services=debug cargo run -p parrot-server
```

## Project Structure

| Directory | Contents |
|-----------|----------|
| `crates/models/` | Domain types, enums, state machines |
| `crates/repositories/` | PostgreSQL data access (SQLx) |
| `crates/services/` | Business logic |
| `crates/api/` | HTTP routes, middleware, request/response schemas |
| `crates/access/` | ABAC permission model |
| `crates/adapters/` | Adapter patterns (Process, Claude Local) |
| `crates/migrations/` | Migration runner |
| `crates/cli/` | CLI client binary (`parrot-cli`) |
| `crates/server/` | Server binary + 21 utility programs + 2 examples |
| `migrations/` | 83 SQL migration files |
| `adapters/` | Default adapter JSON configs |
| `scripts/` | Migration & API tooling (Python, MJS, Shell) |
| `tests/` | Server integration tests + testing guide |
| `docs/` | E2E test cases and reset tool documentation |

## Related Resources

- **Testing Guide**: [tests/TESTING_GUIDE.md](tests/TESTING_GUIDE.md)
- **E2E Test Cases**: [docs/E2E_TEST_CASES.md](docs/E2E_TEST_CASES.md)
- **Architecture Docs**: [architecture/rust-impl-tasks/](architecture/rust-impl-tasks/)
- **Team Catalog**: [teams-catalog/catalog/](teams-catalog/catalog/)
- **Scripts**: [scripts/](scripts/)

## License

[To be added]
