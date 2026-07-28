# Founding Engineer Delegation Plan

Status: ready to delegate  
Delegator: CEO  
Primary assignee: Founding Engineer (once hired)  
Technical reviewer: CEO until a second senior engineer exists

## Direction

The company will earn trust by making agent execution dependable. The first milestone is a thin, observable, end-to-end workflow—not completion of every route in the Paperclip compatibility surface.

## Delegated work

### FE-001 — Establish the vertical-slice contract

Owner: Founding Engineer  
Priority: P0  
Depends on: none

Read the current `README.md`, `COMPARISON_REPORT.md`, `API_GAP_TASKS.md`, and the relevant services/routes. Write a one-page contract for: create agent, authenticate, create/assign issue, checkout, acquire lease, execute, heartbeat, complete, and recover.

Acceptance criteria:

- Lists request/response and state transitions for each step.
- Names the source of actor/user/company identity at every boundary.
- Names idempotency keys, timeout behavior, and compensating action for each side effect.
- CEO approves the slice before implementation begins.

### FE-002 — Remove identity placeholders from the slice

Owner: Founding Engineer  
Priority: P0  
Depends on: FE-001

Replace `Uuid::nil()` and ad-hoc generated user/company IDs in the selected workflow with authenticated context and persisted domain context. Start with `crates/api/src/routes/user_secrets.rs`, `approvals.rs`, `companies.rs`, `issues.rs`, `issue_tree_control.rs`, and any directly exercised service code.

Acceptance criteria:

- No placeholder identity remains on the tested path.
- Cross-company access is rejected.
- Tests cover authenticated user, agent key, missing actor, and wrong-company cases.

### FE-003 — Make execution persistence real

Owner: Founding Engineer  
Priority: P0  
Depends on: FE-001

Close the persistence gaps in the execution environment/workspace path, beginning with `crates/services/src/workspace_operation_service.rs` and the related repositories. Ensure issue checkout, environment lease, workspace creation, and runtime start have durable state and a clear rollback path.

Acceptance criteria:

- A service restart does not lose the active execution state.
- Duplicate requests are idempotent.
- Lease expiry and workspace cleanup are safe to retry.
- Database-backed tests cover success and partial failure.

### FE-004 — Integrate reliability signals

Owner: Founding Engineer  
Priority: P1  
Depends on: FE-002, FE-003

Connect the existing activity log, event bus, saga, watchdog, and scheduler components to the vertical slice. Make failed and compensated executions discoverable without reading logs manually.

Acceptance criteria:

- Every major transition emits a structured activity/event record.
- Failed steps retry according to an explicit policy and eventually enter a visible terminal state.
- Stale heartbeat/lease conditions produce an actionable diagnostic.
- A basic operator query or endpoint shows the run timeline.

### FE-005 — Ship the pilot test harness

Owner: Founding Engineer  
Priority: P1  
Depends on: FE-002, FE-003, FE-004

Create a repeatable local or CI test harness using the existing Cargo workspace and PostgreSQL setup. Cover the happy path plus timeout, authorization failure, duplicate submission, lease expiry, and compensation.

Acceptance criteria:

- One documented command runs the vertical-slice tests.
- Tests are deterministic and isolated.
- CI or equivalent local verification is recorded in the pull request.
- Known failures are tracked with owner and next action.

## Delegation cadence

- Monday: engineer proposes the week’s single outcome and identifies one risk.
- Wednesday: 20-minute CEO checkpoint; unblock decisions only.
- Friday: demo the workflow, review evidence, and re-rank the next task.

The engineer may split these tasks into smaller issues, but may not start more than one P0 implementation thread at a time. Any scope change that affects the pilot date requires CEO approval.

## Immediate CEO actions

1. Begin the targeted sourcing funnel in the hiring plan.
2. Prepare a short product narrative and a sanitized local setup guide.
3. Schedule the first candidate screen and reserve two technical-loop slots.
4. On acceptance, assign FE-001 in the first working session and review it within 48 hours.

