use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, sqlx::FromRow)]
struct IssueRecord {
    id: Uuid,
    title: String,
    company_id: Uuid,
    parent_id: Option<Uuid>,
    origin_kind: Option<String>,
    created_by_agent_id: Option<Uuid>,
    created_by_user_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;

    println!("{}", "=".repeat(80));
    println!("🔍 数据库中所有任务数据分析");
    println!("{}", "=".repeat(80));
    println!();

    // 查询所有任务
    let all_tasks = sqlx::query_as::<_, IssueRecord>(
        r#"
        SELECT 
            id, title, company_id, parent_id, origin_kind,
            created_by_agent_id, created_by_user_id, created_at
        FROM issues 
        ORDER BY created_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(&pool)
    .await?;

    println!("📋 **最近的任务** (最多 50 个)");
    println!("{}", "-".repeat(80));
    
    if all_tasks.is_empty() {
        println!("❌ 数据库中没有任何任务数据！");
        println!("   可能原因：");
        println!("   1. 数据库已被清空");
        println!("   2. 任务还未被创建");
        println!("   3. 连接到了错误的数据库");
    } else {
        // 按 company_id 分组
        let mut companies: std::collections::HashMap<Uuid, Vec<&IssueRecord>> = std::collections::HashMap::new();
        for task in &all_tasks {
            companies.entry(task.company_id).or_insert_with(Vec::new).push(task);
        }

        println!("\n找到 {} 个公司，共 {} 个任务\n", companies.len(), all_tasks.len());

        for (company_id, tasks) in companies.iter() {
            println!("公司 ID: {}", company_id);
            println!("  任务数: {}", tasks.len());
            
            // 检查重复标题
            let mut title_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for task in tasks {
                *title_counts.entry(task.title.clone()).or_insert(0) += 1;
            }
            
            let duplicates: Vec<(&String, &usize)> = title_counts.iter().filter(|(_, &count)| count > 1).collect();
            if !duplicates.is_empty() {
                println!("  ⚠️  发现 {} 个重复标题:", duplicates.len());
                for (title, count) in duplicates {
                    println!("     - \"{}\" (x{})", title, count);
                }
            }

            // 检查孤立任务
            let parent_ids: std::collections::HashSet<Uuid> = tasks.iter()
                .filter_map(|t| t.parent_id)
                .collect();
            
            let orphans: Vec<&&IssueRecord> = tasks.iter()
                .filter(|t| t.parent_id.is_none() && !parent_ids.contains(&t.id))
                .collect();

            if !orphans.is_empty() {
                println!("  ⚠️  发现 {} 个孤立任务 (无父任务，也无子任务)", orphans.len());
            }

            // 验证假设
            let all_null_creator = tasks.iter().all(|t| 
                t.created_by_agent_id.is_none() && t.created_by_user_id.is_none()
            );
            
            if all_null_creator {
                println!("  ✅ 所有任务的 created_by_* 都是 NULL");
            } else {
                let with_creator = tasks.iter().filter(|t| 
                    t.created_by_agent_id.is_some() || t.created_by_user_id.is_some()
                ).count();
                println!("  ℹ️  有 {} 个任务有创建者信息", with_creator);
            }

            let all_manual = tasks.iter().all(|t| 
                t.origin_kind.as_deref() == Some("manual")
            );
            
            if all_manual {
                println!("  ✅ 所有任务的 origin_kind 都是 'manual'");
            }

            println!();
        }

        // 详细展示部分任务
        println!("{}", "=".repeat(80));
        println!("\n📋 **前 10 个任务详情**");
        println!("{}", "-".repeat(80));
        
        for (i, task) in all_tasks.iter().take(10).enumerate() {
            println!("\n任务 #{}: {}", i + 1, task.title);
            println!("  ID: {}", task.id);
            println!("  Company: {}", task.company_id);
            println!("  Parent: {}", task.parent_id.map(|id| id.to_string()).unwrap_or_else(|| "无".to_string()));
            println!("  Origin Kind: {}", task.origin_kind.as_deref().unwrap_or("NULL"));
            println!("  Created By Agent: {}", task.created_by_agent_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
            println!("  Created By User: {}", task.created_by_user_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
            println!("  Created At: {}", task.created_at);
        }

        println!("\n{}", "=".repeat(80));
        
        // 查找特定的重复任务模式
        println!("\n🔍 **重复任务详细分析**");
        println!("{}", "-".repeat(80));

        for (company_id, tasks) in companies.iter() {
            let mut title_map: std::collections::HashMap<String, Vec<&IssueRecord>> = std::collections::HashMap::new();
            for task in tasks.iter() {
                title_map.entry(task.title.clone()).or_insert_with(Vec::new).push(task);
            }

            for (title, matching_tasks) in title_map.iter() {
                if matching_tasks.len() > 1 {
           println!("\n❌ 公司 {} 中的重复任务:", company_id);
                    println!("   标题: \"{}\"", title);
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
                        println!("   ⏱️  时间间隔: {} 秒", seconds);
                        
                        if seconds < 5 {
                            println!("      ⚠️  间隔 < 5秒，可能是重试或并发创建！");
                        } else if seconds < 60 {
                            println!("      ⚠️  间隔 < 1分钟，可能是快速重试！");
                        }

                        // 检查是否一个有 parent_id 一个没有
                        let has_parent = matching_tasks.iter().filter(|t| t.parent_id.is_some()).count();
                        let no_parent = matching_tasks.iter().filter(|t| t.parent_id.is_none()).count();
                        
                        if has_parent > 0 && no_parent > 0 {
                            println!("      🎯 **关键发现**: 同名任务中，{} 个有 parent_id，{} 个没有！", has_parent, no_parent);
                            println!("         这证明了假设：Agent 先调用了 POST /companies/{{id}}/issues");
                            println!("         然后又调用了 POST /issues/{{parentId}}/children");
                        }
                    }
                }
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("✅ 分析完成！");
    println!("{}", "=".repeat(80));

    Ok(())
}
