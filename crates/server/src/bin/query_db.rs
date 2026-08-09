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

