//! Integration tests for the built-in agent managed-resource binding ledger.
//!
//! These run against the live compile database (DATABASE_URL). They verify the
//! handoff's requirement that provisioning a built-in agent binds independent
//! Skill/Routine managed resources to (company, built-in key, canonical key),
//! that reconcile repairs stock-version drift, and that reset removes the
//! bindings.

use services::built_in_agent_service::BuiltInAgentKey;
use services::built_in_agent_service_impl::{BuiltInAgentService, DefaultBuiltInAgentService};
use repositories::{AgentRepository, BuiltInManagedResourceRepository};
use sqlx::PgPool;
use uuid::Uuid;

async fn fresh_company(pool: &PgPool) -> Uuid {
    let company_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("builtin managed resource test company")
        .bind(format!("B{}", &company_id.simple().to_string()[..5]))
        .execute(pool)
        .await
        .unwrap();
    company_id
}

async fn cleanup(pool: &PgPool, company_id: Uuid, agent_id: Option<Uuid>) {
    let _ = sqlx::query("DELETE FROM builtin_managed_resources WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    if let Some(id) = agent_id {
        let _ = repositories::PgAgentRepository::new(pool.clone())
            .delete(id)
            .await;
    }
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn provision_binds_skill_and_routine_managed_resources_idempotently() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone())
        .with_resource_pool(pool.clone());

    let key = BuiltInAgentKey::ReflectionCoach;
    let agent = service.provision(company_id, key, None).await.unwrap();

    let rows = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "expected skill + routine bindings");
    let types: Vec<&str> = rows.iter().map(|r| r.resource_type.as_str()).collect();
    assert!(types.contains(&"skill"), "missing skill binding");
    assert!(types.contains(&"routine"), "missing routine binding");
    for r in &rows {
        assert_eq!(
            r.stock_version, r.current_version,
            "fresh binding should not be drifted"
        );
        assert!(!r.drift_detected);
    }
    let skill_binding = rows
        .iter()
        .find(|row| row.resource_type == "skill")
        .expect("skill binding");
    let skill_id = skill_binding
        .target_resource_id
        .expect("skill binding must point to a company skill");
    let file_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_files WHERE company_id = $1 AND skill_id = $2",
    )
    .bind(company_id)
    .bind(skill_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(file_count > 0, "managed skill files must be materialized");

    // Routine materialization: an actual `routines` row owned by the built-in agent
    // must exist, along with its trigger(s).
    let routine_binding = rows
        .iter()
        .find(|row| row.resource_type == "routine")
        .expect("routine binding");
    let routine_id = routine_binding
        .target_resource_id
        .expect("routine binding must point to a real routine");
    let routine_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routines WHERE id = $1 AND company_id = $2 AND agent_id = $3",
    )
    .bind(routine_id)
    .bind(company_id)
    .bind(agent.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(routine_count, 1, "managed routine row must be materialized and owned by the built-in agent");
    let trigger_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_triggers WHERE routine_id = $1",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(trigger_count >= 1, "managed routine must have at least one trigger");

    // Second provision must be idempotent: no extra rows, no error.
    let _ = service.provision(company_id, key, None).await.unwrap();
    let rows2 = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap();
    assert_eq!(rows2.len(), 2, "provision must be idempotent");

    cleanup(&pool, company_id, Some(agent.id)).await;
}

#[tokio::test]
async fn reconcile_repairs_stock_version_drift() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone())
        .with_resource_pool(pool.clone());

    let key = BuiltInAgentKey::LearningAssistant;
    let agent = service.provision(company_id, key, None).await.unwrap();

    // A target resource can drift while its stored stock version still matches.
    sqlx::query(
        "UPDATE builtin_managed_resources SET drift_detected = TRUE, status = 'drifted' WHERE company_id = $1 AND built_in_key = $2",
    )
    .bind(company_id)
    .bind(key.as_str())
    .execute(&pool)
    .await
    .unwrap();

    let result = service.reconcile(company_id, key).await.unwrap();
    assert!(
        result.skills_synced && result.routines_synced,
        "reconcile should report synced skill + routine bindings"
    );
    assert!(
        result.changes.iter().any(|c| c.contains("drift")),
        "reconcile should report drift repair: {:?}",
        result.changes
    );

    let rows = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for r in &rows {
        assert_eq!(r.stock_version, r.current_version, "drift must be repaired");
        assert!(!r.drift_detected);
    }

    cleanup(&pool, company_id, Some(agent.id)).await;
}

#[tokio::test]
async fn reset_removes_managed_resource_bindings() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone())
        .with_resource_pool(pool.clone());

    let key = BuiltInAgentKey::BriefsGenerator;
    let agent = service.provision(company_id, key, None).await.unwrap();
    assert_eq!(
        managed_repo
            .list_by_company_and_key(company_id, key.as_str())
            .await
            .unwrap()
            .len(),
        2
    );
    let skill_id = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.resource_type == "skill")
        .and_then(|binding| binding.target_resource_id)
        .expect("managed skill id");
    let routine_id = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.resource_type == "routine")
        .and_then(|binding| binding.target_resource_id)
        .expect("managed routine id");

    service.reset(company_id, key).await.unwrap();
    assert_eq!(
        managed_repo
            .list_by_company_and_key(company_id, key.as_str())
            .await
            .unwrap()
            .len(),
        0,
        "reset must remove managed resource bindings"
    );
    let skill_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM company_skills WHERE id = $1 AND company_id = $2)",
    )
    .bind(skill_id)
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!skill_exists, "reset must delete the managed company skill");
    let routine_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM routines WHERE id = $1 AND company_id = $2)",
    )
    .bind(routine_id)
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!routine_exists, "reset must delete the managed routine");
    let trigger_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_triggers WHERE routine_id = $1",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trigger_count, 0, "reset must delete the managed routine's triggers");

    cleanup(&pool, company_id, Some(agent.id)).await;
}

#[tokio::test]
async fn bindings_are_isolated_per_company() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_a = fresh_company(&pool).await;
    let company_b = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone());

    let key = BuiltInAgentKey::Summarizer;
    let agent_a = service.provision(company_a, key, None).await.unwrap();

    // Company B has NOT provisioned this key, so it must have zero bindings.
    let rows_b = managed_repo
        .list_by_company_and_key(company_b, key.as_str())
        .await
        .unwrap();
    assert!(
        rows_b.is_empty(),
        "another company must not see this company's managed bindings"
    );

    cleanup(&pool, company_a, Some(agent_a.id)).await;
    cleanup(&pool, company_b, None).await;
}

#[tokio::test]
async fn provision_keeps_stable_routine_and_trigger_rows() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone())
        .with_resource_pool(pool.clone());

    let key = BuiltInAgentKey::ReflectionCoach;
    let agent = service.provision(company_id, key, None).await.unwrap();
    let routine_id_1 = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.resource_type == "routine")
        .and_then(|binding| binding.target_resource_id)
        .expect("managed routine id");
    let triggers_1: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_triggers WHERE routine_id = $1",
    )
    .bind(routine_id_1)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Repeat provision must not create new routine/trigger rows.
    let _ = service.provision(company_id, key, None).await.unwrap();
    let routine_id_2 = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.resource_type == "routine")
        .and_then(|binding| binding.target_resource_id)
        .expect("managed routine id");
    let triggers_2: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_triggers WHERE routine_id = $1",
    )
    .bind(routine_id_2)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(routine_id_1, routine_id_2, "routine id must be stable across provisions");
    assert_eq!(triggers_1, triggers_2, "triggers must not be duplicated on repeat provision");

    cleanup(&pool, company_id, Some(agent.id)).await;
}

#[tokio::test]
async fn reconcile_repairs_routine_content_drift() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    let company_id = fresh_company(&pool).await;

    let agent_repo = std::sync::Arc::new(repositories::PgAgentRepository::new(pool.clone()));
    let managed_repo = std::sync::Arc::new(
        repositories::PgBuiltInManagedResourceRepository::new(pool.clone()),
    );
    let service = DefaultBuiltInAgentService::new(agent_repo.clone(), managed_repo.clone())
        .with_resource_pool(pool.clone());

    let key = BuiltInAgentKey::LearningAssistant;
    let agent = service.provision(company_id, key, None).await.unwrap();
    let routine_id = managed_repo
        .list_by_company_and_key(company_id, key.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.resource_type == "routine")
        .and_then(|binding| binding.target_resource_id)
        .expect("managed routine id");

    // Simulate user drift: the routine title/status were changed out-of-band.
    sqlx::query("UPDATE routines SET title = 'DRIFTED TITLE', status = 'active' WHERE id = $1")
        .bind(routine_id)
        .execute(&pool)
        .await
        .unwrap();

    let result = service.reconcile(company_id, key).await.unwrap();
    assert!(result.routines_synced, "reconcile should report routine synced");

    let title: String = sqlx::query_scalar("SELECT title FROM routines WHERE id = $1")
        .bind(routine_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        title, "Review open learning follow-ups",
        "reconcile must restore the routine title from the built-in definition"
    );

    cleanup(&pool, company_id, Some(agent.id)).await;
}
