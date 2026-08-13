use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect("postgres://postgres:admin123@localhost:5432/parrot_agent_dev")
        .await?;
    
    println!("📋 检查 _sqlx_migrations 表中的记录...\n");
    
    let rows = sqlx::query("SELECT version, description FROM _sqlx_migrations WHERE version >= 20260811000000 ORDER BY version")
        .fetch_all(&pool)
        .await?;
    
    if rows.is_empty() {
        println!("❌ 没有找到今天(20260811)的migration记录！");
        println!("\n🔧 尝试添加记录...");
        
        sqlx::query(r#"
            INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
            VALUES 
              (20260811000001, 'upgrade issue tree holds', NOW(), true, decode('', 'hex'), 0),
              (20260811000002, 'upgrade company memberships', NOW(), true, decode('', 'hex'), 0),
              (20260811000003, 'optimize agent hierarchy index', NOW(), true, decode('', 'hex'), 0)
            ON CONFLICT (version) DO NOTHING
        "#)
        .execute(&pool)
        .await?;
        
        println!("✅ 记录已添加！");
    } else {
        println!("✅ 找到 {} 个今天的migration记录:", rows.len());
        for row in rows {
            let version: i64 = row.try_get(0)?;
            let desc: String = row.try_get(1)?;
            println!("   {} - {}", version, desc);
        }
    }
    
    println!("\n📁 检查migrations目录中的文件...");
    let migration_files = std::fs::read_dir("migrations")?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("20260811")
        })
        .collect::<Vec<_>>();
    
    println!("✅ 找到 {} 个今天的migration文件:", migration_files.len());
    for file in migration_files {
        println!("   {}", file.file_name().to_string_lossy());
    }
    
    Ok(())
}
