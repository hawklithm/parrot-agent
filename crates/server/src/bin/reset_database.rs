use sqlx::PgPool;
use std::env;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载 .env
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment or .env");
    
    println!("========================================");
    println!("Parrot Agent Database Reset Tool");
    println!("========================================");
    println!("DATABASE_URL: {}", database_url);
    println!();
    
    println!("Connecting to database...");
    let pool = PgPool::connect(&database_url).await?;
    
    println!("Executing reset script...");
    println!();
    
    // 1. 清空所有表(使用 CASCADE 自动处理依赖)
    println!("[1/4] Truncating tables...");
    sqlx::query("TRUNCATE TABLE companies CASCADE")
        .execute(&pool)
        .await?;
    
    // 2. 插入默认公司
    println!("[2/4] Inserting default company...");
    sqlx::query(
        "INSERT INTO companies (
            id, 
            name,
            issue_prefix,
            require_board_approval_for_new_agents,
            created_at,
            updated_at
        ) VALUES ($1, $2, $3, $4, NOW(), NOW())
        ON CONFLICT (id) DO NOTHING"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000")?)
    .bind("Default Company")
    .bind("CMP")
    .bind(false)
    .execute(&pool)
    .await?;
    
    // 3. 插入默认 Board 用户
    println!("[3/4] Inserting board user...");
    sqlx::query(
        "INSERT INTO auth_users (
            id,
            email,
            name,
            created_at,
            updated_at
        ) VALUES ($1, $2, $3, NOW(), NOW())
        ON CONFLICT (id) DO NOTHING"
    )
    .bind(Uuid::parse_str("48592512-465a-4ed7-9b12-ca554ee636e8")?)
    .bind("board@local.dev")
    .bind("Local Board User")
    .execute(&pool)
    .await?;
    
    // 4. 将 Board 用户添加到公司
    println!("[4/4] Adding board user to company...");
    sqlx::query(
        "INSERT INTO company_memberships (
            company_id,
            principal_type,
            principal_id,
            membership_role,
            status,
            created_at,
            updated_at
        ) VALUES ($1, $2::principal_type, $3, $4::membership_role, $5::company_membership_status, NOW(), NOW())
        ON CONFLICT (company_id, principal_type, principal_id) DO NOTHING"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000")?)
    .bind("user")  // principal_type enum
    .bind(Uuid::parse_str("48592512-465a-4ed7-9b12-ca554ee636e8")?)
    .bind("admin")  // membership_role enum
    .bind("active")  // status enum
    .execute(&pool)
    .await?;
    
    println!();
    println!("========================================");
    println!("Database reset completed successfully!");
    println!("========================================");
    println!();
    
    // 验证结果
    println!("Verifying reset...");
    
    let company_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM companies")
        .fetch_one(&pool)
        .await?;
    println!("  Companies: {}", company_count);
    
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_users")
        .fetch_one(&pool)
        .await?;
    println!("  Users: {}", user_count);
    
    let membership_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_memberships")
        .fetch_one(&pool)
        .await?;
    println!("  Memberships: {}", membership_count);
    
    let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool)
        .await?;
    println!("  Agents: {}", agent_count);
    
    let issue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues")
        .fetch_one(&pool)
        .await?;
    println!("  Issues: {}", issue_count);
    
    println!();
    println!("✅ Database is clean and ready for testing!");
    println!();
    println!("Default credentials:");
    println!("  Company ID: 00000000-0000-0000-0000-000000000000");
    println!("  User ID: 48592512-465a-4ed7-9b12-ca554ee636e8");
    println!("  Email: board@local.dev");
    
    Ok(())
}
