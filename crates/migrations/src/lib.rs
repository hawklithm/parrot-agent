//! Database migration SQL definitions and runner.
//!
//! Each migration is embedded as a `&str` constant and applied in ascending
//! order by [`run_migrations`]. All statements use `CREATE TABLE IF NOT EXISTS`
//! so re-running is safe (idempotent).

// Migration SQL files are embedded in the binary
pub const MIGRATION_004: &str = include_str!("../004_create_execution_environments.sql");
pub const MIGRATION_005: &str = include_str!("../005_create_secrets.sql");
pub const MIGRATION_006: &str = include_str!("../006_create_assets.sql");
pub const MIGRATION_007: &str = include_str!("../007_create_execution_workspaces.sql");
pub const MIGRATION_008: &str = include_str!("../008_create_issues.sql");
pub const MIGRATION_009: &str = include_str!("../009_create_cases.sql");
pub const MIGRATION_010: &str = include_str!("../010_create_issue_auxiliary_tables.sql");
pub const MIGRATION_011: &str = include_str!("../011_create_auth_tables.sql");
pub const MIGRATION_012: &str = include_str!("../../../migrations/20260712000002_create_heartbeat_runs.sql");
pub const MIGRATION_013: &str = include_str!("../../../migrations/20260712000003_create_issue_watchdogs.sql");
pub const MIGRATION_014: &str = include_str!("../../../migrations/20260712000004_create_agent_wakeup_requests.sql");
pub const MIGRATION_015: &str = include_str!("../../../migrations/20260712000005_create_issue_thread_interactions.sql");

/// Ordered list of all migrations (ascending by migration number).
pub const ALL_MIGRATIONS: &[(&str, &str)] = &[
    ("004_create_execution_environments", MIGRATION_004),
    ("005_create_secrets", MIGRATION_005),
    ("006_create_assets", MIGRATION_006),
    ("007_create_execution_workspaces", MIGRATION_007),
    ("008_create_issues", MIGRATION_008),
    ("009_create_cases", MIGRATION_009),
    ("010_create_issue_auxiliary_tables", MIGRATION_010),
    ("011_create_auth_tables", MIGRATION_011),
    ("012_create_heartbeat_runs", MIGRATION_012),
    ("013_create_issue_watchdogs", MIGRATION_013),
    ("014_create_agent_wakeup_requests", MIGRATION_014),
    ("015_create_issue_thread_interactions", MIGRATION_015),
];

/// Run all embedded migrations against the given pool in order.
///
/// Each migration is wrapped in its own transaction so a failure aborts that
/// single migration without leaving the schema partially applied.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    for (name, sql) in ALL_MIGRATIONS {
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(sql).execute(&mut *tx).await.map_err(|e| {
            tracing::error!("migration {} failed: {}", name, e);
            e
        })?;
        tx.commit().await?;
        tracing::info!("applied migration {}", name);
    }
    Ok(())
}
