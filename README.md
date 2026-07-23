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

For local development, start PostgreSQL with Docker Compose:

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
