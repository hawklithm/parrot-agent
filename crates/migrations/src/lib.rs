// Migration SQL files are embedded in the binary
pub const MIGRATION_004: &str = include_str!("../004_create_execution_environments.sql");
pub const MIGRATION_005: &str = include_str!("../005_create_secrets.sql");
pub const MIGRATION_006: &str = include_str!("../006_create_assets.sql");
pub const MIGRATION_007: &str = include_str!("../007_create_execution_workspaces.sql");
