use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;
    
    println!("连接数据库成功！");
    
    // 修复Agent名称
    let result1 = sqlx::query!(
        "UPDATE agents SET name = $1 WHERE id = $2",
        "Engineer",
        uuid::Uuid::parse_str("5ea3d1aa-1687-4aec-a34b-f86b3cdd31f1")?
    )
    .execute(&pool)
    .await?;
    println!("✅ 修复Agent名称: {} 行受影响", result1.rows_affected());
    
    // 修复reports_to关系
    let ceo_id = uuid::Uuid::parse_str("f8f42923-300d-4f41-b468-7eee87eccf31")?;
    let result2 = sqlx::query!(
        "UPDATE agents SET reports_to = $1 WHERE id = ANY($2)",
        ceo_id,
        &[
            uuid::Uuid::parse_str("021718e6-3502-495e-9e22-72244d4a4bcb")?,
            uuid::Uuid::parse_str("5ea3d1aa-1687-4aec-a34b-f86b3cdd31f1")?
        ]
    )
    .execute(&pool)
    .await?;
    println!("✅ 修复reports_to关系: {} 行受影响", result2.rows_affected());
    
    // 验证结果
    let agents = sqlx::query!(
        "SELECT id, name, role, reports_to FROM agents WHERE company_id = (SELECT id FROM companies WHERE issue_prefix = 'TES') ORDER BY created_at"
    )
    .fetch_all(&pool)
    .await?;
    
    println!("\n验证结果:");
    for agent in agents {
        println!("  {} | {} | {} | {:?}", 
            agent.id, agent.name, agent.role, agent.reports_to);
    }
    
    Ok(())
}
