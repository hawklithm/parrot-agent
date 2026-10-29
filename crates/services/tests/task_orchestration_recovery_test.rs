use async_trait::async_trait;
use services::heartbeat_service::{
    HeartbeatContext, HeartbeatError, HeartbeatService, HeartbeatWakeupOptions,
};
use services::job_scheduler::{ScheduledJob, TaskOrchestrationRecoveryJob};
use services::task_orchestration_service::TaskOrchestrationService;
use sqlx::PgPool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
struct RecordingHeartbeat {
    pool: PgPool,
    calls: Arc<AtomicUsize>,
    failures_remaining: Arc<AtomicUsize>,
}

impl RecordingHeartbeat {
    fn new(pool: PgPool) -> Self {
        Self {
            pool,
            calls: Arc::new(AtomicUsize::new(0)),
            failures_remaining: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn fail_next(&self) {
        self.failures_remaining.store(1, Ordering::Relaxed);
    }
}

#[async_trait]
impl HeartbeatService for RecordingHeartbeat {
    async fn wakeup(
        &self,
        _agent_id: Uuid,
        _issue_id: Uuid,
        _company_id: Uuid,
    ) -> Result<(), HeartbeatError> {
        Ok(())
    }

    async fn wakeup_with_options(
        &self,
        agent_id: Uuid,
        issue_id: Uuid,
        company_id: Uuid,
        options: HeartbeatWakeupOptions,
    ) -> Result<(), HeartbeatError> {
        if self
            .failures_remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(HeartbeatError::WakeupFailed(
                "injected recovery wake failure".to_string(),
            ));
        }
        let idempotency_key = options.idempotency_key.clone();
        let payload = options
            .payload
            .unwrap_or_else(|| serde_json::json!({}));
        let context_snapshot = options
            .context_snapshot
            .unwrap_or_else(|| serde_json::json!({ "issueId": issue_id }));
        let source = options.source.as_deref().unwrap_or("on_demand");
        let wake_request_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO agent_wakeup_requests
                (company_id, agent_id, status, payload, source, trigger_detail, reason,
                 requested_by_actor_type, requested_by_actor_id, idempotency_key)
             VALUES ($1, $2, 'queued', $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (company_id, idempotency_key)
                 WHERE idempotency_key IS NOT NULL
             DO UPDATE SET status = 'queued', payload = EXCLUDED.payload,
                           source = EXCLUDED.source, trigger_detail = EXCLUDED.trigger_detail,
                           reason = EXCLUDED.reason,
                           requested_by_actor_type = EXCLUDED.requested_by_actor_type,
                           requested_by_actor_id = EXCLUDED.requested_by_actor_id,
                           run_id = NULL, updated_at = NOW()
                 WHERE agent_wakeup_requests.status::text IN
                       ('queued', 'failed', 'cancelled', 'skipped')
                   AND agent_wakeup_requests.run_id IS NULL
             RETURNING id",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(payload)
        .bind(source)
        .bind(options.trigger_detail.as_deref())
        .bind(options.reason.as_deref())
        .bind(options.requested_by_actor_type.as_deref())
        .bind(options.requested_by_actor_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| HeartbeatError::WakeupFailed(error.to_string()))?;
        let Some(wake_request_id) = wake_request_id else {
            return Ok(());
        };

        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO heartbeat_runs
                (id, company_id, agent_id, invocation_source, status, context_snapshot)
             VALUES ($1, $2, $3, $4, 'queued', $5)",
        )
        .bind(run_id)
        .bind(company_id)
        .bind(agent_id)
        .bind(source)
        .bind(context_snapshot)
        .execute(&self.pool)
        .await
        .map_err(|error| HeartbeatError::WakeupFailed(error.to_string()))?;
        sqlx::query(
            "UPDATE agent_wakeup_requests
                SET status = 'dispatched', run_id = $2, updated_at = NOW()
              WHERE id = $1",
        )
        .bind(wake_request_id)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|error| HeartbeatError::WakeupFailed(error.to_string()))?;
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn cancel_run(
        &self,
        _agent_id: Uuid,
        _issue_id: Uuid,
        _company_id: Uuid,
        _reason: &str,
    ) -> Result<(), HeartbeatError> {
        Ok(())
    }

    async fn cancel_scheduled_retry(
        &self,
        _agent_id: Uuid,
        _issue_id: Uuid,
        _company_id: Uuid,
        _reason: &str,
    ) -> Result<bool, HeartbeatError> {
        Ok(false)
    }

    async fn get_heartbeat_context(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<HeartbeatContext, HeartbeatError> {
        Ok(HeartbeatContext {
            issue_id,
            company_id,
            active_agents: vec![],
            last_wakeup_at: None,
            wakeup_count: 0,
        })
    }
}

struct Fixture {
    company_id: Uuid,
    ceo_id: Uuid,
}

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("R{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Orchestration recovery test")
        .bind(issue_prefix)
        .execute(pool)
        .await
        .expect("insert company");

    let ceo_id = seed_ceo(pool, company_id, "Recovery CEO", false).await;
    Fixture { company_id, ceo_id }
}

async fn seed_ceo(pool: &PgPool, company_id: Uuid, name: &str, paused: bool) -> Uuid {
    let ceo_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type, adapter_config, paused_at)
         VALUES ($1, $2, $3, 'ceo', $4, 'process', '{}'::jsonb, CASE WHEN $5 THEN NOW() ELSE NULL END)",
    )
    .bind(ceo_id)
    .bind(company_id)
    .bind(name)
    .bind("idle")
    .bind(paused)
    .execute(pool)
    .await
    .expect("insert CEO");
    ceo_id
}

async fn seed_issue(
    pool: &PgPool,
    fixture: &Fixture,
    title: &str,
    status: &str,
    required: bool,
    ceo_id: Uuid,
) -> Uuid {
    let issue_id = Uuid::new_v4();
    let identifier = format!("R{}", &issue_id.simple().to_string()[..10]);
    sqlx::query(
        "INSERT INTO issues
            (id, company_id, title, identifier, status, priority,
             assignee_agent_id, task_orchestration_required, created_at)
         VALUES ($1, $2, $3, $4, $5::issue_status, 'medium', $6, $7,
                 NOW() - INTERVAL '2 minutes')",
    )
    .bind(issue_id)
    .bind(fixture.company_id)
    .bind(title)
    .bind(identifier)
    .bind(status)
    .bind(ceo_id)
    .bind(required)
    .execute(pool)
    .await
    .expect("insert root issue");
    issue_id
}

fn recovery_job(pool: &PgPool, heartbeat: &RecordingHeartbeat) -> TaskOrchestrationRecoveryJob {
    TaskOrchestrationRecoveryJob::new(TaskOrchestrationService::read_only(
        pool.clone(),
        Arc::new(heartbeat.clone()),
    ))
}

async fn count_for_issue(pool: &PgPool, query: &str, issue_id: Uuid) -> i64 {
    sqlx::query_scalar(query)
        .bind(issue_id)
        .fetch_one(pool)
        .await
        .expect("count recovery rows")
}

#[sqlx::test]
async fn recovery_starts_a_marked_root_after_issue_commit_and_dispatches_once(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Crash before orchestration start",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let heartbeat = RecordingHeartbeat::new(pool.clone());
    let job = recovery_job(&pool, &heartbeat);

    let summary = job.execute().await.expect("recover the issue");
    assert!(summary.contains("recovered 1 required task orchestrations"));
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestrations WHERE root_issue_id = $1",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_actions
              WHERE idempotency_key = 'task-orchestration-initial:' || $1::text",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM agent_wakeup_requests
              WHERE company_id = (SELECT company_id FROM issues WHERE id = $1)
                AND idempotency_key = 'task-orchestration-initial:' || $1::text",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(heartbeat.calls(), 1);

    job.execute().await.expect("repeat recovery sweep");
    assert_eq!(heartbeat.calls(), 1, "a repeated sweep is a no-op");
}

#[sqlx::test]
async fn recovery_enqueues_the_missing_initial_wake_after_start_committed(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Crash after orchestration start",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    sqlx::query("UPDATE agents SET status = 'running' WHERE id = $1")
        .bind(fixture.ceo_id)
        .execute(&pool)
        .await
        .expect("mark CEO running; running CEOs are still invokable");
    let heartbeat = RecordingHeartbeat::new(pool.clone());
    let service = TaskOrchestrationService::read_only(
        pool.clone(),
        Arc::new(heartbeat.clone()),
    );

    service
        .start(fixture.company_id, issue_id, fixture.ceo_id)
        .await
        .expect("simulate committed orchestration before process exit");
    assert_eq!(heartbeat.calls(), 0);

    recovery_job(&pool, &heartbeat)
        .execute()
        .await
        .expect("recover the missing initial wake");
    assert_eq!(heartbeat.calls(), 1);
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestrations WHERE root_issue_id = $1",
            issue_id,
        )
        .await,
        1,
        "recovery reuses the existing orchestration"
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_events
              WHERE orchestration_id = (SELECT id FROM task_orchestrations WHERE root_issue_id = $1)
                AND event_type = 'orchestration_started'",
            issue_id,
        )
        .await,
        1
    );
}

#[sqlx::test]
async fn recovery_skips_unmarked_terminal_and_already_run_tasks(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let foreign_fixture = seed(&pool).await;
    let paused_ceo = seed_ceo(&pool, fixture.company_id, "Paused recovery CEO", true).await;
    let unmarked = seed_issue(
        &pool,
        &fixture,
        "Not marked for orchestration",
        "todo",
        false,
        fixture.ceo_id,
    )
    .await;
    let completed = seed_issue(
        &pool,
        &fixture,
        "Completed root",
        "done",
        true,
        fixture.ceo_id,
    )
    .await;
    let cancelled = seed_issue(
        &pool,
        &fixture,
        "Cancelled root",
        "cancelled",
        true,
        fixture.ceo_id,
    )
    .await;
    let already_run = seed_issue(
        &pool,
        &fixture,
        "Root with a terminal run",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let unavailable = seed_issue(
        &pool,
        &fixture,
        "Root with a paused CEO",
        "todo",
        true,
        paused_ceo,
    )
    .await;
    let cross_company_ceo = seed_issue(
        &pool,
        &fixture,
        "Root assigned to a foreign CEO",
        "todo",
        true,
        foreign_fixture.ceo_id,
    )
    .await;
    let terminal_orchestration = seed_issue(
        &pool,
        &fixture,
        "Root with a terminal orchestration",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    sqlx::query(
        "INSERT INTO heartbeat_runs (company_id, agent_id, status, context_snapshot, finished_at)
         VALUES ($1, $2, 'succeeded', $3, NOW())",
    )
    .bind(fixture.company_id)
    .bind(fixture.ceo_id)
    .bind(serde_json::json!({ "issueId": already_run }))
    .execute(&pool)
    .await
    .expect("insert existing terminal run");

    let heartbeat = RecordingHeartbeat::new(pool.clone());
    TaskOrchestrationService::read_only(pool.clone(), Arc::new(heartbeat.clone()))
        .start(fixture.company_id, terminal_orchestration, fixture.ceo_id)
        .await
        .expect("seed a completed orchestration");
    sqlx::query(
        "UPDATE task_orchestrations
            SET stage = 'completed', completed_at = NOW()
          WHERE root_issue_id = $1 AND company_id = $2",
    )
    .bind(terminal_orchestration)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("mark orchestration terminal");

    recovery_job(&pool, &heartbeat)
        .execute()
        .await
        .expect("skip unsafe recovery candidates");

    for issue_id in [
        unmarked,
        completed,
        cancelled,
        already_run,
        unavailable,
        cross_company_ceo,
    ] {
        assert_eq!(
            count_for_issue(
                &pool,
                "SELECT COUNT(*)::bigint FROM task_orchestrations WHERE root_issue_id = $1",
                issue_id,
            )
            .await,
            0,
            "issue {issue_id} must not be converted into a running orchestration"
        );
    }
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestrations WHERE root_issue_id = $1",
            terminal_orchestration,
        )
        .await,
        1,
        "the terminal orchestration seeded by this fixture must remain the only row"
    );
    let terminal_stage: String = sqlx::query_scalar(
        "SELECT stage::text FROM task_orchestrations WHERE root_issue_id = $1",
    )
    .bind(terminal_orchestration)
    .fetch_one(&pool)
    .await
    .expect("read the seeded terminal orchestration");
    assert_eq!(terminal_stage, "completed");
    assert_eq!(heartbeat.calls(), 0);
}

#[sqlx::test]
async fn concurrent_and_repeated_recovery_jobs_keep_one_start_action_and_wake(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Concurrent recovery",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let heartbeat = RecordingHeartbeat::new(pool.clone());
    let first = recovery_job(&pool, &heartbeat);
    let second = recovery_job(&pool, &heartbeat);

    let (first_result, second_result) = tokio::join!(first.execute(), second.execute());
    first_result.expect("first recovery worker");
    second_result.expect("second recovery worker");

    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestrations WHERE root_issue_id = $1",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_actions
              WHERE idempotency_key = 'task-orchestration-initial:' || $1::text",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM agent_wakeup_requests
              WHERE company_id = (SELECT company_id FROM issues WHERE id = $1)
                AND idempotency_key = 'task-orchestration-initial:' || $1::text",
            issue_id,
        )
        .await,
        1
    );
    assert_eq!(heartbeat.calls(), 1);

    recovery_job(&pool, &heartbeat)
        .execute()
        .await
        .expect("third idempotent recovery sweep");
    assert_eq!(heartbeat.calls(), 1);
}

#[sqlx::test]
async fn failed_initial_wake_remains_in_the_outbox_for_the_next_sweep(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Retry a failed recovery wake",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let heartbeat = RecordingHeartbeat::new(pool.clone());
    heartbeat.fail_next();
    let job = recovery_job(&pool, &heartbeat);

    job.execute()
        .await
        .expect("the recovery job records the transient wake failure");
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_actions
              WHERE idempotency_key = 'task-orchestration-initial:' || $1::text
                AND status = 'pending'",
            issue_id,
        )
        .await,
        1,
        "a failed wake stays retryable in the existing outbox"
    );
    assert_eq!(heartbeat.calls(), 0);

    job.execute()
        .await
        .expect("the next sweep retries the pending outbox action");
    assert_eq!(heartbeat.calls(), 1);
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM agent_wakeup_requests
              WHERE company_id = (SELECT company_id FROM issues WHERE id = $1)
                AND idempotency_key = 'task-orchestration-initial:' || $1::text",
            issue_id,
        )
        .await,
        1
    );
}

#[sqlx::test]
async fn stale_queued_initial_wake_without_a_run_is_reclaimed_before_outbox_completion(
    pool: PgPool,
) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Crash after claiming the initial wake",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let idempotency_key = format!("task-orchestration-initial:{issue_id}");
    sqlx::query(
        "INSERT INTO agent_wakeup_requests
            (company_id, agent_id, status, payload, source, reason,
             idempotency_key, updated_at)
         VALUES ($1, $2, 'queued', $3, 'assignment', 'issue_assigned', $4,
                 NOW() - INTERVAL '2 minutes')",
    )
    .bind(fixture.company_id)
    .bind(fixture.ceo_id)
    .bind(serde_json::json!({ "issueId": issue_id }))
    .bind(idempotency_key)
    .execute(&pool)
    .await
    .expect("insert a wake claim whose worker crashed before creating a run");

    let heartbeat = RecordingHeartbeat::new(pool.clone());
    recovery_job(&pool, &heartbeat)
        .execute()
        .await
        .expect("reclaim the stale initial wake");

    assert_eq!(heartbeat.calls(), 1);
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM agent_wakeup_requests
              WHERE company_id = (SELECT company_id FROM issues WHERE id = $1)
                AND idempotency_key = 'task-orchestration-initial:' || $1::text
                AND status = 'dispatched' AND run_id IS NOT NULL",
            issue_id,
        )
        .await,
        1,
        "the existing idempotency row is reused instead of duplicated"
    );
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_actions
              WHERE idempotency_key = 'task-orchestration-initial:' || $1::text
                AND status = 'applied'",
            issue_id,
        )
        .await,
        1,
        "the outbox completes only after the stale request has been reclaimed"
    );
}

#[sqlx::test]
async fn cancellation_skips_a_claimed_initial_recovery_wake(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let issue_id = seed_issue(
        &pool,
        &fixture,
        "Cancel before the claimed initial wake dispatches",
        "todo",
        true,
        fixture.ceo_id,
    )
    .await;
    let heartbeat = RecordingHeartbeat::new(pool.clone());
    let service = TaskOrchestrationService::read_only(
        pool.clone(),
        Arc::new(heartbeat.clone()),
    );

    assert_eq!(
        service
            .recover_required_task_orchestrations(128)
            .await
            .expect("queue the recovered initial wake"),
        1
    );
    let orchestration_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_orchestrations WHERE company_id = $1 AND root_issue_id = $2",
    )
    .bind(fixture.company_id)
    .bind(issue_id)
    .fetch_one(&pool)
    .await
    .expect("read recovered orchestration");
    sqlx::query(
        "UPDATE task_orchestration_actions
            SET status = 'running', attempts = 1, updated_at = NOW()
          WHERE orchestration_id = $1
            AND idempotency_key = 'task-orchestration-initial:' || $2::text",
    )
    .bind(orchestration_id)
    .bind(issue_id)
    .execute(&pool)
    .await
    .expect("simulate the drainer claiming the initial wake");

    service
        .cancel(fixture.company_id, issue_id)
        .await
        .expect("cancel the root while the initial wake is claimed");
    service
        .drain_actions(orchestration_id)
        .await
        .expect("a cancelled claimed action is not dispatched by a later drain");

    assert_eq!(heartbeat.calls(), 0);
    assert_eq!(
        count_for_issue(
            &pool,
            "SELECT COUNT(*)::bigint FROM task_orchestration_actions
              WHERE orchestration_id = (SELECT id FROM task_orchestrations WHERE root_issue_id = $1)
                AND idempotency_key = 'task-orchestration-initial:' || $1::text
                AND status = 'skipped'",
            issue_id,
        )
        .await,
        1,
        "cancel marks even a claimed initial wake as skipped"
    );
}
