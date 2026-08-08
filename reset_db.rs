
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection, Executor};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接到 postgres 默认数据库
    let mut conn = PgConnection::connect("postgres://postgres:admin123@localhost:5432/postgres").await?;
    
    // 强制断开所有连接
    sqlx::query(r#"
        SELECT pg_terminate_backend(pg_stat_activity.pid)
        FROM pg_stat_activity
        WHERE pg_stat_activity.datname = 'parrot_agent_dev'
        AND pid <> pg_backend_pid()
    "#)
    .execute(&mut conn)
    .await?;
    
    // 删除数据库
    sqlx::query("DROP DATABASE IF EXISTS parrot_agent_dev")
        .execute(&mut conn)
        .await?;
    
    println!("✅ 数据库已删除");
    
    // 创建新数据库
    sqlx::query("CREATE DATABASE parrot_agent_dev")
        .execute(&mut conn)
        .await?;
    
    println!("✅ 数据库已创建");
    
    Ok(())
}
