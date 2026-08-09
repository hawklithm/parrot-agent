//! 分析招聘相关任务重复的问题
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;

    println!("=== 详细分析：招聘相关任务 ===\n");
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
    
    for (idx, i) in hire_issues.iter().enumerate() {
        println!("{}. [{}]", idx + 1, i.id);
        println!("   标题: {}", i.title);
        println!("   状态: {}", &i.status);
        println!("   来源: {} | origin_id={:?} | run_id={:?}", i.origin_kind, i.origin_id, i.origin_run_id);
        println!("   创建者: agent={:?} | user={:?}", i.created_by_agent_id, i.created_by_user_id);
        println!("   分配给: agent={:?}", i.assignee_agent_id);
        println!("   父任务: {:?}", i.parent_id);
        println!("   时间: created={} | updated={}", 
            i.created_at.format("%Y-%m-%d %H:%M:%S"), 
            i.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!();
    }
    
    // 查看 issue_comments
    println!("\n=== Issue Comments (招聘相关，最近20条) ===");
    let comments = sqlx::query!(
        r#"SELECT ic.id, ic.issue_id, ic.body, ic.actor_type::text as "actor_type!", ic.actor_id, ic.created_at
        FROM issue_comments ic
        WHERE ic.issue_id IN (
            SELECT id FROM issues WHERE title LIKE '%Hire%' OR title LIKE '%招聘%'
        )
        ORDER BY ic.created_at
        LIMIT 20"#
    ).fetch_all(&pool).await?;
    
    if comments.is_empty() {
        println!("(没有评论)");
    } else {
        for c in &comments {
            println!("\n评论 ID: {} | Issue: {}", c.id, c.issue_id);
            println!("作者: type={} | id={:?} | 时间: {}", 
                &c.actor_type, c.actor_id, c.created_at.format("%Y-%m-%d %H:%M:%S"));
            let preview = if c.body.len() > 200 {
                format!("{}...", &c.body[..200])
            } else {
                c.body.clone()
            };
            println!("内容: {}", preview);
        }
    }

    Ok(())
}
