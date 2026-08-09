//! Database cleanup utility

use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    println!("🔌 Connecting to database...");
    let pool = PgPool::connect(&database_url).await?;
    
    println!("🗑️  Dropping all tables...");
    
    let sql = include_str!("../../../../clear_db.sql");
    
    for statement in sql.split(';') {
        let stmt = statement.trim();
        if !stmt.is_empty() && !stmt.starts_with("--") {
            match sqlx::raw_sql(stmt).execute(&pool).await {
                Ok(_) => {},
                Err(e) => eprintln!("Warning: {}", e),
            }
        }
    }
    
    println!("✅ Database cleared successfully!");
    println!("💡 Run migrations to recreate tables: cargo run");
    
    Ok(())
}
