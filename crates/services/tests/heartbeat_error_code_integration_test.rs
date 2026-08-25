//! Integration test: heartbeat_runs.error_code / error_family persistence (§4B.2 Stop Metadata).
//!
//! Verifies the migration 58 columns exist and are written by the same finalize
//! UPDATE `execute_run` issues on run completion, and that the dashboard
//! runActivity query groups failures by error_code. Skips when no live DB.

use sqlx::PgPool;
use uuid::Uuid;

async fn connect() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    match PgPool::connect(&database_url).await {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("Skipping heartbeat_error_code test: no DATABASE_URL reachable");
            None
        }
    }
}

#[tokio::test]
async fn heartbeat_run_persists_error_code_and_family() {
    let Some(pool) = connect().await else {
        return;
    };

    // 隔离用唯一公司/agent/issue
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("T{}", &company_id.simple().to_string()[..6]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'HB Test Co', $2)")
        .bind(company_id)
        .bind(&issue_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'HB Test Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("insert agent");
    let issue_id = Uuid::new_v4();

    // 插入一条 running 运行（模拟 execute_run 中途状态）
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_runs (company_id, agent_id, invocation_source, status, context_snapshot)
         VALUES ($1, $2, 'on_demand', 'running', $3::jsonb) RETURNING id",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(serde_json::json!({ "issueId": issue_id.to_string() }))
    .fetch_one(&pool)
    .await
    .expect("insert running run");

    // 复刻 execute_run 完成时的 finalize UPDATE（含 error_code / error_family）
    sqlx::query(
        "UPDATE heartbeat_runs
         SET status = 'failed'::heartbeat_run_status, exit_code = $2, error = $3,
             output = $4, result_json = $5, error_code = $6, error_family = $7,
             finished_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status IN ('queued','running')",
    )
    .bind(run_id)
    .bind(1i32)
    .bind("boom")
    .bind("stdout")
    .bind(serde_json::json!({ "errorCode": "adapter_failed" }))
    .bind("adapter_failed")
    .bind("adapter")
    .execute(&pool)
    .await
    .expect("finalize run");

    // 断言 error_code / error_family 已落库
    let row: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT error_code, error_family, status::text FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read back run");

    assert_eq!(row.0.as_deref(), Some("adapter_failed"));
    assert_eq!(row.1.as_deref(), Some("adapter"));
    assert_eq!(row.2, "failed");

    // 断言 dashboard 的 failedByErrorCode 分组查询能读到该错误码
    let grouped: (String, i64) = sqlx::query_as(
        "SELECT COALESCE(error_code, 'unknown'), COUNT(*)::bigint
         FROM heartbeat_runs
         WHERE company_id = $1 AND status IN ('failed','timed_out')
         GROUP BY error_code",
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("grouped by error_code");

    assert_eq!(grouped.0, "adapter_failed");
    assert_eq!(grouped.1, 1);

    // 清理
    sqlx::query("DELETE FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .execute(&pool)
        .await
        .ok();
}
