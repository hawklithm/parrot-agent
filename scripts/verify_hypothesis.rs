#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! sqlx = { version = "0.8", features = ["runtime-tokio-native-tls", "postgres", "uuid", "chrono"] }
//! tokio = { version = "1", features = ["full"] }
//! uuid = { version = "1", features = ["serde"] }
//! chrono = { version = "0.4", features = ["serde"] }
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, FromRow)]
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
    let database_url = "postgres://postgres:admin123@localhost:5432/parrot_agent_dev";
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

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
        WHERE company_id = '48c1e93b-094d-46d9-8397-3fea50bb62c8'
        ORDER BY created_at
        "#
    )
    .fetch_all(&pool)
    .await?;

    println!("📋 **任务详细信息**");
    println!("{}", "-".repeat(80));
    
    for (i, task) in tasks.iter().enumerate() {
        println!("\n任务 #{}: {}", i + 1, task.title);
        println!("  ID: {}", task.id);
        println!("  Parent ID: {}", task.parent_id.map(|id| id.to_string()).unwrap_or_else(|| "无 (独立任务)".to_string()));
        println!("  Origin Kind: {}", task.origin_kind.as_deref().unwrap_or("NULL"));
        println!("  Origin Run ID: {}", task.origin_run_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Created By Agent: {}", task.created_by_agent_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Created By User: {}", task.created_by_user_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Assignee Agent: {}", task.assignee_agent_id.map(|id| id.to_string()).unwrap_or_else(|| "NULL".to_string()));
        println!("  Created At: {}", task.created_at);
        println!("  Time Diff: {:?}", task.updated_at - task.created_at);
    }

    println!("\n{}", "=".repeat(80));

    // 2. 查找重复任务
    println!("\n🔄 **重复任务分析**");
    println!("{}", "-".repeat(80));

    let duplicate_titles = sqlx::query(
        r#"
        SELECT title, COUNT(*) as count
        FROM issues 
        WHERE company_id = '48c1e93b-094d-46d9-8397-3fea50bb62c8'
        GROUP BY title
        HAVING COUNT(*) > 1
        "#
    )
    .fetch_all(&pool)
    .await?;

    if !duplicate_titles.is_empty() {
        for row in duplicate_titles {
            let title: String = row.get("title");
            let count: i64 = row.get("count");
            
            let matching_tasks: Vec<&IssueRecord> = tasks.iter()
                .filter(|t| t.title == title)
                .collect();
            
            println!("\n❌ 重复标题: \"{}\"", title);
            println!("   出现次数: {}", count);
            
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
                println!("   ⏱️  时间间隔: {:?}", time_gap);
                
                let seconds = time_gap.num_seconds().abs();
                if seconds < 5 {
                    println!("      ⚠️  间隔 < 5秒，可能是重试或并发创建！");
                } else if seconds < 60 {
                    println!("      ⚠️  间隔 < 1分钟，可能是快速重试！");
                } else {
                    println!("      ℹ️  间隔较长，可能是独立操作");
                }
            }
        }
    } else {
        println!("✅ 未发现重复任务");
    }

    println!("\n{}", "=".repeat(80));

    // 3. 查询 Activity Log
    pr"\n📜 **Activity Log 分析**");
    println!("{}", "-".repeat(80));

    let activity_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM activity_log
        WHERE company_id = '48c1e93b-094d-46d9-8397-3fea50bb62c8'
          AND action LIKE '%issue%'
          AND action LIKE '%create%'
        "#
    )
    .fetch_one(&pool)
    .await?;

    if activity_count > 0 {
        println!("找到 {} 条任务创建相关的活动记录", activity_count);
    } else {
        println!("❌ 未找到任务创建相关的活动日志");
        println!("   可能原因：");
        println!("   1. 创建时未记录活动日志");
        println!("   2. 代码中缺少日志记录逻辑");
    }

    println!("\n{}", "=".repeat(80));

    // 4. 验证假设
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
    let parent_ids: Vec<Uuid> = tasks.iter()
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
                    println!("         时间差: {:?}", task.created_at - orphan.created_at);
                }
            }
        }
    } else {
        println!("❌ **验证失败**: 未发现孤立任务");
    }

    println!("\n{}", "=".repeat(80));
    println!("✅ 分析完成！");
    prin