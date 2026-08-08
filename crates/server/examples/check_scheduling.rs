/// Check Issue Scheduling and Wakeup Status
///
/// 验证 issue 创建后的调度状态：
/// - issues 表中的 assignee_agent_id
/// - agent_wakeup_requests 表中的对应记录
/// - 检测是否存在"已分配但未唤醒"的问题
///
/// 用法：cargo run --example check_scheduling

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://paperclip:paperclip@localhost/paperclip".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("=== Issue Scheduling and Wakeup Analysis ===\n");

    // 1. 查找最近创建的、有 assignee 的 issues
    println!("📋 Recent Issues with Assignees (Last 10):");
    println!("{:-<100}", "");
    
    let rows = sqlx::query(
        r#"
        SELECT id, identifier, title, status, assignee_agent_id, created_at
        FROM issues
        WHERE assignee_agent_id IS NOT NULL
        ORDER BY created_at DESC
        LIMIT 10
        "#,
    )
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        println!("❌ No issues with assignees found");
        return Ok(());
    }

    let mut issues_data = Vec::new();
    
    for row in &rows {
        let id: sqlx::types::Uuid = row.try_get("id")?;
        let identifier: Option<String> = row.try_get("identifier").ok();
        let title: String = row.try_get("title")?;
        let status: String = row.try_get("status")?;
        let assignee: sqlx::types::Uuid = row.try_get("assignee_agent_id")?;
        
        let display_id = identifier.as_deref().unwrap_or("N/A");
        let truncated_title = if title.len() > 50 {
            format!("{}...", &title[..47])
        } else {
            title.clone()
        };
        
        println!(
            "{:<15} | {:<50} | Status: {:<12} | Assignee: {}",
            display_id, truncated_title, status, assignee
        );
        
        issues_data.push((id, status, assignee, identifier));
    }

    println!();

    // 2. 查找对应的 wakeup requests
    println!("🔔 Wakeup Requests for These Issues:");
    println!("{:-<100}", "");

    let mut issues_with_wakeup = 0;
    let mut issues_without_wakeup = 0;

    for (issue_id, status, assignee_id, identifier) in &issues_data {
        if status == "backlog" {
            continue; // Backlog issues shouldn't have wakeup
        }

        let wakeup_rows = sqlx::query(
            r#"
            SELECT id, agent_id, issue_id, source, reason, created_at, consumed_at
            FROM agent_wakeup_requests
            WHERE issue_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(issue_id)
        .fetch_all(&pool)
        .await?;

        let display_id = identifier.as_deref().unwrap_or("N/A");
        
        if wakeup_rows.is_empty() {
            println!(
                "⚠️  {:<15} | No wakeup found (Status: {}, Assignee: {})",
                display_id, status, assignee_id
            );
            issues_without_wakeup += 1;
        } else {
            let wakeup = &wakeup_rows[0];
            let source: String = wakeup.try_get("source")?;
            let reason: Option<String> = wakeup.try_get("reason").ok();
            let consumed_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>> = 
                wakeup.try_get("consumed_at").ok().flatten();
            
            let consumed_status = if consumed_at.is_some() {
                "Consumed ✅"
            } else {
                "Pending ⏳"
            };
            println!(
                "✅ {:<15} | Wakeup: {} | Reason: {} | {}",
                display_id,
                source,
                reason.as_deref().unwrap_or("N/A"),
                consumed_status
            );
            issues_with_wakeup += 1;
        }
    }

    println!();

    // 3. 统计分析
    println!("📊 Analysis Summary:");
    println!("{:-<100}", "");
    let non_backlog_count = issues_data.iter().filter(|(_, status, _, _)| status != "backlog").count();
    println!("Total issues with assignee (non-backlog): {}", non_backlog_count);
    println!("Issues WITH wakeup:    {} ✅", issues_with_wakeup);
    println!("Issues WITHOUT wakeup: {} ⚠️", issues_without_wakeup);

    if issues_without_wakeup > 0 {
        println!("\n⚠️  WARNING: Found {} issue(s) with assignee but no wakeup request!", issues_without_wakeup);
        println!("    This indicates the wakeup service was not called after issue creation.");
        println!("    These agents will never be notified about their assigned tasks.");
    } else if non_backlog_count > 0 {
        println!("\n✅ All assigned issues have corresponding wakeup requests!");
    }

    println!();

    // 4. 查看 agents 状态
    println!("🤖 Agent Status:");
    println!("{:-<100}", "");

    let assignee_ids: Vec<sqlx::types::Uuid> = issues_data
        .iter()
        .map(|(_, _, assignee, _)| *assignee)
        .collect();

    if !assignee_ids.is_empty() {
        let agent_rows = sqlx::query(
            r#"
            SELECT id, shortname, status
            FROM agents
            WHERE id = ANY($1)
            "#,
        )
        .bind(&assignee_ids)
        .fetch_all(&pool)
        .await?;

        for agent in agent_rows {
            let agent_id: sqlx::types::Uuid = agent.try_get("id")?;
            let shortname: String = agent.try_get("shortname")?;
            let agent_status: String = agent.try_get("status")?;
            
            let assigned_count = issues_data
                .iter()
                .filter(|(_, _, assignee, _)| *assignee == agent_id)
                .count();
            println!(
                "{:<30} | Status: {:<15} | Assigned: {} issue(s)",
                shortname, agent_status, assigned_count
            );
        }
    }

    println!();

    Ok(())
}
