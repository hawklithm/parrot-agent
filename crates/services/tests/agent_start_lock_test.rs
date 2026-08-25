//! Integration test for the Agent Start Lock (PAPERCLIP_MIGRATION_PLAN §4B.2 line 324).
//!
//! Verifies the per-agent mutual-exclusion contract against the live compile DB:
//!   - acquiring a lock for an agent succeeds and returns a lock id,
//!   - a second concurrent acquire for the SAME agent fails (LockFailed),
//!   - after release the lock can be re-acquired,
//!   - expired locks are reclaimed by cleanup_expired_locks.
//!
//! The table (agent_start_locks) was missing entirely prior to migration 59; this
//! test also guards against a regression that drops it. agent_start_locks references
//! agents(id), so each test seeds a minimal company + agent row.

use services::agent_start_lock_service::AgentStartLockService;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPool::connect(&url).await.unwrap()
}

/// Seed a minimal company + agent and return the agent id. The agent is owned by a
/// throwaway company so FK constraints on both tables are satisfied.
async fn fresh_agent(pool: &PgPool) -> Uuid {
    let company_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_id)
    .bind("start-lock-test-company")
    .bind(format!("S{}", &company_id.simple().to_string()[..5]))
    .execute(pool)
    .await
    .unwrap();

    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_config, runtime_config, permissions, metadata, budget_monthly_cents, spent_monthly_cents)
         VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, '{}'::jsonb, 0, 0)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("start-lock-test-agent")
    .execute(pool)
    .await
    .unwrap();
    agent_id
}

async fn delete_agent(pool: &PgPool, agent_id: Uuid) {
    let _ = sqlx::query("DELETE FROM agent_start_locks WHERE agent_id = $1")
        .bind(agent_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(agent_id)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn same_agent_cannot_hold_two_start_locks() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL is not set");
            return;
        }
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let agent_id = fresh_agent(&pool).await;
    let svc = AgentStartLockService::new(pool.clone());

    let first = svc.acquire_lock(agent_id, "run-a".to_string()).await.unwrap();
    // A second start for the same agent must be rejected (the heartbeat start lock).
    let second = svc.acquire_lock(agent_id, "run-b".to_string()).await;
    assert!(
        matches!(second, Err(services::agent_start_lock_service::AgentStartLockError::LockFailed)),
        "concurrent start for the same agent must fail with LockFailed"
    );

    // After releasing the first lock, a new start succeeds.
    svc.release_lock(first).await.unwrap();
    let third = svc.acquire_lock(agent_id, "run-c".to_string()).await;
    assert!(third.is_ok(), "lock must be re-acquirable after release");
    svc.release_lock(third.unwrap()).await.unwrap();

    // Cleanup should reclaim nothing now (no expired locks).
    let reclaimed = svc.cleanup_expired_locks().await.unwrap();
    assert_eq!(reclaimed, 0, "no expired locks should remain after release");

    delete_agent(&pool, agent_id).await;
}

#[tokio::test]
async fn distinct_agents_can_lock_independently() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL is not set");
            return;
        }
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let a = fresh_agent(&pool).await;
    let b = fresh_agent(&pool).await;
    let svc = AgentStartLockService::new(pool.clone());
    let la = svc.acquire_lock(a, "run-a".to_string()).await.unwrap();
    let lb = svc.acquire_lock(b, "run-b".to_string()).await.unwrap();
    assert_ne!(la, lb, "distinct agents get distinct lock ids");
    svc.release_lock(la).await.unwrap();
    svc.release_lock(lb).await.unwrap();
    delete_agent(&pool, a).await;
    delete_agent(&pool, b).await;
}

#[tokio::test]
async fn expired_start_locks_are_cleaned_up() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL is not set");
            return;
        }
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let agent_id = fresh_agent(&pool).await;
    // Insert an already-expired lock directly to exercise cleanup_expired_locks.
    sqlx::query(
        "INSERT INTO agent_start_locks (id, agent_id, acquired_at, expires_at, holder)
         VALUES ($1, $2, NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '5 minutes', 'stale')",
    )
    .bind(Uuid::new_v4())
    .bind(agent_id)
    .execute(&pool)
    .await
    .unwrap();
    let svc = AgentStartLockService::new(pool.clone());
    let reclaimed = svc.cleanup_expired_locks().await.unwrap();
    assert_eq!(reclaimed, 1, "the expired lock must be reclaimed");
    // And now a fresh acquire for the same agent must succeed.
    let fresh = svc.acquire_lock(agent_id, "run-new".to_string()).await;
    assert!(fresh.is_ok(), "agent must be lockable after stale lock cleanup");
    svc.release_lock(fresh.unwrap()).await.unwrap();
    delete_agent(&pool, agent_id).await;
}
