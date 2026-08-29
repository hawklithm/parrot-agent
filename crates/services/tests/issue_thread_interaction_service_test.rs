use models::{
    CreateThreadInteractionInput, ItemVerdict, SubmitItemVerdictsInput,
    RejectThreadInteractionInput, WithdrawInteractionInput,
};
use services::issue_thread_interaction_service::{
    InteractionCreator, InteractionResolver, IssueThreadInteractionService,
};
use services::{DefaultHeartbeatService, HeartbeatService, HeartbeatWakeupOptions};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_company_and_issue(pool: &PgPool) -> (Uuid, models::Issue) {
    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("IT{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Interaction service test")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status) VALUES ($1, $2, $3, $4, 'todo')",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind("Interaction service issue")
    .bind(format!("{}-1", &company_id.simple().to_string()[..8]))
    .execute(pool)
    .await
    .expect("insert issue");

    let issue = sqlx::query_as::<_, models::Issue>("SELECT * FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(pool)
        .await
        .expect("load issue");
    (company_id, issue)
}

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn cleanup(pool: &PgPool, company_id: Uuid) {
    let _ = sqlx::query("DELETE FROM issue_thread_interactions WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
}

fn user_resolver() -> InteractionResolver {
    InteractionResolver {
        resolver_type: "user".to_string(),
        resolver_id: "board-test-user".to_string(),
        run_id: None,
    }
}

#[sqlx::test]
async fn create_is_idempotent_and_uses_canonical_columns(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let service = IssueThreadInteractionService::new(pool.clone());
    let input = CreateThreadInteractionInput {
        kind: "question".to_string(),
        payload: serde_json::json!({ "question": "Continue?" }),
        title: Some("Continue?".to_string()),
        summary: None,
        continuation_policy: "wake_assignee".to_string(),
        resolver_policy: Some("board_only".to_string()),
        idempotency_key: Some("same-request".to_string()),
        addressee_agent_id: None,
        source_run_id: None,
        source_comment_id: None,
    };

    let first = service
        .create(&issue, input.clone(), InteractionCreator { agent_id: None, user_id: None })
        .await
        .expect("create interaction");
    let second = service
        .create(&issue, input, InteractionCreator { agent_id: None, user_id: None })
        .await
        .expect("replay interaction");

    assert_eq!(first.id, second.id);
    assert_eq!(second.requested_resolver_policy, "human_only");
    assert_eq!(second.effective_resolver_policy, "human_only");
    assert_eq!(second.resolver_policy_provenance, "explicit");
    assert_eq!(second.effective_resolver_policy_source, "requested");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_thread_interactions WHERE company_id = $1 AND issue_id = $2",
    )
    .bind(company_id)
    .bind(issue.id)
    .fetch_one(&pool)
    .await
    .expect("count interactions");
    assert_eq!(count, 1);
    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn resolver_policy_applies_company_caps_and_governed_action_floor(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    sqlx::query("UPDATE companies SET interaction_resolver_governance = $2 WHERE id = $1")
        .bind(company_id)
        .bind(serde_json::json!({
            "ask_user_questions": { "defaultPolicy": "not_creator" },
            "request_confirmation": { "cap": "human_only" },
        }))
        .execute(&pool)
        .await
        .expect("set interaction governance");

    let service = IssueThreadInteractionService::new(pool.clone());
    let capped = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "ask_user_questions".to_string(),
                payload: serde_json::json!({ "questions": [] }),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: None,
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect("create capped interaction");
    assert_eq!(capped.requested_resolver_policy, "not_creator");
    assert_eq!(capped.effective_resolver_policy, "not_creator");
    assert_eq!(capped.resolver_policy_provenance, "inherited");
    assert_eq!(capped.effective_resolver_policy_source, "requested");

    let governed = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "request_confirmation".to_string(),
                payload: serde_json::json!({
                    "toolAction": { "name": "deploy" },
                }),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: Some("anyone".to_string()),
                idempotency_key: None,
                addressee_agent_id: None,
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect("create governed interaction");
    assert_eq!(governed.requested_resolver_policy, "anyone");
    assert_eq!(governed.effective_resolver_policy, "human_only");
    assert_eq!(governed.resolver_policy_provenance, "explicit");
    assert_eq!(governed.effective_resolver_policy_source, "governed_action");

    let invalid = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: Some("unsupported".to_string()),
                idempotency_key: None,
                addressee_agent_id: None,
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("unsupported resolver policy must fail");
    assert!(invalid.contains("Unsupported interaction resolver policy"));

    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn create_rejects_invalid_addressee_and_source_references(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, 'idle', 'process')")
        .bind(agent_id)
        .bind(company_id)
        .bind("Addressee validation agent")
        .execute(&pool)
        .await
        .expect("insert addressee agent");
    let service = IssueThreadInteractionService::new(pool.clone());

    let self_addressed = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: Some(agent_id),
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: Some(agent_id), user_id: None },
        )
        .await
        .expect_err("agents must not address interactions to themselves");
    assert!(self_addressed.contains("themselves"));

    let tool_addressed = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "request_confirmation".to_string(),
                payload: serde_json::json!({ "toolAction": { "name": "deploy" } }),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: Some(agent_id),
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("tool actions must not target agents");
    assert!(tool_addressed.contains("cannot be addressed"));

    sqlx::query("UPDATE agents SET status = 'paused' WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("pause addressee agent");
    let paused_addressed = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: Some(agent_id),
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("paused addressee must not be accepted");
    assert!(paused_addressed.contains("invokable"), "{paused_addressed}");

    let missing_source_run = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: None,
                source_run_id: Some(Uuid::new_v4()),
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("source run must belong to the company");
    assert!(missing_source_run.contains("sourceRunId"));

    let missing_source_comment = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: None,
                source_run_id: None,
                source_comment_id: Some(Uuid::new_v4()),
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("source comment must belong to the issue");
    assert!(missing_source_comment.contains("sourceCommentId"));

    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn create_rejects_addressee_with_invalid_reporting_chain(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let manager_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type)
         VALUES ($1, $2, 'Terminated manager', 'terminated', 'process')",
    )
    .bind(manager_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert terminated manager");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type, reports_to)
         VALUES ($1, $2, 'Child agent', 'idle', 'process', $3)",
    )
    .bind(child_id)
    .bind(company_id)
    .bind(manager_id)
    .execute(&pool)
    .await
    .expect("insert child agent");

    let service = IssueThreadInteractionService::new(pool.clone());
    let terminated_manager = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: Some(child_id),
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("terminated manager must block addressee");
    assert!(terminated_manager.contains("manager_terminated"), "{terminated_manager}");

    sqlx::query("UPDATE agents SET status = 'idle', reports_to = $2 WHERE id = $1")
        .bind(manager_id)
        .bind(child_id)
        .execute(&pool)
        .await
        .expect("create reporting cycle");
    let reporting_cycle = service
        .create(
            &issue,
            CreateThreadInteractionInput {
                kind: "question".to_string(),
                payload: serde_json::json!({}),
                title: None,
                summary: None,
                continuation_policy: "none".to_string(),
                resolver_policy: None,
                idempotency_key: None,
                addressee_agent_id: Some(child_id),
                source_run_id: None,
                source_comment_id: None,
            },
            InteractionCreator { agent_id: None, user_id: None },
        )
        .await
        .expect_err("reporting cycle must block addressee");
    assert!(reporting_cycle.contains("reporting_cycle"), "{reporting_cycle}");
    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn withdraw_updates_canonical_resolver_fields_in_one_transaction(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let interaction_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_thread_interactions
         (id, company_id, issue_id, kind, status, continuation_policy, requested_resolver_policy,
          effective_resolver_policy, payload, created_at, updated_at)
         VALUES ($1, $2, $3, 'question', 'pending', 'wake_assignee', 'board_only', 'board_only', '{}', NOW(), NOW())",
    )
    .bind(interaction_id)
    .bind(company_id)
    .bind(issue.id)
    .execute(&pool)
    .await
    .expect("insert pending interaction");

    let service = IssueThreadInteractionService::new(pool.clone());
    let withdrawn = service
        .withdraw_interaction(
            issue.id,
            interaction_id,
            WithdrawInteractionInput { reason: Some("no longer needed".to_string()) },
            user_resolver(),
        )
        .await
        .expect("withdraw interaction");

    assert_eq!(withdrawn.status, "cancelled");
    assert_eq!(withdrawn.resolved_by_user_id.as_deref(), Some("board-test-user"));
    assert!(withdrawn.resolved_at.is_some());
    assert_eq!(withdrawn.result.as_ref().and_then(|v| v.get("outcome")).and_then(|v| v.as_str()), Some("withdrawn"));

    let issue_updated: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM issues WHERE id = $1",
    )
    .bind(issue.id)
    .fetch_one(&pool)
    .await
    .expect("load issue timestamp");
    assert!(issue_updated >= withdrawn.updated_at);
    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn item_verdicts_merge_partial_submissions_and_replay_completed_request(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let interaction_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_thread_interactions
         (id, company_id, issue_id, kind, status, continuation_policy, requested_resolver_policy,
          effective_resolver_policy, payload, created_at, updated_at)
         VALUES ($1, $2, $3, 'request_item_verdicts', 'pending', 'none', 'anyone', 'anyone', $4, NOW(), NOW())",
    )
    .bind(interaction_id)
    .bind(company_id)
    .bind(issue.id)
    .bind(serde_json::json!({ "items": [{ "id": "one" }, { "id": "two" }] }))
    .execute(&pool)
    .await
    .expect("insert verdict interaction");

    let service = IssueThreadInteractionService::new(pool.clone());
    let partial = service
        .submit_item_verdicts(
            issue.id,
            interaction_id,
            SubmitItemVerdictsInput {
                verdicts: vec![ItemVerdict { item_id: "one".to_string(), verdict: "approve".to_string(), reason: None }],
                summary_markdown: None,
            },
            user_resolver(),
        )
        .await
        .expect("submit partial verdict");
    assert_eq!(partial.status, "pending");
    assert_eq!(partial.result.as_ref().and_then(|v| v.get("items")).and_then(|v| v.as_array()).map(Vec::len), Some(1));

    let complete = service
        .submit_item_verdicts(
            issue.id,
            interaction_id,
            SubmitItemVerdictsInput {
                verdicts: vec![ItemVerdict { item_id: "two".to_string(), verdict: "reject".to_string(), reason: Some("needs work".to_string()) }],
                summary_markdown: Some("Reviewed".to_string()),
            },
            user_resolver(),
        )
        .await
        .expect("submit completing verdict");
    assert_eq!(complete.status, "answered");
    assert_eq!(complete.resolved_by_user_id.as_deref(), Some("board-test-user"));
    assert_eq!(complete.result.as_ref().and_then(|v| v.get("items")).and_then(|v| v.as_array()).map(Vec::len), Some(2));

    let replay = service
        .submit_item_verdicts(
            issue.id,
            interaction_id,
            SubmitItemVerdictsInput {
                verdicts: vec![ItemVerdict { item_id: "one".to_string(), verdict: "approve".to_string(), reason: None }],
                summary_markdown: None,
            },
            user_resolver(),
        )
        .await
        .expect("replay completed verdict");
    assert_eq!(replay.id, complete.id);
    assert_eq!(replay.status, "answered");
    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn heartbeat_wakeup_inserts_request_after_creating_run(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, 'idle', 'process')")
        .bind(agent_id)
        .bind(company_id)
        .bind("Wakeup test agent")
        .execute(&pool)
        .await
        .expect("insert agent");

    let heartbeat = DefaultHeartbeatService::new(pool.clone());
    heartbeat
        .wakeup_with_options(
            agent_id,
            issue.id,
            company_id,
            HeartbeatWakeupOptions {
                source: Some("mention".to_string()),
                reason: Some("issue_comment_mentioned".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("enqueue wakeup");

    let wakeup_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests WHERE company_id = $1 AND agent_id = $2",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .expect("count wakeups");
    assert_eq!(wakeup_count, 1);

    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2 AND context_snapshot->>'issueId' = $3",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(issue.id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count heartbeat runs");
    assert_eq!(run_count, 1);
    cleanup(&pool, company_id).await;
}

#[sqlx::test]
async fn agent_resolution_requires_and_persists_run_attribution(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, issue) = seed_company_and_issue(&pool).await;
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let interaction_id = Uuid::new_v4();

    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, 'idle', 'process')")
        .bind(agent_id)
        .bind(company_id)
        .bind("Resolver test agent")
        .execute(&pool)
        .await
        .expect("insert resolver agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue.id }))
    .execute(&pool)
    .await
    .expect("insert resolver run");
    sqlx::query(
        "INSERT INTO issue_thread_interactions
         (id, company_id, issue_id, kind, status, continuation_policy,
          requested_resolver_policy, effective_resolver_policy, created_by_agent_id,
          source_run_id, payload, created_at, updated_at)
         VALUES ($1, $2, $3, 'request_confirmation', 'pending', 'none', 'anyone', 'anyone', $4, $5, '{}', NOW(), NOW())",
    )
    .bind(interaction_id)
    .bind(company_id)
    .bind(issue.id)
    .bind(agent_id)
    .bind(run_id)
    .execute(&pool)
    .await
    .expect("insert attributed interaction");

    let service = IssueThreadInteractionService::new(pool.clone());
    let missing_run = service
        .reject_interaction(
            &issue,
            interaction_id,
            RejectThreadInteractionInput { reason: None, response: None },
            InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: None,
            },
        )
        .await
        .expect_err("agent without run must be rejected");
    assert!(missing_run.contains("run is required"));

    let resolved = service
        .reject_interaction(
            &issue,
            interaction_id,
            RejectThreadInteractionInput { reason: None, response: None },
            InteractionResolver {
                resolver_type: "agent".to_string(),
                resolver_id: agent_id.to_string(),
                run_id: Some(run_id),
            },
        )
        .await
        .expect("agent with run resolves interaction");
    assert_eq!(resolved.resolved_by_agent_id, Some(agent_id));
    assert_eq!(resolved.resolved_by_run_id, Some(run_id));
    cleanup(&pool, company_id).await;
}
