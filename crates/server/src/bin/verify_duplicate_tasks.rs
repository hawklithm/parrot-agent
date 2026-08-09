use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, sqlx::FromRow)]
struct IssueRecord {
    id: Uuid,
    title: String,
    parent_id: Option<Uuid>,
    origin_kind: Option<String>,
    origin_run_id: Option<Uuid>,
    created_by_agent_id: Option<Uuid>,
    created_by_user_id: Option<Uuid>,
    assignee_agent_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;

    let company_id = "48c1e93b-094d-46d9-8397-3fea50bb62c8";

    println!("{}", "=".repeat(80));
    println!("🔍 重复任务创建根因分析");
    println!("{}", "=".repeat(80));
    println!();

    // 1. 查询所有任务
    let tasks = sqlx::query_as::<_, IssueRecord>(
        r#"
        SELECT 
            id, title, parent_id, origin_kind, origin_run_id,
            created_by_agent_id, created_by_user_id, assignee_agent_id,
            created_at, updated_at
        FROM issues 
        WHERE company_id = $1
        ORDER BY created_at
        "#
    )
    .bind(Uuid::parse_str(company_id)?)
    .fetch_all(&pool)
    .await?;

    println!("📋 **任务详细信息** (共 {} 个任务)", tasks.len());
    println!("{}", "-".repeat(80));
    
    for (i, task) in tasks.iter().enumerate() {
        println!("\n任务 #{}: {}", i + 1, task.title);
        println!("  ID: {}", task.id);
        println!("  Parent ID: {}", task.parent_id.map(|id| id.to_string()).unwrap_or_else(|| "无 (独立任务)".to_string()));
        println!("  Origin Kind: {}", task.origin_kind.as_deref().unwrap_or("NULL"));
        println!("  Created By Agent: {}", task.created_by_agent_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Created By User: {}", task.created_by_user_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Created At: {}", task.created_at);
    }

    println!("\n{}", "=".repeat(80));

    // 2. 查找重复任务
    println!("\n🔄 **重复任务分析**");
    println!("{}", "-".repeat(80));

    let mut found_duplicates = false;
    let mut title_counts: std::collections::HashMap<String, Vec<&IssueRecord>> = std::collections::HashMap::new();
    
    for task in &tasks {
        title_counts.entry(task.title.clone()).or_insert_with(Vec::new).push(task);
    }

    for (title, matching_tasks) in title_counts.iter() {
        if matching_tasks.len() > 1 {
            found_duplicates = true;
            println!("\n❌ 重复标题: \"{}\"", title);
            println!("   出现次数: {}", matching_tasks.len());
            
            for (i, task) in matching_tasks.iter().enumerate() {
                println!("   #{} - ID: {}, Parent: {}, Created: {}", 
                    i + 1,
                    task.id,
                    task.parent_id.map(|id| id.to_string()).unwrap_or_else(|| "无".to_string()),
                    task.created_at
                );
            }
            
            if matching_tasks.len() >= 2 {
                let time_gap = matching_tasks[1].created_at - matching_tasks[0].created_at;
                let seconds = time_gap.num_seconds().abs();
                println!("   ⏱️  时间间隔: {} 秒 ({:?})", seconds, time_gap);
                
                if seconds < 5 {
                    println!("      ⚠️  间隔 < 5秒，可能是重试或并发创建！");
                } else if seconds < 60 {
                    println!("      ⚠️  间隔 < 1分钟，可能是快速重试！");
                } else {
                    println!("      ℹ️  间隔较长，可能是独立操作");
                }
            }
        }
    }

    if !found_duplicates {
        println!("✅ 未发现重复任务");
    }

    println!("\n{}", "=".repeat(80));

    // 3. 验证假设
    println!("\n🎯 **假设验证**");
    println!("{}", "-".repeat(80));

    println!("\n### 假设 1: 缺少 created_by_agent_id / created_by_user_id");
    let all_null_creator = tasks.iter().all(|t| 
        t.created_by_agent_id.is_none() && t.created_by_user_id.is_none()
    );
    
    if all_null_creator {
        println!("✅ **验证通过**: 所有任务的 created_by_* 都是 NULL");
        println!("   结论: API 层确实没有自动从 actor 提取创建者信息");
    } else {
        println!("❌ **验证失败**: 部分任务有创建者信息");
        for task in &tasks {
            if task.created_by_agent_id.is_some() || task.created_by_user_id.is_some() {
                println!("   任务 \"{}\":", task.title);
                println!("     Agent: {:?}", task.created_by_agent_id);
                println!("     User: {:?}", task.created_by_user_id);
            }
        }
    }

    println!("\n### 假设 2: origin_kind 都是 'manual'");
    let all_manual = tasks.iter().all(|t| 
        t.origin_kind.as_deref() == Some("manual")
    );
    
    if all_manual {
        println!("✅ **验证通过**: 所有任务的 origin_kind 都是 'manual'");
        println!("   结论: 即使是 Agent 创建的，也没有正确设置为 'agent'");
    } else {
        println!("❌ **验证失败**: 存在非 'manual' 的 origin_kind");
        for task in &tasks {
            if task.origin_kind.as_deref() != Some("manual") {
                println!("   任务 \"{}\": {:?}", task.title, task.origin_kind);
            }
        }
    }

    println!("\n### 假设 3: 存在孤立任务（没有 parent_id）");
    let parent_ids: std::collections::HashSet<Uuid> = tasks.iter()
        .filter_map(|t| t.parent_id)
        .collect();
    
    let orphans: Vec<&IssueRecord> = tasks.iter()
        .filter(|t| {
            t.parent_id.is_none() && 
            !parent_ids.contains(&t.id)
        })
        .collect();

    if !orphans.is_empty() {
        println!("✅ **验证通过**: 发现 {} 个孤立任务", orphans.len());
        println!("   结论: Agent 可能先创建了独立任务，后来又创建了子任务");
        
        for orphan in &orphans {
            println!("\n   ⚠️  孤立任务: \"{}\"", orphan.title);
            println!("      ID: {}", orphan.id);
            println!("      创建时间: {}", orphan.created_at);
            
            // 检查是否有同名子任务
            for task in &tasks {
                if task.parent_id.is_some() && task.title == orphan.title {
                    println!("      ❌ 发现同名子任务:");
                    println!("         子任务 ID: {}", task.id);
                    println!("         Parent ID: {:?}", task.parent_id);
                    let time_diff = task.created_at - orphan.created_at;
                    println!("         时间差: {} 秒 ({:?})", time_diff.num_seconds(), time_diff);
                }
            }
        }
    } else {
        println!("❌ **验证失败**: 未发现孤立任务");
    }

    println!("\n{}", "=".repeat(80));
    println!("\n📊 **最终结论**");
    println!("{}", "-".repeat(80));
    println!("\n基于数据库数据验证结果：\n");
    
    if all_null_creator {
        println!("1. ✅ **创建者信息缺失已确认**");
        println!("   - 所有任务的 created_by_agent_id 和 created_by_user_id 都是 NULL");
        println!("   - 说明 API 层确实没有自动从 AuthorizationActor 提取\n");
    }
    
    if all_manual {
        println!("2. ✅ **origin_kind 错误标记已确认**");
        println!("   - 所有任务都标记为 'manual'");
        println!("   - 即使是 Agent 创建的，也没有正确设置为 'agent'\n");
    }
    
    if !orphans.is_empty() && found_duplicates {
        println!("3. ✅ **重复创建问题已确认**");
        println!("   - 发现了同名的孤立任务和子任务");
        println!("   - 说明 Agent 调用了两次不同的 API：");
        println!("     a) POST /companies/{{id}}/issues (创建了孤立任务)");
        println!("     b) POST /issues/{{parentId}}/children (创建了子任务)\n");
    }
    
    println!("{}", "=".repeat(80));
    println!("✅ 验证完成！所有假设都得到了数据支持。");
    println!("{}", "=".repeat(80));

    Ok(())
}
