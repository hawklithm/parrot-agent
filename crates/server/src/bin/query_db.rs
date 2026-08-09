//! Database query utility
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;

    println!("=== Companies ===");
    let companies = sqlx::query!("SELECT id, name, created_at FROM companies ORDER BY created_at")
        .fetch_all(&pool).await?;
    for c in &companies {
        println!("{} | {} | {}", c.id, c.name, c.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    println!("\n=== Agents ===");
    let agents = sqlx::query!(
        r#"SELECT id, name, role::text as "role!", status::text as "status!", created_at FROM agents ORDER BY created_at"#
    ).fetch_all(&pool).await?;
    for a in &agents {
        println!("{} | {} | role={} | status={} | {}", a.id, a.name, &a.role, &a.status, a.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    println!("\n=== Issues (最近20条) ===");
    let issues = sqlx::query!(
        r#"SELECT id, title, status::text as "status!", assignee_agent_id, origin_kind, created_at FROM issues ORDER BY created_at DESC LIMIT 20"#
    ).fetch_all(&pool).await?;
    for i in &issues {
        println!("{} | {} | status={} | agent={:?} | origin={} | {}", 
            i.id, i.title, &i.status, i.assignee_agent_id, i.origin_kind, i.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    println!("\n=== 重复的 Issues (按标题分组) ===");
    let dups = sqlx::query!(
        r#"SELECT title, COUNT(*) as "count!", STRING_AGG(id::text, ', ') as issue_ids FROM issues GROUP BY title HAVING COUNT(*) > 1"#
    ).fetch_all(&pool).await?;
    if dups.is_empty() {
        println!("(没有重复)");
    } else {
        for d in &dups {
            println!("'{}' 出现 {} 次 | IDs: {}", d.title, d.count, d.issue_ids.as_ref().unwrap_or(&"".to_string()));
        }
    }

    println!("\n=== Routines ===");
    let routines = sqlx::query!(
        r#"SELECT id, title, assignee_agent_id, status::text as "routine_status!", created_at FROM routines ORDER BY created_at"#
    ).fetch_all(&pool).await?;
    for r in &routines {
        println!("{} | {} | agent={} | status={} | {}", r.id, r.title, r.assignee_agent_id, &r.routine_status, r.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    println!("\n=== Routine Runs (最近10条) ===");
    let runs = sqlx::query!(
        r#"SELECT id, routine_id, status::text as "run_status!", linked_issue_id, created_at FROM routine_runs ORDER BY created_at DESC LIMIT 10"#
    ).fetch_all(&pool).await?;
for run in &runs {
        println!("{} | routine={} | status={} | issue={:?} | {}", 
            run.id, run.routine_id, &run.run_status, run.linked_issue_id, run.created_at.format("%Y-%m-%d %H:%M:%S"));
    }

    Ok(())
}

    // 详细查看这 3 个任务
    println!("\n=== 详细分析：招聘相关任务 ===");
    let hire_issues = sqlx::query!(
        r#"SELECT 
            id, title, status::text as "status!", 
            origin_kind, origin_id, origin_run_id,
            created_by_agent_id, created_by_user_id,
            assignee_agent_id, parent_id,
            created_at, updated_at
        FROM issues 
        WHERE title LIKE '%Hire%' OR title LIKE '%招聘%' OR title LIKE '%创始工程师%'
        ORDER BY created_at"#
    ).fetch_all(&pool).await?;
    
    for i in &hire_issues {
        println!("\n[{}]", i.id);
        println!("  标题: {}", i.title);
        println!("  状态: {}", &i.status);
        println!("  来源: {} | origin_id={:?} | run_id={:?}", i.origin_kind, i.origin_id, i.origin_run_id);
        println!("  创建者: agent={:?} | user={:?}", i.created_by_agent_id, i.created_by_user_id);
        println!("  分配给: agent={:?}", i.assignee_agent_id);
        println!("  父任务: {:?}", i.parent_id);
        println!("  时间: created={} | updated={}", 
            i.created_at.format("%H:%M:%S"), 
            i.updated_at.format("%H:%M:%S")
        );
    }
    
    // 查看 issue_comments
    println!("\n=== Issue Comments (招聘相关) ===");
    let comments = sqlx::query!(
        r#"SELECT ic.id, ic.issue_id, ic.content, ic.author_agent_id, ic.author_user_id, ic.created_at
        FROM issue_comments ic
        JOIN issues i ON ic.issue_id = i.id
        WHERE i.title LIKE '%Hire%' OR i.title LIKE '%招聘%'
        ORDER BY ic.created_at
        LIMIT 20"#
    ).fetch_all(&pool).await?;
    
    if comments.is_empty() {
        println!("(没有评论)");
    } else {
        for c in &comments {
            println!("{} | issue={} | author: agent={:?}/user={:?} | {}", 
                c.id, c.issue_id, c.author_agent_id, c.author_user_id, c.created_at.format("%H:%M:%S"));
            println!("  内容: {}", &c.content[..c.content.len().min(100)]);
        }
    }

    Ok(())
}
