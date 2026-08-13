use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    println!("🔗 连接到数据库...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    println!("🔍 删除不存在文件的migration记录 (>= 20260811000000)...");
    let result = sqlx::query("DELETE FROM _sqlx_migrations WHERE version >= 20260811000000")
        .execute(&pool)
        .await?;
    println!("✅ 删除了 {} 条记录", result.rows_affected());
    
    println!("\n📋 当前最新的5条migrations:");
    let rows = sqlx::query("SELECT version, description, success FROM _sqlx_migrations ORDER BY version DESC LIMIT 5")
        .fetch_all(&pool)
        .await?;
    for row in &rows {
        let version: i64 = row.get(0);
        let desc: String = row.get(1);
        let success: bool = row.get(2);
        println!("  {} - {} ({})", version, desc, if success { "✅" } else { "❌" });
    }
    
    Ok(())
}
