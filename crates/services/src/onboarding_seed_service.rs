//! Company onboarding seed service.
//!
//! Port of Paperclip `server/src/services/onboarding-seed.ts` (415 lines) plus
//! `packages/shared/src/validators/onboarding-seed.ts`. Paperclip Cloud collects
//! the seed at signup and pushes it into the stack at activation; the route
//! answers 200 only once every part of the seed — goal, agent, first task, seed
//! record and audit entry — is durable.
//!
//! Everything runs on **one** transaction. Paperclip's `applyWithin(dbx, …)`
//! rebuilds its services on the transaction handle so every read and write is
//! serialized against a concurrent push for the same company; Parrot's
//! `GoalService` / `AgentService` / `ProjectService` / `IssueService` each open
//! their own `pool.begin()`, so they are not transaction-composable. The whole
//! application is therefore written as raw SQL against `&mut *tx`, which is what
//! makes the "audit write failure rolls the seed back" contract reachable.
//!
//! Deviations from Paperclip, each forced by a Parrot schema difference, are
//! marked `PAPERCLIP DEVIATION:` at the point they apply.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::LazyLock;
use uuid::Uuid;

use crate::errors::ServiceError;
use crate::live_events::publish_live_event;

/// The project the seeded first task lands in, matching the name the tenant's
/// own first-run wizard uses so a later manual run reuses it instead of creating
/// a second "Onboarding" project.
pub const ONBOARDING_SEED_PROJECT_NAME: &str = "Onboarding";

/// Role assigned to the seeded lead agent. The seed's own `agent.role` is
/// customer free text ("Chief of Staff") and lands on the title; `role` stays the
/// structural `ceo` key the org chart and default-instructions lookup read.
const SEEDED_AGENT_ROLE: &str = "ceo";

/// Adapter the seeded agent is created with. Mirrors the teams-catalog default
/// (`claude_local`), which is the safe adapter for agents created server-side
/// without a human running an environment test first.
const FALLBACK_SEEDED_AGENT_ADAPTER_TYPE: &str = "claude_local";

/// Paperclip `seededAgentAdapterType()`.
pub fn seeded_agent_adapter_type() -> String {
    let read = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    read("PAPERCLIP_ONBOARDING_SEED_ADAPTER_TYPE")
        .or_else(|| read("PAPERCLIP_TEAMS_CATALOG_DEFAULT_ADAPTER_TYPE"))
        .unwrap_or_else(|| FALLBACK_SEEDED_AGENT_ADAPTER_TYPE.to_string())
}

/// Paperclip `parseSeedMission`: split a free-text mission into a goal title and
/// description the way the first-run wizard's `parseOnboardingGoalInput` does —
/// first line is the title, the remainder is the description.
///
/// The split is `/\r?\n/`, i.e. on `\n` with an optional preceding `\r`.
pub fn parse_seed_mission(raw: &str) -> (String, Option<String>) {
    static NEWLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\r?\n").unwrap());

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }

    let mut lines = NEWLINE.split(trimmed);
    let title = lines.next().unwrap_or("").trim().to_string();
    let description = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    (
        title,
        if description.is_empty() {
            None
        } else {
            Some(description)
        },
    )
}

/// `trim() || null` — the normalization Paperclip applies to every free-text
/// seed field before it is compared, written or stored.
fn trim_to_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// zod `.optional()` accepts an *absent* key but not an explicit `null`, while
/// serde's `Option<T>` folds both into `None`. Paired with `#[serde(default)]`
/// this restores the distinction: absent takes the default, present-`null` is a
/// shape error and surfaces as a 400 like every other Zod shape failure.
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// `applyOnboardingSeedSchema` (`packages/shared/src/validators/onboarding-seed.ts`).
///
/// The schema is a plain `z.object`, not `.strict()`, so zod *strips* unknown
/// keys rather than rejecting them — serde's default behaviour, which is why
/// there is no `deny_unknown_fields` here.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSeedInput {
    pub revision: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub mission: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub agent: Option<OnboardingSeedAgent>,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub first_task: Option<OnboardingSeedFirstTask>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSeedAgent {
    pub name: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSeedFirstTask {
    pub title: String,
    #[serde(default, deserialize_with = "deserialize_present")]
    pub details: Option<String>,
}

/// Paperclip `OnboardingSeedApplication`.
#[derive(Debug, Clone, Serialize)]
pub struct OnboardingSeedApplication {
    pub revision: String,
    /// False when the stored revision already matched and nothing was re-applied.
    pub changed: bool,
    pub goal_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
}

/// The actor fields the audit entry needs, as Paperclip's `getActorInfo`
/// produces them. Narrowed to what the `activity_logs` row reads.
#[derive(Debug, Clone)]
pub struct OnboardingSeedAuditActor {
    pub actor_type: &'static str,
    pub actor_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

/// The stored seed record, as `readRecord` returns it.
struct OnboardingSeedRecord {
    revision: String,
    goal_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    issue_id: Option<Uuid>,
}

/// Apply an onboarding seed to a company.
///
/// Idempotent per `revision`: a replay of the revision already stored is a no-op
/// that still reports success, because Cloud reads any 2xx as "the tenant holds
/// this content" and retries otherwise. A *different* revision (the customer
/// edited their answers in Cloud) updates the goal, agent and task this seed
/// previously created rather than creating a second set.
///
/// Concurrency: Cloud's reconcile runs off portfolio fetches, which can overlap,
/// so two pushes for the same company can arrive at once. Both would otherwise
/// pass the revision check before either wrote the seed record and each create a
/// company goal, a lead agent and an Onboarding project. A per-company advisory
/// lock held for the transaction serializes them, so the second push sees the
/// first push's writes and updates in place instead of duplicating.
///
/// Auditing: when `audit` is supplied and the push changed anything, the
/// `company.onboarding_seed_applied` entry is written *inside* this same
/// transaction. Logging after the commit would force a choice between two broken
/// outcomes — answer 500 and the retry returns `changed: false` and never logs,
/// or answer 200 and Cloud stops retrying while the entry stays absent.
pub async fn apply(
    pool: &PgPool,
    company_id: Uuid,
    seed: &OnboardingSeedInput,
    audit: Option<&OnboardingSeedAuditActor>,
) -> Result<OnboardingSeedApplication, ServiceError> {
    let mut tx = pool.begin().await?;

    // Paperclip hashes the whole `paperclip:onboarding-seed:<companyId>` string in
    // one argument; the single-string `hashtextextended($1, 0)` form is the
    // in-repo convention (`pg_issue_repository.rs:683`, `routes/cases.rs:88`).
    let lock_key = format!("paperclip:onboarding-seed:{company_id}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await?;

    let applied = apply_within(&mut tx, company_id, seed).await?;

    let written_audit = if applied.changed { audit } else { None };
    if let Some(audit) = written_audit {
        sqlx::query(
            "INSERT INTO activity_logs (\
                 id, company_id, event_type, actor_type, actor_id, \
                 resource_type, resource_id, metadata, created_at, run_id, agent_id\
             ) VALUES ($1, $2, 'company.onboarding_seed_applied', $3, $4, \
                       'company', $5, $6, NOW(), $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(audit.actor_type)
        .bind(audit.actor_id)
        .bind(company_id)
        .bind(json!({
            "revision": applied.revision,
            "goalId": applied.goal_id,
            "agentId": applied.agent_id,
            "issueId": applied.issue_id,
        }))
        .bind(audit.run_id)
        .bind(audit.agent_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Published only after the transaction commits: the activity row is
    // transactional but its realtime fan-out is not, and announcing a seed that
    // then rolled back would be worse than announcing it late.
    if let Some(audit) = written_audit {
        publish_live_event(
            company_id,
            "activity.logged",
            json!({
                "actorType": audit.actor_type,
                "actorId": audit.actor_id.to_string(),
                "action": "company.onboarding_seed_applied",
                "entityType": "company",
                "entityId": company_id.to_string(),
                "agentId": audit.agent_id.map(|value| value.to_string()),
                "runId": audit.run_id.map(|value| value.to_string()),
                "details": {
                    "revision": applied.revision,
                    "goalId": applied.goal_id,
                    "agentId": applied.agent_id,
                    "issueId": applied.issue_id,
                },
            }),
        )
        .await;
    }

    Ok(applied)
}

async fn read_record(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
) -> Result<Option<OnboardingSeedRecord>, ServiceError> {
    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<Uuid>, Option<Uuid>)>(
        "SELECT revision, goal_id, agent_id, issue_id \
         FROM company_onboarding_seeds WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(|(revision, goal_id, agent_id, issue_id)| OnboardingSeedRecord {
        revision,
        goal_id,
        agent_id,
        issue_id,
    }))
}

async fn goal_still_exists(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    goal_id: Option<Uuid>,
) -> Result<bool, ServiceError> {
    let Some(goal_id) = goal_id else {
        return Ok(false);
    };
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM goals WHERE id = $1 AND company_id = $2")
            .bind(goal_id)
            .bind(company_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(found.is_some())
}

async fn issue_still_exists(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    issue_id: Option<Uuid>,
) -> Result<bool, ServiceError> {
    let Some(issue_id) = issue_id else {
        return Ok(false);
    };
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM issues WHERE id = $1 AND company_id = $2")
            .bind(issue_id)
            .bind(company_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(found.is_some())
}

/// Paperclip `getDefaultCompanyGoal` (`services/goals.ts:6-40`): three queries in
/// order, each `ORDER BY created_at ASC LIMIT 1`.
///
/// PAPERCLIP DEVIATION: Parrot's `GoalService` exposes no `get_default_company_goal`
/// (grep for `get_default_company_goal` in `crates/services/src/goal_service.rs`
/// → empty), so the three-step fallback is inlined here rather than called.
async fn get_default_company_goal(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
) -> Result<Option<Uuid>, ServiceError> {
    const QUERIES: [&str; 3] = [
        "SELECT id FROM goals WHERE company_id = $1 AND level = 'company' \
             AND status = 'active' AND parent_id IS NULL \
         ORDER BY created_at ASC LIMIT 1",
        "SELECT id FROM goals WHERE company_id = $1 AND level = 'company' \
             AND parent_id IS NULL \
         ORDER BY created_at ASC LIMIT 1",
        "SELECT id FROM goals WHERE company_id = $1 AND level = 'company' \
         ORDER BY created_at ASC LIMIT 1",
    ];

    for query in QUERIES {
        let found: Option<Uuid> = sqlx::query_scalar(query)
            .bind(company_id)
            .fetch_optional(&mut **tx)
            .await?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// The agent a re-push should update rather than duplicate: the one this seed
/// created if it is still around, else a pre-existing lead the tenant already
/// has. Built-in agents are excluded — they are provisioned by the platform and
/// are not the customer's first hire.
///
/// PAPERCLIP DEVIATION: Paperclip reads its built-in marker from the nested
/// `metadata.paperclipBuiltInAgent` object (`services/built-in-agent-metadata.ts:7`).
/// Parrot has no such marker; its built-ins carry `metadata.builtInKey` plus
/// `metadata.isBuiltIn` (`built_in_agent_service_impl.rs:707-712`, read back at
/// `summary_slot_worker.rs:38`), which is what the predicate below tests.
async fn resolve_target_agent_id(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    recorded_agent_id: Option<Uuid>,
) -> Result<Option<Uuid>, ServiceError> {
    if let Some(recorded_agent_id) = recorded_agent_id {
        let recorded: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM agents WHERE id = $1 AND company_id = $2")
                .bind(recorded_agent_id)
                .bind(company_id)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(id) = recorded {
            return Ok(Some(id));
        }
    }

    let candidates: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agents \
         WHERE company_id = $1 AND role = $2 AND status <> 'terminated' \
           AND NOT (metadata ? 'builtInKey') \
           AND COALESCE(metadata ->> 'isBuiltIn', 'false') <> 'true' \
         ORDER BY created_at ASC, id ASC",
    )
    .bind(company_id)
    .bind(SEEDED_AGENT_ROLE)
    .fetch_all(&mut **tx)
    .await?;

    Ok(candidates.into_iter().next())
}

/// The project the seeded first task lands in.
///
/// PAPERCLIP DEVIATION: Paperclip reuses a project when
/// `status !== "cancelled"` (`services/onboarding-seed.ts:129`), but Parrot's
/// `project_status` enum has no `cancelled` member — it is
/// `{backlog,todo,in_progress,in_review,blocked,done}` and retirement is
/// expressed by `projects.archived_at`. The predicate is therefore
/// `status <> 'done' AND archived_at IS NULL`.
async fn resolve_onboarding_project_id(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    goal_id: Option<Uuid>,
) -> Result<Uuid, ServiceError> {
    let reusable: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM projects \
         WHERE company_id = $1 AND status <> 'done' AND archived_at IS NULL \
           AND lower(btrim(name)) = lower($2) \
         ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .bind(company_id)
    .bind(ONBOARDING_SEED_PROJECT_NAME)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(id) = reusable {
        return Ok(id);
    }

    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (\
             id, company_id, goal_id, name, status, execution_workspace_policy, \
             created_at, updated_at\
         ) VALUES ($1, $2, $3, $4, 'in_progress', 'shared', NOW(), NOW())",
    )
    .bind(project_id)
    .bind(company_id)
    .bind(goal_id)
    .bind(ONBOARDING_SEED_PROJECT_NAME)
    .execute(&mut **tx)
    .await?;

    // Paperclip's `projectSvc.create` writes both the `project_goals` link rows
    // and the singular `projects.goalId` (`services/projects.ts:530-543`).
    // Parrot has no writer for `project_goals` today — neither
    // `PgProjectRepository::create` nor the project route touches it — so this is
    // the first, and it mirrors Paperclip rather than inventing a shape.
    if let Some(goal_id) = goal_id {
        sqlx::query(
            "INSERT INTO project_goals (project_id, goal_id, company_id, created_at, updated_at) \
             VALUES ($1, $2, $3, NOW(), NOW()) ON CONFLICT DO NOTHING",
        )
        .bind(project_id)
        .bind(goal_id)
        .bind(company_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(project_id)
}

/// The issue a replay should update rather than duplicate, resolved through the
/// idempotency key.
///
/// PAPERCLIP DEVIATION: Paperclip's `issueService.create` reads the idempotency
/// key table before inserting (`services/issues.ts:7004-7013`). Parrot's
/// `PgIssueRepository::create` inserts the key row *unconditionally* with no
/// `ON CONFLICT` (`pg_issue_repository.rs:870-884`) against a table with
/// `UNIQUE (company_id, idempotency_key)` — so a second create with the same key
/// would raise a constraint violation. The read-back is reproduced here instead.
async fn read_idempotent_issue(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    idempotency_key: &str,
) -> Result<Option<Uuid>, ServiceError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT i.id FROM issue_create_idempotency_keys k \
         JOIN issues i ON i.id = k.issue_id \
         WHERE k.company_id = $1 AND k.idempotency_key = $2 LIMIT 1",
    )
    .bind(company_id)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(found)
}

/// Paperclip `applyWithin`. Every read and write goes through the locked
/// transaction, so it is serialized against a concurrent push for the same
/// company.
async fn apply_within(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
    seed: &OnboardingSeedInput,
) -> Result<OnboardingSeedApplication, ServiceError> {
    let existing = read_record(tx, company_id).await?;
    if let Some(record) = &existing {
        if record.revision == seed.revision {
            return Ok(OnboardingSeedApplication {
                revision: record.revision.clone(),
                changed: false,
                goal_id: record.goal_id,
                agent_id: record.agent_id,
                issue_id: record.issue_id,
            });
        }
    }

    let mission = trim_to_option(seed.mission.as_deref());
    let agent_name = seed
        .agent
        .as_ref()
        .and_then(|agent| trim_to_option(Some(&agent.name)));
    let agent_role = seed
        .agent
        .as_ref()
        .and_then(|agent| trim_to_option(agent.role.as_deref()));
    let first_task_title = seed
        .first_task
        .as_ref()
        .and_then(|task| trim_to_option(Some(&task.title)));
    let first_task_details = seed
        .first_task
        .as_ref()
        .and_then(|task| trim_to_option(task.details.as_deref()));

    // 1. Mission → the company-level goal the dashboard reads.
    let mut goal_id = existing.as_ref().and_then(|record| record.goal_id);
    if let Some(mission) = &mission {
        let (title, description) = parse_seed_mission(mission);
        let target = if goal_still_exists(tx, company_id, goal_id).await? {
            goal_id
        } else {
            get_default_company_goal(tx, company_id).await?
        };

        match target {
            Some(target) => {
                // `goals.name` is the searchable mirror of `title` and is written
                // by `PgGoalRepository::update` (`goal_repository.rs:107-116`);
                // keeping both in step is what the repository does.
                sqlx::query(
                    "UPDATE goals SET title = $2::varchar, name = $2::text, description = $3, \
                     updated_at = NOW() \
                     WHERE id = $1",
                )
                .bind(target)
                .bind(&title)
                .bind(description.as_deref())
                .execute(&mut **tx)
                .await?;
                goal_id = Some(target);
            }
            None => {
                let created = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO goals (\
                         id, company_id, title, name, description, level, status, priority, \
                         parent_id, owner_agent_id, created_at, updated_at\
                     ) VALUES ($1, $2, $3::varchar, $3::text, $4, 'company', 'active', 'medium', \
                               NULL, NULL, NOW(), NOW())",
                )
                .bind(created)
                .bind(company_id)
                .bind(&title)
                .bind(description.as_deref())
                .execute(&mut **tx)
                .await?;
                goal_id = Some(created);
            }
        }
    }

    // 2. Agent → the customer's first hire, the lead the first task is assigned to.
    let mut agent_id =
        resolve_target_agent_id(tx, company_id, existing.as_ref().and_then(|r| r.agent_id)).await?;
    if let Some(agent_name) = &agent_name {
        match agent_id {
            Some(target) => {
                // PAPERCLIP DEVIATION: Paperclip writes the free-text role to
                // `agents.title` (`packages/db/src/schema/agents.ts:21`). Parrot's
                // `agents` table has no `title` column, so it is stored as
                // `metadata.title`, which is exactly where the agent detail
                // projection reads it (`routes/agents.rs:540-543`).
                //
                // Paperclip's `updateAgent` sets `title: agentRole` even when the
                // role is absent (null), so the key is written either way.
                sqlx::query(
                    "UPDATE agents \
                     SET name = $2, \
                         metadata = metadata || jsonb_build_object('title', $3::text), \
                         updated_at = NOW() \
                     WHERE id = $1",
                )
                .bind(target)
                .bind(agent_name)
                .bind(agent_role.as_deref())
                .execute(&mut **tx)
                .await?;
            }
            None => {
                let created = Uuid::new_v4();
                let permissions =
                    serde_json::to_value(models::AgentPermissions::for_role(models::AgentRole::Ceo))
                        .map_err(|error| ServiceError::Internal(error.to_string()))?;

                // Paperclip passes `permissions: {}`, which
                // `normalizeAgentPermissions` resolves to
                // `defaultPermissionsForRole("ceo")` = `{canCreateAgents: true,
                // canCreateSkills: true}`. Parrot's `AgentPermissions::for_role`
                // yields the same two flags plus the `trustPreset` /
                // `authorizationPolicy` fields its model requires
                // (`models/src/agent.rs:107-114`), and matches the column default
                // literal apart from the two `false` flags Paperclip turns on.
                //
                // PAPERCLIP DEVIATION: `lastHeartbeatAt: null` and
                // `spentMonthlyCents: 0` have no Parrot column / are the column
                // default; `runtimeConfig: {}` is stored as-is because Parrot has
                // no `normalizeRuntimeConfigForNewAgent` port to inject
                // `heartbeat.maxConcurrentRuns`.
                sqlx::query(
                    "INSERT INTO agents (\
                         id, company_id, name, role, status, adapter_type, adapter_config, \
                         runtime_config, permissions, metadata, budget_monthly_cents, \
                         spent_monthly_cents, reports_to, created_at, updated_at\
                     ) VALUES ($1, $2, $3, $4, 'idle', $5, '{}'::jsonb, '{}'::jsonb, $6, \
                               jsonb_build_object('title', $7::text), 0, 0, NULL, NOW(), NOW())",
                )
                .bind(created)
                .bind(company_id)
                .bind(agent_name)
                .bind(SEEDED_AGENT_ROLE)
                .bind(seeded_agent_adapter_type())
                .bind(&permissions)
                .bind(agent_role.as_deref())
                .execute(&mut **tx)
                .await?;
                agent_id = Some(created);
            }
        }
    }

    // 3. First task → an issue in the Onboarding project, assigned to the lead so
    //    the dashboard opens with work on it.
    let mut issue_id = existing.as_ref().and_then(|record| record.issue_id);
    if let Some(first_task_title) = &first_task_title {
        if issue_still_exists(tx, company_id, issue_id).await? {
            let target = issue_id.expect("issue exists");
            // Keep the task's relationships in step with a later revision that
            // supplied the agent or goal after the task already existed —
            // otherwise the record would report an assignee/goal the issue row
            // does not actually carry. `COALESCE` is the "only set when resolved"
            // rule: an absent value never clears an assignment the tenant made.
            sqlx::query(
                "UPDATE issues \
                 SET title = $2, \
                     description = $3, \
                     assignee_agent_id = COALESCE($4, assignee_agent_id), \
                     goal_id = COALESCE($5, goal_id), \
                     updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(target)
            .bind(first_task_title)
            .bind(first_task_details.as_deref())
            .bind(agent_id)
            .bind(goal_id)
            .execute(&mut **tx)
            .await?;
        } else {
            // The idempotency key is what protects two pushes that arrive at once.
            // It is deliberately not revision-scoped: if the recorded issue is
            // lost, a later revision should still dedupe against whatever the
            // first push created.
            let idempotency_key = format!("onboarding-seed:{company_id}");
            if let Some(existing_issue) =
                read_idempotent_issue(tx, company_id, &idempotency_key).await?
            {
                issue_id = Some(existing_issue);
            } else {
                let project_id =
                    resolve_onboarding_project_id(tx, company_id, goal_id).await?;

                // Paperclip's `issueService.create` mints the identifier from the
                // company counter, self-corrected against the highest
                // `issue_number` already stored (`services/issues.ts:7162-7179`),
                // then records the idempotency key.
                let (issue_number, issue_prefix): (i32, String) = sqlx::query_as(
                    "UPDATE companies \
                     SET issue_counter = GREATEST(\
                             issue_counter, \
                             COALESCE((SELECT MAX(issue_number) FROM issues WHERE company_id = $1), 0)\
                         ) + 1 \
                     WHERE id = $1 \
                     RETURNING issue_counter, issue_prefix",
                )
                .bind(company_id)
                .fetch_one(&mut **tx)
                .await?;
                let identifier = format!("{issue_prefix}-{issue_number}");

                let created = Uuid::new_v4();
                // PAPERCLIP DEVIATION: Parrot's `issues` has no
                // `responsible_user_id` producer on this path — Paperclip resolves
                // one from the actor/company defaults
                // (`services/issues.ts:257-308`), but the seed passes no actor
                // identity, so Paperclip's own resolution ends at
                // `createdByUserId ?? null`. The column is bound NULL to match.
                sqlx::query(
                    "INSERT INTO issues (\
                         id, company_id, project_id, goal_id, title, description, status, \
                         work_mode, priority, assignee_agent_id, issue_number, identifier, \
                         origin_kind, origin_fingerprint, request_depth, created_at, updated_at\
                     ) VALUES ($1, $2, $3, $4, $5, $6, 'todo', \
                               'standard', 'medium', $7, $8, $9, \
                               'manual', $10, 0, NOW(), NOW())",
                )
                .bind(created)
                .bind(company_id)
                .bind(project_id)
                .bind(goal_id)
                .bind(first_task_title)
                .bind(first_task_details.as_deref())
                .bind(agent_id)
                .bind(issue_number)
                .bind(&identifier)
                .bind(format!("onboarding-seed:{company_id}:{created}"))
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    "INSERT INTO issue_create_idempotency_keys (company_id, idempotency_key, issue_id) \
                     VALUES ($1, $2, $3)",
                )
                .bind(company_id)
                .bind(&idempotency_key)
                .bind(created)
                .execute(&mut **tx)
                .await?;

                issue_id = Some(created);
            }
        }
    }

    // 4. Record the revision last. Everything above has to have landed before
    //    this row claims the seed is applied.
    sqlx::query(
        "INSERT INTO company_onboarding_seeds (\
             id, company_id, revision, mission, agent_name, agent_role, \
             first_task_title, first_task_details, goal_id, agent_id, issue_id, \
             applied_at, created_at, updated_at\
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW(), NOW(), NOW()) \
         ON CONFLICT (company_id) DO UPDATE SET \
             revision = EXCLUDED.revision, \
             mission = EXCLUDED.mission, \
             agent_name = EXCLUDED.agent_name, \
             agent_role = EXCLUDED.agent_role, \
             first_task_title = EXCLUDED.first_task_title, \
             first_task_details = EXCLUDED.first_task_details, \
             goal_id = EXCLUDED.goal_id, \
             agent_id = EXCLUDED.agent_id, \
             issue_id = EXCLUDED.issue_id, \
             applied_at = EXCLUDED.applied_at, \
             updated_at = EXCLUDED.updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(&seed.revision)
    .bind(mission.as_deref())
    .bind(agent_name.as_deref())
    .bind(agent_role.as_deref())
    .bind(first_task_title.as_deref())
    .bind(first_task_details.as_deref())
    .bind(goal_id)
    .bind(agent_id)
    .bind(issue_id)
    .execute(&mut **tx)
    .await?;

    Ok(OnboardingSeedApplication {
        revision: seed.revision.clone(),
        changed: true,
        goal_id,
        agent_id,
        issue_id,
    })
}

/// Paperclip `onboardingSeedService().get` — the stored seed record, used by the
/// `GET` side of the service. Kept for parity even though the route only posts.
pub async fn get(
    pool: &PgPool,
    company_id: Uuid,
) -> Result<Option<Value>, ServiceError> {
    let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<Uuid>, Option<Uuid>, Option<Uuid>)>(
        "SELECT revision, mission, agent_name, agent_role, first_task_title, \
                first_task_details, goal_id, agent_id, issue_id \
         FROM company_onboarding_seeds WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(
            revision,
            mission,
            agent_name,
            agent_role,
            first_task_title,
            first_task_details,
            goal_id,
            agent_id,
            issue_id,
        )| {
            json!({
                "companyId": company_id,
                "revision": revision,
                "mission": mission,
                "agentName": agent_name,
                "agentRole": agent_role,
                "firstTaskTitle": first_task_title,
                "firstTaskDetails": first_task_details,
                "goalId": goal_id,
                "agentId": agent_id,
                "issueId": issue_id,
            })
        },
    ))
}
