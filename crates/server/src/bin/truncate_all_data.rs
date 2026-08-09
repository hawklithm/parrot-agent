use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());

    println!("🔗 连接到数据库...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("🔧 禁用外键约束检查...");
    sqlx::query("SET session_replication_role = 'replica'")
        .execute(&pool)
        .await?;

    println!("🗑️  清空所有表数据...");
    
    // 按正确的顺序删除数据（从依赖表到主表）
    let tables = vec![
        "run_logs",
        "runs",
        "routine_runs",
        "routines",
        "tasks",
        "approval_comments",
        "approvals",
        "issue_interactions",
        "issue_plan_decompositions",
        "issue_document_annotations",
        "issue_relations",
        "issues",
        "goal_artifacts",
        "project_goals",
        "goals",
        "project_memberships",
        "agent_memberships",
        "project_workspaces",
        "projects",
        "heartbeats",
        "agent_state",
        "agents",
        "budget_transactions",
        "budget_snapshots",
        "resources",
        "plugin_managed_resources",
        "invites",
        "join_requests",
        "company_join_requests",
        "board_api_keys",
        "boards",
        "auth_users",
        "activity_logs",
        "companies",
    ];

    for table in &tables {
        print!("  - 清空 {} ... ", table);
        match sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", table))
            .execute(&pool)
            .await
        {
            Ok(_) => println!("✅"),
            Err(e) => println!("⚠️  ({})", e),
        }
    }

    println!("🔧 重新启用外键约束...");
    sqlx::query("SET session_replication_role = 'origin'")
        .execute(&pool)
        .await?;

    println!("\n✅ 所有数据已清空！");

    Ok(())
}
