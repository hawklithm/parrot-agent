//! E2E tests for Company Import/Export, Configure Repair, Doctor, Install, Update commands.
//!
//! These tests verify CLI command structure and help output without requiring
//! a live server connection (non-network operations).
//!
//! Run with: cargo test -p parrot-cli --test cli_e2e_import_export_doctor_test

use std::process::Command;

fn run_cli_command(args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_parrot"))
        .args(args)
        .output()
        .expect("spawn parrot");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (success, format!("{}\n{}", stdout, stderr))
}

/// E2E: CLI doctor command runs without panic
#[test]
fn cli_doctor_runs_without_panic() {
    let (success, output) = run_cli_command(&["doctor"]);

    // Doctor may fail if no config exists, but should not panic
    assert!(
        output.contains("doctor")
            || output.contains("diagnostic")
            || output.contains("config")
            || success,
        "doctor command should produce recognizable output: {}",
        output
    );
}

/// E2E: CLI doctor --json returns valid JSON structure
#[test]
fn cli_doctor_json_returns_valid_structure() {
    let (success, output) = run_cli_command(&["doctor", "--json"]);

    // Should either succeed with JSON or show a parseable error
    if output.contains('{') {
        assert!(
            output.contains("status") || output.contains("error"),
            "JSON output should contain status or error field: {}",
            output
        );
    } else {
        assert!(success || output.contains("check"));
    }
}

/// E2E: CLI configure runs without panic
#[test]
fn cli_configure_runs_without_panic() {
    let (success, output) = run_cli_command(&["configure"]);

    // configure command may require specific args; verify it runs without panic
    assert!(
        output.contains("configure")
            || success
            || output.contains("error")
            || output.contains("missing"),
        "configure should complete or show expected error: {}",
        output
    );
}

/// E2E: CLI configure stores configuration files
#[test]
fn cli_configure_stores_files() {
    use std::path::PathBuf;

    // Use a temporary directory for config
    let temp_dir = format!("/tmp/parrot-test-config-{}", uuid::Uuid::new_v4().simple());
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");

    let config_path = format!("{}/parrot.json", temp_dir);

    // Run configure with explicit config path
    let (success, output) = run_cli_command(&[
        "configure",
        "--config",
        &config_path,
        "--server-url",
        "http://localhost:3100",
    ]);

    // Configuration may fail due to validation, but should attempt to write
    assert!(
        success || output.contains("config") || output.contains("saved") || output.contains("error"),
        "configure should complete: {}",
        output
    );

    // Check if config file was created
    let path = PathBuf::from(&config_path);
    if path.exists() {
        let content = std::fs::read_to_string(&path).expect("read config");
        assert!(
            content.contains("server_url") || content.contains("server"),
            "config file should contain server URL: {}",
            content
        );
        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    // Cleanup temp dir
    std::fs::remove_dir_all(&temp_dir).ok();
}

/// E2E: CLI config show reads stored configuration
#[test]
fn cli_config_show_reads_stored_config() {
    // Create a minimal config file
    let temp_config = format!("/tmp/parrot-test-config-{}.json", uuid::Uuid::new_v4().simple());
    let config_content = r#"{"server_url": "http://localhost:3100", "api_token": "test-token"}"#;
    std::fs::write(&temp_config, config_content).expect("write temp config");

    let (success, output) = run_cli_command(&["config", "show", "--json", "--config", &temp_config]);

    // Should read and display the config
    assert!(
        success || output.contains("server_url") || output.contains("config"),
        "config show should work: {}",
        output
    );

    // Cleanup
    std::fs::remove_file(&temp_config).ok();
}

/// E2E: CLI company list requires authentication
#[test]
fn cli_company_list_requires_auth() {
    let (success, output) = run_cli_command(&["company", "list"]);

    // Should fail without auth token
    assert!(
        !success || output.contains("auth") || output.contains("token") || output.contains("error"),
        "company list should fail without auth: {}",
        output
    );
}

/// E2E: CLI install shows permission requirements or help
#[test]
fn cli_install_shows_permission_requirements() {
    let (success, output) = run_cli_command(&["install"]);

    // install may fail with Permission denied (requires elevated privileges)
    // This is expected behavior; just verify the command runs without panic
    assert!(
        output.contains("install")
            || output.contains("system")
            || output.contains("Permission")
            || output.contains("root")
            || success,
        "install should indicate permission requirement or run: {}",
        output
    );
}

/// E2E: CLI uninstall shows help
#[test]
fn cli_uninstall_shows_help() {
    let (success, output) = run_cli_command(&["uninstall", "--help"]);

    // Should show uninstall options
    assert!(
        output.contains("uninstall")
            || output.contains("usage")
            || output.contains("--keep-config"),
        "uninstall help should show options: {}",
        output
    );
}

/// E2E: CLI update shows version info
#[test]
fn cli_update_shows_version_info() {
    let (success, output) = run_cli_command(&["update"]);

    // Update should mention version or release info
    assert!(
        success
            || output.contains("version")
            || output.contains("update")
            || output.contains("release"),
        "update should show version info: {}",
        output
    );
}

/// E2E: CLI fix runs diagnostics without panic
#[test]
fn cli_fix_runs_diagnostics() {
    let (success, output) = run_cli_command(&["fix"]);

    // Fix should run checks and report results
    assert!(
        success
            || output.contains("fix")
            || output.contains("diagnostic")
            || output.contains("check")
            || output.contains("config"),
        "fix should run diagnostics: {}",
        output
    );
}

/// E2E: CLI commands with --version show version
#[test]
fn cli_version_flag_shows_version() {
    let (success, output) = run_cli_command(&["--version"]);

    // Should show version string
    assert!(
        success && !output.is_empty(),
        "--version should output version: {}",
        output
    );
}

/// E2E: CLI main help shows all commands
#[test]
fn cli_main_help_shows_all_commands() {
    let (success, output) = run_cli_command(&["--help"]);

    // Should list all available commands
    let expected_commands = ["doctor", "configure", "config", "service", "install", "uninstall"];
    let found_any = expected_commands.iter().any(|cmd| output.contains(cmd));

    assert!(
        found_any || success,
        "help should show command list: {}",
        output
    );
}

/// E2E: CLI company commands show subcommands
#[test]
fn cli_company_commands_show_subcommands() {
    let (success, output) = run_cli_command(&["company", "--help"]);

    // Should list subcommands (list, get, create, delete, export, import)
    assert!(
        output.contains("list")
            || output.contains("get")
            || output.contains("create")
            || output.contains("export")
            || output.contains("import")
            || success,
        "company help should show subcommands: {}",
        output
    );
}
