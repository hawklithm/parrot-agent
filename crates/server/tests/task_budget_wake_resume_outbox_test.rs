//! PostgreSQL coverage for task-budget wake intents.
//!
//! These tests exercise the durable boundary between a budget-blocked wake,
//! its independent resume intent, and the scheduled recovery path. They use a
//! fake local HTTP adapter and never contact a model provider.

use axum::{http::StatusCode, routing::post, Router};
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::{
    HeartbeatService, HeartbeatWakeupOptions, ScheduledJob, TaskOrchestrationRecoveryJob,
    TaskOrchestrationService,
};
use sqlx::PgPool;
use uuid::Uuid;

mod common;
use common::migrate;

#[derive(Clone)]
struct Fixture {
    company_id: Uuid,
    issue_id: Uuid,
    agent_id: Uuid,
    user_id: Uuid,
}

async fn fake_adapter() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake adapter");
    let address = listener.local_addr().expect("fake adapter address");
    let app = Router::new().route(
        "/run",
        post(|| async { (StatusCode::OK, r#"{"result":"resume accepted"}"#) }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let prefix = format!("BW{}", &company_id.simple().to_string()[..8]);
    let adapter = fake_adapter().await;

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents)
         VALUES ($1, 'Task budget wake outbox test', $2, FALSE)",
    )
    .bind(company_id)
    .bind(&prefix)
    .execute(pool)
    .await
    .expect("insert company");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type, adapter_config)
         VALUES ($1, $2, 'Budget worker', 'general', 'idle', 'http', $3)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(json!({
        "url": format!("http://{adapter}/run"),
        "method": "POST",
    }))
    .execute(pool)
    .await
    .expect("insert agent");

    sqlx::query(
        "INSERT INTO issues
            (id, company_id, title, identifier, status, assignee_agent_id,
             task_budget_cents, task_budget_revision)
         VALUES ($1, $2, 'Budgeted root issue', $3, 'todo', $4, 0, 1)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind(format!("{prefix}-1"))
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert budgeted root issue");

    Fixture {
        company_id,
        issue_id,
        agent_id,
        user_id,
    }
}

async fn seed_additional_root(pool: &PgPool, fixture: &Fixture) -> Uuid {
    let issue_id = Uuid::new_v4();
    let issue_prefix: String =
        sqlx::query_scalar("SELECT issue_prefix FROM companies WHERE id = $1")
            .bind(fixture.company_id)
            .fetch_one(pool)
            .await
            .expect("read company issue prefix");
    sqlx::query(
        "INSERT INTO issues
            (id, company_id, title, identifier, status, assignee_agent_id,
             task_budget_cents, task_budget_revision)
         VALUES ($1, $2, 'Second budgeted root issue', $3, 'todo', $4, 0, 1)",
    )
    .bind(issue_id)
    .bind(fixture.company_id)
    .bind(format!("{issue_prefix}-2"))
    .bind(fixture.agent_id)
    .execute(pool)
    .await
    .expect("insert second root issue in the same company");
    issue_id
}

#[derive(Clone)]
struct OrchestrationWakeFixture {
    company_id: Uuid,
    root_issue_id: Uuid,
    subtask_issue_id: Uuid,
    ceo_agent_id: Uuid,
    owner_agent_id: Uuid,
    reviewer_agent_id: Uuid,
    orchestration_id: Uuid,
    action_id: Uuid,
    issue_id: Uuid,
    agent_id: Uuid,
    resume_kind: &'static str,
    source_key: String,
}

async fn seed_orchestration_wake(pool: &PgPool, accepted: bool) -> OrchestrationWakeFixture {
    let base = seed(pool).await;
    let ceo_agent_id = Uuid::new_v4();
    let reviewer_agent_id = Uuid::new_v4();
    let orchestration_id = Uuid::new_v4();
    let plan_id = Uuid::new_v4();
    let subtask_row_id = Uuid::new_v4();
    let subtask_issue_id = Uuid::new_v4();
    let action_id = Uuid::new_v4();
    let deliverable_id = Uuid::new_v4();
    let adapter = fake_adapter().await;
    let prefix: String = sqlx::query_scalar("SELECT issue_prefix FROM companies WHERE id = $1")
        .bind(base.company_id)
        .fetch_one(pool)
        .await
        .expect("read orchestration test prefix");

    for (agent_id, name, role) in [
        (ceo_agent_id, "Budget task CEO", "ceo"),
        (reviewer_agent_id, "Budget task reviewer", "general"),
    ] {
        sqlx::query(
            "INSERT INTO agents (id, company_id, name, role, status, adapter_type, adapter_config)
             VALUES ($1, $2, $3, $4, 'idle', 'http', $5)",
        )
        .bind(agent_id)
        .bind(base.company_id)
        .bind(name)
        .bind(role)
        .bind(json!({ "url": format!("http://{adapter}/run"), "method": "POST" }))
        .execute(pool)
        .await
        .expect("insert orchestration agent");
    }

    sqlx::query(
        "UPDATE issues
            SET assignee_agent_id = $3, task_orchestration_required = TRUE,
                status = 'in_progress'
          WHERE company_id = $1 AND id = $2",
    )
    .bind(base.company_id)
    .bind(base.issue_id)
    .bind(ceo_agent_id)
    .execute(pool)
    .await
    .expect("mark budget issue as orchestrated root");

    sqlx::query(
        "INSERT INTO issues
            (id, company_id, title, identifier, status, assignee_agent_id, parent_id)
         VALUES ($1, $2, 'Budget task subtask', $3, $4::issue_status, $5, $6)",
    )
    .bind(subtask_issue_id)
    .bind(base.company_id)
    .bind(format!("{prefix}-subtask"))
    .bind(if accepted { "done" } else { "todo" })
    .bind(base.agent_id)
    .bind(base.issue_id)
    .execute(pool)
    .await
    .expect("insert budget subtask issue");

    sqlx::query(
        "INSERT INTO task_orchestrations
            (id, company_id, root_issue_id, ceo_agent_id, stage)
         VALUES ($1, $2, $3, $4, 'executing')",
    )
    .bind(orchestration_id)
    .bind(base.company_id)
    .bind(base.issue_id)
    .bind(ceo_agent_id)
    .execute(pool)
    .await
    .expect("insert task orchestration");
    sqlx::query(
        "INSERT INTO task_team_plans
            (id, orchestration_id, company_id, revision, status, created_by_agent_id)
         VALUES ($1, $2, $3, 1, 'approved', $4)",
    )
    .bind(plan_id)
    .bind(orchestration_id)
    .bind(base.company_id)
    .bind(ceo_agent_id)
    .execute(pool)
    .await
    .expect("insert approved task plan");
    for agent_id in [base.agent_id, reviewer_agent_id] {
        sqlx::query(
            "INSERT INTO task_team_members
                (plan_id, company_id, agent_id, action, reports_to_agent_id)
             VALUES ($1, $2, $3, 'reuse', $4)",
        )
        .bind(plan_id)
        .bind(base.company_id)
        .bind(agent_id)
        .bind(ceo_agent_id)
        .execute(pool)
        .await
        .expect("insert approved team member");
    }
    sqlx::query(
        "INSERT INTO task_orchestration_subtasks
            (id, orchestration_id, company_id, issue_id, owner_agent_id,
             reviewer_agent_id, status, required)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)",
    )
    .bind(subtask_row_id)
    .bind(orchestration_id)
    .bind(base.company_id)
    .bind(subtask_issue_id)
    .bind(base.agent_id)
    .bind(reviewer_agent_id)
    .bind(if accepted { "accepted" } else { "pending" })
    .execute(pool)
    .await
    .expect("insert orchestration subtask");
    sqlx::query(
        "INSERT INTO task_orchestration_events
            (orchestration_id, company_id, event_type, actor_agent_id, payload)
         VALUES ($1, $2, 'required_subtask_planned', $3, $4)",
    )
    .bind(orchestration_id)
    .bind(base.company_id)
    .bind(ceo_agent_id)
    .bind(json!({ "issueId": subtask_issue_id }))
    .execute(pool)
    .await
    .expect("insert planned subtask audit event");

    let (issue_id, agent_id, resume_kind, action_key, action_payload) = if accepted {
        sqlx::query(
            "INSERT INTO task_deliverables
                (id, orchestration_id, company_id, kind, subtask_id, version,
                 submitted_by_agent_id, body, evidence)
             VALUES ($1, $2, $3, 'subtask', $4, 1, $5, 'submitted work', '{}'::jsonb)",
        )
        .bind(deliverable_id)
        .bind(orchestration_id)
        .bind(base.company_id)
        .bind(subtask_row_id)
        .bind(base.agent_id)
        .execute(pool)
        .await
        .expect("insert accepted subtask deliverable");
        sqlx::query(
            "INSERT INTO task_acceptance_decisions
                (orchestration_id, company_id, deliverable_id, reviewer_agent_id, verdict)
             VALUES ($1, $2, $3, $4, 'accepted')",
        )
        .bind(orchestration_id)
        .bind(base.company_id)
        .bind(deliverable_id)
        .bind(reviewer_agent_id)
        .execute(pool)
        .await
        .expect("insert accepted reviewer decision");
        sqlx::query(
            "INSERT INTO task_orchestration_events
                (orchestration_id, company_id, event_type, actor_agent_id, payload)
             VALUES ($1, $2, 'acceptance_passed', $3, $4)",
        )
        .bind(orchestration_id)
        .bind(base.company_id)
        .bind(reviewer_agent_id)
        .bind(json!({ "issueId": subtask_issue_id }))
        .execute(pool)
        .await
        .expect("insert accepted reviewer audit event");

        (
            base.issue_id,
            reviewer_agent_id,
            "subtask_accepted",
            format!("subtask-accepted:{subtask_issue_id}:{deliverable_id}"),
            json!({
                "issueId": base.issue_id,
                "agentId": reviewer_agent_id,
                "reason": "subtask_accepted",
            }),
        )
    } else {
        (
            subtask_issue_id,
            base.agent_id,
            "subtask_assigned",
            format!("subtask-assigned:{subtask_issue_id}"),
            json!({
                "issueId": subtask_issue_id,
                "agentId": base.agent_id,
                "reason": "subtask_assigned",
            }),
        )
    };
    sqlx::query(
        "INSERT INTO task_orchestration_actions
            (id, orchestration_id, company_id, action_type, status, payload, idempotency_key)
         VALUES ($1, $2, $3, 'wake_assignee', 'pending', $4, $5)",
    )
    .bind(action_id)
    .bind(orchestration_id)
    .bind(base.company_id)
    .bind(action_payload)
    .bind(&action_key)
    .execute(pool)
    .await
    .expect("insert task wake action");

    OrchestrationWakeFixture {
        company_id: base.company_id,
        root_issue_id: base.issue_id,
        subtask_issue_id,
        ceo_agent_id,
        owner_agent_id: base.agent_id,
        reviewer_agent_id,
        orchestration_id,
        action_id,
        issue_id,
        agent_id,
        resume_kind,
        source_key: format!("task-orchestration-action:{action_id}"),
    }
}

async fn service(
    pool: &PgPool,
) -> (
    TaskOrchestrationService,
    std::sync::Arc<dyn HeartbeatService>,
) {
    let state = build_app_state(pool.clone())
        .await
        .expect("build app state");
    let heartbeat = state.heartbeat_service.clone();
    (
        TaskOrchestrationService::read_only(pool.clone(), heartbeat.clone()),
        heartbeat,
    )
}

fn safe_wake(fixture: &Fixture) -> HeartbeatWakeupOptions {
    HeartbeatWakeupOptions {
        source: Some("assignment".to_string()),
        trigger_detail: Some("system".to_string()),
        reason: Some("issue_assigned".to_string()),
        requested_by_actor_type: Some("user".to_string()),
        requested_by_actor_id: Some(fixture.user_id),
        payload: Some(json!({
            "issueId": fixture.issue_id,
            "mutation": "create",
        })),
        context_snapshot: Some(json!({
            "issueId": fixture.issue_id,
            "source": "issue.create",
        })),
        ..Default::default()
    }
}

async fn record_blocked_wake(
    pool: &PgPool,
    heartbeat: &dyn HeartbeatService,
    fixture: &Fixture,
) -> Uuid {
    heartbeat
        .wakeup_with_options(
            fixture.agent_id,
            fixture.issue_id,
            fixture.company_id,
            safe_wake(fixture),
        )
        .await
        .expect("budget-blocked wake is durably recorded");
    let request_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2
            AND requested_by_actor_id = $3 AND status = 'skipped'
            AND reason = 'task_budget_hard_stop'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(fixture.user_id)
    .fetch_one(pool)
    .await
    .expect("read source wake request id");
    request_id
}

async fn run_count(pool: &PgPool, fixture: &Fixture) -> i64 {
    run_count_for_issue(pool, fixture.company_id, fixture.issue_id).await
}

async fn run_count_for_issue(pool: &PgPool, company_id: Uuid, issue_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs
          WHERE company_id = $1
            AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(company_id)
    .bind(issue_id.to_string())
    .fetch_one(pool)
    .await
    .expect("count issue runs")
}

async fn intent_status_for_source(pool: &PgPool, source_id: Uuid) -> String {
    sqlx::query_scalar(
        "SELECT status FROM task_budget_wake_resumes
          WHERE source_wakeup_request_id = $1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .expect("read scoped replay status")
}

async fn wait_for_succeeded_run(pool: &PgPool, run_id: Uuid) -> String {
    for _ in 0..120 {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status::text FROM heartbeat_runs WHERE id = $1")
                .bind(run_id)
                .fetch_optional(pool)
                .await
                .expect("read replayed run status");
        if status.as_deref() == Some("succeeded") {
            return status.expect("succeeded status was present");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("run {run_id} did not succeed");
}

#[sqlx::test]
async fn blocked_wake_and_safe_original_options_are_stored_once_and_replayed_without_rewriting_source(
    pool: PgPool,
) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;
    let source_id = record_blocked_wake(&pool, heartbeat.as_ref(), &fixture).await;

    let (intent_id, state, source, trigger, reason, payload, context, replay_key): (
        Uuid,
        String,
        String,
        String,
        String,
        Value,
        Value,
        String,
    ) = sqlx::query_as(
        "SELECT id, status, resume_source, resume_trigger_detail, resume_reason,
                resume_payload, resume_context_snapshot, resume_idempotency_key
           FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read durable budget-wake intent");

    assert_eq!(state, "pending");
    assert_eq!(source, "assignment");
    assert_eq!(trigger, "system");
    assert_eq!(reason, "issue_assigned");
    assert_eq!(payload["mutation"], "create");
    assert_eq!(context["source"], "issue.create");
    assert_eq!(replay_key, format!("task-budget-wake-resume:{intent_id}"));
    assert_eq!(run_count(&pool, &fixture).await, 0);

    let source_state: (String, String) =
        sqlx::query_as("SELECT status::text, reason FROM agent_wakeup_requests WHERE id = $1")
            .bind(source_id)
            .fetch_one(&pool)
            .await
            .expect("read source audit request");
    assert_eq!(
        source_state,
        ("skipped".into(), "task_budget_hard_stop".into())
    );

    let update = orchestration
        .update_task_budget(fixture.company_id, fixture.issue_id, fixture.user_id, None)
        .await
        .expect("clear task budget and replay");
    assert_eq!(update.resumed_wake_count, 1);

    let source_status: String =
        sqlx::query_scalar("SELECT status::text FROM agent_wakeup_requests WHERE id = $1")
            .bind(source_id)
            .fetch_one(&pool)
            .await
            .expect("read source status after replay");
    assert_eq!(
        source_status, "skipped",
        "source skip remains an audit record"
    );

    let (outbox_status, resume_request_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read replay outcome");
    assert_eq!(outbox_status, "applied");
    let resume_request_id = resume_request_id.expect("resume request was linked");
    let (resume_key, resume_run_id): (String, Option<Uuid>) =
        sqlx::query_as("SELECT idempotency_key, run_id FROM agent_wakeup_requests WHERE id = $1")
            .bind(resume_request_id)
            .fetch_one(&pool)
            .await
            .expect("read replay request");
    assert_eq!(resume_key, replay_key);
    assert!(
        resume_run_id.is_some(),
        "applied requires a durable run link"
    );

    orchestration
        .update_task_budget(
            fixture.company_id,
            fixture.issue_id,
            fixture.user_id,
            Some(100),
        )
        .await
        .expect("change budget again");
    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("run background recovery");

    let replay_request_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(replay_key)
    .fetch_one(&pool)
    .await
    .expect("count stable replay requests");
    assert_eq!(replay_request_count, 1);
    assert_eq!(run_count(&pool, &fixture).await, 1);
}

#[sqlx::test]
async fn subtask_assignment_resume_rebuilds_only_from_its_task_action(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, false).await;
    let (orchestration, _heartbeat) = service(&pool).await;

    assert_eq!(
        orchestration
            .drain_actions(fixture.orchestration_id)
            .await
            .expect("drain assigned subtask wake"),
        1
    );
    let (source_id, source_agent, source_status, source_reason): (Uuid, Uuid, String, String) =
        sqlx::query_as(
            "SELECT id, agent_id, status::text, reason FROM agent_wakeup_requests
              WHERE company_id = $1 AND agent_id = $2 AND idempotency_key = $3",
        )
        .bind(fixture.company_id)
        .bind(fixture.owner_agent_id)
        .bind(&fixture.source_key)
        .fetch_one(&pool)
        .await
        .expect("read skipped subtask owner wake");
    assert_eq!(source_agent, fixture.owner_agent_id);
    assert_eq!(source_status, "skipped");
    assert_eq!(source_reason, "task_budget_hard_stop");

    let (intent_id, status, resume_kind, source_action, source_key, payload, context): (
        Uuid,
        String,
        String,
        Option<Uuid>,
        Option<String>,
        Option<Value>,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT id, status, resume_kind, source_task_action_id,
                original_idempotency_key, resume_payload, resume_context_snapshot
           FROM task_budget_wake_resumes WHERE source_wakeup_request_id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read typed subtask assignment intent");
    assert_eq!(status, "pending");
    assert_eq!(resume_kind, fixture.resume_kind);
    assert_eq!(source_action, Some(fixture.action_id));
    assert_eq!(source_key.as_deref(), Some(fixture.source_key.as_str()));
    assert!(payload.is_none(), "dynamic JSON is reconstructed from the action");
    assert!(context.is_none(), "dynamic context is reconstructed from the action");

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL, task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("clear task budget");
    TaskOrchestrationRecoveryJob::new(orchestration.clone())
        .execute()
        .await
        .expect("replay skipped subtask owner wake");

    let (resume_status, resume_request): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read replayed assignment intent");
    assert_eq!(resume_status, "applied");
    let resume_request = resume_request.expect("resume request is linked");
    let (resume_key, resume_run_id, resume_issue, resume_agent):
        (String, Option<Uuid>, Uuid, Uuid) = sqlx::query_as(
        "SELECT request.idempotency_key, request.run_id,
                (request.payload->>'issueId')::uuid, request.agent_id
           FROM agent_wakeup_requests request WHERE request.id = $1",
    )
    .bind(resume_request)
    .fetch_one(&pool)
    .await
    .expect("read replay request");
    assert_eq!(resume_key, format!("task-budget-wake-resume:{intent_id}"));
    assert_eq!(resume_issue, fixture.subtask_issue_id);
    assert_eq!(resume_agent, fixture.owner_agent_id);
    let run_id = resume_run_id.expect("resume request has a durable run link");
    assert_eq!(wait_for_succeeded_run(&pool, run_id).await, "succeeded");

    let (run_agent, context): (Uuid, Value) = sqlx::query_as(
        "SELECT agent_id, context_snapshot FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read resumed owner run");
    assert_eq!(run_agent, fixture.owner_agent_id);
    assert_eq!(context["issueId"], json!(fixture.subtask_issue_id));
    assert_eq!(context["taskId"], json!(fixture.subtask_issue_id));
    assert_eq!(context["wakeReason"], "subtask_assigned");
    assert_eq!(context["source"], "task_orchestration");
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status::text FROM agent_wakeup_requests WHERE id = $1",
        )
        .bind(source_id)
        .fetch_one(&pool)
        .await
        .expect("source audit remains skipped"),
        "skipped"
    );

    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("repeat recovery after applied wake");
    let replay_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(format!("task-budget-wake-resume:{intent_id}"))
    .fetch_one(&pool)
    .await
    .expect("count stable replay request");
    assert_eq!(replay_count, 1);
    assert_eq!(run_count_for_issue(&pool, fixture.company_id, fixture.subtask_issue_id).await, 1);
}

#[sqlx::test]
async fn retried_source_action_stays_owned_by_its_budget_resume(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, false).await;
    let (orchestration, _heartbeat) = service(&pool).await;
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("record original budget-blocked action");
    let source_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read source wake request");
    let intent_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2
            AND agent_id = $3 AND issue_id = $4",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .bind(fixture.owner_agent_id)
    .bind(fixture.subtask_issue_id)
    .fetch_one(&pool)
    .await
    .expect("read source-linked budget intent");

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("open budget before retried source action");
    // Model a crash after the skip and intent committed but before the action
    // row was acknowledged as applied.
    sqlx::query(
        "UPDATE task_orchestration_actions
            SET status = 'pending', finished_at = NULL, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(fixture.action_id)
    .execute(&pool)
    .await
    .expect("make source action eligible for retry");
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("retry source action after budget opened");

    let (source_status, source_reason, source_run_id): (String, String, Option<Uuid>) =
        sqlx::query_as(
            "SELECT status::text, reason, run_id
               FROM agent_wakeup_requests WHERE id = $1",
        )
        .bind(source_id)
        .fetch_one(&pool)
        .await
        .expect("read immutable source skip after retry");
    assert_eq!(source_status, "skipped");
    assert_eq!(source_reason, "task_budget_hard_stop");
    assert!(source_run_id.is_none());
    assert_eq!(intent_status_for_source(&pool, source_id).await, "pending");
    let replay_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key LIKE 'task-budget-wake-resume:%'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("count replay requests before recovery");
    assert_eq!(replay_requests, 0);
    assert_eq!(run_count_for_issue(&pool, fixture.company_id, fixture.subtask_issue_id).await, 0);

    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("replay through the durable intent");
    let (intent_status, resume_request_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read recovered intent");
    assert_eq!(intent_status, "applied");
    let resume_request_id = resume_request_id.expect("replay request was linked");
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT run_id FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(resume_request_id)
    .fetch_one(&pool)
    .await
    .expect("read replay run id");
    assert_eq!(wait_for_succeeded_run(&pool, run_id).await, "succeeded");
    assert_eq!(run_count_for_issue(&pool, fixture.company_id, fixture.subtask_issue_id).await, 1);
    let source_status: String = sqlx::query_scalar(
        "SELECT status::text FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("source skip remains visible after replay");
    assert_eq!(source_status, "skipped");
}

#[sqlx::test]
async fn unrelated_idempotent_wake_is_not_claimed_by_another_source_intent(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, false).await;
    let (orchestration, heartbeat) = service(&pool).await;
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("create the original source intent");
    let original_source_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read original source request");
    let matching_intent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2
            AND agent_id = $3 AND issue_id = $4",
    )
    .bind(fixture.company_id)
    .bind(original_source_id)
    .bind(fixture.owner_agent_id)
    .bind(fixture.subtask_issue_id)
    .fetch_one(&pool)
    .await
    .expect("count original matching intent");
    assert_eq!(matching_intent, 1);

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("open budget for the independent wake");

    // This separate request has the same company/agent/issue tuple but its own
    // source idempotency key and no linked budget intent. It must still requeue.
    let unrelated_key = format!("independent-task-wake:{}", Uuid::new_v4());
    let unrelated_source_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agent_wakeup_requests
            (id, company_id, agent_id, status, payload, source, trigger_detail,
             reason, idempotency_key, finished_at, requested_at, updated_at)
         VALUES ($1, $2, $3, 'skipped', $4, 'automation', 'task_orchestration',
                 'independent_task_wake', $5, NOW(), NOW(), NOW())",
    )
    .bind(unrelated_source_id)
    .bind(fixture.company_id)
    .bind(fixture.owner_agent_id)
    .bind(json!({ "issueId": fixture.subtask_issue_id }))
    .bind(&unrelated_key)
    .execute(&pool)
    .await
    .expect("insert independent skipped wake");
    heartbeat
        .wakeup_with_options(
            fixture.owner_agent_id,
            fixture.subtask_issue_id,
            fixture.company_id,
            HeartbeatWakeupOptions {
                source: Some("automation".to_string()),
                trigger_detail: Some("task_orchestration".to_string()),
                reason: Some("independent_task_wake".to_string()),
                idempotency_key: Some(unrelated_key),
                payload: Some(json!({ "issueId": fixture.subtask_issue_id })),
                context_snapshot: Some(json!({
                    "issueId": fixture.subtask_issue_id,
                    "taskId": fixture.subtask_issue_id,
                    "wakeReason": "independent_task_wake",
                    "source": "task_orchestration",
                })),
                ..Default::default()
            },
        )
        .await
        .expect("requeue independent wake");
    let (unrelated_status, unrelated_run_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status::text, run_id FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(unrelated_source_id)
    .fetch_one(&pool)
    .await
    .expect("read independent wake after requeue");
    assert!(matches!(unrelated_status.as_str(), "dispatched" | "running" | "completed"));
    let unrelated_run_id = unrelated_run_id.expect("independent wake owns its run");
    assert_eq!(wait_for_succeeded_run(&pool, unrelated_run_id).await, "succeeded");
    assert_eq!(intent_status_for_source(&pool, original_source_id).await, "pending");
}

#[sqlx::test]
async fn source_action_retry_without_budget_intent_keeps_normal_idempotency(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, false).await;
    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("remove task budget before initial dispatch");
    let (orchestration, _heartbeat) = service(&pool).await;
    assert_eq!(
        orchestration
            .drain_actions(fixture.orchestration_id)
            .await
            .expect("dispatch without a task budget block"),
        1
    );
    let source_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read normal source wake");
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT run_id FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("normal source wake has a run");
    assert_eq!(wait_for_succeeded_run(&pool, run_id).await, "succeeded");

    sqlx::query(
        "UPDATE task_orchestration_actions
            SET status = 'pending', finished_at = NULL, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(fixture.action_id)
    .execute(&pool)
    .await
    .expect("make already-dispatched source action eligible for retry");
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("retry normally dispatched source action");
    let intent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("count source intents");
    assert_eq!(intent_count, 0);
    assert_eq!(run_count_for_issue(&pool, fixture.company_id, fixture.subtask_issue_id).await, 1);
    let source_run_id: Uuid = sqlx::query_scalar(
        "SELECT run_id FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read source run after retry");
    assert_eq!(source_run_id, run_id);
}

#[sqlx::test]
async fn accepted_subtask_resume_targets_its_reviewer_on_the_root_scope(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, true).await;
    assert_ne!(fixture.reviewer_agent_id, fixture.ceo_agent_id);
    let root_assignee: Uuid = sqlx::query_scalar(
        "SELECT assignee_agent_id FROM issues WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .fetch_one(&pool)
    .await
    .expect("read root assignment");
    assert_eq!(root_assignee, fixture.ceo_agent_id);
    assert_ne!(root_assignee, fixture.reviewer_agent_id);

    let (orchestration, _heartbeat) = service(&pool).await;
    assert_eq!(
        orchestration
            .drain_actions(fixture.orchestration_id)
            .await
            .expect("drain accepted reviewer wake"),
        1
    );
    let (source_id, source_agent, source_status): (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT id, agent_id, status::text FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2 AND idempotency_key = $3",
    )
    .bind(fixture.company_id)
    .bind(fixture.reviewer_agent_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read skipped accepted reviewer wake");
    assert_eq!(source_agent, fixture.reviewer_agent_id);
    assert_eq!(source_status, "skipped");
    let (intent_id, resume_kind, source_action, source_key, payload, context): (
        Uuid,
        String,
        Option<Uuid>,
        Option<String>,
        Option<Value>,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT id, resume_kind, source_task_action_id,
                original_idempotency_key, resume_payload, resume_context_snapshot
           FROM task_budget_wake_resumes WHERE source_wakeup_request_id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read accepted reviewer intent");
    assert_eq!(resume_kind, fixture.resume_kind);
    assert_eq!(source_action, Some(fixture.action_id));
    assert_eq!(source_key.as_deref(), Some(fixture.source_key.as_str()));
    assert!(payload.is_none());
    assert!(context.is_none());

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL, task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("clear task budget");
    TaskOrchestrationRecoveryJob::new(orchestration.clone())
        .execute()
        .await
        .expect("replay accepted reviewer wake");

    let (intent_status, resume_request): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read accepted resume status");
    assert_eq!(intent_status, "applied");
    let resume_request = resume_request.expect("resume request is linked");
    let (resume_agent, resume_issue, run_id): (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT request.agent_id, (request.payload->>'issueId')::uuid, request.run_id
           FROM agent_wakeup_requests request WHERE request.id = $1",
    )
    .bind(resume_request)
    .fetch_one(&pool)
    .await
    .expect("read accepted resume request");
    assert_eq!(resume_agent, fixture.reviewer_agent_id);
    assert_eq!(resume_issue, fixture.root_issue_id);
    assert_eq!(wait_for_succeeded_run(&pool, run_id).await, "succeeded");
    let context: Value = sqlx::query_scalar(
        "SELECT context_snapshot FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read accepted reviewer run context");
    assert_eq!(context["issueId"], json!(fixture.root_issue_id));
    assert_eq!(context["taskId"], json!(fixture.root_issue_id));
    assert_eq!(context["wakeReason"], "subtask_accepted");
    assert_eq!(context["source"], "task_orchestration");
    assert_eq!(
        run_count_for_issue(&pool, fixture.company_id, fixture.root_issue_id).await,
        1,
        "reviewer continuation must not be checked against the root CEO assignment"
    );
}

#[sqlx::test]
async fn reviewer_change_between_claim_and_dispatch_cancels_the_resume(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, true).await;
    let (orchestration, _heartbeat) = service(&pool).await;
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("record blocked accepted-reviewer wake");
    let (source_id, intent_id, resume_key): (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT source.id, resume.id, resume.resume_idempotency_key
           FROM agent_wakeup_requests source
           JOIN task_budget_wake_resumes resume
             ON resume.company_id = source.company_id
            AND resume.source_wakeup_request_id = source.id
          WHERE source.company_id = $1 AND source.idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read accepted source and intent");

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.root_issue_id)
    .execute(&pool)
    .await
    .expect("open budget for the reviewer replay");

    // Change the durable reviewer after the outbox's first live-state check,
    // but before Heartbeat's final transaction validates and dispatches. This
    // exercises the race boundary without weakening the successful reviewer
    // case above.
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION change_reviewer_before_resume_request()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
               IF NEW.idempotency_key LIKE 'task-budget-wake-resume:%' THEN
                   UPDATE task_acceptance_decisions decision
                      SET reviewer_agent_id = subtask.owner_agent_id
                     FROM task_budget_wake_resumes resume
                     JOIN task_orchestration_actions action
                       ON action.id = resume.source_task_action_id
                      AND action.company_id = resume.company_id
                     JOIN task_deliverables deliverable
                       ON deliverable.orchestration_id = action.orchestration_id
                      AND deliverable.company_id = action.company_id
                     JOIN task_orchestration_subtasks subtask
                       ON subtask.id = deliverable.subtask_id
                      AND subtask.orchestration_id = action.orchestration_id
                      AND subtask.company_id = action.company_id
                    WHERE resume.company_id = NEW.company_id
                      AND resume.resume_idempotency_key = NEW.idempotency_key
                      AND resume.resume_kind = 'subtask_accepted'
                      AND decision.company_id = resume.company_id
                      AND decision.deliverable_id = deliverable.id;
               END IF;
               RETURN NEW;
           END;
           $$"#,
    )
    .execute(&pool)
    .await
    .expect("create reviewer-change race function");
    sqlx::query(
        "CREATE TRIGGER change_reviewer_before_resume_request
         BEFORE INSERT ON agent_wakeup_requests
         FOR EACH ROW EXECUTE FUNCTION change_reviewer_before_resume_request()",
    )
    .execute(&pool)
    .await
    .expect("install reviewer-change race trigger");

    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("recover accepted reviewer intent");

    let (intent_status, last_error, resume_request_id): (String, Option<String>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT status, last_error, resume_wakeup_request_id
               FROM task_budget_wake_resumes WHERE id = $1",
        )
        .bind(intent_id)
        .fetch_one(&pool)
        .await
        .expect("read reviewer-change intent outcome");
    assert_eq!(intent_status, "cancelled");
    assert_eq!(
        last_error.as_deref(),
        Some("heartbeat cancelled the replay request before dispatch")
    );
    let resume_request_id = resume_request_id.expect("cancelled replay request stays linked");
    let (request_status, request_reason, request_run_id): (String, Option<String>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT status::text, reason, run_id
               FROM agent_wakeup_requests WHERE id = $1",
        )
        .bind(resume_request_id)
        .fetch_one(&pool)
        .await
        .expect("read cancelled reviewer replay request");
    assert_eq!(request_status, "cancelled");
    assert_eq!(request_reason.as_deref(), Some("task_budget_resume_cancelled"));
    assert!(request_run_id.is_none());
    assert_eq!(
        run_count_for_issue(&pool, fixture.company_id, fixture.root_issue_id).await,
        0,
        "a reviewer change between the outer and final checks must not dispatch"
    );
    let source_status: String = sqlx::query_scalar(
        "SELECT status::text FROM agent_wakeup_requests WHERE id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read original source audit");
    assert_eq!(source_status, "skipped");
}

#[sqlx::test]
async fn cancelled_orchestration_cannot_replay_a_pending_subtask_wake(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed_orchestration_wake(&pool, false).await;
    let (orchestration, _heartbeat) = service(&pool).await;
    orchestration
        .drain_actions(fixture.orchestration_id)
        .await
        .expect("persist blocked owner wake");
    let source_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(&fixture.source_key)
    .fetch_one(&pool)
    .await
    .expect("read blocked owner wake");
    let intent_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_budget_wake_resumes WHERE source_wakeup_request_id = $1",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read pending resume");

    orchestration
        .cancel(fixture.company_id, fixture.root_issue_id)
        .await
        .expect("cancel task before resume");
    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("recovery observes task cancellation");

    let status: String = sqlx::query_scalar(
        "SELECT status FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read cancelled resume");
    assert_eq!(status, "cancelled");
    assert_eq!(run_count_for_issue(&pool, fixture.company_id, fixture.subtask_issue_id).await, 0);
}

#[sqlx::test]
async fn required_orchestration_recovery_wake_is_resumed_after_budget_is_raised(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    sqlx::query("UPDATE agents SET role = 'ceo' WHERE company_id = $1 AND id = $2")
        .bind(fixture.company_id)
        .bind(fixture.agent_id)
        .execute(&pool)
        .await
        .expect("promote test agent to CEO");
    sqlx::query(
        "UPDATE issues
            SET task_orchestration_required = TRUE,
                created_at = NOW() - INTERVAL '31 seconds'
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("mark root for required-orchestration recovery");

    // App initialization skips this durable marker until the recovery worker
    // creates its orchestration and its initial wake.
    let (orchestration, _) = service(&pool).await;
    assert_eq!(run_count(&pool, &fixture).await, 0);
    TaskOrchestrationRecoveryJob::new(orchestration.clone())
        .execute()
        .await
        .expect("recover required orchestration and drain its initial wake");

    let initial_key = format!("task-orchestration-initial:{}", fixture.issue_id);
    let (source_id, source_status, source_reason, source_run_id): (
        Uuid,
        String,
        String,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT id, status::text, reason, run_id FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2 AND idempotency_key = $3",
    )
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(&initial_key)
    .fetch_one(&pool)
    .await
    .expect("read budget-blocked initial recovery wake");
    assert_eq!(source_status, "skipped");
    assert_eq!(source_reason, "task_budget_hard_stop");
    assert!(source_run_id.is_none());

    let (intent_id, intent_status, resume_source, trigger_detail, reason, payload, context): (
        Uuid,
        String,
        String,
        String,
        String,
        Value,
        Value,
    ) = sqlx::query_as(
        "SELECT id, status, resume_source, resume_trigger_detail, resume_reason,
                resume_payload, resume_context_snapshot
           FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read recovery wake intent");
    assert_eq!(intent_status, "pending");
    assert_eq!(resume_source, "assignment");
    assert_eq!(trigger_detail, "system");
    assert_eq!(reason, "issue_assigned");
    assert_eq!(
        payload,
        json!({ "issueId": fixture.issue_id, "mutation": "create_recovery" })
    );
    assert_eq!(
        context,
        json!({ "issueId": fixture.issue_id, "source": "issue.create.recovery" })
    );
    assert_eq!(run_count(&pool, &fixture).await, 0);

    let update = orchestration
        .update_task_budget(
            fixture.company_id,
            fixture.issue_id,
            fixture.user_id,
            Some(1),
        )
        .await
        .expect("raise the cap and replay the recovery wake");
    assert_eq!(update.resumed_wake_count, 1);

    let (intent_status, resume_request_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read recovery replay outcome");
    assert_eq!(intent_status, "applied");
    let resume_request_id = resume_request_id.expect("recovery replay request was linked");
    assert_ne!(resume_request_id, source_id);
    let source_status: String =
        sqlx::query_scalar("SELECT status::text FROM agent_wakeup_requests WHERE id = $1")
            .bind(source_id)
            .fetch_one(&pool)
            .await
            .expect("read original recovery source audit state");
    assert_eq!(source_status, "skipped");
    let run_id: Uuid = sqlx::query_scalar("SELECT run_id FROM agent_wakeup_requests WHERE id = $1")
        .bind(resume_request_id)
        .fetch_one(&pool)
        .await
        .expect("read recovery replay run id");
    assert_eq!(wait_for_succeeded_run(&pool, run_id).await, "succeeded");
    assert_eq!(run_count(&pool, &fixture).await, 1);
}

#[sqlx::test]
async fn budget_update_only_resumes_intents_for_its_company_and_root(pool: PgPool) {
    migrate(&pool).await;
    let target_root = seed(&pool).await;
    let same_company_root_id = seed_additional_root(&pool, &target_root).await;
    let other_company_root = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;

    let target_source = record_blocked_wake(&pool, heartbeat.as_ref(), &target_root).await;
    let same_company_root = Fixture {
        issue_id: same_company_root_id,
        ..target_root.clone()
    };
    let same_company_source =
        record_blocked_wake(&pool, heartbeat.as_ref(), &same_company_root).await;
    let other_company_source =
        record_blocked_wake(&pool, heartbeat.as_ref(), &other_company_root).await;

    let update = orchestration
        .update_task_budget(
            target_root.company_id,
            target_root.issue_id,
            target_root.user_id,
            None,
        )
        .await
        .expect("clear only the selected root budget");
    assert_eq!(update.resumed_wake_count, 1);

    assert_eq!(
        intent_status_for_source(&pool, target_source).await,
        "applied"
    );
    assert_eq!(
        intent_status_for_source(&pool, same_company_source).await,
        "pending"
    );
    assert_eq!(
        intent_status_for_source(&pool, other_company_source).await,
        "pending"
    );
    assert_eq!(
        run_count_for_issue(&pool, target_root.company_id, target_root.issue_id).await,
        1
    );
    assert_eq!(
        run_count_for_issue(&pool, target_root.company_id, same_company_root_id).await,
        0
    );
    assert_eq!(
        run_count_for_issue(
            &pool,
            other_company_root.company_id,
            other_company_root.issue_id
        )
        .await,
        0
    );
}

#[sqlx::test]
async fn source_skip_and_intent_roll_back_after_claim_when_intent_insert_fails(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (_, heartbeat) = service(&pool).await;
    // build_app_state reconciles pending assigned issues, so the company may
    // already have a separate budget-resume intent before this failure is
    // injected. Capture that baseline and isolate this wake by its own key.
    let intent_count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_budget_wake_resumes WHERE company_id = $1")
            .bind(fixture.company_id)
            .fetch_one(&pool)
            .await
            .expect("count pre-existing intents before failure injection");
    let idempotency_key = format!("rollback-test:{}", Uuid::new_v4());
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION fail_task_budget_wake_resume_insert()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
               RAISE EXCEPTION 'intent insert failure injected by integration test';
           END;
           $$"#,
    )
    .execute(&pool)
    .await
    .expect("create failure injection function");
    sqlx::query(
        "CREATE TRIGGER fail_task_budget_wake_resume_insert
         BEFORE INSERT ON task_budget_wake_resumes
         FOR EACH ROW EXECUTE FUNCTION fail_task_budget_wake_resume_insert()",
    )
    .execute(&pool)
    .await
    .expect("create failure injection trigger");

    let mut options = safe_wake(&fixture);
    options.idempotency_key = Some(idempotency_key.clone());
    let result = heartbeat
        .wakeup_with_options(
            fixture.agent_id,
            fixture.issue_id,
            fixture.company_id,
            options,
        )
        .await;
    let error = result.expect_err("outbox failure must fail the skipped wake write");
    assert!(
        error
            .to_string()
            .contains("intent insert failure injected by integration test"),
        "expected the injected outbox failure, got: {error}"
    );

    let source: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status::text FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2 AND requested_by_actor_id = $3
            AND idempotency_key = $4",
    )
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(fixture.user_id)
    .bind(&idempotency_key)
    .fetch_optional(&pool)
    .await
    .expect("read claimed source request after rollback");
    let (source_request_id, source_status) =
        source.expect("the separately committed claim must remain available");
    assert_eq!(
        source_status, "queued",
        "the failed transaction must not commit the skipped state"
    );
    let linked_intent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_request_id)
    .fetch_one(&pool)
    .await
    .expect("count intents for the isolated source request");
    assert_eq!(linked_intent_count, 0);
    let intent_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_budget_wake_resumes WHERE company_id = $1")
            .bind(fixture.company_id)
            .fetch_one(&pool)
            .await
            .expect("count durable intents after rollback");
    assert_eq!(intent_count, intent_count_before);
}

#[sqlx::test]
async fn recovery_reclaims_expired_lease_and_repeated_budget_blocks_do_not_create_intent_storm(
    pool: PgPool,
) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;
    let source_id = record_blocked_wake(&pool, heartbeat.as_ref(), &fixture).await;
    let intent_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read intent id");
    let recovery = TaskOrchestrationRecoveryJob::new(orchestration.clone());

    // Repeated scans while the cap is still closed must leave one durable
    // source intent and at most one stable replay request.
    for _ in 0..3 {
        recovery
            .execute()
            .await
            .expect("recover while still blocked");
    }
    let source_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("count source intents");
    assert_eq!(source_count, 1);
    let blocked_intent_state: String =
        sqlx::query_scalar("SELECT status FROM task_budget_wake_resumes WHERE id = $1")
            .bind(intent_id)
            .fetch_one(&pool)
            .await
            .expect("read budget-blocked intent state");
    assert_eq!(blocked_intent_state, "pending");
    let stable_replay_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key LIKE 'task-budget-wake-resume:%'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("count stable replay requests while blocked");
    assert!(stable_replay_requests <= 1);
    assert_eq!(run_count(&pool, &fixture).await, 0);

    // Simulate a process crash after claiming the intent, then make the budget
    // affordable. The expired claim must be released by the scheduled worker.
    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("open budget without inline dispatch");
    sqlx::query(
        "UPDATE task_budget_wake_resumes
            SET status = 'processing', claim_token = gen_random_uuid(),
                next_attempt_at = NOW() - INTERVAL '1 minute',
                claim_expires_at = NOW() - INTERVAL '1 minute',
                updated_at = NOW()
          WHERE id = $1",
    )
    .bind(intent_id)
    .execute(&pool)
    .await
    .expect("simulate stale recovery lease");

    recovery.execute().await.expect("recover expired lease");
    recovery
        .execute()
        .await
        .expect("repeat recovery after dispatch");
    let (outbox_status, resume_request_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read recovered intent");
    assert_eq!(outbox_status, "applied");
    assert!(resume_request_id.is_some());
    assert_eq!(run_count(&pool, &fixture).await, 1);
}

#[sqlx::test]
async fn unsafe_context_is_unsupported_and_legacy_skips_are_not_backfilled(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;

    let unsafe_options = HeartbeatWakeupOptions {
        source: Some("assignment".to_string()),
        trigger_detail: Some("system".to_string()),
        reason: Some("issue_assigned".to_string()),
        requested_by_actor_type: Some("user".to_string()),
        requested_by_actor_id: Some(fixture.user_id),
        payload: Some(json!({ "issueId": fixture.issue_id })),
        context_snapshot: Some(json!({
            "issueId": fixture.issue_id,
            "operatorNote": "unclassified private free text",
        })),
        ..Default::default()
    };
    heartbeat
        .wakeup_with_options(
            fixture.agent_id,
            fixture.issue_id,
            fixture.company_id,
            unsafe_options,
        )
        .await
        .expect("unsafe wake is still recorded as skipped");
    let unsafe_intent: (String, Option<String>) = sqlx::query_as(
        "SELECT status, resume_context_snapshot::text
           FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = (
              SELECT id FROM agent_wakeup_requests
               WHERE company_id = $1 AND agent_id = $2 AND requested_by_actor_id = $3
                 AND status = 'skipped' AND reason = 'task_budget_hard_stop'
               ORDER BY created_at DESC LIMIT 1
          )",
    )
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(fixture.user_id)
    .fetch_one(&pool)
    .await
    .expect("read unsupported intent");
    assert_eq!(unsafe_intent.0, "unsupported");
    assert!(
        !unsafe_intent
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("unclassified private free text"),
        "unsupported free text must not be persisted for replay"
    );

    // Caller-supplied task-shaped JSON is not the internal typed reference;
    // keep these lookalikes unsupported and do not promote them to assignments.
    for (reason, idempotency_key) in [
        (
            "subtask_assigned",
            format!("subtask-assigned:{}", fixture.issue_id),
        ),
        (
            "subtask_accepted",
            format!("subtask-accepted:{}:{}", fixture.issue_id, Uuid::new_v4()),
        ),
    ] {
        let options = HeartbeatWakeupOptions {
            source: Some("automation".to_string()),
            trigger_detail: Some("task_orchestration".to_string()),
            reason: Some(reason.to_string()),
            requested_by_actor_type: Some("system".to_string()),
            idempotency_key: Some(idempotency_key.clone()),
            payload: Some(json!({
                "issueId": fixture.issue_id,
                "agentId": fixture.agent_id,
                "reason": reason,
            })),
            context_snapshot: Some(json!({
                "issueId": fixture.issue_id,
                "taskId": fixture.issue_id,
                "wakeReason": reason,
                "source": "task_orchestration",
            })),
            ..Default::default()
        };
        heartbeat
            .wakeup_with_options(
                fixture.agent_id,
                fixture.issue_id,
                fixture.company_id,
                options.clone(),
            )
            .await
            .expect("task-specific orchestration wake is recorded");
        let intent: (String, Option<Value>, Option<Value>) = sqlx::query_as(
            "SELECT resume.status, resume.resume_payload, resume.resume_context_snapshot
               FROM task_budget_wake_resumes resume
               JOIN agent_wakeup_requests source
                 ON source.id = resume.source_wakeup_request_id
              WHERE resume.company_id = $1 AND source.idempotency_key = $2",
        )
        .bind(fixture.company_id)
        .bind(&idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("read task-specific unsupported intent");
        assert_eq!(intent.0, "unsupported");
        assert!(intent.1.is_none());
        assert!(intent.2.is_none());

        heartbeat
            .wakeup_with_options(
                fixture.agent_id,
                fixture.issue_id,
                fixture.company_id,
                options,
            )
            .await
            .expect("retry must leave the unsupported source audit unchanged");
        let (source_status, intent_status): (String, String) = sqlx::query_as(
            "SELECT source.status::text, resume.status
               FROM agent_wakeup_requests source
               JOIN task_budget_wake_resumes resume
                 ON resume.company_id = source.company_id
                AND resume.source_wakeup_request_id = source.id
              WHERE source.company_id = $1 AND source.idempotency_key = $2",
        )
        .bind(fixture.company_id)
        .bind(&idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("read unsupported source after retry");
        assert_eq!(source_status, "skipped");
        assert_eq!(intent_status, "unsupported");
    }

    // This row predates the outbox and contains only the gate's replacement
    // payload, so recovery must not guess the original context or assignment.
    let legacy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agent_wakeup_requests
            (id, company_id, agent_id, status, payload, source, trigger_detail, reason,
             finished_at, requested_at, updated_at)
         VALUES ($1, $2, $3, 'skipped', $4, 'assignment', 'system',
                 'task_budget_hard_stop', NOW(), NOW(), NOW())",
    )
    .bind(legacy_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(json!({
        "issueId": fixture.issue_id,
        "heartbeatSkip": { "reason": "task_budget_hard_stop" },
    }))
    .execute(&pool)
    .await
    .expect("insert legacy skipped request");

    sqlx::query(
        "UPDATE issues SET task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("open budget for recovery");
    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("run recovery");

    let legacy_intents: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(legacy_id)
    .fetch_one(&pool)
    .await
    .expect("count legacy backfills");
    assert_eq!(legacy_intents, 0);
    assert_eq!(run_count(&pool, &fixture).await, 0);
}

#[sqlx::test]
async fn cancelled_issue_cancels_pending_resume_without_dispatch(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;
    let source_id = record_blocked_wake(&pool, heartbeat.as_ref(), &fixture).await;
    let intent_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read pending intent");

    sqlx::query(
        "UPDATE issues SET status = 'cancelled', cancelled_at = NOW(),
                            task_budget_cents = NULL,
                            task_budget_revision = task_budget_revision + 1
          WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("cancel issue and open budget");
    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("recover cancelled intent");

    let (status, resume_request_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, resume_wakeup_request_id FROM task_budget_wake_resumes WHERE id = $1",
    )
    .bind(intent_id)
    .fetch_one(&pool)
    .await
    .expect("read cancelled intent");
    assert_eq!(status, "cancelled");
    assert!(resume_request_id.is_none());
    assert_eq!(run_count(&pool, &fixture).await, 0);
}

#[sqlx::test]
async fn cross_company_outbox_row_is_cancelled_without_dispatch(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (orchestration, heartbeat) = service(&pool).await;
    let source_id = record_blocked_wake(&pool, heartbeat.as_ref(), &fixture).await;
    let intent_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM task_budget_wake_resumes
          WHERE company_id = $1 AND source_wakeup_request_id = $2",
    )
    .bind(fixture.company_id)
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .expect("read pending intent");

    let foreign_company_id = Uuid::new_v4();
    let foreign_prefix = format!("FC{}", &foreign_company_id.simple().to_string()[..8]);
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents)
         VALUES ($1, 'Foreign tenant for outbox check', $2, FALSE)",
    )
    .bind(foreign_company_id)
    .bind(foreign_prefix)
    .execute(&pool)
    .await
    .expect("insert second company");
    // Simulate a malformed or stale intent row. The issue, root, agent and
    // source request still belong to the original company.
    sqlx::query("UPDATE task_budget_wake_resumes SET company_id = $2 WHERE id = $1")
        .bind(intent_id)
        .bind(foreign_company_id)
        .execute(&pool)
        .await
        .expect("move intent into the wrong company scope");

    TaskOrchestrationRecoveryJob::new(orchestration)
        .execute()
        .await
        .expect("recover mismatched tenant intent");
    let status: String =
        sqlx::query_scalar("SELECT status FROM task_budget_wake_resumes WHERE id = $1")
            .bind(intent_id)
            .fetch_one(&pool)
            .await
            .expect("read mismatched intent state");
    assert_eq!(status, "cancelled");
    assert_eq!(run_count(&pool, &fixture).await, 0);
}
