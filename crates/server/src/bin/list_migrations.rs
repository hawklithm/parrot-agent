use sqlx::Row;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect("postgres://postgres:admin123@localhost:5432/parrot_agent_dev")
        .await?;
    
    let rows = sqlx::query("SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY version DESC LIMIT 10")
        .fetch_all(&pool)
        .await?;
    
    println!("最近10个migrations:");
    for row in rows {
        let version: i64 = row.try_get(0)?;
        let desc: String = row.try_get(1)?;
        println!("  {} - {}", version, desc);
    }
    
    Ok(())
}
