use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());
    
    println!("连接到数据库: {}", database_url);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("删除旧表...");
    sqlx::query("DROP TABLE IF EXISTS plugin_managed_resources CASCADE")
        .execute(&pool)
        .await?;
    
    sqlx::query("DROP TABLE IF EXISTS instruction_templates CASCADE")
        .execute(&pool)
        .await?;

    println!("创建 plugin_managed_resources 表...");
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

    println!("创建 plugin_managed_resources 索引...");
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_plugin_id ON plugin_managed_resources(plugin_id)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_resource_type ON plugin_managed_resources(resource_type)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_created_at ON plugin_managed_resources(created_at DESC)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_plugin_managed_resources_plugin_type ON plugin_managed_resources(plugin_id, resource_type)")
        .execute(&pool).await?;

    println!("创建 instruction_templates 表...");
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

    println!("创建 instruction_templates 索引...");
    sqlx::query("CREATE INDEX idx_instruction_templates_name ON instruction_templates(name)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_instruction_templates_created_at ON instruction_templates(created_at DESC)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX idx_instruction_templates_version ON instruction_templates(version)")
        .execute(&pool).await?;

    println!("更新迁移记录...");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (20260815000001, 20260815000002)")
        .execute(&pool).await.ok();
    
    sqlx::query(r#"
        INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
        VALUES 
            (20260815000001, 'create_plugin_managed_resources', NOW(), true, decode('00000000000000000000000000000000', 'hex'), 0),
            (20260815000002, 'create_instruction_templates', NOW(), true, decode('00000000000000000000000000000000', 'hex'), 0)
        ON CONFLICT (version) DO NOTHING
    "#)
    .execute(&pool)
    .await?;

    println!("✅ 数据库表创建成功！");
    Ok(())
}
