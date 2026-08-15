use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    println!("============================================");
    println!("修复迁移校验和");
    println!("============================================\n");
    println!("连接到: {}\n", database_url);
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("✅ 数据库连接成功\n");
    
    // 删除旧的错误记录
    println!("删除旧的迁移记录...");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (20260815000001, 20260815000002)")
        .execute(&pool)
        .await?;
    println!("✅ 已删除旧记录\n");
    
    // 插入正确的迁移记录（使用实际的校验和）
    println!("插入正确的迁移记录...");
    sqlx::query(r#"
        INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
        VALUES 
            (20260815000001, 'create_plugin_managed_resources', NOW(), true, 
             decode('5128985e5d477d9a4703fdb41c05d1d6a8b526687c25b0b4b96daaf2f5fc307a785b7d4bcf533257164c0253fec0d351', 'hex'), 0),
            (20260815000002, 'create_instruction_templates', NOW(), true, 
             decode('5c112433ee1df18948a1d32e5b9a8bc251f6d1c9b1def9176f8e459e93c89f94616f8f548394e77260f8df6b5448da1d', 'hex'), 0)
        ON CONFLICT (version) DO NOTHING
    "#)
    .execute(&pool)
    .await?;
    println!("✅ 迁移记录已更新\n");
    
    // 验证
    println!("验证迁移状态...");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE version IN (20260815000001, 20260815000002)"
    )
    .fetch_one(&pool)
    .await?;
    
    if count == 2 {
        println!("✅ 验证成功: 找到 {} 条迁移记录\n", count);
    } else {
        println!("⚠️  警告: 只找到 {} 条迁移记录\n", count);
    }
    
    println!("============================================");
    println!("✅ 迁移校验和修复完成！");
    println!("============================================");
    
    Ok(())
}
