use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;

    println!("查询 Run bdbbb599:");
    let rows = sqlx::query("SELECT substring(id::text, 1, 8) as run_id, substring(issue_id::text, 1, 8) as issue_id, status FROM agent_runs WHERE id::text LIKE 'bdbbb599%'")
        .fetch_all(&pool).await?;
    for row in rows {
        let run_id: String = row.try_get(0)?;
        let issue_id: Option<String> = row.try_get(1).ok();
        let status: String = row.try_get(2)?;
        println!("  Run: {} → Issue: {:?}, Status: {}", run_id, issue_id, status);
    }

    println!("\n查询 Run efcfeaf2:");
    let rows = sqlx::query("SELECT substring(id::text, 1, 8) as run_id, substring(issue_id::text, 1, 8) as issue_id, status FROM agent_runs WHERE id::text LIKE 'efcfeaf2%'")
        .fetch_all(&pool).await?;
    for row in rows {
        let run_id: String = row.try_get(0)?;
        let issue_id: Option<String> = row.try_get(1).ok();
        let status: String = row.try_get(2)?;
        println!("  Run: {} → Issue: {:?}, Status: {}", run_id, issue_id, status);
    }

    println!("\n查询关联到 ec3365f4 的所有 Runs:");
    let rows = sqlx::query("SELECT substring(id::text, 1, 8) as run_id, status, created_at FROM agent_runs WHERE issue_id::text LIKE 'ec3365f4%' ORDER BY created_at")
        .fetch_all(&pool).await?;
    
    if rows.is_empty() {
        println!("  (无)");
    } else {
        for row in rows {
            let run_id: String = row.try_get(0)?;
            let status: String = row.try_get(1)?;
            println!("  Run: {}, Status: {}", run_id, status);
        }
    }

    Ok(())
}
