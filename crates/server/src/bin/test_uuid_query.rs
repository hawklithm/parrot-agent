use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());

    println!("=== Test JOIN with principal_id::uuid cast ===");

    let pool = PgPoolOptions::new().max_connections(5).connect(&database_url).await?;
    let company_row = sqlx::query("SELECT id FROM companies LIMIT 1").fetch_one(&pool).await?;
    let company_id: uuid::Uuid = company_row.get("id");

    println!("\nTest: JOIN with m.principal_id::uuid");
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM company_memberships m JOIN auth_users u ON u.id = m.principal_id::uuid WHERE m.company_id = $1 AND m.principal_type = 'user'::principal_type AND m.status = 'active'::company_membership_status"
    ).bind(company_id).fetch_one(&pool).await;
    
    match result {
        Ok(count) => {
            println!("   SUCCESS! Count = {}", count);
            println!("\n>>> FIX CONFIRMED: m.principal_id::uuid solves the problem!");
        }
        Err(e) => println!("   FAIL: {}", e),
    }

    Ok(())
}
