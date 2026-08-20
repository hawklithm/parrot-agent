use services::workspace_instance_cleanup_service::{
    CleanupStatus, CleanupType, WorkspaceInstanceCleanupService,
};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
async fn cleanup_task_marks_workspace_inactive_and_is_idempotent() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping workspace cleanup integration test: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_id)
    .bind("workspace cleanup test company")
    .bind(format!("T{}", &company_id.simple().to_string()[..5]))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, company_id, name, status, metadata) VALUES ($1, $2, $3, 'active', '{}')",
    )
    .bind(workspace_id)
    .bind(company_id)
    .bind("cleanup test workspace")
    .execute(&pool)
    .await
    .unwrap();

    let service = WorkspaceInstanceCleanupService::new(pool.clone());
    let task_id = service
        .schedule_cleanup(workspace_id, CleanupType::FullCleanup)
        .await
        .unwrap();
    service.execute_cleanup(task_id).await.unwrap();
    service.execute_cleanup(task_id).await.unwrap();

    let (status,): (String,) = sqlx::query_as(
        "SELECT status FROM workspace_cleanup_tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (workspace_status, metadata): (String, serde_json::Value) = sqlx::query_as(
        "SELECT status, metadata FROM workspaces WHERE id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, format!("{:?}", CleanupStatus::Completed));
    assert_eq!(workspace_status, "inactive");
    assert_eq!(metadata["cleanupTaskId"], task_id.to_string());

    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
}
