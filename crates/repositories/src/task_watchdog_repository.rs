//! Repository layer for the task-watchdog subsystem.
//!
//! Mirrors paperclip's task-watchdogs data access: heartbeat runs
//! (live/terminal execution paths), issue watchdogs (one per watched
//! issue), agent wakeup requests, and issue thread interactions.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::task_watchdog::{
    AgentWakeupRequest, AgentWakeupRequestStatus, HeartbeatRun, HeartbeatRunStatus,
    IssueThreadInteraction, IssueThreadInteractionStatus, IssueWatchdog, IssueWatchdogStatus,
    TaskWatchdogClassifierIssue,
};

// ============================================================================
// heartbeat_runs
// ============================================================================

#[async_trait]
pub trait HeartbeatRunRepository: Send + Sync {
    /// Live runs (queued|running) whose context references one of `issue_ids`.
    async fn list_live_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<HeartbeatRun>>;

    /// Terminal runs (succeeded|failed|cancelled|timed_out) whose issue
    /// (via execution_run_id) is one of `issue_ids`.
    async fn list_terminal_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<HeartbeatRun>>;
}

pub struct PostgresHeartbeatRunRepository {
    pool: PgPool,
}

impl PostgresHeartbeatRunRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const LIVE_RUN_STATUSES: &[&str] = &["queued", "running"];
const TERMINAL_RUN_STATUSES: &[&str] = &["succeeded", "failed", "cancelled", "timed_out"];

#[async_trait]
impl HeartbeatRunRepository for PostgresHeartbeatRunRepository {
    async fn list_live_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<HeartbeatRun>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }
        let runs = sqlx::query_as::<_, HeartbeatRun>(
            r#"SELECT id, company_id, agent_id, invocation_source, status, responsible_user_id,
                      started_at, finished_at, error, exit_code, context_snapshot, created_at, updated_at
               FROM heartbeat_runs
              WHERE company_id = $1
                AND status = ANY($2)
                AND (
                  context_snapshot->>'issueId' = ANY($3::text[])
                  OR context_snapshot->>'taskId' = ANY($3::text[])
                )"#,
        )
        .bind(company_id)
        .bind(LIVE_RUN_STATUSES)
        .bind(issue_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await?;
        Ok(runs)
    }

    async fn list_terminal_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<HeartbeatRun>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }
        let runs = sqlx::query_as::<_, HeartbeatRun>(
            r#"SELECT hr.id, hr.company_id, hr.agent_id, hr.invocation_source, hr.status,
                      hr.responsible_user_id, hr.started_at, hr.finished_at, hr.error, hr.exit_code,
                      hr.context_snapshot, hr.created_at, hr.updated_at
               FROM heartbeat_runs hr
               JOIN issues i ON i.execution_run_id = hr.id
              WHERE hr.company_id = $1
                AND hr.status = ANY($2)
                AND i.id = ANY($3::uuid[])"#,
        )
        .bind(company_id)
        .bind(TERMINAL_RUN_STATUSES)
        .bind(issue_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(runs)
    }
}

// ============================================================================
// issue_watchdogs
// ============================================================================

#[async_trait]
pub trait IssueWatchdogRepository: Send + Sync {
    /// Upsert the watchdog row for (company_id, issue_id).
    async fn upsert(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        watchdog_agent_id: Uuid,
        instructions: Option<&str>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<&str>,
        created_by_run_id: Option<Uuid>,
    ) -> RepositoryResult<IssueWatchdog>;

    /// Fetch the watchdog row for a watched issue, if any.
    async fn get_by_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> RepositoryResult<Option<IssueWatchdog>>;

    /// Find the synthetic watchdog review issue for a source issue
    /// (origin_kind = 'task_watchdog' AND origin_id = source issue id).
    async fn find_watchdog_issue(
        &self,
        company_id: Uuid,
        source_issue_id: Uuid,
    ) -> RepositoryResult<Option<models::Issue>>;

    /// Load the watched issue's subtree (recursive children), excluding
    /// watchdog-origin issues and hidden issues. Mirrors paperclip's
    /// `loadWatchdogSubtreeIssues` RECURSIVE CTE.
    async fn load_subtree_issues(
        &self,
        company_id: Uuid,
        watched_issue_id: Uuid,
    ) -> RepositoryResult<Vec<TaskWatchdogClassifierIssue>>;

    /// All active watchdogs for a company.
    async fn list_active_by_company(
        &self,
        company_id: Uuid,
    ) -> RepositoryResult<Vec<IssueWatchdog>>;

    /// Record the observed fingerprint + bump trigger count.
    async fn record_observed(
        &self,
        id: Uuid,
        fingerprint: &str,
    ) -> RepositoryResult<()>;

    /// Record that the stopped subtree was reviewed at this fingerprint.
    async fn record_reviewed(
        &self,
        id: Uuid,
        fingerprint: &str,
    ) -> RepositoryResult<()>;

    /// Update the watchdog_issue_id reference after creating a review issue.
    async fn update_watchdog_issue_id(
        &self,
        id: Uuid,
        watchdog_issue_id: Uuid,
    ) -> RepositoryResult<()>;

    /// Update the status of a watchdog.
    async fn update_status(
        &self,
        id: Uuid,
        status: IssueWatchdogStatus,
    ) -> RepositoryResult<IssueWatchdog>;
}

pub struct PostgresIssueWatchdogRepository {
    pool: PgPool,
}

impl PostgresIssueWatchdogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueWatchdogRepository for PostgresIssueWatchdogRepository {
    async fn upsert(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        watchdog_agent_id: Uuid,
        instructions: Option<&str>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<&str>,
        created_by_run_id: Option<Uuid>,
    ) -> RepositoryResult<IssueWatchdog> {
        let row = sqlx::query_as::<_, IssueWatchdog>(
            r#"INSERT INTO issue_watchdogs
               (company_id, issue_id, watchdog_agent_id, instructions,
                created_by_agent_id, created_by_user_id, created_by_run_id, updated_by_agent_id, updated_by_user_id, updated_by_run_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $5, $6, $7)
               ON CONFLICT (company_id, issue_id) DO UPDATE
                 SET watchdog_agent_id = EXCLUDED.watchdog_agent_id,
                     instructions = EXCLUDED.instructions,
                     updated_by_agent_id = EXCLUDED.updated_by_agent_id,
                     updated_by_user_id = EXCLUDED.updated_by_user_id,
                     updated_by_run_id = EXCLUDED.updated_by_run_id,
                     updated_at = NOW()
               RETURNING id, company_id, issue_id, watchdog_agent_id, instructions, status,
                         watchdog_issue_id, last_observed_fingerprint, last_reviewed_fingerprint,
                         last_triggered_at, last_completed_at, trigger_count,
                         created_by_agent_id, created_by_user_id, created_by_run_id,
                         updated_by_agent_id, updated_by_user_id, updated_by_run_id,
                         created_at, updated_at"#,
        )
        .bind(company_id)
        .bind(issue_id)
        .bind(watchdog_agent_id)
        .bind(instructions)
        .bind(created_by_agent_id)
        .bind(created_by_user_id)
        .bind(created_by_run_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn get_by_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> RepositoryResult<Option<IssueWatchdog>> {
        let row = sqlx::query_as::<_, IssueWatchdog>(
            r#"SELECT id, company_id, issue_id, watchdog_agent_id, instructions, status,
                      watchdog_issue_id, last_observed_fingerprint, last_reviewed_fingerprint,
                      last_triggered_at, last_completed_at, trigger_count,
                      created_by_agent_id, created_by_user_id, created_by_run_id,
                      updated_by_agent_id, updated_by_user_id, updated_by_run_id,
                      created_at, updated_at
               FROM issue_watchdogs
              WHERE company_id = $1 AND issue_id = $2"#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn find_watchdog_issue(
        &self,
        company_id: Uuid,
        source_issue_id: Uuid,
    ) -> RepositoryResult<Option<models::Issue>> {
        let issue = sqlx::query_as::<_, models::Issue>(
            r#"SELECT id, company_id, project_id, project_workspace_id, goal_id, parent_id,
                      title, name, description, status, priority, work_mode, assignee_agent_id,
                      assignee_user_id, responsible_user_id, source_trust, created_by_agent_id,
                      created_by_user_id, origin_kind, origin_id, origin_run_id, origin_fingerprint,
                      execution_workspace_id, execution_workspace_preference, execution_policy,
                      execution_state, execution_locked_at, execution_run_id, monitor_scheduled_by,
                      monitor_notes, monitor_next_check_at, monitor_last_triggered_at,
                      monitor_attempt_count, hidden_at, created_at, updated_at, identifier
               FROM issues
              WHERE company_id = $1
                AND origin_kind = 'task_watchdog'
                AND origin_id = $2
              ORDER BY created_at ASC, id ASC
              LIMIT 1"#,
        )
        .bind(company_id)
        .bind(source_issue_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(issue)
    }

    async fn load_subtree_issues(
        &self,
        company_id: Uuid,
        watched_issue_id: Uuid,
    ) -> RepositoryResult<Vec<TaskWatchdogClassifierIssue>> {
        let rows = sqlx::query_as::<_, TaskWatchdogClassifierIssue>(
            r#"WITH RECURSIVE watched_issues AS (
                 SELECT id, company_id, identifier, title, status, parent_id,
                        assignee_agent_id, assignee_user_id, origin_kind,
                        created_at, updated_at, 0 AS depth
                   FROM issues
                  WHERE company_id = $1 AND id = $2
                    AND hidden_at IS NULL
                 UNION ALL
                 SELECT child.id, child.company_id, child.identifier, child.title, child.status,
                        child.parent_id, child.assignee_agent_id, child.assignee_user_id,
                        child.origin_kind, child.created_at, child.updated_at,
                        watched_issues.depth + 1
                   FROM issues child
                   JOIN watched_issues ON child.parent_id = watched_issues.id
                  WHERE child.company_id = $1
                    AND child.hidden_at IS NULL
                    AND child.origin_kind IS DISTINCT FROM 'task_watchdog'
                    AND watched_issues.depth < 32
               )
               SELECT id, company_id, identifier, title, status, parent_id,
                      assignee_agent_id, assignee_user_id, origin_kind,
                      created_at, updated_at
                 FROM watched_issues"#,
        )
        .bind(company_id)
        .bind(watched_issue_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn list_active_by_company(
        &self,
        company_id: Uuid,
    ) -> RepositoryResult<Vec<IssueWatchdog>> {
        let rows = sqlx::query_as::<_, IssueWatchdog>(
            r#"SELECT id, company_id, issue_id, watchdog_agent_id, instructions, status,
                      watchdog_issue_id, last_observed_fingerprint, last_reviewed_fingerprint,
                      last_triggered_at, last_completed_at, trigger_count,
                      created_by_agent_id, created_by_user_id, created_by_run_id,
                      updated_by_agent_id, updated_by_user_id, updated_by_run_id,
                      created_at, updated_at
               FROM issue_watchdogs
              WHERE company_id = $1 AND status = 'active'"#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn record_observed(&self, id: Uuid, fingerprint: &str) -> RepositoryResult<()> {
        sqlx::query(
            r#"UPDATE issue_watchdogs
                 SET last_observed_fingerprint = $2,
                     trigger_count = trigger_count + 1,
                     last_triggered_at = NOW(),
                     updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(fingerprint)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_reviewed(&self, id: Uuid, fingerprint: &str) -> RepositoryResult<()> {
        sqlx::query(
            r#"UPDATE issue_watchdogs
                 SET last_reviewed_fingerprint = $2,
                     last_completed_at = NOW(),
                     updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(fingerprint)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_watchdog_issue_id(&self, id: Uuid, watchdog_issue_id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"UPDATE issue_watchdogs
                 SET watchdog_issue_id = $2,
                     updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(watchdog_issue_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, id: Uuid, status: IssueWatchdogStatus) -> RepositoryResult<IssueWatchdog> {
        let row = sqlx::query_as::<_, IssueWatchdog>(
            r#"UPDATE issue_watchdogs
                 SET status = $2,
                     updated_at = NOW()
               WHERE id = $1
               RETURNING id, company_id, issue_id, watchdog_agent_id, instructions, status,
                         watchdog_issue_id, last_observed_fingerprint, last_reviewed_fingerprint,
                         last_triggered_at, last_completed_at, trigger_count,
                         created_by_agent_id, created_by_user_id, created_by_run_id,
                         updated_by_agent_id, updated_by_user_id, updated_by_run_id,
                         created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }
}

// ============================================================================
// agent_wakeup_requests
// ============================================================================

#[async_trait]
pub trait AgentWakeupRequestRepository: Send + Sync {
    /// Live wake requests whose payload references one of `issue_ids`.
    async fn list_live_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<AgentWakeupRequest>>;
}

pub struct PostgresAgentWakeupRequestRepository {
    pool: PgPool,
}

impl PostgresAgentWakeupRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentWakeupRequestRepository for PostgresAgentWakeupRequestRepository {
    async fn list_live_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<AgentWakeupRequest>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query_as::<_, AgentWakeupRequest>(
            r#"SELECT id, company_id, agent_id, status, payload, created_at, updated_at
               FROM agent_wakeup_requests
              WHERE company_id = $1
                AND status = ANY($2)
                AND (
                  payload->>'issueId' = ANY($3::text[])
                  OR payload->>'taskId' = ANY($3::text[])
                  OR payload->'_paperclipWakeContext'->>'issueId' = ANY($3::text[])
                  OR payload->'_paperclipWakeContext'->>'taskId' = ANY($3::text[])
                )"#,
        )
        .bind(company_id)
        .bind(&["queued", "dispatched", "running"][..])
        .bind(issue_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

// ============================================================================
// issue_thread_interactions
// ============================================================================

#[async_trait]
pub trait IssueThreadInteractionRepository: Send + Sync {
    /// Pending interactions for any of `issue_ids`.
    async fn list_pending_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<IssueThreadInteraction>>;
}

pub struct PostgresIssueThreadInteractionRepository {
    pool: PgPool,
}

impl PostgresIssueThreadInteractionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IssueThreadInteractionRepository for PostgresIssueThreadInteractionRepository {
    async fn list_pending_by_issue_ids(
        &self,
        company_id: Uuid,
        issue_ids: &[Uuid],
    ) -> RepositoryResult<Vec<IssueThreadInteraction>> {
        if issue_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query_as::<_, IssueThreadInteraction>(
            r#"SELECT id, company_id, issue_id, kind, status, source_run_id, created_at, updated_at
               FROM issue_thread_interactions
              WHERE company_id = $1
                AND status = 'pending'
                AND issue_id = ANY($2::uuid[])"#,
        )
        .bind(company_id)
        .bind(issue_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
