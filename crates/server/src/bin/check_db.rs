use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@localhost:5432/parrot_agent_dev".to_string());

    println!("🔗 连接到数据库...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("\n📊 数据库状态:\n");
    println!("{}", "=".repeat(70));
    
    // Companies
    println!("\n1. Companies:");
    println!("{}", "-".repeat(70));
    let companies: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id::text, name, status FROM companies ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if companies.is_empty() {
        println!("  (empty)");
    } else {
        for (id, name, status) in companies {
            println!("  {} | {} | {}", id, name, status);
        }
    }

    // Agents
    println!("\n2. Agents:");
    println!("{}", "-".repeat(70));
    let agents: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id::text, name, company_id::text, adapter_type FROM agents ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if agents.is_empty() {
        println!("  (empty)");
    } else {
        for (id, name, company_id, adapter_type) in agents {
            println!("  {} | {} | {} | {}", id, name, company_id, adapter_type);
        }
    }

    // Auth Users
    println!("\n3. Auth Users:");
    println!("{}", "-".repeat(70));
    let users: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id::text, email, role FROM auth_users ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if users.is_empty() {
        println!("  (empty)");
    } else {
        for (id, email, role) in users {
            println!("  {} | {} | {}", id, email.unwrap_or("(no email)".to_string()), role);
        }
    }

    // Goals
    println!("\n4. Goals:");
    println!("{}", "-".repeat(70));
    let goals: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id::text, title, company_id::text, status FROM goals ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if goals.is_empty() {
        println!("  (empty)");
    } else {
        for (id, title, company_id, status) in goals {
            println!("  {} | {} | {} | {}", id, title, company_id, status);
        }
    }

    // Projects
    println!("\n5. Projects:");
    println!("{}", "-".repeat(70));
    let projects: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id::text, name, company_id::text FROM projects ORDER BY created_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if projects.is_empty() {
        println!("  (empty)");
    } else {
        for (id, name, company_id) in projects {
            println!("  {} | {} | {}", id, name, company_id);
        }
    }

    // Agent Memberships
    println!("\n6. Agent Memberships:");
    println!("{}", "-".repeat(70));
    let memberships: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT agent_id::text, company_id::text, role FROM agent_memberships ORDER BY joined_at DESC"
    )
    .fetch_all(&pool)
    .await?;
    
    if memberships.is_empty() {
        println!("  (empty)");
    } else {
        for (agent_id, company_id, role) in memberships {
            println!("  {} | {} | {}", agent_id, company_id, role);
        }
    }

    println!("\n{}", "=".repeat(70));

    Ok(())
}
