use sqlx::{PgPool, Row};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let pool = PgPool::connect(&std::env::var("DATABASE_URL")?).await?;

    println!("🔍 诊断 case_id 索引问题\n");
    
    // 1. 查找所有包含 case_id 的索引
    println!("📋 查找所有包含 'case_id' 的索引:");
    let rows = sqlx::query(
        "SELECT tablename, indexname, indexdef 
         FROM pg_indexes 
         WHERE schemaname = 'public' 
           AND indexdef ILIKE '%case_id%'
         ORDER BY tablename, indexname"
    ).fetch_all(&pool).await?;
    
    if rows.is_empty() {
        println!("  ✅ 没有找到包含 case_id 的索引\n");
    } else {
        for row in &rows {
            let table: String = row.try_get(0)?;
            let index: String = row.try_get(1)?;
            let def: String = row.try_get(2)?;
            println!("  表: {}", table);
            println!("  索引: {}", index);
            println!("  定义: {}\n", def);
        }
    }
    
    // 2. 检查 case_documents, case_issue_links, case_labels 表的列
    for table_name in ["case_documents", "case_issue_links", "case_labels"] {
        println!("\n📋 {} 表的列:", table_name);
        let rows = sqlx::query(
            "SELECT column_name, data_type 
             FROM information_schema.columns 
             WHERE table_schema = 'public' 
               AND table_name = $1
             ORDER BY ordinal_position"
        )
        .bind(table_name)
        .fetch_all(&pool).await?;
        
        if rows.is_empty() {
            println!("  ❌ 表不存在！");
        } else {
            let mut has_case_id = false;
            for row in &rows {
                let col: String = row.try_get(0)?;
                let typ: String = row.try_get(1)?;
                println!("  {} ({})", col, typ);
                if col == "case_id" {
                    has_case_id = true;
                }
            }
            if has_case_id {
                println!("  ✅ 有 case_id 列");
            } else {
                println!("  ❌ 没有 case_id 列 - 这是问题所在!");
            }
        }
    }
    
    // 3. 查找 pipeline_cases 表的所有列
    println!("\n📋 pipeline_cases 表的列:");
    let rows = sqlx::query(
        "SELECT column_name, data_type 
         FROM information_schema.columns 
         WHERE table_schema = 'public' 
           AND table_name = 'pipeline_cases'
         ORDER BY ordinal_position"
    ).fetch_all(&pool).await?;
    
    for row in &rows {
        let col: String = row.try_get(0)?;
        let typ: String = row.try_get(1)?;
        println!("  {} ({})", col, typ);
    }
    
    // 4. 查找 pipeline_case_events 表的所有列
    println!("\n📋 pipeline_case_events 表的列:");
    let rows = sqlx::query(
        "SELECT column_name, data_type 
         FROM information_schema.columns 
         WHERE table_schema = 'public' 
           AND table_name = 'pipeline_case_events'
         ORDER BY ordinal_position"
    ).fetch_all(&pool).await?;
    
    for row in &rows {
        let col: String = row.try_get(0)?;
        let typ: String = row.try_get(1)?;
        println!("  {} ({})", col, typ);
    }
    
    // 5. 查找所有包含 'case' 的非主键索引
    println!("\n📋 所有包含 'case' 的非主键索引:");
    let rows = sqlx::query(
        "SELECT i.tablename, i.indexname, i.indexdef
         FROM pg_indexes i
         LEFT JOIN pg_constraint c ON c.conname = i.indexname
         WHERE i.schemaname = 'public'
           AND i.indexname LIKE '%case%'
           AND (c.contype IS NULL OR c.contype != 'p')
         ORDER BY i.tablename, i.indexname"
    )
    .fetch_all(&pool).await?;
    println!("  找到 {} 个索引:", rows.len());
    for row in &rows {
        let table: String = row.try_get(0)?;
        let index: String = row.try_get(1)?;
        let def: String = row.try_get(2)?;
        println!("    {}.{}", table, index);
        if def.contains("case_id") {
            println!("      ⚠️  引用 case_id!");
        }
    }

    Ok(())
}
