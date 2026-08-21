use chrono::Utc;
use models::routine::{
    CatchUpPolicy, ConcurrencyPolicy, Routine, RoutineStatus, RoutineTrigger, TriggerStatus,
    TriggerType,
};
use repositories::{
    routine_repository::PostgresRoutineRepository, PostgresRoutineTriggerRepository,
    RoutineRepository, RoutineTriggerRepository,
};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn routine_and_trigger_round_trip_uses_aligned_schema_contract() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping routine repository integration test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let routine_id = Uuid::new_v4();
    let prefix = format!("RT{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Routine repository integration company")
        .bind(prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_id)
        .bind(company_id)
        .bind("Routine repository integration agent")
        .execute(&pool)
        .await
        .expect("insert agent");

    let now = Utc::now();
    let routine = Routine {
        id: routine_id,
        company_id,
        project_id: None,
        goal_id: None,
        parent_issue_id: None,
        name: "builtin_weekly_brief".to_string(),
        title: "Built-in weekly brief".to_string(),
        description: Some("Creates the weekly brief".to_string()),
        agent_id,
        assignee_agent_id: agent_id,
        priority: 50,
        status: RoutineStatus::Paused,
        concurrency_policy: ConcurrencyPolicy::CoalesceIfActive,
        catch_up_policy: CatchUpPolicy::SkipMissed,
        trigger_config: serde_json::json!({"source": "built_in"}),
        variables: serde_json::json!([]),
        env: serde_json::json!({}),
        latest_revision_id: None,
        latest_revision_number: 0,
        responsible_user_id: None,
        created_by_user_id: None,
        last_run_at: None,
        next_run_at: None,
        run_count: 0,
        success_count: 0,
        failure_count: 0,
        last_triggered_at: None,
        last_enqueued_at: None,
        created_at: now,
        updated_at: now,
    };

    let routine_repo = PostgresRoutineRepository::new(pool.clone());
    let trigger_repo = PostgresRoutineTriggerRepository::new(pool.clone());
    routine_repo.create(routine).await.expect("create routine");

    let mut persisted = routine_repo
        .get(routine_id)
        .await
        .expect("get routine")
        .expect("routine exists");
    assert_eq!(persisted.name, "builtin_weekly_brief");
    assert_eq!(persisted.trigger_config["source"], "built_in");
    assert_eq!(persisted.status, RoutineStatus::Paused);

    persisted.name = "builtin_weekly_brief_v2".to_string();
    persisted.title = "Built-in weekly brief v2".to_string();
    persisted.status = RoutineStatus::Active;
    routine_repo.update(persisted).await.expect("update routine");
    let updated = routine_repo
        .get(routine_id)
        .await
        .expect("get updated routine")
        .expect("routine still exists");
    assert_eq!(updated.name, "builtin_weekly_brief_v2");
    assert_eq!(updated.title, "Built-in weekly brief v2");
    assert_eq!(updated.status, RoutineStatus::Active);

    let mut trigger = RoutineTrigger::new_schedule(
        company_id,
        routine_id,
        "0 9 * * 1".to_string(),
        Some("UTC".to_string()),
    );
    trigger.trigger_type = TriggerType::Cron;
    trigger.config = serde_json::json!({"cron_expression": "0 9 * * 1"});
    trigger.status = TriggerStatus::Paused;
    trigger.enabled = false;
    trigger_repo.create(trigger.clone()).await.expect("create trigger");

    let fetched_trigger = trigger_repo
        .find_by_id(trigger.id)
        .await
        .expect("get trigger")
        .expect("trigger exists");
    assert_eq!(fetched_trigger.trigger_type, TriggerType::Cron);
    assert_eq!(fetched_trigger.status, TriggerStatus::Paused);
    assert_eq!(fetched_trigger.config["cron_expression"], "0 9 * * 1");

    routine_repo.delete(routine_id).await.expect("delete routine");
    assert!(trigger_repo
        .find_by_id(trigger.id)
        .await
        .expect("get deleted trigger")
        .is_none());

    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("delete company");
}
