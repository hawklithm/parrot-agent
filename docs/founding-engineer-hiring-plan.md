# Founding Engineer Hiring Plan

Status: approved by CEO for immediate execution  
Owner: CEO  
Role: Founding Engineer, Backend/Systems

## Hiring decision

Hire one senior, product-minded engineer as the first technical hire. The person will own the path from the current Rust backend to a reliable, usable agent-orchestration product. They should be comfortable making architecture decisions, writing production code, testing failure modes, and working directly with the CEO and early users.

This is not a narrow feature-owner role. The first engineer is accountable for the reliability spine: authenticated execution, durable state transitions, environment/workspace lifecycle, observability, and a thin end-to-end user journey.

## Why now

The repository already has broad API and domain coverage, but the planning and review artifacts identify integration risk as the bottleneck. The codebase needs one person with enough ownership to reduce the gap between “route exists” and “workflow works in production.”

## Role brief

### Mission

Make one complete agent workflow dependable: authenticate a user, create or select an agent, assign an issue, acquire an execution environment, run the agent, record activity/cost, and recover cleanly from failure.

### First 90-day outcomes

By day 30:

- Establish a reproducible local development and test workflow.
- Produce a written architecture/risk map and choose the first vertical slice.
- Close the highest-risk auth-context and persistence gaps in the chosen slice.

By day 60:

- Ship the first end-to-end workflow through API, database, execution environment, activity log, and error handling.
- Add integration tests for success, timeout, retry, and compensation paths.
- Add operational signals for failed runs, stale leases, and stuck issues.

By day 90:

- Run a pilot with at least one real internal user or design partner.
- Reduce the selected P0/P1 backlog to a small, explicitly accepted remainder.
- Publish the next 90-day technical/product roadmap with evidence from the pilot.

## Candidate scorecard

| Dimension | Weight | Strong evidence |
|---|---:|---|
| Systems/backend ownership | 25% | Has shipped and operated a stateful service end to end |
| Reliability and failure handling | 20% | Can explain retries, idempotency, leases, sagas, and recovery tradeoffs |
| Product judgment | 20% | Narrows scope to a useful vertical slice and validates with users |
| Rust/async/PostgreSQL ability | 15% | Can become productive in Axum, Tokio, SQLx, and migrations quickly |
| Security and authorization | 10% | Treats identity, tenant isolation, secrets, and auditability as product requirements |
| Communication and autonomy | 10% | Writes clearly, surfaces risks early, and makes reversible decisions |

Minimum bar: no hire if systems ownership, reliability, or product judgment is below strong. Rust experience is preferred but not required if the candidate has equivalent async backend experience and demonstrates a credible ramp plan.

## Sourcing profile

Prioritize engineers who have been an early technical hire, tech lead, or owner of a production platform. Source through trusted founder networks, infrastructure/open-source communities, and targeted referrals. The outreach message should lead with the mission and ownership, not a generic job description.

## Interview loop

1. CEO screen, 30 minutes: motivation, product taste, ownership, and working style.
2. Systems deep dive, 60 minutes: design the reliable agent-execution workflow, including identity, leases, retries, and recovery.
3. Practical pairing, 90 minutes: implement or review a small Rust service change with tests. Evaluate reasoning and maintainability, not typing speed.
4. Product/risk interview, 45 minutes: choose what to cut from a broad roadmap and define success metrics.
5. Reference checks, two references: specifically ask about ownership under ambiguity, incident behavior, and follow-through.
6. Final CEO decision within 48 hours of the last interview.

Use the same scorecard for every candidate. Do not advance a candidate on charisma or framework familiarity alone.

## Offer guardrails

The offer should be competitive for the market and stage, with meaningful early-employee equity and clear decision authority. Exact cash, equity, location, and start date require the company’s financial and legal context; they are intentionally left as CEO decisions rather than invented here.

## Hiring funnel targets

- 30 targeted outreaches
- 10 qualified screens
- 4 technical loops
- 1 hire

Review funnel conversion weekly. If the first 10 screens do not produce two strong technical loops, revise the pitch or sourcing profile before increasing volume.

## CEO responsibilities

- Personally source the first 30 candidates and run every final interview.
- Provide a crisp product narrative, access to the codebase, and fast decisions.
- Protect the engineer from a 500-item backlog by maintaining one committed vertical slice.
- Review progress weekly against outcomes, not activity.

