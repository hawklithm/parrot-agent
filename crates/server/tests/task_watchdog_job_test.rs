//! Task watchdog scheduler integration test
//! (PAPERCLIP_MIGRATION_PLAN §4B.1 line 376).
//!
//! `DefaultWatchdogService` implemented the subtree classifier and the
//! review-issue lifecycle, but nothing ever scheduled it — a stopped subtree
//! was only noticed if some unrelated code path happened to call
//! `evaluate_for_issue`. `TaskWatchdogJob` closes that gap.
//!
//! Covered here:
//!   - the job evaluates active watchdogs and creates a review issue for a
//!     stopped subtree, linked back to the watchdog record
//!   - a terminal review issue is reopened when the subtree stops again
//!   - a live subtree is not treated as stopped
//!   - a failing company does not abort the remaining companies

use async_trait::async_trait;
use services::{
    job_scheduler::{JobSchedule, ScheduledJob},
    DefaultWatchdogService, TaskWatchdogJob, WatchdogService,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    watchdog_agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let watchdog_agent_id = Uuid::new_v4();
    let prefix = format!("WD{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Watchdog Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, 'idle')",
    )
    .bind(watchdog_agent_id)
    .bind(company_id)
    .bind("Watchdog agent")
    .execute(pool)
    .await
    .expect("insert watchdog agent");

    Fixture {
        pool: pool.clone(),
        company_id,
        watchdog_agent_id,
    }
}

async fn cleanup(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issue_watchdogs WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.watchdog_agent_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}

/// Insert an issue, backdating `created_at` past the watchdog first-run grace
/// window so a stopped subtree is classified as stopped rather than
/// `PendingFirstRun`.
async fn insert_issue(f: &Fixture, title: &str, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, assignee_agent_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4::issue_status, $5, NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '10 minutes')",
    )
    .bind(id)
    .bind(f.company_id)
    .bind(title)
    .bind(status)
    .bind(f.watchdog_agent_id)
    .execute(&f.pool)
    .await
    .expect("insert issue");
    id
}

fn build_watchdog_service(f: &Fixture) -> DefaultWatchdogService {
    DefaultWatchdogService::new(
        std::sync::Arc::new(repositories::PgIssueRepository::new(f.pool.clone())),
        std::sync::Arc::new(
            repositories::task_watchdog_repository::PostgresIssueWatchdogRepository::new(
                f.pool.clone(),
            ),
        ),
        std::sync::Arc::new(
            repositories::task_watchdog_repository::PostgresHeartbeatRunRepository::new(
                f.pool.clone(),
            ),
        ),
        std::sync::Arc::new(
            repositories::task_watchdog_repository::PostgresAgentWakeupRequestRepository::new(
                f.pool.clone(),
            ),
        ),
        std::sync::Arc::new(
            repositories::task_watchdog_repository::PostgresIssueThreadInteractionRepository::new(
                f.pool.clone(),
            ),
        ),
    )
}

/// Seed a watched issue plus a child, and register an active watchdog.
async fn seed_watchdog(f: &Fixture, watched_status: &str, child_status: &str) -> (Uuid, Uuid) {
    let watched = insert_issue(f, "watched", watched_status).await;
    let child = insert_issue(f, "child", child_status).await;
    sqlx::query("UPDATE issues SET parent_id = $1 WHERE id = $2")
        .bind(watched)
        .bind(child)
        .execute(&f.pool)
        .await
        .expect("link child");

    sqlx::query(
        "INSERT INTO issue_watchdogs (company_id, issue_id, watchdog_agent_id, status)
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(f.company_id)
    .bind(watched)
    .bind(f.watchdog_agent_id)
    .execute(&f.pool)
    .await
    .expect("insert watchdog");

    (watched, child)
}

async fn count_review_issues(f: &Fixture) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues
          WHERE company_id = $1 AND origin_kind = 'task_watchdog'",
    )
    .bind(f.company_id)
    .fetch_one(&f.pool)
    .await
    .expect("count review issues")
}

/// The job evaluates active watchdogs and creates a review issue for a stopped
/// subtree, linking it back to the watchdog record.
#[sqlx::test]
async fn job_evaluates_watchdogs_and_creates_review_issue(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;

    // A todo issue with no live run, no wake and no completed run is stopped.
    let (watched, _child) = seed_watchdog(&f, "todo", "todo").await;

    assert_eq!(count_review_issues(&f).await, 0);

    let service = build_watchdog_service(&f);
    let job = TaskWatchdogJob::new(pool.clone(), std::sync::Arc::new(service));
    assert_eq!(job.job_name(), "task_watchdog");
    assert!(matches!(job.schedule(), JobSchedule::IntervalSeconds(60)));

    let summary = job.execute().await.expect("watchdog job execution");
    assert!(
        summary.contains("evaluated 1 task watchdogs"),
        "unexpected summary: {summary}"
    );

    assert_eq!(
        count_review_issues(&f).await,
        1,
        "a stopped subtree must get exactly one review issue"
    );

    // The review issue is linked back to the watchdog record.
    let linked: Option<Uuid> = sqlx::query_scalar("SELECT watchdog_issue_id FROM issue_watchdogs WHERE issue_id = $1")
        .bind(watched)
        .fetch_one(&f.pool)
        .await
        .expect("read watchdog link");
    let linked = linked.expect("watchdog must reference its review issue");

    let origin: Option<String> = sqlx::query_scalar("SELECT origin_kind FROM issues WHERE id = $1")
        .bind(linked)
        .fetch_one(&f.pool)
        .await
        .expect("read review issue origin");
    assert_eq!(origin.as_deref(), Some("task_watchdog"));

    // Re-running does not create a second review issue.
    let summary = job.execute().await.expect("watchdog job execution");
    assert_eq!(
        count_review_issues(&f).await,
        1,
        "repeat evaluation must not duplicate the review issue: {summary}"
    );

    cleanup(&f).await;
}

/// A terminal review issue is reopened when the watched subtree stops again.
#[sqlx::test]
async fn job_reopens_terminal_review_issue_on_new_stop(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let (watched, child) = seed_watchdog(&f, "todo", "todo").await;

    let service = build_watchdog_service(&f);
    let job = TaskWatchdogJob::new(pool.clone(), std::sync::Arc::new(service));
    job.execute().await.expect("first evaluation");

    let review_id: Uuid = sqlx::query_scalar("SELECT watchdog_issue_id FROM issue_watchdogs WHERE issue_id = $1")
        .bind(watched)
        .fetch_one(&f.pool)
        .await
        .expect("review issue id");

    // An agent resolves the review...
    sqlx::query("UPDATE issues SET status = 'done'::issue_status WHERE id = $1")
        .bind(review_id)
        .execute(&f.pool)
        .await
        .expect("resolve review issue");

    // ...and the subtree stops again with a *different* fingerprint. Only the
    // leaf issue contributes to the stop fingerprint, so the child must change.
    sqlx::query("UPDATE issues SET status = 'blocked'::issue_status WHERE id = $1")
        .bind(child)
        .execute(&f.pool)
        .await
        .expect("change subtree state");

    job.execute().await.expect("second evaluation");

    let status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(review_id)
        .fetch_one(&f.pool)
        .await
        .expect("read review status");
    assert_eq!(
        status, "todo",
        "a terminal review issue must be reopened on a new stop"
    );

    cleanup(&f).await;
}

#[sqlx::test]
async fn job_ignores_live_subtree(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let (watched, _child) = seed_watchdog(&f, "in_progress", "in_progress").await;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(f.company_id)
    .bind(f.watchdog_agent_id)
    .bind(serde_json::json!({"issueId": watched.to_string()}))
    .execute(&f.pool)
    .await
    .expect("insert live run");

    let service = build_watchdog_service(&f);
    let job = TaskWatchdogJob::new(pool.clone(), std::sync::Arc::new(service));
    job.execute().await.expect("watchdog job execution");

    assert_eq!(
        count_review_issues(&f).await,
        0,
        "a live subtree must not be treated as stopped"
    );

    cleanup(&f).await;
}

/// A failing company does not stop the remaining companies from being
/// evaluated, and a total failure is still reported as a job error.
///
/// The failure is injected with a stub watchdog service: forcing a real
/// PostgreSQL error here would require creating a foreign-key violation that
/// PostgreSQL refuses to let us insert in the first place.
#[sqlx::test]
async fn job_isolates_per_company_failures(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;

    // Two companies, each with an active watchdog row. The job discovers
    // companies from the table, so the stub can be asserted on real rows.
    let (_watched, _child) = seed_watchdog(&f, "todo", "todo").await;
    let second_company = Uuid::new_v4();
    let second_prefix = format!("SC{}", &second_company.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(second_company)
        .bind("Second Watchdog Co")
        .bind(second_prefix)
        .execute(&f.pool)
        .await
        .expect("insert second company");
    let second_issue = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, created_at, updated_at)
         VALUES ($1, $2, 'second watched', 'todo'::issue_status,
                 NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '10 minutes')",
    )
    .bind(second_issue)
    .bind(second_company)
    .execute(&f.pool)
    .await
    .expect("insert second watched issue");
    sqlx::query(
        "INSERT INTO issue_watchdogs (company_id, issue_id, watchdog_agent_id, status)
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(second_company)
    .bind(second_issue)
    .bind(f.watchdog_agent_id)
    .execute(&f.pool)
    .await
    .expect("insert second watchdog");

    // Both companies are discovered and evaluated.
    let job = TaskWatchdogJob::new(pool.clone(), std::sync::Arc::new(StubWatchdogService::ok()));
    let summary = job.execute().await.expect("job must succeed");
    assert!(
        summary.contains("evaluated 2 task watchdogs"),
        "unexpected summary: {summary}"
    );

    // One company fails; the other still gets evaluated.
    let job = TaskWatchdogJob::new(
        pool.clone(),
        std::sync::Arc::new(StubWatchdogService::failing_for(second_company)),
    );
    let summary = job.execute().await.expect("partial failure must not fail the job");
    assert!(
        summary.contains("evaluated 1 task watchdogs") && summary.contains("1 companies failed"),
        "unexpected summary: {summary}"
    );

    // Every company failing is reported as a job error.
    let job = TaskWatchdogJob::new(
        pool.clone(),
        std::sync::Arc::new(StubWatchdogService::failing_for_all()),
    );
    let error = job
        .execute()
        .await
        .expect_err("total failure must be reported");
    assert!(error.contains("failed for 2 companies"), "unexpected error: {error}");

    sqlx::query("DELETE FROM issue_watchdogs WHERE company_id = $1")
        .bind(second_company)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(second_company)
        .execute(&f.pool)
        .await
        .ok();

    cleanup(&f).await;
}

/// Stub watchdog service used to assert the job's per-company error isolation.
struct StubWatchdogService {
    failing: Option<Uuid>,
    fail_all: bool,
}

impl StubWatchdogService {
    fn ok() -> Self {
        Self { failing: None, fail_all: false }
    }
    fn failing_for(company_id: Uuid) -> Self {
        Self { failing: Some(company_id), fail_all: false }
    }
    fn failing_for_all() -> Self {
        Self { failing: None, fail_all: true }
    }
}

#[async_trait]
impl WatchdogService for StubWatchdogService {
    async fn evaluate_all(&self, company_id: Uuid) -> repositories::RepositoryResult<usize> {
        if self.fail_all || self.failing == Some(company_id) {
            return Err(repositories::RepositoryError::NotFound(Uuid::nil()));
        }
        Ok(1)
    }

    async fn evaluate(
        &self,
        _watchdog: models::task_watchdog::IssueWatchdog,
    ) -> repositories::RepositoryResult<()> {
        Ok(())
    }

    async fn evaluate_for_issue(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
    ) -> repositories::RepositoryResult<usize> {
        Ok(0)
    }

    async fn upsert_watchdog(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
        _watchdog_agent_id: Uuid,
        _instructions: Option<String>,
        _created_by_agent_id: Option<Uuid>,
        _created_by_user_id: Option<String>,
        _created_by_run_id: Option<Uuid>,
    ) -> repositories::RepositoryResult<models::task_watchdog::IssueWatchdog> {
        Err(repositories::RepositoryError::NotFound(Uuid::nil()))
    }

    async fn get_watchdog(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
    ) -> repositories::RepositoryResult<Option<models::task_watchdog::IssueWatchdog>> {
        Ok(None)
    }

    async fn update_watchdog_status(
        &self,
        _id: Uuid,
        _status: models::task_watchdog::IssueWatchdogStatus,
    ) -> repositories::RepositoryResult<models::task_watchdog::IssueWatchdog> {
        Err(repositories::RepositoryError::NotFound(Uuid::nil()))
    }
}
