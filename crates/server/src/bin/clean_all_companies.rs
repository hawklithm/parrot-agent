use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    println!("🔗 连接到数据库...");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    
    println!("🔧 删除所有公司...");
    let result = sqlx::query("DELETE FROM companies")
        .execute(&pool)
        .await?;
    
    println!("✅ 成功删除 {} 个公司", result.rows_affected());
    
    Ok(())
}
