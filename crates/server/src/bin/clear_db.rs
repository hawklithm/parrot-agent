use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());

    println!("🗑️  连接到数据库并清空所有数据...\n");
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("清空表数据（保留表结构）:\n");
    
    // 按依赖顺序删除数据（先删除有外键的表）
    let tables = vec![
        "run_logs",
        "run_approvals", 
        "runs",
        "task_documents",
        "task_messages",
        "tasks",
        "issues",
        "projects",
        "project_memberships",
        "agent_memberships",
        "agents",
        "goals",
        "invites",
        "companies",
        "auth_users",
        "skills",
        "adapter_environment_tests",
    ];

    for table in tables {
        print!("  清空 {} ... ", table);
        let result = sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", table))
            .execute(&pool)
            .await;
        
        match result {
            Ok(_) => println!("✅"),
            Err(e) => println!("❌ ({})", e),
        }
    }

    println!("\n✅ 数据库清空完成！\n");

    Ok(())
}
