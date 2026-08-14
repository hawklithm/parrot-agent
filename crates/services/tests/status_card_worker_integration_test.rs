//! Status Card / Summary Slot 后台任务链集成测试（真实 Postgres）。
//!
//! 覆盖：
//! 1. Summarizer 内置 agent 解析（builtInKey=summarizer）
//! 2. request_compile 创建 hidden issue 并 claim 卡片
//! 3. execute_queries 按 scope/status 过滤构建快照
//! 4. request_refresh 执行查询 -> fingerprint diff -> 创建 update 任务
//! 5. tick_due_status_cards 到期扫描 + 乐观锁 claim
//! 6. finalize_stalled_generations 终态释放占位

use services::status_card_worker::{StatusCardWorker, SUMMARIZER_BUILT_IN_KEY};
use sqlx::PgPool;
use uuid::Uuid;

async fn setup(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let prefix = format!("WT{}", Uuid::new_v4().simple().to_string()[..4].to_uppercase());
    sqlx::query(
        "INSERT INTO companies (id, name, status, issue_prefix) \
         VALUES ($1, 'Worker Test Co', 'active', $2)",
    )
    .bind(company_id)
    .bind(&prefix)
    .execute(pool)
    .await
    .unwrap();
    let summarizer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type, metadata) \
         VALUES ($1, $2, 'Summarizer', 'general', 'idle', 'process', \
                 jsonb_build_object('isBuiltIn', true, 'builtInKey', $3))",
    )
    .bind(summarizer_id)
    .bind(company_id)
    .bind(SUMMARIZER_BUILT_IN_KEY)
    .execute(pool)
    .await
    .unwrap();
    (company_id, summarizer_id)
}

async fn create_card(pool: &PgPool, company_id: Uuid) -> Uuid {
    let card_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO status_cards (id, company_id, title, interest_prompt, queries, refresh_policy, state) \
         VALUES ($1, $2, 'Watch main flow', 'watch the main flow', '[]'::jsonb, \
                 jsonb_build_object('mode', 'interval', 'intervalMinutes', 15, \
                                    'triggers', jsonb_build_object('anyUpdate', true)), 'compiling')",
    )
    .bind(card_id)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();
    card_id
}

#[tokio::test]
#[ignore] // 需要真实数据库环境（TEST_DATABASE_URL）
async fn test_status_card_async_chain_end_to_end() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_migrate_verify".to_string());
    let pool = PgPool::connect(&database_url).await.expect("connect db");
    sqlx::migrate!("../../migrations").run(&pool).await.expect("migrate");

    // 清理
    for table in ["status_card_update_runs", "status_card_summary_revisions", "status_cards",
                  "issues", "agents", "companies"] {
        sqlx::query(&format!("DELETE FROM {}", table)).execute(&pool).await.unwrap();
    }

    let (company_id, summarizer_id) = setup(&pool).await;
    let worker = StatusCardWorker::new(pool.clone());

    // 1. Summarizer 解析
    let resolved = worker.resolve_summarizer_agent_id(company_id, None).await.unwrap();
    assert_eq!(resolved, summarizer_id, "Summarizer built-in agent must resolve");

    // 2. request_compile -> hidden issue + claim
    let card_id = create_card(&pool, company_id).await;
    let compile = worker.request_compile(card_id, None, None).await.unwrap();
    assert!(!compile.already_generating);
    let issue_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM issues WHERE id = $1 AND hidden_at IS NOT NULL AND assignee_agent_id = $2",
    )
    .bind(compile.generating_issue_id)
    .bind(summarizer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(issue_count, 1, "hidden issue created and assigned to Summarizer");
    let state: String = sqlx::query_scalar("SELECT state FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert_eq!(state, "compiling", "card claimed with state=compiling");
    let claimed_gid: Option<Uuid> = sqlx::query_scalar("SELECT generating_issue_id FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert_eq!(claimed_gid, Some(compile.generating_issue_id));

    // 3. execute_queries：先造两条 issue，其中一条 in_progress
    let issue_a = Uuid::new_v4();
    let issue_b = Uuid::new_v4();
    let fp_a = format!("test:{}", issue_a);
    let fp_b = format!("test:{}", issue_b);
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority, origin_fingerprint) VALUES \
         ($1, $2, 'Alpha issue', 'in_progress', 'high', $4), \
         ($3, $2, 'Beta issue', 'done', 'low', $5)",
    )
    .bind(issue_a).bind(company_id).bind(issue_b)
    .bind(&fp_a).bind(&fp_b)
    .execute(&pool)
    .await
    .unwrap();
    let queries = serde_json::json!([{ "scope": "all", "status": "in_progress", "limit": 10 }]);
    let snapshot = worker.execute_queries(company_id, &queries).await.unwrap();
    assert_eq!(snapshot.len(), 1, "scope/status filter returns only in_progress");
    assert_eq!(snapshot[0]["title"], "Alpha issue");

    // 4. request_refresh：先写入编译好的 queries，再刷新 -> 应创建 update 任务
    sqlx::query("UPDATE status_cards SET queries = $2, state = 'active', generating_issue_id = NULL, \
                 fingerprint = '{}'::jsonb WHERE id = $1")
        .bind(card_id)
        .bind(serde_json::json!([{ "scope": "all", "status": "in_progress", "limit": 10 }]))
        .execute(&pool)
        .await
        .unwrap();
    let refresh = worker.request_refresh(card_id, false, "interval", None, None).await.unwrap();
    assert!(!refresh.already_generating, "refresh with changes must enqueue");
    assert_ne!(refresh.generating_issue_id, Uuid::nil());
    let gid_after: Option<Uuid> = sqlx::query_scalar("SELECT generating_issue_id FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert_eq!(gid_after, Some(refresh.generating_issue_id));
    let fp: serde_json::Value = sqlx::query_scalar("SELECT fingerprint FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert!(!fp.as_object().unwrap().is_empty(), "fingerprint persisted");

    // 5. tick_due_status_cards：设置 next_eval_at 到期 -> 扫描评估
    //    （卡片已有占位任务，tick 的 claim 条件 generating_issue_id IS NULL 不满足，
    //      不会重复入队；仅验证扫描路径可执行。）
    let now = chrono::Utc::now();
    sqlx::query("UPDATE status_cards SET next_eval_at = $2 WHERE id = $1")
        .bind(card_id)
        .bind(now - chrono::Duration::minutes(1))
        .execute(&pool)
        .await
        .unwrap();
    let (evaluated, _enqueued) = worker.tick_due_status_cards(&now).await.unwrap();
    // 占位任务仍在 -> tick 不重复入队，evaluated 可为 0（占位占用）。
    // 清空占位后再 tick，验证到期扫描 + claim 路径。
    sqlx::query("UPDATE status_cards SET generating_issue_id = NULL WHERE id = $1")
        .bind(card_id)
        .execute(&pool)
        .await
        .unwrap();
    let (evaluated2, enqueued2) = worker.tick_due_status_cards(&now).await.unwrap();
    assert!(evaluated2 >= 1, "due card must be evaluated after clearing claim");
    let _ = (evaluated, enqueued2);

    // 6. finalize_stalled_generations：卡片占位指向 refresh 任务（终态前占位仍在），
    //    置任务 cancelled 后 finalize 应释放占位。
    sqlx::query(
        "UPDATE status_cards SET generating_issue_id = $2 WHERE id = $1",
    )
    .bind(card_id)
    .bind(refresh.generating_issue_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE issues SET status = 'cancelled' WHERE id = $1")
        .bind(refresh.generating_issue_id)
        .execute(&pool)
        .await
        .unwrap();
    let finalized = worker.finalize_stalled_generations().await.unwrap();
    assert!(finalized >= 1, "cancelled generation must be finalized");
    let state2: String = sqlx::query_scalar("SELECT state FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert_eq!(state2, "error", "card flipped to error after stalled finalization");
    let gid_cleared: Option<Uuid> = sqlx::query_scalar("SELECT generating_issue_id FROM status_cards WHERE id = $1")
        .bind(card_id).fetch_one(&pool).await.unwrap();
    assert_eq!(gid_cleared, None, "generating_issue_id released");
}

#[tokio::test]
#[ignore] // 需要真实数据库环境（TEST_DATABASE_URL）
async fn test_summary_slot_generation_chain() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_migrate_verify".to_string());
    let pool = PgPool::connect(&database_url).await.expect("connect db");
    sqlx::migrate!("../../migrations").run(&pool).await.expect("migrate");

    for table in ["summary_slot_revisions", "summary_slots", "issues", "agents", "companies"] {
        sqlx::query(&format!("DELETE FROM {}", table)).execute(&pool).await.unwrap();
    }

    let (company_id, _) = setup(&pool).await;
    let worker = services::summary_slot_worker::SummarySlotWorker::new(pool.clone());
    let generation = worker
        .generate(company_id, "project", None, "weekly-status", None, None)
        .await
        .unwrap();
    assert!(!generation.already_generating);

    let slot_status: String = sqlx::query_scalar("SELECT status FROM summary_slots WHERE id = $1")
        .bind(generation.slot_id).fetch_one(&pool).await.unwrap();
    assert_eq!(slot_status, "generating");
    let slot_gid: Option<Uuid> = sqlx::query_scalar("SELECT generating_issue_id FROM summary_slots WHERE id = $1")
        .bind(generation.slot_id).fetch_one(&pool).await.unwrap();
    assert_eq!(slot_gid, Some(generation.generating_issue_id));

    // 幂等：再次 generate 返回 alreadyGenerating
    let again = worker.generate(company_id, "project", None, "weekly-status", None, None).await.unwrap();
    assert!(again.already_generating, "second generate must be idempotent");

    // 终态 finalize
    sqlx::query("UPDATE issues SET status = 'cancelled' WHERE id = $1")
        .bind(generation.generating_issue_id).execute(&pool).await.unwrap();
    let finalized = worker.finalize_terminal_issues().await.unwrap();
    assert!(finalized >= 1);
    let slot_status2: String = sqlx::query_scalar("SELECT status FROM summary_slots WHERE id = $1")
        .bind(generation.slot_id).fetch_one(&pool).await.unwrap();
    assert_eq!(slot_status2, "failed");
}
