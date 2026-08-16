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
pub const MIGRATION_039: &str = include_str!("../../../migrations/20260727000001_add_heartbeat_run_output.sql");
pub const MIGRATION_040: &str = include_str!("../../../migrations/20260728000001_create_tool_invocation_audit.sql");
pub const MIGRATION_041: &str = include_str!("../../../migrations/20260728000002_create_named_mcp_gateways.sql");
pub const MIGRATION_013: &str = include_str!("../../../migrations/20260712000003_create_issue_watchdogs.sql");
pub const MIGRATION_014: &str = include_str!("../../../migrations/20260712000004_create_agent_wakeup_requests.sql");
pub const MIGRATION_015: &str = include_str!("../../../migrations/20260712000005_create_issue_thread_interactions.sql");
pub const MIGRATION_016: &str = include_str!("../../../migrations/20260805000004_create_issue_relations.sql");
pub const MIGRATION_017: &str = include_str!("../../../migrations/20260805000005_add_issue_harness_kind.sql");
pub const MIGRATION_042: &str = include_str!("../../../migrations/08_fix_activity_logs_add_missing_columns.sql");
pub const MIGRATION_043: &str = include_str!("../../../migrations/09_fix_issue_thread_interactions_add_missing_columns.sql");
pub const MIGRATION_044: &str = include_str!("../../../migrations/10_create_plugin_managed_resources.sql");
pub const MIGRATION_045: &str = include_str!("../../../migrations/11_create_instruction_templates.sql");

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
    ("016_create_issue_relations", MIGRATION_016),
    ("017_add_issue_harness_kind", MIGRATION_017),
    ("039_add_heartbeat_run_output", MIGRATION_039),
    ("040_create_tool_invocation_audit", MIGRATION_040),
    ("044_create_plugin_managed_resources", MIGRATION_044),
    ("045_create_instruction_templates", MIGRATION_045),
    ("042_fix_activity_logs_add_missing_columns", MIGRATION_042),
    ("043_fix_issue_thread_interactions_add_missing_columns", MIGRATION_043),
    ("041_create_named_mcp_gateways", MIGRATION_041),
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
