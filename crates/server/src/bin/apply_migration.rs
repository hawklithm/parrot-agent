use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    println!("============================================");
    println!("数据库迁移执行");
    println!("============================================\n");
    println!("连接到: {}\n", database_url);
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("✅ 数据库连接成功\n");
    
    // 步骤 1: 删除旧表
    println!("步骤 1/4: 删除旧表...");
    sqlx::query("DROP TABLE IF EXISTS plugin_managed_resources CASCADE")
        .execute(&pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS instruction_templates CASCADE")
        .execute(&pool)
        .await?;
    println!("✅ 旧表已删除\n");
    
    // 步骤 2: 创建 plugin_managed_resources 表
    println!("步骤 2/4: 创建 plugin_managed_resources 表...");
    sqlx::query(r#"
        CREATE TABLE plugin_managed_resources (
            id UUID PRIMARY KEY,
            plugin_id UUID NOT NULL,
            resource_type VARCHAR(50) NOT NULL,
            resource_id UUID NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
    "#)
    .execute(&pool)
    .await?;
    
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_plugin_id ON plugin_managed_resources(plugin_id)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_resource_type ON plugin_managed_resources(resource_type)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_created_at ON plugin_managed_resources(created_at DESC)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_plugin_type ON plugin_managed_resources(plugin_id, resource_type)")
        .execute(&pool).await?;
    println!("✅ plugin_managed_resources 表和索引创建完成\n");
    
    // 步骤 3: 创建 instruction_templates 表
    println!("步骤 3/4: 创建 instruction_templates 表...");
    sqlx::query(r#"
        CREATE TABLE instruction_templates (
            id UUID PRIMARY KEY,
            name VARCHAR(255) NOT NULL UNIQUE,
            content TEXT NOT NULL,
            variables TEXT[] NOT NULL DEFAULT '{}',
            version INTEGER NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ
        )
    "#)
    .execute(&pool)
    .await?;
    
    sqlx::query("CREATE INDEX idx_instruction_templates_name ON instruction_templates(name)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_instruction_templates_created_at ON instruction_templates(created_C)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_instruction_templates_version ON instruction_templates(version)")
        .execute(&pool).await?;
    println!("✅ instruction_templates 表和索引创建完成\n");
    
    // 步骤 4: 更新迁移记录
    println!("步骤 4/4: 更新迁移记录...");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (20260815000001, 20260815000002)")
        .execute(&pool).await.ok();
    
    sqlx::query(r#"
        INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
        VALUES 
            (20260815000001,ugin_managed_resources', NOW(), true, decode('00000000000000000000000000000000', 'hex'), 0),
            (20260815000002, 'create_instruction_templates', NOW(), true, decode('00000000000000000000000000000000', 'hex'), 0)
        ON CONFLICT (version) DO NOTHING
    "#)
    .execute(&pool)
    .await?;
    println!("✅ 迁移记录已更新\n");
    
    println!("============================================");
    println!("✅ 数据库迁移成功完成！");
    println!("============================================");
    
    Ok(())
}
