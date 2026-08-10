use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    // 检查表是否存在
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'instance_settings')"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("Table instance_settings exists: {}", table_exists);
    
    if table_exists {
        // 尝试查询数据
        let row = sqlx::query("SELECT * FROM instance_settings WHERE id = 1")
            .fetch_optional(&pool)
            .await?;
        
        if let Some(r) = row {
            println!("Row found in instance_settings");
        } else {
            println!("No row with id=1 in instance_settings");
        }
    }
    
    Ok(())
}
