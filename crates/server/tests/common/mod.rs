//! Shared test-database plumbing for the `parrot-server` integration tests.
//!
//! The connection string always comes from `DATABASE_URL`; there is deliberately
//! no hardcoded fallback, because a stale literal silently pointed every HTTP
//! parity test at a dead port. For the same reason this module does not load
//! `.env` — the repo-root file targets a different database than the tests do.

#![allow(dead_code)]

use sqlx::PgPool;

/// Connects to `DATABASE_URL` and brings the schema up to date.
///
/// # Panics
///
/// Panics with a copy-pasteable command when `DATABASE_URL` is unset.
pub async fn connect_and_migrate() -> PgPool {
    let pool = connect().await;
    migrate(&pool).await;
    pool
}

/// Connects to `DATABASE_URL`, asserting it is set. Callers that own the pool
/// (for example `#[sqlx::test]`, which hands one in) use [`migrate`] instead.
pub async fn connect() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "DATABASE_URL must be set to run parrot-server integration tests, e.g.\n  \
             DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
             cargo test -p parrot-server"
        )
    });
    PgPool::connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("connect {database_url}: {error}"))
}

/// Runs the applied migrations against an existing pool.
pub async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

/// Deletes a company created by a test, along with the rows the parity tests
/// hang off it.
///
/// `companies` has 54 foreign keys without `ON DELETE CASCADE`, so a plain
/// delete trips whichever one the test populated. This clears the tables the
/// company-creation tests actually fill — ordering matters (`routine_triggers`
/// before `routines`) — and removes the company last. A company with other
/// children (issues, approvals, projects, …) still fails loudly on the final
/// delete rather than half-deleting; extend [`DEPENDENTS`] if a test needs it.
pub async fn delete_company(pool: &PgPool, company_id: uuid::Uuid) {
    const DEPENDENTS: &[&str] = &[
        "routine_triggers",
        "routine_runs",
        "routines",
        "skill_files",
        "company_skills",
        "principal_permission_grants",
        "agents",
        "company_memberships",
    ];
    // One transaction, so a company with children outside DEPENDENTS rolls
    // back whole instead of leaving the dependents deleted.
    let mut tx = pool
        .begin()
        .await
        .unwrap_or_else(|error| panic!("begin company {company_id} cleanup: {error}"));
    for table in DEPENDENTS {
        sqlx::query(&format!("DELETE FROM {table} WHERE company_id = $1"))
            .bind(company_id)
            .execute(&mut *tx)
            .await
            .unwrap_or_else(|error| panic!("delete {table} rows for company {company_id}: {error}"));
    }
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|error| panic!("delete company {company_id}: {error}"));
    tx.commit()
        .await
        .unwrap_or_else(|error| panic!("commit company {company_id} cleanup: {error}"));
}
