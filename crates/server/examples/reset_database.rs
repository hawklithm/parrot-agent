use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, Executor, PgConnection};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 开始重置数据库...");
    
    // 连接到 postgres 默认数据库（不是 parrot_agent_dev）
    let postgres_url = "postgres://postgres:admin123@localhost:5432/postgres";
    let mut conn = PgConnection::connect(postgres_url).await?;
    
    println!("✅ 已连接到 PostgreSQL");
    
    // 1. 强制断开所有到目标数据库的连接
    println!("🔌 断开所有现有连接...");
    let _ = sqlx::query(
        r#"
        SELECT pg_terminate_backend(pg_stat_activity.pid)
        FROM pg_stat_activity
        WHERE pg_stat_activity.datname = 'parrot_agent_dev'
        AND pid <> pg_backend_pid()
        "#,
    )
    .execute(&mut conn)
    .await;
    
    // 2. 删除现有数据库
    println!("🗑️  删除现有数据库...");
    sqlx::query("DROP DATABASE IF EXISTS parrot_agent_dev")
        .execute(&mut conn)
        .await?;
    
    println!("✅ 数据库已删除");
    
    // 3. 创建新数据库
    println!("🆕 创建新数据库...");
    sqlx::query("CREATE DATABASE parrot_agent_dev")
        .execute(&mut conn)
        .await?;
    
    println!("✅ 数据库已创建");
    println!("🎉 数据库重置完成！");
    println!("\n下一步: 运行 cargo run --bin parrot-server 来执行迁移");
    
    Ok(())
}
