# Plan: Complete `cost_service.rs` — Replace hardcoded stubs with real DB-backed logic

## Context

`cost_service.rs` (crates/services/src/cost_service.rs) contains traits and default implementations for CostService, BudgetService, and FinanceService. Currently, `DefaultCostService` only has `create_event()` implemented; all other methods (`get_summary`, `by_agent`, `by_agent_model`, `by_provider`, `by_biller`, `by_project`, `window_spend`, `get_quota_windows`, `issue_cost_summary`) return empty/hardcoded values. Similarly, `DefaultBudgetService` and `DefaultFinanceService` are fully stub.

Additionally, `costs.rs` (crates/api/src/routes/costs.rs) has 20 route handlers that all return stubbed JSON. The AppState does not yet have `CostService`, `BudgetService`, or `FinanceService` fields.

The `cost_events` DB table (migrations/20260711000001_create_agents.sql) has only 5 columns (id, agent_id, amount_cents, event_type, created_at), but the `CostEvent` model expects 18 columns. A migration is needed to add the missing columns.

## Changes Required

### 1. New Migration: Add full cost_events schema columns

**File**: `migrations/20260719000001_alter_cost_events_add_columns.sql`

Add columns: company_id, issue_id, project_id, goal_id, heartbeat_run_id, billing_code, provider, biller, billing_type, model, input_tokens, cached_input_tokens, output_tokens, cost_cents, occurred_at

Also create indexes on company_id, occurred_at, provider, biller, model, project_id for aggregation queries.

### 2. Extend `CostEventRepository`

**File**: `crates/repositories/src/cost_event_repository.rs`

Add new methods to the trait and implement them in `PgCostEventRepository`:

| Method | SQL | Purpose |
|--------|-----|---------|
| `summarize(company_id, start, end)` | SUM(cost_cents), SUM(input_tokens), SUM(output_tokens), COUNT(*) WHERE company_id | Cost summary |
| `by_agent(company_id, start, end)` | GROUP BY agent_id | Cost by agent |
| `by_agent_model(company_id, start, end)` | GROUP BY agent_id, model | Cost by agent+model |
| `by_provider(company_id, start, end)` | GROUP BY provider | Cost by provider |
| `by_biller(company_id, start, end)` | GROUP BY biller | Cost by biller |
| `by_project(company_id, start, end)` | GROUP BY project_id (LEFT JOIN cost_events ON project_id IS NOT NULL) | Cost by project |
| `window_spend(company_id, start, end)` | SUM(cost_cents) WHERE company_id | Window spend |
| `issue_cost_summary(issue_id)` | SUM(cost_cents), etc WHERE issue_id | Issue cost |
| `list_by_company(company_id, start, end)` | SELECT * WHERE company_id | List events for company |

Each aggregation returns `Vec<CostSummaryRow>` where `CostSummaryRow` is a new model struct with `dimension: String`, `total_cost_cents: i64`, `total_input_tokens: i64`, `total_output_tokens: i64`, `event_count: i64`.

### 3. Implement `DefaultCostService`

**File**: `crates/services/src/cost_service.rs`

Replace all stub methods with real repository calls:

- `get_summary` → `repo.summarize()`
- `by_agent` → `repo.by_agent()`
- `by_agent_model` → `repo.by_agent_model()`
- `by_provider` → `repo.by_provider()`
- `by_biller` → `repo.by_biller()`
- `by_project` → `repo.by_project()`
- `window_spend` → `repo.window_spend()`
- `issue_cost_summary` → `repo.issue_cost_summary()`
- `get_quota_windows` — still returns empty (requires budget_policies table)
- `create_event` — already implemented, keep as-is

### 4. Add BudgetService & FinanceService (in-memory, minimal)

**File**: `crates/services/src/cost_service.rs`

Budget and Finance services remain in-memory since there are no dedicated DB tables. But enrich them slightly:
- `DefaultBudgetService.get_overview()` — read from company table budget_monthly_cents, and sum cost_events for current month
- `DefaultFinanceService` — stays in-memory (no finance_events table exists)

### 5. Add new model types for aggregation results

**File**: `crates/models/src/cost_event.rs`

Add `CostSummaryRow` struct for aggregation query results:
```rust
pub struct CostSummaryRow {
    pub dimension: String,
    pub total_cost_cents: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub event_count: i64,
}
```

### 6. Register services in AppState

**File**: `crates/api/src/app_state.rs`

Add:
```rust
pub cost_service: Arc<dyn CostService>,
pub budget_service: Arc<dyn BudgetService>,
pub finance_service: Arc<dyn FinanceService>,
```
And add to constructor + create_router wiring.

### 7. Rewrite route handlers in `costs.rs`

**File**: `crates/api/src/routes/costs.rs`

Replace all 20 stub handlers with real service calls using the AppState pattern:
- CO1: `state.cost_service.create_event(company_id, body)`
- CO3-CO10: `state.cost_service.get_summary()`, `by_agent()`, etc.
- CO11-CO14: `state.finance_service` calls
- CO15: `state.cost_service.issue_cost_summary()`
- CO16-CO20: `state.budget_service` calls

## Files to Modify

1. `migrations/20260719000001_alter_cost_events_add_columns.sql` — NEW migration
2. `crates/models/src/cost_event.rs` — Add CostSummaryRow
3. `crates/repositories/src/cost_event_repository.rs` — Add aggregation methods
4. `crates/services/src/cost_service.rs` — Implement real logic
5. `crates/services/src/lib.rs` — Re-export cost_service types
6. `crates/api/src/app_state.rs` — Add cost/budget/finance services
7. `crates/api/src/routes/costs.rs` — Rewrite handlers

## Verification

1. `cargo check -p models` — must compile
2. `cargo check -p repositories` — must compile
3. `cargo check -p services` — must compile
4. `cargo check -p api` — must compile (target: zero new warnings)
5. `cargo test -p api` — existing tests must pass
