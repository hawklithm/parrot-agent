//! Test user_directory SQL queries to diagnose the enum type issue

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从环境变量读取数据库URL
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());

    println!("=== PostgreSQL 枚举类型测试 ===\n");
    println!("连接到数据库: {}\n", database_url);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("✅ 数据库连接成功\n");

    // 1. 查看枚举类型定义
    println!("1. 查看 principal_type 枚举定义:");
    let rows = sqlx::query("SELECT enumlabel FROM pg_enum JOIN pg_type ON pg_enum.enumtypid = pg_type.oid WHERE pg_type.typname = 'principal_type' ORDER BY enumsortorder")
        .fetch_all(&pool)
        .await?;
    for row in rows {
        let label: String = row.get("enumlabel");
        println!("   - {}", label);
    }
    println!();

    println!("2. 查看 company_membership_status 枚举定义:");
    let rows = sqlx::query("SELECT enumlabel FROM pg_enum JOIN pg_type ON pg_enum.enumtypid = pg_type.oid WHERE pg_type.typname = 'company_membership_status' ORDER BY enumsortorder")
        .fetch_all(&pool)
        .await?;
    for row in rows {
        let label: String = row.get("enumlabel");
        println!("   - {}", label);
    }
    println!();

    // 3. 获取第一个company_id
    println!("3. 获取测试用的 company_id:");
    let company_row = sqlx::query("SELECT id, name FROM companies LIMIT 1")
        .fetch_one(&pool)
        .await?;
    let company_id: uuid::Uuid = company_row.get("id");
    let company_name: String = company_row.get("name");
    println!("   Company: {} ({})\n", company_name, company_id);

    // 4. 测试原始SQL（不带类型转换）
    println!("4. 测试原始SQL（不带类型转换）:");
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM company_memberships m JOIN auth_users u ON u.id=m.principal_id WHERE m.company_id=$1 AND m.principal_type='user' AND m.status='active'"
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await;

    match result {
        Ok(count) => println!("   ✅ 成功! Count = {}\n", count),
        Err(e) => println!("   ❌ 失败: {}\n", e),
    }

    // 5. 测试修复后的SQL（带类型转换）
    println!("5. 测试修复后的SQL（带类型转换）:");
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM company_memberships m JOIN auth_users u ON u.id=m.principal_id WHERE m.company_id=$1 AND m.principal_type='user'::principal_type AND m.status='active'::company_membership_status"
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await;

    match result {
        Ok(count) => println!("   ✅ 成功! Count = {}\n", count),
        Err(e) => println!("   ❌ 失败: {}\n", e),
    }

    // 6. 查看实际数据
    println!("6. 查看 company_memberships 实际数据:");
    let rows = sqlx::query(
        "SELECT m.id, m.company_id, m.principal_type, m.principal_id, m.status, u.email, u.name \
         FROM company_memberships m \
         JOIN auth_users u ON u.id=m.principal_id \
         WHERE m.company_id=$1 \
         LIMIT 5"
    )
    .bind(company_id)
    .fetch_all(&pool)
    .await?;

    for row in rows {
        let principal_type: String = row.get("principal_type");
        let status: String = row.get("status");
        let email: String = row.get("email");
        println!("   - Email: {}, Type: {}, Status: {}", email, principal_type, status);
    }
    println!();

    // 7. 测试完整的SELECT查询
    println!("7. 测试完整的SELECT查询（修复后）:");
    let result = sqlx::query(
        "SELECT m.principal_id, m.status::text, u.email, u.name, u.avatar_url \
         FROM company_memberships m \
         JOIN auth_users u ON u.id=m.principal_id \
         WHERE m.company_id=$1 \
         AND m.principal_type='user'::principal_type \
         AND m.status='active'::company_membership_status \
         ORDER BY COALESCE(u.name,u.email), u.id \
         LIMIT 50"
    )
    .bind(company_id)
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            println!("   ✅ 成功! 返回 {} 行", rows.len());
            for row in rows.iter().take(3) {
                let email: String = row.get("email");
                let name: Option<String> = row.get("name");
                println!("   - {}: {:?}", email, name);
            }
        }
        Err(e) => println!("   ❌ 失败: {}", e),
    }

    println!("\n=== 测试完成 ===");
    Ok(())
}
